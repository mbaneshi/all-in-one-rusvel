//! Knowledge base article management.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;

const KIND: &str = "support_article";

// ── Domain types ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArticleId(Uuid);

impl Default for ArticleId {
    fn default() -> Self {
        Self::new()
    }
}

impl ArticleId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl std::fmt::Display for ArticleId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: ArticleId,
    pub session_id: SessionId,
    pub title: String,
    pub content: String,
    pub tags: Vec<String>,
    pub published: bool,
    pub created_at: DateTime<Utc>,
    pub metadata: serde_json::Value,
}

// ── Manager ───────────────────────────────────────────────────────

pub struct KnowledgeManager {
    storage: Arc<dyn StoragePort>,
}

impl KnowledgeManager {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    pub async fn add_article(
        &self,
        session_id: SessionId,
        title: String,
        content: String,
        tags: Vec<String>,
    ) -> Result<Article> {
        let article = Article {
            id: ArticleId::new(),
            session_id,
            title,
            content,
            tags,
            published: false,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let json = serde_json::to_value(&article)?;
        self.storage
            .objects()
            .put(KIND, &article.id.to_string(), json)
            .await?;
        Ok(article)
    }

    pub async fn list_articles(&self, session_id: SessionId) -> Result<Vec<Article>> {
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
