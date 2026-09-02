//! Named secrets via [`AuthPort`] (in-memory in default binary). Values are stored in
//! [`Credential::metadata`] under `rusvel_secret_plain` — list/get API responses redact this field.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use rusvel_core::domain::{Credential, CredentialKind};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::AppState;

pub const SECRET_VALUE_META_KEY: &str = "rusvel_secret_plain";

#[derive(Debug, Serialize)]
pub struct SecretSummary {
    pub key: String,
    pub label: Option<String>,
    pub provider: String,
}

#[derive(Debug, Deserialize)]
pub struct UpsertSecretBody {
    #[serde(default)]
    pub label: Option<String>,
    pub value: String,
}

fn err(status: StatusCode, msg: impl Into<String>) -> (StatusCode, String) {
    (status, msg.into())
}

/// `GET /api/secrets` — keys and labels only (no secret material).
pub async fn list_secrets(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SecretSummary>>, (StatusCode, String)> {
    let keys = state
        .credentials
        .list_credential_keys()
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut out = Vec::new();
    for key in keys {
        if let Some(c) = state
            .credentials
            .get_credential(&key)
            .await
            .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        {
            let label = c
                .metadata
                .get("label")
                .and_then(|v| v.as_str())
                .map(String::from);
            out.push(SecretSummary {
                key,
                label,
                provider: c.provider,
            });
        }
    }
    Ok(Json(out))
}

/// `GET /api/secrets/{key}` — metadata only.
pub async fn get_secret_meta(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<Json<SecretSummary>, (StatusCode, String)> {
    let c = state
        .credentials
        .get_credential(&key)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "secret not found"))?;
    Ok(Json(SecretSummary {
        label: c
            .metadata
            .get("label")
            .and_then(|v| v.as_str())
            .map(String::from),
        provider: c.provider,
        key,
    }))
}

/// `PUT /api/secrets/{key}` — create or update secret value.
pub async fn upsert_secret(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
    Json(body): Json<UpsertSecretBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    if key.trim().is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "key must not be empty"));
    }
    if body.value.is_empty() {
        return Err(err(StatusCode::BAD_REQUEST, "value must not be empty"));
    }
    let mut meta = json!({
        SECRET_VALUE_META_KEY: body.value,
    });
    if let Some(l) = body.label.filter(|s| !s.is_empty()) {
        meta.as_object_mut()
            .map(|o| o.insert("label".into(), json!(l)));
    }
    let cred = Credential {
        provider: "rusvel_secret".into(),
        kind: CredentialKind::ApiKey,
        expires_at: None,
        metadata: meta,
    };
    state
        .credentials
        .store_credential(&key, cred)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/secrets/{key}`
pub async fn delete_secret(
    State(state): State<Arc<AppState>>,
    Path(key): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .credentials
        .delete_credential(&key)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(StatusCode::NO_CONTENT)
}

/// Read raw secret for automation resolution (internal).
pub async fn get_secret_plain(state: &AppState, key: &str) -> Option<String> {
    let c = state.credentials.get_credential(key).await.ok()??;
    c.metadata
        .get(SECRET_VALUE_META_KEY)
        .and_then(|v| v.as_str())
        .map(String::from)
}

/// Replace `{{secret:name}}` substrings in all JSON strings using [`AuthPort`].
pub async fn resolve_secret_placeholders(
    state: &AppState,
    mut v: serde_json::Value,
) -> serde_json::Value {
    resolve_walk(state, &mut v).await;
    v
}

fn resolve_walk<'a>(
    state: &'a AppState,
    v: &'a mut serde_json::Value,
) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        match v {
            serde_json::Value::Object(map) => {
                for (_, val) in map.iter_mut() {
                    resolve_walk(state, val).await;
                }
            }
            serde_json::Value::Array(arr) => {
                for val in arr.iter_mut() {
                    resolve_walk(state, val).await;
                }
            }
            serde_json::Value::String(s) => {
                *s = resolve_string(state, s).await;
            }
            _ => {}
        }
    })
}

async fn resolve_string(state: &AppState, s: &str) -> String {
    let mut out = s.to_string();
    let needle = "{{secret:";
    while let Some(start) = out.find(needle) {
        let after = &out[start + needle.len()..];
        let Some(end) = after.find("}}") else {
            break;
        };
        let key = after[..end].trim();
        if key.is_empty() {
            break;
        }
        let pat_len = needle.len() + end + 2;
        let replacement = get_secret_plain(state, key).await.unwrap_or_default();
        out.replace_range(start..start + pat_len, &replacement);
    }
    out
}
