//! SQLite-backed [`StoragePort`] implementation.
//!
//! A single [`Database`] struct owns the connection and implements all
//! five sub-store traits. Async wrappers use `tokio::task::spawn_blocking`
//! internally since rusqlite is synchronous.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;

fn validate_identifier(name: &str) -> rusvel_core::Result<&str> {
    if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(RusvelError::Validation(format!(
            "invalid SQL identifier: {name}"
        )));
    }
    Ok(name)
}

fn cost_events_spend_snapshot_inner(db: &Connection) -> CostEventsSpendSnapshot {
    let event_row_count: u64 = db
        .query_row("SELECT COUNT(*) FROM cost_events", [], |r| {
            r.get::<_, i64>(0)
        })
        .unwrap_or(0) as u64;
    if event_row_count == 0 {
        return CostEventsSpendSnapshot {
            event_row_count: 0,
            ..Default::default()
        };
    }

    let total_usd_all: f64 = db
        .query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cost_events",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let mut by_department_usd = HashMap::new();
    let mut by_department_tokens = HashMap::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT COALESCE(department_id, '_none') AS dept,
                SUM(cost_usd) AS usd,
                SUM(CAST(input_tokens AS INTEGER) + CAST(output_tokens AS INTEGER)) AS tok
         FROM cost_events GROUP BY dept",
    ) {
        let rows = stmt.query_map([], |row| {
            let dept: String = row.get(0)?;
            let usd: f64 = row.get(1)?;
            let tok: i64 = row.get(2)?;
            Ok((dept, usd, tok.max(0) as u64))
        });
        if let Ok(iter) = rows {
            for r in iter.flatten() {
                let (dept, usd, tok) = r;
                by_department_usd.insert(dept.clone(), usd);
                by_department_tokens.insert(dept, tok);
            }
        }
    }

    let mut by_session_usd = HashMap::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT session_id, SUM(cost_usd) FROM cost_events WHERE session_id IS NOT NULL GROUP BY session_id",
    ) {
        let rows = stmt.query_map([], |row| {
            let sid: String = row.get(0)?;
            let usd: f64 = row.get(1)?;
            Ok((sid, usd))
        });
        if let Ok(iter) = rows {
            for r in iter.flatten() {
                by_session_usd.insert(r.0, r.1);
            }
        }
    }

    let mut by_model = HashMap::new();
    if let Ok(mut stmt) = db.prepare(
        "SELECT model,
                SUM(cost_usd) AS usd,
                SUM(CAST(input_tokens AS INTEGER) + CAST(output_tokens AS INTEGER)) AS tok
         FROM cost_events GROUP BY model",
    ) {
        let rows = stmt.query_map([], |row| {
            let model: String = row.get(0)?;
            let usd: f64 = row.get(1)?;
            let tok: i64 = row.get(2)?;
            Ok((model, usd, tok.max(0) as u64))
        });
        if let Ok(iter) = rows {
            for r in iter.flatten() {
                by_model.insert(r.0, (r.1, r.2));
            }
        }
    }

    CostEventsSpendSnapshot {
        event_row_count,
        total_usd_all,
        by_department_usd,
        by_department_tokens,
        by_session_usd,
        by_model,
    }
}

use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use rusvel_core::error::RusvelError;
use rusvel_core::ports::*;
use rusvel_core::{
    CostEvent, Event, EventFilter, Job, JobFilter, JobKind, JobResult, JobStatus, MetricFilter,
    MetricPoint, NewJob, ObjectFilter, Run, Session, SessionSummary, Thread,
};
use rusvel_core::{EventId, JobId, RunId, SessionId, ThreadId};

use serde::Serialize;

use crate::migrations;

// ════════════════════════════════════════════════════════════════════
//  Schema introspection types
// ════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColumnInfo {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForeignKeyInfo {
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SqlResult {
    pub columns: Vec<SqlColumn>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub row_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct SqlColumn {
    pub name: String,
    pub col_type: String,
}

// ════════════════════════════════════════════════════════════════════
//  Database
// ════════════════════════════════════════════════════════════════════

/// SQLite-backed storage adapter.
///
/// Aggregated LLM spend from `cost_events` for analytics (single query round-trip pattern).
#[derive(Debug, Clone, Default)]
pub struct CostEventsSpendSnapshot {
    /// Rows in `cost_events` (0 if table missing or empty).
    pub event_row_count: u64,
    pub total_usd_all: f64,
    pub by_department_usd: HashMap<String, f64>,
    pub by_department_tokens: HashMap<String, u64>,
    pub by_session_usd: HashMap<String, f64>,
    /// Model name -> (usd, tokens).
    pub by_model: HashMap<String, (f64, u64)>,
}

/// Thread-safe via an internal `Mutex<Connection>`. For the single-writer
/// nature of `SQLite` this is the simplest correct approach.
pub struct Database {
    conn: Arc<Mutex<Connection>>,
}

impl Database {
    /// Open (or create) a database at the given path, enable WAL mode,
    /// and run all pending migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, RusvelError> {
        let conn = Connection::open(path).map_err(|e| RusvelError::Storage(e.to_string()))?;
        Self::init(conn)
    }

    /// Create an in-memory database (useful for tests).
    pub fn in_memory() -> Result<Self, RusvelError> {
        let conn = Connection::open_in_memory().map_err(|e| RusvelError::Storage(e.to_string()))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, RusvelError> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        migrations::run_migrations(&conn).map_err(|e| RusvelError::Storage(e.to_string()))?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    /// Acquire the connection lock. Panics only if a thread panicked
    /// while holding the lock (programming bug).
    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("database mutex poisoned")
    }

    /// Run synchronous work on the underlying SQLite connection (e.g. schema introspection).
    /// Callers that run from async contexts should use `tokio::task::spawn_blocking`.
    pub fn with_connection<R>(
        &self,
        f: impl FnOnce(&Connection) -> rusvel_core::Result<R>,
    ) -> rusvel_core::Result<R> {
        let guard = self.conn();
        f(&guard)
    }

    /// Mark `Running` jobs with `started_at` older than `max_age` as `Failed` (crash / `kill -9` recovery).
    /// Returns rows updated. `max_age` zero disables.
    pub fn sweep_stale_running_jobs(
        &self,
        max_age: std::time::Duration,
    ) -> rusvel_core::Result<u64> {
        if max_age.is_zero() {
            return Ok(0);
        }
        let ch_dur = chrono::Duration::from_std(max_age).map_err(|_| {
            RusvelError::Validation("stale job max age out of range".into())
        })?;
        let cutoff = Utc::now() - ch_dur;
        let cutoff_s = cutoff.to_rfc3339();
        let failed_status =
            serde_json::to_string(&JobStatus::Failed).map_err(|e| RusvelError::Serialization(e.to_string()))?;
        let running_status =
            serde_json::to_string(&JobStatus::Running).map_err(|e| RusvelError::Serialization(e.to_string()))?;
        let msg = "stale Running job (process died or force-killed); swept at startup";
        let conn = self.conn();
        let n = conn
            .execute(
                "UPDATE jobs SET status = ?1, completed_at = ?2, error = ?3, started_at = NULL \
                 WHERE status = ?4 AND started_at IS NOT NULL AND started_at < ?5",
                params![
                    failed_status,
                    Utc::now().to_rfc3339(),
                    msg,
                    running_status,
                    cutoff_s
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        Ok(n as u64)
    }

    /// Roll up `cost_events` for `/api/analytics/spend`. On missing table or query error, returns zeros.
    pub async fn cost_events_spend_snapshot(&self) -> rusvel_core::Result<CostEventsSpendSnapshot> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            Ok(cost_events_spend_snapshot_inner(&db))
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    // ── Schema introspection ─────────────────────────────────────

    /// List all user tables with row counts.
    pub fn list_tables(&self) -> rusvel_core::Result<Vec<TableInfo>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);

        let mut tables = Vec::new();
        for name in names {
            tables.push(self.table_info_inner(&conn, &name)?);
        }
        Ok(tables)
    }

    /// Get detailed info for a single table.
    pub fn get_table_info(&self, name: &str) -> rusvel_core::Result<TableInfo> {
        // Validate table name exists
        let conn = self.conn();
        let exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name = ?1",
                [name],
                |row| row.get(0),
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        if !exists {
            return Err(RusvelError::NotFound {
                kind: "table".into(),
                id: name.into(),
            });
        }
        self.table_info_inner(&conn, name)
    }

    fn table_info_inner(&self, conn: &Connection, table: &str) -> rusvel_core::Result<TableInfo> {
        let table = validate_identifier(table)?;
        let mut col_stmt = conn
            .prepare(&format!("PRAGMA table_info('{table}')"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let columns: Vec<ColumnInfo> = col_stmt
            .query_map([], |row| {
                Ok(ColumnInfo {
                    name: row.get(1)?,
                    col_type: row.get::<_, String>(2).unwrap_or_default(),
                    nullable: row.get::<_, i32>(3).unwrap_or(1) == 0,
                    default_value: row.get(4).ok(),
                    primary_key: row.get::<_, i32>(5).unwrap_or(0) != 0,
                })
            })
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        // Indexes via PRAGMA index_list + index_info
        let mut idx_stmt = conn
            .prepare(&format!("PRAGMA index_list('{table}')"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let idx_list: Vec<(String, bool)> = idx_stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, i32>(2).unwrap_or(0) != 0,
                ))
            })
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();
        drop(idx_stmt);

        let mut indexes = Vec::new();
        for (idx_name, unique) in &idx_list {
            validate_identifier(idx_name)?;
            let mut info_stmt = conn
                .prepare(&format!("PRAGMA index_info('{idx_name}')"))
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            let cols: Vec<String> = info_stmt
                .query_map([], |row| row.get(2))
                .map_err(|e| RusvelError::Storage(e.to_string()))?
                .filter_map(|r| r.ok())
                .collect();
            indexes.push(IndexInfo {
                name: idx_name.clone(),
                columns: cols,
                unique: *unique,
            });
        }

        // Foreign keys via PRAGMA foreign_key_list
        let mut fk_stmt = conn
            .prepare(&format!("PRAGMA foreign_key_list('{table}')"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let foreign_keys: Vec<ForeignKeyInfo> = fk_stmt
            .query_map([], |row| {
                Ok(ForeignKeyInfo {
                    to_table: row.get(2)?,
                    from_column: row.get(3)?,
                    to_column: row.get(4)?,
                })
            })
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .filter_map(|r| r.ok())
            .collect();

        // Row count
        let row_count: u64 = conn
            .query_row(&format!("SELECT count(*) FROM \"{table}\""), [], |row| {
                row.get(0)
            })
            .unwrap_or(0);

        Ok(TableInfo {
            name: table.to_string(),
            columns,
            indexes,
            foreign_keys,
            row_count,
        })
    }

    /// Get paginated rows from a table.
    pub fn get_table_rows(
        &self,
        table: &str,
        limit: u32,
        offset: u32,
        order: Option<&str>,
        select: Option<&str>,
    ) -> rusvel_core::Result<SqlResult> {
        // Validate table exists
        let conn = self.conn();
        let exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name = ?1",
                [table],
                |row| row.get(0),
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        if !exists {
            return Err(RusvelError::NotFound {
                kind: "table".into(),
                id: table.into(),
            });
        }

        validate_identifier(table)?;
        let cols = match select {
            Some(s) => {
                for col in s.split(',') {
                    validate_identifier(col.trim())?;
                }
                s
            }
            None => "*",
        };
        let order_clause = match order {
            Some(o) => {
                for part in o.split(',') {
                    let token = part.trim().split_whitespace().next().unwrap_or("");
                    validate_identifier(token)?;
                }
                format!(" ORDER BY {o}")
            }
            None => String::new(),
        };

        let sql =
            format!("SELECT {cols} FROM \"{table}\"{order_clause} LIMIT {limit} OFFSET {offset}");
        self.execute_sql_inner(&conn, &sql)
    }

    /// Execute an arbitrary SQL query (read-only by default).
    pub fn execute_sql(&self, sql: &str) -> rusvel_core::Result<SqlResult> {
        let conn = self.conn();
        self.execute_sql_inner(&conn, sql)
    }

    fn execute_sql_inner(&self, conn: &Connection, sql: &str) -> rusvel_core::Result<SqlResult> {
        let start = std::time::Instant::now();
        let mut stmt = conn
            .prepare(sql)
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let col_count = stmt.column_count();
        let columns: Vec<SqlColumn> = (0..col_count)
            .map(|i| SqlColumn {
                name: stmt.column_name(i).unwrap_or("?").to_string(),
                col_type: String::new(),
            })
            .collect();

        let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
        let mut raw_rows = stmt
            .query([])
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        while let Some(row) = raw_rows
            .next()
            .map_err(|e| RusvelError::Storage(e.to_string()))?
        {
            let mut vals = Vec::new();
            for i in 0..col_count {
                let val = row_value_to_json(row, i);
                vals.push(val);
            }
            rows.push(vals);
        }

        let duration_ms = start.elapsed().as_millis() as u64;
        let row_count = rows.len();

        Ok(SqlResult {
            columns,
            rows,
            row_count,
            duration_ms,
        })
    }
}

/// Convert a rusqlite row value at a given index to a serde_json::Value.
fn row_value_to_json(row: &rusqlite::Row<'_>, idx: usize) -> serde_json::Value {
    use rusqlite::types::ValueRef;
    match row.get_ref(idx) {
        Ok(ValueRef::Null) => serde_json::Value::Null,
        Ok(ValueRef::Integer(i)) => serde_json::json!(i),
        Ok(ValueRef::Real(f)) => serde_json::json!(f),
        Ok(ValueRef::Text(s)) => {
            let s = String::from_utf8_lossy(s);
            // Try to parse as JSON if it looks like JSON
            if (s.starts_with('{') || s.starts_with('['))
                && let Ok(v) = serde_json::from_str::<serde_json::Value>(&s)
            {
                v
            } else {
                serde_json::Value::String(s.into_owned())
            }
        }
        Ok(ValueRef::Blob(b)) => serde_json::json!(format!("<blob {}B>", b.len())),
        Err(_) => serde_json::Value::Null,
    }
}

// ════════════════════════════════════════════════════════════════════
//  StoragePort
// ════════════════════════════════════════════════════════════════════

impl StoragePort for Database {
    fn events(&self) -> &dyn EventStore {
        self
    }
    fn objects(&self) -> &dyn ObjectStore {
        self
    }
    fn sessions(&self) -> &dyn SessionStore {
        self
    }
    fn jobs(&self) -> &dyn JobStore {
        self
    }
    fn metrics(&self) -> &dyn MetricStore {
        self
    }
}

// ════════════════════════════════════════════════════════════════════
//  EventStore
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl EventStore for Database {
    async fn append(&self, event: &Event) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let event = event.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO events (id, session_id, run_id, source, kind, payload, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    event.id.to_string(),
                    event.session_id.map(|s| s.to_string()),
                    event.run_id.map(|r| r.to_string()),
                    serde_json::to_string(&event.source)?,
                    event.kind,
                    serde_json::to_string(&event.payload)?,
                    event.created_at.to_rfc3339(),
                    serde_json::to_string(&event.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn get(&self, id: &EventId) -> rusvel_core::Result<Option<Event>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut stmt = db
                .prepare("SELECT id, session_id, run_id, source, kind, payload, created_at, metadata FROM events WHERE id = ?1")
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let result = stmt
                .query_row(params![id.to_string()], |row| Ok(row_to_event(row)))
                .optional()
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            match result {
                Some(event) => Ok(Some(event?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn query(&self, filter: EventFilter) -> rusvel_core::Result<Vec<Event>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut sql = String::from(
                "SELECT id, session_id, run_id, source, kind, payload, created_at, metadata FROM events WHERE 1=1",
            );
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;

            if let Some(ref sid) = filter.session_id {
                sql.push_str(&format!(" AND session_id = ?{idx}"));
                param_values.push(Box::new(sid.to_string()));
                idx += 1;
            }
            if let Some(ref rid) = filter.run_id {
                sql.push_str(&format!(" AND run_id = ?{idx}"));
                param_values.push(Box::new(rid.to_string()));
                idx += 1;
            }
            if let Some(ref kind) = filter.kind {
                sql.push_str(&format!(" AND kind = ?{idx}"));
                param_values.push(Box::new(kind.clone()));
                idx += 1;
            }
            if let Some(ref source) = filter.source {
                sql.push_str(&format!(" AND source = ?{idx}"));
                param_values.push(Box::new(serde_json::to_string(source)?));
                idx += 1;
            }
            if let Some(ref since) = filter.since {
                sql.push_str(&format!(" AND created_at >= ?{idx}"));
                param_values.push(Box::new(since.to_rfc3339()));
                idx += 1;
            }

            sql.push_str(" ORDER BY created_at ASC");

            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT ?{idx}"));
                param_values.push(Box::new(i64::from(limit)));
                let _ = idx;
            }

            let mut stmt = db
                .prepare(&sql)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect();

            let rows = stmt
                .query_map(params_refs.as_slice(), |row| Ok(row_to_event(row)))
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut events = Vec::new();
            for row in rows {
                let event = row.map_err(|e| RusvelError::Storage(e.to_string()))??;
                events.push(event);
            }
            Ok(events)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }
}

fn row_to_event(row: &rusqlite::Row<'_>) -> rusvel_core::Result<Event> {
    let id_str: String = row
        .get(0)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let session_str: Option<String> = row
        .get(1)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let run_str: Option<String> = row
        .get(2)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let source_str: String = row
        .get(3)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let kind: String = row
        .get(4)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let payload_str: String = row
        .get(5)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let created_str: String = row
        .get(6)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let meta_str: String = row
        .get(7)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

    Ok(Event {
        id: EventId::from_uuid(
            uuid::Uuid::parse_str(&id_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        session_id: session_str
            .map(|s| {
                uuid::Uuid::parse_str(&s)
                    .map(SessionId::from_uuid)
                    .map_err(|e| RusvelError::Storage(e.to_string()))
            })
            .transpose()?,
        run_id: run_str
            .map(|s| {
                uuid::Uuid::parse_str(&s)
                    .map(RunId::from_uuid)
                    .map_err(|e| RusvelError::Storage(e.to_string()))
            })
            .transpose()?,
        source: serde_json::from_str(&source_str)?,
        kind,
        payload: serde_json::from_str(&payload_str)?,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| RusvelError::Storage(e.to_string()))?,
        metadata: serde_json::from_str(&meta_str)?,
    })
}

// ════════════════════════════════════════════════════════════════════
//  ObjectStore
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl ObjectStore for Database {
    async fn put(
        &self,
        kind: &str,
        id: &str,
        object: serde_json::Value,
    ) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let kind = kind.to_string();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let now = Utc::now().to_rfc3339();
            db.execute(
                "INSERT INTO objects (kind, id, data, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(kind, id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
                params![kind, id, serde_json::to_string(&object)?, &now, &now],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn get(&self, kind: &str, id: &str) -> rusvel_core::Result<Option<serde_json::Value>> {
        let conn = self.conn.clone();
        let kind = kind.to_string();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let result: Option<String> = db
                .query_row(
                    "SELECT data FROM objects WHERE kind = ?1 AND id = ?2",
                    params![kind, id],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            match result {
                Some(data) => Ok(Some(serde_json::from_str(&data)?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn delete(&self, kind: &str, id: &str) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let kind = kind.to_string();
        let id = id.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "DELETE FROM objects WHERE kind = ?1 AND id = ?2",
                params![kind, id],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn list(
        &self,
        kind: &str,
        filter: ObjectFilter,
    ) -> rusvel_core::Result<Vec<serde_json::Value>> {
        let conn = self.conn.clone();
        let kind = kind.to_string();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut sql = String::from("SELECT data FROM objects WHERE kind = ?1");
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            param_values.push(Box::new(kind.to_string()));
            let mut idx = 2;

            if let Some(ref sid) = filter.session_id {
                sql.push_str(&format!(" AND json_extract(data, '$.session_id') = ?{idx}"));
                param_values.push(Box::new(sid.to_string()));
                idx += 1;
            }

            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT ?{idx}"));
                param_values.push(Box::new(i64::from(limit)));
                idx += 1;
            }
            if let Some(offset) = filter.offset {
                // LIMIT is required for OFFSET in SQLite; add a large default if
                // no explicit limit was supplied.
                if filter.limit.is_none() {
                    sql.push_str(&format!(" LIMIT ?{idx}"));
                    param_values.push(Box::new(i64::MAX));
                    idx += 1;
                }
                sql.push_str(&format!(" OFFSET ?{idx}"));
                param_values.push(Box::new(i64::from(offset)));
                let _ = idx;
            }

            let mut stmt = db
                .prepare(&sql)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect();

            let rows = stmt
                .query_map(params_refs.as_slice(), |row| {
                    let data: String = row.get(0)?;
                    Ok(data)
                })
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut objects = Vec::new();
            for row in rows {
                let data = row.map_err(|e| RusvelError::Storage(e.to_string()))?;
                objects.push(serde_json::from_str(&data)?);
            }
            Ok(objects)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }
}

// ════════════════════════════════════════════════════════════════════
//  SessionStore
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl SessionStore for Database {
    async fn put_session(&self, session: &Session) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let session = session.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO sessions (id, name, kind, tags, config, created_at, updated_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
                 ON CONFLICT(id) DO UPDATE SET
                   name = excluded.name,
                   kind = excluded.kind,
                   tags = excluded.tags,
                   config = excluded.config,
                   updated_at = excluded.updated_at,
                   metadata = excluded.metadata",
                params![
                    session.id.to_string(),
                    session.name,
                    serde_json::to_string(&session.kind)?,
                    serde_json::to_string(&session.tags)?,
                    serde_json::to_string(&session.config)?,
                    session.created_at.to_rfc3339(),
                    session.updated_at.to_rfc3339(),
                    serde_json::to_string(&session.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn get_session(&self, id: &SessionId) -> rusvel_core::Result<Option<Session>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let result = db
                .query_row(
                    "SELECT id, name, kind, tags, config, created_at, updated_at, metadata FROM sessions WHERE id = ?1",
                    params![id.to_string()],
                    |row| {
                        Ok(row_to_session(row))
                    },
                )
                .optional()
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            match result {
                Some(s) => Ok(Some(s?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn list_sessions(&self) -> rusvel_core::Result<Vec<SessionSummary>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut stmt = db
                .prepare(
                    "SELECT id, name, kind, tags, updated_at FROM sessions ORDER BY updated_at DESC",
                )
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let rows = stmt
                .query_map([], |row| {
                    let id_str: String = row.get(0)?;
                    let name: String = row.get(1)?;
                    let kind_str: String = row.get(2)?;
                    let tags_str: String = row.get(3)?;
                    let updated_str: String = row.get(4)?;
                    Ok((id_str, name, kind_str, tags_str, updated_str))
                })
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut summaries = Vec::new();
            for row in rows {
                let (id_str, name, kind_str, tags_str, updated_str) =
                    row.map_err(|e| RusvelError::Storage(e.to_string()))?;
                summaries.push(SessionSummary {
                    id: SessionId::from_uuid(
                        uuid::Uuid::parse_str(&id_str)
                            .map_err(|e| RusvelError::Storage(e.to_string()))?,
                    ),
                    name,
                    kind: serde_json::from_str(&kind_str)?,
                    tags: serde_json::from_str(&tags_str)?,
                    updated_at: DateTime::parse_from_rfc3339(&updated_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .map_err(|e| RusvelError::Storage(e.to_string()))?,
                });
            }
            Ok(summaries)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn put_run(&self, run: &Run) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let run = run.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO runs (id, session_id, engine, input_summary, status, llm_budget_used, tool_calls_count, started_at, completed_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(id) DO UPDATE SET
                   status = excluded.status,
                   llm_budget_used = excluded.llm_budget_used,
                   tool_calls_count = excluded.tool_calls_count,
                   completed_at = excluded.completed_at,
                   metadata = excluded.metadata",
                params![
                    run.id.to_string(),
                    run.session_id.to_string(),
                    serde_json::to_string(&run.engine)?,
                    run.input_summary,
                    serde_json::to_string(&run.status)?,
                    run.llm_budget_used,
                    run.tool_calls_count,
                    run.started_at.to_rfc3339(),
                    run.completed_at.map(|dt| dt.to_rfc3339()),
                    serde_json::to_string(&run.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn get_run(&self, id: &RunId) -> rusvel_core::Result<Option<Run>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let result = db
                .query_row(
                    "SELECT id, session_id, engine, input_summary, status, llm_budget_used, tool_calls_count, started_at, completed_at, metadata FROM runs WHERE id = ?1",
                    params![id.to_string()],
                    |row| Ok(row_to_run(row)),
                )
                .optional()
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            match result {
                Some(r) => Ok(Some(r?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn list_runs(&self, session_id: &SessionId) -> rusvel_core::Result<Vec<Run>> {
        let conn = self.conn.clone();
        let session_id = *session_id;
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut stmt = db
                .prepare("SELECT id, session_id, engine, input_summary, status, llm_budget_used, tool_calls_count, started_at, completed_at, metadata FROM runs WHERE session_id = ?1 ORDER BY started_at ASC")
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let rows = stmt
                .query_map(params![session_id.to_string()], |row| Ok(row_to_run(row)))
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut runs = Vec::new();
            for row in rows {
                let run = row.map_err(|e| RusvelError::Storage(e.to_string()))??;
                runs.push(run);
            }
            Ok(runs)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn put_thread(&self, thread: &Thread) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let thread = thread.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO threads (id, run_id, channel, messages, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET
                   messages = excluded.messages,
                   metadata = excluded.metadata",
                params![
                    thread.id.to_string(),
                    thread.run_id.to_string(),
                    serde_json::to_string(&thread.channel)?,
                    serde_json::to_string(&thread.messages)?,
                    serde_json::to_string(&thread.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn get_thread(&self, id: &ThreadId) -> rusvel_core::Result<Option<Thread>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let result = db
                .query_row(
                    "SELECT id, run_id, channel, messages, metadata FROM threads WHERE id = ?1",
                    params![id.to_string()],
                    |row| Ok(row_to_thread(row)),
                )
                .optional()
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            match result {
                Some(t) => Ok(Some(t?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn list_threads(&self, run_id: &RunId) -> rusvel_core::Result<Vec<Thread>> {
        let conn = self.conn.clone();
        let run_id = *run_id;
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut stmt = db
                .prepare(
                    "SELECT id, run_id, channel, messages, metadata FROM threads WHERE run_id = ?1",
                )
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let rows = stmt
                .query_map(params![run_id.to_string()], |row| Ok(row_to_thread(row)))
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut threads = Vec::new();
            for row in rows {
                let thread = row.map_err(|e| RusvelError::Storage(e.to_string()))??;
                threads.push(thread);
            }
            Ok(threads)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }
}

fn row_to_session(row: &rusqlite::Row<'_>) -> rusvel_core::Result<Session> {
    let id_str: String = row
        .get(0)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let name: String = row
        .get(1)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let kind_str: String = row
        .get(2)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let tags_str: String = row
        .get(3)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let config_str: String = row
        .get(4)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let created_str: String = row
        .get(5)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let updated_str: String = row
        .get(6)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let meta_str: String = row
        .get(7)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

    Ok(Session {
        id: SessionId::from_uuid(
            uuid::Uuid::parse_str(&id_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        name,
        kind: serde_json::from_str(&kind_str)?,
        tags: serde_json::from_str(&tags_str)?,
        config: serde_json::from_str(&config_str)?,
        created_at: DateTime::parse_from_rfc3339(&created_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| RusvelError::Storage(e.to_string()))?,
        updated_at: DateTime::parse_from_rfc3339(&updated_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| RusvelError::Storage(e.to_string()))?,
        metadata: serde_json::from_str(&meta_str)?,
    })
}

fn row_to_run(row: &rusqlite::Row<'_>) -> rusvel_core::Result<Run> {
    let id_str: String = row
        .get(0)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let session_str: String = row
        .get(1)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let engine_str: String = row
        .get(2)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let input_summary: String = row
        .get(3)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let status_str: String = row
        .get(4)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let llm_budget_used: f64 = row
        .get(5)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let tool_calls_count: u32 = row
        .get(6)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let started_str: String = row
        .get(7)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let completed_str: Option<String> = row
        .get(8)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let meta_str: String = row
        .get(9)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

    Ok(Run {
        id: RunId::from_uuid(
            uuid::Uuid::parse_str(&id_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        session_id: SessionId::from_uuid(
            uuid::Uuid::parse_str(&session_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        engine: serde_json::from_str(&engine_str)?,
        input_summary,
        status: serde_json::from_str(&status_str)?,
        llm_budget_used,
        tool_calls_count,
        started_at: DateTime::parse_from_rfc3339(&started_str)
            .map(|dt| dt.with_timezone(&Utc))
            .map_err(|e| RusvelError::Storage(e.to_string()))?,
        completed_at: completed_str
            .map(|s| {
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| RusvelError::Storage(e.to_string()))
            })
            .transpose()?,
        metadata: serde_json::from_str(&meta_str)?,
    })
}

fn row_to_thread(row: &rusqlite::Row<'_>) -> rusvel_core::Result<Thread> {
    let id_str: String = row
        .get(0)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let run_str: String = row
        .get(1)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let channel_str: String = row
        .get(2)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let messages_str: String = row
        .get(3)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let meta_str: String = row
        .get(4)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

    Ok(Thread {
        id: ThreadId::from_uuid(
            uuid::Uuid::parse_str(&id_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        run_id: RunId::from_uuid(
            uuid::Uuid::parse_str(&run_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        channel: serde_json::from_str(&channel_str)?,
        messages: serde_json::from_str(&messages_str)?,
        metadata: serde_json::from_str(&meta_str)?,
    })
}

// ════════════════════════════════════════════════════════════════════
//  JobStore
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl JobStore for Database {
    async fn enqueue(&self, job: &Job) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let job = job.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO jobs (id, session_id, kind, payload, status, scheduled_at, started_at, completed_at, retries, max_retries, error, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    job.id.to_string(),
                    job.session_id.to_string(),
                    serde_json::to_string(&job.kind)?,
                    serde_json::to_string(&job.payload)?,
                    serde_json::to_string(&job.status)?,
                    job.scheduled_at.map(|dt| dt.to_rfc3339()),
                    job.started_at.map(|dt| dt.to_rfc3339()),
                    job.completed_at.map(|dt| dt.to_rfc3339()),
                    job.retries,
                    job.max_retries,
                    job.error,
                    serde_json::to_string(&job.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn dequeue(&self, kinds: &[JobKind]) -> rusvel_core::Result<Option<Job>> {
        let conn = self.conn.clone();
        let kinds = kinds.to_vec();
        tokio::task::spawn_blocking(move || {
            let mut db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let now = Utc::now().to_rfc3339();
            let queued_status = serde_json::to_string(&JobStatus::Queued)?;

            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            param_values.push(Box::new(queued_status.clone()));
            param_values.push(Box::new(now.clone()));

            let kind_filter = if kinds.is_empty() {
                String::new()
            } else {
                let placeholders: Vec<String> =
                    (0..kinds.len()).map(|i| format!("?{}", i + 3)).collect();
                for k in &kinds {
                    param_values.push(Box::new(serde_json::to_string(k)?));
                }
                format!(" AND kind IN ({})", placeholders.join(", "))
            };

            let sql = format!(
                "SELECT id, session_id, kind, payload, status, scheduled_at, started_at, completed_at, retries, max_retries, error, metadata
                 FROM jobs
                 WHERE status = ?1 AND (scheduled_at IS NULL OR scheduled_at <= ?2){}
                 ORDER BY rowid ASC
                 LIMIT 1",
                kind_filter
            );

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect();

            let tx = db
                .transaction_with_behavior(TransactionBehavior::Immediate)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let result = {
                let mut stmt = tx
                    .prepare(&sql)
                    .map_err(|e| RusvelError::Storage(e.to_string()))?;
                stmt
                    .query_row(params_refs.as_slice(), |row| Ok(row_to_job(row)))
                    .optional()
                    .map_err(|e| RusvelError::Storage(e.to_string()))?
            };

            match result {
                Some(job) => {
                    let job = job?;
                    let running_status = serde_json::to_string(&JobStatus::Running)?;
                    let started = Utc::now().to_rfc3339();
                    let n = tx
                        .execute(
                            "UPDATE jobs SET status = ?1, started_at = ?2 WHERE id = ?3 AND status = ?4",
                            params![
                                running_status,
                                started,
                                job.id.to_string(),
                                queued_status
                            ],
                        )
                        .map_err(|e| RusvelError::Storage(e.to_string()))?;
                    if n != 1 {
                        tx.rollback()
                            .map_err(|e| RusvelError::Storage(e.to_string()))?;
                        return Ok(None);
                    }
                    tx.commit()
                        .map_err(|e| RusvelError::Storage(e.to_string()))?;
                    let mut claimed = job;
                    claimed.status = JobStatus::Running;
                    claimed.started_at = Some(Utc::now());
                    Ok(Some(claimed))
                }
                None => {
                    tx.commit()
                        .map_err(|e| RusvelError::Storage(e.to_string()))?;
                    Ok(None)
                }
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn update(&self, job: &Job) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let job = job.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "UPDATE jobs SET
                   status = ?1, payload = ?2, scheduled_at = ?3, started_at = ?4,
                   completed_at = ?5, retries = ?6, max_retries = ?7, error = ?8, metadata = ?9
                 WHERE id = ?10",
                params![
                    serde_json::to_string(&job.status)?,
                    serde_json::to_string(&job.payload)?,
                    job.scheduled_at.map(|dt| dt.to_rfc3339()),
                    job.started_at.map(|dt| dt.to_rfc3339()),
                    job.completed_at.map(|dt| dt.to_rfc3339()),
                    job.retries,
                    job.max_retries,
                    job.error,
                    serde_json::to_string(&job.metadata)?,
                    job.id.to_string(),
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn get(&self, id: &JobId) -> rusvel_core::Result<Option<Job>> {
        let conn = self.conn.clone();
        let id = *id;
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let result = db
                .query_row(
                    "SELECT id, session_id, kind, payload, status, scheduled_at, started_at, completed_at, retries, max_retries, error, metadata FROM jobs WHERE id = ?1",
                    params![id.to_string()],
                    |row| Ok(row_to_job(row)),
                )
                .optional()
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            match result {
                Some(j) => Ok(Some(j?)),
                None => Ok(None),
            }
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn list(&self, filter: JobFilter) -> rusvel_core::Result<Vec<Job>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn.lock().map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut sql = String::from(
                "SELECT id, session_id, kind, payload, status, scheduled_at, started_at, completed_at, retries, max_retries, error, metadata FROM jobs WHERE 1=1",
            );
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;

            if let Some(ref sid) = filter.session_id {
                sql.push_str(&format!(" AND session_id = ?{idx}"));
                param_values.push(Box::new(sid.to_string()));
                idx += 1;
            }

            if !filter.kinds.is_empty() {
                let placeholders: Vec<String> = filter
                    .kinds
                    .iter()
                    .map(|_| {
                        let p = format!("?{idx}");
                        idx += 1;
                        p
                    })
                    .collect();
                sql.push_str(&format!(" AND kind IN ({})", placeholders.join(", ")));
                for k in &filter.kinds {
                    param_values.push(Box::new(serde_json::to_string(k)?));
                }
            }

            if !filter.statuses.is_empty() {
                let placeholders: Vec<String> = filter
                    .statuses
                    .iter()
                    .map(|_| {
                        let p = format!("?{idx}");
                        idx += 1;
                        p
                    })
                    .collect();
                sql.push_str(&format!(" AND status IN ({})", placeholders.join(", ")));
                for s in &filter.statuses {
                    param_values.push(Box::new(serde_json::to_string(s)?));
                }
            }

            sql.push_str(" ORDER BY rowid ASC");

            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT ?{idx}"));
                param_values.push(Box::new(i64::from(limit)));
                let _ = idx;
            }

            let mut stmt = db
                .prepare(&sql)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect();

            let rows = stmt
                .query_map(params_refs.as_slice(), |row| Ok(row_to_job(row)))
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut jobs = Vec::new();
            for row in rows {
                let job = row.map_err(|e| RusvelError::Storage(e.to_string()))??;
                jobs.push(job);
            }
            Ok(jobs)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }
}

fn row_to_job(row: &rusqlite::Row<'_>) -> rusvel_core::Result<Job> {
    let id_str: String = row
        .get(0)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let session_str: String = row
        .get(1)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let kind_str: String = row
        .get(2)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let payload_str: String = row
        .get(3)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let status_str: String = row
        .get(4)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let scheduled_str: Option<String> = row
        .get(5)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let started_str: Option<String> = row
        .get(6)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let completed_str: Option<String> = row
        .get(7)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let retries: u32 = row
        .get(8)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let max_retries: u32 = row
        .get(9)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let error: Option<String> = row
        .get(10)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let meta_str: String = row
        .get(11)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

    fn parse_opt_dt(s: Option<String>) -> rusvel_core::Result<Option<DateTime<Utc>>> {
        match s {
            Some(s) => Ok(Some(
                DateTime::parse_from_rfc3339(&s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| RusvelError::Storage(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    Ok(Job {
        id: JobId::from_uuid(
            uuid::Uuid::parse_str(&id_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        session_id: SessionId::from_uuid(
            uuid::Uuid::parse_str(&session_str).map_err(|e| RusvelError::Storage(e.to_string()))?,
        ),
        kind: serde_json::from_str(&kind_str)?,
        payload: serde_json::from_str(&payload_str)?,
        status: serde_json::from_str(&status_str)?,
        scheduled_at: parse_opt_dt(scheduled_str)?,
        started_at: parse_opt_dt(started_str)?,
        completed_at: parse_opt_dt(completed_str)?,
        retries,
        max_retries,
        error,
        metadata: serde_json::from_str(&meta_str)?,
    })
}

fn job_from_new_job(new: NewJob) -> Job {
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

// ════════════════════════════════════════════════════════════════════
//  JobPort (SQLite-backed; same store as JobStore / StoragePort::jobs)
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl JobPort for Database {
    async fn enqueue(&self, new: NewJob) -> rusvel_core::Result<JobId> {
        let job = job_from_new_job(new);
        let id = job.id;
        JobStore::enqueue(self, &job).await?;
        Ok(id)
    }

    async fn dequeue(&self, kinds: &[JobKind]) -> rusvel_core::Result<Option<Job>> {
        JobStore::dequeue(self, kinds).await
    }

    async fn complete(&self, id: &JobId, result: JobResult) -> rusvel_core::Result<()> {
        let mut job = JobStore::get(self, id)
            .await?
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
        JobStore::update(self, &job).await
    }

    async fn hold_for_approval(&self, id: &JobId, result: JobResult) -> rusvel_core::Result<()> {
        let mut job = JobStore::get(self, id)
            .await?
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
        JobStore::update(self, &job).await
    }

    async fn fail(&self, id: &JobId, error: String) -> rusvel_core::Result<()> {
        let mut job = JobStore::get(self, id)
            .await?
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
        JobStore::update(self, &job).await
    }

    async fn schedule(&self, new: NewJob, cron: &str) -> rusvel_core::Result<JobId> {
        let mut job = job_from_new_job(new);
        job.scheduled_at = Some(Utc::now());
        job.metadata["cron"] = serde_json::Value::String(cron.to_string());
        let id = job.id;
        JobStore::enqueue(self, &job).await?;
        Ok(id)
    }

    async fn cancel(&self, id: &JobId) -> rusvel_core::Result<()> {
        let mut job = JobStore::get(self, id)
            .await?
            .ok_or_else(|| RusvelError::NotFound {
                kind: "Job".into(),
                id: id.to_string(),
            })?;

        match job.status {
            JobStatus::Queued | JobStatus::AwaitingApproval => {
                job.status = JobStatus::Cancelled;
                job.completed_at = Some(Utc::now());
                JobStore::update(self, &job).await
            }
            _ => Err(RusvelError::InvalidState {
                from: format!("{:?}", job.status),
                to: "Cancelled".into(),
            }),
        }
    }

    async fn approve(&self, id: &JobId) -> rusvel_core::Result<()> {
        let mut job = JobStore::get(self, id)
            .await?
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
        JobStore::update(self, &job).await
    }

    async fn list(&self, filter: JobFilter) -> rusvel_core::Result<Vec<Job>> {
        JobStore::list(self, filter).await
    }
}

// ════════════════════════════════════════════════════════════════════
//  MetricStore
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl MetricStore for Database {
    async fn record(&self, point: &MetricPoint) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let point = point.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO metrics (name, value, tags, recorded_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    point.name,
                    point.value,
                    serde_json::to_string(&point.tags)?,
                    point.recorded_at.to_rfc3339(),
                    serde_json::to_string(&point.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn query(&self, filter: MetricFilter) -> rusvel_core::Result<Vec<MetricPoint>> {
        let conn = self.conn.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut sql = String::from(
                "SELECT name, value, tags, recorded_at, metadata FROM metrics WHERE 1=1",
            );
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;

            if let Some(ref name) = filter.name {
                sql.push_str(&format!(" AND name = ?{idx}"));
                param_values.push(Box::new(name.clone()));
                idx += 1;
            }
            if let Some(ref since) = filter.since {
                sql.push_str(&format!(" AND recorded_at >= ?{idx}"));
                param_values.push(Box::new(since.to_rfc3339()));
                idx += 1;
            }
            if let Some(ref until) = filter.until {
                sql.push_str(&format!(" AND recorded_at <= ?{idx}"));
                param_values.push(Box::new(until.to_rfc3339()));
                idx += 1;
            }

            sql.push_str(" ORDER BY recorded_at ASC");

            if let Some(limit) = filter.limit {
                sql.push_str(&format!(" LIMIT ?{idx}"));
                param_values.push(Box::new(i64::from(limit)));
                let _ = idx;
            }

            let mut stmt = db
                .prepare(&sql)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect();

            let rows = stmt
                .query_map(params_refs.as_slice(), |row| {
                    let name: String = row.get(0)?;
                    let value: f64 = row.get(1)?;
                    let tags_str: String = row.get(2)?;
                    let recorded_str: String = row.get(3)?;
                    let meta_str: String = row.get(4)?;
                    Ok((name, value, tags_str, recorded_str, meta_str))
                })
                .map_err(|e| RusvelError::Storage(e.to_string()))?;

            let mut points = Vec::new();
            for row in rows {
                let (name, value, tags_str, recorded_str, meta_str) =
                    row.map_err(|e| RusvelError::Storage(e.to_string()))?;
                points.push(MetricPoint {
                    name,
                    value,
                    tags: serde_json::from_str(&tags_str)?,
                    recorded_at: DateTime::parse_from_rfc3339(&recorded_str)
                        .map(|dt| dt.with_timezone(&Utc))
                        .map_err(|e| RusvelError::Storage(e.to_string()))?,
                    metadata: serde_json::from_str(&meta_str)?,
                });
            }
            Ok(points)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn record_cost(&self, event: CostEvent) -> rusvel_core::Result<()> {
        let conn = self.conn.clone();
        let event = event.clone();
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            db.execute(
                "INSERT INTO cost_events (id, session_id, department_id, model, provider,
                 input_tokens, output_tokens, cost_usd, operation, created_at, metadata)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    event.id,
                    event.session_id.map(|s| s.to_string()),
                    event.department_id,
                    event.model,
                    event.provider,
                    event.input_tokens,
                    event.output_tokens,
                    event.cost_usd,
                    event.operation,
                    event.created_at.to_rfc3339(),
                    serde_json::to_string(&event.metadata)?,
                ],
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
            Ok(())
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }

    async fn query_costs(
        &self,
        session_id: Option<&SessionId>,
        department_id: Option<&str>,
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
    ) -> rusvel_core::Result<Vec<CostEvent>> {
        let conn = self.conn.clone();
        let sid = session_id.map(|s| s.to_string());
        let dept = department_id.map(str::to_string);
        tokio::task::spawn_blocking(move || {
            let db = conn
                .lock()
                .map_err(|e| RusvelError::Storage(format!("mutex poisoned: {e}")))?;
            let mut sql = String::from(
                "SELECT id, session_id, department_id, model, provider, input_tokens, output_tokens,
                        cost_usd, operation, created_at, metadata FROM cost_events WHERE 1=1",
            );
            let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
            let mut idx = 1;
            if let Some(ref s) = sid {
                sql.push_str(&format!(" AND session_id = ?{idx}"));
                param_values.push(Box::new(s.clone()));
                idx += 1;
            }
            if let Some(ref d) = dept {
                sql.push_str(&format!(" AND department_id = ?{idx}"));
                param_values.push(Box::new(d.clone()));
                idx += 1;
            }
            if let Some(ref t) = from {
                sql.push_str(&format!(" AND created_at >= ?{idx}"));
                param_values.push(Box::new(t.to_rfc3339()));
                idx += 1;
            }
            if let Some(ref t) = to {
                sql.push_str(&format!(" AND created_at <= ?{idx}"));
                param_values.push(Box::new(t.to_rfc3339()));
                let _ = idx;
            }
            sql.push_str(" ORDER BY created_at ASC");
            let mut stmt = db
                .prepare(&sql)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
                .iter()
                .map(std::convert::AsRef::as_ref)
                .collect();
            let rows = stmt
                .query_map(params_refs.as_slice(), |row| {
                    let id: String = row.get(0)?;
                    let session_s: Option<String> = row.get(1)?;
                    let department_id: Option<String> = row.get(2)?;
                    let model: String = row.get(3)?;
                    let provider: String = row.get(4)?;
                    let input_tokens: u32 = row.get(5)?;
                    let output_tokens: u32 = row.get(6)?;
                    let cost_usd: f64 = row.get(7)?;
                    let operation: String = row.get(8)?;
                    let created_s: String = row.get(9)?;
                    let meta_s: String = row.get(10)?;
                    Ok((
                        id,
                        session_s,
                        department_id,
                        model,
                        provider,
                        input_tokens,
                        output_tokens,
                        cost_usd,
                        operation,
                        created_s,
                        meta_s,
                    ))
                })
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            let mut out = Vec::new();
            for row in rows {
                let (
                    id,
                    session_s,
                    department_id,
                    model,
                    provider,
                    input_tokens,
                    output_tokens,
                    cost_usd,
                    operation,
                    created_s,
                    meta_s,
                ) = row.map_err(|e| RusvelError::Storage(e.to_string()))?;
                let session_id = session_s
                    .and_then(|s| uuid::Uuid::parse_str(&s).ok())
                    .map(SessionId::from);
                let created_at = DateTime::parse_from_rfc3339(&created_s)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|e| RusvelError::Storage(e.to_string()))?;
                let metadata: serde_json::Value = serde_json::from_str(&meta_s)
                    .map_err(|e| RusvelError::Storage(e.to_string()))?;
                out.push(CostEvent {
                    id,
                    session_id,
                    department_id,
                    model,
                    provider,
                    input_tokens,
                    output_tokens,
                    cost_usd,
                    operation,
                    created_at,
                    metadata,
                });
            }
            Ok(out)
        })
        .await
        .map_err(|e| RusvelError::Internal(format!("spawn_blocking: {e}")))?
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::*;
    #[allow(unused_imports)]
    use rusvel_core::id::*;

    fn test_db() -> Database {
        Database::in_memory().expect("failed to create test database")
    }

    fn make_session() -> Session {
        Session {
            id: SessionId::new(),
            name: "Test Session".into(),
            kind: SessionKind::General,
            tags: vec!["test".into()],
            config: SessionConfig::default(),
            created_at: Utc::now(),
            updated_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    // ── EventStore tests ──────────────────────────────────────────

    #[tokio::test]
    async fn event_append_and_get() {
        let db = test_db();
        let event = Event {
            id: EventId::new(),
            session_id: Some(SessionId::new()),
            run_id: None,
            source: "forge".into(),
            kind: "test.event".into(),
            payload: serde_json::json!({"key": "value"}),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };

        db.append(&event).await.unwrap();
        let retrieved = EventStore::get(&db, &event.id).await.unwrap().unwrap();
        assert_eq!(retrieved.id, event.id);
        assert_eq!(retrieved.kind, "test.event");
    }

    #[tokio::test]
    async fn event_query_by_kind() {
        let db = test_db();
        let sid = SessionId::new();

        for i in 0..3 {
            let event = Event {
                id: EventId::new(),
                session_id: Some(sid),
                run_id: None,
                source: "forge".into(),
                kind: if i < 2 {
                    "test.a".into()
                } else {
                    "test.b".into()
                },
                payload: serde_json::json!({}),
                created_at: Utc::now(),
                metadata: serde_json::json!({}),
            };
            db.append(&event).await.unwrap();
        }

        let filter = EventFilter {
            kind: Some("test.a".into()),
            ..Default::default()
        };
        let results = EventStore::query(&db, filter).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    // ── ObjectStore tests ─────────────────────────────────────────

    #[tokio::test]
    async fn object_put_get_delete() {
        let db = test_db();
        let obj = serde_json::json!({"name": "Widget", "price": 42});

        ObjectStore::put(&db, "product", "p1", obj.clone())
            .await
            .unwrap();
        let got = ObjectStore::get(&db, "product", "p1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(got["name"], "Widget");

        ObjectStore::delete(&db, "product", "p1").await.unwrap();
        let gone = ObjectStore::get(&db, "product", "p1").await.unwrap();
        assert!(gone.is_none());
    }

    #[tokio::test]
    async fn object_list() {
        let db = test_db();
        for i in 0..5 {
            ObjectStore::put(&db, "item", &format!("i{i}"), serde_json::json!({"n": i}))
                .await
                .unwrap();
        }

        let all = ObjectStore::list(&db, "item", ObjectFilter::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 5);

        let limited = ObjectStore::list(
            &db,
            "item",
            ObjectFilter {
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(limited.len(), 2);
    }

    #[tokio::test]
    async fn object_list_filters_by_session_id_json() {
        let db = test_db();
        let sid_a = SessionId::new();
        let sid_b = SessionId::new();

        ObjectStore::put(
            &db,
            "content",
            "c1",
            serde_json::json!({
                "id": "00000000-0000-0000-0000-000000000001",
                "session_id": sid_a.to_string(),
                "title": "A",
            }),
        )
        .await
        .unwrap();
        ObjectStore::put(
            &db,
            "content",
            "c2",
            serde_json::json!({
                "id": "00000000-0000-0000-0000-000000000002",
                "session_id": sid_b.to_string(),
                "title": "B",
            }),
        )
        .await
        .unwrap();

        let for_a = ObjectStore::list(
            &db,
            "content",
            ObjectFilter {
                session_id: Some(sid_a),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(for_a.len(), 1);
        assert_eq!(for_a[0]["title"], "A");

        let for_b = ObjectStore::list(
            &db,
            "content",
            ObjectFilter {
                session_id: Some(sid_b),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(for_b.len(), 1);
        assert_eq!(for_b[0]["title"], "B");
    }

    // ── SessionStore tests ────────────────────────────────────────

    #[tokio::test]
    async fn session_put_and_get() {
        let db = test_db();
        let session = make_session();

        db.put_session(&session).await.unwrap();
        let got = db.get_session(&session.id).await.unwrap().unwrap();
        assert_eq!(got.name, "Test Session");
    }

    #[tokio::test]
    async fn session_list_summaries() {
        let db = test_db();
        let s1 = make_session();
        let mut s2 = make_session();
        s2.name = "Second".into();

        db.put_session(&s1).await.unwrap();
        db.put_session(&s2).await.unwrap();

        let summaries = db.list_sessions().await.unwrap();
        assert_eq!(summaries.len(), 2);
    }

    #[tokio::test]
    async fn run_and_thread_crud() {
        let db = test_db();
        let session = make_session();
        db.put_session(&session).await.unwrap();

        let run = Run {
            id: RunId::new(),
            session_id: session.id,
            engine: "code".into(),
            input_summary: "analyze repo".into(),
            status: RunStatus::Running,
            llm_budget_used: 0.0,
            tool_calls_count: 0,
            started_at: Utc::now(),
            completed_at: None,
            metadata: serde_json::json!({}),
        };
        db.put_run(&run).await.unwrap();
        let got_run = db.get_run(&run.id).await.unwrap().unwrap();
        assert_eq!(got_run.input_summary, "analyze repo");

        let runs = db.list_runs(&session.id).await.unwrap();
        assert_eq!(runs.len(), 1);

        let thread = Thread {
            id: ThreadId::new(),
            run_id: run.id,
            channel: ThreadChannel::User,
            messages: vec![],
            metadata: serde_json::json!({}),
        };
        db.put_thread(&thread).await.unwrap();
        let got_thread = db.get_thread(&thread.id).await.unwrap().unwrap();
        assert_eq!(got_thread.run_id, run.id);

        let threads = db.list_threads(&run.id).await.unwrap();
        assert_eq!(threads.len(), 1);
    }

    // ── JobStore tests ────────────────────────────────────────────

    #[tokio::test]
    async fn job_enqueue_dequeue() {
        let db = test_db();
        let job = Job {
            id: JobId::new(),
            session_id: SessionId::new(),
            kind: JobKind::AgentRun,
            payload: serde_json::json!({"task": "analyze"}),
            status: JobStatus::Queued,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retries: 0,
            max_retries: 3,
            error: None,
            metadata: serde_json::json!({}),
        };

        JobStore::enqueue(&db, &job).await.unwrap();

        let dequeued = JobStore::dequeue(&db, &[JobKind::AgentRun])
            .await
            .unwrap()
            .unwrap();
        assert_eq!(dequeued.id, job.id);
        assert_eq!(dequeued.status, JobStatus::Running);

        // No more jobs to dequeue
        let none = JobStore::dequeue(&db, &[JobKind::AgentRun]).await.unwrap();
        assert!(none.is_none());
    }

    #[tokio::test]
    async fn job_dequeue_empty_kinds_matches_any_kind() {
        let db = test_db();
        let job = Job {
            id: JobId::new(),
            session_id: SessionId::new(),
            kind: JobKind::HarvestScan,
            payload: serde_json::json!({}),
            status: JobStatus::Queued,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retries: 0,
            max_retries: 3,
            error: None,
            metadata: serde_json::json!({}),
        };

        JobStore::enqueue(&db, &job).await.unwrap();

        let dequeued = JobStore::dequeue(&db, &[]).await.unwrap().unwrap();
        assert_eq!(dequeued.id, job.id);
        assert_eq!(dequeued.kind, JobKind::HarvestScan);
    }

    #[tokio::test]
    async fn job_update_and_list() {
        let db = test_db();
        let sid = SessionId::new();
        let mut job = Job {
            id: JobId::new(),
            session_id: sid,
            kind: JobKind::ContentPublish,
            payload: serde_json::json!({}),
            status: JobStatus::Queued,
            scheduled_at: None,
            started_at: None,
            completed_at: None,
            retries: 0,
            max_retries: 3,
            error: None,
            metadata: serde_json::json!({}),
        };

        JobStore::enqueue(&db, &job).await.unwrap();

        job.status = JobStatus::Succeeded;
        job.completed_at = Some(Utc::now());
        JobStore::update(&db, &job).await.unwrap();

        let got = JobStore::get(&db, &job.id).await.unwrap().unwrap();
        assert_eq!(got.status, JobStatus::Succeeded);

        let filtered = JobStore::list(
            &db,
            JobFilter {
                statuses: vec![JobStatus::Succeeded],
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(filtered.len(), 1);
    }

    // ── MetricStore tests ─────────────────────────────────────────

    #[tokio::test]
    async fn metric_record_and_query() {
        let db = test_db();
        let now = Utc::now();

        for i in 0..5 {
            let point = MetricPoint {
                name: "cpu.usage".into(),
                value: 50.0 + i as f64,
                tags: vec!["host:alpha".into()],
                recorded_at: now + chrono::Duration::seconds(i),
                metadata: serde_json::json!({}),
            };
            MetricStore::record(&db, &point).await.unwrap();
        }

        // Also record a different metric
        MetricStore::record(
            &db,
            &MetricPoint {
                name: "mem.usage".into(),
                value: 80.0,
                tags: vec![],
                recorded_at: now,
                metadata: serde_json::json!({}),
            },
        )
        .await
        .unwrap();

        let cpu = MetricStore::query(
            &db,
            MetricFilter {
                name: Some("cpu.usage".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(cpu.len(), 5);

        let limited = MetricStore::query(
            &db,
            MetricFilter {
                name: Some("cpu.usage".into()),
                limit: Some(2),
                ..Default::default()
            },
        )
        .await
        .unwrap();
        assert_eq!(limited.len(), 2);

        let all = MetricStore::query(&db, MetricFilter::default())
            .await
            .unwrap();
        assert_eq!(all.len(), 6);
    }

    #[tokio::test]
    async fn cost_events_spend_snapshot_empty() {
        let db = test_db();
        let snap = db.cost_events_spend_snapshot().await.unwrap();
        assert_eq!(snap.event_row_count, 0);
        assert!(snap.by_department_usd.is_empty());
        assert_eq!(snap.total_usd_all, 0.0);
    }

    #[tokio::test]
    async fn cost_events_spend_snapshot_aggregates() {
        let db = test_db();
        let sid = SessionId::new();
        let base = Utc::now();
        let e1 = CostEvent {
            id: "ce-a".into(),
            session_id: Some(sid),
            department_id: Some("harvest".into()),
            model: "m1".into(),
            provider: "ollama".into(),
            input_tokens: 10,
            output_tokens: 20,
            cost_usd: 1.0,
            operation: "generate".into(),
            created_at: base,
            metadata: serde_json::json!({}),
        };
        let e2 = CostEvent {
            id: "ce-b".into(),
            session_id: Some(sid),
            department_id: None,
            model: "m1".into(),
            provider: "ollama".into(),
            input_tokens: 5,
            output_tokens: 5,
            cost_usd: 2.0,
            operation: "generate".into(),
            created_at: base,
            metadata: serde_json::json!({}),
        };
        MetricStore::record_cost(&db, e1).await.unwrap();
        MetricStore::record_cost(&db, e2).await.unwrap();

        let snap = db.cost_events_spend_snapshot().await.unwrap();
        assert_eq!(snap.event_row_count, 2);
        assert!((snap.total_usd_all - 3.0).abs() < 1e-9);
        assert!((snap.by_department_usd["harvest"] - 1.0).abs() < 1e-9);
        assert!((snap.by_department_usd["_none"] - 2.0).abs() < 1e-9);
        assert_eq!(snap.by_department_tokens["harvest"], 30);
        assert_eq!(snap.by_department_tokens["_none"], 10);
        assert!((snap.by_session_usd[&sid.to_string()] - 3.0).abs() < 1e-9);
        let m = snap.by_model.get("m1").expect("model m1");
        assert!((m.0 - 3.0).abs() < 1e-9);
        assert_eq!(m.1, 40);
    }

    // ── StoragePort trait object ──────────────────────────────────

    #[tokio::test]
    async fn storage_port_dispatches_to_substores() {
        let db = test_db();
        let storage: &dyn StoragePort = &db;

        // Just verify we can call through the trait object
        let events = storage
            .events()
            .query(EventFilter::default())
            .await
            .unwrap();
        assert!(events.is_empty());

        let objects = storage
            .objects()
            .list("nonexistent", ObjectFilter::default())
            .await
            .unwrap();
        assert!(objects.is_empty());

        let sessions = storage.sessions().list_sessions().await.unwrap();
        assert!(sessions.is_empty());

        let jobs = storage.jobs().list(JobFilter::default()).await.unwrap();
        assert!(jobs.is_empty());

        let metrics = storage
            .metrics()
            .query(MetricFilter::default())
            .await
            .unwrap();
        assert!(metrics.is_empty());
    }
}
