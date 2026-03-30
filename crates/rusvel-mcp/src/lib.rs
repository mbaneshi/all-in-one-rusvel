//! MCP (Model Context Protocol) server surface for RUSVEL.
//!
//! Exposes Forge Engine capabilities as MCP tools over JSON-RPC/stdio.
//! This is a lightweight JSON-RPC implementation that reads newline-delimited
//! JSON from stdin and writes responses to stdout.
//!
//! HTTP transport: [`http::nest_mcp_http`] (POST `/mcp`, GET `/mcp/sse`).

use std::sync::Arc;

use chrono::Utc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use forge_engine::ForgeEngine;
use rusvel_core::domain::*;
use rusvel_core::error::RusvelError;
use rusvel_core::id::SessionId;
use rusvel_core::ports::SessionPort;

pub mod http;

// ════════════════════════════════════════════════════════════════════
//  Error
// ════════════════════════════════════════════════════════════════════

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("invalid params: {0}")]
    InvalidParams(String),
    #[error("engine error: {0}")]
    Engine(#[from] rusvel_core::error::RusvelError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

// ════════════════════════════════════════════════════════════════════
//  JSON-RPC (stdio + HTTP)
// ════════════════════════════════════════════════════════════════════

pub(crate) mod jsonrpc {
    use serde::{Deserialize, Serialize};

    use super::{McpError, RusvelMcp};

    #[derive(Debug, Clone, Deserialize)]
    pub struct JsonRpcRequest {
        #[allow(dead_code)]
        pub jsonrpc: String,
        pub id: serde_json::Value,
        pub method: String,
        #[serde(default)]
        pub params: serde_json::Value,
    }

    #[derive(Debug, Serialize)]
    pub struct JsonRpcResponse {
        jsonrpc: String,
        id: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        result: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<JsonRpcError>,
    }

    #[derive(Debug, Serialize)]
    struct JsonRpcError {
        code: i32,
        message: String,
    }

    impl JsonRpcResponse {
        pub fn success(id: serde_json::Value, result: serde_json::Value) -> Self {
            Self {
                jsonrpc: "2.0".into(),
                id,
                result: Some(result),
                error: None,
            }
        }
        pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
            Self {
                jsonrpc: "2.0".into(),
                id,
                result: None,
                error: Some(JsonRpcError { code, message }),
            }
        }
    }

    pub async fn dispatch(
        mcp: &RusvelMcp,
        req: JsonRpcRequest,
    ) -> Result<Option<JsonRpcResponse>, McpError> {
        let is_notification = req.method.starts_with("notifications/");
        if is_notification {
            mcp.handle_method(&req.method, req.params).await?;
            return Ok(None);
        }
        let resp = match mcp.handle_method(&req.method, req.params).await {
            Ok(result) => JsonRpcResponse::success(req.id, result),
            Err(e) => JsonRpcResponse::error(req.id, -32603, e.to_string()),
        };
        Ok(Some(resp))
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tool definitions
// ════════════════════════════════════════════════════════════════════

fn tool_definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "session_list",
            "description": "List all sessions",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "session_create",
            "description": "Create a new session",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Session name" },
                    "kind": { "type": "string", "enum": ["Project", "Lead", "ContentCampaign", "General"] }
                },
                "required": ["name", "kind"]
            }
        },
        {
            "name": "mission_today",
            "description": "Get today's prioritized plan for a session",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session UUID" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "mission_goals",
            "description": "List goals for a session",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session UUID" }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "mission_add_goal",
            "description": "Add a goal to a session",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string", "description": "Session UUID" },
                    "title": { "type": "string" },
                    "description": { "type": "string" },
                    "timeframe": { "type": "string", "enum": ["Day", "Week", "Month", "Quarter"] }
                },
                "required": ["session_id", "title", "description", "timeframe"]
            }
        },
        {
            "name": "automation_run_flow",
            "description": "Run a saved flow (DAG) by id. Uses the same dispatcher as cron/webhook automation; requires app state (start API so /api registers worker state, or use streamable HTTP MCP).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "flow_id": { "type": "string", "description": "Flow UUID" },
                    "variables": { "type": "object", "description": "Optional trigger_data object" },
                    "department_id": { "type": "string" }
                },
                "required": ["session_id", "flow_id"]
            }
        },
        {
            "name": "automation_run_playbook",
            "description": "Start a playbook run by playbook id (builtin or user-stored).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": { "type": "string" },
                    "playbook_id": { "type": "string" },
                    "variables": { "type": "object" }
                },
                "required": ["session_id", "playbook_id"]
            }
        },
        {
            "name": "automation_upsert_cron_schedule",
            "description": "Create or update a cron schedule with event_kind rusvel.automation.v1 and payload = AutomationTriggerPayload (same as POST /api/cron). Omit schedule_id to create; set schedule_id to update. Requires app state (HTTP API).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "schedule_id": { "type": "string", "description": "Existing cron id for update; omit to create" },
                    "name": { "type": "string" },
                    "session_id": { "type": "string" },
                    "schedule": { "type": "string", "description": "Preset hourly|daily|weekly or cron expression" },
                    "action": { "type": "string", "enum": ["run_flow", "run_playbook"] },
                    "target_id": { "type": "string", "description": "Flow UUID or playbook id" },
                    "variables": { "type": "object" },
                    "department_id": { "type": "string" },
                    "enabled": { "type": "boolean", "default": true }
                },
                "required": ["name", "session_id", "schedule", "action", "target_id"]
            }
        },
        {
            "name": "visual_inspect",
            "description": "Run visual regression tests on the frontend and return results. Captures screenshots of all routes and compares against baselines.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "routes": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Specific routes to test (e.g. [\"/\", \"/dept/forge\"]). Empty array = all routes."
                    },
                    "update_baselines": {
                        "type": "boolean",
                        "description": "If true, update baseline screenshots instead of comparing"
                    }
                }
            }
        }
    ])
}

// ════════════════════════════════════════════════════════════════════
//  RusvelMcp — core dispatcher
// ════════════════════════════════════════════════════════════════════

pub struct RusvelMcp {
    engine: Arc<ForgeEngine>,
    session: Arc<dyn SessionPort>,
}

impl RusvelMcp {
    pub fn new(engine: Arc<ForgeEngine>, session: Arc<dyn SessionPort>) -> Self {
        Self { engine, session }
    }

    /// Handle an MCP method call and return a JSON result.
    pub async fn handle_method(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        match method {
            "initialize" => Ok(serde_json::json!({
                "protocolVersion": "2025-11-25",
                "serverInfo": { "name": "rusvel-mcp", "version": "0.1.0" },
                "capabilities": { "tools": {} }
            })),
            "tools/list" => Ok(serde_json::json!({ "tools": tool_definitions() })),
            "tools/call" => self.handle_tool_call(params).await,
            "notifications/initialized" | "notifications/cancelled" => Ok(serde_json::Value::Null),
            _ => Err(McpError::UnknownTool(method.into())),
        }
    }

    async fn handle_tool_call(
        &self,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, McpError> {
        let name = params["name"]
            .as_str()
            .ok_or_else(|| McpError::InvalidParams("missing tool name".into()))?;
        let args = &params["arguments"];

        let result = match name {
            "session_list" => {
                let sessions = self.session.list().await?;
                serde_json::to_value(&sessions)?
            }
            "session_create" => {
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("missing name".into()))?;
                let kind: SessionKind = serde_json::from_value(args["kind"].clone())
                    .map_err(|e| McpError::InvalidParams(e.to_string()))?;
                let session = Session {
                    id: SessionId::new(),
                    name: name.into(),
                    kind,
                    tags: vec![],
                    config: SessionConfig::default(),
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    metadata: serde_json::json!({}),
                };
                let id = self.session.create(session).await?;
                serde_json::json!({ "session_id": id })
            }
            "mission_today" => {
                let sid = parse_session_id(args)?;
                let plan = self.engine.mission_today(&sid).await?;
                serde_json::to_value(&plan)?
            }
            "mission_goals" => {
                let sid = parse_session_id(args)?;
                let goals = self.engine.list_goals(&sid).await?;
                serde_json::to_value(&goals)?
            }
            "mission_add_goal" => {
                let sid = parse_session_id(args)?;
                let title = args["title"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("missing title".into()))?
                    .to_string();
                let desc = args["description"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("missing description".into()))?
                    .to_string();
                let timeframe: Timeframe = serde_json::from_value(args["timeframe"].clone())
                    .map_err(|e| McpError::InvalidParams(e.to_string()))?;
                let goal = self.engine.set_goal(&sid, title, desc, timeframe).await?;
                serde_json::to_value(&goal)?
            }
            "automation_run_flow" => {
                let app = rusvel_api::automation::worker_app_state().ok_or_else(|| {
                    McpError::InvalidParams(
                        "automation tools need app state: run the HTTP API (or MCP HTTP) so the router registers worker state; stdio-only MCP has no automation dispatch"
                            .into(),
                    )
                })?;
                let sid = parse_session_id(args)?;
                let flow_id = args["flow_id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("flow_id required".into()))?
                    .to_string();
                let vars = args
                    .get("variables")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let department_id = args
                    .get("department_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let trigger = AutomationTriggerPayload {
                    version: 1,
                    action: AutomationTriggerAction::RunFlow,
                    target_id: flow_id,
                    variables: vars,
                    department_id,
                };
                let out = rusvel_api::automation::dispatch_automation_trigger(
                    app,
                    sid,
                    trigger,
                    None,
                )
                    .await
                    .map_err(|e| McpError::Engine(RusvelError::Validation(e)))?;
                serde_json::to_value(out).map_err(McpError::Json)?
            }
            "automation_run_playbook" => {
                let app = rusvel_api::automation::worker_app_state().ok_or_else(|| {
                    McpError::InvalidParams(
                        "automation tools need app state (HTTP API or MCP HTTP)".into(),
                    )
                })?;
                let sid = parse_session_id(args)?;
                let playbook_id = args["playbook_id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("playbook_id required".into()))?
                    .to_string();
                let vars = args
                    .get("variables")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let trigger = AutomationTriggerPayload {
                    version: 1,
                    action: AutomationTriggerAction::RunPlaybook,
                    target_id: playbook_id,
                    variables: vars,
                    department_id: None,
                };
                let out = rusvel_api::automation::dispatch_automation_trigger(
                    app,
                    sid,
                    trigger,
                    None,
                )
                    .await
                    .map_err(|e| McpError::Engine(RusvelError::Validation(e)))?;
                serde_json::to_value(out).map_err(McpError::Json)?
            }
            "automation_upsert_cron_schedule" => {
                let app = rusvel_api::automation::worker_app_state().ok_or_else(|| {
                    McpError::InvalidParams(
                        "automation tools need app state (HTTP API or MCP HTTP)".into(),
                    )
                })?;
                let sid = parse_session_id(args)?;
                let name = args["name"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("name required".into()))?
                    .to_string();
                let schedule = args["schedule"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("schedule required".into()))?
                    .to_string();
                let action_str = args["action"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("action required".into()))?;
                let action = match action_str {
                    "run_flow" => AutomationTriggerAction::RunFlow,
                    "run_playbook" => AutomationTriggerAction::RunPlaybook,
                    _ => {
                        return Err(McpError::InvalidParams(
                            "action must be run_flow or run_playbook".into(),
                        ));
                    }
                };
                let target_id = args["target_id"]
                    .as_str()
                    .ok_or_else(|| McpError::InvalidParams("target_id required".into()))?
                    .to_string();
                let vars = args
                    .get("variables")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let department_id = args
                    .get("department_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let enabled = args["enabled"].as_bool().unwrap_or(true);
                let schedule_id = args
                    .get("schedule_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let trigger = AutomationTriggerPayload {
                    version: 1,
                    action,
                    target_id,
                    variables: vars,
                    department_id,
                };
                let out = rusvel_api::automation::upsert_automation_cron_schedule(
                    app,
                    schedule_id,
                    name,
                    sid,
                    schedule,
                    trigger,
                    enabled,
                )
                .await?;
                serde_json::to_value(out).map_err(McpError::Json)?
            }
            "visual_inspect" => {
                let update = args["update_baselines"].as_bool().unwrap_or(false);
                let mut cmd_args = vec![
                    "exec",
                    "playwright",
                    "test",
                    "--project=visual",
                    "--reporter=json",
                ];
                if update {
                    cmd_args.push("--update-snapshots");
                }

                // Find project dir
                let project_dir = if std::path::Path::new("frontend").exists() {
                    "frontend".to_string()
                } else if std::path::Path::new("/Users/bm/rusvel/frontend").exists() {
                    "/Users/bm/rusvel/frontend".to_string()
                } else {
                    return Err(McpError::InvalidParams(
                        "Cannot find frontend directory".into(),
                    ));
                };

                match tokio::process::Command::new("pnpm")
                    .args(&cmd_args)
                    .current_dir(&project_dir)
                    .output()
                    .await
                {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        serde_json::json!({
                            "success": output.status.success(),
                            "stdout": stdout.chars().take(8000).collect::<String>(),
                            "stderr": stderr.chars().take(2000).collect::<String>(),
                        })
                    }
                    Err(e) => serde_json::json!({
                        "success": false,
                        "error": format!("Failed to run playwright: {e}"),
                    }),
                }
            }
            other => return Err(McpError::UnknownTool(other.into())),
        };

        Ok(serde_json::json!({
            "content": [{ "type": "text", "text": result.to_string() }]
        }))
    }
}

fn parse_session_id(args: &serde_json::Value) -> Result<SessionId, McpError> {
    let raw = args["session_id"]
        .as_str()
        .ok_or_else(|| McpError::InvalidParams("missing session_id".into()))?;
    let uuid: uuid::Uuid = raw
        .parse()
        .map_err(|e| McpError::InvalidParams(format!("invalid session_id: {e}")))?;
    Ok(SessionId::from_uuid(uuid))
}

// ════════════════════════════════════════════════════════════════════
//  stdio transport
// ════════════════════════════════════════════════════════════════════

/// Run the MCP server over stdin/stdout (JSON-RPC, newline-delimited).
pub async fn run_stdio(mcp: Arc<RusvelMcp>) -> Result<(), McpError> {
    let stdin = BufReader::new(tokio::io::stdin());
    let mut stdout = tokio::io::stdout();
    let mut lines = stdin.lines();

    while let Some(line) = lines.next_line().await? {
        let line = line.trim().to_string();
        if line.is_empty() {
            continue;
        }

        let req: jsonrpc::JsonRpcRequest = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = jsonrpc::JsonRpcResponse::error(
                    serde_json::Value::Null,
                    -32700,
                    format!("parse error: {e}"),
                );
                let mut out = serde_json::to_string(&resp)?;
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
                continue;
            }
        };

        match jsonrpc::dispatch(&mcp, req).await {
            Ok(None) => {}
            Ok(Some(resp)) => {
                let mut out = serde_json::to_string(&resp)?;
                out.push('\n');
                stdout.write_all(out.as_bytes()).await?;
                stdout.flush().await?;
            }
            Err(_) => {}
        }
    }

    Ok(())
}
