//! Deployment tracking.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "infra_deployment";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DeploymentId(Uuid);

impl Default for DeploymentId {
    fn default() -> Self {
        Self::new()
    }
}

impl DeploymentId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for DeploymentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeployStatus {
    Pending,
    Running,
    Success,
    Failed,
    Rolledback,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deployment {
    pub id: DeploymentId,
    pub session_id: SessionId,
    pub service: String,
    pub version: String,
    pub environment: String,
    pub status: DeployStatus,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct DeployManager {
    storage: Arc<dyn StoragePort>,
}

impl DeployManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn record_deployment(
        &self,
        session_id: SessionId,
        service: String,
        version: String,
        environment: String,
    ) -> Result<Deployment> {
        let deployment = Deployment {
            id: DeploymentId::new(),
            session_id,
            service,
            version,
            environment,
            status: DeployStatus::Pending,
            started_at: Utc::now(),
            completed_at: None,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&deployment)?;
        self.storage
            .objects()
            .put(KIND, &deployment.id.to_string(), json)
            .await?;
        Ok(deployment)
    }

    pub async fn list_deployments(&self, session_id: SessionId) -> Result<Vec<Deployment>> {
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
