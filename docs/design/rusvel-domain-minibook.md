# RUSVEL Domain & Runtime — Validated Minibook

> **Purpose:** Validate the code-grounded domain report against the repository, cite sources, flag inaccuracies, and point toward next-level engineering.  
> **Audience:** Builders extending RUSVEL (backend, frontend, UX).  
> **Last validated:** 2026-03-28 (spot-checked against `main`).

---

## Index

1. [How to use this book](#1-how-to-use-this-book)
2. [Validation summary](#2-validation-summary-right--wrong--nuanced)
3. [Mental model & boot](#3-mental-model--boot)
4. [Department container](#4-department-container)
5. [Session & AgentConfig](#5-session--agentconfig)
6. [Skills, agents, rules](#6-skills-agents-rules)
7. [Hooks & events](#7-hooks--events)
8. [MCP servers](#8-mcp-servers)
9. [Workflows & playbooks & flows](#9-workflows-playbooks--flows)
10. [God chat vs department chat](#10-god-chat-vs-department-chat)
11. [Storage map](#11-storage-map)
12. [Department chat pipeline (ordered)](#12-department-chat-pipeline-ordered)
13. [Next level — backend](#13-next-level--backend)
14. [Next level — frontend & UX](#14-next-level--frontend--ux)
15. [Next level — engineering paradigms](#15-next-level--engineering-paradigms)
16. [Canonical doc drift](#16-canonical-doc-drift)

---

## 1. How to use this book

- **Green path:** Sections marked **Verified** match the cited code on the date above.
- **Cross-check:** When behavior changes, re-run greps for `dept_chat`, `build_mcp_config_for_engine`, and `boot_departments`.
- **Related prompt:** `docs/prompts/generate-domain-concepts.md` (meta-prompt for a longer narrative doc).

---

## 2. Validation summary (right / wrong / nuanced)

| Claim in report | Verdict | Notes |
|-----------------|---------|--------|
| Department chat validates `dept` against `AppState.registry` | **Right** | `validate_dept` uses `state.registry.get(dept)`. |
| Registry from `DepartmentApp` boot, not TOML, in the running app | **Right** | `rusvel-app` calls `boot::boot_departments` and uses returned `registry` (see §3). |
| `DepartmentRegistry::load` / `defaults()` still exist | **Right** | Legacy / bench / tooling; not the live API server path. |
| `resolve_dept_config`: registry + stored + profile | **Nuanced** | Profile text is merged into the **base** system prompt only when `base.system_prompt.is_none()` after copying `dept_def.default_config` — not an unconditional third layer. See §5 citation. |
| Skills `/name`, exclusions, `{{input}}`, dept scope | **Right** | Matches `skills.rs` + call site in `dept_chat`. |
| `@agent` first matching token, no engine filter in chat | **Right** | `extract_agent_mention` + full `agents` list. |
| Rules append `--- Rules ---` | **Right** | `load_rules_for_engine` + `dept_chat`. |
| Hooks: `dispatch_hooks`, suffix/wildcard match, `{dept}.chat.completed` | **Right** | `hook_dispatch.rs` + `department.rs` on `Done`. |
| `build_mcp_config_for_engine` unused in app | **Right** | Only defined in `mcp_servers.rs`; no callers in workspace (except the prompt doc). |
| Workflows use `ClaudeCliStreamer`, not `AgentRuntime` | **Right** | `workflows.rs` `run_workflow`. |
| Playbooks in-memory; Approval pauses without resume | **Right** | `playbooks.rs` `PlaybookStore` + `spawn_run_with_context`. |
| Flow engine stores `flows` / executions / checkpoints | **Right** | `flow-engine/src/lib.rs`. |
| `parallel_evaluate` node type hidden unless env | **Omitted in report** | Not wrong; just not mentioned. `flow_routes::list_node_types` filters on `RUSVEL_FLOW_PARALLEL_EVALUATE`. |
| God chat uses `chat_message`, dept uses `dept_msg_{dept}` | **Right** | `chat.rs` vs `department.rs`. |

**Wrong or misleading elsewhere in the repo (not in your report):** `docs/status/current-state.md` states department chat includes “per-dept MCP config”; **`department.rs` has no MCP references** — so that canonical status line overstates wiring until `build_mcp_config_for_engine` (or equivalent) is hooked into `AgentRuntime` / chat. See §16.

---

## 3. Mental model & boot

**Verified:** The HTTP server builds a `DepartmentRegistry` from department boot artifacts, not from loading TOML in the main path.

```9:12:crates/rusvel-app/src/main.rs
//! Departments are registered via [`boot::boot_departments()`] which
//! ...
//! registry for the API server.
```

```812:831:crates/rusvel-app/src/main.rs
    let dept_registry = boot::boot_departments(
        // ... ports and department list ...
    )
    .await?;
    let boot::DepartmentsBootResult {
        registry,
        // ...
    } = dept_registry;
```

Department chat merges stored per-dept config from object kind `dept_config` (key = department id), registry defaults, and conditional profile text:

```40:44:crates/rusvel-api/src/department.rs
const CONFIG_STORE_KEY: &str = "dept_config";

fn msg_namespace(engine: &str) -> String {
    format!("dept_msg_{engine}")
}
```

User-defined rows (skills, agents, rules, hooks, MCP **records**) live in SQLite `objects` via `ObjectStore`, with `metadata.engine` scoping in list/load helpers (see respective `*_api` modules).

---

## 4. Department container

**Verified:** `DepartmentApp`, manifest, registration context, and `from_manifests` match the report.

```19:33:crates/rusvel-core/src/department/app.rs
pub trait DepartmentApp: Send + Sync {
    fn manifest(&self) -> DepartmentManifest;
    async fn register(&self, ctx: &mut RegistrationContext) -> Result<()>;
    // ...
}
```

```24:104:crates/rusvel-core/src/department/context.rs
pub struct RegistrationContext {
    pub agent: Arc<dyn AgentPort>,
    pub events: Arc<dyn EventPort>,
    pub storage: Arc<dyn StoragePort>,
    // ...
    pub tools: ToolRegistrar,
    pub event_handlers: EventHandlerRegistrar,
    pub job_handlers: JobHandlerRegistrar,
    // ...
}
```

```335:365:crates/rusvel-core/src/department/context.rs
impl DepartmentRegistry {
    pub fn from_manifests(manifests: &[DepartmentManifest]) -> Self {
        let departments = manifests
            .iter()
            .map(|m| DepartmentDef {
                id: m.id.clone(),
                // ...
                default_config: m.default_config.clone(),
            })
            .collect();
        Self { departments }
    }
}
```

**Legacy registry file** (still in tree for `load` / `defaults`):

```41:61:crates/rusvel-core/src/registry.rs
impl DepartmentRegistry {
    pub fn load(path: &Path) -> Self { /* ... */ }
    pub fn defaults() -> Self { /* embedded DepartmentDef list */ }
}
```

---

## 5. Session & AgentConfig

**Verified:** Domain hierarchy and `AgentConfig` fields:

```483:454:crates/rusvel-core/src/domain.rs
pub struct Session { /* id, name, kind, tags, config, ... */ }
pub struct SessionConfig { pub default_model: Option<ModelRef>, pub budget_limit: Option<f64>, pub approval_policies: Vec<ApprovalPolicy>, ... }
pub struct AgentConfig {
    pub profile_id: Option<AgentProfileId>,
    pub session_id: SessionId,
    pub model: Option<ModelRef>,
    pub tools: Vec<String>,
    pub instructions: Option<String>,
    // ...
    pub metadata: serde_json::Value,
}
```

**Nuance (config cascade):** Profile is not always merged; it is applied when building the base system prompt **if** `default_config` did not already set `system_prompt`:

```58:77:crates/rusvel-api/src/department.rs
fn resolve_dept_config(
    dept_def: &DepartmentDef,
    stored: &LayeredConfig,
    profile: Option<&UserProfile>,
) -> ResolvedConfig {
    let mut base = dept_def.default_config.clone();
    let user_context = profile
        .map(rusvel_core::UserProfile::to_system_prompt)
        .unwrap_or_default();
    if base.system_prompt.is_none() {
        base.system_prompt = Some(format!("{}\n\n{user_context}", dept_def.system_prompt));
    }
    let merged = stored.overlay(&base);
    merged.resolve()
}
```

Context pack when `session_id` is valid:

```447:454:crates/rusvel-api/src/department.rs
    if let Some(sid_str) = body.session_id.as_ref()
        && let Ok(u) = Uuid::parse_str(sid_str)
    {
        let sid = SessionId::from_uuid(u);
        if let Ok(pack) = context_pack_for_chat(&state, &dept, &sid, &stored).await {
            resolved.system_prompt.push_str(&to_prompt_section(&pack));
        }
    }
```

Metadata keys for department and tier:

```208:215:crates/rusvel-core/src/domain.rs
pub const RUSVEL_META_MODEL_TIER: &str = "rusvel.model_tier";
pub const RUSVEL_META_DEPARTMENT_ID: &str = "rusvel.department_id";
```

---

## 6. Skills, agents, rules

**Skills** — storage kind `skills`, `resolve_skill`, used in `dept_chat`:

```123:179:crates/rusvel-api/src/skills.rs
pub async fn resolve_skill(state: &Arc<AppState>, engine: &str, message: &str) -> Option<String> {
    // /prefix, exclusions, normalize slug, filter by metadata.engine, {{input}}
}
```

```407:413:crates/rusvel-api/src/department.rs
    let effective_message =
        if let Some(expanded) = crate::skills::resolve_skill(&state, &dept, &body.message).await {
            expanded
        } else {
            body.message.clone()
        };
```

**Agents** — mention extraction and override (no engine filter in this path):

```805:811:crates/rusvel-api/src/department.rs
fn extract_agent_mention(message: &str) -> Option<String> {
    for word in message.split_whitespace() {
        if word.starts_with('@') && word.len() > 1 {
            return Some(word[1..].to_string());
        }
    }
    None
}
```

```416:433:crates/rusvel-api/src/department.rs
    if let Some(agent_name) = extract_agent_mention(&body.message)
        && let Ok(agents) = state.storage.objects().list("agents", ...).await
    {
        let found = agents.into_iter()
            .filter_map(|v| serde_json::from_value::<AgentProfile>(v).ok())
            .find(|a| a.name.eq_ignore_ascii_case(&agent_name));
        if let Some(agent) = found {
            resolved.system_prompt = agent.instructions.clone();
            resolved.model = agent.default_model.model.clone();
            if !agent.allowed_tools.is_empty() {
                resolved.allowed_tools = agent.allowed_tools.clone();
            }
        }
    }
```

**Rules:**

```125:139:crates/rusvel-api/src/rules.rs
pub async fn load_rules_for_engine(state: &Arc<AppState>, engine: &str) -> Vec<RuleDefinition> {
    // enabled + metadata.engine match or global
}
```

---

## 7. Hooks & events

**Dispatch and matching:**

```23:96:crates/rusvel-api/src/hook_dispatch.rs
pub fn dispatch_hooks(event_kind: &str, payload: serde_json::Value, engine: &str, storage: Arc<dyn StoragePort>) {
    tokio::spawn(async move { /* load hooks, matches_event, spawn per hook */ });
}
fn matches_event(hook_event: &str, hook_matcher: &str, event_kind: &str) -> bool {
    if hook_matcher == "*" { return true; }
    if event_kind == hook_event { return true; }
    if event_kind.ends_with(hook_event) { return true; }
    false
}
```

**After department chat completes:**

```632:657:crates/rusvel-api/src/department.rs
                    let _ = events_port.emit(rusvel_core::domain::Event {
                        // ...
                        kind: format!("{eng}.chat.completed"),
                        // ...
                    }).await;
                    crate::hook_dispatch::dispatch_hooks(
                        &format!("{eng}.chat.completed"),
                        serde_json::json!({ /* ... */ }),
                        &eng.to_string(),
                        storage.clone(),
                    );
```

---

## 8. MCP servers

**CRUD and JSON builder exist:**

```133:182:crates/rusvel-api/src/mcp_servers.rs
pub async fn build_mcp_config_for_engine(state: &Arc<AppState>, engine: &str) -> Option<String> {
    // builds { "mcpServers": ... }
}
```

**Verified gap:** No caller in `rusvel-app`, `department.rs`, or `rusvel-agent` in-tree (grep workspace). Department chat does not reference MCP. **Recommendation:** wire this into the same path that builds LLM/tool requests for `AgentRuntime`, or adjust `current-state.md` until it does.

---

## 9. Workflows, playbooks & flows

**Workflows** — Claude CLI, sequential agents:

```200:272:crates/rusvel-api/src/workflows.rs
pub async fn run_workflow(/* ... */) -> Result<Json<WorkflowRunResult>, /* ... */> {
    // load WorkflowDefinition, list agents, per step: ClaudeCliStreamer::stream_with_args
}
```

**Playbooks** — in-memory store + `Approval` stops run:

```25:45:crates/rusvel-api/src/playbooks.rs
struct PlaybookStore {
    user: std::sync::RwLock<HashMap<String, Playbook>>,
    runs: std::sync::RwLock<HashMap<String, PlaybookRun>>,
}
```

```383:394:crates/rusvel-api/src/playbooks.rs
                    if let Some(obj) = result.as_object_mut() {
                        if obj.get("kind").and_then(|k| k.as_str()) == Some("approval") {
                            run.status = PlaybookRunStatus::Paused;
                            // ...
                            return;
                        }
                    }
```

**Flow engine** — registrations and stores:

```33:77:crates/flow-engine/src/lib.rs
const FLOW_STORE: &str = "flows";
const EXECUTION_STORE: &str = "flow_executions";
pub(crate) const CHECKPOINT_STORE: &str = "flow_checkpoints";
// FlowEngine::new registers Code, Condition, Agent, Browser*, ParallelEvaluate nodes
```

**Node list API nuance:**

```225:232:crates/rusvel-api/src/flow_routes.rs
    let parallel_on = std::env::var("RUSVEL_FLOW_PARALLEL_EVALUATE")
        .ok()
        .as_deref()
        == Some("1");
    if !parallel_on {
        types.retain(|t| t != "parallel_evaluate");
    }
```

---

## 10. God chat vs department chat

**God chat** — requires profile; `chat_message` kind; department id `global`:

```65:131:crates/rusvel-api/src/chat.rs
pub async fn chat_handler(/* ... */) -> Result</* ... */> {
    let profile = state.profile.as_ref().ok_or(/* ... */)?;
    // load_history -> list "chat_message"
    // ...
    meta.insert(RUSVEL_META_DEPARTMENT_ID.into(), serde_json::json!("global"));
```

```290:297:crates/rusvel-api/src/chat.rs
async fn store_message(/* ... */) -> rusvel_core::error::Result<()> {
    storage.objects().put("chat_message", &msg.id, serde_json::to_value(msg)?).await
}
```

**Department chat** — `dept_msg_{dept}` and full pipeline: `dept_chat` in `department.rs` (lines 321–669 region).

---

## 11. Storage map

| Concern | Kind / store | Code reference |
|--------|----------------|----------------|
| Dept layered config | `dept_config` + id | `CONFIG_STORE_KEY` in `department.rs` |
| Dept chat messages | `dept_msg_{engine}` | `msg_namespace`, `store_namespaced_message` |
| God chat | `chat_message` | `chat.rs` |
| Skills / agents / rules / hooks / MCP records | respective kinds | `*_api` modules + `store.rs` `ObjectStore` |
| Workflows | `workflows` | `workflows.rs` |
| Flows | `flows`, `flow_executions`, `flow_checkpoints` | `flow-engine/src/lib.rs` |
| Events | `events` table | `EventStore` in `rusvel-db/src/store.rs` |
| Playbooks (user) / runs | process memory | `playbooks.rs` |

Object upsert:

```595:614:crates/rusvel-db/src/store.rs
async fn put(&self, kind: &str, id: &str, object: serde_json::Value) -> rusvel_core::Result<()> {
    // INSERT INTO objects ... ON CONFLICT(kind, id) DO UPDATE
}
```

---

## 12. Department chat pipeline (ordered)

Order matches `dept_chat` in `department.rs`: validate dept → load/merge config → conversation id → history (50) → persist user → `!build` early return → `resolve_skill` → `@agent` → rules → optional context pack → platform API blurb → dept-specific action strings → optional RAG → build `user_input` → `AgentConfig` → `create` + `run_streaming` → on `Done`: persist assistant, emit event, `dispatch_hooks`.

Entry point:

```321:328:crates/rusvel-api/src/department.rs
pub async fn dept_chat(
    State(state): State<Arc<AppState>>,
    Path(dept): Path<String>,
    Json(body): Json<ChatRequest>,
) -> Result<Sse</* ... */>, (StatusCode, String)> {
```

---

## 13. Next level — backend

1. **Wire MCP configs into `AgentRuntime` / `LlmPort` path** — `build_mcp_config_for_engine` is implemented but unused; this is the highest-leverage fix to align product with stored `mcp_servers` rows and with `current-state.md`.
2. **Unify execution stacks** — Workflows use `ClaudeCliStreamer`; department chat uses `AgentRuntime`. Routing workflows through `AgentPort` would give one observability, tool, and approval model.
3. **Persist playbooks and runs** — In-memory `PlaybookStore` does not survive restarts; move to `ObjectStore` or SQL with migration.
4. **Playbook `Approval` completion** — Add resume endpoint + job/approval integration so `Paused` runs can continue (aligns with ADR-008 human gates).
5. **Scope `@agent` by department** — Match `list_agents` filtering (`metadata.engine`) in `dept_chat` to avoid cross-dept persona leakage.
6. **AuthZ middleware** — Plans reference phased auth (`docs/plans/sprint-6-7-implementation.md`); API surface is large (`~153` route chains in `lib.rs` per `current-state.md`).
7. **Structured logging / tracing** — Correlate `run_id`, `session_id`, `department_id` on every SSE stream and hook execution for ops.
8. **Rate limits & budget enforcement** — `AgentConfig.budget_limit` exists; ensure hard enforcement and user-visible burn-down in API responses.

---

## 14. Next level — frontend & UX

1. **Surface wiring gaps in UI** — If MCP is not in runtime, show “configured but not active” or hide until wired to avoid false confidence.
2. **Playbook run state** — Expose `Paused` / `awaiting_approval` with a clear CTA or explain limitation until backend resume exists.
3. **Department parity** — God chat vs dept chat feature gap (skills, rules): either document in UI or add optional rules/skills to god chat for consistency.
4. **Onboarding to domain concepts** — Link from Command Palette / dept help to this minibook or a shorter in-app glossary (Session, Skill, Rule, Hook, Flow).
5. **Approval queue UX** — Tie UI to real job states (`AwaitingApproval`) end-to-end with notifications (Telegram channel already exists server-side).
6. **Flow builder** — If `parallel_evaluate` is hidden by default, surface the env flag in settings or developer tools.

---

## 15. Next level — engineering paradigms

1. **Contract tests for API promises** — Generate tests from OpenAPI or route table that assert critical paths (dept chat, hooks fired, MCP when enabled).
2. **Feature flags** — Replace ad-hoc env vars (`RUSVEL_FLOW_PARALLEL_EVALUATE`) with a central config port for UI + API consistency.
3. **ADR sync** — When behavior changes (e.g. MCP), update `docs/status/current-state.md` in the same PR to avoid “canonical drift” (§16).
4. **Hexagonal boundaries** — Keep engines free of adapter imports; extend `engine-check` / `arch-reviewer` usage on large PRs.
5. **Observability** — Metrics already track cost; add RED/USE dashboards for chat latency and job queue depth.

---

## 16. Canonical doc drift

`docs/status/current-state.md` currently implies department chat includes **per-dept MCP config**. As of validation, **`crates/rusvel-api/src/department.rs` contains no MCP integration**. Either implement MCP wiring in the chat/runtime path or revise that bullet to avoid misleading operators and contributors.

---

## Changelog

| Date | Change |
|------|--------|
| 2026-03-28 | Initial minibook: validation of code-grounded report + next-level backlog. |
