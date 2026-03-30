//! MCP over HTTP: JSON-RPC POST + SSE for server push / keep-alive.
//!
//! Routes are nested at `/mcp` (POST `/mcp`, GET `/mcp/sse`) via [`nest_mcp_http`].

use std::collections::HashSet;
use std::convert::Infallible;
use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::Router;
use axum::body::Body;
use axum::extract::State;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use futures::Stream;
use futures::stream;

use tokio::sync::RwLock;

use crate::RusvelMcp;
use crate::jsonrpc::{self, JsonRpcResponse};

/// OAuth / bearer placeholder for streamable HTTP MCP.
#[derive(Debug, Clone, Default)]
pub struct McpAuth {
    pub enabled: bool,
    pub token: Option<String>,
}

impl McpAuth {
    /// Reads `RUSVEL_MCP_HTTP_AUTH` (true/1 enables) and `RUSVEL_MCP_HTTP_TOKEN` (bearer secret).
    pub fn from_env() -> Self {
        let enabled = std::env::var("RUSVEL_MCP_HTTP_AUTH")
            .ok()
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(false);
        let token = std::env::var("RUSVEL_MCP_HTTP_TOKEN")
            .ok()
            .filter(|s| !s.is_empty());
        Self { enabled, token }
    }
}

pub const MCP_SESSION_HEADER: &str = "mcp-session-id";

/// Shared state for HTTP MCP (nested router).
#[derive(Clone)]
pub struct McpHttpState {
    pub mcp: Arc<RusvelMcp>,
    pub auth: McpAuth,
    sessions: Arc<RwLock<HashSet<String>>>,
}

impl McpHttpState {
    pub fn new(mcp: Arc<RusvelMcp>, auth: McpAuth) -> Arc<Self> {
        Arc::new(Self {
            mcp,
            auth,
            sessions: Arc::new(RwLock::new(HashSet::new())),
        })
    }
}

/// Nest MCP HTTP under `/mcp` on the main Axum router (separate inner state).
pub fn nest_mcp_http<S>(router: Router<S>, mcp: Arc<RusvelMcp>, auth: McpAuth) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let state = McpHttpState::new(mcp, auth);
    let inner = Router::new()
        .route("/", post(handle_mcp_post))
        .route("/sse", get(handle_mcp_sse))
        .layer(from_fn_with_state(state.clone(), mcp_auth_middleware))
        .with_state(state);
    router.nest_service("/mcp", inner)
}

async fn mcp_auth_middleware(
    State(state): State<Arc<McpHttpState>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if state.auth.enabled {
        let Some(expected) = state.auth.token.as_ref() else {
            return Err(StatusCode::UNAUTHORIZED);
        };
        let auth = req
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;
        const PREFIX: &str = "Bearer ";
        if !auth.starts_with(PREFIX) || auth[PREFIX.len()..] != *expected {
            return Err(StatusCode::UNAUTHORIZED);
        }
    }
    Ok(next.run(req).await)
}

async fn handle_mcp_post(
    State(state): State<Arc<McpHttpState>>,
    headers: HeaderMap,
    Json(req): Json<jsonrpc::JsonRpcRequest>,
) -> Response {
    let rpc_id = req.id.clone();
    let session_id = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());

    {
        let mut s = state.sessions.write().await;
        s.insert(session_id.clone());
    }

    match jsonrpc::dispatch(state.mcp.as_ref(), req).await {
        Ok(None) => empty_with_session(&session_id),
        Ok(Some(resp)) => json_response(StatusCode::OK, &session_id, resp),
        Err(e) => {
            let resp = JsonRpcResponse::error(rpc_id, -32603, e.to_string());
            json_response(StatusCode::OK, &session_id, resp)
        }
    }
}

fn json_response(status: StatusCode, session_id: &str, resp: JsonRpcResponse) -> Response {
    let body = match serde_json::to_vec(&resp) {
        Ok(b) => b,
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };
    let mut res = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    if let Ok(val) = HeaderValue::from_str(session_id) {
        res.headers_mut()
            .insert(HeaderName::from_static("mcp-session-id"), val);
    }
    res
}

fn empty_with_session(session_id: &str) -> Response {
    let mut res = Response::builder()
        .status(StatusCode::NO_CONTENT)
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response());
    if let Ok(val) = HeaderValue::from_str(session_id) {
        res.headers_mut()
            .insert(HeaderName::from_static("mcp-session-id"), val);
    }
    res
}

async fn handle_mcp_sse(
    State(state): State<Arc<McpHttpState>>,
    headers: HeaderMap,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>> + Send>, StatusCode> {
    let session_id = headers
        .get(MCP_SESSION_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .ok_or(StatusCode::BAD_REQUEST)?;

    {
        let s = state.sessions.read().await;
        if !s.contains(&session_id) {
            return Err(StatusCode::NOT_FOUND);
        }
    }

    let stream = stream::empty::<Result<Event, Infallible>>();
    Ok(Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    ))
}
