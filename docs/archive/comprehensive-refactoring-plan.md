# Comprehensive Refactoring Plan for RUSVEL

> Design pattern alignment, concept hierarchy, extensibility, and scalability.
> Applies Gang of Four patterns (Rust-idiomatic), DDD, SOLID, Clean Architecture,
> and modern system design patterns to a 55-crate hexagonal Rust + SvelteKit monorepo.

---

## Document control

| Field | Value |
|-------|--------|
| **Version** | 1.0 |
| **Date** | 2026-03-28 |
| **Architecture baseline** | [docs/design/architecture-v2.md](../design/architecture-v2.md) |
| **ADRs** | [docs/design/decisions.md](../design/decisions.md) (ADR-001 through ADR-014) |
| **Codebase** | 55 crates, ~69K lines Rust, 57 Svelte components |

**Scope:** Structural refactoring for pattern adherence, extensibility, and scalability — not feature work.

**Non-scope:** New features, new departments, UI redesign, performance optimization (see benchmarks plan).

---

## Table of contents

1. [Executive summary](#1-executive-summary)
2. [Current architecture assessment](#2-current-architecture-assessment)
3. [Concept hierarchy — the domain model spine](#3-concept-hierarchy--the-domain-model-spine)
4. [Gang of Four patterns — Rust-idiomatic application](#4-gang-of-four-patterns--rust-idiomatic-application)
5. [SOLID principle alignment](#5-solid-principle-alignment)
6. [Modern system design patterns](#6-modern-system-design-patterns)
7. [Sprint plan — execution order](#7-sprint-plan--execution-order)
8. [Sprint 1 — Foundation: Type safety and domain model](#8-sprint-1--foundation-type-safety-and-domain-model)
9. [Sprint 2 — Creational: Builders, factories, composition root](#9-sprint-2--creational-builders-factories-composition-root)
10. [Sprint 3 — Structural: Decorator, proxy, facade hardening](#10-sprint-3--structural-decorator-proxy-facade-hardening)
11. [Sprint 4 — Behavioral: Strategy, observer, command, chain](#11-sprint-4--behavioral-strategy-observer-command-chain)
12. [Sprint 5 — DDD: Aggregates, value objects, bounded contexts](#12-sprint-5--ddd-aggregates-value-objects-bounded-contexts)
13. [Sprint 6 — Event architecture: Saga, projections, replay](#13-sprint-6--event-architecture-saga-projections-replay)
14. [Sprint 7 — Plugin and capability architecture](#14-sprint-7--plugin-and-capability-architecture)
15. [Sprint 8 — Workflow engine hardening](#15-sprint-8--workflow-engine-hardening)
16. [Sprint 9 — Agent orchestration hierarchy](#16-sprint-9--agent-orchestration-hierarchy)
17. [Sprint 10 — Scalability and cross-cutting concerns](#17-sprint-10--scalability-and-cross-cutting-concerns)
18. [Sprint 11 — Frontend architecture alignment](#18-sprint-11--frontend-architecture-alignment)
19. [Sprint 12 — Self-improving system feedback loops](#19-sprint-12--self-improving-system-feedback-loops)
20. [Anti-patterns to avoid](#20-anti-patterns-to-avoid)
21. [Success metrics](#21-success-metrics)
22. [References](#22-references)

---

## 1. Executive summary

RUSVEL's hexagonal architecture is fundamentally sound. The port/adapter boundary, DepartmentApp plugin pattern (ADR-014), and composition root are correctly implemented. This plan does not propose replacing the architecture — it proposes **hardening** it with industry-standard design patterns to maximize extensibility, eliminate runtime errors that could be compile-time errors, and prepare the system for scale.

**Three strategic themes:**

1. **Type safety over stringly-typed** — Replace magic strings (event kinds, job kinds, tool names, department IDs) with compile-time-safe alternatives where possible, and validated registries where not.
2. **GoF patterns as Rust idioms** — Apply Builder (typestate), Strategy (trait generics), Observer (typed channels), Decorator (tower layers), Command (enum dispatch), and Visitor (AST traversal) where they eliminate boilerplate or catch bugs.
3. **Scalability substrate** — Add saga orchestration to flow-engine, association graphs to ObjectStore, reflection-based self-improvement loops, and Tower Service composability to the agent runtime.

**What is NOT changing:** The hexagonal port/adapter boundary, DepartmentApp trait, composition root pattern, single-binary constraint, SQLite WAL backend, or any ADR decisions.

---

## 2. Current architecture assessment

### Patterns already in use (verified)

| GoF Pattern | Where | Quality |
|-------------|-------|---------|
| **Strategy** | Port traits (LlmPort, AgentPort, StoragePort) | Excellent — runtime dispatch via `Arc<dyn Trait>` |
| **Abstract Factory** | Composition root (`rusvel-app/src/main.rs`) | Excellent — wires all adapters |
| **Facade** | `AgentRuntime` over LlmPort+ToolPort+MemoryPort (ADR-009) | Excellent |
| **Adapter** | Every `rusvel-*` adapter crate | Excellent — clean port/adapter boundary |
| **Proxy** | `ScopedToolRegistry` filters tools per department | Good |
| **Observer** | `AgentEvent` over mpsc; `EventPort` broadcast | Good |
| **Command** | `JobKind` enum dispatch in worker; `ToolHandler` closures | Good |
| **Bridge** | Port traits separate abstraction from implementation | Excellent |
| **Flyweight** | `Arc<dyn Port>` shared across engines | Good |
| **Builder** | `DepartmentManifest::new()`, `LlmRequest` construction | Partial — no typestate |
| **Template Method** | `LlmPort::stream()` default calls `generate()` | Good |
| **Singleton** | `Arc<T>` passed at construction (no static globals) | Excellent |

### Patterns missing or incomplete

| Pattern | Gap | Impact |
|---------|-----|--------|
| **Typestate Builder** | No compile-time enforcement of required fields | Runtime panics on misconfiguration |
| **Decorator (tower)** | `CostTrackingLlm` wraps manually, not as composable Layer | Can't stack middleware on LlmPort/AgentPort |
| **Chain of Responsibility** | Axum middleware is flat, no per-route middleware chain | All routes share same middleware |
| **Visitor** | `code-engine` symbol graph has no visitor trait | Adding new analysis = modifying traversal code |
| **State (typestate)** | Job/Flow status as runtime enum, not compile-time states | Invalid transitions possible at runtime |
| **Memento** | No checkpoint/snapshot pattern for flow-engine | Flows restart from zero on failure |
| **Mediator** | No cross-department coordination beyond events | ForgeEngine can't delegate to other dept agents |
| **Interpreter** | Flow condition nodes use ad-hoc evaluation | No formal expression language |

### Structural issues identified

| Issue | Location | Severity |
|-------|----------|----------|
| Monolithic `AppState` (~25 fields) | `rusvel-api/src/lib.rs` | High — hard to test, implicit coupling |
| Tool name collisions not validated | `rusvel-tool` ToolRegistrar | High — silent overwrite |
| Event kinds stringly-typed | All engines | Medium — typos undetected |
| Job results stored in metadata | `rusvel-jobs` | Medium — type-unsafe |
| No association graph between domain objects | `ObjectStore` | Medium — no Contact↔Opportunity links |
| Magic numbers scattered | Agent loop (10), compaction (30), broadcast (256) | Low — no central config |
| Worker polling has no shutdown signal | `rusvel-jobs` spawn_worker | Medium — can't gracefully stop |
| CORS origins hardcoded | `rusvel-api` | Low — not configurable |
| CLI department variants repeated 9 times | `rusvel-cli` | Low — DRY violation |

---

## 3. Concept hierarchy — the domain model spine

This section defines the **hierarchy of concepts** that all refactoring must preserve and strengthen. This is RUSVEL's ontology — the backbone that every pattern, every crate, and every API route maps onto.

### Level 0: Platform

```
RUSVEL (Platform)
├── Identity: single binary, single human, infinite leverage
├── Constraint: SQLite WAL, tokio async, Rust + SvelteKit
└── Invariant: every action traceable to a Session
```

### Level 1: Session → Workspace

```
Session
├── Owns: Runs, Threads, Goals, Events, Jobs, Config overrides
├── Scopes: All state is session-namespaced
├── Lifecycle: create → active → archived
└── Future: Session becomes Workspace when multi-user
```

### Level 2: Departments (Bounded Contexts)

```
Department (DepartmentApp)
├── Identity: string ID, manifest, icon, color
├── Owns: Engine, Tools, Skills, Rules, Hooks, Agents, Workflows
├── Communicates via: Events (pub/sub), Jobs (async work), ObjectStore (shared state)
├── Never: imports another department's crate
└── Types:
    ├── Wired (6): forge, code, harvest, content, gtm, flow
    ├── Skeleton (7): finance, product, growth, distro, legal, support, infra
    └── Shell (1): messaging (no engine crate yet)
```

### Level 3: Domain Entities (per department)

```
Forge: Goal → Task → Plan → Review → Persona → AgentProfile
Code: Repository → SymbolGraph → Symbol → Metric → SearchResult
Harvest: Opportunity → Proposal → Pipeline → Source → Score
Content: ContentItem → CalendarEntry → PlatformAdapter → PublishResult
GTM: Contact → Deal → OutreachSequence → Step → Invoice
Flow: Workflow → Node → Edge → Execution → Checkpoint → NodeResult
Finance: Ledger → Transaction → TaxEstimate → RunwayForecast
Product: Roadmap → Feature → PricingTier → FeedbackItem
Growth: Funnel → Cohort → KPI → Experiment
Distro: Listing → SEOProfile → AffiliateProgram → Partnership
Legal: Contract → ComplianceCheck → IPRecord → LicenseAgreement
Support: Ticket → KBArticle → NPSSurvey → AutoTriageRule
Infra: Deployment → Monitor → Incident → Pipeline
```

### Level 4: Cross-cutting Primitives

```
Event (immutable, append-only, kind=String)
Job (queue item: Queued → Running → Succeeded/Failed/AwaitingApproval)
Tool (registered capability: name, schema, handler, permission)
Skill (stored prompt template with {{input}} interpolation)
Rule (injected system prompt fragment per engine)
Hook (trigger: command/http/prompt on event match)
Agent (LLM + tools + persona + memory, streaming via AgentEvent)
Approval (human gate on Job or content publishing)
```

### Level 5: Infrastructure Ports

```
LlmPort → raw model access (generate, stream, embed)
AgentPort → orchestrated LLM (tool loop, memory, verification)
StoragePort → 5 sub-stores (events, objects, sessions, jobs, metrics)
EventPort → pub/sub + persistence
JobPort → central async work queue
ToolPort → tool registry + execution + permission
MemoryPort → session-scoped context + FTS5 search
ConfigPort → layered settings (global → dept → session)
AuthPort → opaque credential handles
EmbeddingPort → text → dense vectors
VectorStorePort → similarity search
ChannelPort → outbound notifications (Telegram, Discord)
TerminalPort → PTY multiplexer
BrowserPort → Chrome DevTools Protocol
DeployPort → deployment operations
SessionPort → session lifecycle management
```

### Design principle: Concept ownership

Every concept belongs to exactly one level. Cross-level references use IDs (newtypes), never owned structs. A Department never owns a Session. A Session never owns a Department. They reference each other via `SessionId` and department string ID.

**Refactoring rule:** If a struct holds an owned reference to a concept from a different level, it must be refactored to hold an ID reference instead.

---

## 4. Gang of Four patterns — Rust-idiomatic application

### 4.1 Creational patterns

#### Builder with Typestate (GoF: Builder)

**Problem:** `DepartmentManifest`, `LlmRequest`, `AgentConfig`, `FlowDefinition` are constructed with many fields. Missing required fields cause runtime panics or silent defaults.

**Solution:** Typestate builder pattern — encode required fields as generic type parameters. The `build()` method is only available when all required fields are set.

**Target types:**

```
[ ] DepartmentManifest — require id, name, system_prompt before build()
[ ] LlmRequest — require model and messages before build()
[ ] AgentConfig — require session_id and system_prompt before build()
[ ] FlowDefinition — require at least one node before build()
[ ] WebhookRegistration — require name, event_kind, and secret before build()
```

**Rust pattern:**

```rust
// Zero-sized state markers
pub struct NoId;
pub struct HasId(String);
pub struct NoPrompt;
pub struct HasPrompt(String);

pub struct ManifestBuilder<Id, Prompt> {
    id: Id,
    prompt: Prompt,
    name: String,
    // ... optional fields with defaults
}

impl ManifestBuilder<NoId, NoPrompt> {
    pub fn new(name: impl Into<String>) -> Self { /* defaults */ }
}

impl<Prompt> ManifestBuilder<NoId, Prompt> {
    pub fn id(self, id: impl Into<String>) -> ManifestBuilder<HasId, Prompt> { /* move fields */ }
}

impl<Id> ManifestBuilder<Id, NoPrompt> {
    pub fn system_prompt(self, p: impl Into<String>) -> ManifestBuilder<Id, HasPrompt> { /* move */ }
}

impl ManifestBuilder<HasId, HasPrompt> {
    pub fn build(self) -> DepartmentManifest { /* only callable when both set */ }
}
```

**Scope:** 5 types across `rusvel-core`, `rusvel-agent`, `flow-engine`, `rusvel-webhook`.

**References:** [Typestate pattern — Cliffle](https://cliffle.com/blog/rust-typestate/), [Builder with typestate — greyblake.com](https://www.greyblake.com/blog/builder-with-typestate-in-rust/)

---

#### Abstract Factory (GoF: Abstract Factory) — Composition Root Hardening

**Problem:** `rusvel-app/src/main.rs` constructs 25+ adapters with ad-hoc wiring. No structured validation that all required ports are provided.

**Solution:** Introduce a `PlatformFactory` trait that encapsulates adapter construction. The composition root becomes a concrete factory. Test harnesses become alternative factories.

```rust
pub trait PlatformFactory {
    fn create_storage(&self) -> Result<Arc<dyn StoragePort>>;
    fn create_llm(&self) -> Result<Arc<dyn LlmPort>>;
    fn create_agent(&self, llm: Arc<dyn LlmPort>, tools: Arc<dyn ToolPort>, memory: Arc<dyn MemoryPort>) -> Result<Arc<dyn AgentPort>>;
    fn create_event_bus(&self, store: Arc<dyn EventStore>) -> Result<Arc<dyn EventPort>>;
    // ... one method per required port
}

pub struct ProductionFactory { config: AppConfig }
pub struct TestFactory { /* in-memory everything */ }
```

**Checklist:**

```
[ ] Define PlatformFactory trait in rusvel-core (or a new rusvel-factory crate)
[ ] Implement ProductionFactory in rusvel-app
[ ] Implement TestFactory replacing rusvel-api/tests/common/mod.rs TestHarness ad-hoc construction
[ ] Validate: all required ports constructed before boot_departments() is called
```

---

#### Registry Pattern (GoF: not directly — combines Factory + Singleton)

**Problem:** Tool names, event kinds, job kinds, and department IDs are stringly-typed. Collisions are silent. Typos are undetected.

**Solution:** Validated registries with collision detection at registration time.

```
[ ] ToolRegistry: fail-fast on duplicate tool name registration
    - Change internal storage from allowing silent overwrite to returning Err on collision
    - Add deregister() for cleanup

[ ] EventKindRegistry (new): departments declare events_produced in manifest
    - At boot: collect all event kinds from manifests
    - At subscription: validate event kind exists in registry
    - Warn on unregistered event kind emission (log, don't fail — for extensibility)

[ ] JobKindRegistry (new): departments declare job kinds in manifest
    - At boot: collect all job kinds, validate no duplicate handlers
    - At dispatch: validate job kind has a registered handler

[ ] DepartmentIdRegistry: already exists via boot validation
    - Strengthen: validate IDs are valid URL slugs (lowercase, no spaces, no special chars)
```

---

### 4.2 Structural patterns

#### Decorator via Tower Layers (GoF: Decorator)

**Problem:** Cross-cutting concerns (cost tracking, rate limiting, retry, timeout, logging) are applied inconsistently. `CostTrackingLlm` wraps `LlmPort` manually. No composable middleware for ports.

**Solution:** Define port operations as `tower::Service` where middleware composition is needed. Wrap with `tower::Layer` for cross-cutting concerns.

**Target ports:**

```
[ ] LlmPort operations as tower::Service
    - CostTrackingLayer — wraps any LlmPort, accumulates cost
    - RateLimitLayer — per-provider rate limiting
    - RetryLayer — retry on transient errors with exponential backoff
    - TimeoutLayer — per-request timeout
    - LoggingLayer — structured tracing for LLM calls

    Composition:
    ServiceBuilder::new()
        .layer(LoggingLayer)
        .layer(TimeoutLayer::new(Duration::from_secs(30)))
        .layer(RetryLayer::new(3))
        .layer(RateLimitLayer::new(10, Duration::from_secs(1)))
        .layer(CostTrackingLayer::new(metrics.clone()))
        .service(OllamaProvider::new(url))

[ ] AgentPort as tower::Service<AgentRequest, Response = AgentStream>
    - ConcurrencyLimitLayer — max simultaneous agent runs
    - BackpressureLayer — bounded channel, reject when full
    - TimeoutLayer — max agent run duration

[ ] ToolPort::call() as tower::Service<ToolRequest, Response = ToolResult>
    - PermissionLayer — check tool permissions before dispatch
    - ValidationLayer — validate JSON Schema before handler
    - AuditLayer — log tool calls for observability
```

**Implementation approach:**

```rust
// Define a request/response pair for LLM
pub struct LlmCall {
    pub request: LlmRequest,
}

pub struct LlmCallResponse {
    pub response: LlmResponse,
}

// Implement tower::Service for each LlmPort provider
impl tower::Service<LlmCall> for OllamaProvider {
    type Response = LlmCallResponse;
    type Error = RusvelError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: LlmCall) -> Self::Future {
        // ... actual LLM call
    }
}
```

**References:** [Inventing the Service trait — Tokio blog](https://tokio.rs/blog/2021-05-14-inventing-the-service-trait), [Tower docs](https://docs.rs/tower/latest/tower/trait.Service.html)

---

#### Facade Decomposition (GoF: Facade) — AppState Breakup

**Problem:** `AppState` in `rusvel-api` holds ~25 fields. Every handler receives the entire state. This creates implicit coupling — a chat handler has access to the deployment port.

**Solution:** Split `AppState` into focused sub-states. Each handler group receives only the sub-state it needs.

```
[ ] CoreState — storage, events, sessions, config, auth (used by all handlers)
[ ] AgentState — agent_runtime, tools, memory, profile (used by chat/agent handlers)
[ ] EngineState — forge, code_engine, content_engine, harvest_engine, gtm_engine, flow_engine
[ ] InfraState — terminal, cdp, deploy, channel, webhook_receiver, cron_scheduler
[ ] BootState — registry, boot_time, failed_departments (read-only after boot)
```

```rust
// Before: every handler gets everything
async fn handle_chat(State(state): State<Arc<AppState>>) -> impl IntoResponse { ... }

// After: handlers declare their dependencies
async fn handle_chat(
    State(core): State<Arc<CoreState>>,
    State(agent): State<Arc<AgentState>>,
) -> impl IntoResponse { ... }
```

**Implementation:**

```
[ ] Define sub-state structs in rusvel-api
[ ] Use Axum's FromRef pattern to extract sub-states from a parent AppState
[ ] Migrate handlers one module at a time (start with chat, then departments, then system)
[ ] Remove fields from handlers that don't need them
```

---

#### Composite Pattern (GoF: Composite) — Flow Node Hierarchy

**Problem:** Flow engine nodes (code, condition, agent) are flat. No way to nest sub-flows, group nodes, or compose workflows.

**Solution:** Make `FlowNode` a composite — a node can contain a sub-flow (DAG of nodes).

```rust
pub enum FlowNode {
    Code(CodeNode),
    Condition(ConditionNode),
    Agent(AgentNode),
    BrowserTrigger(BrowserTriggerNode),
    BrowserAction(BrowserActionNode),
    SubFlow(SubFlowNode),       // NEW: contains a nested FlowDefinition
    Parallel(ParallelNode),      // NEW: fan-out to N nodes, fan-in results
    Loop(LoopNode),              // NEW: repeat until condition met
}

pub struct SubFlowNode {
    pub flow_id: FlowId,        // Reference to another flow definition
    pub input_mapping: HashMap<String, String>,
    pub output_mapping: HashMap<String, String>,
}
```

**Checklist:**

```
[ ] Add SubFlowNode, ParallelNode, LoopNode variants to FlowNode enum
[ ] Implement recursive execution in flow-engine executor
[ ] Add cycle detection for SubFlow references (prevent infinite recursion)
[ ] Add depth limit (e.g., max 5 levels of nesting)
[ ] Update flow-engine/src/lib.rs NodeRegistry to handle new types
[ ] Update API: GET /api/flows/node-types to include new variants
[ ] Update frontend WorkflowBuilder to render nested flows
```

---

### 4.3 Behavioral patterns

#### Strategy with Static Dispatch (GoF: Strategy)

**Problem:** All strategy selection is via `Arc<dyn Trait>` (dynamic dispatch). For hot paths where the strategy set is closed and known, this adds unnecessary vtable overhead.

**Solution:** Use generic type parameters for closed strategy sets; keep `dyn Trait` for open sets.

**Closed sets (use generics):**

```
[ ] ModelTier routing — only 3 tiers (Fast/Balanced/Premium)
    - Replace runtime match with generic: fn route<T: TierStrategy>(tier: T, request: LlmRequest)

[ ] Content platform adapters — known set (LinkedIn, Twitter, DEV.to)
    - Replace Vec<Box<dyn PlatformAdapter>> with enum dispatch for known platforms
    - Keep Box<dyn PlatformAdapter> for user-added platforms via MCP

[ ] Flow condition evaluation — known evaluators
    - Replace dynamic dispatch with enum: ConditionEvaluator { JsonPath, Expression, LlmJudge }
```

**Open sets (keep dyn Trait):**

```
LlmPort — new providers can be added
DepartmentApp — new departments can be added
ToolHandler — user-defined tools
EventHandler — department-defined handlers
```

**Principle:** If the set of strategies is fixed and known at compile time, use enum dispatch. If the set is open (user-extensible), use trait objects.

---

#### Observer with Typed Channels (GoF: Observer)

**Problem:** `EventPort` broadcasts untyped `Event` structs. Subscribers must pattern-match on `event.kind` string. No compile-time guarantee that a subscriber handles the right event type.

**Solution:** Add a typed event channel layer on top of the existing string-based EventPort.

```rust
// Typed event wrapper (zero-cost at runtime — just a newtype)
pub trait TypedEvent: Send + Sync + 'static {
    const KIND: &'static str;
    fn from_event(event: &Event) -> Result<Self> where Self: Sized;
    fn into_event(self, session_id: SessionId) -> Event;
}

// Example: typed event for code analysis completion
pub struct CodeAnalyzed {
    pub session_id: SessionId,
    pub path: PathBuf,
    pub symbols_found: usize,
}

impl TypedEvent for CodeAnalyzed {
    const KIND: &'static str = "code.analyzed";
    fn from_event(event: &Event) -> Result<Self> { serde_json::from_value(event.data.clone()) }
    fn into_event(self, session_id: SessionId) -> Event { /* construct */ }
}

// Typed subscription
pub trait TypedEventBus {
    fn subscribe<E: TypedEvent>(&self) -> broadcast::Receiver<E>;
    fn emit_typed<E: TypedEvent>(&self, event: E, session_id: SessionId) -> Result<EventId>;
}
```

**Checklist:**

```
[ ] Define TypedEvent trait in rusvel-core
[ ] Implement TypedEventBus as a layer on top of EventPort
[ ] Define typed events for the 6 wired engines (code.analyzed, content.drafted, harvest.scored, etc.)
[ ] Migrate engine event subscriptions from string matching to typed subscriptions
[ ] Keep string-based EventPort as the underlying transport (backward compatible)
[ ] Event kind constants as associated consts on TypedEvent (compile-time safe)
```

---

#### Command Pattern Enhancement (GoF: Command)

**Problem:** `JobKind` uses `Custom(String)` for extensibility but loses type safety. Job results are stored in untyped `metadata: serde_json::Value`.

**Solution:** Typed job commands with associated input/output types.

```rust
pub trait JobCommand: Send + Sync + 'static {
    const KIND: &'static str;
    type Input: Serialize + DeserializeOwned;
    type Output: Serialize + DeserializeOwned;

    fn kind(&self) -> &str { Self::KIND }
    fn input(&self) -> &Self::Input;
}

// Example
pub struct ContentPublishCommand {
    pub content_id: ContentId,
    pub platform: Platform,
}

impl JobCommand for ContentPublishCommand {
    const KIND: &'static str = "content.publish";
    type Input = ContentPublishInput;
    type Output = PublishResult;
    // ...
}
```

**Checklist:**

```
[ ] Define JobCommand trait in rusvel-core
[ ] Implement for existing job kinds: CodeAnalyze, ContentPublish, HarvestScan, OutreachSend, Custom
[ ] Type-safe enqueue: job_port.enqueue_typed(command) serializes Input to metadata
[ ] Type-safe complete: job_port.complete_typed::<C>(job_id, output) serializes Output
[ ] Migrate job worker dispatch to typed commands
[ ] Keep JobKind::Custom(String) for backward compatibility with untyped jobs
```

---

#### Chain of Responsibility (GoF: Chain) — Middleware Pipeline

**Problem:** API middleware is global. No per-route or per-department middleware chains. Auth, rate limiting, and validation are either on or off for all routes.

**Solution:** Use Axum's nested router pattern with per-group middleware.

```
[ ] Department routes: auth + rate limit + department context injection
[ ] System routes: auth + admin check
[ ] Health routes: no auth, no rate limit
[ ] Chat/SSE routes: auth + SSE-specific timeout + backpressure
[ ] Webhook routes: HMAC verification middleware (not bearer token)
```

```rust
let dept_routes = Router::new()
    .route("/api/dept/:id/chat", post(chat_handler))
    .route("/api/dept/:id/goals", get(list_goals).post(create_goal))
    .layer(DepartmentContextLayer::new())
    .layer(RateLimitLayer::new(100));

let webhook_routes = Router::new()
    .route("/api/webhooks/:id/receive", post(receive_webhook))
    .layer(HmacVerificationLayer::new());

let app = Router::new()
    .merge(dept_routes)
    .merge(webhook_routes)
    .route("/api/health", get(health_handler))  // no middleware
    .layer(TracingLayer::new());
```

---

#### Visitor Pattern (GoF: Visitor) — Code Engine AST

**Problem:** `code-engine` symbol graph traversal is hardcoded. Adding a new analysis pass (e.g., complexity metrics, dependency mapping) requires modifying the traversal code.

**Solution:** Define a `SymbolVisitor` trait. Traversal walks the graph and calls visitor methods. New analyses implement the visitor trait.

```rust
pub trait SymbolVisitor {
    fn visit_module(&mut self, module: &Module) -> Result<()> { Ok(()) }
    fn visit_function(&mut self, func: &Function) -> Result<()> { Ok(()) }
    fn visit_struct(&mut self, s: &Struct) -> Result<()> { Ok(()) }
    fn visit_trait(&mut self, t: &Trait) -> Result<()> { Ok(()) }
    fn visit_impl(&mut self, i: &Impl) -> Result<()> { Ok(()) }
    fn visit_import(&mut self, import: &Import) -> Result<()> { Ok(()) }
}

// Traversal (unchanged when new visitors are added)
pub fn walk_symbol_graph<V: SymbolVisitor>(graph: &SymbolGraph, visitor: &mut V) -> Result<()> {
    for module in &graph.modules {
        visitor.visit_module(module)?;
        for func in &module.functions {
            visitor.visit_function(func)?;
        }
        // ... etc
    }
    Ok(())
}

// New analysis = new visitor (Open/Closed principle)
pub struct ComplexityVisitor { pub total_complexity: usize }
impl SymbolVisitor for ComplexityVisitor { /* count branches per function */ }

pub struct DependencyVisitor { pub edges: Vec<(String, String)> }
impl SymbolVisitor for DependencyVisitor { /* track import relationships */ }
```

**Checklist:**

```
[ ] Define SymbolVisitor trait in code-engine
[ ] Implement walk_symbol_graph() traversal function
[ ] Refactor existing metric calculation as MetricsVisitor
[ ] Refactor existing search indexing as SearchIndexVisitor
[ ] Add ComplexityVisitor for cyclomatic complexity
[ ] Add DependencyVisitor for import graph extraction
```

**Reference:** [Visitor pattern — rust-unofficial patterns](https://rust-unofficial.github.io/patterns/patterns/behavioural/visitor.html)

---

#### State Pattern with Typestate (GoF: State)

**Problem:** `JobStatus` transitions are unchecked at compile time. Code can call `complete()` on a job that's already completed. `FlowExecution` status transitions are similarly unchecked.

**Solution:** Typestate for finite state machines where invalid transitions should be impossible.

```rust
// Job state machine (compile-time safe)
pub struct Job<S: JobState> {
    pub id: JobId,
    pub kind: JobKind,
    pub session_id: SessionId,
    _state: PhantomData<S>,
    data: JobData,  // shared internal data
}

pub struct Queued;
pub struct Running;
pub struct AwaitingApproval;
pub struct Completed;
pub struct Failed;

// Only Queued jobs can be started
impl Job<Queued> {
    pub fn start(self) -> Job<Running> { /* transition */ }
    pub fn cancel(self) -> Job<Failed> { /* transition */ }
}

// Only Running jobs can complete or fail
impl Job<Running> {
    pub fn complete(self, result: serde_json::Value) -> Job<Completed> { /* transition */ }
    pub fn fail(self, error: String) -> Job<Failed> { /* transition */ }
    pub fn hold_for_approval(self) -> Job<AwaitingApproval> { /* transition */ }
}

// Only AwaitingApproval jobs can be approved
impl Job<AwaitingApproval> {
    pub fn approve(self) -> Job<Queued> { /* back to queue */ }
    pub fn reject(self) -> Job<Failed> { /* transition */ }
}
```

**Scope:** Apply to `Job` and `FlowExecution` state machines. Do NOT apply to domain entities with >5 states or dynamic state sets.

**When NOT to use:** If the state machine must be serialized/deserialized across process boundaries (the typestate is erased by serde). In that case, use a runtime-checked `Status` enum with a `transition()` method that validates transitions.

**Practical approach for RUSVEL:** Use typestate internally within a single function's scope (the job worker loop). Use runtime-checked `JobStatus` enum for persistence and API responses.

```
[ ] Define typestate Job<S> for internal job worker logic
[ ] Keep JobStatus enum for persistence layer (serde-compatible)
[ ] Add Job::transition(from: JobStatus, to: JobStatus) -> Result<()> with explicit validation
[ ] Define allowed transitions as a static map
[ ] Return descriptive error on invalid transition
```

---

#### Mediator Pattern (GoF: Mediator) — Cross-Department Agent Delegation

**Problem:** ForgeEngine is the "meta-engine" but cannot delegate tasks to other department agents. Cross-department coordination requires manual event chains.

**Solution:** A `DelegateAgent` tool that ForgeEngine can use to invoke another department's agent with a scoped prompt.

```rust
pub struct DelegateAgentTool {
    agent_port: Arc<dyn AgentPort>,
    tool_registry: Arc<dyn ToolPort>,
    department_registry: Arc<DepartmentRegistry>,
}

impl DelegateAgentTool {
    pub async fn execute(&self, args: DelegateArgs) -> Result<ToolOutput> {
        // 1. Validate target department exists
        let dept = self.department_registry.get(&args.department_id)?;

        // 2. Create scoped tool set for target department
        let scoped_tools = ScopedToolRegistry::new(
            self.tool_registry.clone(),
            &dept.manifest().tools,
        );

        // 3. Run agent with department's system prompt + delegated task
        let config = AgentConfig::builder()
            .session_id(args.session_id)
            .system_prompt(format!("{}\n\nDelegated task: {}", dept.manifest().system_prompt, args.task))
            .build();

        let result = self.agent_port.run(config, scoped_tools).await?;
        Ok(ToolOutput::from(result))
    }
}
```

**Checklist:**

```
[ ] Define DelegateAgentTool in rusvel-builtin-tools or rusvel-engine-tools
[ ] Register as a forge-only tool (not available to other departments)
[ ] Add depth limit (max 3 delegation levels to prevent infinite recursion)
[ ] Emit event: forge.agent.delegated with target department and task
[ ] Add delegation tracking in AgentEvent stream (show delegation in chat UI)
```

---

## 5. SOLID principle alignment

### Current compliance and gaps

| Principle | Status | Gap | Refactoring |
|-----------|--------|-----|-------------|
| **S** — Single Responsibility | Good | `AppState` holds 25+ concerns. `rusvel-cli` repeats dept variants 9x | Sprint 3: AppState breakup. Sprint 10: CLI data-driven dispatch |
| **O** — Open/Closed | Excellent | DepartmentApp is perfectly OCP. Tool registry is mostly OCP | Sprint 4: SymbolVisitor for code-engine. Sprint 7: WASM plugin boundary |
| **L** — Liskov Substitution | Good | Port trait contracts are implicit (documented, not enforced) | Sprint 4: Engine contract tests (from testing plan). Sprint 1: Port trait documentation |
| **I** — Interface Segregation | Excellent | StoragePort → 5 sub-stores (ADR-004). Ports are focused | Sprint 3: AppState sub-states extend ISP to API layer |
| **D** — Dependency Inversion | Excellent | Engines → traits only (ADR-010). Composition root wires concretes | Sprint 2: PlatformFactory formalizes the pattern |

### Specific ISP improvements

```
[ ] Split EventPort into EventEmitter + EventQuerier
    - Engines that only emit don't need query methods
    - EventEmitter: emit()
    - EventQuerier: get(), query()
    - EventPort: extends both (for backward compatibility)

[ ] Split ToolPort into ToolRegistry + ToolExecutor
    - Registration (boot-time) vs execution (runtime) are separate concerns
    - ToolRegistry: register(), deregister(), list()
    - ToolExecutor: call(), search()
    - ToolPort: extends both

[ ] Split AgentPort into AgentRunner + AgentStreamRunner
    - Sync run() vs streaming run_streaming() are different use cases
    - Some callers (job worker) only need sync
    - Some callers (chat handler) only need streaming
```

---

## 6. Modern system design patterns

### 6.1 Domain-Driven Design refinements

**Current state:** RUSVEL has implicit DDD. Domain types are in `rusvel-core`, engines are use-case interactors, events are domain events. But aggregates are not explicit, and there's no aggregate root enforcement.

**Refactoring:**

```
[ ] Define Aggregate Root marker trait
    pub trait AggregateRoot {
        type Id: Clone + Eq + Hash;
        fn id(&self) -> &Self::Id;
    }

[ ] Mark aggregate roots:
    - Session is aggregate root for Runs and Threads
    - Opportunity is aggregate root for Proposals
    - ContentItem is aggregate root for PublishResults
    - Contact is aggregate root for OutreachSteps
    - Workflow is aggregate root for Nodes and Edges
    - Goal is aggregate root for Tasks

[ ] Add ObjectStore association edges (Sprint 5)
    pub trait ObjectStore {
        // Existing CRUD...
        async fn relate(&self, from_kind: &str, from_id: &str, to_kind: &str, to_id: &str, relation: &str) -> Result<()>;
        async fn related(&self, kind: &str, id: &str, relation: &str) -> Result<Vec<serde_json::Value>>;
        async fn unrelate(&self, from_kind: &str, from_id: &str, to_kind: &str, to_id: &str, relation: &str) -> Result<()>;
    }

    Implementation: SQLite junction table
    CREATE TABLE object_relations (
        from_kind TEXT NOT NULL,
        from_id TEXT NOT NULL,
        to_kind TEXT NOT NULL,
        to_id TEXT NOT NULL,
        relation TEXT NOT NULL,
        created_at TEXT NOT NULL DEFAULT (datetime('now')),
        PRIMARY KEY (from_kind, from_id, to_kind, to_id, relation)
    );

    Usage:
    store.objects().relate("contact", contact_id, "opportunity", opp_id, "sourced_from").await?;
    let opps = store.objects().related("contact", contact_id, "sourced_from").await?;
```

### 6.2 Event architecture — Saga pattern

**Problem:** Multi-step business flows (harvest → proposal → outreach → invoice) have no compensation logic. A failure mid-flow leaves orphaned state.

**Solution:** Model multi-step flows as sagas in flow-engine with compensation edges.

```rust
pub struct SagaStep {
    pub action_node_id: NodeId,
    pub compensation_node_id: Option<NodeId>,  // Reverse action on failure
}

pub struct SagaDefinition {
    pub steps: Vec<SagaStep>,  // Ordered
    pub on_complete: Option<NodeId>,
    pub on_failure: Option<NodeId>,
}
```

**Execution semantics:**
1. Execute steps in order (or in DAG order if parallel)
2. On failure at step N: execute compensation for steps N-1, N-2, ..., 1 in reverse
3. Compensation is best-effort (log failures, don't retry infinitely)

**Checklist:**

```
[ ] Add SagaStep and SagaDefinition to flow-engine
[ ] Add compensation edge type to FlowNode graph
[ ] Implement reverse-order compensation execution
[ ] Store saga state in flow checkpoints
[ ] Emit events: flow.saga.step_completed, flow.saga.compensating, flow.saga.compensated
[ ] Define compensation actions for existing pipelines:
    - Harvest → Content: on content failure, mark opportunity as "draft_failed"
    - GTM outreach: on send failure, revert contact status to "pending"
```

### 6.3 Event projections (lightweight CQRS)

**Problem:** Reading domain state requires querying ObjectStore + joining with Events for timeline. No pre-computed read models.

**Solution:** Lightweight projections — event subscribers that maintain denormalized read views.

```rust
pub trait Projection: Send + Sync {
    fn name(&self) -> &str;
    fn handles(&self) -> &[&str];  // Event kinds this projection processes
    async fn apply(&self, event: &Event, store: &dyn ObjectStore) -> Result<()>;
}

// Example: Department activity feed projection
pub struct DepartmentActivityProjection;

impl Projection for DepartmentActivityProjection {
    fn name(&self) -> &str { "department_activity" }
    fn handles(&self) -> &[&str] { &["code.analyzed", "content.drafted", "harvest.scored", "gtm.outreach.sent"] }

    async fn apply(&self, event: &Event, store: &dyn ObjectStore) -> Result<()> {
        // Write a denormalized activity entry to ObjectStore
        let activity = json!({
            "department": extract_dept(event),
            "action": event.kind,
            "summary": extract_summary(event),
            "timestamp": event.created_at,
        });
        store.put("department_activity", &Uuid::now_v7().to_string(), &activity).await
    }
}
```

**Checklist:**

```
[ ] Define Projection trait in rusvel-core
[ ] Register projections during department boot (alongside event subscriptions)
[ ] Wire projection execution in event dispatch (after primary handler, non-blocking)
[ ] Implement DepartmentActivityProjection — feeds the dashboard
[ ] Implement PipelineStatusProjection — feeds the GTM pipeline view
[ ] Implement CostProjection — aggregates LLM costs by department/session
[ ] Add GET /api/dept/:id/activity endpoint reading from projection store
```

### 6.4 Capability-based tool access

**Current:** `ScopedToolRegistry` filters tools by department prefix. This is a visibility boundary, not a security boundary.

**Improvement:** Per-agent-run capability tokens.

```
[ ] Generate a RunCapability token at agent run start
    - Contains: run_id, session_id, department_id, allowed_tool_patterns, expiry
    - Signed with a per-session HMAC key

[ ] Tool execution checks capability token
    - Token must be valid, not expired, and tool must match allowed patterns
    - Prevents tool replay attacks (token is single-use per run)

[ ] Log capability grants and exercises for audit trail
```

**Priority:** Low for single-user. Essential before multi-user or if agents can spawn sub-agents.

---

## 7. Sprint plan — execution order

| Sprint | Theme | Effort | Impact | Dependencies |
|--------|-------|--------|--------|-------------|
| 1 | Type safety and domain model | Medium | High | None |
| 2 | Builders, factories, composition root | Medium | High | Sprint 1 |
| 3 | Decorator, proxy, facade (AppState) | Medium | High | Sprint 2 |
| 4 | Strategy, observer, command, chain | High | High | Sprint 1 |
| 5 | DDD: aggregates, associations, value objects | Medium | Medium | Sprint 1 |
| 6 | Event architecture: saga, projections | Medium | High | Sprints 4, 5 |
| 7 | Plugin and capability architecture | Low | Medium | Sprint 4 |
| 8 | Workflow engine hardening | Medium | High | Sprint 6 |
| 9 | Agent orchestration hierarchy | Medium | High | Sprints 4, 8 |
| 10 | Scalability and cross-cutting | Low | Medium | Sprints 3, 4 |
| 11 | Frontend architecture alignment | Medium | Medium | Sprint 3 |
| 12 | Self-improving feedback loops | Medium | High | Sprints 4, 6, 9 |

---

## 8. Sprint 1 — Foundation: Type safety and domain model

> Goal: Eliminate stringly-typed runtime errors. Strengthen the domain model spine.

### Exit criteria

- [ ] All event kinds used in code are defined as constants (no string literals)
- [ ] All job kinds are registered at boot with collision detection
- [ ] All tool names validated for uniqueness at registration
- [ ] Department IDs validated as URL-safe slugs
- [ ] Newtype IDs used consistently (no raw Uuid in public APIs)

### Detailed checklist

```
[ ] 1.1 Event kind constants
    File: crates/rusvel-core/src/event_kinds.rs (new)
    - Define pub const for each known event kind
    - Group by department: pub mod code { pub const ANALYZED: &str = "code.analyzed"; }
    - Engines reference constants instead of string literals
    - Keep Event.kind as String for extensibility (ADR-005 preserved)
    - Lint: grep for string literals matching "*.analyzed", "*.drafted" etc → replace with constants

[ ] 1.2 Tool name validation
    File: crates/rusvel-tool/src/lib.rs
    - Change register_with_handler() to return Err on duplicate name
    - Add tool name format validation: lowercase, dots as namespace separator
    - Log warning for tools registered without JSON Schema

[ ] 1.3 Job kind registry
    File: crates/rusvel-jobs/src/lib.rs
    - Add JobKindRegistry that collects registered kinds at boot
    - Detect duplicate handler registrations → return Err
    - Validate job kind on enqueue: warn if unregistered (don't fail — Custom jobs)

[ ] 1.4 Department ID validation
    File: crates/rusvel-core/src/department/mod.rs
    - Add validation in DepartmentManifest: id must be [a-z][a-z0-9-]* (URL slug)
    - Validate at manifest creation, not just at boot

[ ] 1.5 Newtype audit
    - Grep for raw Uuid in public function signatures
    - Replace with appropriate newtype (SessionId, GoalId, etc.)
    - Ensure all newtypes implement Display, FromStr, Serialize, Deserialize
```

---

## 9. Sprint 2 — Creational: Builders, factories, composition root

> Goal: Compile-time safe construction. Testable factory pattern.

### Exit criteria

- [ ] DepartmentManifest uses typestate builder (can't build without id + name + system_prompt)
- [ ] PlatformFactory trait defined; TestFactory replaces ad-hoc test harness
- [ ] AgentConfig uses builder with validation
- [ ] LlmRequest uses builder

### Detailed checklist

```
[ ] 2.1 DepartmentManifest typestate builder
    File: crates/rusvel-core/src/department/builder.rs (new)
    - Required fields: id, name, system_prompt
    - Optional fields: icon, color, description, capabilities, etc. (with defaults)
    - Migrate all 14 dept-* crates to use builder

[ ] 2.2 PlatformFactory trait
    File: crates/rusvel-core/src/factory.rs (new)
    - Define trait with one method per required port
    - Implement ProductionFactory in rusvel-app
    - Implement TestFactory for use in rusvel-api/tests/common/
    - RegistrationContext constructed from factory output

[ ] 2.3 AgentConfig builder
    File: crates/rusvel-core/src/domain.rs (modify AgentConfig)
    - Required: session_id, system_prompt
    - Optional: persona, max_iterations, temperature, tools
    - Typestate: NoSession → HasSession, then build()

[ ] 2.4 LlmRequest builder
    File: crates/rusvel-core/src/domain.rs (modify LlmRequest)
    - Required: model, messages
    - Optional: temperature, max_tokens, tools, metadata
```

---

## 10. Sprint 3 — Structural: Decorator, proxy, facade hardening

> Goal: Composable middleware. Focused handler dependencies. Reduced coupling.

### Exit criteria

- [ ] AppState decomposed into sub-states (CoreState, AgentState, EngineState, InfraState)
- [ ] At least one port (LlmPort or ToolPort) wrapped with tower::Layer middleware
- [ ] Per-route middleware groups (dept routes, webhook routes, health routes)

### Detailed checklist

```
[ ] 3.1 AppState decomposition
    File: crates/rusvel-api/src/state.rs (new)
    - Define CoreState, AgentState, EngineState, InfraState, BootState
    - Implement FromRef<AppState> for each sub-state
    - Migrate handlers one module at a time
    - Start with healthiest boundary: chat handlers → AgentState + CoreState

[ ] 3.2 Tower Layer for LlmPort
    File: crates/rusvel-llm/src/layers.rs (new)
    - CostTrackingLayer: accumulates usage per request
    - TimeoutLayer: wraps LlmPort::generate with tokio timeout
    - LoggingLayer: structured tracing span per LLM call
    - Compose in rusvel-app composition root

[ ] 3.3 Per-route middleware groups
    File: crates/rusvel-api/src/lib.rs
    - Nest dept routes under department middleware (context injection, rate limit)
    - Nest webhook routes under HMAC verification
    - Health route: no middleware
    - System routes: admin auth layer

[ ] 3.4 ScopedToolRegistry → ToolProxy with audit
    File: crates/rusvel-tool/src/proxy.rs (new or extend scope.rs)
    - Wrap ScopedToolRegistry with audit logging
    - Log every tool call: who, what tool, what args, result status
    - Add metrics: tool call count per department, latency histogram
```

---

## 11. Sprint 4 — Behavioral: Strategy, observer, command, chain

> Goal: Type-safe event subscriptions. Typed job commands. Visitor for code analysis.

### Exit criteria

- [ ] TypedEvent trait defined and used by at least 3 engines
- [ ] JobCommand trait defined and used by existing job kinds
- [ ] SymbolVisitor trait in code-engine with at least 2 visitor implementations
- [ ] DelegateAgentTool registered for forge department

### Detailed checklist

```
[ ] 4.1 TypedEvent system (see §4.3 Observer with Typed Channels)
[ ] 4.2 JobCommand system (see §4.3 Command Pattern Enhancement)
[ ] 4.3 SymbolVisitor in code-engine (see §4.3 Visitor Pattern)
[ ] 4.4 DelegateAgentTool (see §4.3 Mediator Pattern)
[ ] 4.5 Enum dispatch for closed strategy sets (ModelTier, ConditionEvaluator)
```

---

## 12. Sprint 5 — DDD: Aggregates, value objects, bounded contexts

> Goal: Explicit aggregate roots. Association graph. Stronger bounded context boundaries.

### Exit criteria

- [ ] AggregateRoot marker trait defined and applied to 6+ types
- [ ] ObjectStore has relate/related/unrelate methods
- [ ] SQLite migration adds object_relations table
- [ ] At least one engine uses associations (harvest: Opportunity → Contact)

### Detailed checklist

```
[ ] 5.1 AggregateRoot marker trait
    File: crates/rusvel-core/src/domain.rs
    - Define trait with id() method
    - Implement for: Session, Opportunity, ContentItem, Contact, Workflow, Goal

[ ] 5.2 Value object enforcement
    - Audit domain types: which are entities (have id) vs value objects (compared by value)
    - Value objects: derive PartialEq, Eq, Clone. No id field
    - Examples: Score, Platform, ModelTier, ApprovalPolicy, QuickAction
    - Ensure value objects are immutable (no &mut self methods)

[ ] 5.3 ObjectStore association graph
    File: crates/rusvel-core/src/ports.rs (extend ObjectStore trait)
    File: crates/rusvel-db/src/store.rs (SQLite implementation)
    - Add relate(), related(), unrelate() methods
    - Add SQLite migration for object_relations table
    - Index: (from_kind, from_id, relation), (to_kind, to_id, relation)

[ ] 5.4 Bounded context documentation
    File: crates/rusvel-core/src/lib.rs (module doc comments)
    - Document which types belong to which bounded context
    - Document shared kernel types (types used across all contexts)
    - Document anti-corruption layer patterns (how contexts translate between types)
```

---

## 13. Sprint 6 — Event architecture: Saga, projections, replay

> Goal: Multi-step flow compensation. Denormalized read views. Event replay capability.

### Exit criteria

- [ ] Projection trait defined and at least 2 projections running
- [ ] Saga definition with compensation edges in flow-engine
- [ ] Event replay utility: rebuild projections from event history

### Detailed checklist

```
[ ] 6.1 Projection system (see §6.3)
[ ] 6.2 Saga pattern in flow-engine (see §6.2)
[ ] 6.3 Event replay utility
    File: crates/rusvel-event/src/replay.rs (new)
    - replay_from(timestamp, projections: &[&dyn Projection]) → re-applies events
    - Useful for: rebuilding projections after schema change, debugging
    - NOT full event sourcing — ObjectStore remains the primary write model
```

---

## 14. Sprint 7 — Plugin and capability architecture

> Goal: Strengthen the DepartmentApp plugin boundary. Prepare for future WASM extensions.

### Exit criteria

- [ ] Department manifest schema validation at boot
- [ ] Port requirement validation (manifest declares required ports → boot checks they exist)
- [ ] Capability token prototype for tool access

### Detailed checklist

```
[ ] 7.1 Manifest schema validation
    File: crates/rusvel-core/src/department/validation.rs (new)
    - Validate manifest JSON Schema (config_schema) is valid JSON Schema
    - Validate routes don't conflict across departments
    - Validate tool contributions have valid schemas
    - Report all validation errors at boot (not fail-fast per department)

[ ] 7.2 Port requirement validation
    File: crates/rusvel-core/src/department/boot.rs
    - After all departments registered, check each manifest's requires_ports
    - If a required port was not injected (is None), emit warning with department name
    - If a required port is missing and department declared it as hard requirement, fail boot

[ ] 7.3 Plugin boundary documentation
    - Document the "plugin contract": what a DepartmentApp can rely on
    - Document what is NOT guaranteed (internal port behavior, execution order)
    - Prepare for future WASM boundary: identify which port methods are WASM-safe

[ ] 7.4 Capability token prototype (see §6.4)
```

---

## 15. Sprint 8 — Workflow engine hardening

> Goal: Checkpoint/resume. Nested flows. Loop/parallel nodes. Time-delayed scheduling.

### Exit criteria

- [ ] Flow execution persists per-node results (checkpoint)
- [ ] Resume from checkpoint on restart
- [ ] SubFlowNode, ParallelNode, LoopNode variants added
- [ ] Per-node retry policy

### Detailed checklist

```
[ ] 8.1 Per-node result persistence
    File: crates/flow-engine/src/checkpoint.rs (new or extend)
    - Store each node's output in ObjectStore keyed by (execution_id, node_id)
    - On resume: check which nodes have stored results → skip them
    - Delete checkpoint data on flow completion (configurable retention)

[ ] 8.2 Composite flow nodes (see §4.2 Composite Pattern)

[ ] 8.3 Per-node retry policy
    - Add retry_policy to FlowNode: max_attempts, backoff (fixed/exponential), jitter
    - Executor retries failed nodes per policy before marking flow as failed
    - Emit event per retry: flow.node.retrying

[ ] 8.4 Time-delayed node execution
    - Add DelayNode variant: waits for Duration before proceeding
    - Enable outreach sequences: send email → wait 3 days → follow up
    - Persist delay state in checkpoint (survives restart)

[ ] 8.5 Flow-level timeout
    - Add max_duration to FlowDefinition
    - Executor cancels running nodes if flow exceeds timeout
    - Emit event: flow.execution.timed_out
```

---

## 16. Sprint 9 — Agent orchestration hierarchy

> Goal: Hierarchical agent delegation. Supervisor pattern. Cross-department coordination.

### Exit criteria

- [ ] DelegateAgentTool working for forge → any department
- [ ] Supervisor agent pattern: forge monitors sub-agent runs
- [ ] Agent run depth tracking (prevent infinite delegation)
- [ ] Cross-department pipeline via delegation (harvest → content → gtm)

### Detailed checklist

```
[ ] 9.1 DelegateAgentTool (see §4.3 Mediator)

[ ] 9.2 Supervisor pattern
    File: crates/forge-engine/src/supervisor.rs (new)
    - ForgeEngine::orchestrate_pipeline uses DelegateAgentTool
    - Monitors sub-agent AgentEvent streams
    - Aggregates results from multiple department agents
    - Reports unified status to caller

[ ] 9.3 Agent run depth tracking
    File: crates/rusvel-agent/src/lib.rs
    - Add depth: u32 to AgentConfig
    - DelegateAgentTool increments depth
    - Max depth: 3 (configurable)
    - Reject delegation at max depth with descriptive error

[ ] 9.4 Blackboard pattern for cross-engine awareness
    File: crates/rusvel-core/src/blackboard.rs (new, or extend ObjectStore)
    - Engines write structured findings to well-known keys
    - Other engines read findings without direct coupling
    - Key convention: blackboard/{dept}/{topic} (e.g., blackboard/harvest/latest_opportunities)
    - Engines subscribe to blackboard changes via EventPort (emit "blackboard.updated")
```

---

## 17. Sprint 10 — Scalability and cross-cutting concerns

> Goal: Extract magic numbers. Graceful shutdown. Data-driven CLI. Build acceleration.

### Exit criteria

- [ ] All magic numbers extracted to ConfigPort with documented defaults
- [ ] Job worker supports graceful shutdown via tokio CancellationToken
- [ ] CLI department dispatch is data-driven (not 9 repeated match arms)
- [ ] sccache configured for local build caching

### Detailed checklist

```
[ ] 10.1 Magic number extraction
    File: crates/rusvel-config/src/defaults.rs (new)
    - Agent max iterations: 10 → config("agent.max_iterations", 10)
    - Message compaction threshold: 30 → config("agent.compaction_threshold", 30)
    - Broadcast channel capacity: 256 → config("event.broadcast_capacity", 256)
    - Job worker backoff: 100ms/500ms → config("jobs.backoff_empty_ms", 100)
    - Context pack TTL: 45s → config("api.context_pack_ttl_secs", 45)
    - Rate limit: 100 req/s → config("api.rate_limit", 100)
    - Graceful shutdown grace: 5s → config("api.shutdown_grace_secs", 5)
    - Agent channel capacity: 64 → config("agent.channel_capacity", 64)

[ ] 10.2 Graceful shutdown for job worker
    File: crates/rusvel-jobs/src/lib.rs
    - Replace infinite polling loop with tokio::select! on CancellationToken
    - Main signals cancellation on SIGTERM/SIGINT
    - Worker finishes current job, then exits
    - Log: "Job worker shutting down, finishing current job..."

[ ] 10.3 Data-driven CLI department dispatch
    File: crates/rusvel-cli/src/lib.rs
    - Replace 9 match arms (Finance, Growth, Distro, ...) with a single generic handler
    - Use department registry to look up engine by string ID
    - CLI subcommands generated from DepartmentManifest.commands

[ ] 10.4 Build acceleration
    - Add .cargo/config.toml with sccache wrapper
    - Document: RUSTC_WRAPPER=sccache cargo build
    - Add cargo-hakari for unified feature resolution (optional)
    - Add cargo-nextest for parallel test execution (optional)
    - Document in CLAUDE.md Quick Commands section

[ ] 10.5 CORS configuration from config
    File: crates/rusvel-api/src/lib.rs
    - Move CORS origins to ConfigPort: config("api.cors_origins", ["http://localhost:5173"])
    - Support wildcard for development: config("api.cors_allow_all", false)
```

---

## 18. Sprint 11 — Frontend architecture alignment

> Goal: Apply patterns to frontend that mirror backend architecture.

### Exit criteria

- [ ] API client uses typed response interfaces (not `any`)
- [ ] Store pattern follows command/query separation
- [ ] Component hierarchy mirrors concept hierarchy
- [ ] Department manifest drives navigation (already partially done)

### Detailed checklist

```
[ ] 11.1 Typed API client
    File: frontend/src/lib/api.ts
    - Define TypeScript interfaces for ALL API response types
    - Each API function returns typed response (not Promise<any>)
    - Use zod or io-ts for runtime response validation (optional)
    - Group API functions by domain: sessions, departments, chat, flows, etc.

[ ] 11.2 Command/Query store separation
    File: frontend/src/lib/stores/
    - Split stores into queries (read) and commands (write)
    - Queries: derived stores that auto-refresh from API
    - Commands: action functions that mutate state and invalidate queries
    - Pattern mirrors CQRS from backend

[ ] 11.3 Component hierarchy alignment
    - Shell components → Level 0 (Platform)
    - Session components → Level 1 (Session)
    - Department components → Level 2 (Department)
    - Entity components → Level 3 (Domain entities)
    - Primitive UI components → shared across all levels

[ ] 11.4 Department-driven routing
    - Verify /dept/[id] route loads manifest from API
    - All department-specific UI generated from manifest (tabs, actions, tools)
    - Adding a department requires zero frontend changes (already partially true)

[ ] 11.5 Error boundary per department
    - Each department page wrapped in Svelte error boundary
    - Department failure doesn't crash the whole app
    - Show department-specific error state with retry
```

---

## 19. Sprint 12 — Self-improving system feedback loops

> Goal: Close the loop — agent failures generate skills, successful patterns accumulate.

### Exit criteria

- [ ] Failed agent runs trigger reflection analysis
- [ ] Reflection generates skill/rule suggestions stored in ObjectStore
- [ ] Successful tool call sequences persisted as reusable skill templates
- [ ] Benchmark gating: auto-approve agents that pass test set

### Detailed checklist

```
[ ] 12.1 Failure reflection loop
    File: crates/forge-engine/src/reflection.rs (new)
    - On agent run failure (AgentEvent::Error or empty result):
      1. Emit event: forge.run.failed with full transcript
      2. Hook triggers reflection agent
      3. Reflection agent analyzes transcript with prompt:
         "What went wrong? What skill or rule would prevent this?"
      4. Output: suggested Skill or Rule stored as draft in ObjectStore
      5. Human reviews draft in approval queue

[ ] 12.2 Skill accumulation from success
    File: crates/forge-engine/src/skill_miner.rs (new)
    - On agent run success with tool calls:
      1. Extract tool call sequence as a "recipe"
      2. If recipe matches no existing skill → suggest new skill
      3. If recipe matches existing skill → increment success counter
      4. Skills with high success count → auto-promote to default

[ ] 12.3 Experience replay for prompting
    File: crates/rusvel-agent/src/context.rs (extend)
    - Before agent run: query MemoryPort for similar past successes
    - Inject 1-2 successful trajectories as few-shot examples
    - Track which examples were used → correlate with success/failure

[ ] 12.4 Benchmark gating for auto-approval
    File: crates/rusvel-jobs/src/approval.rs (new or extend)
    - Define "test set" per department: known inputs with expected outputs
    - Before auto-approving a job, run agent on test set
    - If test set passes → auto-approve
    - If test set fails → hold for human approval
    - Configurable per department via DepartmentManifest.config_schema
```

---

## 20. Anti-patterns to avoid

| Anti-pattern | Why it's tempting | Why it's wrong for RUSVEL |
|-------------|-------------------|--------------------------|
| **Full actor framework** (actix actors) | Clean message passing | Adds complexity without benefit at single-binary scale. `tokio::spawn` + channels is sufficient |
| **Full event sourcing** (aggregate reconstitution from events) | Perfect audit trail | High query complexity. ObjectStore + EventPort combination is appropriate until audit trails are explicitly required |
| **Dynamic `.so` plugin loading** | Hot-reload departments | Rust has no stable ABI. Single binary is strictly superior. Use WASM if needed |
| **Generic trait bounds on stored structs** | Avoids vtable overhead | Monomorphization across 55 crates causes compile time bloat. Prefer `Arc<dyn Trait>` |
| **Typestate everywhere** | Compile-time state safety | More than ~5 states becomes unmaintainable. Typestate can't survive serde boundaries. Use for internal hot paths only |
| **Global static singletons** | Easy access to shared state | Breaks testability. Pass `Arc<T>` via constructors |
| **Premature abstraction** | "What if we need to..." | Build for today's 14 departments, not hypothetical 100. Extract patterns after 3+ instances |
| **Over-sealed traits** | Prevent misuse | RUSVEL's ports should be implementable by adapters in the workspace. Seal only if publishing as external library |
| **DI container/framework** | "Spring for Rust" | Rust's type system + composition root is the DI framework. No reflection needed |
| **Microservices** | Independent deployment | RUSVEL's value proposition is ONE binary. Split only at extreme scale (>100K users) |

---

## 21. Success metrics

| Metric | Current | Target | How to Measure |
|--------|---------|--------|----------------|
| String literals for event kinds | ~40 | 0 | `grep -r '"[a-z]*\.[a-z]*"' crates/` excluding tests |
| Tool name collisions at boot | Silent | Error | Boot validation |
| Invalid job transitions (runtime) | Possible | Impossible (in hot path) | Typestate audit |
| AppState fields | ~25 | 5 sub-states × ~5 fields | Count struct fields |
| Port middleware layers (tower) | 0 | 3+ (cost, timeout, logging) | Count Layer impls |
| Association graph queries | 0 | Available on ObjectStore | API test |
| Projections running | 0 | 3+ (activity, pipeline, cost) | Boot log |
| Agent delegation depth support | 0 | Max 3 levels | Config + test |
| Flow checkpoint/resume | No | Yes | Integration test |
| Saga compensation | No | Yes | Integration test |
| Frontend typed API responses | ~10% | 100% | TypeScript strict mode errors |
| Magic numbers in code | ~10 | 0 (all in ConfigPort) | `grep` for numeric literals in non-test code |
| Self-improvement feedback loops | 0 | 2 (failure reflection + skill mining) | Feature flag check |
| Build cache hit rate | 0% | >60% | sccache stats |

---

## 22. References

### Gang of Four in Rust
- [GoF Design Patterns in Rust — fadeevab/design-patterns-rust](https://github.com/fadeevab/design-patterns-rust)
- [Refactoring.guru — Design Patterns in Rust](https://refactoring.guru/design-patterns/rust)
- [Rust Design Patterns — rust-unofficial](https://rust-unofficial.github.io/patterns/)
- [SOLID Principles in Rust](https://rust-unofficial.github.io/patterns/additional_resources/design-principles.html)

### Rust-Specific Patterns
- [Typestate Pattern — Cliffle](https://cliffle.com/blog/rust-typestate/)
- [Builder with Typestate — greyblake.com](https://www.greyblake.com/blog/builder-with-typestate-in-rust/)
- [Sealed Traits — Rust API Guidelines](https://rust-lang.github.io/api-guidelines/future-proofing.html)
- [Visitor Pattern — rust-unofficial](https://rust-unofficial.github.io/patterns/patterns/behavioural/visitor.html)

### Architecture
- [Hexagonal Architecture in Rust — howtocodeit.com](https://www.howtocodeit.com/guides/master-hexagonal-architecture-in-rust)
- [Clean Architecture — Uncle Bob](https://blog.cleancoder.com/uncle-bob/2012/08/13/the-clean-architecture.html)
- [Domain-Driven Design — rust-cqrs.org](https://doc.rust-cqrs.org/theory_ddd.html)
- [CQRS and Event Sourcing in Rust — doc.rust-cqrs.org](https://doc.rust-cqrs.org/)

### Tower and Middleware
- [Inventing the Service Trait — Tokio blog](https://tokio.rs/blog/2021-05-14-inventing-the-service-trait)
- [Tower Service docs](https://docs.rs/tower/latest/tower/trait.Service.html)
- [Tower Middleware for Axum](https://docs.rs/axum/latest/axum/middleware/index.html)

### System Design
- [AI Agent Orchestration Workflows](https://www.digitalapplied.com/blog/ai-agent-orchestration-workflows-guide)
- [CrewAI vs LangGraph vs AutoGen — DataCamp](https://www.datacamp.com/tutorial/crewai-vs-langgraph-vs-autogen)
- [Saga Pattern — microservices.io](https://microservices.io/patterns/data/saga.html)
- [Event-Driven Architecture — Solace](https://solace.com/event-driven-architecture-patterns/)
- [MACH Architecture](https://macharchitecture.com)
- [Capability-Based Security — Wikipedia](https://en.wikipedia.org/wiki/Capability-based_security)
- [Self-Improving AI Agents — Yohei Nakajima](https://yoheinakajima.com/better-ways-to-build-self-improving-ai-agents/)

### Monorepo and Build
- [Cargo Workspace — Rust book](https://doc.rust-lang.org/book/ch14-03-cargo-workspaces.html)
- [sccache — Rust build cache](https://github.com/mozilla/sccache)
- [cargo-nextest — parallel test runner](https://nexte.st/)
- [cargo-hakari — unified feature resolution](https://docs.rs/cargo-hakari/latest/cargo_hakari/)

### Real-World Rust Projects (pattern study)
- [Bevy ECS — Plugin architecture](https://docs.rs/bevy_ecs/latest/bevy_ecs/)
- [Axum — Tower Service + Router](https://docs.rs/axum/latest/axum/)
- [SQLx — Typestate queries](https://docs.rs/sqlx/latest/sqlx/)
- [Ratatui — Widget trait (Strategy)](https://docs.rs/ratatui/latest/ratatui/)
