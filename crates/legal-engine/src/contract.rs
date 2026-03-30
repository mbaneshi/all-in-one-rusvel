//! Contract lifecycle management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "legal_contract";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ContractId(Uuid);

impl Default for ContractId {
    fn default() -> Self {
        Self::new()
    }
}

impl ContractId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for ContractId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContractStatus {
    Draft,
    Sent,
    Signed,
    Expired,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: ContractId,
    pub session_id: SessionId,
    pub title: String,
    pub counterparty: String,
    pub status: ContractStatus,
    pub template: String,
    pub signed_at: Option<DateTime<Utc>>,
    pub expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct ContractManager {
    storage: Arc<dyn StoragePort>,
}

impl ContractManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn create_contract(
        &self,
        session_id: SessionId,
        title: String,
        counterparty: String,
        template: String,
    ) -> Result<Contract> {
        let contract = Contract {
            id: ContractId::new(),
            session_id,
            title,
            counterparty,
            status: ContractStatus::Draft,
            template,
            signed_at: None,
            expires_at: None,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&contract)?;
        self.storage
            .objects()
            .put(KIND, &contract.id.to_string(), json)
            .await?;
        Ok(contract)
    }

    pub async fn list_contracts(&self, session_id: SessionId) -> Result<Vec<Contract>> {
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
