//! RusvelBase / SQL console port — blocking SQLite work for `/api/db/*`.

use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use rusqlite::{Connection, Statement, types::ValueRef};
use serde_json::{Number, Value};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::{
    RusvelBaseColumnMeta, RusvelBasePagedRows, RusvelBasePort, RusvelBaseSqlExecute,
    RusvelBaseTableSummary,
};
use rusvel_schema::SchemaIntrospector;

use crate::Database;

/// Wraps [`Arc<Database>`] for [`RusvelBasePort`] (spawn_blocking inside each call).
pub struct RusvelBaseAdapter(pub Arc<Database>);

/// When `RUSVEL_DB_SQL_WRITE` is `0`, `false`, or `off`, [`RusvelBaseAdapter::execute_sql`]
/// always runs with `PRAGMA query_only = ON` (writes blocked) regardless of `read_only`.
fn env_disallows_sql_writes() -> bool {
    std::env::var("RUSVEL_DB_SQL_WRITE")
        .map(|v| {
            let v = v.trim();
            v == "0" || v.eq_ignore_ascii_case("false") || v.eq_ignore_ascii_case("off")
        })
        .unwrap_or(false)
}

fn statement_columns(stmt: &Statement<'_>) -> Vec<RusvelBaseColumnMeta> {
    stmt.columns()
        .into_iter()
        .map(|c| RusvelBaseColumnMeta {
            name: c.name().to_string(),
            col_type: c.decl_type().unwrap_or("").to_string(),
        })
        .collect()
}

fn value_ref_to_json(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => Value::Number(Number::from(i)),
        ValueRef::Real(f) => Number::from_f64(f)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::String(format!("<blob {} bytes>", b.len())),
    }
}

fn run_sql(
    conn: &Connection,
    query: &str,
) -> Result<(Vec<RusvelBaseColumnMeta>, Vec<Vec<Value>>, usize)> {
    let query = query.trim();
    if query.is_empty() {
        return Err(RusvelError::Validation("empty query".into()));
    }
    let mut stmt = conn
        .prepare(query)
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    let ncols = stmt.column_count();
    if ncols == 0 {
        let n = stmt
            .execute([])
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        return Ok((vec![], vec![], n));
    }
    let cols = statement_columns(&stmt);
    let mut rows = Vec::new();
    let mut rows_iter = stmt
        .query([])
        .map_err(|e| RusvelError::Storage(e.to_string()))?;
    while let Some(row) = rows_iter
        .next()
        .map_err(|e| RusvelError::Storage(e.to_string()))?
    {
        let mut r = Vec::with_capacity(ncols);
        for i in 0..ncols {
            let cell = row
                .get_ref(i)
                .map_err(|e| RusvelError::Storage(e.to_string()))?;
            r.push(value_ref_to_json(cell));
        }
        rows.push(r);
    }
    let n = rows.len();
    Ok((cols, rows, n))
}

fn join_err(e: tokio::task::JoinError) -> RusvelError {
    RusvelError::Storage(format!("spawn_blocking join: {e}"))
}

#[async_trait]
impl RusvelBasePort for RusvelBaseAdapter {
    async fn list_table_summaries(&self) -> Result<Vec<RusvelBaseTableSummary>> {
        let db = self.0.clone();
        tokio::task::spawn_blocking(move || {
            db.with_connection(|conn| {
                SchemaIntrospector::list_tables(conn).map(|v| {
                    v.into_iter()
                        .map(|t| RusvelBaseTableSummary {
                            name: t.name,
                            row_count: t.row_count,
                        })
                        .collect()
                })
            })
        })
        .await
        .map_err(join_err)?
    }

    async fn get_table_schema_json(&self, table: &str) -> Result<Value> {
        let db = self.0.clone();
        let table = table.to_string();
        tokio::task::spawn_blocking(move || {
            db.with_connection(|conn| {
                SchemaIntrospector::get_table(conn, &table).and_then(|info| {
                    serde_json::to_value(&info)
                        .map_err(|e| RusvelError::Serialization(e.to_string()))
                })
            })
        })
        .await
        .map_err(join_err)?
    }

    async fn query_table_rows(
        &self,
        table: &str,
        limit: u32,
        offset: u32,
        order: Option<&str>,
    ) -> Result<RusvelBasePagedRows> {
        let db = self.0.clone();
        let table = table.to_string();
        let order = order.map(|s| s.to_string());
        tokio::task::spawn_blocking(move || {
            db.with_connection(|conn| {
                if !SchemaIntrospector::validate_table_name(&table) {
                    return Err(RusvelError::Validation(format!("invalid table: {table}")));
                }
                let info = SchemaIntrospector::get_table(conn, &table)?;
                let order_clause = if let Some(ref o) = order {
                    let (col, desc) = rusvel_schema::parse_order_column_spec(o)?;
                    if !SchemaIntrospector::validate_column_for_table(conn, &table, &col)? {
                        return Err(RusvelError::Validation(format!("unknown column: {col}")));
                    }
                    let dir = if desc { "DESC" } else { "ASC" };
                    format!(r#" ORDER BY "{col}" {dir}"#)
                } else {
                    String::new()
                };
                let sql = format!(r#"SELECT * FROM "{table}"{order_clause} LIMIT ? OFFSET ?"#);
                let mut stmt = conn
                    .prepare(&sql)
                    .map_err(|e| RusvelError::Storage(e.to_string()))?;
                let cols = statement_columns(&stmt);
                let ncols = cols.len();
                let limit_i = i64::from(limit);
                let offset_i = i64::from(offset);
                let mut rows = Vec::new();
                let mut rows_iter = stmt
                    .query(rusqlite::params![limit_i, offset_i])
                    .map_err(|e| RusvelError::Storage(e.to_string()))?;
                while let Some(row) = rows_iter
                    .next()
                    .map_err(|e| RusvelError::Storage(e.to_string()))?
                {
                    let mut r = Vec::with_capacity(ncols);
                    for i in 0..ncols {
                        let cell = row
                            .get_ref(i)
                            .map_err(|e| RusvelError::Storage(e.to_string()))?;
                        r.push(value_ref_to_json(cell));
                    }
                    rows.push(r);
                }
                let row_count = rows.len();
                Ok(RusvelBasePagedRows {
                    columns: cols,
                    rows,
                    row_count,
                    table_row_count: info.row_count,
                })
            })
        })
        .await
        .map_err(join_err)?
    }

    /// Runs ad-hoc SQL. When env `RUSVEL_DB_SQL_WRITE` is `0`/`false`/`off`, `read_only` is forced on.
    async fn execute_sql(&self, sql: &str, read_only: bool) -> Result<RusvelBaseSqlExecute> {
        let db = self.0.clone();
        let sql = sql.to_string();
        let read_only = env_disallows_sql_writes() || read_only;
        tokio::task::spawn_blocking(move || {
            db.with_connection(|conn| {
                let start = Instant::now();
                if read_only {
                    conn.execute_batch("PRAGMA query_only = ON;")
                        .map_err(|e| RusvelError::Storage(e.to_string()))?;
                }
                let res = run_sql(conn, &sql);
                if read_only {
                    let _ = conn.execute_batch("PRAGMA query_only = OFF;");
                }
                let (columns, rows, row_count) = res?;
                let duration_ms = start.elapsed().as_millis() as u64;
                Ok(RusvelBaseSqlExecute {
                    columns,
                    rows,
                    row_count,
                    duration_ms,
                })
            })
        })
        .await
        .map_err(join_err)?
    }
}
