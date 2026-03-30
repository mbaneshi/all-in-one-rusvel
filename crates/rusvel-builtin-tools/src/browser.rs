//! Browser tools: `browser_observe`, `browser_search`, `browser_act` over [`BrowserPort`](rusvel_core::ports::BrowserPort).

use std::sync::Arc;

use rusvel_core::domain::{Content, ToolDefinition, ToolResult};
use rusvel_core::ports::BrowserPort;
use rusvel_tool::ToolRegistry;
use serde_json::json;

fn tab_id_for_platform(tabs: &[rusvel_core::domain::TabInfo], platform: &str) -> Option<String> {
    let p = platform.to_lowercase();
    tabs.iter().find_map(|t| {
        t.platform
            .as_deref()
            .map(|x| x.to_lowercase() == p)
            .unwrap_or(false)
            .then(|| t.id.clone())
    })
}

pub async fn register(registry: &ToolRegistry, browser: Arc<dyn BrowserPort>) {
    // ── browser_observe ─────────────────────────────────────────────
    let b = browser.clone();
    registry
        .register_with_handler(
            ToolDefinition {
                name: "browser_observe".into(),
                description: "Subscribe to observation events for a browser tab. Pass `platform` (e.g. upwork, linkedin) to pick the matching tab from the current session.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "platform": {
                            "type": "string",
                            "description": "Platform hint to select a tab (matches TabInfo.platform)"
                        }
                    },
                    "required": ["platform"]
                }),
                searchable: true,
                metadata: json!({"category": "browser"}),
            },
            Arc::new(move |args| {
                let b = b.clone();
                Box::pin(async move {
                    let platform = args["platform"].as_str().unwrap_or_default();
                    if platform.is_empty() {
                        return Err(rusvel_core::error::RusvelError::Tool(
                            "browser_observe: platform is required".into(),
                        ));
                    }
                    let tabs = b.tabs().await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("browser_observe: tabs: {e}"))
                    })?;
                    let tab_id = tab_id_for_platform(&tabs, platform).ok_or_else(|| {
                        rusvel_core::error::RusvelError::Tool(format!(
                            "browser_observe: no tab for platform {platform:?}"
                        ))
                    })?;
                    let _rx = b.observe(&tab_id).await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("browser_observe: {e}"))
                    })?;
                    Ok(ToolResult {
                        success: true,
                        output: Content::text(format!(
                            "Observing tab {tab_id} (platform hint: {platform}). Event stream subscribed."
                        )),
                        metadata: json!({"tab_id": tab_id, "platform": platform}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── browser_search ──────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "browser_search".into(),
                description: "Search indexed browser capture data. Returns structured matches; full-text capture indexing is not wired yet — results may be empty until CDP capture search lands.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "platform": {
                            "type": "string",
                            "description": "Optional platform filter"
                        }
                    },
                    "required": ["query"]
                }),
                searchable: true,
                metadata: json!({"category": "browser", "read_only": true}),
            },
            Arc::new(move |args| {
                Box::pin(async move {
                    let query = args["query"].as_str().unwrap_or_default();
                    let platform = args.get("platform").and_then(|v| v.as_str());
                    Ok(ToolResult {
                        success: true,
                        output: Content::text(
                            "No indexed captures matched (capture search pipeline not active).",
                        ),
                        metadata: json!({
                            "matches": [],
                            "query": query,
                            "platform": platform,
                            "note": "PASTE-30 stub: wire CDP capture index for real matches"
                        }),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── browser_act ─────────────────────────────────────────────────
    let b = browser.clone();
    registry
        .register_with_handler(
            ToolDefinition {
                name: "browser_act".into(),
                description: "Execute a browser action on a tab (navigate or evaluate_js). Set `confirmed` to true after human approval; otherwise the tool returns AWAITING_APPROVAL.".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "action": {
                            "type": "string",
                            "enum": ["navigate", "evaluate_js"],
                            "description": "Action to run"
                        },
                        "target": {
                            "type": "string",
                            "description": "URL for navigate, or JavaScript source for evaluate_js"
                        },
                        "tab_id": {
                            "type": "string",
                            "description": "CDP target id; if omitted, use `platform` to pick a tab"
                        },
                        "platform": {
                            "type": "string",
                            "description": "Platform hint when tab_id is omitted"
                        },
                        "confirmed": {
                            "type": "boolean",
                            "description": "Must be true to execute (human approval gate)"
                        }
                    },
                    "required": ["action", "target"]
                }),
                searchable: true,
                metadata: json!({"category": "browser", "requires_approval": true}),
            },
            Arc::new(move |args| {
                let b = b.clone();
                Box::pin(async move {
                    let action = args["action"].as_str().unwrap_or_default();
                    let target = args["target"].as_str().unwrap_or_default();
                    let confirmed = args.get("confirmed").and_then(|v| v.as_bool()) == Some(true);

                    if !confirmed {
                        return Ok(ToolResult {
                            success: true,
                            output: Content::text("AWAITING_APPROVAL"),
                            metadata: json!({
                                "status": "AWAITING_APPROVAL",
                                "action": action,
                                "target": target,
                                "message": "Set confirmed=true after approving this action in the UI."
                            }),
                        });
                    }

                    let tabs = b.tabs().await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("browser_act: tabs: {e}"))
                    })?;

                    let tab_id = if let Some(id) = args.get("tab_id").and_then(|v| v.as_str()) {
                        if id.is_empty() {
                            String::new()
                        } else {
                            id.to_string()
                        }
                    } else {
                        String::new()
                    };

                    let tab_id = if tab_id.is_empty() {
                        let platform = args.get("platform").and_then(|v| v.as_str()).unwrap_or("");
                        tab_id_for_platform(&tabs, platform).ok_or_else(|| {
                            rusvel_core::error::RusvelError::Tool(
                                "browser_act: provide tab_id or a valid platform".into(),
                            )
                        })?
                    } else {
                        tab_id
                    };

                    match action {
                        "navigate" => {
                            b.navigate(&tab_id, target).await.map_err(|e| {
                                rusvel_core::error::RusvelError::Tool(format!("browser_act: navigate: {e}"))
                            })?;
                            Ok(ToolResult {
                                success: true,
                                output: Content::text(format!("Navigated tab {tab_id} to {target}")),
                                metadata: json!({"tab_id": tab_id}),
                            })
                        }
                        "evaluate_js" => {
                            let v = b.evaluate_js(&tab_id, target).await.map_err(|e| {
                                rusvel_core::error::RusvelError::Tool(format!(
                                    "browser_act: evaluate_js: {e}"
                                ))
                            })?;
                            Ok(ToolResult {
                                success: true,
                                output: Content::text(v.to_string()),
                                metadata: json!({"tab_id": tab_id, "result": v}),
                            })
                        }
                        _ => Err(rusvel_core::error::RusvelError::Tool(format!(
                            "browser_act: unknown action {action:?}"
                        ))),
                    }
                })
            }),
        )
        .await
        .unwrap();
}
