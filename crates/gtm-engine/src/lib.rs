//! `GoToMarket` engine — CRM, outreach, and ops.

use std::sync::Arc;

use async_trait::async_trait;
use rusvel_core::domain::{Capability, Event, HealthStatus, Job};
use rusvel_core::error::Result;
use rusvel_core::id::EventId;
use rusvel_core::ports::{AgentPort, EventPort, JobPort, StoragePort};

pub mod crm;
pub mod email;
pub mod invoice;
pub mod outreach;

pub use crm::{CrmManager, Deal, DealId, DealStage};
pub use email::{
    EmailAdapter, EmailMessage, MockEmailAdapter, SmtpEmailAdapter, send_email_with_event,
};
pub use invoice::{Invoice, InvoiceId, InvoiceManager, InvoiceStatus, LineItem};
pub use outreach::{
    FollowUp, FollowUpId, OutreachManager, OutreachSendDispatch, OutreachSequence, SequenceId,
    SequenceStatus, SequenceStep,
};

pub mod events {
    pub const OUTREACH_SENT: &str = "gtm.outreach.sent";
    pub const EMAIL_SENT: &str = "gtm.email.sent";
    pub const DEAL_UPDATED: &str = "gtm.deal.updated";
    pub const CONTACT_ADDED: &str = "gtm.contact.added";
    pub const INVOICE_CREATED: &str = "gtm.invoice.created";
    pub const INVOICE_PAID: &str = "gtm.invoice.paid";
    pub const SEQUENCE_CREATED: &str = "gtm.sequence.created";
}

pub struct GtmEngine {
    #[allow(dead_code)]
    storage: Arc<dyn StoragePort>,
    events: Arc<dyn EventPort>,
    #[allow(dead_code)]
    agent: Arc<dyn AgentPort>,
    #[allow(dead_code)]
    jobs: Arc<dyn JobPort>,
    crm: CrmManager,
    outreach: OutreachManager,
    invoices: InvoiceManager,
}

impl GtmEngine {
    pub fn new(
        storage: Arc<dyn StoragePort>,
        events: Arc<dyn EventPort>,
        agent: Arc<dyn AgentPort>,
        jobs: Arc<dyn JobPort>,
    ) -> Self {
        let crm = CrmManager::new(Arc::clone(&storage));
        let outreach =
            OutreachManager::new(Arc::clone(&storage), Arc::clone(&agent), Arc::clone(&jobs));
        let invoices = InvoiceManager::new(Arc::clone(&storage));
        Self {
            storage,
            events,
            agent,
            jobs,
            crm,
            outreach,
            invoices,
        }
    }

    pub fn crm(&self) -> &CrmManager {
        &self.crm
    }
    pub fn outreach(&self) -> &OutreachManager {
        &self.outreach
    }
    pub fn invoices(&self) -> &InvoiceManager {
        &self.invoices
    }

    /// Emit a domain event on the event bus.
    pub async fn emit_event(&self, kind: &str, payload: serde_json::Value) -> Result<EventId> {
        let event = Event {
            id: EventId::new(),
            session_id: None,
            run_id: None,
            source: "gtm".into(),
            kind: kind.into(),
            payload,
            created_at: chrono::Utc::now(),
            metadata: serde_json::json!({}),
        };
        self.events.emit(event).await
    }

    /// Process one [`rusvel_core::domain::JobKind::OutreachSend`] dequeue (draft → approval → SMTP).
    pub async fn process_outreach_send_job(
        &self,
        job: &Job,
        email: &dyn EmailAdapter,
    ) -> Result<OutreachSendDispatch> {
        self.outreach
            .process_outreach_send_job(job, self.events.as_ref(), email)
            .await
    }
}

#[async_trait]
impl rusvel_core::engine::Engine for GtmEngine {
    fn kind(&self) -> &str {
        "gtm"
    }
    fn name(&self) -> &'static str {
        "GoToMarket Engine"
    }

    fn capabilities(&self) -> Vec<Capability> {
        vec![
            Capability::Outreach,
            Capability::Custom("crm".into()),
            Capability::Custom("invoicing".into()),
        ]
    }

    async fn initialize(&self) -> Result<()> {
        Ok(())
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            message: None,
            metadata: serde_json::json!({}),
        })
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rusvel_core::domain::*;
    use rusvel_core::id::*;
    use rusvel_core::ports::*;
    use std::sync::Mutex;

    // ── In-memory stub stores ──────────────────────────────────────

    struct StubStore {
        objects: StubObjects,
    }

    impl StubStore {
        fn new() -> Self {
            Self {
                objects: StubObjects::new(),
            }
        }
    }

    struct StubEvents;
    struct StubSessions;
    struct StubJobStore;
    struct StubMetrics;

    struct StubObjects {
        data: Mutex<Vec<(String, String, serde_json::Value)>>,
    }

    impl StubObjects {
        fn new() -> Self {
            Self {
                data: Mutex::new(Vec::new()),
            }
        }
    }

    impl StoragePort for StubStore {
        fn events(&self) -> &dyn EventStore {
            &StubEvents
        }
        fn objects(&self) -> &dyn ObjectStore {
            &self.objects
        }
        fn sessions(&self) -> &dyn SessionStore {
            &StubSessions
        }
        fn jobs(&self) -> &dyn JobStore {
            &StubJobStore
        }
        fn metrics(&self) -> &dyn MetricStore {
            &StubMetrics
        }
    }

    #[async_trait]
    impl EventStore for StubEvents {
        async fn append(&self, _: &Event) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &EventId) -> Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl ObjectStore for StubObjects {
        async fn put(&self, kind: &str, id: &str, object: serde_json::Value) -> Result<()> {
            let mut data = self.data.lock().unwrap();
            data.retain(|(k, i, _)| !(k == kind && i == id));
            data.push((kind.into(), id.into(), object));
            Ok(())
        }
        async fn get(&self, kind: &str, id: &str) -> Result<Option<serde_json::Value>> {
            let data = self.data.lock().unwrap();
            Ok(data
                .iter()
                .find(|(k, i, _)| k == kind && i == id)
                .map(|(_, _, v)| v.clone()))
        }
        async fn delete(&self, kind: &str, id: &str) -> Result<()> {
            let mut data = self.data.lock().unwrap();
            data.retain(|(k, i, _)| !(k == kind && i == id));
            Ok(())
        }
        async fn list(&self, kind: &str, _filter: ObjectFilter) -> Result<Vec<serde_json::Value>> {
            let data = self.data.lock().unwrap();
            Ok(data
                .iter()
                .filter(|(k, _, _)| k == kind)
                .map(|(_, _, v)| v.clone())
                .collect())
        }
    }

    #[async_trait]
    impl SessionStore for StubSessions {
        async fn put_session(&self, _: &Session) -> Result<()> {
            Ok(())
        }
        async fn get_session(&self, _: &SessionId) -> Result<Option<Session>> {
            Ok(None)
        }
        async fn list_sessions(&self) -> Result<Vec<SessionSummary>> {
            Ok(vec![])
        }
        async fn put_run(&self, _: &Run) -> Result<()> {
            Ok(())
        }
        async fn get_run(&self, _: &RunId) -> Result<Option<Run>> {
            Ok(None)
        }
        async fn list_runs(&self, _: &SessionId) -> Result<Vec<Run>> {
            Ok(vec![])
        }
        async fn put_thread(&self, _: &Thread) -> Result<()> {
            Ok(())
        }
        async fn get_thread(&self, _: &ThreadId) -> Result<Option<Thread>> {
            Ok(None)
        }
        async fn list_threads(&self, _: &RunId) -> Result<Vec<Thread>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl JobStore for StubJobStore {
        async fn enqueue(&self, _: &Job) -> Result<()> {
            Ok(())
        }
        async fn dequeue(&self, _: &[JobKind]) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn update(&self, _: &Job) -> Result<()> {
            Ok(())
        }
        async fn get(&self, _: &JobId) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn list(&self, _: JobFilter) -> Result<Vec<Job>> {
            Ok(vec![])
        }
    }

    #[async_trait]
    impl MetricStore for StubMetrics {
        async fn record(&self, _: &MetricPoint) -> Result<()> {
            Ok(())
        }
        async fn query(&self, _: MetricFilter) -> Result<Vec<MetricPoint>> {
            Ok(vec![])
        }
    }

    struct StubEventPort;

    #[async_trait]
    impl EventPort for StubEventPort {
        async fn emit(&self, event: Event) -> Result<EventId> {
            Ok(event.id)
        }
        async fn get(&self, _: &EventId) -> Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: EventFilter) -> Result<Vec<Event>> {
            Ok(vec![])
        }
    }

    struct StubAgentPort;

    #[async_trait]
    impl AgentPort for StubAgentPort {
        async fn create(&self, _: AgentConfig) -> Result<RunId> {
            Ok(RunId::new())
        }
        async fn run(&self, _: &RunId, _: Content) -> Result<AgentOutput> {
            Ok(AgentOutput {
                run_id: RunId::new(),
                content: Content::text("Hello!"),
                tool_calls: 0,
                usage: LlmUsage::default(),
                cost_estimate: 0.0,
                metadata: serde_json::json!({}),
            })
        }
        async fn stop(&self, _: &RunId) -> Result<()> {
            Ok(())
        }
        async fn status(&self, _: &RunId) -> Result<AgentStatus> {
            Ok(AgentStatus::Idle)
        }
    }

    struct StubJobPort;

    #[async_trait]
    impl JobPort for StubJobPort {
        async fn enqueue(&self, _: NewJob) -> Result<JobId> {
            Ok(JobId::new())
        }
        async fn dequeue(&self, _: &[JobKind]) -> Result<Option<Job>> {
            Ok(None)
        }
        async fn complete(&self, _: &JobId, _: JobResult) -> Result<()> {
            Ok(())
        }
        async fn hold_for_approval(&self, _: &JobId, _: JobResult) -> Result<()> {
            Ok(())
        }
        async fn fail(&self, _: &JobId, _: String) -> Result<()> {
            Ok(())
        }
        async fn schedule(&self, _: NewJob, _: &str) -> Result<JobId> {
            Ok(JobId::new())
        }
        async fn cancel(&self, _: &JobId) -> Result<()> {
            Ok(())
        }
        async fn approve(&self, _: &JobId) -> Result<()> {
            Ok(())
        }
        async fn list(&self, _: JobFilter) -> Result<Vec<Job>> {
            Ok(vec![])
        }
    }

    fn make_engine() -> GtmEngine {
        GtmEngine::new(
            Arc::new(StubStore::new()),
            Arc::new(StubEventPort),
            Arc::new(StubAgentPort),
            Arc::new(StubJobPort),
        )
    }

    fn make_contact(session_id: SessionId) -> Contact {
        Contact {
            id: ContactId::new(),
            session_id,
            name: "Alice Smith".into(),
            emails: vec!["alice@example.com".into()],
            links: vec![],
            company: Some("Acme Corp".into()),
            role: Some("CTO".into()),
            tags: vec!["vip".into()],
            last_contacted_at: None,
            metadata: serde_json::json!({}),
        }
    }

    // ── CRM tests ──────────────────────────────────────────────────

    #[tokio::test]
    async fn crm_add_and_list_contacts() {
        let engine = make_engine();
        let sid = SessionId::new();
        let contact = make_contact(sid);

        let id = engine
            .crm()
            .add_contact(sid, contact.clone())
            .await
            .unwrap();
        let fetched = engine.crm().get_contact(&id).await.unwrap();
        assert_eq!(fetched.name, "Alice Smith");

        let all = engine.crm().list_contacts(sid).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    // ── Deal tests ─────────────────────────────────────────────────

    #[tokio::test]
    async fn deal_add_and_advance_stage() {
        let engine = make_engine();
        let sid = SessionId::new();
        let cid = ContactId::new();

        let deal = Deal {
            id: DealId::new(),
            session_id: sid,
            contact_id: cid,
            title: "Enterprise license".into(),
            value: 50_000.0,
            stage: DealStage::Lead,
            notes: String::new(),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };

        let deal_id = engine.crm().add_deal(sid, deal).await.unwrap();

        engine
            .crm()
            .advance_deal(&deal_id, DealStage::Qualified)
            .await
            .unwrap();

        let deals = engine
            .crm()
            .list_deals(sid, Some(DealStage::Qualified))
            .await
            .unwrap();
        assert_eq!(deals.len(), 1);
        assert_eq!(deals[0].stage, DealStage::Qualified);
    }

    // ── Invoice tests ──────────────────────────────────────────────

    #[tokio::test]
    async fn invoice_create_mark_paid_revenue() {
        let engine = make_engine();
        let sid = SessionId::new();
        let cid = ContactId::new();

        let items = vec![
            LineItem {
                description: "Consulting".into(),
                quantity: 10.0,
                unit_price: 150.0,
            },
            LineItem {
                description: "Support".into(),
                quantity: 1.0,
                unit_price: 500.0,
            },
        ];

        let inv_id = engine
            .invoices()
            .create_invoice(sid, cid, items, Utc::now())
            .await
            .unwrap();

        // Before paying, revenue should be 0.
        let rev = engine.invoices().total_revenue(sid).await.unwrap();
        assert!((rev - 0.0).abs() < f64::EPSILON);

        engine.invoices().mark_paid(&inv_id).await.unwrap();

        let rev = engine.invoices().total_revenue(sid).await.unwrap();
        assert!((rev - 2000.0).abs() < f64::EPSILON);
    }

    // ── Outreach tests ─────────────────────────────────────────────

    #[tokio::test]
    async fn outreach_create_and_list_sequence() {
        let engine = make_engine();
        let sid = SessionId::new();

        let steps = vec![
            SequenceStep {
                delay_days: 0,
                channel: "email".into(),
                template: "intro".into(),
            },
            SequenceStep {
                delay_days: 3,
                channel: "email".into(),
                template: "follow_up".into(),
            },
        ];

        engine
            .outreach()
            .create_sequence(sid, "Welcome flow".into(), steps)
            .await
            .unwrap();

        let seqs = engine.outreach().list_sequences(sid).await.unwrap();
        assert_eq!(seqs.len(), 1);
        assert_eq!(seqs[0].name, "Welcome flow");
        assert_eq!(seqs[0].steps.len(), 2);
    }

    #[tokio::test]
    async fn outreach_execute_sequence_fails_until_activated() {
        let engine = make_engine();
        let sid = SessionId::new();
        let contact = make_contact(sid);
        let cid = engine.crm().add_contact(sid, contact).await.unwrap();
        let steps = vec![SequenceStep {
            delay_days: 0,
            channel: "email".into(),
            template: "a".into(),
        }];
        let seq_id = engine
            .outreach()
            .create_sequence(sid, "seq".into(), steps)
            .await
            .unwrap();
        let err = engine
            .outreach()
            .execute_sequence(sid, seq_id, cid)
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("Active") || msg.contains("active"),
            "unexpected error: {msg}"
        );

        engine.outreach().activate_sequence(&seq_id).await.unwrap();
        let job_id = engine
            .outreach()
            .execute_sequence(sid, seq_id, cid)
            .await
            .unwrap();
        assert!(!job_id.to_string().is_empty());
    }

    #[tokio::test]
    async fn process_outreach_send_job_draft_holds_without_sending() {
        use crate::email::MockEmailAdapter;
        use crate::outreach::OutreachSendDispatch;
        use rusvel_core::domain::{Job, JobKind, JobStatus};

        let engine = make_engine();
        let sid = SessionId::new();
        let contact = make_contact(sid);
        let cid = engine.crm().add_contact(sid, contact).await.unwrap();
        let steps = vec![SequenceStep {
            delay_days: 0,
            channel: "email".into(),
            template: "intro".into(),
        }];
        let seq_id = engine
            .outreach()
            .create_sequence(sid, "flow".into(), steps)
            .await
            .unwrap();
        engine.outreach().activate_sequence(&seq_id).await.unwrap();

        let job = Job {
            id: JobId::new(),
            session_id: sid,
            kind: JobKind::OutreachSend,
            payload: serde_json::json!({
                "sequence_id": seq_id.to_string(),
                "sequence_run_id": uuid::Uuid::now_v7().to_string(),
                "contact_id": cid.to_string(),
                "step_index": 0,
                "channel": "email",
                "template": "intro",
            }),
            status: JobStatus::Queued,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retries: 0,
            max_retries: 2,
            error: None,
            metadata: serde_json::json!({}),
        };
        let mock = MockEmailAdapter::new();
        let dispatch = engine.process_outreach_send_job(&job, &mock).await.unwrap();
        match dispatch {
            OutreachSendDispatch::HoldForApproval(r) => {
                assert_eq!(r.output["to"], "alice@example.com");
            }
            OutreachSendDispatch::Complete { .. } => panic!("expected HoldForApproval"),
        }
        assert_eq!(mock.sent_len(), 0);
    }

    #[tokio::test]
    async fn process_outreach_send_job_approval_sends_and_schedules_next_step() {
        use crate::email::MockEmailAdapter;
        use crate::outreach::OutreachSendDispatch;
        use rusvel_core::domain::{Job, JobKind, JobResult, JobStatus};

        let engine = make_engine();
        let sid = SessionId::new();
        let contact = make_contact(sid);
        let cid = engine.crm().add_contact(sid, contact).await.unwrap();
        let steps = vec![
            SequenceStep {
                delay_days: 0,
                channel: "email".into(),
                template: "intro".into(),
            },
            SequenceStep {
                delay_days: 2,
                channel: "email".into(),
                template: "nudge".into(),
            },
        ];
        let seq_id = engine
            .outreach()
            .create_sequence(sid, "drip".into(), steps)
            .await
            .unwrap();
        engine.outreach().activate_sequence(&seq_id).await.unwrap();

        let run_key = uuid::Uuid::now_v7().to_string();
        let pending = JobResult {
            output: serde_json::json!({
                "sequence_id": seq_id.to_string(),
                "contact_id": cid.to_string(),
                "step_index": 0,
                "to": "alice@example.com",
                "subject": "drip — intro",
                "body": "Hello approved",
                "channel": "email",
            }),
            metadata: serde_json::json!({"engine": "gtm", "phase": "draft"}),
        };

        let job = Job {
            id: JobId::new(),
            session_id: sid,
            kind: JobKind::OutreachSend,
            payload: serde_json::json!({
                "sequence_id": seq_id.to_string(),
                "sequence_run_id": run_key,
                "contact_id": cid.to_string(),
                "step_index": 0,
                "channel": "email",
                "template": "intro",
            }),
            status: JobStatus::Queued,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retries: 0,
            max_retries: 2,
            error: None,
            metadata: serde_json::json!({
                "approval_pending_result": serde_json::to_value(&pending).unwrap(),
            }),
        };

        let mock = MockEmailAdapter::new();
        let dispatch = engine.process_outreach_send_job(&job, &mock).await.unwrap();
        match dispatch {
            OutreachSendDispatch::Complete { result, next } => {
                assert_eq!(result.output["status"], "sent");
                let next = next.expect("second step should be scheduled");
                assert_eq!(next.kind, JobKind::OutreachSend);
                assert_eq!(next.payload["step_index"], serde_json::json!(1));
            }
            OutreachSendDispatch::HoldForApproval(_) => panic!("expected Complete"),
        }
        assert_eq!(mock.sent_len(), 1);
    }

    // ── Engine trait test ──────────────────────────────────────────

    #[tokio::test]
    async fn health_returns_healthy() {
        use rusvel_core::engine::Engine;
        let engine = make_engine();
        let status = engine.health().await.unwrap();
        assert!(status.healthy);
    }
}
