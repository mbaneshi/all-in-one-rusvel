//! WebSocket bridge for the PTY-backed terminal.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use axum::Json;
use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, Query, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use futures::stream::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use uuid::Uuid;

use rusvel_core::id::{PaneId, RunId, SessionId, WindowId};
use rusvel_core::terminal::{Layout, PaneSize, PaneSource, WindowSource};

use crate::AppState;

#[derive(Debug, Deserialize)]
pub struct TerminalSessionQuery {
    pub session_id: String,
}

/// GET /api/terminal/session/snapshot?session_id=… — windows + panes for restore (in-process state).
pub async fn terminal_session_snapshot(
    Query(q): Query<TerminalSessionQuery>,
    State(state): State<std::sync::Arc<AppState>>,
) -> impl IntoResponse {
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Terminal not configured" })),
            )
                .into_response();
        }
    };

    let session_uuid = match Uuid::parse_str(q.session_id.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid session_id" })),
            )
                .into_response();
        }
    };
    let session_id = SessionId::from_uuid(session_uuid);

    let windows = match terminal.list_windows(&session_id).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("list_windows: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to list windows" })),
            )
                .into_response();
        }
    };

    let panes = match terminal.list_panes_for_session(&session_id).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("list_panes_for_session: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to list panes" })),
            )
                .into_response();
        }
    };

    Json(serde_json::json!({
        "windows": windows,
        "panes": panes,
    }))
    .into_response()
}

#[derive(Clone, Copy)]
struct DeptPaneEntry {
    window_id: WindowId,
    pane_id: PaneId,
}

fn dept_pane_cache() -> &'static Mutex<HashMap<(SessionId, String), DeptPaneEntry>> {
    static CACHE: OnceLock<Mutex<HashMap<(SessionId, String), DeptPaneEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

async fn session_owns_window(
    terminal: &std::sync::Arc<dyn rusvel_core::ports::TerminalPort>,
    session_id: &SessionId,
    window_id: &WindowId,
) -> bool {
    match terminal.list_windows(session_id).await {
        Ok(ws) => ws.iter().any(|w| w.id == *window_id),
        Err(_) => false,
    }
}

#[derive(Debug, Deserialize)]
pub struct TerminalDeptQuery {
    pub session_id: String,
}

/// GET /api/terminal/dept/:dept_id?session_id=… — get or create a PTY pane for this department.
pub async fn terminal_dept_pane(
    Path(dept_id): Path<String>,
    Query(q): Query<TerminalDeptQuery>,
    State(state): State<std::sync::Arc<AppState>>,
) -> impl IntoResponse {
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Terminal not configured" })),
            )
                .into_response();
        }
    };

    let session_uuid = match Uuid::parse_str(q.session_id.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid session_id" })),
            )
                .into_response();
        }
    };
    let session_id = SessionId::from_uuid(session_uuid);

    let key = (session_id, dept_id.clone());
    {
        let guard = dept_pane_cache().lock().unwrap();
        if let Some(ent) = guard.get(&key) {
            return Json(serde_json::json!({
                "pane_id": ent.pane_id.to_string(),
                "window_id": ent.window_id.to_string(),
            }))
            .into_response();
        }
    }

    let window_id = match terminal
        .create_window(
            &session_id,
            &format!("dept-{dept_id}"),
            WindowSource::Department(dept_id.clone()),
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to create terminal window: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to create window" })),
            )
                .into_response();
        }
    };

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let size = PaneSize { rows: 24, cols: 80 };

    let pane_id = match terminal
        .create_pane(
            &window_id,
            &shell,
            &cwd,
            size,
            PaneSource::Department(dept_id.clone()),
            None,
        )
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("Failed to create terminal pane: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to create pane" })),
            )
                .into_response();
        }
    };

    let mut guard = dept_pane_cache().lock().unwrap();
    guard.insert(
        key,
        DeptPaneEntry {
            window_id,
            pane_id,
        },
    );

    Json(serde_json::json!({
        "pane_id": pane_id.to_string(),
        "window_id": window_id.to_string(),
    }))
    .into_response()
}

#[derive(Debug, Deserialize)]
pub struct TerminalCreatePaneBody {
    #[serde(default)]
    pub cmd: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default = "default_term_rows")]
    pub rows: u16,
    #[serde(default = "default_term_cols")]
    pub cols: u16,
}

fn default_term_rows() -> u16 {
    24
}

fn default_term_cols() -> u16 {
    80
}

/// POST /api/terminal/window/:window_id/pane?session_id=… — add a PTY pane to an existing window.
pub async fn terminal_window_add_pane(
    Path(window_id_str): Path<String>,
    Query(q): Query<TerminalDeptQuery>,
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<TerminalCreatePaneBody>,
) -> impl IntoResponse {
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Terminal not configured" })),
            )
                .into_response();
        }
    };

    let session_uuid = match Uuid::parse_str(q.session_id.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid session_id" })),
            )
                .into_response();
        }
    };
    let session_id = SessionId::from_uuid(session_uuid);

    let win_uuid = match Uuid::parse_str(window_id_str.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid window_id" })),
            )
                .into_response();
        }
    };
    let window_id = WindowId::from_uuid(win_uuid);

    if !session_owns_window(&terminal, &session_id, &window_id).await {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "window not found for session" })),
        )
            .into_response();
    }

    let windows = match terminal.list_windows(&session_id).await {
        Ok(w) => w,
        Err(e) => {
            tracing::error!("list_windows: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to list windows" })),
            )
                .into_response();
        }
    };
    let window = match windows.iter().find(|w| w.id == window_id) {
        Some(w) => w,
        None => {
            return (
                axum::http::StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "window not found" })),
            )
                .into_response();
        }
    };

    let pane_source = if let Some(ref d) = window.dept_id {
        PaneSource::Department(d.clone())
    } else {
        PaneSource::Shell
    };

    let shell = body
        .cmd
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()));
    let cwd = body
        .cwd
        .as_ref()
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
    let size = PaneSize {
        rows: body.rows.max(1),
        cols: body.cols.max(1),
    };

    let pane_id = match terminal
        .create_pane(&window_id, &shell, &cwd, size, pane_source, None)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("create_pane: {e}");
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to create pane" })),
            )
                .into_response();
        }
    };

    Json(serde_json::json!({
        "pane_id": pane_id.to_string(),
        "window_id": window_id.to_string(),
    }))
    .into_response()
}

/// POST /api/terminal/window/:window_id/layout?session_id=… — set multiplexer layout for a window.
pub async fn terminal_window_set_layout(
    Path(window_id_str): Path<String>,
    Query(q): Query<TerminalDeptQuery>,
    State(state): State<std::sync::Arc<AppState>>,
    Json(layout): Json<Layout>,
) -> impl IntoResponse {
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Terminal not configured" })),
            )
                .into_response();
        }
    };

    let session_uuid = match Uuid::parse_str(q.session_id.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid session_id" })),
            )
                .into_response();
        }
    };
    let session_id = SessionId::from_uuid(session_uuid);

    let win_uuid = match Uuid::parse_str(window_id_str.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid window_id" })),
            )
                .into_response();
        }
    };
    let window_id = WindowId::from_uuid(win_uuid);

    if !session_owns_window(&terminal, &session_id, &window_id).await {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "window not found for session" })),
        )
            .into_response();
    }

    match terminal.set_layout(&window_id, layout).await {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::error!("set_layout: {e}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to set layout" })),
            )
                .into_response()
        }
    }
}

/// GET /api/terminal/runs/:run_id/panes — panes indexed to this agent run (delegation child run, etc.).
pub async fn terminal_run_panes(
    Path(run_id_str): Path<String>,
    State(state): State<std::sync::Arc<AppState>>,
) -> impl IntoResponse {
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({ "error": "Terminal not configured" })),
            )
                .into_response();
        }
    };

    let run_uuid = match Uuid::parse_str(run_id_str.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "invalid run_id" })),
            )
                .into_response();
        }
    };
    let run_id = RunId::from_uuid(run_uuid);

    match terminal.panes_for_run(&run_id).await {
        Ok(panes) => Json(serde_json::json!({ "panes": panes })).into_response(),
        Err(e) => {
            tracing::error!("panes_for_run: {e}");
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "failed to list panes" })),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TerminalResizeBody {
    pub rows: u16,
    pub cols: u16,
}

#[derive(Serialize)]
struct ResizeError<'a> {
    error: &'a str,
}

/// POST /api/terminal/pane/:pane_id/resize — sync PTY to xterm dimensions (cols/rows).
pub async fn terminal_resize_pane(
    Path(pane_id_str): Path<String>,
    State(state): State<std::sync::Arc<AppState>>,
    Json(body): Json<TerminalResizeBody>,
) -> impl IntoResponse {
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            return (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(ResizeError {
                    error: "Terminal not configured",
                }),
            )
                .into_response();
        }
    };

    let uuid = match Uuid::parse_str(pane_id_str.trim()) {
        Ok(u) => u,
        Err(_) => {
            return (
                axum::http::StatusCode::BAD_REQUEST,
                Json(ResizeError {
                    error: "invalid pane_id",
                }),
            )
                .into_response();
        }
    };
    let pane_id = PaneId::from_uuid(uuid);

    if body.rows == 0 || body.cols == 0 {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(ResizeError {
                error: "rows and cols must be positive",
            }),
        )
            .into_response();
    }

    let size = PaneSize {
        rows: body.rows,
        cols: body.cols,
    };

    match terminal.resize_pane(&pane_id, size).await {
        Ok(()) => axum::http::StatusCode::NO_CONTENT.into_response(),
        Err(e) => {
            tracing::debug!("resize_pane failed: {e}");
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(ResizeError {
                    error: "resize failed",
                }),
            )
                .into_response()
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct TerminalWsQuery {
    pub pane_id: Option<String>,
}

/// GET /api/terminal/ws — upgrade to WebSocket, spawn a PTY pane or attach to `pane_id`, bridge I/O.
pub async fn terminal_ws(
    ws: WebSocketUpgrade,
    Query(q): Query<TerminalWsQuery>,
    State(state): State<std::sync::Arc<AppState>>,
) -> impl IntoResponse {
    let pane_id = q.pane_id;
    ws.on_upgrade(move |socket| handle_ws(socket, state, pane_id))
}

async fn handle_ws(
    socket: WebSocket,
    state: std::sync::Arc<AppState>,
    existing_pane: Option<String>,
) {
    let owns_pane = existing_pane.is_none();
    let terminal = match state.terminal.as_ref() {
        Some(t) => t.clone(),
        None => {
            tracing::warn!("Terminal WebSocket requested but TerminalPort not configured");
            return;
        }
    };

    let pane_id = if let Some(pane_str) = existing_pane {
        let uuid = match Uuid::parse_str(pane_str.trim()) {
            Ok(u) => u,
            Err(_) => {
                tracing::warn!("Invalid pane_id in WebSocket query");
                return;
            }
        };
        PaneId::from_uuid(uuid)
    } else {
        // Create a session-scoped window + pane for this WebSocket connection.
        let session_id = SessionId::new();
        let window_id = match terminal
            .create_window(&session_id, "ws-terminal", WindowSource::Manual)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to create terminal window: {e}");
                return;
            }
        };

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let size = PaneSize { rows: 24, cols: 80 };

        match terminal
            .create_pane(&window_id, &shell, &cwd, size, PaneSource::Shell, None)
            .await
        {
            Ok(id) => id,
            Err(e) => {
                tracing::error!("Failed to create terminal pane: {e}");
                return;
            }
        }
    };

    let scrollback = terminal
        .pane_scrollback(&pane_id)
        .await
        .unwrap_or_default();

    let mut rx: broadcast::Receiver<Vec<u8>> = match terminal.subscribe_pane(&pane_id).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to subscribe to pane output: {e}");
            return;
        }
    };

    let (mut ws_tx, mut ws_rx) = socket.split();

    // PTY output -> WebSocket (replay bounded scrollback first so prompt is visible)
    let terminal_write = terminal.clone();
    let pane_for_close = pane_id;
    let pty_to_ws = tokio::spawn(async move {
        use futures::SinkExt;
        const CHUNK: usize = 16 * 1024;
        for chunk in scrollback.chunks(CHUNK) {
            if ws_tx
                .send(Message::Binary(chunk.to_vec().into()))
                .await
                .is_err()
            {
                return;
            }
        }
        loop {
            match rx.recv().await {
                Ok(data) => {
                    if ws_tx.send(Message::Binary(data.into())).await.is_err() {
                        break;
                    }
                }
                Err(broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!("Terminal WS lagged {n} messages");
                }
                Err(broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    // WebSocket input -> PTY
    let terminal_input = terminal.clone();
    let pane_for_input = pane_id;
    let ws_to_pty = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_rx.next().await {
            match msg {
                Message::Text(text) => {
                    if let Err(e) = terminal_input
                        .write_pane(&pane_for_input, text.as_bytes())
                        .await
                    {
                        tracing::debug!("write_pane error: {e}");
                        break;
                    }
                }
                Message::Binary(data) => {
                    if let Err(e) = terminal_input.write_pane(&pane_for_input, &data).await {
                        tracing::debug!("write_pane error: {e}");
                        break;
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
    });

    // Wait for either direction to finish, then clean up.
    tokio::select! {
        _ = pty_to_ws => {},
        _ = ws_to_pty => {},
    }

    if owns_pane {
        let _ = terminal_write.close_pane(&pane_for_close).await;
    }
    tracing::debug!(
        "Terminal WebSocket session closed (pane {pane_for_close}, owns_pane={owns_pane})"
    );
}
