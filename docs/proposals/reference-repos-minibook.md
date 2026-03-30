# Reference Repos Minibook: Extracting Value for RUSVEL

> **A comprehensive proposal for integrating patterns, architectures, and capabilities
> from six reference repositories into RUSVEL.**
>
> Date: 2026-03-30
> Status: Proposal
> Audience: Solo builder (Mehdi), future contributors

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [The Six Reference Repos](#2-the-six-reference-repos)
3. [RUSVEL Current State & Gaps](#3-rusvel-current-state--gaps)
4. [Part I: Workflow Engine Patterns (from n8n)](#4-part-i-workflow-engine-patterns-from-n8n)
5. [Part II: Multi-Channel Architecture (from OpenClaw)](#5-part-ii-multi-channel-architecture-from-openclaw)
6. [Part III: AI Harness & Self-Improvement (from Everything Claude Code)](#6-part-iii-ai-harness--self-improvement-from-everything-claude-code)
7. [Part IV: Database & Realtime Patterns (from Supabase)](#7-part-iv-database--realtime-patterns-from-supabase)
8. [Part V: Rust Plugin & IPC Architecture (from Tauri)](#8-part-v-rust-plugin--ipc-architecture-from-tauri)
9. [Part VI: Claude Code Integration Patterns](#9-part-vi-claude-code-integration-patterns)
10. [Cross-Cutting Concerns](#10-cross-cutting-concerns)
11. [Implementation Roadmap](#11-implementation-roadmap)
12. [Appendix: Pattern Catalog](#12-appendix-pattern-catalog)

---

## 1. Executive Summary

RUSVEL has six reference repositories checked out under `repos/`. Each contains
production-tested patterns that can fill specific gaps in RUSVEL's architecture.
This document goes deep into each repo, maps their patterns to RUSVEL's gaps,
and proposes concrete implementation strategies.

**Key thesis:** Don't copy code -- extract patterns, adapt to Rust's type system
and RUSVEL's hexagonal architecture, and implement with RUSVEL's conventions.

### What Each Repo Brings

| Repo | Primary Value | RUSVEL Gap It Fills |
|------|--------------|-------------------|
| **n8n** | Workflow execution engine, node system, partial reruns | `flow-engine` has 5 node types, no retry/rerun, no expression language |
| **OpenClaw** | Multi-channel routing, plugin SDK, session management | `rusvel-channel` is Telegram-only, send-only, 112 lines |
| **Everything Claude Code** | Skills/hooks/rules lifecycle, continuous learning, session persistence | `.claude/` tooling exists but no self-improvement loop |
| **Supabase** | Realtime subscriptions, RLS patterns, component library, vector search | Frontend lacks design system, no realtime push, basic vector ops |
| **Tauri** | Rust plugin architecture, state management, IPC patterns | No plugin system, state is ad-hoc, no desktop distribution path |
| **Claude Code** | CLI patterns, plugin architecture | Reference for Claude Code integration best practices |

### Impact Matrix

```
                    Effort -->
                    Low         Medium        High
               +------------+------------+------------+
  High Impact  | ECC hooks  | n8n nodes  | OpenClaw   |
               | ECC skills | Flow wire  | channels   |
               | Tool search| Cost track | Plugin SDK |
               +------------+------------+------------+
  Med Impact   | Supabase   | Tauri state| Extended   |
               | components | patterns   | engines    |
               | Realtime   | Session    | Desktop    |
               +------------+------------+------------+
  Low Impact   | Claude Code| API version| Collab     |
               | patterns   | ing        | editing    |
               +------------+------------+------------+
```

---

## 2. The Six Reference Repos

### 2.1 n8n (Workflow Automation Platform)

**What it is:** TypeScript monorepo with 400+ integrations, native AI capabilities,
and a visual workflow builder. Fair-code licensed.

**Architecture:**
- `packages/core/` -- Workflow execution engine (node stack, directed graph)
- `packages/workflow/` -- Core interfaces (`INodeType`, `IExecuteFunctions`)
- `packages/nodes-base/` -- 400+ built-in node implementations
- `packages/cli/` -- REST API backend (Express)
- `packages/editor-ui/` -- Vue 3 frontend (Pinia stores)
- `packages/@n8n/design-system/` -- 97 Vue components
- `packages/@n8n/ai-utilities/` -- LangChain integration

**Why it matters for RUSVEL:** RUSVEL's `flow-engine` has 5 node types and basic
DAG execution. n8n has 40+ node types, partial reruns, error recovery, credential
management, and a mature execution model. It's the gold standard for workflow engines.

### 2.2 OpenClaw (Multi-Channel AI Assistant)

**What it is:** Personal AI assistant running on your devices, answering on
WhatsApp, Telegram, Slack, Discord, Signal, iMessage, LINE, IRC, and more.
TypeScript with WebSocket gateway.

**Architecture:**
- `src/channels/` -- Channel plugins (10+ platforms)
- `src/gateway/` -- WebSocket RPC control plane
- `src/agents/` -- Embedded agent runtime
- `src/routing/` -- Hierarchical session routing
- `extensions/` -- Plugin workspace packages
- `skills/` -- 50+ installable skills

**Why it matters for RUSVEL:** RUSVEL's `rusvel-channel` is 112 lines with
Telegram-only, send-only support. OpenClaw has a production-tested multi-channel
abstraction with unified message actions, inbound routing, media pipelines,
threading, and approval gates.

### 2.3 Everything Claude Code (ECC)

**What it is:** Comprehensive agent harness optimization system (Anthropic hackathon
winner). Production-ready agents, skills, hooks, commands, rules, and MCP configs.

**Architecture:**
- `agents/` -- 28 specialized subagent definitions
- `skills/` -- 11 curated + learned + imported skill system
- `commands/` -- 59 slash commands
- `hooks/` -- Event-driven automations (PreToolUse, PostToolUse, Stop, Session*)
- `rules/` -- 39 language-specific + cross-cutting rules
- `scripts/hooks/` -- Hook implementation scripts
- `manifests/` -- Install profiles (core/developer/security/research/full)

**Why it matters for RUSVEL:** RUSVEL already has `.claude/` tooling but lacks
session persistence, continuous learning, self-improvement loops, and the depth
of harness optimization that ECC provides.

### 2.4 Supabase (PostgreSQL Development Platform)

**What it is:** Firebase alternative using open-source tools. Hosted Postgres,
auth, auto-generated APIs, realtime, edge functions, storage, vector toolkit.

**Architecture:**
- `apps/studio/` -- Next.js dashboard
- `packages/ui/` -- 45+ shared components (Radix + CVA)
- `packages/ui-patterns/` -- 40+ higher-order patterns
- `packages/pg-meta/` -- PostgreSQL schema introspection
- `supabase/functions/` -- Edge functions (Deno)
- `supabase/migrations/` -- Timestamped SQL migrations

**Why it matters for RUSVEL:** RUSVEL's frontend lacks a systematic design system.
Supabase's component library (CVA variants, Radix foundations) and its realtime
subscription patterns are directly applicable. The pgvector integration patterns
inform RUSVEL's vector search via LanceDB.

### 2.5 Tauri (Desktop Application Framework)

**What it is:** Framework for building desktop binaries using Rust backend +
any HTML/JS/CSS frontend via system webviews. Multi-platform (macOS, Windows,
Linux, iOS, Android).

**Architecture:**
- `crates/tauri/` -- Core framework (IPC, commands, plugins, state, events)
- `crates/tauri-runtime/` -- Runtime abstraction layer
- `crates/tauri-build/` -- Build pipeline + asset embedding
- `crates/tauri-bundler/` -- Cross-platform packaging
- `packages/api/` -- TypeScript API bindings

**Why it matters for RUSVEL:** Tauri's Rust patterns for plugin systems, type-safe
state management, IPC command macros, and event systems are directly transferable.
RUSVEL already embeds frontend via rust-embed; Tauri shows how to evolve this
into a full desktop app if needed.

### 2.6 Claude Code (CLI Reference)

**What it is:** Anthropic's official agentic coding tool. Terminal-first,
codebase-aware, extensible via plugins and configurations.

**Why it matters for RUSVEL:** Reference for how Claude Code itself works,
informing RUSVEL's MCP server and Claude Code integration patterns.

---

## 3. RUSVEL Current State & Gaps

### 3.1 Architecture Overview

RUSVEL is a 55-member Rust workspace (~73,058 LOC under `crates/*.rs`) implementing hexagonal
architecture with ports & adapters:

```
                    +-----------+
                    | Surfaces  |  CLI, API, MCP, TUI
                    +-----+-----+
                          |
                    +-----+-----+
                    |  Engines  |  13 domain engines
                    +-----+-----+
                          |
              +-----------+-----------+
              |                       |
        +-----+-----+         +------+------+
        |   Ports    |         |  Adapters   |
        | (core)     |         | (db, llm,   |
        | 22 traits  |         |  agent...)  |
        +------------+         +-------------+
```

### 3.2 What's Strong

| Area | Status | Details |
|------|--------|---------|
| Hexagonal architecture | Solid | 22 port traits, clean boundaries enforced |
| DepartmentApp pattern | Solid | 14 dept-* crates, ADR-014 implemented |
| Agent runtime | Mature | Streaming, tool-use loop, context packing, verification |
| Tool registry | Solid | 22 tools, permission system, scoped views |
| LLM integration | Solid | 4 providers, ModelTier routing, cost tracking stub |
| CLI (3-tier) | Solid | One-shot + REPL + TUI all working |
| API surface | Growing | 34 modules, ~40+ endpoints, SSE streaming |
| Frontend | Growing | SvelteKit 5, department routing, chat, flows UI |

### 3.3 The Gaps (Ranked by Impact)

#### Gap 1: Flow Engine Not Wired (Critical)

`flow-engine` exists (1,819 LOC) with DAG execution via petgraph, but it's
**not constructed or injected** into `rusvel-app`'s `AppState`.

**Current node types (5):**
- `CodeNode` -- JSON expression evaluation
- `ConditionNode` -- boolean branching
- `AgentNode` -- LLM invocation via AgentPort
- `BrowserTriggerNode` / `BrowserActionNode` -- CDP (partial)
- `ParallelEvaluateNode` -- concurrent agent runs

**Missing compared to n8n (40+ types):**
- Loop/iteration nodes
- Delay/wait/schedule nodes
- Tool-call nodes (dedicated)
- Map/reduce (batch operations)
- HTTP request nodes
- Transform/filter nodes
- Notification/message nodes
- Sub-flow invocation nodes
- Error handling branch nodes

**Missing execution features:**
- No partial reruns (n8n has full checkpoint + selective re-execution)
- No expression language (n8n uses `$json`, `$env`, expressions in every field)
- No node-level error policies (n8n: stop, continue, retry N times)
- No execution history/audit trail
- No flow versioning

#### Gap 2: Channel Ecosystem (Critical)

`rusvel-channel` is **112 lines** with a minimal trait:

```rust
pub trait ChannelPort: Send + Sync {
    fn channel_kind(&self) -> &'static str;
    async fn send_message(&self, session_id: &SessionId, payload: Value) -> Result<()>;
}
```

Only Telegram implemented. Send-only. No inbound. No rich messages.

**Missing compared to OpenClaw (10+ platforms):**
- No Discord, Slack, Email, WhatsApp, Signal, SMS
- No inbound message handling (webhook receivers)
- No rich message types (buttons, embeds, cards, threads)
- No media pipeline (images, audio, video)
- No delivery receipts or retry logic
- No channel routing (multi-user, multi-channel)
- No rate limiting or message batching
- No unified message format across channels

#### Gap 3: Self-Improvement Loop (High)

RUSVEL has `.claude/agents/`, `.claude/skills/`, `.claude/rules/`, `.claude/hooks/`
but lacks the continuous learning lifecycle that ECC provides.

**Missing:**
- Session persistence across conversations
- Pattern extraction from completed sessions
- Learned skill generation with provenance tracking
- Confidence scoring on extracted patterns
- Skill evolution (clustering similar patterns)
- Instinct promotion (project -> global scope)
- Hook-driven observation (capture every tool use for learning)
- Quality gate hooks (auto-format, type-check after edits)

#### Gap 4: Frontend Design System (Medium)

RUSVEL's frontend uses Tailwind 4 + SvelteKit 5 but has no systematic
component library.

**Missing compared to Supabase:**
- No design token system (consistent spacing, colors, typography)
- No variant system (CVA-like typed component variants)
- No higher-order patterns (FilterBar, CommandMenu, AssistantChat)
- No component documentation/playground
- No dark mode consistency

#### Gap 5: Observability & Cost Tracking (Medium)

No per-node, per-department, or per-agent cost/latency tracking.

**Missing:**
- LLM cost per call (token in/out * price)
- Tool cost per invocation
- Flow execution cost (sum of node costs)
- Department spend dashboards
- Agent efficiency metrics (cost per task completion)

#### Gap 6: Plugin System (Medium-Low)

No runtime plugin system. All extensions require workspace crate additions.

**Missing compared to Tauri:**
- No plugin trait with lifecycle hooks
- No runtime plugin registration
- No plugin isolation/sandboxing
- No plugin marketplace/discovery

---

## 4. Part I: Workflow Engine Patterns (from n8n)

### 4.1 n8n's Execution Model -- Deep Dive

n8n's workflow engine is the most mature open-source workflow execution system.
Understanding it deeply informs how to evolve RUSVEL's `flow-engine`.

#### 4.1.1 The Execution Stack Model

n8n processes workflows using a **node execution stack** -- a LIFO structure
where each entry contains the node to execute, its input data, and source
reference:

```
ExecutionStack = [
    { node: "HTTP Request", data: {...}, source: "Start" },
    { node: "Transform", data: {...}, source: "HTTP Request" },
    ...
]
```

**How it works:**
1. Initialize stack with start node (trigger node or manual start)
2. Pop top entry, establish execution context
3. Execute node with full context (credentials, parameters, expressions)
4. Push output connections onto stack (fan-out for multiple outputs)
5. Repeat until stack is empty or error occurs

**What RUSVEL can learn:**
RUSVEL currently uses petgraph's `Topo` iterator for topological traversal.
This is correct for simple DAGs but doesn't support:
- Mid-execution pause/resume (stack model makes this natural)
- Partial re-execution (just push the target node onto an empty stack)
- Dynamic fan-out (push N entries for N downstream nodes)

**Proposed adaptation:**

```rust
// In flow-engine, replace Topo with explicit stack
struct ExecutionStack {
    entries: Vec<StackEntry>,
}

struct StackEntry {
    node_idx: NodeIndex,
    input_data: HashMap<String, Value>,
    source_node: Option<NodeIndex>,
    attempt: u32,       // For retry tracking
}

impl FlowEngine {
    async fn execute_from_stack(
        &self,
        flow: &FlowDef,
        stack: &mut ExecutionStack,
        checkpoint: &mut FlowCheckpoint,
    ) -> Result<FlowResult> {
        while let Some(entry) = stack.pop() {
            let result = self.execute_node(flow, &entry).await;
            match result {
                Ok(outputs) => {
                    checkpoint.record_success(entry.node_idx, &outputs);
                    for downstream in flow.downstream_of(entry.node_idx) {
                        stack.push(StackEntry {
                            node_idx: downstream,
                            input_data: outputs.clone(),
                            source_node: Some(entry.node_idx),
                            attempt: 0,
                        });
                    }
                }
                Err(e) => {
                    checkpoint.record_error(entry.node_idx, &e);
                    match flow.error_policy(entry.node_idx) {
                        ErrorPolicy::Stop => return Err(e),
                        ErrorPolicy::Continue => continue,
                        ErrorPolicy::Retry(max) if entry.attempt < max => {
                            stack.push(StackEntry {
                                attempt: entry.attempt + 1,
                                ..entry
                            });
                        }
                        _ => return Err(e),
                    }
                }
            }
        }
        Ok(checkpoint.into_result())
    }
}
```

#### 4.1.2 The Directed Graph Model

n8n wraps workflows in a `DirectedGraph` for efficient traversal:

```typescript
class DirectedGraph {
    fromWorkflow(workflow): DirectedGraph
    toWorkflow(...): Workflow
    removeNode(node, { reconnectConnections })
    getParentNodes(node)
    getChildNodes(node)
}
```

**Key operations:**
- `getParentNodes()` -- walk backwards to find all ancestors (for partial execution)
- `removeNode()` with reconnection -- for pruning optional nodes
- Cycle detection -- prevent infinite loops in user-defined flows

**RUSVEL already has petgraph** which provides all these operations natively.
The gap is in *using* them for partial execution:

```rust
// Proposed: Partial execution support
impl FlowEngine {
    /// Execute only the target node and its ancestors
    async fn execute_partial(
        &self,
        flow: &FlowDef,
        target_node: NodeIndex,
        pin_data: &HashMap<NodeIndex, Value>, // Pre-computed results to skip
    ) -> Result<FlowResult> {
        let ancestors = self.graph_ancestors(flow, target_node);
        let mut stack = ExecutionStack::new();

        for node in ancestors.iter().rev() {
            if let Some(pinned) = pin_data.get(node) {
                // Skip -- use pinned data as output
                continue;
            }
            stack.push(StackEntry::new(*node));
        }
        stack.push(StackEntry::new(target_node));

        self.execute_from_stack(flow, &mut stack, &mut FlowCheckpoint::new()).await
    }
}
```

#### 4.1.3 Node Definition System

n8n nodes are self-describing objects with rich metadata:

```typescript
interface INodeType {
    description: INodeTypeDescription;  // Display name, group, I/O, credentials, params
    execute(this: IExecuteFunctions): Promise<INodeExecutionData[][]>;
    webhook?(): void;
    poll?(): void;
    trigger?(): { closeFunction, manualTriggerFunction };
    methods?: { loadOptions?, listSearch? };
}
```

**What makes this powerful:**
- **Self-describing:** Every node carries its own UI schema (parameters, display conditions)
- **Versioned:** `VersionedNodeType` supports multiple versions per node
- **Credential-aware:** Nodes declare which credentials they need
- **Expression-aware:** Every parameter can contain expressions (`{{ $json.field }}`)

**Proposed adaptation for RUSVEL:**

```rust
/// Extended node trait for flow-engine
pub trait FlowNode: Send + Sync {
    /// Unique type identifier (e.g., "http_request", "transform", "llm_call")
    fn node_type(&self) -> &'static str;

    /// Human-readable display name
    fn display_name(&self) -> &str;

    /// JSON Schema for node parameters (drives UI)
    fn parameter_schema(&self) -> Value;

    /// Input/output port definitions
    fn ports(&self) -> NodePorts;

    /// Execute the node with resolved parameters
    async fn execute(&self, ctx: &NodeContext) -> Result<NodeOutput>;

    /// Optional: validate parameters before execution
    fn validate(&self, params: &Value) -> Result<()> { Ok(()) }

    /// Optional: error behavior override
    fn error_policy(&self) -> ErrorPolicy { ErrorPolicy::Stop }
}

pub struct NodePorts {
    pub inputs: Vec<PortDef>,   // e.g., [PortDef::main("data")]
    pub outputs: Vec<PortDef>,  // e.g., [PortDef::main("result"), PortDef::error("error")]
}
```

#### 4.1.4 Missing Node Types to Implement

Based on n8n's node ecosystem, these are the highest-value additions:

**Tier 1 (Essential):**

| Node Type | Purpose | n8n Equivalent |
|-----------|---------|---------------|
| `LoopNode` | Iterate over array items | Loop Over Items |
| `DelayNode` | Wait N seconds/until time | Wait |
| `HttpRequestNode` | Make HTTP calls | HTTP Request |
| `ToolCallNode` | Invoke a registered tool | Function Item |
| `TransformNode` | Map/filter/reshape data | Set / Function |
| `SwitchNode` | Multi-way branching (>2 paths) | Switch |
| `MergeNode` | Combine outputs from parallel branches | Merge |
| `ErrorTriggerNode` | Handle upstream errors | Error Trigger |
| `SubFlowNode` | Execute another flow | Execute Workflow |

**Tier 2 (Valuable):**

| Node Type | Purpose | n8n Equivalent |
|-----------|---------|---------------|
| `CronTriggerNode` | Scheduled flow execution | Schedule Trigger |
| `WebhookTriggerNode` | HTTP webhook initiates flow | Webhook |
| `NotifyNode` | Send via ChannelPort | Slack/Email/Telegram |
| `ApprovalNode` | Human-in-the-loop gate | Manual Approval (n8n cloud) |
| `DatabaseNode` | Query/insert via ObjectStore | Postgres / MySQL nodes |
| `EmbeddingNode` | Generate embeddings | OpenAI Embeddings |
| `VectorSearchNode` | Similarity search | Vector Store |

**Proposed implementation for LoopNode:**

```rust
pub struct LoopNode;

#[async_trait]
impl FlowNode for LoopNode {
    fn node_type(&self) -> &'static str { "loop" }
    fn display_name(&self) -> &str { "Loop Over Items" }

    fn ports(&self) -> NodePorts {
        NodePorts {
            inputs: vec![PortDef::main("items")],
            outputs: vec![
                PortDef::main("item"),      // Per-iteration output
                PortDef::main("completed"), // After all iterations
            ],
        }
    }

    async fn execute(&self, ctx: &NodeContext) -> Result<NodeOutput> {
        let items = ctx.input("items")?.as_array()?;
        let mut results = Vec::new();

        for (index, item) in items.iter().enumerate() {
            ctx.set_variable("loop.index", json!(index));
            ctx.set_variable("loop.item", item.clone());

            // Execute the "item" output branch for each iteration
            let iteration_result = ctx.execute_branch("item").await?;
            results.push(iteration_result);
        }

        Ok(NodeOutput::new("completed", json!(results)))
    }
}
```

#### 4.1.5 Expression Language

n8n allows expressions in every parameter field:

```
{{ $json.name }}           -- Access current item's field
{{ $env.API_KEY }}         -- Environment variable
{{ $('Node Name').item }}  -- Reference another node's output
{{ DateTime.now() }}       -- Built-in functions
```

**RUSVEL's current state:** Code nodes use raw JSON `parameters` with no
interpolation. This means every dynamic value requires an agent node or
code node to compute it.

**Proposed solution:** Adopt a lightweight template engine. Options:

1. **Handlebars** (`handlebars` crate) -- familiar syntax, good Rust support
2. **MiniJinja** (`minijinja` crate) -- Jinja2-compatible, fast, well-maintained
3. **Custom** -- simple `{{ var.path }}` resolver

**Recommendation: MiniJinja** -- it's fast, well-maintained, and the Jinja2
syntax is familiar from Python/Ansible/dbt. Example:

```rust
use minijinja::{Environment, Value};

fn resolve_parameters(
    template_params: &Value,
    context: &FlowContext,
) -> Result<Value> {
    let env = Environment::new();
    // Recursively resolve all string values in the parameters
    resolve_value(&env, template_params, context)
}

// Usage in a node's parameters:
// {
//   "url": "https://api.example.com/users/{{ inputs.user_id }}",
//   "headers": { "Authorization": "Bearer {{ env.API_TOKEN }}" }
// }
```

#### 4.1.6 Error Handling Patterns

n8n has a rich error hierarchy:

```
ApplicationError
  +-- NodeOperationError     (bad credentials, invalid params)
  +-- NodeApiError           (external API returned error)
  +-- ExpressionError        (template evaluation failed)
  +-- ExecutionCancelledError (user/system cancelled)
```

Each error carries:
- **Node context** -- which node failed, item index, run index
- **User-facing message** -- sanitized for display
- **Functionality tag** -- categorizes error for UI display

**Proposed adaptation:**

```rust
#[derive(Debug, thiserror::Error)]
pub enum FlowError {
    #[error("Node '{node_id}' failed: {message}")]
    NodeExecution {
        node_id: String,
        node_type: String,
        message: String,
        item_index: Option<usize>,
        #[source] source: anyhow::Error,
    },

    #[error("Expression error in '{node_id}': {message}")]
    Expression {
        node_id: String,
        expression: String,
        message: String,
    },

    #[error("Flow cancelled: {reason}")]
    Cancelled { reason: String },

    #[error("Flow timed out after {elapsed:?}")]
    Timeout { elapsed: Duration },
}
```

#### 4.1.7 Credential Management

n8n encrypts credentials at rest (AES-256) and decrypts per-execution:

```typescript
class Credentials {
    getData(): T           // Decrypt on demand
    setData(data: T): void // Encrypt before storage
}
```

**RUSVEL's current approach:** `rusvel-auth` is in-memory from env vars.

**What to adopt:**
- Encrypt stored credentials (API keys, OAuth tokens) at rest in SQLite
- Decrypt per-execution with a master key (from env or keyring)
- Node-level credential declarations (which credentials a node needs)
- Credential testing (verify before using in production flows)

#### 4.1.8 Execution Context & Audit Trail

n8n creates an immutable execution context at start:

```typescript
type ExecutionContext = {
    version: 1;
    establishedAt: number;
    source: 'manual' | 'webhook' | 'trigger' | ...;
    triggerNode?: { name, type };
    parentExecutionId?: string;  // Sub-workflow
};
```

**Proposed adaptation:**

```rust
pub struct FlowExecutionContext {
    pub execution_id: Uuid,
    pub flow_id: String,
    pub flow_version: u32,
    pub started_at: DateTime<Utc>,
    pub source: ExecutionSource,
    pub trigger_node: Option<String>,
    pub parent_execution_id: Option<Uuid>,
    pub session_id: SessionId,
    pub department: String,
}

pub enum ExecutionSource {
    Manual,
    Webhook { webhook_id: String },
    Schedule { cron_id: String },
    SubFlow { parent_id: Uuid },
    Api,
}
```

#### 4.1.9 Event System

n8n emits typed events throughout execution:

```
EventMessageWorkflow  -- flow start, complete, error
EventMessageNode      -- node start, complete, error
EventMessageExecution -- execution lifecycle
EventMessageAiNode    -- AI-specific events
```

**RUSVEL already has `rusvel-event`** with `Event { kind: String, ... }`.
The adaptation is defining flow-specific event kinds:

```rust
// Event kinds for flow execution
const FLOW_STARTED: &str       = "flow.execution.started";
const FLOW_COMPLETED: &str     = "flow.execution.completed";
const FLOW_FAILED: &str        = "flow.execution.failed";
const FLOW_NODE_STARTED: &str  = "flow.node.started";
const FLOW_NODE_COMPLETED: &str = "flow.node.completed";
const FLOW_NODE_FAILED: &str   = "flow.node.failed";
const FLOW_NODE_SKIPPED: &str  = "flow.node.skipped";
const FLOW_CHECKPOINT: &str    = "flow.checkpoint.saved";
```

#### 4.1.10 Design System Patterns

n8n has 97 Vue components with a design token system:

```css
--color--primary--shade-1
--color--primary
--color--primary--tint-1
--spacing--xs: 12px
--font-size--md: 16px
```

**Component architecture:**
- Base layer: N8nButton, N8nInput, N8nSelect, N8nCheckbox
- Composition: N8nCollapsiblePanel, N8nAccordion, N8nCard
- Specialized: AskAssistantButton, CodeDiff

**Frontend state:** Pinia stores composed from specialized composables:

```typescript
const workflowDocument = useWorkflowDocumentStore(id);
// Composed of:
// - useWorkflowDocumentActive()
// - useWorkflowDocumentNodes()
// - useWorkflowDocumentConnections()
// - useWorkflowDocumentPinData()
```

**What RUSVEL should adopt:**
- Design tokens as CSS custom properties (works with Tailwind 4)
- Composable Svelte stores (equivalent of Pinia composables)
- Component variant system (CVA or similar for Svelte)

---

## 5. Part II: Multi-Channel Architecture (from OpenClaw)

### 5.1 The Channel Plugin Contract

OpenClaw's most valuable pattern is the **ChannelPlugin** interface. Every
channel (WhatsApp, Telegram, Discord, Slack, Signal, iMessage, LINE, IRC)
implements this unified contract:

```typescript
type ChannelPlugin = {
    id: ChannelId;
    meta: ChannelMeta;
    capabilities: ChannelCapabilities;
    config: ChannelConfigAdapter;
    outbound?: ChannelOutboundAdapter;
    gateway?: ChannelGatewayAdapter;
    security?: ChannelSecurityAdapter;
    groups?: ChannelGroupAdapter;
    threading?: ChannelThreadingAdapter;
    messaging?: ChannelMessagingAdapter;
    actions?: ChannelMessageActionAdapter;
    directory?: ChannelDirectoryAdapter;
    heartbeat?: ChannelHeartbeatAdapter;
    agentTools?: ChannelAgentToolFactory;
    lifecycle?: ChannelLifecycleAdapter;
};
```

**Key insight:** The interface is **adapter-based** -- each channel only
implements the adapters it supports. Discord has `threading` and `groups`;
SMS doesn't. The system gracefully degrades.

### 5.2 Proposed Rust Adaptation

Transform OpenClaw's TypeScript interface into RUSVEL's port trait system:

```rust
/// Core channel trait (replaces current minimal ChannelPort)
#[async_trait]
pub trait ChannelPort: Send + Sync {
    /// Channel identifier (e.g., "telegram", "discord", "slack")
    fn channel_id(&self) -> &str;

    /// Human-readable name
    fn display_name(&self) -> &str;

    /// What this channel can do
    fn capabilities(&self) -> ChannelCapabilities;

    /// Send a message (required)
    async fn send_message(&self, target: &ChannelTarget, payload: &MessagePayload) -> Result<DeliveryReceipt>;

    /// Send rich content (optional, falls back to text)
    async fn send_rich(&self, target: &ChannelTarget, payload: &RichPayload) -> Result<DeliveryReceipt> {
        // Default: extract text and send as plain message
        self.send_message(target, &payload.to_text()).await
    }

    /// Handle inbound message (optional)
    async fn handle_inbound(&self, raw: Value) -> Result<InboundMessage> {
        Err(anyhow!("Inbound not supported for {}", self.channel_id()))
    }
}

/// What a channel supports
#[derive(Debug, Clone)]
pub struct ChannelCapabilities {
    pub inbound: bool,
    pub rich_text: bool,       // Markdown, HTML
    pub buttons: bool,         // Interactive buttons
    pub embeds: bool,          // Rich embeds/cards
    pub threads: bool,         // Thread replies
    pub reactions: bool,       // Emoji reactions
    pub media: MediaCapabilities,
    pub max_message_length: usize,
}

#[derive(Debug, Clone)]
pub struct MediaCapabilities {
    pub images: bool,
    pub audio: bool,
    pub video: bool,
    pub files: bool,
    pub max_file_size: usize,
}

/// Unified message target
pub struct ChannelTarget {
    pub channel_id: String,    // Which channel adapter
    pub recipient: String,     // User/group/channel ID
    pub thread_id: Option<String>,
}

/// Outbound message payload
pub struct MessagePayload {
    pub text: String,
    pub format: MessageFormat,  // Plain, Markdown, Html
}

/// Rich outbound payload
pub struct RichPayload {
    pub text: String,
    pub embeds: Vec<Embed>,
    pub buttons: Vec<Button>,
    pub media: Vec<MediaAttachment>,
}

/// Inbound message (normalized from any channel)
pub struct InboundMessage {
    pub channel_id: String,
    pub sender_id: String,
    pub sender_name: Option<String>,
    pub text: Option<String>,
    pub media: Vec<MediaAttachment>,
    pub thread_id: Option<String>,
    pub reply_to_id: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub raw: Value,  // Original platform-specific payload
}

/// Delivery confirmation
pub struct DeliveryReceipt {
    pub message_id: String,
    pub timestamp: DateTime<Utc>,
    pub status: DeliveryStatus,
}

pub enum DeliveryStatus {
    Sent,
    Delivered,
    Read,
    Failed(String),
}
```

### 5.3 Channel Router

OpenClaw routes messages through a hierarchical binding system:

1. Peer-specific binding (exact user/group match)
2. Guild + role binding (Discord roles)
3. Account binding (per-channel default)
4. Global default

**Proposed adaptation:**

```rust
pub struct ChannelRouter {
    channels: HashMap<String, Box<dyn ChannelPort>>,
    routes: Vec<ChannelRoute>,
}

pub struct ChannelRoute {
    pub pattern: RoutePattern,
    pub channel_id: String,
    pub priority: u32,
}

pub enum RoutePattern {
    /// Match a specific department
    Department(String),
    /// Match an event kind pattern
    EventKind(String),
    /// Match all
    Default,
}

impl ChannelRouter {
    /// Route a notification to the appropriate channel(s)
    pub async fn notify(
        &self,
        department: &str,
        event_kind: &str,
        payload: &MessagePayload,
    ) -> Result<Vec<DeliveryReceipt>> {
        let targets = self.resolve_targets(department, event_kind);
        let mut receipts = Vec::new();
        for target in targets {
            let channel = self.channels.get(&target.channel_id)
                .ok_or_else(|| anyhow!("Unknown channel: {}", target.channel_id))?;
            receipts.push(channel.send_message(&target, payload).await?);
        }
        Ok(receipts)
    }
}
```

### 5.4 Priority Channel Implementations

Based on OpenClaw's proven adapters, implement in this order:

**Phase 1: Discord** (highest value for developer communities)
- Rich embeds for department status updates
- Thread-per-department for organized conversations
- Slash commands mapped to RUSVEL CLI commands
- Reaction-based approvals
- Webhook receiver for inbound

**Phase 2: Slack** (highest value for teams/enterprises)
- Thread routing per department
- Interactive messages (buttons, selects)
- App mentions trigger agent responses
- Block Kit for rich formatting

**Phase 3: Email** (universal, async communication)
- SMTP outbound with retry queue
- IMAP/webhook inbound
- HTML templates per department
- Attachment support for reports

**Phase 4: Webhook** (custom integrations)
- Generic HTTP POST outbound
- Configurable payload templates
- Webhook signature verification
- Retry with exponential backoff

### 5.5 Message Queuing & Batching

OpenClaw debounces rapid messages to prevent spam:

```typescript
// Coalesce messages within debounceMs window
const debounceMs = channel.config.debounceMs ?? 500;
```

**Proposed adaptation:**

```rust
pub struct MessageQueue {
    pending: Mutex<HashMap<String, Vec<QueuedMessage>>>,
    flush_interval: Duration,
}

struct QueuedMessage {
    payload: MessagePayload,
    queued_at: Instant,
}

impl MessageQueue {
    /// Queue a message. Flushes when:
    /// 1. Batch reaches max_batch_size
    /// 2. Oldest message exceeds flush_interval
    pub async fn enqueue(&self, target: &ChannelTarget, payload: MessagePayload) {
        // ...
    }

    /// Flush pending messages, combining where possible
    async fn flush(&self, channel: &dyn ChannelPort, target: &ChannelTarget) {
        let messages = self.pending.lock().remove(&target.key());
        if let Some(msgs) = messages {
            let combined = self.combine_messages(msgs);
            channel.send_message(target, &combined).await.ok();
        }
    }
}
```

### 5.6 Inbound Webhook Handler

OpenClaw normalizes inbound messages from all platforms. For RUSVEL, this
integrates with `rusvel-webhook`:

```rust
// In rusvel-api, add channel webhook routes
async fn handle_channel_webhook(
    State(state): State<AppState>,
    Path(channel_id): Path<String>,
    body: Bytes,
) -> Result<impl IntoResponse> {
    let channel = state.channel_router.get(&channel_id)?;

    // Verify webhook signature (platform-specific)
    channel.verify_webhook(&body)?;

    // Normalize to InboundMessage
    let message = channel.handle_inbound(serde_json::from_slice(&body)?).await?;

    // Route to appropriate department/agent
    let route = state.channel_router.resolve_inbound(&message);
    state.event_bus.emit(Event::new(
        "channel.message.received",
        json!({ "channel": channel_id, "message": message }),
    )).await?;

    Ok(StatusCode::OK)
}
```

### 5.7 Media Pipeline

OpenClaw handles media across channels with:
- MIME detection (`detectMime`)
- Image resizing (`resizeToJpeg`)
- Audio transcription (`transcribeAudioFile`)
- Video thumbnail extraction

**Proposed minimal adaptation:**

```rust
pub struct MediaPipeline {
    embed_port: Option<Arc<dyn EmbedPort>>,
    max_file_size: usize,
}

impl MediaPipeline {
    /// Process media for a target channel
    pub async fn prepare(
        &self,
        media: &MediaAttachment,
        target_capabilities: &MediaCapabilities,
    ) -> Result<ProcessedMedia> {
        // Resize images if needed
        // Convert audio formats if needed
        // Generate thumbnails for video
        // Respect max_file_size
        todo!()
    }
}
```

---

## 6. Part III: AI Harness & Self-Improvement (from Everything Claude Code)

### 6.1 The ECC Architecture

Everything Claude Code (ECC) is a **comprehensive agent harness** with:
- 28 specialized agents
- 59 slash commands
- 39 rules files
- 11+ skills
- Comprehensive hook system
- Continuous learning pipeline

The key insight is that ECC treats the Claude Code harness itself as a
**software system that can be optimized, extended, and self-improved**.

### 6.2 Skills System

ECC skills are structured knowledge packages with clear trigger conditions:

```markdown
---
name: article-writing
description: Long-form content creation with distinctive voice
origin: ECC
---

# Article Writing

## When to Activate
- User requests blog post, guide, newsletter, or long-form content
- Content involves explaining technical concepts

## Core Rules
- Capture voice from examples before writing
- Structure: hook -> context -> insight -> action -> close
...

## Quality Gate
- [ ] Voice matches provided examples
- [ ] Technical accuracy verified
- [ ] Call-to-action present
```

**What RUSVEL should adopt:**

RUSVEL already has `.claude/skills/` with 6 skills. The gap is:

1. **Learned skills** -- automatically generated from successful sessions
2. **Provenance tracking** -- where did this skill come from? How confident?
3. **Skill health dashboard** -- which skills are used, which are stale?

**Proposed additions to `.claude/skills/`:**

```
.claude/skills/
  learned/                    # Auto-generated from sessions
    rust-borrow-patterns/
      SKILL.md
      .provenance.json       # { source, confidence, created_at }
    flow-node-creation/
      SKILL.md
      .provenance.json
  imported/                   # From external sources
    ...
```

### 6.3 Hooks System -- The Self-Improvement Engine

ECC's hooks are the most sophisticated part. They fire at tool lifecycle points:

| Trigger | When | Can Block? | Key Use |
|---------|------|-----------|---------|
| PreToolUse | Before any tool | YES (exit 2) | Validation, safety |
| PostToolUse | After tool executes | NO | Auto-format, quality gates |
| Stop | After response | NO | Session evaluation, learning |
| SessionStart | New session | NO | Load context, detect tools |
| PreCompact | Before context compaction | NO | Save state |

**Key hooks RUSVEL should implement:**

#### 6.3.1 Pre-Commit Quality Gate

```bash
# hooks/pre-bash-commit-quality.sh
# Block git commit if:
# - Staged files contain console.log/debugger
# - Commit message doesn't follow conventional commits
# - Staged files contain hardcoded secrets

input=$(cat)
command=$(echo "$input" | jq -r '.tool_input.command // ""')

if [[ "$command" == git\ commit* ]]; then
    # Check for secrets
    if git diff --staged | grep -iE '(api_key|secret|password|token)\s*=\s*["\x27]'; then
        echo "BLOCKED: Possible secrets in staged changes" >&2
        exit 2
    fi
fi
echo "$input"
```

#### 6.3.2 Post-Edit Auto-Format

```bash
# hooks/post-edit-format.sh
# Auto-run rustfmt after editing .rs files

input=$(cat)
file_path=$(echo "$input" | jq -r '.tool_input.file_path // ""')

if [[ "$file_path" == *.rs ]]; then
    rustfmt "$file_path" 2>/dev/null
fi
echo "$input"
```

#### 6.3.3 Session Persistence

```bash
# hooks/session-end.sh
# Save session state for resumption

input=$(cat)
transcript=$(echo "$input" | jq -r '.session.transcript_path // ""')

if [[ -n "$transcript" ]]; then
    # Extract key decisions and state
    node scripts/extract-session-state.js "$transcript" > \
        ~/.claude/session-data/$(date +%Y-%m-%d)-session.md
fi
echo "$input"
```

#### 6.3.4 Observation Hook (Continuous Learning)

```bash
# hooks/observe.sh (async, runs on every tool use)
# Captures tool use patterns for later learning extraction

input=$(cat)
tool_name=$(echo "$input" | jq -r '.tool_name // ""')
file_path=$(echo "$input" | jq -r '.tool_input.file_path // ""')

# Append to observation log (lightweight, async)
echo "{\"ts\":\"$(date -u +%FT%TZ)\",\"tool\":\"$tool_name\",\"file\":\"$file_path\"}" \
    >> ~/.claude/observations/$(date +%Y-%m-%d).jsonl

echo "$input"
```

### 6.4 Continuous Learning Pipeline

ECC's most unique feature is the **learn -> evolve -> promote** lifecycle:

```
Session
  |
  v
Observation hooks capture tool uses (async, non-blocking)
  |
  v
/learn command extracts patterns from session
  |
  v
Learned skill created: ~/.claude/skills/learned/<pattern>/SKILL.md
  + .provenance.json { source, confidence: 0.3, created_at }
  |
  v
/evolve clusters similar instincts into evolved skills
  |
  v
/promote moves project-scoped patterns to global scope
  |
  v
Confidence increases with repeated validation (0.3 -> 0.7 -> 1.0)
```

**What RUSVEL should implement:**

1. **Session state files** -- structured markdown capturing what worked, what didn't
2. **Pattern extraction** -- `/learn` command for RUSVEL-specific patterns
3. **Provenance tracking** -- every learned skill knows its origin
4. **Confidence scoring** -- 0.0-1.0, increases with validation

**Proposed `.claude/commands/learn.md`:**

```markdown
---
description: Extract reusable patterns from current session
---

# Learn from Session

Review the current session and extract any non-obvious patterns:

1. **Error resolution patterns** -- specific error -> root cause -> fix
2. **Architecture decisions** -- why a certain approach was chosen
3. **Tool combinations** -- non-obvious tool sequences that solved problems
4. **Performance insights** -- what was slow, what optimized it

For each pattern:
- Create `.claude/skills/learned/<pattern-name>/SKILL.md`
- Include `.provenance.json` with confidence score
- Format with: Problem, Solution, When to Use, Example

Skip trivial patterns (typos, simple syntax). Focus on patterns that would
save 5+ minutes if encountered again.
```

### 6.5 Agent Orchestration Patterns

ECC defines 28 agents with clear delegation rules:

**Proactive invocation (no user prompt needed):**
- Complex features -> `planner` (opus model)
- Code written -> `code-reviewer` (sonnet model)
- Bug fix -> `tdd-guide` (sonnet model)
- Architecture decision -> `architect` (opus model)
- Security-sensitive code -> `security-reviewer` (sonnet model)

**RUSVEL already has agents** (`@researcher`, `@arch-reviewer`, `@dept-auditor`).
What to add:

```markdown
# .claude/agents/tdd-guide.md
---
name: tdd-guide
description: Test-driven development specialist for RUSVEL
tools: ["Read", "Write", "Edit", "Bash", "Grep", "Glob"]
model: sonnet
---

You are a TDD specialist for the RUSVEL project.

## Workflow
1. RED: Write failing test first
2. GREEN: Minimal implementation to pass
3. IMPROVE: Refactor while tests stay green

## RUSVEL-Specific Rules
- Engine tests use mock ports (see forge-engine/src/tests/)
- API tests use test helpers in rusvel-api/tests/
- Always run `cargo test -p <crate>` after changes
- Integration tests over mocks (CLAUDE.md rule)
- Target 80%+ coverage for new code
```

### 6.6 Rules System -- Cross-Cutting Guidelines

ECC has 39 rules files organized by language and concern. RUSVEL already has
path-based rules in `.claude/rules/`. What to add:

**Common rules (always apply):**

```markdown
# .claude/rules/common-testing.md
---
description: Testing standards for all RUSVEL crates
globs: ["crates/**/*.rs"]
---

## Testing Standards
- Minimum 80% coverage for new code
- Engine tests: mock all ports, test domain logic in isolation
- API tests: use test helpers, test HTTP status + response shape
- Integration tests preferred over mocks for database operations
- Always test error paths, not just happy path

## TDD Workflow
1. Write failing test
2. Run `cargo test -p <crate>` -- confirm it fails
3. Implement minimal code to pass
4. Run test again -- confirm it passes
5. Refactor if needed

## Edge Cases to Test
- Empty inputs (empty string, empty vec, None)
- Invalid IDs (non-existent department, bad UUID)
- Concurrent access (where applicable)
- Large payloads (1000+ items)
```

### 6.7 Model Routing Strategy

ECC assigns models by agent responsibility:

| Model | Use Case | Cost |
|-------|----------|------|
| **Opus** | Architecture, planning, deep reasoning | High |
| **Sonnet** | Main development, code review, TDD | Medium |
| **Haiku** | Lightweight tasks, formatting, quick checks | Low (3x savings) |

**RUSVEL already has ModelTier routing** in `rusvel-llm`. The connection point
is mapping this to agent definitions:

```rust
// In rusvel-agent, use model tier based on task complexity
pub enum AgentTier {
    Lightweight, // Haiku -- formatting, simple lookups
    Standard,    // Sonnet -- code generation, review
    Deep,        // Opus -- architecture, planning, complex reasoning
}

impl AgentTier {
    pub fn to_model_tier(&self) -> ModelTier {
        match self {
            Self::Lightweight => ModelTier::Haiku,
            Self::Standard => ModelTier::Sonnet,
            Self::Deep => ModelTier::Opus,
        }
    }
}
```

---

## 7. Part IV: Database & Realtime Patterns (from Supabase)

### 7.1 Realtime Subscriptions

Supabase provides WebSocket-based realtime with three patterns:

1. **Broadcast** -- fire-and-forget messages to subscribers
2. **Presence** -- track who's connected (join/leave events)
3. **Postgres Changes** -- CDC (Change Data Capture) from database

**What RUSVEL should adopt:**

RUSVEL already has SSE streaming for department events. The gap is
**push-based subscriptions** for the frontend:

```rust
// Proposed: WebSocket upgrade for real-time department events
async fn ws_department_events(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
    Path(dept): Path<String>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_dept_ws(socket, state, dept))
}

async fn handle_dept_ws(
    mut socket: WebSocket,
    state: AppState,
    dept: String,
) {
    let mut rx = state.event_bus.subscribe(&dept);

    while let Some(event) = rx.recv().await {
        let msg = serde_json::to_string(&event).unwrap();
        if socket.send(Message::Text(msg)).await.is_err() {
            break;
        }
    }
}
```

### 7.2 Component Library Patterns

Supabase uses **CVA (Class Variance Authority)** for type-safe component variants:

```typescript
const buttonVariants = cva('base-classes', {
    variants: {
        type: { primary: 'bg-brand-400', danger: 'bg-destructive-300' },
        size: { tiny: 'px-2.5 py-1', small: 'px-3 py-2' },
    }
});
```

**Svelte 5 equivalent for RUSVEL:**

```svelte
<!-- lib/components/Button.svelte -->
<script lang="ts">
    import { tv } from 'tailwind-variants'; // Svelte-compatible CVA alternative

    const button = tv({
        base: 'inline-flex items-center justify-center rounded-md font-medium transition-colors',
        variants: {
            variant: {
                primary: 'bg-blue-600 text-white hover:bg-blue-700',
                secondary: 'bg-gray-100 text-gray-900 hover:bg-gray-200',
                danger: 'bg-red-600 text-white hover:bg-red-700',
                ghost: 'hover:bg-gray-100',
            },
            size: {
                sm: 'h-8 px-3 text-sm',
                md: 'h-10 px-4 text-sm',
                lg: 'h-12 px-6 text-base',
            },
        },
        defaultVariants: {
            variant: 'primary',
            size: 'md',
        },
    });

    let { variant, size, children, ...rest }: {
        variant?: 'primary' | 'secondary' | 'danger' | 'ghost';
        size?: 'sm' | 'md' | 'lg';
        children: any;
        [key: string]: any;
    } = $props();
</script>

<button class={button({ variant, size })} {...rest}>
    {@render children()}
</button>
```

### 7.3 Design Token System

```css
/* frontend/src/app.css -- Design tokens */
:root {
    /* Brand */
    --color-brand-50: theme('colors.blue.50');
    --color-brand-500: theme('colors.blue.500');
    --color-brand-600: theme('colors.blue.600');

    /* Semantic */
    --color-bg-primary: theme('colors.white');
    --color-bg-secondary: theme('colors.gray.50');
    --color-text-primary: theme('colors.gray.900');
    --color-text-secondary: theme('colors.gray.500');
    --color-border: theme('colors.gray.200');

    /* Spacing scale */
    --space-xs: 0.25rem;
    --space-sm: 0.5rem;
    --space-md: 1rem;
    --space-lg: 1.5rem;
    --space-xl: 2rem;

    /* Typography */
    --text-xs: 0.75rem;
    --text-sm: 0.875rem;
    --text-base: 1rem;
    --text-lg: 1.125rem;
}

.dark {
    --color-bg-primary: theme('colors.gray.900');
    --color-bg-secondary: theme('colors.gray.800');
    --color-text-primary: theme('colors.gray.100');
    --color-text-secondary: theme('colors.gray.400');
    --color-border: theme('colors.gray.700');
}
```

### 7.4 Vector Search Patterns

Supabase uses pgvector with HNSW indexing:

```sql
CREATE INDEX ON documents USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
```

**RUSVEL uses LanceDB** (via `rusvel-vector`), which already has similar
capabilities. What to adopt from Supabase:

1. **Similarity threshold tuning** -- Supabase uses 0.78 as default threshold
2. **Hybrid search** -- combine vector similarity with metadata filters
3. **Embedding caching** -- cache embeddings for repeated queries

### 7.5 SQL Execution Safety

Supabase validates queries before execution:

```typescript
// Preflight EXPLAIN check
const cost = await explain(sql);
if (cost >= 200_000) throw new Error('Query too expensive');

// Size limit
if (new Blob([sql]).size > 0.98 * MB) throw new Error('Too large');
```

**RUSVEL's RusvelBase** should adopt:
- Query cost estimation before execution
- Size limits on user-supplied SQL
- Statement timeouts
- Read-only mode for the database browser

---

## 8. Part V: Rust Plugin & IPC Architecture (from Tauri)

### 8.1 Plugin Trait Pattern

Tauri's plugin system is the best reference for Rust plugin architecture:

```rust
pub trait Plugin<R: Runtime>: Send {
    fn name(&self) -> &'static str;
    fn initialize(&mut self, app: &AppHandle<R>, config: JsonValue) -> Result<()>;
    fn on_page_load(&mut self, webview: &Webview<R>, payload: &PageLoadPayload);
    fn extend_api(&mut self, invoke: Invoke<R>) -> bool;
}
```

**Proposed RUSVEL adaptation:**

```rust
/// Plugin trait for runtime extensions
#[async_trait]
pub trait RusvelPlugin: Send + Sync {
    /// Unique plugin identifier
    fn id(&self) -> &str;

    /// Human-readable name
    fn name(&self) -> &str;

    /// Plugin version
    fn version(&self) -> &str;

    /// Initialize with app context
    async fn initialize(&self, ctx: &PluginContext) -> Result<()>;

    /// Register tools this plugin provides
    fn tools(&self) -> Vec<ToolDef> { vec![] }

    /// Register flow node types this plugin provides
    fn flow_nodes(&self) -> Vec<Box<dyn FlowNode>> { vec![] }

    /// Register channel adapters this plugin provides
    fn channels(&self) -> Vec<Box<dyn ChannelPort>> { vec![] }

    /// Handle plugin-specific API routes
    fn routes(&self) -> Vec<axum::Router> { vec![] }

    /// Cleanup on shutdown
    async fn shutdown(&self) -> Result<()> { Ok(()) }
}

pub struct PluginContext {
    pub config: Value,
    pub db: Arc<dyn ObjectStore>,
    pub event_bus: Arc<dyn EventPort>,
    pub tool_registry: Arc<ToolRegistry>,
}
```

### 8.2 State Management Pattern

Tauri uses TypeId-indexed, `Pin<Box<T>>`, immutable state:

```rust
pub struct StateManager {
    map: Mutex<HashMap<TypeId, Pin<Box<dyn Any + Sync + Send>>>>,
}
```

**Key properties:**
- One instance per type (prevents confusion)
- Immovable (prevents dangling references)
- Immutable references only (interior mutability via Arc<Mutex<T>>)

**RUSVEL's current approach:** `AppState` struct with named fields.
This is fine for a known set of dependencies but doesn't support
dynamic plugin state.

**Proposed hybrid:**

```rust
/// AppState keeps known dependencies as named fields
/// AND supports dynamic plugin state via TypeId map
pub struct AppState {
    // Known dependencies
    pub db: Arc<dyn ObjectStore>,
    pub llm: Arc<dyn LlmPort>,
    pub agent: Arc<dyn AgentPort>,
    // ... existing fields ...

    // Dynamic plugin state
    plugin_state: StateManager,
}

impl AppState {
    pub fn plugin<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.plugin_state.get::<T>()
    }

    pub fn manage_plugin<T: Send + Sync + 'static>(&self, state: T) {
        self.plugin_state.set(state);
    }
}
```

### 8.3 Command Macro Pattern

Tauri's `#[tauri::command]` macro auto-generates deserialization and
error handling:

```rust
#[tauri::command]
async fn my_command(
    name: String,
    state: State<'_, MyState>,
) -> Result<String> {
    Ok(format!("Hello, {}", name))
}
```

**RUSVEL equivalent:** Axum's extractors already provide similar ergonomics:

```rust
async fn my_handler(
    State(state): State<AppState>,
    Path(dept): Path<String>,
    Json(body): Json<MyRequest>,
) -> Result<Json<MyResponse>> {
    // ...
}
```

No need to add a custom macro -- Axum's type system already handles this.
But for **plugin-provided handlers**, a registration pattern is useful:

```rust
impl RusvelPlugin for MyPlugin {
    fn routes(&self) -> Vec<axum::Router> {
        vec![
            axum::Router::new()
                .route("/api/plugin/my-plugin/action", post(my_handler))
        ]
    }
}
```

### 8.4 Event System

Tauri's event system uses typed targets:

```rust
pub enum EventTarget {
    Any,
    App,
    Window { label: String },
    Webview { label: String },
}
```

**RUSVEL already has `rusvel-event`** with string-based event kinds.
What to add from Tauri:

```rust
/// Scoped event emission
pub enum EventScope {
    /// All listeners
    Global,
    /// Only listeners in this department
    Department(String),
    /// Only listeners in this session
    Session(SessionId),
    /// Only listeners on this specific entity
    Entity { kind: String, id: String },
}

// Usage:
event_bus.emit_scoped(
    EventScope::Department("forge".into()),
    Event::new("forge.mission.completed", payload),
).await?;
```

### 8.5 Desktop Distribution Path

If RUSVEL ever needs desktop distribution, Tauri provides the blueprint:

1. **System webview** -- Replace rust-embed SPA with Tauri's WRY (system WebView)
2. **Native menus** -- System tray, menu bar, context menus
3. **Auto-updater** -- Built-in update mechanism
4. **Cross-platform bundling** -- .dmg, .exe, .AppImage from one build

**Current RUSVEL approach (rust-embed + Axum) is simpler and sufficient
for now.** Tauri becomes relevant if native desktop features are needed
(system tray, native notifications, file system access without HTTP).

---

## 9. Part VI: Claude Code Integration Patterns

### 9.1 From the Claude Code Reference Repo

The `repos/claude-code/` repo shows how Claude Code itself works as a product.
Key patterns relevant to RUSVEL's `--mcp` mode:

1. **Plugin architecture** -- extensible command system
2. **Environment integration** -- terminal, IDE, GitHub
3. **CHANGELOG discipline** -- clear version tracking

### 9.2 RUSVEL's MCP Server Enhancement

RUSVEL already has `--mcp` mode. Enhancements from studying Claude Code:

```rust
// Current: Basic MCP server
// Enhanced: Department-aware tool scoping per MCP session

impl RusvelMcp {
    fn tools_for_context(&self, context: &McpContext) -> Vec<ToolDef> {
        match context.scope {
            McpScope::Global => self.all_tools(),
            McpScope::Department(ref dept) => {
                self.tool_registry.scoped(dept).list()
            }
        }
    }
}
```

---

## 10. Cross-Cutting Concerns

### 10.1 Cost Tracking (from n8n + ECC)

Both n8n and ECC track costs. RUSVEL needs this across all ports:

```rust
/// Cost event emitted after every billable operation
pub struct CostEvent {
    pub department: String,
    pub operation: CostOperation,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub cost_usd: f64,
    pub model: String,
    pub timestamp: DateTime<Utc>,
    pub session_id: SessionId,
    pub context: CostContext,
}

pub enum CostOperation {
    LlmCall,
    EmbeddingGeneration,
    ToolExecution,
    FlowNodeExecution,
    VectorSearch,
}

pub enum CostContext {
    Chat { message_id: String },
    Flow { execution_id: Uuid, node_id: String },
    Job { job_id: String },
    Agent { run_id: String },
}

/// Track in MetricStore
#[async_trait]
pub trait CostTracker: Send + Sync {
    async fn record_cost(&self, event: CostEvent) -> Result<()>;
    async fn department_spend(&self, dept: &str, since: DateTime<Utc>) -> Result<f64>;
    async fn total_spend(&self, since: DateTime<Utc>) -> Result<f64>;
}
```

### 10.2 Observability (from n8n + Supabase)

Per-node execution metrics for flows:

```rust
pub struct NodeMetrics {
    pub node_id: String,
    pub node_type: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub duration: Duration,
    pub status: NodeStatus,
    pub cost: Option<CostEvent>,
    pub input_size: usize,
    pub output_size: usize,
    pub error: Option<String>,
}

// Aggregate for flow execution
pub struct FlowExecutionMetrics {
    pub execution_id: Uuid,
    pub total_duration: Duration,
    pub total_cost_usd: f64,
    pub nodes_executed: usize,
    pub nodes_failed: usize,
    pub nodes_skipped: usize,
    pub per_node: Vec<NodeMetrics>,
}
```

### 10.3 Testing Strategy (from ECC)

ECC mandates TDD with 80% coverage. Adopt for RUSVEL:

```
Test Pyramid for RUSVEL:
                    /\
                   /E2E\        Playwright visual tests
                  /------\
                 /  API   \     rusvel-api integration tests
                /----------\
               / Integration \  Engine tests with mock ports
              /--------------\
             /    Unit Tests   \ Core logic, pure functions
            /------------------\
```

**Coverage targets by crate type:**
- `rusvel-core`: 90% (pure domain logic)
- `*-engine`: 80% (domain + orchestration)
- `rusvel-api`: 70% (route handlers)
- `dept-*`: 60% (thin wrappers, mostly delegation)
- `rusvel-db`: 80% (integration tests with real SQLite)

### 10.4 Configuration Evolution (from OpenClaw + n8n)

Both OpenClaw and n8n have rich configuration with:
- Schema validation (Zod / JSON Schema)
- Hot-reload on config change
- Per-environment overrides

**RUSVEL's `rusvel-config`** is currently 287 lines with TOML config.
Enhancements:

```rust
/// Config with validation and hot-reload
pub struct ConfigManager {
    config: RwLock<RusvelConfig>,
    watchers: Vec<Box<dyn ConfigWatcher>>,
}

pub trait ConfigWatcher: Send + Sync {
    fn on_config_change(&self, old: &RusvelConfig, new: &RusvelConfig);
}

impl ConfigManager {
    /// Validate config against schema before applying
    pub fn update(&self, new_config: RusvelConfig) -> Result<()> {
        new_config.validate()?;
        let old = self.config.read().clone();
        *self.config.write() = new_config.clone();
        for watcher in &self.watchers {
            watcher.on_config_change(&old, &new_config);
        }
        Ok(())
    }
}
```

### 10.5 Session Management (from OpenClaw + ECC)

Both repos have rich session management. RUSVEL has sessions via `SessionId`
but lacks:

1. **Session transcripts** -- full conversation history per session
2. **Session metadata** -- what departments were accessed, what tools used
3. **Session resume** -- pick up where you left off
4. **Session export** -- share sessions for debugging

**Proposed:**

```rust
pub struct SessionTranscript {
    pub session_id: SessionId,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub department: String,
    pub messages: Vec<TranscriptMessage>,
    pub tools_used: Vec<String>,
    pub cost: f64,
    pub status: SessionStatus,
}

pub struct TranscriptMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub tool_calls: Vec<ToolCallRecord>,
}

pub enum SessionStatus {
    Active,
    Paused,    // Can be resumed
    Completed, // Naturally ended
    Expired,   // Timed out
}
```

---

## 11. Implementation Roadmap

### Phase 1: Quick Wins (1-2 weeks)

| Item | Source | Effort | Impact |
|------|--------|--------|--------|
| Wire flow-engine into AppState | RUSVEL internal | Low | Critical |
| Add flow execution events | n8n | Low | High |
| Add `.claude/hooks/` quality gates | ECC | Low | High |
| Add `/learn` command to `.claude/` | ECC | Low | Medium |
| Add design tokens CSS | Supabase | Low | Medium |
| Add `LoopNode` + `DelayNode` | n8n | Medium | High |

### Phase 2: Core Capabilities (2-4 weeks)

| Item | Source | Effort | Impact |
|------|--------|--------|--------|
| Expand ChannelPort trait | OpenClaw | Medium | Critical |
| Discord channel adapter | OpenClaw | Medium | High |
| Expression language (MiniJinja) | n8n | Medium | High |
| Cost tracking across all ports | n8n + ECC | Medium | High |
| Partial flow re-execution | n8n | Medium | High |
| Component variant system | Supabase | Medium | Medium |
| Session persistence hooks | ECC | Medium | Medium |

### Phase 3: Depth (4-8 weeks)

| Item | Source | Effort | Impact |
|------|--------|--------|--------|
| Slack channel adapter | OpenClaw | Medium | High |
| Email channel adapter | OpenClaw | Medium | Medium |
| Full node type library (10+ types) | n8n | High | High |
| Plugin system trait | Tauri | High | Medium |
| Continuous learning pipeline | ECC | High | Medium |
| Flow versioning + templates | n8n | High | Medium |
| WebSocket realtime events | Supabase | Medium | Medium |

### Phase 4: Scale (8-12 weeks)

| Item | Source | Effort | Impact |
|------|--------|--------|--------|
| Channel router + message queue | OpenClaw | High | High |
| Credential management (encrypted) | n8n | High | High |
| Media pipeline | OpenClaw | High | Medium |
| Plugin marketplace | Tauri + n8n | Very High | Medium |
| Desktop distribution | Tauri | Very High | Low |
| Collaborative editing | Supabase | Very High | Low |

### Dependency Graph

```
Phase 1
  |
  +-- Wire flow-engine --------+
  |                             |
  +-- Flow events              Phase 2
  |                             |
  +-- ECC hooks                 +-- Expression language
  |                             |     (needs flow-engine)
  +-- Design tokens             |
                                +-- ChannelPort expansion
                                |     |
                                |     +-- Discord adapter
                                |     |
                                |     Phase 3
                                |     |
                                |     +-- Slack adapter
                                |     +-- Email adapter
                                |     +-- Channel router
                                |
                                +-- Cost tracking
                                |     (needs flow events)
                                |
                                +-- Partial re-execution
                                      (needs flow-engine wired)
```

---

## 12. Appendix: Pattern Catalog

### A. Pattern: Node Execution Context (n8n)

**Problem:** Nodes need access to credentials, parameters, upstream data,
and execution metadata without coupling to engine internals.

**Solution:** Create an immutable context object passed to each node:

```rust
pub struct NodeContext<'a> {
    pub node_id: &'a str,
    pub parameters: &'a Value,
    pub inputs: &'a HashMap<String, Value>,
    pub execution: &'a FlowExecutionContext,
    pub credentials: &'a CredentialResolver,
    pub variables: &'a FlowVariables,
}
```

### B. Pattern: Channel Capability Degradation (OpenClaw)

**Problem:** Different channels support different features. Discord has
embeds; SMS has only text.

**Solution:** Channels declare capabilities; sender code checks before
using features and falls back gracefully:

```rust
if channel.capabilities().buttons {
    channel.send_rich(&target, &rich_with_buttons).await?;
} else {
    channel.send_message(&target, &text_fallback).await?;
}
```

### C. Pattern: Provenance Tracking (ECC)

**Problem:** Learned skills have unknown reliability. Which ones were
validated? Which are experimental?

**Solution:** Every learned artifact carries provenance metadata:

```json
{
    "source": "session-2026-03-30-abc123",
    "created_at": "2026-03-30T14:00:00Z",
    "confidence": 0.5,
    "author": "session-extraction",
    "validated_count": 2,
    "last_validated": "2026-03-30T16:00:00Z"
}
```

### D. Pattern: Typed Component Variants (Supabase)

**Problem:** Component styling is inconsistent; every instance uses
ad-hoc Tailwind classes.

**Solution:** Define variants as a typed schema; components derive
classes from variant selection:

```typescript
const button = tv({
    base: 'rounded-md font-medium',
    variants: {
        variant: { primary: '...', danger: '...' },
        size: { sm: '...', md: '...', lg: '...' },
    },
});
// Usage: button({ variant: 'primary', size: 'md' })
```

### E. Pattern: TypeId State Registry (Tauri)

**Problem:** Plugins need to register and retrieve state without
knowing about each other.

**Solution:** Use `TypeId` as key, `Box<dyn Any>` as value:

```rust
state.set(MyPluginState { ... });
let my_state: &MyPluginState = state.get();
```

### F. Pattern: Hook-Based Quality Gates (ECC)

**Problem:** Code quality degrades when working fast; formatting,
type errors, and secrets slip through.

**Solution:** Hooks fire automatically after tool use:

```
PostToolUse(Edit, *.rs) -> rustfmt
PostToolUse(Edit, *.ts) -> prettier
PreToolUse(Bash, git commit) -> check for secrets
Stop -> evaluate session for learnings
```

### G. Pattern: Execution Stack (n8n)

**Problem:** Topological sort can't handle partial execution,
dynamic fan-out, or mid-execution pause.

**Solution:** Use an explicit LIFO stack instead of iterator-based
traversal. Push/pop entries as execution progresses.

### H. Pattern: Debounced Message Batching (OpenClaw)

**Problem:** Rapid-fire notifications spam channels.

**Solution:** Queue messages; flush when batch is full or
oldest message exceeds age threshold.

### I. Pattern: Credential Encryption at Rest (n8n)

**Problem:** API keys and tokens stored in plaintext in database.

**Solution:** AES-256 encrypt on write, decrypt on read, using
a master key from environment or system keyring.

### J. Pattern: Hierarchical Session Routing (OpenClaw)

**Problem:** Messages need to reach the right agent in the right
department on the right channel.

**Solution:** Hierarchical binding resolution:
1. Exact peer match
2. Group/team match
3. Channel default
4. Global default

---

## Summary

This minibook maps six reference repositories to RUSVEL's specific gaps
and proposes concrete implementations for each. The key priorities are:

1. **Wire the flow engine** and add essential node types (from n8n)
2. **Expand channel abstraction** with Discord + Slack (from OpenClaw)
3. **Implement self-improvement hooks** for continuous learning (from ECC)
4. **Add design tokens + component variants** for frontend consistency (from Supabase)
5. **Track costs** across all operations (from n8n + ECC)

Each pattern is adapted to Rust's type system and RUSVEL's hexagonal
architecture. No code is copied -- only patterns are transferred.
