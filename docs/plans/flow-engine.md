# RUSVEL Flow — n8n-style Workflow Automation Engine

> **STATUS: IMPLEMENTED.** Flow Engine is wired with petgraph DAG executor, 3 node types (code, condition, agent), 7 API routes at `/api/flows`, and visual workflow builder in the frontend. See `flow-engine` and `dept-flow` crates.

> Original proposal below (DAG-based workflow engine with visual builder, triggers, conditionals, and error handling — all within the single-binary constraint).

## Parameter templates (MiniJinja)

Node `parameters` JSON is walked recursively; **string leaves** that contain `{{ … }}` are rendered with [MiniJinja](https://github.com/mitsuhiko/minijinja) against a per-node context built in [`flow_parameter_context`](../../crates/flow-engine/src/expressions.rs):

| Source | In template context |
|--------|---------------------|
| Flow run `trigger_data` | Each **top-level object key** is a variable (e.g. `department_id`, `rusvel`). |
| Upstream node outputs | **Flat:** each upstream node UUID string → that node’s output JSON. |
| Nested aliases | `flow_trigger` — full trigger object. `upstream` — map of upstream node id → output. |

Examples (string parameters):

- `{{ department_id }}` — from trigger.
- `{{ flow_trigger.rusvel.job_id }}` — when the worker set [`trigger_data.rusvel`](../../crates/rusvel-api/src/automation.rs) (cron / automation job).
- `{{ upstream["<node-uuid>"].field }}` — drill into a predecessor output (use the node’s id string).

If rendering fails, the **original string is kept**. Set **`RUSVEL_FLOW_TEMPLATE_DEBUG=1`** to log template errors at `warn` (operator troubleshooting).

Legacy `{{node_id}}` resolution for some string templates lives in [`expression.rs`](../../crates/flow-engine/src/expression.rs); prefer MiniJinja + the context above for new flows.

## Motivation

n8n's power comes from three things: (1) visual node graph, (2) rich node type system, (3) event-driven triggers. The current workflow system (`crates/rusvel-agent/src/workflow.rs`) only supports `Sequential | Parallel | Loop | Agent` — no conditionals, no triggers, no branching, no error paths. The API layer (`rusvel-api/src/workflows.rs`) stores flat step lists with `agent_name + prompt_template`. This is Phase 0 scaffolding. RUSVEL Flow is Phase 1.

## What We Already Have (and What It Maps To)

| RUSVEL Today | n8n Equivalent | Gap |
|---|---|---|
| `WorkflowStep::Agent` | Action Node | No parameter schema, no typed I/O |
| `WorkflowStep::Sequential` | Linear chain | No branching/merge |
| `WorkflowStep::Parallel` | Split node | No join/merge semantics |
| `WorkflowStep::Loop` | Loop node | Works, needs condition expressions |
| `JobQueue` | Execution queue | No execution tracking per-node |
| `EventBus` | Event triggers | Not wired as workflow triggers |
| `ToolPort` | Built-in nodes | Already trait-based — great fit |
| `AgentPort` | AI nodes | Perfect — ADR-009 keeps this clean |
| `WorkflowBuilder.svelte` | Canvas | Uses @xyflow — foundation exists |
| `ObjectStore("workflows")` | Workflow storage | Needs richer schema |

## Architecture

### Layer 1: Domain Model (rusvel-core) — New Types

```rust
WorkflowDef {
    id: WorkflowId,
    name: String,
    description: String,
    nodes: Vec<NodeDef>,             // The graph nodes
    connections: Vec<ConnectionDef>, // Edges between nodes
    variables: HashMap<String, String>, // Default variables
    trigger: Option<TriggerDef>,     // How this workflow starts
    error_workflow_id: Option<WorkflowId>,
    metadata: Value,
}

NodeDef {
    id: NodeId,
    node_type: String,           // "agent", "http", "if", "code", "split", "merge", "wait"
    name: String,
    parameters: Value,           // Node-specific config (JSON)
    position: (f64, f64),        // Canvas position
    retry: Option<RetryConfig>,  // Per-node retry
    on_error: ErrorBehavior,     // StopWorkflow | ContinueOnFail | UseErrorOutput
    metadata: Value,
}

ConnectionDef {
    source_node: NodeId,
    source_output: String,       // "main", "true", "false", "error"
    target_node: NodeId,
    target_input: String,        // "main"
}

TriggerDef {
    kind: TriggerKind,           // Webhook, Cron, Event, Manual
    config: Value,               // Trigger-specific parameters
}

WorkflowExecution {
    id: ExecutionId,
    workflow_id: WorkflowId,
    status: ExecutionStatus,     // Queued | Running | Succeeded | Failed | Cancelled
    trigger_data: Value,         // What started this execution
    node_results: HashMap<NodeId, NodeResult>,
    started_at: DateTime<Utc>,
    finished_at: Option<DateTime<Utc>>,
    error: Option<String>,
    metadata: Value,
}

NodeResult {
    status: NodeStatus,          // Pending | Running | Succeeded | Failed | Skipped
    output: Option<Value>,       // JSON output data
    error: Option<String>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    retries: u32,
}
```

**Reasoning:** Mirrors n8n's JSON structure (nodes array + connections) but is strongly typed. `node_type` is a String (following ADR-005 pattern for Event.kind) — open for extension without enum explosion. Canvas position stored alongside logic so the visual builder roundtrips perfectly.

### Layer 2: Port Trait (rusvel-core)

```rust
#[async_trait]
pub trait WorkflowPort: Send + Sync {
    // CRUD
    async fn save_workflow(&self, def: &WorkflowDef) -> Result<WorkflowId>;
    async fn get_workflow(&self, id: &WorkflowId) -> Result<Option<WorkflowDef>>;
    async fn list_workflows(&self) -> Result<Vec<WorkflowDef>>;
    async fn delete_workflow(&self, id: &WorkflowId) -> Result<()>;

    // Execution
    async fn start_execution(&self, id: &WorkflowId, trigger_data: Value) -> Result<ExecutionId>;
    async fn get_execution(&self, id: &ExecutionId) -> Result<Option<WorkflowExecution>>;
    async fn list_executions(&self, workflow_id: &WorkflowId) -> Result<Vec<WorkflowExecution>>;
    async fn cancel_execution(&self, id: &ExecutionId) -> Result<()>;
    async fn retry_execution(&self, id: &ExecutionId) -> Result<ExecutionId>;
}
```

**Reasoning:** Follows hexagonal pattern. Engines call this port, never the DB directly. Execution tracking is first-class — every node's result is persisted, enabling the UI to show real-time progress and historical replay.

### Layer 3: New Crate — `flow-engine`

The DAG execution engine. Sits alongside `forge-engine` etc.

```
crates/flow-engine/
├── src/
│   ├── lib.rs          // FlowEngine struct (depends on WorkflowPort + AgentPort + ToolPort + EventPort)
│   ├── executor.rs     // DAG walker: topological sort → parallel branch execution
│   ├── nodes/
│   │   ├── mod.rs      // NodeHandler trait
│   │   ├── agent.rs    // Calls AgentPort (LLM-powered node)
│   │   ├── http.rs     // HTTP request node (reqwest)
│   │   ├── code.rs     // Expression evaluator (rhai)
│   │   ├── condition.rs // If/else branching
│   │   ├── split.rs    // Fan-out to parallel branches
│   │   ├── merge.rs    // Fan-in: wait for all/any inputs
│   │   ├── wait.rs     // Delay/sleep node
│   │   ├── tool.rs     // Calls ToolPort (reuse existing tools)
│   │   └── trigger.rs  // Webhook, Cron, Event-based triggers
│   ├── expression.rs   // Template expressions: {{ $node["name"].output.field }}
│   └── registry.rs     // Maps node_type strings → NodeHandler impls
```

#### Key Design Decisions

**1. petgraph for DAG execution**

Build `StableDiGraph` from `WorkflowDef` at execution time. Topological sort gives execution order. Independent branches run in parallel via `tokio::JoinSet` (already used in `WorkflowStep::Parallel`).

**2. NodeHandler trait**

Each node type implements:

```rust
#[async_trait]
pub trait NodeHandler: Send + Sync {
    fn node_type(&self) -> &str;
    fn parameter_schema(&self) -> Value; // JSON Schema for UI
    async fn execute(&self, ctx: &NodeContext) -> Result<NodeOutput>;
}
```

Where `NodeContext` carries: input data from upstream nodes, parameters, credentials, expression resolver.

**3. Rhai as expression language**

n8n uses JavaScript; Rhai gives similar power without linking V8. Pure Rust, embeddable, safe sandbox. Expressions like `{{ nodes.step1.output.title }}` resolve at runtime. Rhai's syntax is close to JS which matches n8n's feel. Zero FFI — compiles into the single binary.

**4. Error paths via named outputs**

Each connection has an output name. Condition nodes output to `"true"` / `"false"`. Failed nodes (with `on_error: UseErrorOutput`) route to `"error"` output. This gives n8n-style error workflows without a separate concept.

### Layer 4: Trigger System

| Trigger Type | Implementation | Where It Lives |
|---|---|---|
| **Manual** | `POST /api/flows/{id}/run` | rusvel-api |
| **Webhook** | Dynamic route registration on Axum | rusvel-api (new webhook router) |
| **Cron** | Schedule via existing `JobPort` with `JobKind::ScheduledCron` | rusvel-jobs |
| **Event** | Subscribe to `EventBus` patterns | flow-engine listens on EventPort |
| **Chat** | User message in dept chat triggers workflow | rusvel-api/chat.rs |

**Reasoning:** Existing `JobKind::ScheduledCron` and `EventBus` with broadcast channels are reused. Cron triggers enqueue jobs; event triggers subscribe to the bus. Webhooks need a dynamic Axum route table (`axum::Router::nest` with catch-all at `/webhook/{workflow_id}`).

### Layer 5: Frontend — Upgrade WorkflowBuilder

The existing `WorkflowBuilder.svelte` already uses `@xyflow/svelte`. The upgrade:

```
frontend/src/lib/components/flow/
├── FlowCanvas.svelte       // Full DAG canvas (replaces linear step list)
├── FlowNodePanel.svelte    // Right panel: node config form (driven by parameter_schema)
├── FlowToolbar.svelte      // Add node, run, save, execution history
├── nodes/
│   ├── AgentNode.svelte    // LLM agent node (upgrade existing)
│   ├── HttpNode.svelte     // HTTP request config
│   ├── ConditionNode.svelte // If/else with expression editor
│   ├── CodeNode.svelte     // Rhai code editor
│   ├── TriggerNode.svelte  // Webhook/Cron/Event config
│   └── MergeNode.svelte    // Join parallel branches
├── ExecutionView.svelte    // Real-time execution overlay (node status colors)
└── ExecutionHistory.svelte // Past executions list + detail view
```

**Key UI patterns from n8n to adopt:**

- **Node palette** — Sidebar with categorized node types, drag onto canvas
- **Connection handles** — Named outputs (main, true, false, error) with colored dots
- **Execution overlay** — Green/red/yellow borders on nodes during/after execution
- **Expression editor** — Inline `{{ }}` autocomplete referencing upstream node outputs
- **Test step** — Run a single node with sample data (huge for debugging)

### Layer 6: API Routes (rusvel-api)

```
GET    /api/flows                              # List all flow definitions
POST   /api/flows                              # Create flow
GET    /api/flows/{id}                         # Get flow definition
PUT    /api/flows/{id}                         # Update flow
DELETE /api/flows/{id}                         # Delete flow
POST   /api/flows/{id}/run                     # Execute flow (manual trigger)
GET    /api/flows/{id}/executions              # List executions
GET    /api/flows/executions/{exec_id}         # Get execution detail + node results
POST   /api/flows/executions/{exec_id}/cancel  # Cancel running execution
POST   /api/flows/executions/{exec_id}/retry   # Retry failed execution
GET    /api/flows/executions/{exec_id}/stream  # SSE: real-time node status updates
POST   /api/flows/{id}/test-node               # Execute single node with test data
POST   /webhook/{workflow_id}                  # Webhook trigger endpoint
GET    /api/flows/node-types                   # List available node types + schemas
```

**Reasoning:** Separate from existing `/api/workflows` routes — "flows" is the new system, "workflows" stays for backward compat until migration. The SSE stream endpoint reuses the existing SSE pattern from chat.

## Implementation Phases

### Phase 1: Foundation

1. Domain types in `rusvel-core` (WorkflowDef, NodeDef, ConnectionDef, WorkflowExecution, NodeResult)
2. `WorkflowPort` trait in `rusvel-core/src/ports.rs`
3. `flow-engine` crate with `executor.rs` (petgraph DAG walker) + 3 node types: Agent, Condition, Code
4. SQLite persistence for workflows + executions in `rusvel-db`
5. Basic API routes (CRUD + run + execution status)
6. Manual trigger only

### Phase 2: Visual Builder

1. Upgrade frontend canvas to full DAG editing
2. Node palette + parameter forms driven by JSON Schema
3. Execution overlay (SSE-driven)
4. Expression editor with autocomplete

### Phase 3: Triggers & Nodes

1. Webhook trigger (dynamic Axum routes)
2. Cron trigger (via JobPort)
3. Event trigger (via EventBus subscription)
4. HTTP Request node, Tool node, Wait node
5. Split/Merge nodes for explicit fan-out/fan-in

### Phase 4: Production Hardening

1. Per-node retry with exponential backoff
2. Execution timeout + cancellation
3. Credential references (encrypted, never in workflow JSON)
4. Workflow versioning (store versions, rollback)
5. Import/export (JSON format, compatible with sharing)

## Why This Design Is "Design-Proof"

1. **NodeHandler trait is open** — Adding a new node type is one file + register in registry. No enum changes, no core changes. Same pattern as `Event.kind: String` (ADR-005).

2. **Expression engine is pluggable** — Rhai today, could swap to Lua/WASM later. The `ExpressionResolver` is a trait.

3. **Execution is decoupled from definition** — `WorkflowDef` is the blueprint, `WorkflowExecution` is the instance. Run the same workflow concurrently, retry individual executions, version definitions independently.

4. **Reuses the entire stack** — AgentPort for AI nodes, ToolPort for tool nodes, EventPort for triggers, JobPort for scheduling, ObjectStore for persistence. No new infrastructure.

5. **Single binary** — petgraph and rhai are pure Rust, zero FFI. Compiles into the binary like everything else.

6. **Backward compatible** — Existing `/api/workflows` still works. New system lives at `/api/flows`. Migrate when ready.

## Dependencies (new crates)

```toml
petgraph = "0.7"    # DAG representation + topological sort
rhai = "1.21"       # Expression/scripting engine (pure Rust, no FFI)
```

Both are pure Rust, no C dependencies, no FFI — single binary stays single binary.
