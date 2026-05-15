//! # rusvel-evals
//!
//! Fixture-based eval suite for engine work. Per the LangChain 2025 survey,
//! only 52% of agent teams have evals — this crate is RUSVEL's merge gate.
//!
//! ## Design
//!
//! An [`Eval`] is a deterministic test that runs an engine against an
//! [`EvalCtx`] holding **real** ports (in-memory SQLite via [`rusvel_db::Database`])
//! plus a **stub LLM** ([`stubs::ScriptedAgent`]) that replays recorded
//! JSON fixtures keyed by prompt prefix.
//!
//! Failure messages are intentionally specific (`expected score >= 0.3,
//! got 0.12`) so CI logs are actionable.

pub mod ctx;
pub mod fixtures;
pub mod runner;
pub mod stubs;

pub use ctx::EvalCtx;
pub use runner::{EvalOutcome, EvalReport, registry, run_one, run_suite};

use async_trait::async_trait;

/// Outcome of running a single eval.
#[derive(Debug, Clone)]
pub struct EvalResult {
    pub passed: bool,
    /// Human-readable message — on failure, the *specific* assertion that failed.
    pub message: String,
    /// Optional metrics for trend tracking (score, latency_ms, etc).
    pub metrics: serde_json::Value,
}

impl EvalResult {
    pub fn pass(message: impl Into<String>) -> Self {
        Self {
            passed: true,
            message: message.into(),
            metrics: serde_json::json!({}),
        }
    }

    pub fn fail(message: impl Into<String>) -> Self {
        Self {
            passed: false,
            message: message.into(),
            metrics: serde_json::json!({}),
        }
    }

    pub fn with_metrics(mut self, metrics: serde_json::Value) -> Self {
        self.metrics = metrics;
        self
    }
}

/// A single eval scenario. Implementors construct fixture data, run an
/// engine entry point, and assert observable behavior.
#[async_trait]
pub trait Eval: Send + Sync {
    /// Stable name (used by `--suite <name>` and report rows).
    fn name(&self) -> &str;

    /// Which suite this eval belongs to (e.g. "forge", "harvest").
    fn suite(&self) -> &str;

    /// Run the eval. Implementations should return [`EvalResult::fail`]
    /// with a *specific* assertion message on failure rather than panicking.
    async fn run(&self, ctx: &EvalCtx) -> EvalResult;
}
