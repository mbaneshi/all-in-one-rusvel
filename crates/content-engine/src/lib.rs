//! Content Engine — creation, adaptation, scheduling, and publishing.
//!
//! Orchestrates AI-powered content writing, multi-platform adaptation,
//! calendar scheduling, and engagement analytics.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusvel_core::domain::*;
use rusvel_core::engine::Engine;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::*;
use rusvel_core::ports::{AgentPort, EventPort, JobPort, StoragePort};

pub mod adapters;
pub mod analytics;
pub mod avalai_media;
pub mod calendar;
pub mod code_bridge;
pub mod media_gen;
pub mod platform;
pub mod writer;

pub use adapters::devto::DevToAdapter;
pub use analytics::ContentAnalytics;
pub use avalai_media::AvalAiMediaGen;
pub use calendar::{ContentCalendar, ScheduledPost};
pub use media_gen::{
    GeneratedImage, ImageGenRequest, ImageGenResult, ImagePayload, MediaGenPort, MockMediaGen,
};
pub use platform::{MockPlatformAdapter, PlatformAdapter, PostMetrics, PublishResult};
pub use writer::{ContentReview, ContentWriter, build_code_prompt};

// ════════════════════════════════════════════════════════════════════
//  Event constants
// ════════════════════════════════════════════════════════════════════

pub mod events {
    pub const CONTENT_DRAFTED: &str = "content.drafted";
    pub const CONTENT_ADAPTED: &str = "content.adapted";
    pub const CONTENT_SCHEDULED: &str = "content.scheduled";
    pub const CONTENT_PUBLISHED: &str = "content.published";
    pub const CONTENT_REVIEWED: &str = "content.reviewed";
    pub const CONTENT_CANCELLED: &str = "content.cancelled";
    pub const METRICS_RECORDED: &str = "content.metrics_recorded";
}

// ════════════════════════════════════════════════════════════════════
//  ContentEngine
// ════════════════════════════════════════════════════════════════════

/// Central content engine that wires together writing, publishing,
/// scheduling, and analytics.
pub struct ContentEngine {
    storage: Arc<dyn StoragePort>,
    event_bus: Arc<dyn EventPort>,
    writer: ContentWriter,
    calendar: ContentCalendar,
    analytics: ContentAnalytics,
    adapters: std::sync::Mutex<HashMap<String, Arc<dyn PlatformAdapter>>>,
    media_gen: std::sync::Mutex<Option<Arc<dyn MediaGenPort>>>,
}

impl ContentEngine {
    pub fn new(
        storage: Arc<dyn StoragePort>,
        event_bus: Arc<dyn EventPort>,
        agent: Arc<dyn AgentPort>,
        jobs: Arc<dyn JobPort>,
    ) -> Self {
        Self {
            writer: ContentWriter::new(agent),
            calendar: ContentCalendar::new(Arc::clone(&storage), jobs),
            analytics: ContentAnalytics::new(Arc::clone(&storage)),
            adapters: std::sync::Mutex::new(HashMap::new()),
            media_gen: std::sync::Mutex::new(None),
            storage,
            event_bus,
        }
    }

    /// Set voice/style rules injected into all content generation prompts.
    /// Typically loaded from the content department's rules in ObjectStore.
    pub fn set_voice_rules(&self, rules: impl Into<String>) {
        self.writer.set_voice_rules(rules);
    }

    /// Register a platform adapter for publishing.
    pub fn register_platform(&self, adapter: Arc<dyn PlatformAdapter>) {
        let key = format!("{:?}", adapter.platform());
        self.adapters.lock().unwrap().insert(key, adapter);
    }

    /// Register the media-generation adapter (e.g. [`AvalAiMediaGen`]) used
    /// for image drafts. Optional — [`Self::generate_image`] errors clearly
    /// if nothing is registered rather than silently no-op-ing.
    pub fn register_media_gen(&self, adapter: Arc<dyn MediaGenPort>) {
        *self.media_gen.lock().unwrap() = Some(adapter);
    }

    /// Generate an image via the registered media-gen adapter.
    pub async fn generate_image(&self, request: ImageGenRequest) -> Result<ImageGenResult> {
        let adapter = self.media_gen.lock().unwrap().clone().ok_or_else(|| {
            RusvelError::Validation(
                "no media-gen adapter registered — call register_media_gen first".into(),
            )
        })?;
        adapter.generate_image(request).await
    }

    fn get_adapter(&self, platform: &Platform) -> Result<Arc<dyn PlatformAdapter>> {
        let key = format!("{platform:?}");
        self.adapters
            .lock()
            .unwrap()
            .get(&key)
            .cloned()
            .ok_or_else(|| RusvelError::NotFound {
                kind: "PlatformAdapter".into(),
                id: key,
            })
    }

    // ── Domain operations ──────────────────────────────────────────

    /// Draft new content via AI.
    pub async fn draft(
        &self,
        session_id: &SessionId,
        topic: &str,
        kind: ContentKind,
    ) -> Result<ContentItem> {
        let item = self.writer.draft(session_id, topic, kind).await?;
        self.storage
            .objects()
            .put(
                "content",
                &item.id.to_string(),
                serde_json::to_value(&item)?,
            )
            .await?;
        self.emit(events::CONTENT_DRAFTED, Some(*session_id), &item.id)
            .await?;
        Ok(item)
    }

    /// Adapt existing content for a target platform.
    pub async fn adapt(
        &self,
        session_id: &SessionId,
        content_id: ContentId,
        platform: Platform,
    ) -> Result<ContentItem> {
        let json = self.load_content(&content_id).await?;
        let original: ContentItem = serde_json::from_value(json)?;
        let max_len = self
            .get_adapter(&platform)
            .ok()
            .and_then(|a| a.max_length());
        let adapted_body = self
            .writer
            .adapt(&original, platform.clone(), max_len)
            .await?;

        let mut adapted = original;
        adapted.id = ContentId::new();
        adapted.session_id = *session_id;
        adapted.body_markdown = adapted_body;
        adapted.status = ContentStatus::Adapted;
        adapted.platform_targets = vec![platform];
        self.storage
            .objects()
            .put(
                "content",
                &adapted.id.to_string(),
                serde_json::to_value(&adapted)?,
            )
            .await?;
        self.emit(events::CONTENT_ADAPTED, Some(*session_id), &adapted.id)
            .await?;
        Ok(adapted)
    }

    /// Publish content to a platform. Requires approval status == Approved.
    pub async fn publish(
        &self,
        session_id: &SessionId,
        content_id: ContentId,
        platform: Platform,
    ) -> Result<PublishResult> {
        let json = self.load_content(&content_id).await?;
        let mut item: ContentItem = serde_json::from_value(json)?;

        if item.approval != ApprovalStatus::Approved
            && item.approval != ApprovalStatus::AutoApproved
        {
            return Err(RusvelError::Validation(
                "Content must be approved before publishing".into(),
            ));
        }

        let adapter = self.get_adapter(&platform)?;
        let result = adapter.publish(&item).await?;

        item.status = ContentStatus::Published;
        item.published_at = Some(result.published_at);
        self.storage
            .objects()
            .put(
                "content",
                &content_id.to_string(),
                serde_json::to_value(&item)?,
            )
            .await?;
        self.emit(events::CONTENT_PUBLISHED, Some(*session_id), &content_id)
            .await?;
        Ok(result)
    }

    /// Schedule content for future publication.
    pub async fn schedule(
        &self,
        session_id: &SessionId,
        content_id: ContentId,
        platform: Platform,
        at: DateTime<Utc>,
    ) -> Result<()> {
        self.calendar
            .schedule(content_id, platform.clone(), at, *session_id)
            .await?;
        self.emit_with_payload(
            events::CONTENT_SCHEDULED,
            Some(*session_id),
            &content_id,
            serde_json::json!({
                "platform": platform,
                "publish_at": at.to_rfc3339(),
            }),
        )
        .await?;
        Ok(())
    }

    /// Alias for [`Self::schedule`] — matches “schedule a draft” wording in product specs.
    pub async fn schedule_draft(
        &self,
        session_id: &SessionId,
        draft_id: ContentId,
        platform: Platform,
        publish_at: DateTime<Utc>,
    ) -> Result<()> {
        self.schedule(session_id, draft_id, platform, publish_at)
            .await
    }

    /// Mark a content item as human-approved (ADR-008 content gate before publish).
    pub async fn approve_content(&self, content_id: ContentId) -> Result<ContentItem> {
        let json = self.load_content(&content_id).await?;
        let mut item: ContentItem = serde_json::from_value(json)?;
        item.approval = ApprovalStatus::Approved;
        self.storage
            .objects()
            .put(
                "content",
                &content_id.to_string(),
                serde_json::to_value(&item)?,
            )
            .await?;
        Ok(item)
    }

    /// Draft a blog post from a stored `code_analysis` snapshot (responds to `code.analyzed`).
    pub async fn draft_blog_from_code_snapshot(
        &self,
        session_id: &SessionId,
        snapshot_id: &str,
    ) -> Result<ContentItem> {
        let json = self
            .storage
            .objects()
            .get("code_analysis", snapshot_id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "code_analysis".into(),
                id: snapshot_id.into(),
            })?;
        let summary = code_bridge::summary_from_stored_code_analysis(&json)?;
        let topic = crate::writer::build_code_prompt(&summary, &ContentKind::Blog);
        self.draft(session_id, &topic, ContentKind::Blog).await
    }

    /// Execute a queued content publish job (payload: `content_id`, `platform`).
    pub async fn execute_content_publish_job(&self, job: Job) -> Result<serde_json::Value> {
        match &job.kind {
            JobKind::ContentPublish => {}
            JobKind::Custom(s) if s == "content.publish" => {}
            _ => {
                return Err(RusvelError::Validation(format!(
                    "expected ContentPublish job, got {:?}",
                    job.kind
                )));
            }
        }
        let sid = job.session_id;
        let content_id_str = job
            .payload
            .get("content_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let platform_str = job
            .payload
            .get("platform")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let cid = content_id_str
            .parse::<uuid::Uuid>()
            .map(ContentId::from_uuid)
            .map_err(|e| RusvelError::Validation(format!("content_id: {e}")))?;
        let platform: Platform = serde_json::from_value(serde_json::json!(platform_str))
            .map_err(|e| RusvelError::Validation(format!("platform: {e}")))?;
        let result = self.publish(&sid, cid, platform).await?;
        serde_json::to_value(&result).map_err(|e| RusvelError::Serialization(e.to_string()))
    }

    /// List content items, optionally filtered by status.
    pub async fn list_content(
        &self,
        session_id: &SessionId,
        status_filter: Option<ContentStatus>,
    ) -> Result<Vec<ContentItem>> {
        let filter = ObjectFilter {
            conversation_id: None,
            session_id: Some(*session_id),
            tags: vec![],
            limit: None,
            offset: None,
        };
        let all = self.storage.objects().list("content", filter).await?;
        let mut items: Vec<ContentItem> = all
            .into_iter()
            .filter_map(|v| serde_json::from_value(v).ok())
            .collect();
        if let Some(status) = status_filter {
            items.retain(|i| i.status == status);
        }
        Ok(items)
    }

    /// Scheduled posts (content calendar).
    pub async fn list_scheduled(&self, session_id: &SessionId) -> Result<Vec<ScheduledPost>> {
        self.calendar.list_scheduled(session_id).await
    }

    /// Scheduled posts within a date range (inclusive).
    pub async fn list_scheduled_in_range(
        &self,
        session_id: &SessionId,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<ScheduledPost>> {
        self.calendar
            .list_scheduled_in_range(session_id, from, to)
            .await
    }

    /// Get engagement metrics for a content item.
    pub async fn get_metrics(&self, content_id: ContentId) -> Result<Vec<(Platform, PostMetrics)>> {
        self.analytics.get_metrics(content_id).await
    }

    // ── Internal helpers ───────────────────────────────────────────

    async fn load_content(&self, id: &ContentId) -> Result<serde_json::Value> {
        self.storage
            .objects()
            .get("content", &id.to_string())
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "ContentItem".into(),
                id: id.to_string(),
            })
    }

    async fn emit(
        &self,
        kind: &str,
        session_id: Option<SessionId>,
        content_id: &ContentId,
    ) -> Result<()> {
        self.emit_with_payload(kind, session_id, content_id, serde_json::json!({}))
            .await
    }

    async fn emit_with_payload(
        &self,
        kind: &str,
        session_id: Option<SessionId>,
        content_id: &ContentId,
        extra: serde_json::Value,
    ) -> Result<()> {
        let mut payload = serde_json::json!({ "content_id": content_id.to_string() });
        if let (Some(po), Some(eo)) = (payload.as_object_mut(), extra.as_object()) {
            for (k, v) in eo {
                po.insert(k.clone(), v.clone());
            }
        }
        let event = Event {
            id: EventId::new(),
            session_id,
            run_id: None,
            source: "content".into(),
            kind: kind.into(),
            payload,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        self.event_bus.emit(event).await?;
        Ok(())
    }
}

#[async_trait]
impl Engine for ContentEngine {
    fn kind(&self) -> &str {
        "content"
    }
    fn name(&self) -> &'static str {
        "Content Engine"
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::ContentCreation]
    }
    async fn initialize(&self) -> Result<()> {
        Ok(())
    }
    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }
    async fn health(&self) -> Result<HealthStatus> {
        Ok(HealthStatus {
            healthy: true,
            message: None,
            metadata: serde_json::json!({}),
        })
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests;
