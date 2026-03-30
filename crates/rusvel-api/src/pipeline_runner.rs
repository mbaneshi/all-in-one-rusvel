//! [`forge_engine::pipeline::PipelineStepRunner`] for Harvest + Content engines.

use std::sync::Arc;

use async_trait::async_trait;
use content_engine::ContentEngine;
use harvest_engine::{HarvestEngine, HarvestScanParams, scan_from_params};
use rusvel_core::domain::ContentKind;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use rusvel_core::ports::BrowserPort;
use serde_json::{Value, json};

/// Runs scan → score → propose → draft using real engines.
pub struct HarvestContentPipelineRunner {
    pub harvest: Arc<HarvestEngine>,
    pub content: Arc<ContentEngine>,
    pub browser: Option<Arc<dyn BrowserPort>>,
    pub scan_params: HarvestScanParams,
}

impl HarvestContentPipelineRunner {
    /// Pipeline scan defaults to mock-only when `scan_params` is default-mock.
    pub fn new(
        harvest: Arc<HarvestEngine>,
        content: Arc<ContentEngine>,
        browser: Option<Arc<dyn BrowserPort>>,
        scan_params: HarvestScanParams,
    ) -> Self {
        Self {
            harvest,
            content,
            browser,
            scan_params,
        }
    }
}

#[async_trait]
impl forge_engine::pipeline::PipelineStepRunner for HarvestContentPipelineRunner {
    async fn scan(&self, session_id: &SessionId) -> Result<Value> {
        let opps = scan_from_params(
            self.harvest.as_ref(),
            session_id,
            &self.scan_params,
            self.browser.clone(),
        )
        .await?;
        let opportunity_ids: Vec<String> = opps.iter().map(|o| o.id.to_string()).collect();
        let titles: Vec<String> = opps.iter().map(|o| o.title.clone()).collect();
        Ok(json!({
            "opportunity_ids": opportunity_ids,
            "titles": titles,
            "count": opps.len(),
        }))
    }

    async fn score(&self, session_id: &SessionId, after_scan: &Value) -> Result<Value> {
        let ids = after_scan
            .get("scan")
            .and_then(|s| s.get("opportunity_ids"))
            .and_then(|a| a.as_array())
            .cloned()
            .unwrap_or_default();
        let mut n = 0u32;
        for id in ids.iter().filter_map(|v| v.as_str()).take(20) {
            self.harvest.score_opportunity(session_id, id).await?;
            n += 1;
        }
        Ok(json!({ "rescored": n }))
    }

    async fn propose(&self, session_id: &SessionId, ctx: &Value, profile: &str) -> Result<Value> {
        let scan = ctx
            .get("scan")
            .ok_or_else(|| RusvelError::Validation("pipeline: missing scan step output".into()))?;
        let first_id = scan
            .get("opportunity_ids")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                RusvelError::Validation("pipeline: no opportunity_ids from scan".into())
            })?;
        let proposal = self
            .harvest
            .generate_proposal(session_id, first_id, profile)
            .await?;
        Ok(json!({
            "opportunity_id": first_id,
            "proposal": proposal,
        }))
    }

    async fn draft_content(
        &self,
        session_id: &SessionId,
        ctx: &Value,
        draft_topic: Option<&str>,
        kind: ContentKind,
    ) -> Result<Value> {
        let topic = draft_topic
            .map(String::from)
            .or_else(|| {
                ctx.get("scan")
                    .and_then(|s| s.get("titles"))
                    .and_then(|a| a.as_array())
                    .and_then(|a| a.first())
                    .and_then(|v| v.as_str())
                    .map(String::from)
            })
            .unwrap_or_else(|| "Pipeline content draft".into());
        let item = self.content.draft(session_id, &topic, kind).await?;
        Ok(json!({
            "content_id": item.id.to_string(),
            "title": item.title,
        }))
    }
}
