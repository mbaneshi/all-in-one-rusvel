//! Compose [`MultiProvider`] from environment (Phase 0 — LLM wiring truth).
//!
//! - `ANTHROPIC_API_KEY`: use [`ClaudeProvider`] (Messages API) for [`ModelProvider::Claude`].
//! - Otherwise: [`ClaudeCliProvider`] (subscription / `claude` CLI).
//! - `OPENAI_API_KEY`: register [`OpenAiProvider`].
//! - Ollama: always registered at `OLLAMA_HOST` or `http://localhost:11434` (fails at call time if down).
//! - `cursor`: [`CursorAgentProvider::from_env`].

use std::sync::Arc;

use rusvel_core::domain::ModelProvider;
use rusvel_llm::{
    ClaudeCliProvider, ClaudeProvider, CursorAgentProvider, MultiProvider, OllamaProvider,
    OpenAiProvider,
};

/// Build the default multi-provider stack for the API server / agent runtime.
pub fn compose_llm_multi() -> MultiProvider {
    let mut llm_multi = MultiProvider::new();

    if let Ok(raw) = std::env::var("ANTHROPIC_API_KEY") {
        let key = raw.trim().to_string();
        if key.is_empty() {
            tracing::info!(
                target: "rusvel::llm",
                "registering ClaudeCliProvider for ModelProvider::Claude (empty ANTHROPIC_API_KEY)"
            );
            llm_multi.register(
                ModelProvider::Claude,
                Arc::new(ClaudeCliProvider::max_subscription()),
            );
        } else {
            tracing::info!(
                target: "rusvel::llm",
                "registering ClaudeProvider (ANTHROPIC_API_KEY set) for ModelProvider::Claude"
            );
            llm_multi.register(ModelProvider::Claude, Arc::new(ClaudeProvider::new(key)));
        }
    } else {
        tracing::info!(
            target: "rusvel::llm",
            "registering ClaudeCliProvider for ModelProvider::Claude (no ANTHROPIC_API_KEY)"
        );
        llm_multi.register(
            ModelProvider::Claude,
            Arc::new(ClaudeCliProvider::max_subscription()),
        );
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
