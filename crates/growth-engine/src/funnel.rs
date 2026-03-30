//! Conversion funnel tracking.

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FunnelStageId(Uuid);
impl Default for FunnelStageId {
    fn default() -> Self {
        Self::new()
    }
}

impl FunnelStageId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for FunnelStageId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunnelStage {
    pub id: FunnelStageId,
    pub session_id: SessionId,
    pub name: String,
    pub order: u32,
    pub visitors: u64,
    pub conversions: u64,
    pub metadata: serde_json::Value,
}

const KIND: &str = "growth_funnel_stage";

pub struct FunnelManager {
    storage: Arc<dyn StoragePort>,
}

impl FunnelManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_stage(
        &self,
        session_id: SessionId,
        name: String,
        order: u32,
    ) -> Result<FunnelStageId> {
        let stage = FunnelStage {
            id: FunnelStageId::new(),
            session_id,
            name,
            order,
            visitors: 0,
            conversions: 0,
            metadata: serde_json::json!({}),
        };
        let id = stage.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&stage)?)
            .await?;
        Ok(id)
    }

    pub async fn list_stages(&self, session_id: SessionId) -> Result<Vec<FunnelStage>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND, filter).await?;
        let mut stages: Vec<FunnelStage> = vals
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        stages.sort_by_key(|s| s.order);
        Ok(stages)
    }
}
