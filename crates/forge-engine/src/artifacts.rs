//! Persisted doc artifacts for Forge (S-049) — stored in [`ObjectStore`] kind [`ARTIFACT_KIND`].

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusvel_core::domain::ObjectFilter;
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};

pub const ARTIFACT_KIND: &str = "forge_artifacts";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub id: String,
    pub session_id: SessionId,
    pub title: String,
    pub body_markdown: String,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

pub async fn save_artifact(
    storage: &Arc<dyn StoragePort>,
    session_id: SessionId,
    title: &str,
    body_markdown: &str,
) -> Result<ArtifactRecord> {
    let id = uuid::Uuid::now_v7().to_string();
    let record = ArtifactRecord {
        id: id.clone(),
        session_id,
        title: title.to_string(),
        body_markdown: body_markdown.to_string(),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };
    let json = serde_json::to_value(&record)?;
    storage.objects().put(ARTIFACT_KIND, &id, json).await?;
    Ok(record)
}

pub async fn list_artifacts(
    storage: &Arc<dyn StoragePort>,
    session_id: &SessionId,
    limit: u32,
) -> Result<Vec<ArtifactRecord>> {
    let rows = storage
        .objects()
        .list(
            ARTIFACT_KIND,
            ObjectFilter {
                conversation_id: None,
                session_id: Some(*session_id),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await?;
    let mut out: Vec<ArtifactRecord> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}
