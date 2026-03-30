//! Incident management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "infra_incident";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IncidentId(Uuid);

impl Default for IncidentId {
    fn default() -> Self {
        Self::new()
    }
}

impl IncidentId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for IncidentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    P1,
    P2,
    P3,
    P4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IncidentStatus {
    Open,
    Investigating,
    Mitigated,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: IncidentId,
    pub session_id: SessionId,
    pub title: String,
    pub description: String,
    pub severity: Severity,
    pub status: IncidentStatus,
    pub opened_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct IncidentManager {
    storage: Arc<dyn StoragePort>,
}

impl IncidentManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn open_incident(
        &self,
        session_id: SessionId,
        title: String,
        description: String,
        severity: Severity,
    ) -> Result<Incident> {
        let incident = Incident {
            id: IncidentId::new(),
            session_id,
            title,
            description,
            severity,
            status: IncidentStatus::Open,
            opened_at: Utc::now(),
            resolved_at: None,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&incident)?;
        self.storage
            .objects()
            .put(KIND, &incident.id.to_string(), json)
            .await?;
        Ok(incident)
    }

    pub async fn list_incidents(&self, session_id: SessionId) -> Result<Vec<Incident>> {
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

    pub async fn resolve(&self, id: &IncidentId) -> Result<()> {
        let val = self.storage.objects().get(KIND, &id.to_string()).await?;
        match val {
            Some(v) => {
                let mut incident: Incident = serde_json::from_value(v)?;
                incident.status = IncidentStatus::Resolved;
                incident.resolved_at = Some(Utc::now());
                let json = serde_json::to_value(&incident)?;
                self.storage
                    .objects()
                    .put(KIND, &id.to_string(), json)
                    .await
            }
            None => Err(RusvelError::NotFound {
                kind: KIND.into(),
                id: id.to_string(),
            }),
        }
    }
}
