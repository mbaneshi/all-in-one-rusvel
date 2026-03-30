//! Terminal multiplexer tools: open a pane and sample output from a pane.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use rusvel_core::domain::{Content, ToolDefinition, ToolResult};
use rusvel_core::id::{PaneId, SessionId};
use rusvel_core::ports::TerminalPort;
use rusvel_core::terminal::{PaneSize, PaneSource, WindowSource};
use rusvel_tool::ToolRegistry;
use serde_json::json;
use tokio::sync::broadcast::error::RecvError;
use uuid::Uuid;

pub async fn register(registry: &ToolRegistry, terminal: Option<Arc<dyn TerminalPort>>) {
    registry
        .register_with_handler(
            ToolDefinition {
                name: "terminal_open".into(),
                description: "Create a new terminal window with a shell pane and return its pane_id for WebSocket attach or observation."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Window label (default: agent-pane)"
                        },
                        "session_id": {
                            "type": "string",
                            "description": "Optional session UUID; random if omitted"
                        }
                    }
                }),
                searchable: true,
                metadata: json!({"category": "terminal"}),
            },
            Arc::new({
                let terminal = terminal.clone();
                move |args| {
                    let terminal = terminal.clone();
                    Box::pin(async move {
                        let Some(term) = terminal else {
                            return Ok(ToolResult {
                                success: false,
                                output: Content::text("terminal_open: TerminalPort not configured"),
                                metadata: json!({"error": "terminal_unavailable"}),
                            });
                        };

                        let name = args
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("agent-pane");

                        let session_id = args
                            .get("session_id")
                            .and_then(|v| v.as_str())
                            .and_then(|s| Uuid::parse_str(s.trim()).ok())
                            .map(SessionId::from_uuid)
                            .unwrap_or_else(SessionId::new);

                        let window_id = term
                            .create_window(&session_id, name, WindowSource::Manual)
                            .await
                            .map_err(|e| {
                                rusvel_core::error::RusvelError::Tool(format!(
                                    "terminal_open: create_window: {e}"
                                ))
                            })?;

                        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
                        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
                        let pane_id = term
                            .create_pane(
                                &window_id,
                                &shell,
                                &cwd,
                                PaneSize {
                                    rows: 24,
                                    cols: 80,
                                },
                                PaneSource::Shell,
                            )
                            .await
                            .map_err(|e| {
                                rusvel_core::error::RusvelError::Tool(format!(
                                    "terminal_open: create_pane: {e}"
                                ))
                            })?;

                        Ok(ToolResult {
                            success: true,
                            output: Content::text(pane_id.to_string()),
                            metadata: json!({
                                "pane_id": pane_id.to_string(),
                                "window_id": window_id.to_string(),
                                "session_id": session_id.to_string(),
                            }),
                        })
                    })
                }
            }),
        )
        .await
        .unwrap();

    registry
        .register_with_handler(
            ToolDefinition {
                name: "terminal_watch".into(),
                description: "Subscribe to an existing pane's output broadcast and return a short sample (UTF-8 lossy) for agent observation. For full streaming, use the web UI WebSocket /api/terminal/ws?pane_id=…"
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pane_id": {
                            "type": "string",
                            "description": "Pane UUID from terminal_open or delegation metadata"
                        },
                        "duration_ms": {
                            "type": "integer",
                            "description": "How long to collect output (default 2000, max 10000)"
                        }
                    },
                    "required": ["pane_id"]
                }),
                searchable: true,
                metadata: json!({"category": "terminal"}),
            },
            Arc::new({
                let terminal = terminal.clone();
                move |args| {
                    let terminal = terminal.clone();
                    Box::pin(async move {
                        let Some(term) = terminal else {
                            return Ok(ToolResult {
                                success: false,
                                output: Content::text("terminal_watch: TerminalPort not configured"),
                                metadata: json!({"error": "terminal_unavailable"}),
                            });
                        };

                        let pane_str = args["pane_id"].as_str().unwrap_or_default().trim();
                        let uuid = match Uuid::parse_str(pane_str) {
                            Ok(u) => u,
                            Err(_) => {
                                return Ok(ToolResult {
                                    success: false,
                                    output: Content::text("terminal_watch: invalid pane_id"),
                                    metadata: json!({"error": "invalid_pane_id"}),
                                });
                            }
                        };
                        let pane_id = PaneId::from_uuid(uuid);

                        let duration_ms = args
                            .get("duration_ms")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(2000)
                            .min(10_000);

                        let mut rx = match term.subscribe_pane(&pane_id).await {
                            Ok(r) => r,
                            Err(e) => {
                                return Ok(ToolResult {
                                    success: false,
                                    output: Content::text(format!("terminal_watch: subscribe failed: {e}")),
                                    metadata: json!({"error": "subscribe_failed"}),
                                });
                            }
                        };

                        let mut buf = Vec::<u8>::new();
                        let deadline =
                            tokio::time::Instant::now() + Duration::from_millis(duration_ms);

                        while tokio::time::Instant::now() < deadline {
                            let remaining = deadline.saturating_duration_since(
                                tokio::time::Instant::now(),
                            );
                            if remaining.is_zero() {
                                break;
                            }
                            match tokio::time::timeout(remaining, rx.recv()).await {
                                Ok(Ok(data)) => buf.extend_from_slice(&data),
                                Ok(Err(RecvError::Lagged(_))) => {}
                                Ok(Err(RecvError::Closed)) => break,
                                Err(_) => break,
                            }
                        }

                        let sample = String::from_utf8_lossy(&buf);
                        let truncated = sample.chars().take(8000).collect::<String>();

                        Ok(ToolResult {
                            success: true,
                            output: Content::text(truncated),
                            metadata: json!({
                                "pane_id": pane_id.to_string(),
                                "bytes_collected": buf.len(),
                                "duration_ms": duration_ms,
                            }),
                        })
                    })
                }
            }),
        )
        .await
        .unwrap();
}
