//! Engagement tracking and performance analytics for published content.

use std::sync::Arc;

use rusvel_core::domain::*;
use rusvel_core::error::Result;
use rusvel_core::id::*;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};

use crate::platform::PostMetrics;

/// Stored metrics snapshot for a content item on a specific platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentMetricsRecord {
    pub content_id: ContentId,
    pub platform: Platform,
    pub metrics: PostMetrics,
}

/// Tracks and queries engagement metrics for published content.
pub struct ContentAnalytics {
    storage: Arc<dyn StoragePort>,
}

impl ContentAnalytics {
    pub fn new(storage: Arc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    /// Record engagement metrics for a content item on a platform.
    pub async fn record_metrics(
        &self,
        content_id: ContentId,
        platform: Platform,
        metrics: PostMetrics,
    ) -> Result<()> {
        let record = ContentMetricsRecord {
            content_id,
            platform: platform.clone(),
            metrics,
        };
        let key = format!("{content_id}:{platform:?}");
        self.storage
            .objects()
            .put("content_metrics", &key, serde_json::to_value(&record)?)
            .await
    }

    /// Get all per-platform metrics for a content item.
    pub async fn get_metrics(&self, content_id: ContentId) -> Result<Vec<(Platform, PostMetrics)>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: None,
            tags: vec![],
            limit: None,
            offset: None,
        };
        let all = self
            .storage
            .objects()
            .list("content_metrics", filter)
            .await?;
        let mut result = Vec::new();
        for val in all {
            if let Ok(rec) = serde_json::from_value::<ContentMetricsRecord>(val)
                && rec.content_id == content_id
            {
                result.push((rec.platform, rec.metrics));
            }
        }
        Ok(result)
    }

    /// Return the top-performing content items by total engagement.
    pub async fn top_performing(
        &self,
        session_id: &SessionId,
        limit: usize,
    ) -> Result<Vec<(ContentItem, PostMetrics)>> {
        // Load all content items for this session.
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(*session_id),
            tags: vec![],
            limit: None,
            offset: None,
        };
        let items_json = self.storage.objects().list("content", filter).await?;
        let items: Vec<ContentItem> = items_json
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();

        // Aggregate metrics per content item.
        let mut scored: Vec<(ContentItem, PostMetrics)> = Vec::new();
        for item in items {
            let per_platform = self.get_metrics(item.id).await?;
            let combined = per_platform
                .iter()
                .fold(PostMetrics::default(), |mut acc, (_, m)| {
                    acc.views += m.views;
                    acc.likes += m.likes;
                    acc.comments += m.comments;
                    acc.shares += m.shares;
                    acc
                });
            scored.push((item, combined));
        }
        scored.sort_by(|a, b| {
            let score_a = a.1.views + a.1.likes * 5 + a.1.comments * 10 + a.1.shares * 15;
            let score_b = b.1.views + b.1.likes * 5 + b.1.comments * 10 + b.1.shares * 15;
            score_b.cmp(&score_a)
        });
        scored.truncate(limit);
        Ok(scored)
    }
}
