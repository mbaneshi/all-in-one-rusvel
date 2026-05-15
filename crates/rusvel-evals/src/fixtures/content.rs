//! content eval — draft a Tweet, assert length constraint.

use std::sync::Arc;

use async_trait::async_trait;
use content_engine::ContentEngine;
use rusvel_core::domain::ContentKind;
use rusvel_core::id::SessionId;

use crate::{Eval, EvalCtx, EvalResult, stubs::ScriptedAgent};

pub struct ContentDraftTweetEval;

/// Twitter long-tweet ceiling. The eval requires drafts to fit comfortably
/// inside the platform's hard limit (no soft-degrade required).
const TWEET_LIMIT: usize = 280;

/// Canned tweet response — under 280 chars, title on first line.
const TWEET_FIXTURE: &str = include_str!("../../fixtures/content_tweet.md");

#[async_trait]
impl Eval for ContentDraftTweetEval {
    fn name(&self) -> &str {
        "content.draft_tweet_length"
    }

    fn suite(&self) -> &str {
        "content"
    }

    async fn run(&self, ctx: &EvalCtx) -> EvalResult {
        let agent = Arc::new(
            ScriptedAgent::new()
                .with_fixture("Write a Tweet article", TWEET_FIXTURE)
                .with_fallback(TWEET_FIXTURE),
        );
        ctx.set_agent(agent);

        let engine = ContentEngine::new(ctx.storage(), ctx.events_port(), ctx.agent(), ctx.jobs());

        let session = SessionId::new();
        let item = match engine
            .draft(&session, "Rust async I/O for beginners", ContentKind::Tweet)
            .await
        {
            Ok(i) => i,
            Err(e) => return EvalResult::fail(format!("draft failed: {e}")),
        };

        if item.title.is_empty() {
            return EvalResult::fail("drafted Tweet had empty title");
        }
        if item.body_markdown.is_empty() {
            return EvalResult::fail("drafted Tweet had empty body");
        }
        let body_len = item.body_markdown.chars().count();
        if body_len > TWEET_LIMIT {
            return EvalResult::fail(format!(
                "expected Tweet body ≤ {TWEET_LIMIT} chars, got {body_len}"
            ));
        }
        if body_len < 20 {
            return EvalResult::fail(format!(
                "Tweet body suspiciously short: {body_len} chars — '{}'",
                item.body_markdown
            ));
        }

        EvalResult::pass(format!(
            "drafted Tweet '{}' — {} chars (limit {})",
            item.title, body_len, TWEET_LIMIT
        ))
        .with_metrics(serde_json::json!({
            "body_chars": body_len,
            "limit":      TWEET_LIMIT
        }))
    }
}
