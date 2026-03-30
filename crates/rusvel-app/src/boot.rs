//! Boot sequence for the Department-as-App architecture (ADR-014).
//!
//! Implements `installed_departments()` + `boot_departments()`:
//!
//! 1. Read all manifests (no side effects, fast)
//! 2. Validate IDs and dependencies
//! 3. Register departments in dependency order
//! 4. Finalize [`RegistrationContext`] → registry + tools + event subs + job handlers
//!
//! The composition root (`main.rs`) creates ports, then calls
//! `boot_departments()` which registers all departments. The host wires
//! event subscriptions to [`rusvel_event::EventBus`] and job handlers to
//! the queue worker.

use std::sync::Arc;

use rusvel_core::DepartmentApp;
use rusvel_core::department::{
    DepartmentsBootArtifacts, EventSubscription, RegistrationContext, resolve_dependency_order,
    validate_unique_ids,
};
use rusvel_core::ports::*;
use rusvel_event::EventBus;

/// Subscribe to live [`EventBus`] broadcasts and invoke matching department handlers.
pub fn spawn_department_event_dispatch(
    event_bus: Arc<EventBus>,
    subscriptions: Vec<EventSubscription>,
) {
    if subscriptions.is_empty() {
        return;
    }
    tracing::info!(
        count = subscriptions.len(),
        "Department event dispatch: bridging EventBus → department handlers"
    );
    tokio::spawn(async move {
        let mut rx = event_bus.subscribe();
        loop {
            match rx.recv().await {
                Ok(event) => {
                    for sub in &subscriptions {
                        if sub.event_kind == event.kind {
                            let h = sub.handler.clone();
                            let kind = sub.event_kind.clone();
                            let ev = event.clone();
                            tokio::spawn(async move {
                                if let Err(e) = h(ev).await {
                                    tracing::warn!(
                                        error = %e,
                                        kind = %kind,
                                        "department event handler failed"
                                    );
                                }
                            });
                        }
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });
}

/// Ordered list of installed departments.
///
/// Like Django's `INSTALLED_APPS` — order matters for dependencies.
/// Departments with no `depends_on` are listed first.
pub fn installed_departments() -> Vec<Box<dyn DepartmentApp>> {
    vec![
        // Core departments (no deps on other depts)
        Box::new(dept_forge::ForgeDepartment::new()),
        Box::new(dept_code::CodeDepartment::new()),
        // Departments with cross-dept event subscriptions
        Box::new(dept_content::ContentDepartment::new()),
        Box::new(dept_harvest::HarvestDepartment::new()),
        Box::new(dept_flow::FlowDepartment::new()),
        // Departments with minimal logic (progressive enhancement)
        Box::new(dept_gtm::GtmDepartment::new()),
        Box::new(dept_finance::FinanceDepartment::new()),
        Box::new(dept_product::ProductDepartment::new()),
        Box::new(dept_growth::GrowthDepartment::new()),
        Box::new(dept_distro::DistroDepartment::new()),
        Box::new(dept_legal::LegalDepartment::new()),
        Box::new(dept_support::SupportDepartment::new()),
        Box::new(dept_infra::InfraDepartment::new()),
        // Outbound channels — registered after core departments (see dept-messaging crate).
        Box::new(dept_messaging::MessagingDepartment::new()),
    ]
}

/// Boot all departments: validate manifests, register in dependency order,
/// return the generated registry.
///
/// # Arguments
///
/// * `departments` — from [`installed_departments()`]
/// * Ports — shared infrastructure created by the composition root
#[allow(clippy::too_many_arguments)]
pub async fn boot_departments(
    departments: &[Box<dyn DepartmentApp>],
    agent: Arc<dyn AgentPort>,
    events: Arc<dyn EventPort>,
    storage: Arc<dyn StoragePort>,
    jobs: Arc<dyn JobPort>,
    memory: Arc<dyn MemoryPort>,
    sessions: Arc<dyn SessionPort>,
    config: Arc<dyn ConfigPort>,
    auth: Arc<dyn AuthPort>,
    embedding: Option<Arc<dyn EmbeddingPort>>,
    vector_store: Option<Arc<dyn VectorStorePort>>,
) -> anyhow::Result<DepartmentsBootArtifacts> {
    // Phase 1: Read all manifests (no side effects)
    let manifests: Vec<_> = departments.iter().map(|d| d.manifest()).collect();

    // Phase 2: Validate
    validate_unique_ids(&manifests)?;
    let order = resolve_dependency_order(&manifests)?;

    tracing::info!(
        "Department boot: {} departments, dependency order: {}",
        manifests.len(),
        order
            .iter()
            .map(|&i| manifests[i].id.as_str())
            .collect::<Vec<_>>()
            .join(" → ")
    );

    // Phase 3: Register in dependency order
    let mut ctx = RegistrationContext::new(
        agent,
        events,
        storage,
        jobs,
        memory,
        sessions,
        config,
        auth,
        embedding,
        vector_store,
    );

    let mut failed = Vec::new();
    for &idx in &order {
        let dept = &departments[idx];
        let manifest = dept.manifest();
        ctx.add_manifest(manifest.clone());

        if let Err(e) = dept.register(&mut ctx).await {
            tracing::error!("Failed to register department '{}': {e}", manifest.id);
            failed.push((manifest.id.clone(), e.to_string()));
        }
    }

    let tools_n = ctx.tools.len();
    let ev_n = ctx.event_handlers.len();
    let job_n = ctx.job_handlers.len();

    let artifacts = ctx.finalize(failed);
    tracing::info!(
        "Department boot complete: {} departments registered, {} tools, {} event handlers, {} job handlers",
        artifacts.registry.departments.len(),
        tools_n,
        ev_n,
        job_n,
    );

    Ok(artifacts)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};
    use std::sync::Arc;

    use async_trait::async_trait;
    use rusvel_agent::AgentRuntime;
    use rusvel_auth::InMemoryAuthAdapter;
    use rusvel_config::TomlConfig;
    use rusvel_core::domain::{
        Content, FinishReason, LlmRequest, LlmResponse, LlmUsage, ModelRef, Session,
        SessionSummary, ToolDefinition, ToolResult,
    };
    use rusvel_core::error::Result;
    use rusvel_core::id::SessionId;
    use rusvel_core::ports::{
        AuthPort, ConfigPort, EventPort, JobPort, LlmPort, MemoryPort, SessionPort, StoragePort,
        ToolPort,
    };
    use rusvel_db::Database;
    use rusvel_event::EventBus;
    use rusvel_memory::MemoryStore;
    use serde_json::json;

    use super::*;

    struct SessionAdapter(pub Arc<dyn StoragePort>);

    #[async_trait]
    impl SessionPort for SessionAdapter {
        async fn create(
            &self,
            session: rusvel_core::domain::Session,
        ) -> rusvel_core::error::Result<SessionId> {
            let id = session.id;
            self.0.sessions().put_session(&session).await?;
            Ok(id)
        }
        async fn load(&self, id: &SessionId) -> rusvel_core::error::Result<Session> {
            self.0.sessions().get_session(id).await?.ok_or_else(|| {
                rusvel_core::error::RusvelError::NotFound {
                    kind: "session".into(),
                    id: id.to_string(),
                }
            })
        }
        async fn save(&self, session: &Session) -> rusvel_core::error::Result<()> {
            self.0.sessions().put_session(session).await
        }
        async fn list(&self) -> rusvel_core::error::Result<Vec<SessionSummary>> {
            self.0.sessions().list_sessions().await
        }
    }

    struct StubLlm;

    #[async_trait]
    impl LlmPort for StubLlm {
        async fn generate(&self, _: LlmRequest) -> Result<LlmResponse> {
            Ok(LlmResponse {
                content: Content::text("stub"),
                finish_reason: FinishReason::Stop,
                usage: LlmUsage::default(),
                metadata: json!({}),
            })
        }
        async fn embed(&self, _: &ModelRef, _: &str) -> Result<Vec<f32>> {
            Ok(vec![])
        }
        async fn list_models(&self) -> Result<Vec<ModelRef>> {
            Ok(vec![])
        }
    }

    struct StubTool;

    #[async_trait]
    impl ToolPort for StubTool {
        async fn register(&self, _: ToolDefinition) -> Result<()> {
            Ok(())
        }
        async fn call(&self, _: &str, _: serde_json::Value) -> Result<ToolResult> {
            Ok(ToolResult {
                success: true,
                output: Content::text("ok"),
                metadata: json!({}),
            })
        }
        fn list(&self) -> Vec<ToolDefinition> {
            vec![]
        }
        fn search(&self, _: &str, _: usize) -> Vec<ToolDefinition> {
            vec![]
        }
        fn schema(&self, _: &str) -> Option<serde_json::Value> {
            None
        }
    }

    #[tokio::test]
    async fn boot_registers_many_department_tools_without_name_collisions() {
        let base = std::env::temp_dir().join(format!("rusvel-boot-test-{}", uuid::Uuid::now_v7()));
        std::fs::create_dir_all(&base).expect("temp dir");
        let db_path = base.join("rusvel.db");
        let db: Arc<Database> = Arc::new(Database::open(&db_path).expect("db"));
        let storage: Arc<dyn StoragePort> = db.clone();
        let config: Arc<dyn ConfigPort> =
            Arc::new(TomlConfig::load(base.join("config.toml")).expect("config"));
        let events: Arc<dyn EventPort> = Arc::new(EventBus::new(
            db.clone() as Arc<dyn rusvel_core::ports::EventStore>
        ));
        let memory: Arc<dyn MemoryPort> =
            Arc::new(MemoryStore::open(base.join("memory.db").to_str().unwrap()).expect("memory"));
        let jobs: Arc<dyn JobPort> = db.clone() as Arc<dyn JobPort>;
        let sessions: Arc<dyn SessionPort> = Arc::new(SessionAdapter(storage.clone()));
        let tools: Arc<dyn ToolPort> = Arc::new(StubTool);
        let agent_runtime = Arc::new(AgentRuntime::new(
            Arc::new(StubLlm),
            tools.clone(),
            memory.clone(),
        ));
        let auth: Arc<dyn AuthPort> = Arc::new(InMemoryAuthAdapter::new());

        let departments = installed_departments();
        let artifacts = boot_departments(
            &departments,
            agent_runtime.clone(),
            events,
            storage,
            jobs,
            memory,
            sessions,
            config,
            auth,
            None,
            None,
        )
        .await
        .expect("boot");

        let n = artifacts.tools.len();
        assert!(
            n >= 45,
            "expected at least 45 department-registered tools (Sprint 2 target), got {n}"
        );

        let mut seen = HashSet::new();
        for t in &artifacts.tools {
            assert!(
                seen.insert(t.name.clone()),
                "duplicate tool name: {}",
                t.name
            );
        }

        let mut by_dept: HashMap<String, usize> = HashMap::new();
        for t in &artifacts.tools {
            *by_dept.entry(t.department_id.clone()).or_insert(0) += 1;
        }
        for (dept, c) in &by_dept {
            assert!(
                *c >= 2,
                "department {dept} should have at least 2 tools, got {c}"
            );
        }
    }
}
