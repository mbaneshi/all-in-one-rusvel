//! Health check monitoring.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "infra_health_check";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct HealthCheckId(Uuid);

impl Default for HealthCheckId {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthCheckId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for HealthCheckId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    Healthy,
    Degraded,
    Down,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub id: HealthCheckId,
    pub session_id: SessionId,
    pub service: String,
    pub url: String,
    pub status: CheckStatus,
    pub response_time_ms: u64,
    pub checked_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct MonitorManager {
    storage: Arc<dyn StoragePort>,
}

impl MonitorManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_check(
        &self,
        session_id: SessionId,
        service: String,
        url: String,
        status: CheckStatus,
        response_time_ms: u64,
    ) -> Result<HealthCheck> {
        let check = HealthCheck {
            id: HealthCheckId::new(),
            session_id,
            service,
            url,
            status,
            response_time_ms,
            checked_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&check)?;
        self.storage
            .objects()
            .put(KIND, &check.id.to_string(), json)
            .await?;
        Ok(check)
    }

    pub async fn list_checks(&self, session_id: SessionId) -> Result<Vec<HealthCheck>> {
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
