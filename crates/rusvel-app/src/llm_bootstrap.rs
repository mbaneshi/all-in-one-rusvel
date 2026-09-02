//! Compose [`MultiProvider`] from environment (Phase 0 — LLM wiring truth).
//!
//! - Operator prefs (`force_claude_cli`) from ObjectStore at boot override env when set.
//! - `RUSVEL_USE_CLAUDE_CLI=1` (or `true`/`yes`/`on`): always [`ClaudeCliProvider`] when pref is `None`.
//! - Else `ANTHROPIC_API_KEY` non-empty: [`ClaudeProvider`] (Messages API).
//! - Else: [`ClaudeCliProvider`] (subscription / `claude` CLI).
//! - `OPENAI_API_KEY`: register [`OpenAiProvider`].
//! - Ollama: always registered at `OLLAMA_HOST` or `http://localhost:11434` (fails at call time if down).
//! - `cursor`: [`CursorAgentProvider::from_env`].

use std::sync::Arc;

use rusvel_api::operator_runtime::OperatorRuntimePrefs;
use rusvel_core::domain::ModelProvider;
use rusvel_llm::{
    ClaudeCliProvider, ClaudeProvider, CursorAgentProvider, MultiProvider, OllamaProvider,
    OpenAiProvider, claude_transport_is_cli,
};

fn anthropic_key_present() -> bool {
    std::env::var("ANTHROPIC_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// Resolve whether Claude traffic uses CLI (`claude -p`) for this process.
pub fn resolve_claude_cli(pref: &OperatorRuntimePrefs) -> bool {
    match pref.force_claude_cli {
        Some(true) => true,
        Some(false) => !anthropic_key_present(),
        None => claude_transport_is_cli(),
    }
}

/// Build the multi-provider stack. Returns `(Arc<multi>, claude_uses_cli)`.
pub fn compose_llm_multi(pref: &OperatorRuntimePrefs) -> (Arc<MultiProvider>, bool) {
    let mut use_cli = resolve_claude_cli(pref);
    if !use_cli && !anthropic_key_present() {
        use_cli = true;
    }

    let llm_multi = MultiProvider::new();

    if use_cli {
        tracing::info!(
            target: "rusvel::llm",
            "registering ClaudeCliProvider for ModelProvider::Claude (claude -p / subscription path)"
        );
        llm_multi.register(
            ModelProvider::Claude,
            Arc::new(ClaudeCliProvider::max_subscription()),
        );
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .unwrap_or_default()
            .trim()
            .to_string();
        // `ANTHROPIC_BASE_URL` (same env the official SDKs honor) points the
        // Messages API at a gateway (e.g. AvalAI: https://api.avalai.ir/v1).
        let provider = match std::env::var("ANTHROPIC_BASE_URL") {
            Ok(url) if !url.trim().is_empty() => {
                tracing::info!(
                    target: "rusvel::llm",
                    base_url = %url.trim(),
                    "registering ClaudeProvider (Messages API, custom base URL) for ModelProvider::Claude"
                );
                ClaudeProvider::with_base_url(key, url.trim())
            }
            _ => {
                tracing::info!(
                    target: "rusvel::llm",
                    "registering ClaudeProvider (Messages API) for ModelProvider::Claude"
                );
                ClaudeProvider::new(key)
            }
        };
        llm_multi.register(ModelProvider::Claude, Arc::new(provider));
    }

    if let Ok(raw) = std::env::var("OPENAI_API_KEY") {
        let key = raw.trim().to_string();
        if !key.is_empty() {
            // `OPENAI_BASE_URL` for OpenAI-compatible gateways (AvalAI, OpenRouter, …).
            let provider = match std::env::var("OPENAI_BASE_URL") {
                Ok(url) if !url.trim().is_empty() => {
                    tracing::info!(target: "rusvel::llm", base_url = %url.trim(), "registering OpenAiProvider (custom base URL)");
                    OpenAiProvider::with_base_url(key, url.trim())
                }
                _ => {
                    tracing::info!(target: "rusvel::llm", "registering OpenAiProvider");
                    OpenAiProvider::new(key)
                }
            };
            llm_multi.register(ModelProvider::OpenAI, Arc::new(provider));
        }
    }

    let ollama_url =
        std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".into());
    llm_multi.register(
        ModelProvider::Ollama,
        Arc::new(OllamaProvider::with_base_url(ollama_url)),
    );

    llm_multi.register(
        ModelProvider::Other("cursor".into()),
        Arc::new(CursorAgentProvider::from_env()),
    );

    (Arc::new(llm_multi), use_cli)
}
