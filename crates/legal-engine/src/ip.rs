//! Intellectual property asset management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "legal_ip";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct IpAssetId(Uuid);

impl Default for IpAssetId {
    fn default() -> Self {
        Self::new()
    }
}

impl IpAssetId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for IpAssetId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IpKind {
    Patent,
    Trademark,
    Copyright,
    TradeSecret,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpAsset {
    pub id: IpAssetId,
    pub session_id: SessionId,
    pub kind: IpKind,
    pub name: String,
    pub description: String,
    pub filed_at: DateTime<Utc>,
    pub status: String,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct IpManager {
    storage: Arc<dyn StoragePort>,
}

impl IpManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn file_asset(
        &self,
        session_id: SessionId,
        kind: IpKind,
        name: String,
        description: String,
    ) -> Result<IpAsset> {
        let asset = IpAsset {
            id: IpAssetId::new(),
            session_id,
            kind,
            name,
            description,
            filed_at: Utc::now(),
            status: "Filed".into(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&asset)?;
        self.storage
            .objects()
            .put(KIND, &asset.id.to_string(), json)
            .await?;
        Ok(asset)
    }

    pub async fn list_assets(&self, session_id: SessionId) -> Result<Vec<IpAsset>> {
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
