//! Outreach sequence and follow-up management.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::crm::CrmManager;
use rusvel_core::domain::{
    AgentConfig, Contact, Content, Event, Job, JobKind, JobResult, NewJob, ObjectFilter,
};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::{ContactId, EventId, JobId, SessionId};
use rusvel_core::ports::{AgentPort, EventPort, JobPort, StoragePort};

use crate::email::{EmailAdapter, EmailMessage, send_email_with_event};

// ── Local ID + domain types ────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SequenceId(Uuid);

impl Default for SequenceId {
    fn default() -> Self {
        Self::new()
    }
}

impl SequenceId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for SequenceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for SequenceId {
    type Err = uuid::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(Self(Uuid::parse_str(s)?))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FollowUpId(Uuid);

impl Default for FollowUpId {
    fn default() -> Self {
        Self::new()
    }
}

impl FollowUpId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for FollowUpId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SequenceStatus {
    Draft,
    Active,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SequenceStep {
    pub delay_days: u32,
    pub channel: String,
    pub template: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutreachSequence {
    pub id: SequenceId,
    pub session_id: SessionId,
    pub name: String,
    pub steps: Vec<SequenceStep>,
    pub status: SequenceStatus,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

/// Worker handling for a single [`JobKind::OutreachSend`] dequeue.
#[derive(Debug)]
pub enum OutreachSendDispatch {
    /// Draft ready; worker should call [`JobPort::hold_for_approval`].
    HoldForApproval(JobResult),
    /// Email sent (or skipped); worker should [`JobPort::complete`] and optionally enqueue `next`.
    Complete {
        result: JobResult,
        next: Option<NewJob>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FollowUp {
    pub id: FollowUpId,
    pub contact_id: ContactId,
    pub session_id: SessionId,
    pub due_date: DateTime<Utc>,
    pub context: String,
    pub completed: bool,
    pub metadata: serde_json::Value,
}

// ── OutreachManager ────────────────────────────────────────────────

const KIND_SEQUENCE: &str = "outreach_sequence";
const KIND_FOLLOWUP: &str = "followup";
const KIND_SEQ_RUN: &str = "outreach_sequence_run";

pub struct OutreachManager {
    storage: Arc<dyn StoragePort>,
    agent: Arc<dyn AgentPort>,
    jobs: Arc<dyn JobPort>,
}

impl OutreachManager {
    pub fn new(
        storage: Arc<dyn StoragePort>,
        agent: Arc<dyn AgentPort>,
        jobs: Arc<dyn JobPort>,
    ) -> Self {
        Self {
            storage,
            agent,
            jobs,
        }
    }

    pub async fn create_sequence(
        &self,
        session_id: SessionId,
        name: String,
        steps: Vec<SequenceStep>,
    ) -> Result<SequenceId> {
        let seq = OutreachSequence {
            id: SequenceId::new(),
            session_id,
            name,
            steps,
            status: SequenceStatus::Draft,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let id = seq.id;
        let json = serde_json::to_value(&seq)?;
        self.storage
            .objects()
            .put(KIND_SEQUENCE, &id.to_string(), json)
            .await?;
        Ok(id)
    }

    pub async fn list_sequences(&self, session_id: SessionId) -> Result<Vec<OutreachSequence>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND_SEQUENCE, filter).await?;
        vals.into_iter()
            .map(|v| Ok(serde_json::from_value(v)?))
            .collect()
    }

    pub async fn get_sequence(&self, id: &SequenceId) -> Result<OutreachSequence> {
        let val = self
            .storage
            .objects()
            .get(KIND_SEQUENCE, &id.to_string())
            .await?;
        match val {
            Some(v) => Ok(serde_json::from_value(v)?),
            None => Err(RusvelError::NotFound {
                kind: KIND_SEQUENCE.into(),
                id: id.to_string(),
            }),
        }
    }

    pub async fn activate_sequence(&self, id: &SequenceId) -> Result<()> {
        let mut seq = self.get_sequence(id).await?;
        seq.status = SequenceStatus::Active;
        self.storage
            .objects()
            .put(KIND_SEQUENCE, &id.to_string(), serde_json::to_value(&seq)?)
            .await
    }

    /// Enqueue the **first** [`JobKind::OutreachSend`] for step 0. Later steps are scheduled after
    /// each send completes (human approval + SMTP), so approval order is preserved.
    /// Sequence must be [`SequenceStatus::Active`]. Persists a run record under `outreach_sequence_run`.
    pub async fn execute_sequence(
        &self,
        session_id: SessionId,
        sequence_id: SequenceId,
        contact_id: ContactId,
    ) -> Result<JobId> {
        let crm = CrmManager::new(Arc::clone(&self.storage));
        let contact = crm.get_contact(&contact_id).await?;
        if contact.session_id != session_id {
            return Err(RusvelError::Validation(
                "contact session_id does not match".into(),
            ));
        }
        let seq = self.get_sequence(&sequence_id).await?;
        if seq.session_id != session_id {
            return Err(RusvelError::Validation(
                "sequence session_id does not match".into(),
            ));
        }
        if seq.status != SequenceStatus::Active {
            return Err(RusvelError::Validation(
                "sequence must be Active before execute_sequence".into(),
            ));
        }
        if seq.steps.is_empty() {
            return Err(RusvelError::Validation("sequence has no steps".into()));
        }

        let run_key = Uuid::now_v7().to_string();
        let step0 = &seq.steps[0];
        let scheduled_at = Utc::now() + Duration::days(i64::from(step0.delay_days));

        let payload = serde_json::json!({
            "sequence_id": sequence_id.to_string(),
            "sequence_run_id": run_key,
            "contact_id": contact_id.to_string(),
            "step_index": 0,
            "channel": step0.channel,
            "template": step0.template,
        });

        let job_id = self
            .jobs
            .enqueue(NewJob {
                session_id,
                kind: JobKind::OutreachSend,
                payload,
                max_retries: 2,
                metadata: serde_json::json!({
                    "department": "gtm",
                    "requires_approval": true,
                }),
                scheduled_at: Some(scheduled_at),
            })
            .await?;

        let step_rows: Vec<serde_json::Value> = seq
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                serde_json::json!({
                    "step_index": i,
                    "job_id": if i == 0 { job_id.to_string() } else { String::new() },
                    "status": if i == 0 { "queued" } else { "pending" },
                    "channel": s.channel,
                })
            })
            .collect();

        let run = serde_json::json!({
            "id": run_key,
            "sequence_id": sequence_id.to_string(),
            "contact_id": contact_id.to_string(),
            "session_id": session_id.to_string(),
            "status": "running",
            "started_at": Utc::now().to_rfc3339(),
            "steps": step_rows,
        });
        self.storage
            .objects()
            .put(KIND_SEQ_RUN, &run_key, run)
            .await?;

        Ok(job_id)
    }

    /// Result of processing one [`JobKind::OutreachSend`] (draft vs send + optional follow-up job).
    pub async fn process_outreach_send_job(
        &self,
        job: &Job,
        events: &dyn EventPort,
        email: &dyn EmailAdapter,
    ) -> Result<OutreachSendDispatch> {
        let session_id = job.session_id;
        let sequence_id_str = job
            .payload
            .get("sequence_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RusvelError::Validation("outreach job missing sequence_id".into()))?;
        let sequence_id: SequenceId = sequence_id_str
            .parse()
            .map_err(|e: uuid::Error| RusvelError::Validation(e.to_string()))?;

        let contact_id_str = job
            .payload
            .get("contact_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| RusvelError::Validation("outreach job missing contact_id".into()))?;
        let contact_id = Uuid::parse_str(contact_id_str)
            .map(ContactId::from_uuid)
            .map_err(|e| RusvelError::Validation(e.to_string()))?;

        let step_index = job
            .payload
            .get("step_index")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| RusvelError::Validation("outreach job missing step_index".into()))?
            as usize;

        let run_key = job
            .payload
            .get("sequence_run_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let channel = job
            .payload
            .get("channel")
            .and_then(|v| v.as_str())
            .unwrap_or("email");
        let template = job
            .payload
            .get("template")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if !channel.eq_ignore_ascii_case("email") {
            return Err(RusvelError::Validation(format!(
                "outreach channel `{channel}` is not supported (only email)"
            )));
        }

        let seq = self.get_sequence(&sequence_id).await?;
        if step_index >= seq.steps.len() {
            return Err(RusvelError::Validation("step_index out of range".into()));
        }

        let crm = CrmManager::new(Arc::clone(&self.storage));
        let contact = crm.get_contact(&contact_id).await?;

        if job.metadata.get("approval_pending_result").is_some() {
            let pending: JobResult = serde_json::from_value(
                job.metadata
                    .get("approval_pending_result")
                    .cloned()
                    .ok_or_else(|| RusvelError::Internal("approval metadata missing".into()))?,
            )
            .map_err(|e| RusvelError::Serialization(e.to_string()))?;

            let output = pending.output;
            let to = output
                .get("to")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let subject = output
                .get("subject")
                .and_then(|v| v.as_str())
                .unwrap_or("Outreach")
                .to_string();
            let body = output
                .get("body")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if to.is_empty() {
                return Err(RusvelError::Validation(
                    "approved outreach draft missing `to`".into(),
                ));
            }

            send_email_with_event(
                email,
                events,
                Some(session_id),
                EmailMessage {
                    to,
                    from: String::new(),
                    subject: subject.clone(),
                    body,
                },
            )
            .await?;

            let _ = self
                .emit_outreach_sent(events, session_id, &sequence_id, &contact_id, step_index)
                .await;

            self.merge_run_step(&run_key, step_index, "sent", &job.id.to_string())
                .await?;

            let next = if step_index + 1 < seq.steps.len() {
                let next_step = &seq.steps[step_index + 1];
                let delay = next_step.delay_days;
                Some(NewJob {
                    session_id,
                    kind: JobKind::OutreachSend,
                    payload: serde_json::json!({
                        "sequence_id": sequence_id.to_string(),
                        "sequence_run_id": run_key,
                        "contact_id": contact_id.to_string(),
                        "step_index": step_index + 1,
                        "channel": next_step.channel,
                        "template": next_step.template,
                    }),
                    max_retries: 2,
                    metadata: serde_json::json!({
                        "department": "gtm",
                        "requires_approval": true,
                    }),
                    scheduled_at: Some(Utc::now() + Duration::days(i64::from(delay))),
                })
            } else {
                self.merge_run_complete(&run_key).await?;
                None
            };

            let result = JobResult {
                output: serde_json::json!({
                    "status": "sent",
                    "sequence_id": sequence_id.to_string(),
                    "contact_id": contact_id.to_string(),
                    "step_index": step_index,
                }),
                metadata: serde_json::json!({"engine": "gtm"}),
            };

            return Ok(OutreachSendDispatch::Complete { result, next });
        }

        let body = self
            .generate_message(
                session_id,
                &contact,
                &format!("Template: {template}\nSequence: {}", seq.name),
            )
            .await?;

        let to = contact.emails.first().cloned().unwrap_or_default();
        if to.is_empty() {
            return Err(RusvelError::Validation(
                "contact has no email address".into(),
            ));
        }

        let subject = format!("{} — {}", seq.name, template);
        let draft = serde_json::json!({
            "sequence_id": sequence_id.to_string(),
            "contact_id": contact_id.to_string(),
            "step_index": step_index,
            "to": to,
            "subject": subject,
            "body": body,
            "channel": channel,
        });

        Ok(OutreachSendDispatch::HoldForApproval(JobResult {
            output: draft,
            metadata: serde_json::json!({"engine": "gtm", "phase": "draft"}),
        }))
    }

    async fn emit_outreach_sent(
        &self,
        events: &dyn EventPort,
        session_id: SessionId,
        sequence_id: &SequenceId,
        contact_id: &ContactId,
        step_index: usize,
    ) -> Result<()> {
        let event = Event {
            id: EventId::new(),
            session_id: Some(session_id),
            run_id: None,
            source: "gtm".into(),
            kind: crate::events::OUTREACH_SENT.into(),
            payload: serde_json::json!({
                "sequence_id": sequence_id.to_string(),
                "contact_id": contact_id.to_string(),
                "step_index": step_index,
                "status": "sent",
            }),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        events.emit(event).await?;
        Ok(())
    }

    async fn merge_run_step(
        &self,
        run_key: &str,
        step_index: usize,
        status: &str,
        job_id: &str,
    ) -> Result<()> {
        if run_key.is_empty() {
            return Ok(());
        }
        let Some(mut run) = self.storage.objects().get(KIND_SEQ_RUN, run_key).await? else {
            return Ok(());
        };
        if let Some(steps) = run.get_mut("steps").and_then(|s| s.as_array_mut()) {
            if let Some(step) = steps.get_mut(step_index) {
                if let Some(obj) = step.as_object_mut() {
                    obj.insert("status".into(), status.into());
                    obj.insert("job_id".into(), job_id.into());
                    obj.insert("sent_at".into(), Utc::now().to_rfc3339().into());
                }
            }
        }
        self.storage.objects().put(KIND_SEQ_RUN, run_key, run).await
    }

    async fn merge_run_complete(&self, run_key: &str) -> Result<()> {
        if run_key.is_empty() {
            return Ok(());
        }
        let Some(mut run) = self.storage.objects().get(KIND_SEQ_RUN, run_key).await? else {
            return Ok(());
        };
        if let Some(obj) = run.as_object_mut() {
            obj.insert("status".into(), "completed".into());
            obj.insert("completed_at".into(), Utc::now().to_rfc3339().into());
        }
        self.storage.objects().put(KIND_SEQ_RUN, run_key, run).await
    }

    pub async fn schedule_followup(
        &self,
        session_id: SessionId,
        contact_id: ContactId,
        due_date: DateTime<Utc>,
        context: String,
    ) -> Result<()> {
        let fu = FollowUp {
            id: FollowUpId::new(),
            contact_id,
            session_id,
            due_date,
            context,
            completed: false,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&fu)?;
        self.storage
            .objects()
            .put(KIND_FOLLOWUP, &fu.id.to_string(), json)
            .await
    }

    pub async fn list_followups(&self, session_id: SessionId) -> Result<Vec<FollowUp>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND_FOLLOWUP, filter).await?;
        vals.into_iter()
            .map(|v| Ok(serde_json::from_value(v)?))
            .collect()
    }

    pub async fn generate_message(
        &self,
        session_id: SessionId,
        contact: &Contact,
        context: &str,
    ) -> Result<String> {
        let prompt = format!(
            "Write a short, personalized outreach message for {} ({}).\n\
             Company: {}\nContext: {}",
            contact.name,
            contact.emails.first().unwrap_or(&String::new()),
            contact.company.as_deref().unwrap_or("unknown"),
            context,
        );
        let config = AgentConfig {
            profile_id: None,
            session_id,
            model: None,
            tools: vec![],
            instructions: Some("You are a sales outreach assistant.".into()),
            budget_limit: Some(0.05),
            max_iterations: None,
            permission_mode: Default::default(),
            metadata: serde_json::json!({}),
        };
        let run_id = self.agent.create(config).await?;
        let output = self.agent.run(&run_id, Content::text(prompt)).await?;
        let text = output
            .content
            .parts
            .into_iter()
            .filter_map(|p| match p {
                rusvel_core::domain::Part::Text(t) => Some(t),
                _ => None,
            })
            .collect::<String>();
        Ok(text)
    }
}
