//! Integration test — `cargo test -p rusvel-evals` runs every fixture eval.
//!
//! Each registered [`rusvel_evals::Eval`] becomes one subtest. Failures
//! report the specific assertion (e.g. "expected score >= 0.3, got 0.12")
//! so CI logs are immediately actionable.

use rusvel_evals::{registry, run_one};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn all_fixture_evals_pass() {
    let evals = registry();
    assert!(
        evals.len() >= 5,
        "expected ≥5 registered evals, got {}",
        evals.len()
    );

    let mut failures = Vec::new();
    for eval in &evals {
        let outcome = run_one(eval.as_ref()).await;
        println!(
            "[{}] {}.{}  ({} ms)\n    {}",
            if outcome.result.passed {
                "PASS"
            } else {
                "FAIL"
            },
            outcome.suite,
            outcome.name,
            outcome.duration_ms,
            outcome.result.message
        );
        if !outcome.result.passed {
            failures.push(format!(
                "{}.{}: {}",
                outcome.suite, outcome.name, outcome.result.message
            ));
        }
    }

    assert!(
        failures.is_empty(),
        "{} eval(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[tokio::test]
async fn suites_are_unique_per_engine() {
    let evals = registry();
    let mut suites: Vec<_> = evals.iter().map(|e| e.suite().to_string()).collect();
    suites.sort();
    suites.dedup();
    // Each engine (forge, harvest, code, content, flow) should be represented exactly once
    // in the starter set so the merge gate covers all engines.
    assert!(
        suites.contains(&"forge".to_string())
            && suites.contains(&"harvest".to_string())
            && suites.contains(&"code".to_string())
            && suites.contains(&"content".to_string())
            && suites.contains(&"flow".to_string()),
        "missing engine coverage. suites present: {suites:?}"
    );
}
