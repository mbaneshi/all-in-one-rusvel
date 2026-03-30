//! Content scheduling via the central job queue.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::*;
use rusvel_core::ports::{JobPort, StoragePort};
use serde::{Deserialize, Serialize};

/// A post scheduled for future publication.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledPost {
    pub content_id: ContentId,
    pub platform: Platform,
    pub publish_at: DateTime<Utc>,
    pub status: ContentStatus,
}

/// Manages content scheduling through the central job queue.
pub struct ContentCalendar {
    storage: Arc<dyn StoragePort>,
    jobs: Arc<dyn JobPort>,
}

impl ContentCalendar {
    pub fn new(storage: Arc<dyn StoragePort>, jobs: Arc<dyn JobPort>) -> Self {
        Self { storage, jobs }
    }

    /// Schedule a content item for future publication on the given platform.
    pub async fn schedule(
        &self,
        content_id: ContentId,
        platform: Platform,
        publish_at: DateTime<Utc>,
        session_id: SessionId,
    ) -> Result<()> {
        // Persist the schedule metadata on the content item.
        let json = self
            .storage
            .objects()
            .get("content", &content_id.to_string())
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "ContentItem".into(),
                id: content_id.to_string(),
            })?;
        let mut item: ContentItem = serde_json::from_value(json)?;
        item.status = ContentStatus::Scheduled;
        item.scheduled_at = Some(publish_at);
        if !item.platform_targets.contains(&platform) {
            item.platform_targets.push(platform.clone());
        }
        self.storage
            .objects()
            .put(
                "content",
                &content_id.to_string(),
                serde_json::to_value(&item)?,
            )
            .await?;

        // Enqueue a job for the publish worker.
        let payload = serde_json::json!({
            "session_id": session_id.to_string(),
            "content_id": content_id.to_string(),
            "platform": platform,
            "publish_at": publish_at,
        });
        let new_job = NewJob {
            session_id,
            kind: JobKind::ContentPublish,
            payload,
            max_retries: 3,
            metadata: serde_json::json!({}),
            scheduled_at: None,
        };
        self.jobs.enqueue(new_job).await?;
        Ok(())
    }

    /// List scheduled posts whose `publish_at` falls within `[from, to]` (inclusive).
    pub async fn list_scheduled_in_range(
        &self,
        session_id: &SessionId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<ScheduledPost>> {
        let all = self.list_scheduled(session_id).await?;
        Ok(all
            .into_iter()
            .filter(|p| p.publish_at >= from && p.publish_at <= to)
            .collect())
    }

    /// List all scheduled (not-yet-published) posts for a session.
    pub async fn list_scheduled(&self, session_id: &SessionId) -> Result<Vec<ScheduledPost>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(*session_id),
            tags: vec![],
            limit: None,
            offset: None,
        };
        let items = self.storage.objects().list("content", filter).await?;
        let mut result = Vec::new();
        for val in items {
            let item: ContentItem = serde_json::from_value(val)?;
            if item.status == ContentStatus::Scheduled {
                let platform = item
                    .platform_targets
                    .first()
                    .cloned()
                    .unwrap_or(Platform::Custom("unknown".into()));
                result.push(ScheduledPost {
                    content_id: item.id,
                    platform,
                    publish_at: item.scheduled_at.unwrap_or_else(Utc::now),
                    status: item.status,
                });
            }
        }
        Ok(result)
    }

    /// Cancel a previously scheduled post by reverting it to Draft.
    pub async fn cancel(&self, content_id: ContentId) -> Result<()> {
        let json = self
            .storage
            .objects()
            .get("content", &content_id.to_string())
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "ContentItem".into(),
                id: content_id.to_string(),
            })?;
        let mut item: ContentItem = serde_json::from_value(json)?;
        item.status = ContentStatus::Draft;
        item.scheduled_at = None;
        self.storage
            .objects()
            .put(
                "content",
                &content_id.to_string(),
                serde_json::to_value(&item)?,
            )
            .await
    }
}
