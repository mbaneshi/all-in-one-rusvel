//! code eval — analyze a tiny fixture repo, assert symbol count is sane.

use async_trait::async_trait;
use code_engine::CodeEngine;

use crate::{Eval, EvalCtx, EvalResult};

pub struct CodeAnalyzeEval;

const FIXTURE_LIB_RS: &str = r#"
//! Tiny fixture crate for evals.

pub fn hello() -> &'static str { "hi" }

pub fn add(a: i32, b: i32) -> i32 { a + b }

pub struct Counter { pub n: u64 }

impl Counter {
    pub fn new() -> Self { Self { n: 0 } }
    pub fn tick(&mut self) { self.n += 1; }
}

fn private_helper() -> bool { true }
"#;

#[async_trait]
impl Eval for CodeAnalyzeEval {
    fn name(&self) -> &str {
        "code.analyze_fixture_repo"
    }

    fn suite(&self) -> &str {
        "code"
    }

    async fn run(&self, ctx: &EvalCtx) -> EvalResult {
        // Lay out a temp "repo" with one Rust file.
        let dir = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => return EvalResult::fail(format!("tempdir failed: {e}")),
        };
        let lib = dir.path().join("lib.rs");
        if let Err(e) = std::fs::write(&lib, FIXTURE_LIB_RS) {
            return EvalResult::fail(format!("write fixture failed: {e}"));
        }

        let engine = CodeEngine::new(ctx.storage(), ctx.events_port());

        let analysis = match engine.analyze(dir.path()).await {
            Ok(a) => a,
            Err(e) => return EvalResult::fail(format!("analyze failed: {e}")),
        };

        // Expect at least 5 top-level symbols: hello, add, Counter, new, tick.
        // (private_helper / impl block bring it higher; we use ≥5 as a soft lower bound.)
        let expected_min = 5;
        if analysis.metrics.total_symbols < expected_min {
            return EvalResult::fail(format!(
                "expected total_symbols ≥ {expected_min}, got {}",
                analysis.metrics.total_symbols
            ));
        }
        if analysis.metrics.total_files == 0 {
            return EvalResult::fail("expected total_files ≥ 1 after analyze");
        }

        let has_add = analysis.symbols.iter().any(|s| s.name == "add");
        let has_counter = analysis.symbols.iter().any(|s| s.name == "Counter");
        if !has_add || !has_counter {
            return EvalResult::fail(format!(
                "missing expected symbols: has_add={has_add}, has_counter={has_counter}"
            ));
        }

        // Index should be populated; a search must return a hit for a known symbol.
        let hits = match engine.search("hello", 5) {
            Ok(h) => h,
            Err(e) => return EvalResult::fail(format!("search failed: {e}")),
        };
        if hits.is_empty() {
            return EvalResult::fail("BM25 search for 'hello' returned 0 hits");
        }

        EvalResult::pass(format!(
            "{} symbols across {} files; search('hello') → {} hit(s)",
            analysis.metrics.total_symbols,
            analysis.metrics.total_files,
            hits.len()
        ))
        .with_metrics(serde_json::json!({
            "total_symbols": analysis.metrics.total_symbols,
            "total_files":   analysis.metrics.total_files,
            "search_hits":   hits.len()
        }))
    }
}
