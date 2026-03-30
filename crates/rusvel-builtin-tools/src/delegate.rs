//! Delegate agent tool: spawn a sub-agent to handle a task.

use std::path::PathBuf;
use std::sync::Arc;

use rusvel_agent::{AgentEvent, AgentRuntime};
use rusvel_core::SessionId;
use rusvel_core::domain::{AgentConfig, AgentOutput, Content, Part, ToolDefinition, ToolResult};
use rusvel_core::id::{PaneId, RunId};
use rusvel_core::ports::{AgentPort, TerminalPort};
use rusvel_core::terminal::{PaneSize, PaneSource, WindowSource};
use rusvel_tool::ToolRegistry;
use serde_json::json;
use uuid::Uuid;

const MAX_DELEGATION_DEPTH: u64 = 3;
const IDLE_PANE_CMD: &str = "sleep 86400";

pub async fn register(
    registry: &ToolRegistry,
    agent: Arc<AgentRuntime>,
    terminal: Option<Arc<dyn TerminalPort>>,
) {
    registry
        .register_with_handler(
            ToolDefinition {
                name: "delegate_agent".into(),
                description: "Delegate a task to a sub-agent. The sub-agent runs with its own \
                    persona, tools, and model tier, then returns its output text. When a terminal \
                    is available, execution is mirrored to a delegation pane for live visibility."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "prompt": {
                            "type": "string",
                            "description": "The task prompt for the delegated agent"
                        },
                        "persona": {
                            "type": "string",
                            "description": "Optional persona name (e.g. 'CodeWriter', 'Researcher')"
                        },
                        "tools": {
                            "type": "array",
                            "items": { "type": "string" },
                            "description": "Tool names the sub-agent may use"
                        },
                        "max_tokens": {
                            "type": "integer",
                            "description": "Maximum tokens for the sub-agent response"
                        },
                        "model_tier": {
                            "type": "string",
                            "enum": ["fast", "balanced", "powerful"],
                            "description": "Model tier: fast, balanced, or powerful"
                        },
                        "delegation_depth": {
                            "type": "integer",
                            "description": "Current delegation depth (set automatically, do not override)"
                        },
                        "parent_run_id": {
                            "type": "string",
                            "description": "Optional orchestrator run UUID for delegation-chain grouping"
                        }
                    },
                    "required": ["prompt"]
                }),
                searchable: true,
        metadata: json!({"category": "agent", "max_depth": MAX_DELEGATION_DEPTH}),
            },
            Arc::new(move |args| {
                let agent = agent.clone();
                let terminal = terminal.clone();
                Box::pin(async move {
                    let depth = args
                        .get("delegation_depth")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    if depth >= MAX_DELEGATION_DEPTH {
                        return Ok(ToolResult {
                            success: false,
                            output: Content::text(format!(
                                "Delegation rejected: depth {depth} >= max {MAX_DELEGATION_DEPTH}"
                            )),
        metadata: json!({"error": "max_delegation_depth_exceeded", "depth": depth}),
                        });
                    }

                    let prompt = args["prompt"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();

                    if prompt.is_empty() {
                        return Ok(ToolResult {
                            success: false,
                            output: Content::text("delegate_agent: prompt is required"),
        metadata: json!({"error": "missing_prompt"}),
                        });
                    }

                    let tools: Vec<String> = args
                        .get("tools")
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(String::from))
                                .collect()
                        })
                        .unwrap_or_default();

                    let persona = args
                        .get("persona")
                        .and_then(|v| v.as_str())
                        .map(String::from);

                    let model_tier = args
                        .get("model_tier")
                        .and_then(|v| v.as_str())
                        .unwrap_or("balanced");

                    let max_tokens = args
                        .get("max_tokens")
                        .and_then(|v| v.as_u64());

                    let parent_run_id_opt = args
                        .get("parent_run_id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| Uuid::parse_str(s.trim()).ok())
                        .map(RunId::from_uuid);

                    let instructions = if let Some(ref p) = persona {
                        format!("You are acting as the '{p}' persona. {prompt}")
                    } else {
                        prompt.clone()
                    };

                    let mut metadata = json!({
                        "delegation_depth": depth + 1,
                        "model_tier": model_tier,
                    });
                    if let Some(ref p) = persona {
                        metadata["persona"] = json!(p);
                    }
                    if let Some(mt) = max_tokens {
                        metadata["max_tokens"] = json!(mt);
                    }

                    let config = AgentConfig {
                        profile_id: None,
                        session_id: SessionId::new(),
                        model: None,
                        tools,
                        instructions: Some(instructions),
                        budget_limit: None,
                        max_iterations: None,
                        permission_mode: Default::default(),
                        metadata,
                    };

                    let child_run_id = agent.create(config).await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!(
                            "delegate_agent: failed to create agent: {e}"
                        ))
                    })?;

                    let persona_label = persona.clone().unwrap_or_else(|| "delegate".to_string());

                    let parent_for_pane = parent_run_id_opt.unwrap_or(child_run_id);

                    let mut pane_id_opt = None;
                    if let Some(ref term) = terminal {
                        let session_id = SessionId::new();
                        let window_source = if parent_run_id_opt.is_some() {
                            WindowSource::DelegationChain(parent_for_pane)
                        } else {
                            WindowSource::Manual
                        };
                        let window_id = term
                            .create_window(&session_id, "delegation", window_source)
                            .await
                            .map_err(|e| {
                                rusvel_core::error::RusvelError::Tool(format!(
                                    "delegate_agent: create_window failed: {e}"
                                ))
                            })?;
                        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
                        let pane_id = term
                            .create_pane(
                                &window_id,
                                IDLE_PANE_CMD,
                                &cwd,
                                PaneSize {
                                    rows: 24,
                                    cols: 80,
                                },
                                PaneSource::Delegation {
                                    parent_run_id: parent_for_pane,
                                    delegated_run_id: child_run_id,
                                    persona: persona_label.clone(),
                                },
                                None,
                            )
                            .await
                            .map_err(|e| {
                                rusvel_core::error::RusvelError::Tool(format!(
                                    "delegate_agent: create_pane failed: {e}"
                                ))
                            })?;
                        let banner = format!(
                            "\r\n\x1b[1;36m── delegate_agent · run {child_run_id} · {persona_label} ──\x1b[0m\r\n"
                        );
                        let _ = term.inject_pane_output(&pane_id, banner.as_bytes()).await;
                        pane_id_opt = Some(pane_id);
                    }

                    let input = Content::text(prompt);

                    let output = if let (Some(term), Some(pid)) = (&terminal, &pane_id_opt) {
                        run_delegation_streaming(&agent, term, pid, &child_run_id, input).await?
                    } else {
                        agent.run(&child_run_id, input).await.map_err(|e| {
                            rusvel_core::error::RusvelError::Tool(format!(
                                "delegate_agent: agent run failed: {e}"
                            ))
                        })?
                    };

                    let text: String = output
                        .content
                        .parts
                        .iter()
                        .filter_map(|p| match p {
                            Part::Text(t) => Some(t.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");

                    let mut meta = json!({
                        "run_id": output.run_id.to_string(),
                        "tool_calls": output.tool_calls,
                        "cost_estimate": output.cost_estimate,
                        "delegation_depth": depth + 1,
                    });
                    if let Some(pid) = pane_id_opt {
                        meta["pane_id"] = json!(pid.to_string());
                    }

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(text),
        metadata: meta,
                    })
                })
            }),
        )
        .await
        .unwrap();
}

async fn run_delegation_streaming(
    agent: &Arc<AgentRuntime>,
    term: &Arc<dyn TerminalPort>,
    pane_id: &PaneId,
    child_run_id: &RunId,
    input: Content,
) -> Result<AgentOutput, rusvel_core::error::RusvelError> {
    let mut rx = agent
        .run_streaming(child_run_id, input)
        .await
        .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("delegate_agent: {e}")))?;

    let mut final_out: Option<AgentOutput> = None;

    while let Some(ev) = rx.recv().await {
        if let Some(bytes) = format_event_for_pane(&ev) {
            let _ = term.inject_pane_output(pane_id, &bytes).await;
        }

        match ev {
            AgentEvent::Done { output } => {
                final_out = Some(output);
                break;
            }
            AgentEvent::Error { message } => {
                let line = format!("\r\n\x1b[31m[error] {message}\x1b[0m\r\n");
                let _ = term.inject_pane_output(pane_id, line.as_bytes()).await;
                return Err(rusvel_core::error::RusvelError::Tool(format!(
                    "delegate_agent: {message}"
                )));
            }
            _ => {}
        }
    }

    final_out.ok_or_else(|| {
        rusvel_core::error::RusvelError::Tool(
            "delegate_agent: stream ended without completion".into(),
        )
    })
}

fn format_event_for_pane(ev: &AgentEvent) -> Option<Vec<u8>> {
    match ev {
        AgentEvent::TextDelta { text } => Some(text.as_bytes().to_vec()),
        AgentEvent::ToolCall { name, args, .. } => {
            let s = format!("\r\n\x1b[33m▶ {name}\x1b[0m {}\r\n", args);
            Some(s.into_bytes())
        }
        AgentEvent::ToolResult {
            name,
            output,
            is_error,
            ..
        } => {
            let tag = if *is_error { "err" } else { "ok" };
            let s = format!(
                "\r\n\x1b[35m◀ {name} [{tag}]\x1b[0m {}\r\n",
                output.chars().take(4000).collect::<String>()
            );
            Some(s.into_bytes())
        }
        AgentEvent::StateDelta { delta } => {
            let s = format!("\r\n\x1b[90m[state] {delta}\x1b[0m\r\n");
            Some(s.into_bytes())
        }
        AgentEvent::Done { .. } | AgentEvent::Error { .. } => None,
    }
}
