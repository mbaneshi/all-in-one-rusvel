//! [`MemoryStore`] — `SQLite` + FTS5 implementation of [`MemoryPort`].

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rusqlite::Connection;
use uuid::Uuid;

use rusvel_core::domain::{MemoryEntry, MemoryKind};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::id::SessionId;
use rusvel_core::ports::MemoryPort;

/// A session-namespaced memory store backed by `SQLite` with FTS5 full-text search.
///
/// Thread-safe via interior `Mutex<Connection>`.
pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl MemoryStore {
    /// Open an in-memory `SQLite` database (useful for tests).
    pub fn in_memory() -> std::result::Result<Self, RusvelError> {
        let conn = Connection::open_in_memory().map_err(|e| RusvelError::Storage(e.to_string()))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    /// Open (or create) a `SQLite` database at the given path.
    pub fn open(path: &str) -> std::result::Result<Self, RusvelError> {
        let conn = Connection::open(path).map_err(|e| RusvelError::Storage(e.to_string()))?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    /// Wrap an existing `rusqlite::Connection`.
    ///
    /// The caller is responsible for ensuring the connection is not shared
    /// elsewhere while the `MemoryStore` holds it.
    pub fn from_connection(conn: Connection) -> std::result::Result<Self, RusvelError> {
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.init_tables()?;
        Ok(store)
    }

    /// Create the main table and FTS5 virtual table if they do not exist.
    fn init_tables(&self) -> std::result::Result<(), RusvelError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RusvelError::Internal(e.to_string()))?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memory_entries (
                id          TEXT PRIMARY KEY,
                session_id  TEXT NOT NULL,
                kind        TEXT NOT NULL,
                content     TEXT NOT NULL,
                embedding   BLOB,
                created_at  TEXT NOT NULL,
                metadata    TEXT NOT NULL DEFAULT '{}'
            );

            CREATE INDEX IF NOT EXISTS idx_memory_session
                ON memory_entries(session_id);

            CREATE VIRTUAL TABLE IF NOT EXISTS memory_fts USING fts5(
                content,
                content='memory_entries',
                content_rowid='rowid'
            );

            -- Triggers to keep FTS5 in sync with the main table.
            CREATE TRIGGER IF NOT EXISTS memory_fts_insert
            AFTER INSERT ON memory_entries BEGIN
                INSERT INTO memory_fts(rowid, content)
                VALUES (new.rowid, new.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_fts_delete
            AFTER DELETE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content)
                VALUES ('delete', old.rowid, old.content);
            END;

            CREATE TRIGGER IF NOT EXISTS memory_fts_update
            AFTER UPDATE ON memory_entries BEGIN
                INSERT INTO memory_fts(memory_fts, rowid, content)
                VALUES ('delete', old.rowid, old.content);
                INSERT INTO memory_fts(rowid, content)
                VALUES (new.rowid, new.content);
            END;
            ",
        )
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Serialize a [`MemoryKind`] to a string for storage.
    fn kind_to_string(kind: &MemoryKind) -> String {
        match kind {
            MemoryKind::Fact => "Fact".to_string(),
            MemoryKind::Conversation => "Conversation".to_string(),
            MemoryKind::Decision => "Decision".to_string(),
            MemoryKind::Preference => "Preference".to_string(),
            MemoryKind::Custom(s) => format!("Custom:{s}"),
        }
    }

    /// Deserialize a [`MemoryKind`] from the stored string.
    fn string_to_kind(s: &str) -> MemoryKind {
        match s {
            "Fact" => MemoryKind::Fact,
            "Conversation" => MemoryKind::Conversation,
            "Decision" => MemoryKind::Decision,
            "Preference" => MemoryKind::Preference,
            other => {
                if let Some(custom) = other.strip_prefix("Custom:") {
                    MemoryKind::Custom(custom.to_string())
                } else {
                    MemoryKind::Custom(other.to_string())
                }
            }
        }
    }

    /// Parse a row from the `memory_entries` table into a [`MemoryEntry`].
    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
        let id_str: String = row.get(0)?;
        let session_id_str: String = row.get(1)?;
        let kind_str: String = row.get(2)?;
        let content: String = row.get(3)?;
        let embedding_blob: Option<Vec<u8>> = row.get(4)?;
        let created_at_str: String = row.get(5)?;
        let metadata_str: String = row.get(6)?;

        let id = Uuid::parse_str(&id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })?;

        let session_uuid = Uuid::parse_str(&session_id_str).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?;

        let created_at: DateTime<Utc> = created_at_str.parse::<DateTime<Utc>>().map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
        })?;

        let metadata: serde_json::Value =
            serde_json::from_str(&metadata_str).unwrap_or_else(|_| serde_json::json!({}));

        let embedding = embedding_blob.map(|blob| {
            blob.as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        });

        Ok(MemoryEntry {
            id: Some(id),
            session_id: SessionId::from_uuid(session_uuid),
            kind: Self::string_to_kind(&kind_str),
            content,
            embedding,
            created_at,
            metadata,
        })
    }

    /// Encode an embedding vector as a byte blob for `SQLite` storage.
    fn embedding_to_blob(embedding: &[f32]) -> Vec<u8> {
        embedding.iter().flat_map(|f| f.to_le_bytes()).collect()
    }
}

#[async_trait]
impl MemoryPort for MemoryStore {
    async fn store(&self, entry: MemoryEntry) -> Result<Uuid> {
        let id = entry.id.unwrap_or_else(Uuid::now_v7);
        let session_id = entry.session_id.as_uuid().to_string();
        let kind = Self::kind_to_string(&entry.kind);
        let content = entry.content.clone();
        let created_at = entry.created_at.to_rfc3339();
        let metadata = serde_json::to_string(&entry.metadata)
            .map_err(|e| RusvelError::Serialization(e.to_string()))?;
        let embedding_blob = entry.embedding.as_deref().map(Self::embedding_to_blob);

        let conn = self
            .conn
            .lock()
            .map_err(|e| RusvelError::Internal(e.to_string()))?;
        conn.execute(
            "INSERT OR REPLACE INTO memory_entries (id, session_id, kind, content, embedding, created_at, metadata)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                id.to_string(),
                session_id,
                kind,
                content,
                embedding_blob,
                created_at,
                metadata,
            ],
        )
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

        Ok(id)
    }

    async fn recall(&self, id: &Uuid) -> Result<Option<MemoryEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RusvelError::Internal(e.to_string()))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, session_id, kind, content, embedding, created_at, metadata
                 FROM memory_entries WHERE id = ?1",
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let result = stmt
            .query_row(rusqlite::params![id.to_string()], Self::row_to_entry)
            .optional()
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        Ok(result)
    }

    async fn search(
        &self,
        session_id: &SessionId,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RusvelError::Internal(e.to_string()))?;

        // Use FTS5 MATCH to find relevant entries, filtered by session_id.
        let mut stmt = conn
            .prepare(
                "SELECT e.id, e.session_id, e.kind, e.content, e.embedding, e.created_at, e.metadata
                 FROM memory_entries e
                 JOIN memory_fts f ON e.rowid = f.rowid
                 WHERE f.memory_fts MATCH ?1
                   AND e.session_id = ?2
                 ORDER BY f.rank
                 LIMIT ?3",
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let rows = stmt
            .query_map(
                rusqlite::params![query, session_id.as_uuid().to_string(), limit as i64],
                Self::row_to_entry,
            )
            .map_err(|e| RusvelError::Storage(e.to_string()))?;

        let mut entries = Vec::new();
        for row in rows {
            entries.push(row.map_err(|e| RusvelError::Storage(e.to_string()))?);
        }

        Ok(entries)
    }

    async fn forget(&self, id: &Uuid) -> Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| RusvelError::Internal(e.to_string()))?;

        // The trigger on DELETE will remove the FTS5 entry automatically.
        conn.execute(
            "DELETE FROM memory_entries WHERE id = ?1",
            rusqlite::params![id.to_string()],
        )
        .map_err(|e| RusvelError::Storage(e.to_string()))?;

        Ok(())
    }
}

/// Extension trait for `rusqlite::OptionalExtension`.
trait OptionalExt<T> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for std::result::Result<T, rusqlite::Error> {
    fn optional(self) -> std::result::Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rusvel_core::domain::MemoryKind;
    use rusvel_core::id::SessionId;

    fn make_entry(session_id: SessionId, content: &str, kind: MemoryKind) -> MemoryEntry {
        MemoryEntry {
            id: None,
            session_id,
            kind,
            content: content.to_string(),
            embedding: None,
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        }
    }

    #[tokio::test]
    async fn store_and_recall() {
        let store = MemoryStore::in_memory().unwrap();
        let session = SessionId::new();
        let entry = make_entry(session, "Rust is a systems language", MemoryKind::Fact);

        let id = store.store(entry.clone()).await.unwrap();

        let recalled = store.recall(&id).await.unwrap();
        assert!(recalled.is_some());
        let recalled = recalled.unwrap();
        assert_eq!(recalled.id, Some(id));
        assert_eq!(recalled.session_id, session);
        assert_eq!(recalled.content, "Rust is a systems language");
        assert_eq!(recalled.kind, MemoryKind::Fact);
    }

    #[tokio::test]
    async fn recall_nonexistent_returns_none() {
        let store = MemoryStore::in_memory().unwrap();
        let id = Uuid::now_v7();
        let result = store.recall(&id).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn search_returns_matching_entries() {
        let store = MemoryStore::in_memory().unwrap();
        let session = SessionId::new();

        store
            .store(make_entry(
                session,
                "The quick brown fox jumps",
                MemoryKind::Fact,
            ))
            .await
            .unwrap();
        store
            .store(make_entry(
                session,
                "Lazy dog sleeps all day",
                MemoryKind::Conversation,
            ))
            .await
            .unwrap();
        store
            .store(make_entry(
                session,
                "Fox hunting is controversial",
                MemoryKind::Decision,
            ))
            .await
            .unwrap();

        let results = store.search(&session, "fox", 10).await.unwrap();
        assert_eq!(results.len(), 2);
        for entry in &results {
            assert!(entry.content.to_lowercase().contains("fox"));
        }
    }

    #[tokio::test]
    async fn search_is_session_namespaced() {
        let store = MemoryStore::in_memory().unwrap();
        let session_a = SessionId::new();
        let session_b = SessionId::new();

        store
            .store(make_entry(
                session_a,
                "Secret plan for session A",
                MemoryKind::Fact,
            ))
            .await
            .unwrap();
        store
            .store(make_entry(
                session_b,
                "Secret plan for session B",
                MemoryKind::Fact,
            ))
            .await
            .unwrap();

        let results_a = store.search(&session_a, "secret", 10).await.unwrap();
        assert_eq!(results_a.len(), 1);
        assert_eq!(results_a[0].session_id, session_a);

        let results_b = store.search(&session_b, "secret", 10).await.unwrap();
        assert_eq!(results_b.len(), 1);
        assert_eq!(results_b[0].session_id, session_b);
    }

    #[tokio::test]
    async fn forget_removes_entry_and_fts() {
        let store = MemoryStore::in_memory().unwrap();
        let session = SessionId::new();

        let id = store
            .store(make_entry(
                session,
                "Remember this important fact",
                MemoryKind::Fact,
            ))
            .await
            .unwrap();

        // Verify it exists.
        assert!(store.recall(&id).await.unwrap().is_some());

        // Forget it.
        store.forget(&id).await.unwrap();

        // Verify it is gone from main table.
        assert!(store.recall(&id).await.unwrap().is_none());

        // Verify it is gone from FTS index.
        let search_results = store.search(&session, "important", 10).await.unwrap();
        assert!(search_results.is_empty());
    }

    #[tokio::test]
    async fn search_respects_limit() {
        let store = MemoryStore::in_memory().unwrap();
        let session = SessionId::new();

        for i in 0..5 {
            store
                .store(make_entry(
                    session,
                    &format!("Entry number {i} about testing"),
                    MemoryKind::Fact,
                ))
                .await
                .unwrap();
        }

        let results = store.search(&session, "testing", 3).await.unwrap();
        assert_eq!(results.len(), 3);
    }

    #[tokio::test]
    async fn custom_memory_kind_roundtrips() {
        let store = MemoryStore::in_memory().unwrap();
        let session = SessionId::new();

        let id = store
            .store(make_entry(
                session,
                "Custom kind data",
                MemoryKind::Custom("MyKind".to_string()),
            ))
            .await
            .unwrap();

        let recalled = store.recall(&id).await.unwrap().unwrap();
        assert_eq!(recalled.kind, MemoryKind::Custom("MyKind".to_string()));
    }

    #[tokio::test]
    async fn embedding_roundtrips() {
        let store = MemoryStore::in_memory().unwrap();
        let session = SessionId::new();

        let mut entry = make_entry(session, "With embeddings", MemoryKind::Fact);
        entry.embedding = Some(vec![0.1, 0.2, 0.3, 0.4]);

        let id = store.store(entry).await.unwrap();

        let recalled = store.recall(&id).await.unwrap().unwrap();
        let emb = recalled.embedding.unwrap();
        assert_eq!(emb.len(), 4);
        assert!((emb[0] - 0.1).abs() < 1e-6);
        assert!((emb[3] - 0.4).abs() < 1e-6);
    }
}
