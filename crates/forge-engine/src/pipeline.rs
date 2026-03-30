//! Cross-engine pipeline orchestration (S-042): ordered steps with events and
//! a persisted [`FlowExecution`] record (same object store kind as [`FlowEngine`]).

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use rusvel_core::domain::{
    ContentKind, Event, FlowExecution, FlowExecutionStatus, FlowNodeResult, FlowNodeStatus,
    ObjectFilter,
};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use rusvel_core::id::{EventId, FlowExecutionId, FlowId};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use rusvel_core::ports::EventPort;

use crate::ForgeEngine;
use crate::events as forge_events;

/// Same bucket as `flow-engine` for execution records.
pub const FLOW_EXECUTIONS_OBJECT_KIND: &str = "flow_executions";

/// `trigger_data.kind` / metadata tag for forge pipeline runs (S-042).
pub const FORGE_PIPELINE_FLOW_PREFIX: &str = "forge_pipeline";

/// One step in the cross-engine harvest → content pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStepKind {
    Scan,
    Score,
    Propose,
    DraftContent,
}

impl PipelineStepKind {
    pub fn node_key(&self) -> &'static str {
        match self {
            PipelineStepKind::Scan => "scan",
            PipelineStepKind::Score => "score",
            PipelineStepKind::Propose => "propose",
            PipelineStepKind::DraftContent => "draft_content",
        }
    }
}

/// Definition passed to [`ForgeEngine::orchestrate_pipeline`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineOrchestrationDef {
    #[serde(default = "default_steps")]
    pub steps: Vec<PipelineStepKind>,
    #[serde(default = "default_profile")]
    pub proposal_profile: String,
    #[serde(default)]
    pub draft_topic: Option<String>,
    #[serde(default = "default_draft_kind")]
    pub draft_kind: ContentKind,
    /// Harvest scan sources for the pipeline `scan` step (default: mock).
    #[serde(default = "default_harvest_sources")]
    pub harvest_sources: Vec<String>,
    #[serde(default)]
    pub harvest_query: String,
    #[serde(default)]
    pub harvest_cdp_extract_js: Option<String>,
    /// CDP HTTP base for the pipeline scan step (overrides `RUSVEL_CDP_ENDPOINT`).
    #[serde(default)]
    pub harvest_cdp_endpoint: Option<String>,
}

fn default_steps() -> Vec<PipelineStepKind> {
    vec![
        PipelineStepKind::Scan,
        PipelineStepKind::Score,
        PipelineStepKind::Propose,
        PipelineStepKind::DraftContent,
    ]
}

fn default_profile() -> String {
    "default".into()
}

fn default_draft_kind() -> ContentKind {
    ContentKind::Blog
}

fn default_harvest_sources() -> Vec<String> {
    vec!["mock".into()]
}

impl Default for PipelineOrchestrationDef {
    fn default() -> Self {
        Self {
            steps: default_steps(),
            proposal_profile: default_profile(),
            draft_topic: None,
            draft_kind: default_draft_kind(),
            harvest_sources: default_harvest_sources(),
            harvest_query: String::new(),
            harvest_cdp_extract_js: None,
            harvest_cdp_endpoint: None,
        }
    }
}

/// Host-supplied execution of harvest/content steps (implemented in `rusvel-api`).
#[async_trait]
pub trait PipelineStepRunner: Send + Sync {
    async fn scan(&self, session_id: &SessionId) -> Result<Value>;
    async fn score(&self, session_id: &SessionId, after_scan: &Value) -> Result<Value>;
    async fn propose(&self, session_id: &SessionId, ctx: &Value, profile: &str) -> Result<Value>;
    async fn draft_content(
        &self,
        session_id: &SessionId,
        ctx: &Value,
        draft_topic: Option<&str>,
        kind: ContentKind,
    ) -> Result<Value>;
}

async fn emit_pipeline_event(
    events: &Arc<dyn EventPort>,
    session_id: SessionId,
    execution_id: &str,
    kind: &str,
    payload: Value,
) -> Result<()> {
    events
        .emit(Event {
            id: EventId::new(),
            session_id: Some(session_id),
            run_id: None,
            source: "forge".into(),
            kind: kind.into(),
            payload,
            created_at: Utc::now(),
            metadata: json!({ "pipeline_execution_id": execution_id }),
        })
        .await?;
    Ok(())
}

impl ForgeEngine {
    /// Run an ordered pipeline, emit per-step events, persist a [`FlowExecution`].
    pub async fn orchestrate_pipeline(
        &self,
        session_id: SessionId,
        def: PipelineOrchestrationDef,
        runner: &dyn PipelineStepRunner,
    ) -> Result<FlowExecution> {
        if def.steps.is_empty() {
            return Err(RusvelError::Validation(
                "pipeline steps must not be empty".into(),
            ));
        }

        let execution_id = FlowExecutionId::new();
        let flow_id = FlowId::new();
        let started_at = Utc::now();
        let mut accumulated = json!({});
        let mut node_results = std::collections::HashMap::new();

        let exec_str = execution_id.to_string();
        emit_pipeline_event(
            &self.events,
            session_id,
            &exec_str,
            forge_events::PIPELINE_STARTED,
            json!({
                "execution_id": execution_id.to_string(),
                "flow_id": flow_id.to_string(),
                "session_id": session_id.to_string(),
                "steps": def.steps.iter().map(|s| s.node_key()).collect::<Vec<_>>(),
            }),
        )
        .await?;

        let mut last_err: Option<String> = None;

        for step in &def.steps {
            let key = step.node_key();
            let step_start = Utc::now();

            emit_pipeline_event(
                &self.events,
                session_id,
                &exec_str,
                forge_events::PIPELINE_STEP_STARTED,
                json!({
                    "execution_id": execution_id.to_string(),
                    "step": key,
                }),
            )
            .await?;

            let step_out: std::result::Result<Value, RusvelError> = match step {
                PipelineStepKind::Scan => runner.scan(&session_id).await,
                PipelineStepKind::Score => runner.score(&session_id, &accumulated).await,
                PipelineStepKind::Propose => {
                    runner
                        .propose(&session_id, &accumulated, &def.proposal_profile)
                        .await
                }
                PipelineStepKind::DraftContent => {
                    runner
                        .draft_content(
                            &session_id,
                            &accumulated,
                            def.draft_topic.as_deref(),
                            def.draft_kind.clone(),
                        )
                        .await
                }
            };

            match step_out {
                Ok(out) => {
                    accumulated[key] = out.clone();
                    node_results.insert(
                        key.to_string(),
                        FlowNodeResult {
                            status: FlowNodeStatus::Succeeded,
                            output: Some(out),
                            error: None,
                            started_at: Some(step_start),
                            finished_at: Some(Utc::now()),
                            metadata: serde_json::Value::default(),
                        },
                    );
                    emit_pipeline_event(
                        &self.events,
                        session_id,
                        &exec_str,
                        forge_events::PIPELINE_STEP_COMPLETED,
                        json!({
                            "execution_id": execution_id.to_string(),
                            "step": key,
                        }),
                    )
                    .await?;
                }
                Err(e) => {
                    tracing::error!(step = key, error = %e, "Pipeline step failed");
                    last_err = Some(format!("{key}: {e}"));
                    node_results.insert(
                        key.to_string(),
                        FlowNodeResult {
                            status: FlowNodeStatus::Failed,
                            output: None,
                            error: Some(e.to_string()),
                            started_at: Some(step_start),
                            finished_at: Some(Utc::now()),
                            metadata: serde_json::Value::default(),
                        },
                    );
                    emit_pipeline_event(
                        &self.events,
                        session_id,
                        &exec_str,
                        forge_events::PIPELINE_FAILED,
                        json!({
                            "execution_id": execution_id.to_string(),
                            "step": key,
                            "error": e.to_string(),
                        }),
                    )
                    .await?;
                    break;
                }
            }
        }

        let failed = last_err.is_some();
        let finished_at = Utc::now();
        let status = if failed {
            FlowExecutionStatus::Failed
        } else {
            FlowExecutionStatus::Succeeded
        };

        let execution = FlowExecution {
            id: execution_id,
            flow_id,
            status: status.clone(),
            trigger_data: json!({
                "session_id": session_id.to_string(),
                "def": def,
                "kind": "forge_pipeline",
            }),
            node_results,
            started_at,
            finished_at: Some(finished_at),
            error: last_err.clone(),
            metadata: json!({
                "source": "forge.pipeline",
                "pipeline": true,
            }),
        };

        let exec_value = serde_json::to_value(&execution)?;
        self.storage
            .objects()
            .put(
                FLOW_EXECUTIONS_OBJECT_KIND,
                &execution.id.to_string(),
                exec_value,
            )
            .await?;

        if !failed {
            emit_pipeline_event(
                &self.events,
                session_id,
                &exec_str,
                forge_events::PIPELINE_COMPLETED,
                json!({
                    "execution_id": execution.id.to_string(),
                    "flow_id": flow_id.to_string(),
                }),
            )
            .await?;
        }

        Ok(execution)
    }

    /// List persisted pipeline executions (same store as flow executions; filter by metadata).
    pub async fn list_pipeline_executions(&self) -> Result<Vec<FlowExecution>> {
        let values = self
            .storage
            .objects()
            .list(FLOW_EXECUTIONS_OBJECT_KIND, ObjectFilter::default())
            .await?;
        let mut out: Vec<FlowExecution> = values
            .into_iter()
            .filter_map(|v| serde_json::from_value::<FlowExecution>(v).ok())
            .filter(|e| {
                e.metadata
                    .get("source")
                    .and_then(|s| s.as_str())
                    .map(|s| s == "forge.pipeline")
                    .unwrap_or(false)
            })
            .collect();
        out.sort_by_key(|e| e.started_at);
        Ok(out)
    }
}
