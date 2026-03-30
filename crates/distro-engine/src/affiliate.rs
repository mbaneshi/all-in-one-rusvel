//! Affiliate partner management.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "distro_partner";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PartnerId(Uuid);

impl Default for PartnerId {
    fn default() -> Self {
        Self::new()
    }
}

impl PartnerId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for PartnerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Partner {
    pub id: PartnerId,
    pub session_id: SessionId,
    pub name: String,
    pub commission_rate: f64,
    pub referrals: u64,
    pub revenue: f64,
    pub active: bool,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct AffiliateManager {
    storage: Arc<dyn StoragePort>,
}

impl AffiliateManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_partner(
        &self,
        session_id: SessionId,
        name: String,
        commission_rate: f64,
    ) -> Result<Partner> {
        let partner = Partner {
            id: PartnerId::new(),
            session_id,
            name,
            commission_rate,
            referrals: 0,
            revenue: 0.0,
            active: true,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&partner)?;
        self.storage
            .objects()
            .put(KIND, &partner.id.to_string(), json)
            .await?;
        Ok(partner)
    }

    pub async fn list_partners(&self, session_id: SessionId) -> Result<Vec<Partner>> {
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
