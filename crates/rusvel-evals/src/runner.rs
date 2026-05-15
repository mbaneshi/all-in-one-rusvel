//! Registry + runner that orchestrates all evals and produces an
//! [`EvalReport`] suitable for printing in CI logs.

use std::sync::Arc;

use crate::{Eval, EvalCtx, EvalResult};

/// Outcome row, one per eval.
#[derive(Debug, Clone)]
pub struct EvalOutcome {
    pub name: String,
    pub suite: String,
    pub result: EvalResult,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Default)]
pub struct EvalReport {
    pub outcomes: Vec<EvalOutcome>,
}

impl EvalReport {
    pub fn passed(&self) -> usize {
        self.outcomes.iter().filter(|o| o.result.passed).count()
    }
    pub fn failed(&self) -> usize {
        self.outcomes.iter().filter(|o| !o.result.passed).count()
    }
    pub fn all_passed(&self) -> bool {
        self.failed() == 0
    }

    /// Render a compact, CI-readable summary.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("┌─ rusvel-evals ──────────────────────────────────────────\n");
        for o in &self.outcomes {
            let tag = if o.result.passed { "PASS" } else { "FAIL" };
            out.push_str(&format!(
                "│ [{tag}] {:<10} {:<30}  ({} ms)\n",
                o.suite, o.name, o.duration_ms
            ));
            if !o.result.passed {
                out.push_str(&format!("│        → {}\n", o.result.message));
            }
        }
        out.push_str(&format!(
            "└─ {} passed / {} failed (total {})\n",
            self.passed(),
            self.failed(),
            self.outcomes.len()
        ));
        out
    }
}

/// All evals registered with the suite. Add a new fixture here when you
/// land one — the CI workflow runs every entry.
pub fn registry() -> Vec<Arc<dyn Eval>> {
    use crate::fixtures::*;
    vec![
        Arc::new(forge::ForgeMissionEval),
        Arc::new(harvest::HarvestScanScoreEval),
        Arc::new(code::CodeAnalyzeEval),
        Arc::new(content::ContentDraftTweetEval),
        Arc::new(flow::FlowLinearDagEval),
    ]
}

/// Run a single eval in a fresh [`EvalCtx`].
pub async fn run_one(eval: &dyn Eval) -> EvalOutcome {
    let ctx = match EvalCtx::new() {
        Ok(c) => c,
        Err(e) => {
            return EvalOutcome {
                name: eval.name().to_string(),
                suite: eval.suite().to_string(),
                result: EvalResult::fail(format!("EvalCtx::new failed: {e}")),
                duration_ms: 0,
            };
        }
    };
    let start = std::time::Instant::now();
    let result = eval.run(&ctx).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    EvalOutcome {
        name: eval.name().to_string(),
        suite: eval.suite().to_string(),
        result,
        duration_ms,
    }
}

/// Run every registered eval, optionally filtered to one `--suite`.
pub async fn run_suite(filter: Option<&str>) -> EvalReport {
    let mut report = EvalReport::default();
    for eval in registry() {
        if let Some(s) = filter {
            if eval.suite() != s {
                continue;
            }
        }
        report.outcomes.push(run_one(eval.as_ref()).await);
    }
    report
}
