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
    AutomationTriggerAction, AutomationTriggerPayload, AUTOMATION_CRON_EVENT_KIND,
};

/// [`JobKind::Custom`] payload carries `trigger`: [`AutomationTriggerPayload`].
pub const AUTOMATION_DISPATCH_JOB_KIND: &str = "rusvel.automation.dispatch";
use rusvel_core::id::{FlowId, SessionId};
use serde_json::{Value, json};

use crate::AppState;
use crate::playbooks;
use crate::secrets;

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
    session_id: SessionId,
    trigger: AutomationTriggerPayload,
) -> Result<Value, String> {
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
            td = secrets::resolve_secret_placeholders(state.as_ref(), td).await;
            if let Some(dept) = trigger.department_id.clone() {
                td.as_object_mut()
                    .map(|o| o.insert("department_id".into(), json!(dept)));
            }
            let execution = engine
                .run_flow(&fid, td)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_value(&execution).map_err(|e| e.to_string())
        }
        AutomationTriggerAction::RunPlaybook => {
            let vars = secrets::resolve_secret_placeholders(state.as_ref(), trigger.variables).await;
            let run_id = playbooks::start_playbook_run(state, &trigger.target_id, vars).await?;
            Ok(json!({ "run_id": run_id, "playbook_id": trigger.target_id }))
        }
    }
}
