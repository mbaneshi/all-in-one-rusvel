//! User feedback collection.

use chrono::{DateTime, Utc};
use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct FeedbackId(Uuid);
impl Default for FeedbackId {
    fn default() -> Self {
        Self::new()
    }
}

impl FeedbackId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}
impl std::fmt::Display for FeedbackId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedbackKind {
    FeatureRequest,
    Bug,
    Praise,
    Complaint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Feedback {
    pub id: FeedbackId,
    pub session_id: SessionId,
    pub source: String,
    pub kind: FeedbackKind,
    pub content: String,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

const KIND: &str = "product_feedback";

pub struct FeedbackManager {
    storage: Arc<dyn StoragePort>,
}

impl FeedbackManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_feedback(
        &self,
        session_id: SessionId,
        source: String,
        kind: FeedbackKind,
        content: String,
    ) -> Result<FeedbackId> {
        let fb = Feedback {
            id: FeedbackId::new(),
            session_id,
            source,
            kind,
            content,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let id = fb.id;
        self.storage
            .objects()
            .put(KIND, &id.to_string(), serde_json::to_value(&fb)?)
            .await?;
        Ok(id)
    }

    pub async fn list_feedback(&self, session_id: SessionId) -> Result<Vec<Feedback>> {
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
