//! Approvals — human-in-the-loop approval flow for jobs (ADR-008).
//!
//! Lists jobs in `AwaitingApproval` status and allows approving or rejecting them.

use std::sync::Arc;

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::json;

use rusvel_core::domain::{Job, JobFilter, JobStatus};
use rusvel_core::error::RusvelError;
use rusvel_core::id::JobId;

use crate::AppState;

#[derive(Serialize)]
pub struct PendingApprovalItem {
    #[serde(flatten)]
    pub job: Job,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_pane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_window_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_dept_id: Option<String>,
}

fn approval_terminal_fields(
    meta: &serde_json::Value,
) -> (Option<String>, Option<String>, Option<String>) {
    let obj = meta.as_object();
    let g = |k: &str| {
        obj
            .and_then(|m| m.get(k))
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string)
    };
    (
        g("terminal_pane_id"),
        g("terminal_window_id"),
        g("terminal_dept_id"),
    )
}

/// `GET /api/approvals` — list jobs awaiting human approval.
pub async fn list_pending(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<PendingApprovalItem>>, (StatusCode, String)> {
    let filter = JobFilter {
        session_id: None,
        kinds: vec![],
        statuses: vec![JobStatus::AwaitingApproval],
        limit: None,
    };

    let jobs = state
        .jobs
        .list(filter)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let out: Vec<PendingApprovalItem> = jobs
        .into_iter()
        .map(|job| {
            let (terminal_pane_id, terminal_window_id, terminal_dept_id) =
                approval_terminal_fields(&job.metadata);
            PendingApprovalItem {
                job,
                terminal_pane_id,
                terminal_window_id,
                terminal_dept_id,
            }
        })
        .collect();

    Ok(Json(out))
}

/// `POST /api/approvals/{id}/approve` — approve a pending job.
pub async fn approve_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<StatusCode, (StatusCode, String)> {
    let uuid = uuid::Uuid::parse_str(&id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid job id".into()))?;
    let job_id = JobId::from_uuid(uuid);

    state.jobs.approve(&job_id).await.map_err(|e| match e {
        RusvelError::NotFound { .. } => (StatusCode::NOT_FOUND, "job not found".into()),
        RusvelError::InvalidState { from, .. } => (
            StatusCode::CONFLICT,
            format!("job is not awaiting approval (status: {from})"),
        ),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Deserialize, Default)]
pub struct RejectBody {
    /// Optional human-readable reason stored on `job.metadata.reject_reason`.
    #[serde(default)]
    pub reason: Option<String>,
}

/// `POST /api/approvals/{id}/reject` — reject a pending job (sets status to Cancelled).
pub async fn reject_job(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(body): Json<RejectBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    let uuid = uuid::Uuid::parse_str(&id)
        .map_err(|_| (StatusCode::BAD_REQUEST, "invalid job id".into()))?;
    let job_id = JobId::from_uuid(uuid);

    // Reject is only valid for the approval gate; `JobPort::cancel` also allows `Queued`.
    let mut job = state
        .storage
        .jobs()
        .get(&job_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
        .ok_or((StatusCode::NOT_FOUND, "job not found".into()))?;

    if job.status != JobStatus::AwaitingApproval {
        return Err((
            StatusCode::CONFLICT,
            format!("job is not awaiting approval (status: {:?})", job.status),
        ));
    }

    if let Some(r) = body
        .reason
        .as_ref()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        if let Some(m) = job.metadata.as_object_mut() {
            m.insert("reject_reason".into(), json!(r));
        } else {
            job.metadata = json!({ "reject_reason": r });
        }
        state
            .storage
            .jobs()
            .update(&job)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    }

    state.jobs.cancel(&job_id).await.map_err(|e| match e {
        RusvelError::NotFound { .. } => (StatusCode::NOT_FOUND, "job not found".into()),
        RusvelError::InvalidState { from, .. } => (
            StatusCode::CONFLICT,
            format!("job is not awaiting approval (status: {from})"),
        ),
        _ => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    })?;

    Ok(StatusCode::OK)
}
