//! MCP Servers CRUD — manage Model Context Protocol server configurations.
//!
//! MCP servers are passed to claude -p via --mcp-config flag.
//! Scoped by department via metadata.engine.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};

use crate::AppState;

const STORE_KIND: &str = "mcp_servers";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    pub server_type: String,     // "stdio" | "http" | "sse" | "ws"
    pub command: Option<String>, // for stdio: command to run
    pub args: Vec<String>,       // for stdio: command arguments
    pub url: Option<String>,     // for http/sse/ws: endpoint URL
    pub env: serde_json::Value,  // environment variables
    pub enabled: bool,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Deserialize)]
pub struct McpQuery {
    pub engine: Option<String>,
}

pub async fn list_mcp_servers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<McpQuery>,
) -> Result<Json<Vec<McpServerConfig>>, (StatusCode, String)> {
    let all = state
        .storage
        .objects()
        .list(STORE_KIND, rusvel_core::domain::ObjectFilter::default())
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut servers: Vec<McpServerConfig> = all
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();

    if let Some(ref engine) = params.engine {
        servers.retain(|s| {
            let eng = s.metadata.get("engine").and_then(|e| e.as_str());
            eng == Some(engine) || eng.is_none()
        });
    }

    Ok(Json(servers))
}

pub async fn create_mcp_server(
    State(state): State<Arc<AppState>>,
    Json(mut server): Json<McpServerConfig>,
) -> Result<(StatusCode, Json<McpServerConfig>), (StatusCode, String)> {
    if server.id.is_empty() {
        server.id = uuid::Uuid::now_v7().to_string();
    }
    state
        .storage
        .objects()
        .put(
            STORE_KIND,
            &server.id,
            serde_json::to_value(&server).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(server)))
}

pub async fn get_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<McpServerConfig>, (StatusCode, String)> {
    let val = state
        .storage
        .objects()
        .get(STORE_KIND, &id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    match val {
        Some(v) => {
            let server: McpServerConfig = serde_json::from_value(v)
                .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
            Ok(Json(server))
        }
        None => Err((StatusCode::NOT_FOUND, format!("MCP server {id} not found"))),
    }
}

pub async fn update_mcp_server(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(server): Json<McpServerConfig>,
) -> Result<Json<McpServerConfig>, (StatusCode, String)> {
    state
        .storage
        .objects()
        .put(
            STORE_KIND,
            &id,
            serde_json::to_value(&server).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?,
        )
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(server))
}

pub async fn delete_mcp_server(
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

/// Build --mcp-config JSON for enabled MCP servers in a department.
pub async fn build_mcp_config_for_engine(state: &Arc<AppState>, engine: &str) -> Option<String> {
    let servers: Vec<McpServerConfig> = state
        .storage
        .objects()
        .list(STORE_KIND, rusvel_core::domain::ObjectFilter::default())
        .await
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .filter(|s: &McpServerConfig| s.enabled)
        .filter(|s| {
            let eng = s.metadata.get("engine").and_then(|e| e.as_str());
            eng == Some(engine) || eng.is_none()
        })
        .collect();

    if servers.is_empty() {
        return None;
    }

    // Build mcpServers JSON matching Claude Code format
    let mut mcp_json = serde_json::Map::new();
    for s in &servers {
        let mut entry = serde_json::Map::new();
        match s.server_type.as_str() {
            "stdio" => {
                if let Some(ref cmd) = s.command {
                    entry.insert("command".into(), serde_json::Value::String(cmd.clone()));
                    entry.insert(
                        "args".into(),
                        serde_json::to_value(&s.args).unwrap_or_default(),
                    );
                }
            }
            "http" | "sse" | "ws" => {
                if let Some(ref url) = s.url {
                    entry.insert("url".into(), serde_json::Value::String(url.clone()));
                }
            }
            _ => {}
        }
        if !s.env.is_null() {
            entry.insert("env".into(), s.env.clone());
        }
        mcp_json.insert(s.name.clone(), serde_json::Value::Object(entry));
    }

    let config = serde_json::json!({ "mcpServers": mcp_json });
    Some(config.to_string())
}
