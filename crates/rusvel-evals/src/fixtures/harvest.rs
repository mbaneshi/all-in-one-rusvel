//! harvest eval — scan MockSource, top-N by score, assert distribution is sane.

use harvest_engine::{HarvestConfig, HarvestEngine, source::MockSource};
use rusvel_core::id::SessionId;

use async_trait::async_trait;

use crate::{Eval, EvalCtx, EvalResult};

pub struct HarvestScanScoreEval;

/// Lower bound on the *highest* score in the result set. Mock fixtures
/// include Rust roles — with `skills: ["rust"]` keyword scoring, at least
/// one item should beat this threshold. Soft check (`assert_close`-style)
/// per the issue's risk mitigation.
const SCORE_FLOOR_TOP: f64 = 0.30;

#[async_trait]
impl Eval for HarvestScanScoreEval {
    fn name(&self) -> &str {
        "harvest.scan_score_distribution"
    }

    fn suite(&self) -> &str {
        "harvest"
    }

    async fn run(&self, ctx: &EvalCtx) -> EvalResult {
        // No agent — keyword scoring path. Deterministic.
        let engine = HarvestEngine::new(ctx.storage())
            .with_events(ctx.events_port())
            .with_config(HarvestConfig {
                skills: vec!["rust".into(), "cli".into()],
                min_budget: Some(1000.0),
            });

        let session = SessionId::new();
        let source = MockSource::new();

        let opps = match engine.scan(&session, &source).await {
            Ok(v) => v,
            Err(e) => return EvalResult::fail(format!("scan failed: {e}")),
        };

        if opps.len() < 3 {
            return EvalResult::fail(format!(
                "expected ≥3 mock opportunities, got {}",
                opps.len()
            ));
        }

        let mut sorted = opps.clone();
        sorted.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let top = &sorted[0];

        if top.score < SCORE_FLOOR_TOP {
            return EvalResult::fail(format!(
                "expected top score ≥ {SCORE_FLOOR_TOP:.2} in distribution, got {:.2} ({})",
                top.score, top.title
            ));
        }

        let rust_count = opps
            .iter()
            .filter(|o| {
                o.description.to_lowercase().contains("rust")
                    || o.title.to_lowercase().contains("rust")
            })
            .count();
        if rust_count == 0 {
            return EvalResult::fail(
                "expected at least one Rust-related opportunity in MockSource scan",
            );
        }

        // Bounds check: scores must lie in [0.0, 1.0].
        if let Some(out_of_range) = sorted.iter().find(|o| !(0.0..=1.0).contains(&o.score)) {
            return EvalResult::fail(format!(
                "score out of [0.0, 1.0] for '{}': {:.3}",
                out_of_range.title, out_of_range.score
            ));
        }

        EvalResult::pass(format!(
            "{} opps, top={:.2} ('{}'), rust-tagged={}",
            opps.len(),
            top.score,
            top.title,
            rust_count
        ))
        .with_metrics(serde_json::json!({
            "count": opps.len(),
            "top_score": top.score,
            "rust_count": rust_count
        }))
    }
}
