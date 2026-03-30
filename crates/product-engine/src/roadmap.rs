//! Feature and milestone management.

use chrono::{DateTime, Utc};
use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeatureId(Uuid);
impl Default for FeatureId {
    fn default() -> Self {
        Self::new()
    }
}

impl FeatureId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for FeatureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureStatus {
    Planned,
    InProgress,
    Done,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feature {
    pub id: FeatureId,
    pub session_id: SessionId,
    pub name: String,
    pub description: String,
    pub priority: Priority,
    pub status: FeatureStatus,
    pub milestone: Option<String>,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

const KIND: &str = "product_feature";

pub struct RoadmapManager {
    storage: Arc<dyn StoragePort>,
}

impl RoadmapManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_feature(
        &self,
        session_id: SessionId,
        name: String,
        description: String,
        priority: Priority,
        milestone: Option<String>,
    ) -> Result<FeatureId> {
        let f = Feature {
            id: FeatureId::new(),
            session_id,
            name,
            description,
            priority,
            status: FeatureStatus::Planned,
            milestone,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let id = f.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&f)?)
            .await?;
        Ok(id)
    }

    pub async fn list_features(&self, session_id: SessionId) -> Result<Vec<Feature>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(session_id),
            ..Default::default()
        };
        let vals = self.storage.objects().list(KIND, filter).await?;
        vals.into_iter()
            .map(|v| Ok(serde_json::from_value(v)?))
            .collect()
    }

    pub async fn update_status(&self, id: &FeatureId, status: FeatureStatus) -> Result<()> {
        let val = self.storage.objects().get(KIND, &id.to_string()).await?;
        if let Some(v) = val {
            let mut f: Feature = serde_json::from_value(v)?;
            f.status = status;
            self.storage
                .objects()
                .put(KIND, &id.to_string(), serde_json::to_value(&f)?)
                .await?;
        }
        Ok(())
    }
}
