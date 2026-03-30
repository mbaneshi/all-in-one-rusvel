//! Saved chat / agent outputs (Claude-style Artifacts) in `ObjectStore`.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;
use rusvel_core::domain::ObjectFilter;

const STORE: &str = "artifacts";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub kind: String,
    pub body: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct CreateArtifactBody {
    pub title: String,
    #[serde(default)]
    pub kind: String,
    pub body: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub department: Option<String>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

/// `GET /api/artifacts` — list artifacts (newest first by id/time; limit 100).
pub async fn list_artifacts(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<ArtifactRecord>>, (StatusCode, String)> {
    let raw = state
        .storage
        .objects()
        .list(
            STORE,
            ObjectFilter {
                conversation_id: None,
                limit: Some(100),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut out: Vec<ArtifactRecord> = raw
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(Json(out))
}

/// `GET /api/artifacts/{id}` — fetch one artifact.
pub async fn get_artifact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<ArtifactRecord>, (StatusCode, String)> {
    let v = state
        .storage
        .objects()
        .get(STORE, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "artifact not found".into()))?;
    let rec: ArtifactRecord = serde_json::from_value(v)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rec))
}

/// `POST /api/artifacts` — create artifact.
pub async fn create_artifact(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateArtifactBody>,
) -> Result<Json<ArtifactRecord>, (StatusCode, String)> {
    if body.title.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "title required".into()));
    }
    let id = uuid::Uuid::now_v7().to_string();
    let rec = ArtifactRecord {
        id: id.clone(),
        title: body.title,
        kind: if body.kind.is_empty() {
            "markdown".into()
        } else {
            body.kind
        },
        body: body.body,
        session_id: body.session_id,
        department: body.department,
        created_at: Utc::now().to_rfc3339(),
        metadata: if body.metadata.is_null() {
            json!({})
        } else {
            body.metadata
        },
    };
    let val = serde_json::to_value(&rec)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    state
        .storage
        .objects()
        .put(STORE, &id, val)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rec))
}

/// `DELETE /api/artifacts/{id}` — remove artifact.
pub async fn delete_artifact(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .storage
        .objects()
        .delete(STORE, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}
