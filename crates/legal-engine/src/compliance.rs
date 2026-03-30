//! Compliance check tracking.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "legal_compliance";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ComplianceCheckId(Uuid);

impl Default for ComplianceCheckId {
    fn default() -> Self {
        Self::new()
    }
}

impl ComplianceCheckId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for ComplianceCheckId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ComplianceArea {
    GDPR,
    Privacy,
    Licensing,
    Tax,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceCheck {
    pub id: ComplianceCheckId,
    pub session_id: SessionId,
    pub area: ComplianceArea,
    pub description: String,
    pub passed: bool,
    pub checked_at: DateTime<Utc>,
    pub notes: String,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct ComplianceManager {
    storage: Arc<dyn StoragePort>,
}

impl ComplianceManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_check(
        &self,
        session_id: SessionId,
        area: ComplianceArea,
        description: String,
        passed: bool,
        notes: String,
    ) -> Result<ComplianceCheck> {
        let check = ComplianceCheck {
            id: ComplianceCheckId::new(),
            session_id,
            area,
            description,
            passed,
            checked_at: Utc::now(),
            notes,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&check)?;
        self.storage
            .objects()
            .put(KIND, &check.id.to_string(), json)
            .await?;
        Ok(check)
    }

    pub async fn list_checks(&self, session_id: SessionId) -> Result<Vec<ComplianceCheck>> {
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
