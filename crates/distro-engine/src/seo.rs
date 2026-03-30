//! SEO keyword tracking.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "distro_keyword";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct KeywordId(Uuid);

impl Default for KeywordId {
    fn default() -> Self {
        Self::new()
    }
}

impl KeywordId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for KeywordId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Keyword {
    pub id: KeywordId,
    pub session_id: SessionId,
    pub term: String,
    pub position: u32,
    pub volume: u64,
    pub url: String,
    pub tracked_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct SeoManager {
    storage: Arc<dyn StoragePort>,
}

impl SeoManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_keyword(
        &self,
        session_id: SessionId,
        term: String,
        url: String,
    ) -> Result<Keyword> {
        let keyword = Keyword {
            id: KeywordId::new(),
            session_id,
            term,
            position: 0,
            volume: 0,
            url,
            tracked_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&keyword)?;
        self.storage
            .objects()
            .put(KIND, &keyword.id.to_string(), json)
            .await?;
        Ok(keyword)
    }

    pub async fn list_keywords(&self, session_id: SessionId) -> Result<Vec<Keyword>> {
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
