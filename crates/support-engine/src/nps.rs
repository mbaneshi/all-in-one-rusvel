//! Net Promoter Score tracking.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "support_nps";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NpsResponseId(Uuid);

impl Default for NpsResponseId {
    fn default() -> Self {
        Self::new()
    }
}

impl NpsResponseId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for NpsResponseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NpsResponse {
    pub id: NpsResponseId,
    pub session_id: SessionId,
    pub score: i32,
    pub comment: String,
    pub source: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct NpsManager {
    storage: Arc<dyn StoragePort>,
}

impl NpsManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn record_response(
        &self,
        session_id: SessionId,
        score: i32,
        comment: String,
        source: String,
    ) -> Result<NpsResponse> {
        let response = NpsResponse {
            id: NpsResponseId::new(),
            session_id,
            score,
            comment,
            source,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&response)?;
        self.storage
            .objects()
            .put(KIND, &response.id.to_string(), json)
            .await?;
        Ok(response)
    }

    pub async fn list_responses(&self, session_id: SessionId) -> Result<Vec<NpsResponse>> {
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

    /// Calculate NPS score: (promoters - detractors) / total * 100
    /// Promoters: score 9-10, Detractors: score 0-6, Passives: 7-8
    pub async fn calculate_nps(&self, session_id: SessionId) -> Result<f64> {
        let responses = self.list_responses(session_id).await?;
        if responses.is_empty() {
            return Ok(0.0);
        }
        let total = responses.len() as f64;
        let promoters = responses.iter().filter(|r| r.score >= 9).count() as f64;
        let detractors = responses.iter().filter(|r| r.score <= 6).count() as f64;
        Ok((promoters - detractors) / total * 100.0)
    }
}
