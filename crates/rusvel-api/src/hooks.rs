//! Hooks CRUD — lifecycle event automation.
//!
//! Hooks fire on events like `PreToolUse`, `PostToolUse`, `SessionStart`, etc.
//! They can run commands, call HTTP endpoints, or use prompts.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::AppState;

const STORE_KIND: &str = "hooks";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HookDefinition {
    pub id: String,
    pub name: String,
    pub event: String,     // "PreToolUse" | "PostToolUse" | "SessionStart" | etc.
    pub matcher: String,   // regex/glob to match on (e.g., "Bash" for PreToolUse)
    pub hook_type: String, // "command" | "http" | "prompt"
    pub action: String,    // shell command, URL, or prompt text
    pub enabled: bool,
    pub metadata: serde_json::Value,
}

/// All supported hook events.
pub const HOOK_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PermissionRequest",
    "Notification",
    "SubagentStart",
    "SubagentStop",
    "TaskCompleted",
    "ConfigChange",
    "PreCompact",
    "PostCompact",
    "UserPromptSubmit",
    "Stop",
    "StopFailure",
];

#[derive(Debug, Deserialize)]
pub struct HookQuery {
    pub engine: Option<String>,
    pub event: Option<String>,
}

pub async fn list_hooks(
    State(state): State<Arc<AppState>>,
    Query(params): Query<HookQuery>,
) -> Result<Json<Vec<HookDefinition>>, (StatusCode, String)> {
    let all = state
        .storage
        .objects()
        .list(STORE_KIND, rusvel_core::domain::ObjectFilter::default())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut hooks: Vec<HookDefinition> = all
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    if let Some(ref engine) = params.engine {
        hooks.retain(|h| {
            let eng = h.metadata.get("engine").and_then(|e| e.as_str());
            eng == Some(engine) || eng.is_none()
        });
    }
    if let Some(ref event) = params.event {
        hooks.retain(|h| h.event == *event);
    }

    Ok(Json(hooks))
}

pub async fn create_hook(
    State(state): State<Arc<AppState>>,
    Json(mut hook): Json<HookDefinition>,
) -> Result<(StatusCode, Json<HookDefinition>), (StatusCode, String)> {
    if hook.id.is_empty() {
        hook.id = uuid::Uuid::now_v7().to_string();
    }
    state
        .storage
        .objects()
        .put(
            STORE_KIND,
            &hook.id,
            serde_json::to_value(&hook).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(hook)))
}

pub async fn get_hook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<HookDefinition>, (StatusCode, String)> {
    let val = state
        .storage
        .objects()
        .get(STORE_KIND, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match val {
        Some(v) => {
            let hook: HookDefinition = serde_json::from_value(v)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(Json(hook))
        }
        None => Err((StatusCode::NOT_FOUND, format!("Hook {id} not found"))),
    }
}

pub async fn update_hook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(hook): Json<HookDefinition>,
) -> Result<Json<HookDefinition>, (StatusCode, String)> {
    state
        .storage
        .objects()
        .put(
            STORE_KIND,
            &id,
            serde_json::to_value(&hook).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(hook))
}

pub async fn delete_hook(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .storage
        .objects()
        .delete(STORE_KIND, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn list_hook_events() -> Json<Vec<&'static str>> {
    Json(HOOK_EVENTS.to_vec())
}
