//! Code intelligence engine — Rust-only parsing, symbol graph,
//! BM25 search, and basic code metrics.

use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use rusvel_core::domain::{Capability, CodeSnapshotRef, Event, HealthStatus, RepoRef};
use rusvel_core::engine::Engine;
use rusvel_core::id::{EventId, SnapshotId};
use rusvel_core::ports::{EventPort, StoragePort};

pub mod error;
pub mod graph;
pub mod metrics;
pub mod parser;
pub mod search;

pub mod events {
    pub const CODE_ANALYZED: &str = "code.analyzed";
}

/// The result of a full code analysis.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CodeAnalysis {
    pub snapshot: CodeSnapshotRef,
    pub symbols: Vec<parser::Symbol>,
    pub graph: graph::SymbolGraph,
    pub metrics: metrics::ProjectMetrics,
}

impl CodeAnalysis {
    /// Build a compact summary for downstream content / social generation.
    pub fn summary(&self) -> rusvel_core::domain::CodeAnalysisSummary {
        use crate::parser::SymbolKind;
        let top_symbols: Vec<String> = self
            .symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Function)
            .take(10)
            .map(|s| s.name.clone())
            .collect();
        rusvel_core::domain::CodeAnalysisSummary {
            snapshot_id: self.snapshot.id.to_string(),
            repo_path: self.snapshot.repo.local_path.display().to_string(),
            total_files: self.metrics.total_files,
            total_symbols: self.metrics.total_symbols,
            top_symbols,
            largest_function: self.metrics.largest_function.clone(),
            metadata: Default::default(),
        }
    }
}

impl From<&CodeAnalysis> for rusvel_core::domain::CodeAnalysisSummary {
    fn from(analysis: &CodeAnalysis) -> Self {
        analysis.summary()
    }
}

/// Code intelligence engine.
pub struct CodeEngine {
    storage: Arc<dyn StoragePort>,
    event_port: Arc<dyn EventPort>,
    index: std::sync::Mutex<Option<search::SearchIndex>>,
}

impl CodeEngine {
    pub fn new(storage: Arc<dyn StoragePort>, event_port: Arc<dyn EventPort>) -> Self {
        Self {
            storage,
            event_port,
            index: std::sync::Mutex::new(None),
        }
    }

    /// Analyze a repository: parse, build graph, compute metrics, index.
    pub async fn analyze(&self, repo_path: &Path) -> rusvel_core::error::Result<CodeAnalysis> {
        let symbols = parser::parse_directory(repo_path)?;

        // Collect file metrics for all unique files
        let mut seen = std::collections::HashSet::new();
        let mut file_metrics = Vec::new();
        for sym in &symbols {
            if seen.insert(sym.file_path.clone())
                && let Ok(fm) = metrics::count_lines(&sym.file_path)
            {
                file_metrics.push(fm);
            }
        }

        let project_metrics = metrics::compute_project_metrics(&symbols, &file_metrics);
        let sym_graph = graph::SymbolGraph::build(symbols.clone());

        // Build search index
        let items: Vec<(String, String, String, usize)> = symbols
            .iter()
            .map(|s| {
                (
                    s.name.clone(),
                    s.file_path.display().to_string(),
                    s.body.clone(),
                    s.line,
                )
            })
            .collect();
        let idx = search::SearchIndex::build(&items);
        *self.index.lock().unwrap() = Some(idx);

        let snapshot = CodeSnapshotRef {
            id: SnapshotId::new(),
            repo: RepoRef {
                local_path: repo_path.to_path_buf(),
                remote_url: None,
                metadata: Default::default(),
            },
            analyzed_at: Utc::now(),
            metadata: serde_json::Value::default(),
        };

        let analysis = CodeAnalysis {
            snapshot: snapshot.clone(),
            symbols,
            graph: sym_graph,
            metrics: project_metrics,
        };

        // Persist analysis to object store
        let json = serde_json::to_value(&analysis)?;
        self.storage
            .objects()
            .put("code_analysis", &snapshot.id.to_string(), json)
            .await?;

        // Emit event
        let event = Event {
            id: EventId::new(),
            session_id: None,
            run_id: None,
            source: "code".into(),
            kind: events::CODE_ANALYZED.into(),
            payload: serde_json::json!({
                "snapshot_id": snapshot.id.to_string(),
                "total_symbols": analysis.metrics.total_symbols,
                "total_files": analysis.metrics.total_files,
            }),
            created_at: Utc::now(),
            metadata: serde_json::json!({}),
        };
        let _ = self.event_port.emit(event).await;

        Ok(analysis)
    }

    /// Search previously indexed symbols.
    pub fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> rusvel_core::error::Result<Vec<search::SearchResult>> {
        let guard = self.index.lock().unwrap();
        match guard.as_ref() {
            Some(idx) => Ok(idx.search(query, limit)),
            None => Err(rusvel_core::error::RusvelError::Internal(
                "no analysis index available; call analyze() first".into(),
            )),
        }
    }
}

#[async_trait]
impl Engine for CodeEngine {
    fn kind(&self) -> &str {
        "code"
    }
    fn name(&self) -> &'static str {
        "Code Engine"
    }
    fn capabilities(&self) -> Vec<Capability> {
        vec![Capability::CodeAnalysis]
    }
    async fn initialize(&self) -> rusvel_core::error::Result<()> {
        Ok(())
    }
    async fn shutdown(&self) -> rusvel_core::error::Result<()> {
        Ok(())
    }
    async fn health(&self) -> rusvel_core::error::Result<HealthStatus> {
        let indexed = self.index.lock().unwrap().is_some();
        Ok(HealthStatus {
            healthy: true,
            message: None,
            metadata: serde_json::json!({ "indexed": indexed }),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::{Event, EventFilter};
    use rusvel_core::id::EventId;
    use rusvel_core::ports::*;
    use std::sync::Mutex;

    // ── Fake storage ──
    struct FakeObjectStore;
    #[async_trait]
    impl ObjectStore for FakeObjectStore {
        async fn put(
            &self,
            _: &str,
            _: &str,
            _: serde_json::Value,
        ) -> rusvel_core::error::Result<()> {
            Ok(())
        }
        async fn get(
            &self,
            _: &str,
            _: &str,
        ) -> rusvel_core::error::Result<Option<serde_json::Value>> {
            Ok(None)
        }
        async fn delete(&self, _: &str, _: &str) -> rusvel_core::error::Result<()> {
            Ok(())
        }
        async fn list(
            &self,
            _: &str,
            _: rusvel_core::domain::ObjectFilter,
        ) -> rusvel_core::error::Result<Vec<serde_json::Value>> {
            Ok(vec![])
        }
    }

    struct FakeStorage;
    impl StoragePort for FakeStorage {
        fn events(&self) -> &dyn EventStore {
            panic!("not used in tests")
        }
        fn objects(&self) -> &dyn ObjectStore {
            &FakeObjectStore
        }
        fn sessions(&self) -> &dyn SessionStore {
            panic!("not used in tests")
        }
        fn jobs(&self) -> &dyn JobStore {
            panic!("not used in tests")
        }
        fn metrics(&self) -> &dyn MetricStore {
            panic!("not used in tests")
        }
    }

    struct FakeEventPort(Mutex<Vec<Event>>);
    #[async_trait]
    impl EventPort for FakeEventPort {
        async fn emit(&self, event: Event) -> rusvel_core::error::Result<EventId> {
            let id = event.id;
            self.0.lock().unwrap().push(event);
            Ok(id)
        }
        async fn get(&self, _: &EventId) -> rusvel_core::error::Result<Option<Event>> {
            Ok(None)
        }
        async fn query(&self, _: EventFilter) -> rusvel_core::error::Result<Vec<Event>> {
            Ok(vec![])
        }
    }

    #[tokio::test]
    async fn analyze_and_search() {
        // Write a temp Rust file
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sample.rs");
        std::fs::write(&file, "pub fn hello() {}\nfn world() {}").unwrap();

        let events = Arc::new(FakeEventPort(Mutex::new(Vec::new())));
        let engine = CodeEngine::new(Arc::new(FakeStorage), events.clone());

        let analysis = engine.analyze(dir.path()).await.unwrap();
        assert_eq!(analysis.symbols.len(), 2);
        assert_eq!(analysis.metrics.total_symbols, 2);

        let results = engine.search("hello", 10).unwrap();
        assert!(!results.is_empty());
        assert_eq!(results[0].symbol_name, "hello");

        // Check event was emitted
        let emitted = events.0.lock().unwrap();
        assert_eq!(emitted.len(), 1);
        assert_eq!(emitted[0].kind, "code.analyzed");
    }

    #[tokio::test]
    async fn health_returns_healthy() {
        let engine = CodeEngine::new(
            Arc::new(FakeStorage),
            Arc::new(FakeEventPort(Mutex::new(Vec::new()))),
        );
        let status = engine.health().await.unwrap();
        assert!(status.healthy);
    }
}
