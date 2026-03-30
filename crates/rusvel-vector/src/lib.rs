//! # rusvel-vector
//!
//! LanceDB-backed `VectorStorePort` adapter for RUSVEL.
//!
//! Stores knowledge entries with vector embeddings for semantic search.
//! Data is persisted at a configurable path (e.g. `~/.rusvel/knowledge.lance`).

use std::sync::Arc;

use arrow_array::{Array, Float32Array, RecordBatch, RecordBatchIterator, StringArray};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use chrono::Utc;
use lancedb::query::{ExecutableQuery, QueryBase};

use rusvel_core::domain::{VectorEntry, VectorSearchResult};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::VectorStorePort;

// ════════════════════════════════════════════════════════════════════
//  LanceDB VectorStore adapter
// ════════════════════════════════════════════════════════════════════

const TABLE_NAME: &str = "knowledge";

/// LanceDB-backed vector store for knowledge entries.
pub struct LanceVectorStore {
    db: lancedb::Connection,
    dimensions: usize,
}

impl LanceVectorStore {
    /// Open (or create) a `LanceDB` database at the given path.
    ///
    /// `dimensions` must match the embedding model output (e.g. 384, 768, 1536).
    pub async fn open(path: &str, dimensions: usize) -> Result<Self> {
        let db = lancedb::connect(path)
            .execute()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB connect: {e}")))?;

        let store = Self { db, dimensions };
        store.ensure_table().await?;
        Ok(store)
    }

    /// Create the knowledge table if it does not exist.
    async fn ensure_table(&self) -> Result<()> {
        let names = self
            .db
            .table_names()
            .execute()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB table_names: {e}")))?;

        if !names.contains(&TABLE_NAME.to_string()) {
            let schema = self.table_schema();
            let batch = RecordBatch::new_empty(Arc::new(schema.clone()));
            let batches = RecordBatchIterator::new(vec![Ok(batch)], Arc::new(schema));
            self.db
                .create_table(TABLE_NAME, Box::new(batches))
                .execute()
                .await
                .map_err(|e| RusvelError::Internal(format!("LanceDB create_table: {e}")))?;
            tracing::info!(
                "Created LanceDB table '{TABLE_NAME}' with {dim}d vectors",
                dim = self.dimensions
            );
        }
        Ok(())
    }

    /// Arrow schema for the knowledge table.
    fn table_schema(&self) -> Schema {
        Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("content", DataType::Utf8, false),
            Field::new("source", DataType::Utf8, false),
            Field::new(
                "embedding",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    i32::try_from(self.dimensions).expect("embedding dimensions fit i32"),
                ),
                false,
            ),
            Field::new("metadata", DataType::Utf8, true),
            Field::new("created_at", DataType::Utf8, false),
        ])
    }

    /// Open the table handle.
    async fn open_table(&self) -> Result<lancedb::Table> {
        self.db
            .open_table(TABLE_NAME)
            .execute()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB open_table: {e}")))
    }

    /// Build a `FixedSizeList` array from a slice of embeddings.
    fn build_embedding_array(&self, embeddings: &[Vec<f32>]) -> arrow_array::ArrayRef {
        let flat: Vec<f32> = embeddings.iter().flat_map(|e| e.iter().copied()).collect();
        let values = Float32Array::from(flat);
        let field = Arc::new(Field::new("item", DataType::Float32, true));
        let list = arrow_array::FixedSizeListArray::try_new(
            field,
            i32::try_from(self.dimensions).expect("embedding dimensions fit i32"),
            Arc::new(values),
            None,
        )
        .expect("valid FixedSizeListArray");
        Arc::new(list)
    }

    /// Parse record batches into `VectorEntry` values.
    fn batches_to_entries(batches: &[RecordBatch]) -> Vec<VectorEntry> {
        let mut entries = Vec::new();
        for batch in batches {
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let contents = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let sources = batch
                .column_by_name("source")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let metadatas = batch
                .column_by_name("metadata")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let created_ats = batch
                .column_by_name("created_at")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());

            let (Some(ids), Some(contents), Some(sources)) = (ids, contents, sources) else {
                continue;
            };

            for i in 0..batch.num_rows() {
                let metadata: serde_json::Value = metadatas
                    .and_then(|m| {
                        if m.is_null(i) {
                            None
                        } else {
                            serde_json::from_str(m.value(i)).ok()
                        }
                    })
                    .unwrap_or(serde_json::json!({}));

                let created_at = created_ats
                    .map(|c| c.value(i).to_string())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

                entries.push(VectorEntry {
                    id: ids.value(i).to_string(),
                    content: contents.value(i).to_string(),
                    source: sources.value(i).to_string(),
                    metadata,
                    created_at,
                });
            }
        }
        entries
    }
}

#[async_trait]
impl VectorStorePort for LanceVectorStore {
    async fn upsert(
        &self,
        id: &str,
        content: &str,
        embedding: Vec<f32>,
        metadata: serde_json::Value,
    ) -> Result<()> {
        // Delete existing entry with this id (for "upsert" semantics)
        let _ = self.delete(id).await;

        let table = self.open_table().await?;
        let schema = Arc::new(self.table_schema());

        let now = Utc::now().to_rfc3339();
        let source = metadata
            .get("source")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let ids = StringArray::from(vec![id]);
        let contents = StringArray::from(vec![content]);
        let sources = StringArray::from(vec![source.as_str()]);
        let embedding_array = self.build_embedding_array(&[embedding]);
        let metadata_str = serde_json::to_string(&metadata).unwrap_or_default();
        let metadatas = StringArray::from(vec![metadata_str.as_str()]);
        let created_ats = StringArray::from(vec![now.as_str()]);

        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(ids),
                Arc::new(contents),
                Arc::new(sources),
                embedding_array,
                Arc::new(metadatas),
                Arc::new(created_ats),
            ],
        )
        .map_err(|e| RusvelError::Internal(format!("RecordBatch: {e}")))?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        table
            .add(Box::new(batches))
            .execute()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB add: {e}")))?;

        Ok(())
    }

    async fn search(
        &self,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<VectorSearchResult>> {
        let table = self.open_table().await?;

        let results = table
            .vector_search(query_embedding)
            .map_err(|e| RusvelError::Internal(format!("LanceDB vector_search setup: {e}")))?
            .limit(limit)
            .execute()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB search execute: {e}")))?;

        use futures::TryStreamExt;
        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB collect: {e}")))?;

        let mut search_results = Vec::new();
        for batch in &batches {
            let ids = batch
                .column_by_name("id")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let contents = batch
                .column_by_name("content")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let sources = batch
                .column_by_name("source")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let metadatas = batch
                .column_by_name("metadata")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let created_ats = batch
                .column_by_name("created_at")
                .and_then(|c| c.as_any().downcast_ref::<StringArray>());
            let distances = batch
                .column_by_name("_distance")
                .and_then(|c| c.as_any().downcast_ref::<Float32Array>());

            let (Some(ids), Some(contents), Some(sources)) = (ids, contents, sources) else {
                continue;
            };

            for i in 0..batch.num_rows() {
                let metadata: serde_json::Value = metadatas
                    .and_then(|m| {
                        if m.is_null(i) {
                            None
                        } else {
                            serde_json::from_str(m.value(i)).ok()
                        }
                    })
                    .unwrap_or(serde_json::json!({}));

                let created_at = created_ats
                    .map(|c| c.value(i).to_string())
                    .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                    .map_or_else(Utc::now, |dt| dt.with_timezone(&Utc));

                let distance = distances.map_or(0.0, |d| f64::from(d.value(i)));
                // Convert distance to similarity score (1 / (1 + distance))
                let score = 1.0 / (1.0 + distance);

                search_results.push(VectorSearchResult {
                    entry: VectorEntry {
                        id: ids.value(i).to_string(),
                        content: contents.value(i).to_string(),
                        source: sources.value(i).to_string(),
                        metadata,
                        created_at,
                    },
                    score,
                    metadata: Default::default(),
                });
            }
        }

        Ok(search_results)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let table = self.open_table().await?;
        table
            .delete(&format!("id = '{id}'"))
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB delete: {e}")))?;
        Ok(())
    }

    async fn list(&self, limit: usize) -> Result<Vec<VectorEntry>> {
        let table = self.open_table().await?;

        let results = table
            .query()
            .limit(limit)
            .execute()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB query: {e}")))?;

        use futures::TryStreamExt;
        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB collect: {e}")))?;

        Ok(Self::batches_to_entries(&batches))
    }

    async fn count(&self) -> Result<usize> {
        let table = self.open_table().await?;
        let count = table
            .count_rows(None)
            .await
            .map_err(|e| RusvelError::Internal(format!("LanceDB count: {e}")))?;
        Ok(count)
    }
}
