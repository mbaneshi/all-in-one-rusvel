//! Compose [`MultiProvider`] from environment (Phase 0 — LLM wiring truth).
//!
//! - `RUSVEL_USE_CLAUDE_CLI=1` (or `true`/`yes`/`on`): always [`ClaudeCliProvider`] for [`ModelProvider::Claude`],
//!   even if `ANTHROPIC_API_KEY` is set (API credits exhausted but Max subscription works).
//! - Else `ANTHROPIC_API_KEY` non-empty: [`ClaudeProvider`] (Messages API).
//! - Else: [`ClaudeCliProvider`] (subscription / `claude` CLI).
//! - `OPENAI_API_KEY`: register [`OpenAiProvider`].
//! - Ollama: always registered at `OLLAMA_HOST` or `http://localhost:11434` (fails at call time if down).
//! - `cursor`: [`CursorAgentProvider::from_env`].

use std::sync::Arc;

use rusvel_core::domain::ModelProvider;
use rusvel_llm::{
    ClaudeCliProvider, ClaudeProvider, CursorAgentProvider, MultiProvider, OllamaProvider,
    OpenAiProvider, claude_transport_is_cli,
};

/// Build the default multi-provider stack for the API server / agent runtime.
pub fn compose_llm_multi() -> MultiProvider {
    let mut llm_multi = MultiProvider::new();

    if claude_transport_is_cli() {
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
        tracing::info!(
            target: "rusvel::llm",
            "registering ClaudeProvider (Messages API) for ModelProvider::Claude"
        );
        llm_multi.register(ModelProvider::Claude, Arc::new(ClaudeProvider::new(key)));
    }

    if let Ok(raw) = std::env::var("OPENAI_API_KEY") {
        let key = raw.trim().to_string();
        if !key.is_empty() {
            tracing::info!(target: "rusvel::llm", "registering OpenAiProvider");
            llm_multi.register(ModelProvider::OpenAI, Arc::new(OpenAiProvider::new(key)));
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

    llm_multi
}
