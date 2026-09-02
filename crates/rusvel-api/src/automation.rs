//! Native automation dispatch: cron / webhook → same payload → flow or playbook.
//!
//! See [`rusvel_core::domain::AutomationTriggerPayload`] and constants
//! `AUTOMATION_CRON_EVENT_KIND` / `AUTOMATION_WEBHOOK_EVENT_KIND`.
//!
//! [`register_app_state_for_worker`] is called from [`crate::build_router_with_frontend`] so the
//! background job worker (in `rusvel-app`) can call [`dispatch_automation_trigger`] before HTTP
//! is accepting traffic (first registration wins).

use std::sync::{Arc, OnceLock};

static WORKER_APP_STATE: OnceLock<Arc<AppState>> = OnceLock::new();

/// Register `Arc<AppState>` for [`worker_app_state`] (job worker, webhooks enqueue path).
pub fn register_app_state_for_worker(state: Arc<AppState>) {
    let _ = WORKER_APP_STATE.set(state);
}

/// Resolves after the API router is built with [`crate::build_router_with_frontend`].
pub fn worker_app_state() -> Option<Arc<AppState>> {
    WORKER_APP_STATE.get().cloned()
}

use rusvel_core::domain::{
    AUTOMATION_CRON_EVENT_KIND, AutomationTriggerAction, AutomationTriggerPayload,
};
use rusvel_core::error::RusvelError;

/// [`JobKind::Custom`] payload carries `trigger`: [`AutomationTriggerPayload`].
pub const AUTOMATION_DISPATCH_JOB_KIND: &str = "rusvel.automation.dispatch";
use rusvel_core::id::{FlowId, SessionId};
use serde_json::{Value, json};

use crate::AppState;
use crate::playbooks;
use crate::secrets;

/// Reserved object key on flow `trigger_data` / playbook variables for job correlation (filled by the worker).
pub const RUSVEL_TRIGGER_KEY: &str = "rusvel";

/// Who started this automation run; merged into `trigger_data` / variables under [`RUSVEL_TRIGGER_KEY`].
#[derive(Debug, Clone)]
pub struct AutomationProvenance {
    pub job_id: String,
    pub trigger: String,
    pub schedule_id: Option<String>,
    pub event_kind: Option<String>,
}

/// Shallow-merge: existing `rusvel` keys are kept except server fields always overwrite.
pub fn merge_rusvel_provenance(td: &mut Value, provenance: Option<&AutomationProvenance>) {
    let Some(p) = provenance else {
        return;
    };
    if !td.is_object() {
        *td = json!({});
    }
    let Some(obj) = td.as_object_mut() else {
        return;
    };
    let mut merged = serde_json::Map::new();
    if let Some(Value::Object(existing)) = obj.get(RUSVEL_TRIGGER_KEY) {
        for (k, v) in existing {
            merged.insert(k.clone(), v.clone());
        }
    }
    merged.insert("job_id".into(), json!(p.job_id));
    merged.insert("trigger".into(), json!(p.trigger));
    if let Some(ref s) = p.schedule_id {
        if !s.is_empty() {
            merged.insert("schedule_id".into(), json!(s));
        }
    }
    if let Some(ref e) = p.event_kind {
        if !e.is_empty() {
            merged.insert("event_kind".into(), json!(e));
        }
    }
    obj.insert(RUSVEL_TRIGGER_KEY.into(), Value::Object(merged));
}

/// If `event_kind` is our automation cron kind, or `inner` looks like an automation payload, parse it.
pub fn parse_automation_trigger_from_cron(
    event_kind: &str,
    inner: &Value,
) -> Option<AutomationTriggerPayload> {
    if event_kind == AUTOMATION_CRON_EVENT_KIND {
        return serde_json::from_value(inner.clone()).ok();
    }
    // Convenience: allow generic cron rows whose `payload` embeds action + target_id
    if inner.get("action").is_some() && inner.get("target_id").is_some() {
        return serde_json::from_value(inner.clone()).ok();
    }
    None
}

/// Run flow or playbook; used by job worker and optionally MCP/HTTP helpers.
pub async fn dispatch_automation_trigger(
    state: Arc<AppState>,
    _session_id: SessionId,
    trigger: AutomationTriggerPayload,
    provenance: Option<AutomationProvenance>,
) -> Result<Value, String> {
    let prov = provenance.as_ref();
    match trigger.action {
        AutomationTriggerAction::RunFlow => {
            let engine = state
                .flow_engine
                .as_ref()
                .map(|e| e.as_ref())
                .ok_or_else(|| "Flow engine not available".to_string())?;
            let fid = trigger
                .target_id
                .parse::<uuid::Uuid>()
                .map(FlowId::from_uuid)
                .map_err(|_| "invalid flow target_id (expected UUID)".to_string())?;
            let mut td = trigger.variables;
            if !td.is_object() {
                td = json!({});
            }
            merge_rusvel_provenance(&mut td, prov);
            td = secrets::resolve_secret_placeholders(state.as_ref(), td).await;
            if let Some(dept) = trigger.department_id.clone() {
                td.as_object_mut()
                    .map(|o| o.insert("department_id".into(), json!(dept)));
            }
            let execution = engine.run_flow(&fid, td).await.map_err(|e| e.to_string())?;
            serde_json::to_value(&execution).map_err(|e| e.to_string())
        }
        AutomationTriggerAction::RunPlaybook => {
            let mut vars = trigger.variables;
            if !vars.is_object() {
                vars = json!({});
            }
            merge_rusvel_provenance(&mut vars, prov);
            let vars = secrets::resolve_secret_placeholders(state.as_ref(), vars).await;
            let run_id = playbooks::start_playbook_run(state, &trigger.target_id, vars).await?;
            Ok(json!({ "run_id": run_id, "playbook_id": trigger.target_id }))
        }
    }
}

/// Create or update a cron row that fires [`AUTOMATION_CRON_EVENT_KIND`] with `payload` = [`AutomationTriggerPayload`].
pub async fn upsert_automation_cron_schedule(
    state: Arc<AppState>,
    schedule_id: Option<String>,
    name: String,
    session_id: SessionId,
    schedule: String,
    trigger: AutomationTriggerPayload,
    enabled: bool,
) -> Result<Value, RusvelError> {
    let payload =
        serde_json::to_value(&trigger).map_err(|e| RusvelError::Serialization(e.to_string()))?;
    if let Some(id) = schedule_id.filter(|s| !s.trim().is_empty()) {
        let updated = state
            .cron_scheduler
            .update(
                id.trim(),
                Some(name),
                Some(schedule),
                Some(enabled),
                Some(payload),
                Some(AUTOMATION_CRON_EVENT_KIND.to_string()),
            )
            .await?;
        return serde_json::to_value(&updated)
            .map_err(|e| RusvelError::Serialization(e.to_string()));
    }
    let created = state
        .cron_scheduler
        .create(
            name,
            session_id,
            schedule,
            payload,
            AUTOMATION_CRON_EVENT_KIND.to_string(),
            enabled,
        )
        .await?;
    serde_json::to_value(&created).map_err(|e| RusvelError::Serialization(e.to_string()))
}
