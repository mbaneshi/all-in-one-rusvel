//! Registration context and subsystem registrars.
//!
//! Passed to [`DepartmentApp::register()`](super::DepartmentApp::register).
//! Each registrar collects contributions from departments during boot,
//! then the host builds surfaces (API, CLI, tools, events, jobs) from
//! the accumulated registrations.

use std::collections::HashMap;
use std::sync::Arc;

use super::manifest::DepartmentManifest;
use crate::ports::*;
use crate::registry::{DepartmentDef, DepartmentRegistry, QuickAction as RegQuickAction};

// ════════════════════════════════════════════════════════════════════
//  RegistrationContext — passed to DepartmentApp::register()
// ════════════════════════════════════════════════════════════════════

/// Context provided to departments during registration.
///
/// Contains the ports the department can use and registrars for
/// each host subsystem. The department calls registrar methods to
/// contribute routes, tools, event handlers, and job handlers.
pub struct RegistrationContext {
    // ── Ports (what the department consumes) ──────────────────────
    pub agent: Arc<dyn AgentPort>,
    pub events: Arc<dyn EventPort>,
    pub storage: Arc<dyn StoragePort>,
    pub jobs: Arc<dyn JobPort>,
    pub memory: Arc<dyn MemoryPort>,
    pub sessions: Arc<dyn SessionPort>,
    pub config: Arc<dyn ConfigPort>,
    pub auth: Arc<dyn AuthPort>,
    pub embedding: Option<Arc<dyn EmbeddingPort>>,
    pub vector_store: Option<Arc<dyn VectorStorePort>>,

    // ── Registrars ───────────────────────────────────────────────
    pub tools: ToolRegistrar,
    pub event_handlers: EventHandlerRegistrar,
    pub job_handlers: JobHandlerRegistrar,

    // ── Internal: manifest collection ────────────────────────────
    manifests: Vec<DepartmentManifest>,
}

impl RegistrationContext {
    /// Create a new context with the given ports. Registrars start empty.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
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
    ) -> Self {
        Self {
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
            tools: ToolRegistrar::new(),
            event_handlers: EventHandlerRegistrar::new(),
            job_handlers: JobHandlerRegistrar::new(),
            manifests: Vec::new(),
        }
    }

    /// Record a manifest (called by the boot sequence, not by departments).
    pub fn add_manifest(&mut self, manifest: DepartmentManifest) {
        self.manifests.push(manifest);
    }

    /// Generate a [`DepartmentRegistry`] from all collected manifests.
    /// Used to serve `GET /api/departments` and render the frontend.
    pub fn build_registry(&self) -> DepartmentRegistry {
        DepartmentRegistry::from_manifests(&self.manifests)
    }

    /// Consume the context after registration and return the registry plus
    /// all subsystem contributions (tools, event subscriptions, job handlers).
    pub fn finalize(self, failed_departments: Vec<(String, String)>) -> DepartmentsBootArtifacts {
        let registry = DepartmentRegistry::from_manifests(&self.manifests);
        let tools = self.tools.into_tools();
        let event_subscriptions = self.event_handlers.into_subscriptions();
        let job_handlers = self.job_handlers.into_handlers();
        DepartmentsBootArtifacts {
            registry,
            tools,
            event_subscriptions,
            job_handlers,
            failed_departments,
        }
    }
}

/// Output of [`RegistrationContext::finalize`]: registry and collected registrations.
pub struct DepartmentsBootArtifacts {
    pub registry: DepartmentRegistry,
    pub tools: Vec<ToolRegistration>,
    pub event_subscriptions: Vec<EventSubscription>,
    pub job_handlers: HashMap<String, JobHandlerEntry>,
    pub failed_departments: Vec<(String, String)>,
}

// ════════════════════════════════════════════════════════════════════
//  ToolRegistrar — collects tool definitions from departments
// ════════════════════════════════════════════════════════════════════

/// Collects tool definitions during department registration.
/// After boot, the host transfers these to the `ToolPort` implementation.
pub struct ToolRegistrar {
    tools: Vec<ToolRegistration>,
}

/// A tool definition plus its async handler.
pub struct ToolRegistration {
    pub name: String,
    pub description: String,
    pub parameters_schema: serde_json::Value,
    pub department_id: String,
    pub handler: ToolHandler,
}

/// Async tool handler function.
pub type ToolHandler = Arc<
    dyn Fn(
            serde_json::Value,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = crate::error::Result<ToolOutput>> + Send>,
        > + Send
        + Sync,
>;

/// Output from a tool execution.
#[derive(Debug, Clone)]
pub struct ToolOutput {
    pub content: String,
    pub is_error: bool,
    pub metadata: serde_json::Value,
}

impl ToolRegistrar {
    pub fn new() -> Self {
        Self { tools: Vec::new() }
    }

    /// Register a tool with its handler.
    pub fn add(
        &mut self,
        department_id: &str,
        name: impl Into<String>,
        description: impl Into<String>,
        parameters_schema: serde_json::Value,
        handler: ToolHandler,
    ) {
        self.tools.push(ToolRegistration {
            name: name.into(),
            description: description.into(),
            parameters_schema,
            department_id: department_id.into(),
            handler,
        });
    }

    /// Consume the registrar and return all collected tools.
    pub fn into_tools(self) -> Vec<ToolRegistration> {
        self.tools
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

impl Default for ToolRegistrar {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  EventHandlerRegistrar — collects event subscriptions
// ════════════════════════════════════════════════════════════════════

/// Async handler for incoming events.
pub type EventHandlerFn = Arc<
    dyn Fn(
            crate::domain::Event,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = crate::error::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// A single event subscription.
pub struct EventSubscription {
    pub event_kind: String,
    pub department_id: String,
    pub handler: EventHandlerFn,
}

/// Collects event subscriptions from departments.
pub struct EventHandlerRegistrar {
    subscriptions: Vec<EventSubscription>,
}

impl EventHandlerRegistrar {
    pub fn new() -> Self {
        Self {
            subscriptions: Vec::new(),
        }
    }

    /// Subscribe to events matching a kind.
    pub fn on(
        &mut self,
        department_id: &str,
        event_kind: impl Into<String>,
        handler: EventHandlerFn,
    ) {
        self.subscriptions.push(EventSubscription {
            event_kind: event_kind.into(),
            department_id: department_id.into(),
            handler,
        });
    }

    /// Consume and return all subscriptions.
    pub fn into_subscriptions(self) -> Vec<EventSubscription> {
        self.subscriptions
    }

    pub fn len(&self) -> usize {
        self.subscriptions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.subscriptions.is_empty()
    }
}

impl Default for EventHandlerRegistrar {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  JobHandlerRegistrar — collects job type handlers
// ════════════════════════════════════════════════════════════════════

/// Async handler for a specific job kind.
pub type JobHandlerFn = Arc<
    dyn Fn(
            crate::domain::Job,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = crate::error::Result<serde_json::Value>> + Send>,
        > + Send
        + Sync,
>;

/// A single job handler registration.
pub struct JobHandlerEntry {
    pub kind: String,
    pub department_id: String,
    pub handler: JobHandlerFn,
}

/// Collects job handlers from departments.
pub struct JobHandlerRegistrar {
    handlers: HashMap<String, JobHandlerEntry>,
}

impl JobHandlerRegistrar {
    pub fn new() -> Self {
        Self {
            handlers: HashMap::new(),
        }
    }

    /// Register a handler for a job kind.
    /// Later registrations for the same kind overwrite earlier ones.
    pub fn handle(&mut self, department_id: &str, kind: impl Into<String>, handler: JobHandlerFn) {
        let kind = kind.into();
        self.handlers.insert(
            kind.clone(),
            JobHandlerEntry {
                kind,
                department_id: department_id.into(),
                handler,
            },
        );
    }

    /// Consume and return all handlers.
    pub fn into_handlers(self) -> HashMap<String, JobHandlerEntry> {
        self.handlers
    }

    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }
}

impl Default for JobHandlerRegistrar {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  DepartmentRegistry generation from manifests
// ════════════════════════════════════════════════════════════════════

impl DepartmentRegistry {
    /// Build a [`DepartmentRegistry`] from a list of manifests.
    ///
    /// This bridges ADR-014 (manifest-based) with ADR-011 (registry-based)
    /// for backward compatibility with existing frontend and API code.
    pub fn from_manifests(manifests: &[DepartmentManifest]) -> Self {
        let departments = manifests
            .iter()
            .map(|m| DepartmentDef {
                id: m.id.clone(),
                name: m.name.clone(),
                title: m.name.clone(),
                icon: m.icon.clone(),
                color: m.color.clone(),
                system_prompt: m.system_prompt.clone(),
                capabilities: m.capabilities.clone(),
                tabs: m.ui.tabs.clone(),
                quick_actions: m
                    .quick_actions
                    .iter()
                    .map(|qa| RegQuickAction {
                        label: qa.label.clone(),
                        prompt: qa.prompt.clone(),
                    })
                    .collect(),
                default_config: m.default_config.clone(),
                terminal: m.terminal.clone(),
            })
            .collect();

        Self { departments }
    }
}

// ════════════════════════════════════════════════════════════════════
//  Boot helpers
// ════════════════════════════════════════════════════════════════════

/// Validate that all department IDs are unique.
pub fn validate_unique_ids(manifests: &[DepartmentManifest]) -> crate::error::Result<()> {
    let mut seen = std::collections::HashSet::new();
    for m in manifests {
        if !seen.insert(&m.id) {
            return Err(crate::error::RusvelError::Config(format!(
                "Duplicate department ID: '{}'",
                m.id
            )));
        }
    }
    Ok(())
}

/// Resolve registration order via topological sort on `depends_on`.
///
/// Returns indices into the original slice, ordered so that dependencies
/// come before dependents. Detects cycles.
pub fn resolve_dependency_order(
    manifests: &[DepartmentManifest],
) -> crate::error::Result<Vec<usize>> {
    let id_to_idx: HashMap<&str, usize> = manifests
        .iter()
        .enumerate()
        .map(|(i, m)| (m.id.as_str(), i))
        .collect();

    let n = manifests.len();
    let mut in_degree = vec![0usize; n];
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

    for (i, m) in manifests.iter().enumerate() {
        for dep in &m.depends_on {
            if let Some(&dep_idx) = id_to_idx.get(dep.as_str()) {
                adj[dep_idx].push(i);
                in_degree[i] += 1;
            }
            // Soft dependency on unknown dept is OK — skip silently
        }
    }

    // Kahn's algorithm
    let mut queue: std::collections::VecDeque<usize> = in_degree
        .iter()
        .enumerate()
        .filter(|&(_, d)| *d == 0)
        .map(|(i, _)| i)
        .collect();

    let mut order = Vec::with_capacity(n);
    while let Some(idx) = queue.pop_front() {
        order.push(idx);
        for &next in &adj[idx] {
            in_degree[next] -= 1;
            if in_degree[next] == 0 {
                queue.push_back(next);
            }
        }
    }

    if order.len() != n {
        return Err(crate::error::RusvelError::Config(
            "Circular dependency detected among departments".into(),
        ));
    }

    Ok(order)
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest(id: &str, deps: Vec<&str>) -> DepartmentManifest {
        let mut m = DepartmentManifest::new(id, id);
        m.depends_on = deps.into_iter().map(String::from).collect();
        m
    }

    #[test]
    fn unique_ids_passes_for_distinct() {
        let manifests = vec![
            DepartmentManifest::new("a", "A"),
            DepartmentManifest::new("b", "B"),
        ];
        assert!(validate_unique_ids(&manifests).is_ok());
    }

    #[test]
    fn unique_ids_fails_for_duplicates() {
        let manifests = vec![
            DepartmentManifest::new("a", "A"),
            DepartmentManifest::new("a", "A2"),
        ];
        assert!(validate_unique_ids(&manifests).is_err());
    }

    #[test]
    fn dependency_order_no_deps() {
        let manifests = vec![
            DepartmentManifest::new("a", "A"),
            DepartmentManifest::new("b", "B"),
            DepartmentManifest::new("c", "C"),
        ];
        let order = resolve_dependency_order(&manifests).unwrap();
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn dependency_order_linear_chain() {
        let manifests = vec![
            make_manifest("c", vec!["b"]),
            make_manifest("b", vec!["a"]),
            make_manifest("a", vec![]),
        ];
        let order = resolve_dependency_order(&manifests).unwrap();
        // a (idx 2) must come before b (idx 1) must come before c (idx 0)
        let pos_a = order.iter().position(|&x| x == 2).unwrap();
        let pos_b = order.iter().position(|&x| x == 1).unwrap();
        let pos_c = order.iter().position(|&x| x == 0).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn dependency_order_detects_cycle() {
        let manifests = vec![make_manifest("a", vec!["b"]), make_manifest("b", vec!["a"])];
        assert!(resolve_dependency_order(&manifests).is_err());
    }

    #[test]
    fn dependency_order_ignores_unknown_deps() {
        let manifests = vec![make_manifest("a", vec!["nonexistent"])];
        let order = resolve_dependency_order(&manifests).unwrap();
        assert_eq!(order, vec![0]);
    }

    #[test]
    fn tool_registrar_collects() {
        let mut reg = ToolRegistrar::new();
        let handler: ToolHandler = Arc::new(|_| {
            Box::pin(async {
                Ok(ToolOutput {
                    content: "ok".into(),
                    is_error: false,
                    metadata: serde_json::json!({}),
                })
            })
        });
        reg.add(
            "content",
            "content.draft",
            "Draft content",
            serde_json::json!({}),
            handler,
        );
        assert_eq!(reg.len(), 1);
        let tools = reg.into_tools();
        assert_eq!(tools[0].name, "content.draft");
        assert_eq!(tools[0].department_id, "content");
    }

    #[test]
    fn event_handler_registrar_collects() {
        let mut reg = EventHandlerRegistrar::new();
        let handler: EventHandlerFn = Arc::new(|_| Box::pin(async { Ok(()) }));
        reg.on("content", "code.analyzed", handler);
        assert_eq!(reg.len(), 1);
        let subs = reg.into_subscriptions();
        assert_eq!(subs[0].event_kind, "code.analyzed");
        assert_eq!(subs[0].department_id, "content");
    }

    #[test]
    fn job_handler_registrar_collects() {
        let mut reg = JobHandlerRegistrar::new();
        let handler: JobHandlerFn = Arc::new(|_| Box::pin(async { Ok(serde_json::json!({})) }));
        reg.handle("content", "content.publish", handler);
        assert_eq!(reg.len(), 1);
        let handlers = reg.into_handlers();
        assert!(handlers.contains_key("content.publish"));
    }

    #[test]
    fn from_manifests_builds_registry() {
        let manifests = vec![{
            let mut m = DepartmentManifest::new("forge", "Forge Department");
            m.icon = "=".into();
            m.color = "indigo".into();
            m.capabilities = vec!["planning".into()];
            m.ui.tabs = vec!["actions".into(), "agents".into()];
            m.quick_actions = vec![crate::department::QuickAction {
                label: "Plan".into(),
                prompt: "Make a plan".into(),
            }];
            m
        }];
        let registry = DepartmentRegistry::from_manifests(&manifests);
        assert_eq!(registry.departments.len(), 1);
        let dept = &registry.departments[0];
        assert_eq!(dept.id, "forge");
        assert_eq!(dept.icon, "=");
        assert_eq!(dept.tabs, vec!["actions", "agents"]);
        assert_eq!(dept.quick_actions.len(), 1);
    }
}
