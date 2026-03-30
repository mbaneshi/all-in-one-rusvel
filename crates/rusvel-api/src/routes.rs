//! HTTP handler functions for the RUSVEL API.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::Deserialize;
use std::sync::Arc;

use rusvel_core::domain::*;
use rusvel_core::id::SessionId;
use uuid::Uuid;

use crate::AppState;

/// Parse a path string into a `SessionId` via UUID.
fn parse_session_id(id: &str) -> Result<SessionId, (StatusCode, String)> {
    let uuid: Uuid = id
        .parse()
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid session id".into()))?;
    Ok(SessionId::from_uuid(uuid))
}

// ── Request bodies ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateSessionBody {
    pub name: String,
    pub kind: SessionKind,
}

#[derive(Debug, Deserialize)]
pub struct CreateGoalBody {
    pub title: String,
    pub description: String,
    pub timeframe: Timeframe,
}

// ── Handlers ─────────────────────────────────────────────────────

pub async fn health(State(state): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let uptime_seconds = state.boot_time.elapsed().as_secs();

    let db_check = match state.database.execute_sql("SELECT 1") {
        Ok(_) => "ok".to_string(),
        Err(e) => format!("error: {e}"),
    };

    let vector_check = if state.vector_store.is_some() {
        "ok"
    } else {
        "not_configured"
    };

    let embed_check = if state.embedding.is_some() {
        "ok"
    } else {
        "not_configured"
    };

    let degraded = db_check != "ok" || !state.failed_departments.is_empty();
    let status = if degraded { "degraded" } else { "ok" };

    let failed: Vec<serde_json::Value> = state
        .failed_departments
        .iter()
        .map(|(id, msg)| serde_json::json!({ "id": id, "error": msg }))
        .collect();

    Json(serde_json::json!({
        "status": status,
        "uptime_seconds": uptime_seconds,
        "checks": {
            "database": db_check,
            "vector_store": vector_check,
            "embedding": embed_check,
        },
        "departments": {
            "failed": failed,
        }
    }))
}

pub async fn list_sessions(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SessionSummary>>, (StatusCode, String)> {
    state
        .sessions
        .list()
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn create_session(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateSessionBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, String)> {
    let now = Utc::now();
    let session = Session {
        id: SessionId::new(),
        name: body.name,
        kind: body.kind,
        tags: vec![],
        config: SessionConfig::default(),
        created_at: now,
        updated_at: now,
        metadata: serde_json::json!({}),
    };
    let id = state
        .sessions
        .create(session)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(serde_json::json!({ "id": id }))))
}

pub async fn get_session(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Session>, (StatusCode, String)> {
    let sid = parse_session_id(&id)?;
    state
        .sessions
        .load(&sid)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))
}

pub async fn mission_today(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<forge_engine::DailyPlan>, (StatusCode, String)> {
    let sid = parse_session_id(&id)?;
    state
        .forge
        .mission_today(&sid)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn list_goals(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Goal>>, (StatusCode, String)> {
    let sid = parse_session_id(&id)?;
    state
        .forge
        .list_goals(&sid)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

pub async fn create_goal(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<CreateGoalBody>,
) -> Result<(StatusCode, Json<Goal>), (StatusCode, String)> {
    let sid = parse_session_id(&id)?;
    let goal = state
        .forge
        .set_goal(&sid, body.title, body.description, body.timeframe)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(goal)))
}

pub async fn query_events(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<Vec<Event>>, (StatusCode, String)> {
    let sid = parse_session_id(&id)?;
    let filter = EventFilter {
        session_id: Some(sid),
        ..Default::default()
    };
    state
        .events
        .query(filter)
        .await
        .map(Json)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}
