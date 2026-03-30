//! Cohort retention analysis.

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
pub struct CohortId(Uuid);
impl Default for CohortId {
    fn default() -> Self {
        Self::new()
    }
}

impl CohortId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for CohortId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cohort {
    pub id: CohortId,
    pub session_id: SessionId,
    pub name: String,
    pub start_date: DateTime<Utc>,
    pub size: u64,
    pub retention_rates: Vec<f64>,
    pub metadata: serde_json::Value,
}

const KIND: &str = "growth_cohort";

pub struct CohortManager {
    storage: Arc<dyn StoragePort>,
}

impl CohortManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn create_cohort(
        &self,
        session_id: SessionId,
        name: String,
        size: u64,
    ) -> Result<CohortId> {
        let c = Cohort {
            id: CohortId::new(),
            session_id,
            name,
            start_date: Utc::now(),
            size,
            retention_rates: vec![],
            metadata: serde_json::json!({}),
        };
        let id = c.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&c)?)
            .await?;
        Ok(id)
    }

    pub async fn list_cohorts(&self, session_id: SessionId) -> Result<Vec<Cohort>> {
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
}
