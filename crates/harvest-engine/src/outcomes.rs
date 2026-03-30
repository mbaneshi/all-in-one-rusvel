//! Record won/lost/withdrawn outcomes for opportunities to improve scoring prompts (S-044).
//!
//! Stored in [`ObjectStore`] kind `harvest_outcome`. When the host wires embedding + vector store
//! via `HarvestEngine::configure_rag`, outcomes are upserted for similarity retrieval during
//! scoring.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use rusvel_core::domain::{ObjectFilter, Opportunity};
use rusvel_core::error::Result;
use rusvel_core::id::SessionId;
use rusvel_core::ports::StoragePort;
use serde::{Deserialize, Serialize};

pub const HARVEST_OUTCOME_KIND: &str = "harvest_outcome";

/// Result of pursuing an opportunity (proposal / pipeline).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum HarvestDealOutcome {
    Won,
    Lost,
    Withdrawn,
}

/// Persisted outcome row (session-scoped for [`ObjectFilter::session_id`]).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HarvestOutcomeRecord {
    pub id: String,
    pub session_id: SessionId,
    pub opportunity_id: String,
    pub result: HarvestDealOutcome,
    #[serde(default)]
    pub notes: String,
    /// Snapshot for learning (title, url, score, skills, etc.).
    #[serde(default)]
    pub opportunity_snapshot: serde_json::Value,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

pub async fn record_outcome(
    storage: &Arc<dyn StoragePort>,
    session_id: &SessionId,
    opportunity_id: &str,
    result: HarvestDealOutcome,
    notes: String,
) -> Result<HarvestOutcomeRecord> {
    let value = storage
        .objects()
        .get("opportunity", opportunity_id)
        .await?
        .ok_or_else(|| rusvel_core::error::RusvelError::NotFound {
            kind: "opportunity".into(),
            id: opportunity_id.into(),
        })?;
    let opp: Opportunity = serde_json::from_value(value)?;
    let snapshot = serde_json::json!({
        "title": opp.title,
        "url": opp.url,
        "score": opp.score,
        "stage": format!("{:?}", opp.stage),
        "skills": opp.metadata.get("skills"),
    });

    let id = uuid::Uuid::now_v7().to_string();
    let record = HarvestOutcomeRecord {
        id: id.clone(),
        session_id: *session_id,
        opportunity_id: opportunity_id.to_string(),
        result,
        notes,
        opportunity_snapshot: snapshot,
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    };
    let json = serde_json::to_value(&record)?;
    storage
        .objects()
        .put(HARVEST_OUTCOME_KIND, &id, json)
        .await?;
    Ok(record)
}

pub async fn list_outcomes(
    storage: &Arc<dyn StoragePort>,
    session_id: &SessionId,
    limit: u32,
) -> Result<Vec<HarvestOutcomeRecord>> {
    let rows = storage
        .objects()
        .list(
            HARVEST_OUTCOME_KIND,
            ObjectFilter {
                conversation_id: None,
                session_id: Some(*session_id),
                limit: Some(limit),
                ..Default::default()
            },
        )
        .await?;
    let mut out: Vec<HarvestOutcomeRecord> = rows
        .into_iter()
        .filter_map(|v| serde_json::from_value(v).ok())
        .collect();
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// Short lines for the LLM scoring prompt (most recent first).
pub async fn recent_outcome_prompt_lines(
    storage: &Arc<dyn StoragePort>,
    session_id: &SessionId,
    max: usize,
) -> Result<Vec<String>> {
    let rows = list_outcomes(storage, session_id, max as u32).await?;
    Ok(rows
        .into_iter()
        .take(max)
        .map(|r| {
            let title = r
                .opportunity_snapshot
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!(
                "- [{}] {} — {}",
                match r.result {
                    HarvestDealOutcome::Won => "won",
                    HarvestDealOutcome::Lost => "lost",
                    HarvestDealOutcome::Withdrawn => "withdrawn",
                },
                title,
                if r.notes.is_empty() {
                    "no notes".into()
                } else {
                    r.notes.chars().take(160).collect::<String>()
                }
            )
        })
        .collect())
}
