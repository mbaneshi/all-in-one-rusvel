//! Configuration API — manages model, effort, tools, and budget settings.
//!
//! Settings are persisted in `ObjectStore` and used by the chat handler
//! to build [`AgentConfig`](rusvel_core::domain::AgentConfig) (Claude CLI,
//! Cursor agent, Ollama, etc. depending on `model`).

use std::sync::Arc;
use std::time::Duration;

use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use tokio::time::timeout;
use tracing::info;

use rusvel_core::ports::StoragePort;
use rusvel_llm::claude_cli_forced_by_env;

use crate::AppState;

/// The user's chat configuration — assembled into `claude -p` flags.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatConfig {
    pub model: String,
    pub effort: String,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub max_turns: Option<u32>,
    /// Optional tier hint: `fast` | `balanced` | `premium` (see `rusvel_core::domain::ModelTier`).
    #[serde(default)]
    pub model_tier: Option<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            // Default to Cursor terminal agent (`rusvel-llm` CursorAgentProvider) to avoid
            // Claude Max / API limits when Cursor subscription is available. Pick
            // `claude/sonnet` etc. in the UI for Claude CLI routing.
            model: "cursor/sonnet-4".into(),
            effort: "medium".into(),
            max_budget_usd: None,
            permission_mode: "default".into(),
            allowed_tools: vec![],
            disallowed_tools: vec![],
            max_turns: None,
            model_tier: None,
        }
    }
}

impl ChatConfig {
    /// Model id for `claude -p --model` (strip `claude/` prefix; bare ids unchanged).
    fn claude_cli_model_flag(&self) -> String {
        match self.model.split_once('/') {
            Some(("claude", name)) => name.to_string(),
            _ => self.model.clone(),
        }
    }

    /// Build CLI arguments for `claude -p` from this config.
    pub fn to_claude_args(&self) -> Vec<String> {
        let mut args = vec![
            "--model".into(),
            self.claude_cli_model_flag(),
            "--effort".into(),
            self.effort.clone(),
            "--permission-mode".into(),
            self.permission_mode.clone(),
        ];
        if let Some(budget) = self.max_budget_usd {
            args.push("--max-budget-usd".into());
            args.push(budget.to_string());
        }
        if !self.allowed_tools.is_empty() {
            args.push("--allowedTools".into());
            args.push(self.allowed_tools.join(" "));
        }
        if !self.disallowed_tools.is_empty() {
            args.push("--disallowedTools".into());
            args.push(self.disallowed_tools.join(" "));
        }
        if let Some(turns) = self.max_turns {
            args.push("--max-turns".into());
            args.push(turns.to_string());
        }
        args
    }
}

const CONFIG_KEY: &str = "chat_config";
const CONFIG_ID: &str = "current";

/// Legacy picker values (before `cursor/…` and `claude/…` prefixes) routed to Claude CLI
/// and hit Max/API limits. Rewrite once to Cursor agent.
pub(crate) fn migrate_legacy_chat_model(config: &mut ChatConfig) -> bool {
    if matches!(config.model.as_str(), "sonnet" | "opus" | "haiku") {
        config.model = "cursor/sonnet-4".into();
        true
    } else {
        false
    }
}

/// Load persisted chat config and migrate legacy model ids to `cursor/sonnet-4` when needed.
pub async fn load_and_migrate_chat_config(
    storage: &Arc<dyn StoragePort>,
) -> Result<ChatConfig, (StatusCode, String)> {
    let stored = storage
        .objects()
        .get(CONFIG_KEY, CONFIG_ID)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let mut config: ChatConfig = stored
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();

    if migrate_legacy_chat_model(&mut config) {
        info!(
            target: "rusvel::config",
            model = %config.model,
            "migrated legacy bare Claude chat model to Cursor agent default"
        );
        let value = serde_json::to_value(&config)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        storage
            .objects()
            .put(CONFIG_KEY, CONFIG_ID, value)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    Ok(config)
}

/// `GET /api/config` — get current chat configuration.
pub async fn get_config(
    State(state): State<Arc<AppState>>,
) -> Result<Json<ChatConfig>, (StatusCode, String)> {
    let config = load_and_migrate_chat_config(&state.storage).await?;
    Ok(Json(config))
}

/// `PUT /api/config` — update chat configuration.
pub async fn update_config(
    State(state): State<Arc<AppState>>,
    Json(config): Json<ChatConfig>,
) -> Result<Json<ChatConfig>, (StatusCode, String)> {
    let value =
        serde_json::to_value(&config).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    state
        .storage
        .objects()
        .put(CONFIG_KEY, CONFIG_ID, value)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(config))
}

/// Available models for the picker.
#[derive(Serialize)]
pub struct ModelOption {
    pub value: String,
    pub label: String,
    pub description: String,
}

/// `GET /api/config/models` — list available models.
pub async fn list_models() -> Json<Vec<ModelOption>> {
    Json(vec![
        ModelOption {
            value: "cursor/sonnet-4".into(),
            label: "Cursor · Sonnet 4".into(),
            description: "Cursor terminal agent (cursor agent --print) — uses Cursor quota; RUSVEL tools not forwarded".into(),
        },
        ModelOption {
            value: "cursor/gpt-5".into(),
            label: "Cursor · GPT-5".into(),
            description: "Cursor agent with OpenAI-class model id (if available on your account)".into(),
        },
        ModelOption {
            value: "cursor/sonnet-4-thinking".into(),
            label: "Cursor · Sonnet thinking".into(),
            description: "Cursor agent with extended reasoning variant (if listed by cursor agent --list-models)".into(),
        },
        ModelOption {
            value: "claude/sonnet".into(),
            label: "Claude · Sonnet".into(),
            description: "Claude: Messages API when ANTHROPIC_API_KEY is set at boot; otherwise Claude CLI (Max/subscription).".into(),
        },
        ModelOption {
            value: "claude/opus".into(),
            label: "Claude · Opus".into(),
            description: "Claude: same routing as Sonnet — API key prefers HTTP adapter.".into(),
        },
        ModelOption {
            value: "claude/haiku".into(),
            label: "Claude · Haiku".into(),
            description: "Claude: fastest tier; HTTP API normalizes shorthand ids automatically.".into(),
        },
        ModelOption {
            value: "ollama/llama3.2".into(),
            label: "Ollama · llama3.2".into(),
            description: "Local Ollama at OLLAMA_HOST (default localhost:11434); requires Ollama running".into(),
        },
        ModelOption {
            value: "ollama/qwen2.5-coder:7b".into(),
            label: "Ollama · qwen2.5-coder".into(),
            description: "Requires this exact tag in Ollama: `ollama pull qwen2.5-coder:7b` (not the same as qwen2.5:0.5b).".into(),
        },
        ModelOption {
            value: "openai/gpt-4o".into(),
            label: "OpenAI · gpt-4o".into(),
            description: "OpenAI API when OPENAI_API_KEY is set at server boot".into(),
        },
    ])
}

/// Available tools for toggle panel.
#[derive(Serialize)]
pub struct ToolOption {
    pub name: String,
    pub description: String,
    pub category: String,
}

/// `GET /api/config/tools` — list all available tools.
pub async fn list_tools() -> Json<Vec<ToolOption>> {
    Json(vec![
        ToolOption {
            name: "Read".into(),
            description: "Read file contents".into(),
            category: "Files".into(),
        },
        ToolOption {
            name: "Write".into(),
            description: "Create/overwrite files".into(),
            category: "Files".into(),
        },
        ToolOption {
            name: "Edit".into(),
            description: "Edit specific parts of files".into(),
            category: "Files".into(),
        },
        ToolOption {
            name: "Bash".into(),
            description: "Execute shell commands".into(),
            category: "System".into(),
        },
        ToolOption {
            name: "Glob".into(),
            description: "Find files by pattern".into(),
            category: "Search".into(),
        },
        ToolOption {
            name: "Grep".into(),
            description: "Search file contents".into(),
            category: "Search".into(),
        },
        ToolOption {
            name: "WebSearch".into(),
            description: "Search the web".into(),
            category: "Web".into(),
        },
        ToolOption {
            name: "WebFetch".into(),
            description: "Fetch URL content".into(),
            category: "Web".into(),
        },
        ToolOption {
            name: "Agent".into(),
            description: "Spawn sub-agents".into(),
            category: "Agents".into(),
        },
        ToolOption {
            name: "NotebookEdit".into(),
            description: "Edit Jupyter notebooks".into(),
            category: "Files".into(),
        },
    ])
}

/// One row for Settings → LLM providers (no secrets).
#[derive(Debug, Clone, Serialize)]
pub struct LlmProviderStatus {
    pub id: &'static str,
    pub display_name: String,
    /// Registered in [`MultiProvider`] / expected usable for model ids.
    pub wired: bool,
    /// How traffic is routed (API key, CLI, local daemon, …).
    pub route: String,
    /// Probe result when cheap; `null` = not probed or unknown.
    pub healthy: Option<bool>,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct LlmProvidersReport {
    pub providers: Vec<LlmProviderStatus>,
    pub ollama_host: String,
    /// Names from `GET /api/tags` when reachable.
    pub ollama_models: Vec<String>,
    /// Actual backend for `claude/…` on this running process (`cli` = `claude -p`, `api` = Messages API).
    pub claude_effective_transport: String,
    /// True when `RUSVEL_USE_CLAUDE_CLI` forced CLI despite a present API key.
    pub claude_cli_forced_by_env: bool,
    /// True when `operator_prefs.force_claude_cli == Some(true)` at boot.
    pub claude_cli_forced_by_operator_pref: bool,
}

async fn probe_cli(cmd: &str, args: &[&str]) -> Option<bool> {
    let mut child = tokio::process::Command::new(cmd)
        .args(args)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let status = match timeout(Duration::from_secs(4), child.wait()).await {
        Ok(Ok(s)) => s,
        _ => return None,
    };
    Some(status.success())
}

/// Build LLM provider report using **this process’s** effective Claude transport (from boot).
pub async fn build_llm_providers_report_for_state(state: &AppState) -> LlmProvidersReport {
    let forced_pref = state.operator_prefs.force_claude_cli == Some(true);
    build_llm_providers_report(
        state.claude_transport_cli.load(std::sync::atomic::Ordering::Relaxed),
        claude_cli_forced_by_env(),
        forced_pref,
    )
    .await
}

/// Wiring + light reachability probes. `effective_claude_cli` must match how `rusvel-app` registered Claude at boot.
pub async fn build_llm_providers_report(
    effective_claude_cli: bool,
    claude_forced_env: bool,
    claude_forced_operator_pref: bool,
) -> LlmProvidersReport {
    let ollama_host =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".into());
    let ollama_base = ollama_host.trim_end_matches('/').to_string();
    let tags_url = format!("{ollama_base}/api/tags");

    let mut ollama_models: Vec<String> = Vec::new();
    let ollama_healthy = match reqwest::Client::builder()
        .timeout(Duration::from_secs(3))
        .build()
    {
        Ok(client) => match client.get(&tags_url).send().await {
            Ok(resp) if resp.status().is_success() => {
                #[derive(Deserialize)]
                struct TagRow {
                    name: String,
                }
                #[derive(Deserialize)]
                struct TagsBody {
                    models: Vec<TagRow>,
                }
                match resp.json::<TagsBody>().await {
                    Ok(body) => {
                        ollama_models = body.models.into_iter().map(|m| m.name).collect();
                        Some(true)
                    }
                    Err(_) => Some(true),
                }
            }
            Ok(resp) => Some(resp.status().is_success()),
            Err(e) => {
                tracing::debug!(target: "rusvel::config", error = %e, "ollama probe failed");
                Some(false)
            }
        },
        Err(_) => None,
    };

    let anthropic_raw = std::env::var("ANTHROPIC_API_KEY").unwrap_or_default();
    let anthropic_key = !anthropic_raw.trim().is_empty();

    let mut claude_cli_ok = probe_cli("claude", &["--version"]).await;
    if claude_cli_ok != Some(true) {
        claude_cli_ok = probe_cli("claude", &["-h"]).await;
    }
    let claude_cli_detail = match claude_cli_ok {
        Some(true) => Some(
            if effective_claude_cli {
                if claude_forced_operator_pref {
                    "This process routes `claude/…` through `claude -p` because operator_prefs.force_claude_cli=true was saved and applied at boot."
                        .into()
                } else if claude_forced_env {
                    "This process routes `claude/…` through `claude -p` because RUSVEL_USE_CLAUDE_CLI is set (even if ANTHROPIC_API_KEY exists)."
                        .into()
                } else {
                    "This process routes `claude/…` through `claude -p` (no Anthropic API path at boot). Subscription / Claude Code Max."
                        .into()
                }
            } else {
                "`claude` works on PATH, but this server uses the Anthropic Messages API for `claude/…` (API key present and CLI not forced). Low-credit errors come from the API — set operator pref, RUSVEL_USE_CLAUDE_CLI=1, or unset the key, then restart."
                    .into()
            },
        ),
        Some(false) => Some(
            if effective_claude_cli {
                "`claude` failed — fix install/PATH; this server expects CLI for `claude/…`.".into()
            } else {
                "`claude` exited non-zero — only needed if you switch to CLI routing.".into()
            },
        ),
        None => Some(
            if effective_claude_cli {
                "Could not run `claude` within 4s — required for `claude/…` on this server.".into()
            } else {
                "Could not probe `claude` within 4s (optional while using API).".into()
            },
        ),
    };

    let openai_key = std::env::var("OPENAI_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);

    let cursor_bin = std::env::var("RUSVEL_CURSOR_BIN").unwrap_or_else(|_| "cursor".into());
    let cursor_ok = probe_cli(&cursor_bin, &["--version"]).await;
    let cursor_detail = match cursor_ok {
        Some(true) => Some(format!("`{cursor_bin} --version` OK — Cursor agent models use prefix cursor/…")),
        Some(false) => Some(format!("`{cursor_bin} --version` failed — install Cursor CLI or set RUSVEL_CURSOR_BIN.")),
        None => Some(format!("Could not run `{cursor_bin}` within 4s.")),
    };

    let providers = vec![
        LlmProviderStatus {
            id: "anthropic_api",
            display_name: "Anthropic API".into(),
            wired: anthropic_key,
            route: if anthropic_key {
                "ANTHROPIC_API_KEY set".into()
            } else {
                "ANTHROPIC_API_KEY not set".into()
            },
            healthy: None,
            detail: Some(
                "Saving `claude/…` in the UI only sets the model id — it does not switch API vs CLI. Backend is fixed at server boot: operator prefs, then env. Billing errors mean the API rejected the key/account."
                    .into(),
            ),
        },
        LlmProviderStatus {
            id: "claude_cli",
            display_name: "Claude CLI (claude -p)".into(),
            wired: true,
            route: "`claude` on PATH".into(),
            healthy: claude_cli_ok,
            detail: claude_cli_detail,
        },
        LlmProviderStatus {
            id: "cursor",
            display_name: "Cursor agent".into(),
            wired: true,
            route: format!("cursor CLI ({cursor_bin})"),
            healthy: cursor_ok,
            detail: cursor_detail,
        },
        LlmProviderStatus {
            id: "ollama",
            display_name: "Ollama".into(),
            wired: true,
            route: format!("HTTP {ollama_base}"),
            healthy: ollama_healthy,
            detail: Some(
                "Models listed in Settings → LLM are already pulled (ollama pull if missing). Use ollama/<name> in App defaults. Agent tool-calling is not implemented for this adapter yet — use Claude, Cursor, or OpenAI for tool loops."
                    .into(),
            ),
        },
        LlmProviderStatus {
            id: "openai",
            display_name: "OpenAI".into(),
            wired: openai_key,
            route: if openai_key {
                "OPENAI_API_KEY set".into()
            } else {
                "not configured".into()
            },
            healthy: if openai_key { None } else { None },
            detail: if openai_key {
                Some("Use openai/gpt-4o etc. in App defaults.".into())
            } else {
                Some("Set OPENAI_API_KEY and restart the server.".into())
            },
        },
    ];

    LlmProvidersReport {
        providers,
        ollama_host: ollama_base,
        ollama_models,
        claude_effective_transport: if effective_claude_cli {
            "cli".into()
        } else {
            "api".into()
        },
        claude_cli_forced_by_env: claude_forced_env,
        claude_cli_forced_by_operator_pref: claude_forced_operator_pref,
    }
}

/// `GET /api/config/llm-providers` — wiring + light reachability (Ollama HTTP, CLI `--version`).
pub async fn llm_providers_status(State(state): State<Arc<AppState>>) -> Json<LlmProvidersReport> {
    Json(build_llm_providers_report_for_state(&state).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_legacy_opus_to_cursor() {
        let mut c = ChatConfig {
            model: "opus".into(),
            ..Default::default()
        };
        assert!(migrate_legacy_chat_model(&mut c));
        assert_eq!(c.model, "cursor/sonnet-4");
    }

    #[test]
    fn does_not_migrate_claude_prefixed() {
        let mut c = ChatConfig {
            model: "claude/opus".into(),
            ..Default::default()
        };
        assert!(!migrate_legacy_chat_model(&mut c));
    }

    #[test]
    fn to_claude_args_strips_claude_prefix() {
        let c = ChatConfig {
            model: "claude/opus".into(),
            ..Default::default()
        };
        let args = c.to_claude_args();
        let i = args.iter().position(|a| a == "--model").unwrap();
        assert_eq!(args.get(i + 1).map(String::as_str), Some("opus"));
    }
}
