//! SQLite schema introspection for RusvelBase (tables, columns, indexes, FKs).
//!
//! All dynamic SQL building in the API layer should validate identifiers through
//! [`SchemaIntrospector::validate_table_name`] and column checks against [`TableInfo`].

use rusqlite::{Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use rusvel_core::Result;
use rusvel_core::error::RusvelError;

/// User-facing table listing (name + approximate row count).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableSummary {
    pub name: String,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    pub col_type: String,
    pub nullable: bool,
    pub default_value: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub unique: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForeignKeyInfo {
    pub from_column: String,
    pub to_table: String,
    pub to_column: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TableInfo {
    pub name: String,
    pub columns: Vec<ColumnInfo>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
    pub row_count: u64,
}

/// Introspects SQLite metadata via PRAGMAs and `sqlite_master`.
pub struct SchemaIntrospector;

impl SchemaIntrospector {
    /// Allowed unquoted SQL identifiers: ASCII alphanumeric + underscore (MVP safety).
    pub fn validate_table_name(name: &str) -> bool {
        !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    /// Same rules as table names for column identifiers we interpolate into SQL.
    pub fn validate_column_name(name: &str) -> bool {
        Self::validate_table_name(name)
    }

    /// True if `table` exists as a user table and `column` is a column on it.
    pub fn validate_column_for_table(conn: &Connection, table: &str, column: &str) -> Result<bool> {
        if !Self::validate_table_name(table) || !Self::validate_column_name(column) {
            return Ok(false);
        }
        let info = Self::get_table(conn, table)?;
        Ok(info.columns.iter().any(|c| c.name == column))
    }

    /// All non-internal tables with row counts.
    pub fn list_tables(conn: &Connection) -> Result<Vec<TableSummary>> {
        let mut stmt = conn
            .prepare(
                "SELECT name FROM sqlite_master \
                 WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
                 ORDER BY name",
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let names: Vec<String> = stmt
            .query_map([], |row| row.get(0))
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let mut out = Vec::with_capacity(names.len());
        for name in names {
            if !Self::validate_table_name(&name) {
                continue;
            }
            let row_count = Self::count_rows(conn, &name)?;
            out.push(TableSummary { name, row_count });
        }
        Ok(out)
    }

    /// Full metadata for one table, or validation error if missing / unsafe name.
    pub fn get_table(conn: &Connection, name: &str) -> Result<TableInfo> {
        if !Self::validate_table_name(name) {
            return Err(RusvelError::Validation(format!(
                "invalid table name: {name}"
            )));
        }
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1 LIMIT 1",
                [name],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .is_some();
        if !exists {
            return Err(RusvelError::NotFound {
                kind: "table".into(),
                id: name.to_string(),
            });
        }

        let columns = Self::read_columns(conn, name)?;
        let indexes = Self::read_indexes(conn, name)?;
        let foreign_keys = Self::read_foreign_keys(conn, name)?;
        let row_count = Self::count_rows(conn, name)?;

        Ok(TableInfo {
            name: name.to_string(),
            columns,
            indexes,
            foreign_keys,
            row_count,
        })
    }

    fn count_rows(conn: &Connection, table: &str) -> Result<u64> {
        let sql = format!("SELECT COUNT(*) FROM \"{table}\"");
        let n: i64 = conn
            .query_row(&sql, [], |row| row.get(0))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        Ok(n.max(0) as u64)
    }

    fn read_columns(conn: &Connection, table: &str) -> Result<Vec<ColumnInfo>> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA table_info(\"{table}\")"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let cols = stmt
            .query_map([], |row| Self::map_table_info_row(row))
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        Ok(cols)
    }

    fn map_table_info_row(row: &Row<'_>) -> std::result::Result<ColumnInfo, rusqlite::Error> {
        let name: String = row.get(1)?;
        let col_type: String = row.get(2)?;
        let notnull: i64 = row.get(3)?;
        let default_value: Option<String> = row.get(4)?;
        let pk: i64 = row.get(5)?;
        Ok(ColumnInfo {
            name,
            col_type,
            nullable: notnull == 0,
            default_value,
            primary_key: pk != 0,
        })
    }

    fn read_indexes(conn: &Connection, table: &str) -> Result<Vec<IndexInfo>> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA index_list(\"{table}\")"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        // PRAGMA index_list: seq, name, unique, origin, partial
        let index_names: Vec<(String, bool)> = stmt
            .query_map([], |row| {
                let name: String = row.get(1)?;
                let unique: i64 = row.get(2)?;
                Ok((name, unique != 0))
            })
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let mut out = Vec::new();
        for (idx_name, unique) in index_names {
            if idx_name.starts_with("sqlite_") {
                continue;
            }
            let columns = Self::index_columns(conn, &idx_name)?;
            out.push(IndexInfo {
                name: idx_name,
                columns,
                unique,
            });
        }
        Ok(out)
    }

    fn index_columns(conn: &Connection, index_name: &str) -> Result<Vec<String>> {
        let safe: String = index_name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if safe != index_name || safe.is_empty() {
            return Ok(vec![]);
        }
        let mut stmt = conn
            .prepare(&format!("PRAGMA index_info(\"{safe}\")"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(2))
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        Ok(cols)
    }

    fn read_foreign_keys(conn: &Connection, table: &str) -> Result<Vec<ForeignKeyInfo>> {
        let mut stmt = conn
            .prepare(&format!("PRAGMA foreign_key_list(\"{table}\")"))
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        let fks = stmt
            .query_map([], |row| {
                Ok(ForeignKeyInfo {
                    from_column: row.get::<_, String>(3)?,
                    to_table: row.get::<_, String>(2)?,
                    to_column: row.get::<_, String>(4)?,
                })
            })
            .map_err(|e| RusvelError::Storage(e.to_string()))?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(|e| RusvelError::Storage(e.to_string()))?;
        Ok(fks)
    }
}

/// Parse a single SQLite `ORDER BY` column parameter for RusvelBase table browsing:
/// `col`, `col.asc`, or `col.desc` (case-insensitive). Returns `(column_name, descending)`.
pub fn parse_order_column_spec(s: &str) -> Result<(String, bool)> {
    let s = s.trim();
    if s.is_empty() {
        return Err(RusvelError::Validation("empty order".into()));
    }
    if let Some((col, dir)) = s.rsplit_once('.') {
        match dir.to_ascii_lowercase().as_str() {
            "asc" => {
                if SchemaIntrospector::validate_column_name(col) {
                    return Ok((col.to_string(), false));
                }
            }
            "desc" => {
                if SchemaIntrospector::validate_column_name(col) {
                    return Ok((col.to_string(), true));
                }
            }
            _ => {}
        }
    }
    if SchemaIntrospector::validate_column_name(s) {
        return Ok((s.to_string(), false));
    }
    Err(RusvelError::Validation(format!("invalid order: {s}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn sample_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
             CREATE TABLE books (
               id INTEGER PRIMARY KEY,
               title TEXT,
               author_id INTEGER REFERENCES authors(id)
             );
             CREATE INDEX idx_books_title ON books(title);",
        )
        .unwrap();
        conn
    }

    #[test]
    fn list_tables_includes_user_tables() {
        let conn = sample_db();
        let tables = SchemaIntrospector::list_tables(&conn).unwrap();
        let names: Vec<_> = tables.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"authors"));
        assert!(names.contains(&"books"));
    }

    #[test]
    fn get_table_columns_and_fk() {
        let conn = sample_db();
        let books = SchemaIntrospector::get_table(&conn, "books").unwrap();
        assert_eq!(books.columns.len(), 3);
        assert!(!books.foreign_keys.is_empty());
        assert!(books.indexes.iter().any(|i| i.name == "idx_books_title"));
    }

    #[test]
    fn validate_column_for_table() {
        let conn = sample_db();
        assert!(SchemaIntrospector::validate_column_for_table(&conn, "books", "title").unwrap());
        assert!(!SchemaIntrospector::validate_column_for_table(&conn, "books", "nope").unwrap());
    }

    #[test]
    fn reject_bad_table_name() {
        let conn = sample_db();
        assert!(SchemaIntrospector::get_table(&conn, "books;DROP").is_err());
    }

    #[test]
    fn parse_order_column_spec_plain_and_suffix() {
        assert_eq!(
            parse_order_column_spec("id").unwrap(),
            ("id".to_string(), false)
        );
        assert_eq!(
            parse_order_column_spec(" title ").unwrap(),
            ("title".to_string(), false)
        );
        assert_eq!(
            parse_order_column_spec("created_at.DESC").unwrap(),
            ("created_at".to_string(), true)
        );
        assert_eq!(
            parse_order_column_spec("name.asc").unwrap(),
            ("name".to_string(), false)
        );
    }

    #[test]
    fn parse_order_column_spec_rejects_empty_and_invalid() {
        assert!(parse_order_column_spec("").is_err());
        assert!(parse_order_column_spec("   ").is_err());
        assert!(parse_order_column_spec("id, 1=1").is_err());
        assert!(parse_order_column_spec("col;drop").is_err());
        assert!(parse_order_column_spec("bad.dropped").is_err());
    }
}
