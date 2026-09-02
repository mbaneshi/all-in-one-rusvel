//! forge eval — generate a daily mission, assert plan shape.

use std::sync::Arc;

use async_trait::async_trait;
use forge_engine::ForgeEngine;
use rusvel_core::domain::Timeframe;
use rusvel_core::id::SessionId;

use crate::{Eval, EvalCtx, EvalResult, stubs::ScriptedAgent};

pub struct ForgeMissionEval;

const PLAN_FIXTURE: &str = include_str!("../../fixtures/forge_daily_plan.json");

#[async_trait]
impl Eval for ForgeMissionEval {
    fn name(&self) -> &str {
        "forge.mission_today"
    }

    fn suite(&self) -> &str {
        "forge"
    }

    async fn run(&self, ctx: &EvalCtx) -> EvalResult {
        // Pre-load the scripted agent: any prompt mentioning "daily plan"
        // returns the canned JSON above.
        let agent = Arc::new(
            ScriptedAgent::new()
                .with_fixture("prioritized daily plan", PLAN_FIXTURE)
                .with_fallback(PLAN_FIXTURE),
        );
        ctx.set_agent(agent);

        let engine = ForgeEngine::new(
            ctx.agent(),
            ctx.events_port(),
            ctx.memory_port(),
            ctx.storage(),
            ctx.jobs(),
            ctx.session_port(),
            ctx.config_port(),
        );

        let session = SessionId::new();

        if let Err(e) = engine
            .set_goal(
                &session,
                "Ship v0.1".into(),
                "Cut MVP release".into(),
                Timeframe::Month,
            )
            .await
        {
            return EvalResult::fail(format!("set_goal failed: {e}"));
        }

        let plan = match engine.mission_today(&session).await {
            Ok(p) => p,
            Err(e) => return EvalResult::fail(format!("mission_today failed: {e}")),
        };

        if plan.tasks.len() < 3 {
            return EvalResult::fail(format!(
                "expected ≥3 tasks in plan, got {}",
                plan.tasks.len()
            ));
        }
        if plan.focus_areas.is_empty() {
            return EvalResult::fail("plan.focus_areas was empty — expected ≥1");
        }
        if !plan.tasks.iter().any(|t| t.title.contains("PR")) {
            return EvalResult::fail(format!(
                "expected a task mentioning 'PR'; got titles {:?}",
                plan.tasks.iter().map(|t| &t.title).collect::<Vec<_>>()
            ));
        }

        EvalResult::pass(format!(
            "generated plan with {} tasks, {} focus areas",
            plan.tasks.len(),
            plan.focus_areas.len()
        ))
        .with_metrics(serde_json::json!({
            "tasks": plan.tasks.len(),
            "focus_areas": plan.focus_areas.len()
        }))
    }
}
