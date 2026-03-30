//! In-memory job queue implementing [`JobPort`] from `rusvel-core`.
//!
//! The production binary uses [`rusvel_db::Database`] as [`JobPort`]
//! (SQLite, shared with [`rusvel_core::ports::JobStore`]). This crate
//! remains useful for fast unit tests and the [`spawn_worker`] helper.

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use tokio::sync::Mutex;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::*;
use rusvel_core::ports::JobPort;

// ════════════════════════════════════════════════════════════════════
//  JobQueue — in-memory implementation of JobPort
// ════════════════════════════════════════════════════════════════════

/// In-memory job queue backed by a `Vec<Job>` behind a `Mutex`.
#[derive(Debug, Clone)]
pub struct JobQueue {
    jobs: Arc<Mutex<Vec<Job>>>,
}

impl JobQueue {
    /// Create a new empty job queue.
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Helper: build a `Job` from a `NewJob` with `Queued` status.
    fn make_job(new: NewJob) -> Job {
        Job {
            id: JobId::new(),
            session_id: new.session_id,
            kind: new.kind,
            payload: new.payload,
            status: JobStatus::Queued,
            scheduled_at: new.scheduled_at,
            started_at: None,
            completed_at: None,
            retries: 0,
            max_retries: new.max_retries,
            error: None,
            metadata: new.metadata,
        }
    }
}

impl Default for JobQueue {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl JobPort for JobQueue {
    async fn enqueue(&self, new: NewJob) -> Result<JobId> {
        let job = Self::make_job(new);
        let id = job.id;
        self.jobs.lock().await.push(job);
        Ok(id)
    }

    async fn dequeue(&self, kinds: &[JobKind]) -> Result<Option<Job>> {
        let mut jobs = self.jobs.lock().await;
        let now = Utc::now();

        let pos = jobs.iter().position(|j| {
            j.status == JobStatus::Queued
                && (kinds.is_empty() || kinds.contains(&j.kind))
                && j.scheduled_at.is_none_or(|t| t <= now)
        });

        if let Some(idx) = pos {
            jobs[idx].status = JobStatus::Running;
            jobs[idx].started_at = Some(now);
            Ok(Some(jobs[idx].clone()))
        } else {
            Ok(None)
        }
    }

    async fn complete(&self, id: &JobId, result: JobResult) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs
            .iter_mut()
            .find(|j| &j.id == id)
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Job".into(),
                id: id.to_string(),
            })?;

        if job.status != JobStatus::Running {
            return Err(RusvelError::InvalidState {
                from: format!("{:?}", job.status),
                to: "Succeeded".into(),
            });
        }

        job.status = JobStatus::Succeeded;
        job.completed_at = Some(Utc::now());
        job.metadata["result"] =
            serde_json::to_value(&result).map_err(|e| RusvelError::Serialization(e.to_string()))?;
        Ok(())
    }

    async fn hold_for_approval(&self, id: &JobId, result: JobResult) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs
            .iter_mut()
            .find(|j| &j.id == id)
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Job".into(),
                id: id.to_string(),
            })?;

        if job.status != JobStatus::Running {
            return Err(RusvelError::InvalidState {
                from: format!("{:?}", job.status),
                to: "AwaitingApproval".into(),
            });
        }

        job.status = JobStatus::AwaitingApproval;
        job.metadata["approval_pending_result"] =
            serde_json::to_value(&result).map_err(|e| RusvelError::Serialization(e.to_string()))?;
        Ok(())
    }

    async fn fail(&self, id: &JobId, error: String) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs
            .iter_mut()
            .find(|j| &j.id == id)
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Job".into(),
                id: id.to_string(),
            })?;

        if job.status != JobStatus::Running {
            return Err(RusvelError::InvalidState {
                from: format!("{:?}", job.status),
                to: "Failed".into(),
            });
        }

        if job.retries < job.max_retries {
            job.retries += 1;
            job.status = JobStatus::Queued;
            job.started_at = None;
            job.completed_at = None;
            job.error = Some(format!(
                "retry {}/{}: {}",
                job.retries, job.max_retries, error
            ));
        } else {
            job.status = JobStatus::Failed;
            job.completed_at = Some(Utc::now());
            job.error = Some(error);
        }
        Ok(())
    }

    async fn schedule(&self, new: NewJob, _cron: &str) -> Result<JobId> {
        // Phase 0: store the cron expression in metadata and enqueue
        // with a scheduled_at timestamp. Real cron parsing comes later.
        let mut job = Self::make_job(new);
        job.scheduled_at = Some(Utc::now());
        job.metadata["cron"] = serde_json::Value::String(_cron.to_string());
        let id = job.id;
        self.jobs.lock().await.push(job);
        Ok(id)
    }

    async fn cancel(&self, id: &JobId) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs
            .iter_mut()
            .find(|j| &j.id == id)
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Job".into(),
                id: id.to_string(),
            })?;

        match job.status {
            JobStatus::Queued | JobStatus::AwaitingApproval => {
                job.status = JobStatus::Cancelled;
                job.completed_at = Some(Utc::now());
                Ok(())
            }
            _ => Err(RusvelError::InvalidState {
                from: format!("{:?}", job.status),
                to: "Cancelled".into(),
            }),
        }
    }

    async fn approve(&self, id: &JobId) -> Result<()> {
        let mut jobs = self.jobs.lock().await;
        let job = jobs
            .iter_mut()
            .find(|j| &j.id == id)
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Job".into(),
                id: id.to_string(),
            })?;

        if job.status != JobStatus::AwaitingApproval {
            return Err(RusvelError::InvalidState {
                from: format!("{:?}", job.status),
                to: "Queued".into(),
            });
        }

        job.status = JobStatus::Queued;
        Ok(())
    }

    async fn list(&self, filter: JobFilter) -> Result<Vec<Job>> {
        let jobs = self.jobs.lock().await;
        let iter = jobs.iter().filter(|j| {
            if let Some(ref sid) = filter.session_id
                && &j.session_id != sid
            {
                return false;
            }
            if !filter.kinds.is_empty() && !filter.kinds.contains(&j.kind) {
                return false;
            }
            if !filter.statuses.is_empty() && !filter.statuses.contains(&j.status) {
                return false;
            }
            true
        });

        let results: Vec<Job> = match filter.limit {
            Some(n) => iter.take(n as usize).cloned().collect(),
            None => iter.cloned().collect(),
        };
        Ok(results)
    }
}

// ════════════════════════════════════════════════════════════════════
//  Worker — simple polling loop
// ════════════════════════════════════════════════════════════════════

/// Spawn a background worker that polls the queue and dispatches jobs
/// to the provided handler function.
///
/// Returns a `JoinHandle` that runs until the token is cancelled or
/// the handler returns an unrecoverable error.
pub fn spawn_worker<F, Fut>(
    queue: Arc<dyn JobPort>,
    kinds: Vec<JobKind>,
    handler: F,
) -> tokio::task::JoinHandle<()>
where
    F: Fn(Job) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = std::result::Result<JobResult, String>> + Send,
{
    tokio::spawn(async move {
        loop {
            match queue.dequeue(&kinds).await {
                Ok(Some(job)) => {
                    let job_id = job.id;
                    match handler(job).await {
                        Ok(result) => {
                            let _ = queue.complete(&job_id, result).await;
                        }
                        Err(error) => {
                            let _ = queue.fail(&job_id, error).await;
                        }
                    }
                }
                Ok(None) => {
                    // No work available — back off briefly.
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
                Err(_) => {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            }
        }
    })
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn test_new_job() -> NewJob {
        NewJob {
            session_id: SessionId::new(),
            kind: JobKind::AgentRun,
            payload: serde_json::json!({"prompt": "hello"}),
            max_retries: 3,
            metadata: serde_json::json!({}),
            scheduled_at: None,
        }
    }

    #[tokio::test]
    async fn enqueue_and_list() {
        let q = JobQueue::new();
        let id = q.enqueue(test_new_job()).await.unwrap();

        let jobs = q
            .list(JobFilter {
                statuses: vec![JobStatus::Queued],
                ..Default::default()
            })
            .await
            .unwrap();

        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].status, JobStatus::Queued);
    }

    #[tokio::test]
    async fn dequeue_sets_running() {
        let q = JobQueue::new();
        let id = q.enqueue(test_new_job()).await.unwrap();

        let job = q.dequeue(&[JobKind::AgentRun]).await.unwrap().unwrap();
        assert_eq!(job.id, id);
        assert_eq!(job.status, JobStatus::Running);

        // Second dequeue returns None (no more queued jobs).
        let none = q.dequeue(&[JobKind::AgentRun]).await.unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn complete_marks_succeeded() {
        let q = JobQueue::new();
        let id = q.enqueue(test_new_job()).await.unwrap();
        q.dequeue(&[]).await.unwrap(); // set to Running

        let result = JobResult {
            output: serde_json::json!({"answer": 42}),
            metadata: serde_json::json!({}),
        };
        q.complete(&id, result).await.unwrap();

        let jobs = q
            .list(JobFilter {
                statuses: vec![JobStatus::Succeeded],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
        assert!(jobs[0].completed_at.is_some());
    }

    #[tokio::test]
    async fn fail_marks_failed_when_max_retries_zero() {
        let q = JobQueue::new();
        let id = q
            .enqueue(NewJob {
                max_retries: 0,
                ..test_new_job()
            })
            .await
            .unwrap();
        q.dequeue(&[]).await.unwrap();

        q.fail(&id, "boom".into()).await.unwrap();

        let jobs = q
            .list(JobFilter {
                statuses: vec![JobStatus::Failed],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].error.as_deref(), Some("boom"));
    }

    #[tokio::test]
    async fn fail_requeues_until_max_retries_exceeded() {
        let q = JobQueue::new();
        let id = q
            .enqueue(NewJob {
                max_retries: 2,
                ..test_new_job()
            })
            .await
            .unwrap();

        for expected_retry in 1..=2 {
            q.dequeue(&[]).await.unwrap();
            q.fail(&id, "transient".into()).await.unwrap();
            let j = q
                .list(JobFilter {
                    statuses: vec![JobStatus::Queued],
                    ..Default::default()
                })
                .await
                .unwrap();
            assert_eq!(j.len(), 1);
            assert_eq!(j[0].retries, expected_retry);
            assert!(j[0].error.as_deref().unwrap().contains("retry"));
        }

        q.dequeue(&[]).await.unwrap();
        q.fail(&id, "final".into()).await.unwrap();
        let failed = q
            .list(JobFilter {
                statuses: vec![JobStatus::Failed],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].error.as_deref(), Some("final"));
    }

    #[tokio::test]
    async fn approve_moves_to_queued() {
        let q = JobQueue::new();
        let id = q.enqueue(test_new_job()).await.unwrap();

        // Manually set to AwaitingApproval.
        {
            let mut jobs = q.jobs.lock().await;
            jobs[0].status = JobStatus::AwaitingApproval;
        }

        q.approve(&id).await.unwrap();

        let jobs = q
            .list(JobFilter {
                statuses: vec![JobStatus::Queued],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[tokio::test]
    async fn cancel_queued_job() {
        let q = JobQueue::new();
        let id = q.enqueue(test_new_job()).await.unwrap();

        q.cancel(&id).await.unwrap();

        let jobs = q
            .list(JobFilter {
                statuses: vec![JobStatus::Cancelled],
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(jobs.len(), 1);
    }

    #[tokio::test]
    async fn dequeue_filters_by_kind() {
        let q = JobQueue::new();
        q.enqueue(test_new_job()).await.unwrap(); // AgentRun

        // Ask for a different kind -- should get None.
        let none = q.dequeue(&[JobKind::HarvestScan]).await.unwrap();
        assert!(none.is_none());

        // Empty kinds means "any kind".
        let some = q.dequeue(&[]).await.unwrap();
        assert!(some.is_some());
    }
}
