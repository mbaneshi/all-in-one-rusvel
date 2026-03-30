//! Marketplace listing management.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "distro_listing";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ListingId(Uuid);

impl Default for ListingId {
    fn default() -> Self {
        Self::new()
    }
}

impl ListingId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for ListingId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListingStatus {
    Draft,
    Published,
    Suspended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Listing {
    pub id: ListingId,
    pub session_id: SessionId,
    pub platform: String,
    pub name: String,
    pub url: String,
    pub status: ListingStatus,
    pub downloads: u64,
    pub revenue: f64,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct MarketplaceManager {
    storage: Arc<dyn StoragePort>,
}

impl MarketplaceManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_listing(
        &self,
        session_id: SessionId,
        platform: String,
        name: String,
        url: String,
    ) -> Result<Listing> {
        let listing = Listing {
            id: ListingId::new(),
            session_id,
            platform,
            name,
            url,
            status: ListingStatus::Draft,
            downloads: 0,
            revenue: 0.0,
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&listing)?;
        self.storage
            .objects()
            .put(KIND, &listing.id.to_string(), json)
            .await?;
        Ok(listing)
    }

    pub async fn list_listings(&self, session_id: SessionId) -> Result<Vec<Listing>> {
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
