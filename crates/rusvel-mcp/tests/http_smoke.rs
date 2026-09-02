//! MCP HTTP: JSON-RPC POST through nested `/m` router (no network bind).

use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use rusvel_agent::AgentRuntime;
use rusvel_config::TomlConfig;
use rusvel_core::domain::{
    Content, FinishReason, LlmRequest, LlmResponse, LlmUsage, Session, ToolDefinition, ToolResult,
};
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::{
    AgentPort, ConfigPort, EventPort, JobPort, LlmPort, MemoryPort, SessionPort, StoragePort,
    ToolPort,
};
use rusvel_db::Database;
use rusvel_event::EventBus;
use rusvel_mcp::RusvelMcp;
use rusvel_mcp::http::{McpAuth, nest_mcp_http};
use rusvel_memory::MemoryStore;
use serde_json::json;
use tempfile::tempdir;
use tower::ServiceExt;

struct SessionAdapter(Arc<dyn StoragePort>);

#[async_trait]
impl SessionPort for SessionAdapter {
    async fn create(&self, session: Session) -> Result<SessionId> {
        let id = session.id;
        self.0.sessions().put_session(&session).await?;
        Ok(id)
    }
    async fn load(&self, id: &SessionId) -> Result<Session> {
        self.0.sessions().get_session(id).await?.ok_or_else(|| {
            rusvel_core::error::RusvelError::NotFound {
                kind: "session".into(),
                id: id.to_string(),
            }
        })
    }
    async fn save(&self, session: &Session) -> Result<()> {
        self.0.sessions().put_session(session).await
    }
    async fn list(&self) -> Result<Vec<rusvel_core::domain::SessionSummary>> {
        self.0.sessions().list_sessions().await
    }
}

struct StubLlm;

#[async_trait]
impl LlmPort for StubLlm {
    async fn generate(&self, _: LlmRequest) -> Result<LlmResponse> {
        Ok(LlmResponse {
            content: Content::text("stub"),
            finish_reason: FinishReason::Stop,
            usage: LlmUsage::default(),
            metadata: json!({}),
        })
    }
    async fn embed(&self, _: &rusvel_core::domain::ModelRef, _: &str) -> Result<Vec<f32>> {
        Ok(vec![])
    }
    async fn list_models(&self) -> Result<Vec<rusvel_core::domain::ModelRef>> {
        Ok(vec![])
    }
}

struct StubTool;

#[async_trait]
impl ToolPort for StubTool {
    async fn register(&self, _: ToolDefinition) -> Result<()> {
        Ok(())
    }
    async fn call(&self, _: &str, _: serde_json::Value) -> Result<ToolResult> {
        Ok(ToolResult {
            success: true,
            output: Content::text("ok"),
            metadata: json!({}),
        })
    }
    fn list(&self) -> Vec<ToolDefinition> {
        vec![]
    }
    fn search(&self, _: &str, _: usize) -> Vec<ToolDefinition> {
        vec![]
    }
    fn schema(&self, _: &str) -> Option<serde_json::Value> {
        None
    }
}

async fn build_rusvel_mcp() -> (Arc<RusvelMcp>, tempfile::TempDir) {
    build_rusvel_mcp_with_tools(Arc::new(StubTool)).await
}

async fn build_rusvel_mcp_with_tools(
    tools: Arc<dyn ToolPort>,
) -> (Arc<RusvelMcp>, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let base = dir.path();
    let db: Arc<Database> = Arc::new(Database::open(base.join("rusvel.db")).unwrap());
    let config: Arc<dyn ConfigPort> = Arc::new(TomlConfig::load(base.join("config.toml")).unwrap());
    let events: Arc<dyn EventPort> = Arc::new(EventBus::new(
        db.clone() as Arc<dyn rusvel_core::ports::EventStore>
    ));
    let memory: Arc<dyn MemoryPort> =
        Arc::new(MemoryStore::open(base.join("memory.db").to_str().unwrap()).unwrap());
    let jobs: Arc<dyn JobPort> = db.clone() as Arc<dyn JobPort>;
    let storage: Arc<dyn StoragePort> = db.clone();
    let sessions: Arc<dyn SessionPort> = Arc::new(SessionAdapter(storage.clone()));
    let agent_runtime = Arc::new(AgentRuntime::new(
        Arc::new(StubLlm),
        tools.clone(),
        memory.clone(),
    ));

    let forge = Arc::new(forge_engine::ForgeEngine::new(
        agent_runtime.clone() as Arc<dyn AgentPort>,
        events,
        memory.clone(),
        storage,
        jobs,
        sessions.clone(),
        config,
    ));

    let mcp = Arc::new(RusvelMcp::new(forge, sessions, tools));
    (mcp, dir)
}

async fn mcp_router() -> (Router, tempfile::TempDir) {
    let (mcp, dir) = build_rusvel_mcp().await;
    (nest_mcp_http(Router::new(), mcp, McpAuth::default()), dir)
}

#[tokio::test]
async fn post_initialize_returns_protocol_json() {
    let (app, _guard) = mcp_router().await;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {}
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["jsonrpc"], "2.0");
    assert!(v["result"]["protocolVersion"].as_str().is_some());
}

#[tokio::test]
async fn post_tools_list_includes_session_list() {
    let (app, _guard) = mcp_router().await;
    let body = json!({
        "jsonrpc": "2.0",
        "id": 2,
        "method": "tools/list",
        "params": {}
    });
    let req = Request::builder()
        .method("POST")
        .uri("/mcp")
        .header("content-type", "application/json")
        .body(Body::from(serde_json::to_vec(&body).unwrap()))
        .unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bytes = to_bytes(res.into_body(), usize::MAX).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let tools = v["result"]["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"session_list"));
}

/// Same `handle_method` path as stdio JSON-RPC (initialize + tools/list).
#[tokio::test]
async fn handle_method_initialize_and_tools_list_stdio_path() {
    let (mcp, _guard) = build_rusvel_mcp().await;
    let init = mcp
        .handle_method("initialize", serde_json::json!({}))
        .await
        .expect("initialize");
    assert_eq!(init["serverInfo"]["name"], "rusvel-mcp");
    let listed = mcp
        .handle_method("tools/list", serde_json::json!({}))
        .await
        .expect("tools/list");
    let tools = listed["tools"].as_array().expect("tools array");
    assert!(!tools.is_empty());
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert!(names.contains(&"session_list"));
}

/// Issue #13 (MCP-first): tools registered on the shared [`ToolPort`] — the
/// same registry `rusvel_tool::register_department_tools` bridges every
/// department's `ToolRegistrar` tools into — must surface in `tools/list`
/// alongside the hand-written MCP-native tools.
#[tokio::test]
async fn tools_list_includes_toolport_registered_tools() {
    let registry = rusvel_tool::ToolRegistry::new();
    registry
        .register_with_handler(
            rusvel_core::domain::ToolDefinition {
                name: "harvest.scan".into(),
                description: "Scan for opportunities".into(),
                parameters: json!({"type": "object", "properties": {}}),
                searchable: false,
                metadata: json!({"department_id": "harvest"}),
            },
            Arc::new(|_args| {
                Box::pin(async move {
                    Ok(ToolResult {
                        success: true,
                        output: Content::text("3 opportunities found"),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();
    let tools: Arc<dyn ToolPort> = Arc::new(registry);

    let (mcp, _guard) = build_rusvel_mcp_with_tools(tools).await;

    let listed = mcp
        .handle_method("tools/list", serde_json::json!({}))
        .await
        .expect("tools/list");
    let tools = listed["tools"].as_array().expect("tools array");
    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    // native MCP tool still present alongside the bridged department tool
    assert!(names.contains(&"session_list"));
    assert!(names.contains(&"harvest.scan"));
}

/// A `tools/call` for a name not handled natively falls through to the
/// [`ToolPort`] bridge, and the result is wrapped in the MCP content shape.
#[tokio::test]
async fn tools_call_dispatches_unknown_native_names_to_toolport() {
    let registry = rusvel_tool::ToolRegistry::new();
    registry
        .register_with_handler(
            rusvel_core::domain::ToolDefinition {
                name: "content.draft".into(),
                description: "Draft a content item".into(),
                parameters: json!({
                    "type": "object",
                    "properties": { "topic": { "type": "string" } },
                    "required": ["topic"]
                }),
                searchable: false,
                metadata: json!({"department_id": "content"}),
            },
            Arc::new(|args: serde_json::Value| {
                Box::pin(async move {
                    let topic = args["topic"].as_str().unwrap_or("(none)").to_string();
                    Ok(ToolResult {
                        success: true,
                        output: Content::text(format!("draft about {topic}")),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();
    let tools: Arc<dyn ToolPort> = Arc::new(registry);

    let (mcp, _guard) = build_rusvel_mcp_with_tools(tools).await;

    let out = mcp
        .handle_method(
            "tools/call",
            json!({ "name": "content.draft", "arguments": { "topic": "rust" } }),
        )
        .await
        .expect("tools/call");
    assert_eq!(out["content"][0]["text"], "draft about rust");
    assert_eq!(out["isError"], false);
}

/// A name unknown to both the native tools and the `ToolPort` registry is
/// still rejected — the bridge must not silently swallow unknown-tool errors.
#[tokio::test]
async fn tools_call_unknown_name_still_errors() {
    let (mcp, _guard) = build_rusvel_mcp().await;
    let err = mcp
        .handle_method(
            "tools/call",
            json!({ "name": "totally.unregistered", "arguments": {} }),
        )
        .await;
    assert!(err.is_err());
}
