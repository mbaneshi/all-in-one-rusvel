//! Opportunity pipeline manager — stores, lists, advances, and reports.

use std::collections::HashMap;
use std::sync::Arc;

use rusvel_core::ports::StoragePort;
use rusvel_core::{ObjectFilter, Opportunity, OpportunityStage, Result, RusvelError, SessionId};
use serde::{Deserialize, Serialize};

/// Counts of opportunities grouped by pipeline stage.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PipelineStats {
    pub total: usize,
    pub by_stage: HashMap<String, usize>,
}

/// Manages the opportunity pipeline backed by `ObjectStore`.
pub struct Pipeline {
    storage: Arc<dyn StoragePort>,
}

const OBJ_KIND: &str = "opportunity";

impl Pipeline {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    /// Store a new opportunity in the pipeline.
    pub async fn add(&self, opportunity: &Opportunity) -> Result<()> {
        self.save(opportunity).await
    }

    /// Insert or replace an opportunity row by id.
    pub async fn save(&self, opportunity: &Opportunity) -> Result<()> {
        let value = serde_json::to_value(opportunity)?;
        self.storage
            .objects()
            .put(OBJ_KIND, &opportunity.id.to_string(), value)
            .await
    }

    /// List opportunities for a session, optionally filtered by stage.
    pub async fn list(
        &self,
        session_id: &SessionId,
        stage_filter: Option<&OpportunityStage>,
    ) -> Result<Vec<Opportunity>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(*session_id),
            tags: Vec::new(),
            limit: None,
            offset: None,
        };

        let objects = self.storage.objects().list(OBJ_KIND, filter).await?;

        let mut opportunities: Vec<Opportunity> = objects
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        if let Some(stage) = stage_filter {
            opportunities.retain(|o| &o.stage == stage);
        }

        Ok(opportunities)
    }

    /// Advance an opportunity to a new pipeline stage.
    pub async fn advance(&self, id: &str, new_stage: OpportunityStage) -> Result<()> {
        let value = self
            .storage
            .objects()
            .get(OBJ_KIND, id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: OBJ_KIND.into(),
                id: id.into(),
            })?;

        let mut opportunity: Opportunity = serde_json::from_value(value)?;
        opportunity.stage = new_stage;

        let updated = serde_json::to_value(&opportunity)?;
        self.storage.objects().put(OBJ_KIND, id, updated).await
    }

    /// Get pipeline statistics for a session.
    pub async fn stats(&self, session_id: &SessionId) -> Result<PipelineStats> {
        let opportunities = self.list(session_id, None).await?;

        let mut by_stage: HashMap<String, usize> = HashMap::new();
        for opp in &opportunities {
            let stage_name = serde_json::to_value(&opp.stage)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", opp.stage));
            *by_stage.entry(stage_name).or_default() += 1;
        }

        Ok(PipelineStats {
            total: opportunities.len(),
            by_stage,
        })
    }
}
