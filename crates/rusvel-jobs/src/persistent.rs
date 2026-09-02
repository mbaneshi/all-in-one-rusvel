//! SQLite-backed job queue that survives process restarts.
//!
//! [`PersistentJobQueue`] wraps [`rusvel_db::Database`] (the SQLite WAL
//! `jobs` table, shared with `JobStore` / `StoragePort::jobs`) and adds
//! the two lifecycle pieces a restart-safe queue needs on top of the
//! write-through [`JobPort`] implementation:
//!
//! 1. **Startup recovery** — jobs left `Running` by a previous process
//!    are requeued with a retry increment (same bounded-retry semantics
//!    as [`JobPort::fail`]), or marked `Failed` once retries are
//!    exhausted.
//! 2. **Retention** — `Succeeded` / `Failed` / `Cancelled` rows older
//!    than a bound (default 7 days) are pruned so the table does not
//!    grow without limit.
//!
//! `Queued` jobs need no explicit reload: every [`JobPort::dequeue`]
//! polls the table directly, so anything enqueued before a restart is
//! picked up by the next worker poll.

use std::path::Path;

use async_trait::async_trait;
use chrono::Utc;

use rusvel_core::domain::*;
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::JobId;
use rusvel_core::ports::JobPort;
use rusvel_db::Database;

/// Default retention bound for finished (`Succeeded` / `Failed` /
/// `Cancelled`) jobs: 7 days.
#[allow(clippy::duration_suboptimal_units)] // `Duration::from_days` is not yet stable
pub const DEFAULT_RETENTION: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

/// Counts reported by [`PersistentJobQueue::recover_interrupted_jobs`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RecoveryReport {
    /// `Running` jobs requeued with a retry increment.
    pub requeued: u64,
    /// `Running` jobs marked `Failed` because retries were exhausted.
    pub failed: u64,
}

/// SQLite-backed [`JobPort`] that survives restarts.
///
/// All queue operations are write-through to the `jobs` table; the
/// struct holds no in-memory job state, so dropping and recreating it
/// from the same database file is equivalent to a process restart.
pub struct PersistentJobQueue {
    db: Database,
}

impl PersistentJobQueue {
    /// Open (or create) the queue at the given SQLite path, then run
    /// startup recovery and prune finished jobs older than
    /// [`DEFAULT_RETENTION`].
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        Self::from_database(Database::open(path)?)
    }

    /// In-memory variant (useful for tests). Note that an in-memory
    /// database cannot survive a real restart.
    pub fn in_memory() -> Result<Self> {
        Self::from_database(Database::in_memory()?)
    }

    /// Wrap an already-open [`Database`], then run startup recovery and
    /// prune finished jobs older than [`DEFAULT_RETENTION`].
    ///
    /// Call this once at process startup, before any worker is spawned:
    /// recovery assumes every `Running` row was interrupted by the
    /// previous process.
    pub fn from_database(db: Database) -> Result<Self> {
        let queue = Self { db };
        queue.recover_interrupted_jobs()?;
        queue.prune_finished_jobs(DEFAULT_RETENTION)?;
        Ok(queue)
    }

    /// Access the underlying database (shared `jobs` table).
    pub fn database(&self) -> &Database {
        &self.db
    }

    /// Requeue jobs left `Running` by an interrupted process.
    ///
    /// Mirrors the bounded-retry semantics of [`JobPort::fail`]: jobs
    /// with retries remaining go back to `Queued` with `retries + 1`
    /// and a `retry n/m: ...` error note; jobs with retries exhausted
    /// are marked `Failed`.
    pub fn recover_interrupted_jobs(&self) -> Result<RecoveryReport> {
        let running = status_str(&JobStatus::Running)?;
        let queued = status_str(&JobStatus::Queued)?;
        let failed = status_str(&JobStatus::Failed)?;
        let now = Utc::now().to_rfc3339();

        self.db.with_connection(|conn| {
            let failed_n = conn
                .execute(
                    "UPDATE jobs SET status = ?1, started_at = NULL, completed_at = ?2,
                            error = 'interrupted by restart (retries exhausted)'
                     WHERE status = ?3 AND retries >= max_retries",
                    rusqlite::params![failed, now, running],
                )
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let requeued_n = conn
                .execute(
                    "UPDATE jobs SET status = ?1, started_at = NULL, completed_at = NULL,
                            retries = retries + 1,
                            error = 'retry ' || (retries + 1) || '/' || max_retries
                                    || ': interrupted by restart'
                     WHERE status = ?2",
                    rusqlite::params![queued, running],
                )
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            Ok(RecoveryReport {
                requeued: requeued_n as u64,
                failed: failed_n as u64,
            })
        })
    }

    /// Delete `Succeeded` / `Failed` / `Cancelled` jobs whose
    /// `completed_at` is older than `retention`. Returns rows deleted.
    /// A zero `retention` disables pruning.
    pub fn prune_finished_jobs(&self, retention: std::time::Duration) -> Result<u64> {
        if retention.is_zero() {
            return Ok(0);
        }
        let ch_dur = chrono::Duration::from_std(retention)
            .map_err(|_| RusvelError::Validation("job retention out of range".into()))?;
        let cutoff = (Utc::now() - ch_dur).to_rfc3339();
        let succeeded = status_str(&JobStatus::Succeeded)?;
        let failed = status_str(&JobStatus::Failed)?;
        let cancelled = status_str(&JobStatus::Cancelled)?;

        self.db.with_connection(|conn| {
            let n = conn
                .execute(
                    "DELETE FROM jobs
                     WHERE status IN (?1, ?2, ?3)
                       AND completed_at IS NOT NULL AND completed_at < ?4",
                    rusqlite::params![succeeded, failed, cancelled, cutoff],
                )
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(n as u64)
        })
    }
}

/// Statuses are stored JSON-encoded (e.g. `"Queued"` including quotes),
/// matching the `JobStore` implementation in `rusvel-db`.
fn status_str(status: &JobStatus) -> Result<String> {
    serde_json::to_string(status).map_err(|e| RusvelError::Serialization(e.to_string()))
}

#[async_trait]
impl JobPort for PersistentJobQueue {
    async fn enqueue(&self, new: NewJob) -> Result<JobId> {
        self.db.enqueue(new).await
    }

    async fn dequeue(&self, kinds: &[JobKind]) -> Result<Option<Job>> {
        JobPort::dequeue(&self.db, kinds).await
    }

    async fn complete(&self, id: &JobId, result: JobResult) -> Result<()> {
        self.db.complete(id, result).await
    }

    async fn hold_for_approval(&self, id: &JobId, result: JobResult) -> Result<()> {
        self.db.hold_for_approval(id, result).await
    }

    async fn fail(&self, id: &JobId, error: String) -> Result<()> {
        self.db.fail(id, error).await
    }

    async fn schedule(&self, new: NewJob, cron: &str) -> Result<JobId> {
        self.db.schedule(new, cron).await
    }

    async fn cancel(&self, id: &JobId) -> Result<()> {
        self.db.cancel(id).await
    }

    async fn approve(&self, id: &JobId) -> Result<()> {
        self.db.approve(id).await
    }

    async fn list(&self, filter: JobFilter) -> Result<Vec<Job>> {
        JobPort::list(&self.db, filter).await
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests — restart simulation against a real temp-file SQLite DB
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::id::SessionId;
    use std::path::PathBuf;

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

    fn temp_db() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("jobs.db");
        (dir, path)
    }

    async fn list_all(q: &PersistentJobQueue) -> Vec<Job> {
        JobPort::list(q, JobFilter::default()).await.unwrap()
    }

    #[tokio::test]
    async fn queued_job_survives_restart() {
        let (_dir, path) = temp_db();

        let q = PersistentJobQueue::open(&path).unwrap();
        let id = q.enqueue(test_new_job()).await.unwrap();
        drop(q);

        // Simulate restart: recreate the queue from the same DB file.
        let q = PersistentJobQueue::open(&path).unwrap();
        let jobs = list_all(&q).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].status, JobStatus::Queued);

        // And it is actually dequeueable after the restart.
        let job = q.dequeue(&[JobKind::AgentRun]).await.unwrap().unwrap();
        assert_eq!(job.id, id);
        assert_eq!(job.status, JobStatus::Running);
    }

    #[tokio::test]
    async fn running_job_recovered_to_queued_with_retry_increment() {
        let (_dir, path) = temp_db();

        let q = PersistentJobQueue::open(&path).unwrap();
        let id = q.enqueue(test_new_job()).await.unwrap();
        q.dequeue(&[]).await.unwrap().unwrap(); // now Running
        drop(q);

        let q = PersistentJobQueue::open(&path).unwrap();
        let jobs = list_all(&q).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, id);
        assert_eq!(jobs[0].status, JobStatus::Queued);
        assert_eq!(jobs[0].retries, 1);
        assert!(jobs[0].started_at.is_none());
        let err = jobs[0].error.as_deref().unwrap();
        assert!(err.contains("retry 1/3"), "unexpected error note: {err}");
        assert!(err.contains("interrupted by restart"));
    }

    #[tokio::test]
    async fn running_job_with_exhausted_retries_fails_on_restart() {
        let (_dir, path) = temp_db();

        let q = PersistentJobQueue::open(&path).unwrap();
        q.enqueue(NewJob {
            max_retries: 0,
            ..test_new_job()
        })
        .await
        .unwrap();
        q.dequeue(&[]).await.unwrap().unwrap();
        drop(q);

        let q = PersistentJobQueue::open(&path).unwrap();
        let jobs = list_all(&q).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Failed);
        assert!(jobs[0].completed_at.is_some());
        assert!(
            jobs[0]
                .error
                .as_deref()
                .unwrap()
                .contains("retries exhausted")
        );
    }

    #[tokio::test]
    async fn state_transitions_are_write_through() {
        let (_dir, path) = temp_db();

        let q = PersistentJobQueue::open(&path).unwrap();
        let id = q.enqueue(test_new_job()).await.unwrap();
        q.dequeue(&[]).await.unwrap();
        q.complete(
            &id,
            JobResult {
                output: serde_json::json!({"answer": 42}),
                metadata: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
        drop(q);

        let q = PersistentJobQueue::open(&path).unwrap();
        let jobs = list_all(&q).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].status, JobStatus::Succeeded);
        assert_eq!(jobs[0].metadata["result"]["output"]["answer"], 42);
    }

    #[tokio::test]
    async fn old_finished_jobs_pruned_on_restart_recent_ones_kept() {
        let (_dir, path) = temp_db();

        let q = PersistentJobQueue::open(&path).unwrap();
        let old_id = q.enqueue(test_new_job()).await.unwrap();
        q.dequeue(&[]).await.unwrap();
        q.complete(
            &old_id,
            JobResult {
                output: serde_json::json!({}),
                metadata: serde_json::json!({}),
            },
        )
        .await
        .unwrap();
        let fresh_id = q.enqueue(test_new_job()).await.unwrap();

        // Backdate the finished job past the retention bound.
        let eight_days_ago = (Utc::now() - chrono::Duration::days(8)).to_rfc3339();
        q.database()
            .with_connection(|conn| {
                conn.execute(
                    "UPDATE jobs SET completed_at = ?1 WHERE id = ?2",
                    rusqlite::params![eight_days_ago, old_id.to_string()],
                )
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
                Ok(())
            })
            .unwrap();
        drop(q);

        let q = PersistentJobQueue::open(&path).unwrap();
        let jobs = list_all(&q).await;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, fresh_id);
        assert_eq!(jobs[0].status, JobStatus::Queued);
    }

    #[tokio::test]
    async fn recovery_report_counts() {
        let q = PersistentJobQueue::in_memory().unwrap();
        q.enqueue(test_new_job()).await.unwrap();
        q.enqueue(NewJob {
            max_retries: 0,
            ..test_new_job()
        })
        .await
        .unwrap();
        q.dequeue(&[]).await.unwrap().unwrap();
        q.dequeue(&[]).await.unwrap().unwrap();

        // Same-process recovery (e.g. explicit sweep): both Running
        // jobs are handled according to their retry budget.
        let report = q.recover_interrupted_jobs().unwrap();
        assert_eq!(
            report,
            RecoveryReport {
                requeued: 1,
                failed: 1
            }
        );
    }
}
