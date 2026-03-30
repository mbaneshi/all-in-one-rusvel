//! Key performance indicators.

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
pub struct KpiId(Uuid);
impl Default for KpiId {
    fn default() -> Self {
        Self::new()
    }
}

impl KpiId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for KpiId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KpiEntry {
    pub id: KpiId,
    pub session_id: SessionId,
    pub name: String,
    pub value: f64,
    pub unit: String,
    pub recorded_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

const KIND: &str = "growth_kpi";

pub struct KpiManager {
    storage: Arc<dyn StoragePort>,
}

impl KpiManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn record_kpi(
        &self,
        session_id: SessionId,
        name: String,
        value: f64,
        unit: String,
    ) -> Result<KpiId> {
        let entry = KpiEntry {
            id: KpiId::new(),
            session_id,
            name,
            value,
            unit,
            recorded_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let id = entry.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&entry)?)
            .await?;
        Ok(id)
    }

    pub async fn list_kpis(&self, session_id: SessionId) -> Result<Vec<KpiEntry>> {
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
