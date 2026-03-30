//! First-party connectors (GitHub PAT storage). OAuth can be added later; PAT works without app registration.

use std::sync::Arc;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;

const GH_STORE: &str = "github_connector";
const GH_ID: &str = "default";

#[derive(Debug, Serialize)]
pub struct GitHubConnectorStatus {
    pub connected: bool,
}

#[derive(Debug, Deserialize)]
pub struct GitHubPatBody {
    pub token: String,
}

/// `GET /api/connectors/github/status` — whether a token is stored (never returns the secret).
pub async fn github_status(
    State(state): State<Arc<AppState>>,
) -> Result<Json<GitHubConnectorStatus>, (StatusCode, String)> {
    let connected = state
        .storage
        .objects()
        .get(GH_STORE, GH_ID)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .and_then(|v| {
            v.get("token")
                .and_then(|t| t.as_str())
                .map(|s| !s.trim().is_empty())
        })
        .unwrap_or(false);
    Ok(Json(GitHubConnectorStatus { connected }))
}

/// `POST /api/connectors/github/pat` — store a GitHub personal access token (server-side object store).
pub async fn github_set_pat(
    State(state): State<Arc<AppState>>,
    Json(body): Json<GitHubPatBody>,
) -> Result<Json<GitHubConnectorStatus>, (StatusCode, String)> {
    let token = body.token.trim();
    if token.len() < 8 {
        return Err((StatusCode::BAD_REQUEST, "token too short".into()));
    }
    let val = json!({ "token": token });
    state
        .storage
        .objects()
        .put(GH_STORE, GH_ID, val)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(GitHubConnectorStatus { connected: true }))
}

/// `DELETE /api/connectors/github/pat` — remove stored token.
pub async fn github_clear_pat(
    State(state): State<Arc<AppState>>,
) -> Result<StatusCode, (StatusCode, String)> {
    let _ = state.storage.objects().delete(GH_STORE, GH_ID).await;
    Ok(StatusCode::NO_CONTENT)
}
