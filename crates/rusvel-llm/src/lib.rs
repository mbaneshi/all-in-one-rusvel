//! `rusvel-llm` — LLM provider adapters implementing [`LlmPort`].
//!
//! Included adapters:
//! - [`OllamaProvider`] — local Ollama instance
//! - [`ClaudeProvider`] — Anthropic Claude API
//! - [`OpenAiProvider`] — `OpenAI` API (also works with compatible proxies)
//! - [`MultiProvider`] — routes requests by [`ModelProvider`] to the right adapter

mod claude;
mod claude_cli;
mod claude_routing;
pub mod cost;
mod cost_tracking;
mod cursor_agent;
mod flat_prompt;
mod multi;
mod ollama;
mod openai;
pub mod stream;
mod tier_routing;

pub use claude::{CLAUDE_META_EFFORT, CLAUDE_META_THINKING, ClaudeProvider};
pub use claude_cli::ClaudeCliProvider;
pub use claude_routing::{claude_cli_forced_by_env, claude_transport_is_cli};
pub use cost::{LLM_COST_METRIC_NAME, SpendAggregation, aggregate_spend};
pub use cost_tracking::CostTrackingLlm;
pub use cursor_agent::CursorAgentProvider;
pub use multi::MultiProvider;
pub use ollama::OllamaProvider;
pub use openai::OpenAiProvider;
pub use stream::{ClaudeCliStreamer, StreamEvent};
pub use tier_routing::apply_model_tier;
