//! Pricing tier management.

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PricingTierId(Uuid);
impl Default for PricingTierId {
    fn default() -> Self {
        Self::new()
    }
}

impl PricingTierId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for PricingTierId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PricingTier {
    pub id: PricingTierId,
    pub session_id: SessionId,
    pub name: String,
    pub price_monthly: f64,
    pub price_yearly: Option<f64>,
    pub features: Vec<String>,
    pub is_active: bool,
    pub metadata: serde_json::Value,
}

const KIND: &str = "product_pricing_tier";

pub struct PricingManager {
    storage: Arc<dyn StoragePort>,
}

impl PricingManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn create_tier(
        &self,
        session_id: SessionId,
        name: String,
        price_monthly: f64,
        price_yearly: Option<f64>,
        features: Vec<String>,
    ) -> Result<PricingTierId> {
        let tier = PricingTier {
            id: PricingTierId::new(),
            session_id,
            name,
            price_monthly,
            price_yearly,
            features,
            is_active: true,
            metadata: serde_json::json!({}),
        };
        let id = tier.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&tier)?)
            .await?;
        Ok(id)
    }

    pub async fn list_tiers(&self, session_id: SessionId) -> Result<Vec<PricingTier>> {
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
