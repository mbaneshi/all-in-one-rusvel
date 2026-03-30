# 06 — LLM, tools & abuse surface

## Context (for next agents)

- **Stack:** `rusvel-agent`, `rusvel-tool` / `ScopedToolRegistry`, `rusvel-engine-tools`, MCP client, streaming LLM providers in `rusvel-llm`.
- **Risks:** Tool exfiltration, prompt injection via untrusted content, credential leakage in logs/events, cost blowups (ModelTier / streaming).

---

## Agent prompt (copy below)

```
Audit LLM + tool surface for RUSVEL.

Review: tool registration, scoping per department, MCP tool dispatch, logging of prompts/responses, human approval gates (tie to ADR-008 where relevant).

Produce in docs/audit/agents/06-llm-tools-abuse-surface.md Report:
- Executive summary.
- Findings: injection/abuse/cost/privacy — with evidence paths.
- Fix proposals (guardrails, allowlists, rate limits, redaction).
- Space for improvement.

Handoff: APIs/UI flows that must stay aligned for audit 09.
```

---

## Report

### Executive summary

The in-process agent path (`AgentRuntime` + shared `ToolRegistry`) exposes a **large, global tool surface** to every chat run: initial tools are **all non-`searchable` registered tools**, with **no `ScopedToolRegistry` wrapper** in `rusvel-app`, and **`AgentConfig.tools` (department/agent allowlists) is not applied** inside the agent loop. That widens **abuse, accidental exfiltration, and cost** versus the documented “per-department tools” story. **Registry-level** `ToolPermissionMode::Supervised` is effectively **disconnected** from department context because **`__department_id` is never injected** into tool args by the runtime—so `ToolRegistry::check_permission` almost always sees **no department** and falls back to **Auto**. **ADR-008** is **real for the job queue** (`AwaitingApproval`, `/api/approvals`, worker `hold_for_approval`) but is **not** the same mechanism as tool-level “supervised” strings returned from `ToolRegistry`. **MCP**: stored MCP server configs feed **`build_mcp_config_for_engine`** (Claude-style JSON for external CLI); **`McpClientManager` / `register_mcp_tools` are not wired** in `rusvel-app`, so **in-process agent chat does not auto-bridge** configured MCP servers. **Logging**: chat **persists full user/assistant text** in object storage; completion **events** carry **metadata only** (e.g. cost, response length); **`tracing`** can emit **tool names** at debug. **RAG and rules** inject **retrieved or configured text** into the system prompt—standard **indirect prompt-injection** surface.

### Findings

| Severity | Category (injection / abuse / cost / privacy) | Evidence | Notes |
|----------|-----------------------------------------------|----------|-------|
| High | abuse, cost | `crates/rusvel-agent/src/lib.rs` (`run_streaming_loop` / `AgentPort::run`): `tool_defs` seeded from `tools.list()` filtered by `!searchable` only; `tool_search` can add more via `tools.search` — no department allowlist enforced here. | Any department/global chat that can invoke tools effectively sees the same broad registry (minus searchable-only defs until discovered). |
| High | abuse, privacy | `crates/rusvel-app/src/main.rs`: `ToolRegistry::new()` + builtins + `tool_search` + engine tools + terminal/delegate; **no** `ScopedToolRegistry::new(...)`. | Conflicts with docs claiming per-department scoping via `ScopedToolRegistry` unless another layer filters (see next row). |
| High | abuse | `crates/rusvel-api/src/department.rs`: `AgentConfig { tools: resolved.allowed_tools.clone(), ... }` — **`rusvel-agent` never reads `config.tools`** (grep: no `config.tools` in agent crate). | UI/config allowlists are misleading for actual LLM tool lists. |
| Medium | abuse, privacy | `crates/rusvel-tool/src/lib.rs`: `ScopedToolRegistry` filters `list`/`call`/`search`/`schema` by prefix/exact allowlist — **implemented but not used** at composition root per `docs/design/rusvel-core-concept-validated.md` (§ScopedToolRegistry / `main.rs`). | Defense-in-depth exists in code but is not active for the running binary. |
| Medium | abuse | `crates/rusvel-tool/src/lib.rs`: `ToolRegistry::call` reads `__department_id` from args for `check_permission`; **`rusvel-agent` passes `effective_args` from the LLM without injecting `__department_id`**. | Dept-specific `ToolPermission` rules in the registry rarely apply; global rules only. |
| Medium | abuse | `crates/rusvel-tool/src/lib.rs`: `Supervised` returns `ToolResult` with text `AWAITING_APPROVAL` and `success: true` — **not** integrated with `JobPort` or `/api/approvals`. | Differs from ADR-008 job approval; model may treat as normal tool success. |
| Medium | injection | `crates/rusvel-api/src/department.rs`: RAG block appends `vector_store.search` hits into `resolved.system_prompt` under “Relevant Knowledge”. | Untrusted KB content can steer the model (classic indirect injection). |
| Medium | injection | `crates/rusvel-api/src/department.rs`: `load_rules_for_engine` appends rule `content` into the system prompt. | Compromised or malicious rules stored in `ObjectStore` affect all chats loading them. |
| Low–Med | injection | `crates/rusvel-api/src/department.rs`: `/skill` expansion via `resolve_skill`, `@agent` override of instructions/tools from stored `agents`. | Stored objects become prompt surface; same trust model as rules. |
| Low–Med | privacy | `crates/rusvel-api/src/department.rs`, `crates/rusvel-api/src/chat.rs`: user messages and assistant replies stored via `store_message` / `store_namespaced_message` (full `content`). | Durable plaintext transcripts; backup/DB access = data exposure. |
| Low | privacy | `crates/rusvel-api/src/department.rs`: `{eng}.chat.completed` event payload includes `cost_usd`, `response_length` — not full text (`department.rs` ~682–696). | Better than logging full prompts; still session/usage signal. |
| Low | privacy | `crates/rusvel-api/src/department.rs`: GitHub connector block adds note that a PAT exists when `github_connector` has token. | Expands secret *existence* into model context (not the token itself). |
| Low | cost | `crates/rusvel-agent/src/lib.rs`: `DEFAULT_MAX_ITERATIONS` = 50; each iteration can be a full LLM round-trip + tools. | Budget fields exist on config but loop bound dominates worst case. |
| Low | cost | `crates/rusvel-llm/src/cost_tracking.rs`: `CostTrackingLlm` records estimated USD + token counts to `MetricStore` — not a cap. | Spend visibility without hard enforcement at LLM layer. |
| Info | abuse / architecture | `crates/rusvel-mcp-client/src/lib.rs`: `McpClientManager::connect` registers `server__tool` into `ToolRegistry`. | **Unused** in `crates/rusvel-app` (no references); MCP CRUD is separate path. |
| Info | architecture | `crates/rusvel-api/src/mcp_servers.rs`: comment “passed to claude -p via --mcp-config”; `build_mcp_config_for_engine` builds JSON for external Claude, not in-process tools. | “MCP dispatch” for the **API server agent** ≠ subprocess MCP bridge. |
| Positive | abuse (partial) | `crates/rusvel-agent/src/lib.rs`: `agent_permission_blocks_tool` — `Supervised` blocks tools with `metadata.destructive == true` (see builtins). | Narrows destructive tools when dept `permission_mode` is supervised; **does not** replace allowlists or registry scoping. |
| Positive | privacy (partial) | `crates/rusvel-llm/src/cost_tracking.rs`: metrics metadata uses token counts, not full prompts. | Reduces credential leakage via metrics vs logging full requests. |
| Positive | ADR-008 | `docs/design/decisions.md` (ADR-008); `crates/rusvel-api/src/approvals.rs`; `crates/rusvel-db/src/store.rs` `hold_for_approval`; `crates/rusvel-app/src/main.rs` worker branches. | Publishing/outreach-style **jobs** use real approval queue; align messaging so UI does not conflate with tool “AWAITING_APPROVAL”. |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| F1 | Wrap or substitute `ToolPort` passed into `AgentRuntime` with **`ScopedToolRegistry`** built from resolved department **`allowed_tools`** / manifest defaults; ensure `tool_search` only returns allowed names. | M | `rusvel-app`, `rusvel-api`/`department.rs`, `rusvel-agent` |
| F2 | **Enforce `AgentConfig.tools`** inside `AgentRuntime` when building `tool_defs` and before `tools.call` (deny unknown names); treat empty list as explicit “defaults” vs “deny all” per product decision. | S–M | `rusvel-agent` |
| F3 | Inject trusted **`__department_id`** (from `AgentConfig.metadata`, not LLM args) in the agent loop before `ToolPort::call`, or move permission check to a wrapper that takes `department_id: Option<&str>` explicitly. | M | `rusvel-agent`, `rusvel-tool` |
| F4 | Replace or gate `ToolPermissionMode::Supervised` **string** result: enqueue **`JobPort`** approval, or return **`success: false`** + structured code so the model cannot misread approval state. | M | `rusvel-tool`, `rusvel-jobs`, API |
| F5 | **Rate limits / budgets**: per-session tool-call count, per-run wall clock, and hard stop when `budget_limit` exceeded (today verify if only advisory). | M | `rusvel-agent`, API |
| F6 | **Prompt injection hygiene**: sandbox markers for RAG/rules/skills; optional **strip or quote** KB snippets; admin-only rule writes; periodic rules signing or hash display in UI. | M–L | `rusvel-api`, `frontend` |
| F7 | **Redaction** for stored chat and for SSE: optional mask of secrets in tool args/results; configurable **PII scrub** before `ObjectStore`. | L | `rusvel-api`, storage layer |
| F8 | **Wire or delete** `McpClientManager` path: either connect enabled `mcp_servers` on boot with **explicit** tool prefix allowlist per dept, or document that MCP configs are **CLI-only** to avoid false sense of in-app exposure. | M | `rusvel-app`, docs |
| F9 | Structured **audit log** (append-only) for tool name + dept + session + arg hash (not full args) for forensics. | M | new small module + `EventStore` |

### Space for improvement

- **Single story for “approval”**: unify vocabulary across UI, SSE, and docs—**job queue** (`/api/approvals`) vs **tool-level** supervised/locked vs **`AWAITING_APPROVAL` text**.
- **Threat model doc**: treat `ToolRegistry` as **host capability**; clarify that **LLM-chosen tool args** are attacker-controlled from any untrusted content that reached the model.
- **`tool_search`**: cap discovered tools per run; consider **allowlist intersection** after discovery.
- **Tests**: contract tests that department X cannot `call` tool Y (registry + agent loop), not only `ScopedToolRegistry` unit tests in isolation.
- **MCP**: if in-process bridge ships, add **spawn isolation** review (subprocess env, command injection from stored config).

### Handoff (for audit 09)

Audit **09 (APIs/UI flows)** should trace end-to-end alignment with this report:

- **Chat surfaces**: `POST /api/chat` (God agent), `POST /api/dept/{dept}/chat` — SSE event types (`tool_call_start` / `tool_call_end` / `text_delta` / `run_completed`) and what the **frontend** renders from `frontend/src/lib/api.ts` (`streamChat`, department streams); verify **auth** on these routes matches approval sensitivity.
- **Config that does not match runtime**: department **`allowed_tools`**, **`permission_mode`**, **`disallowed_tools`** in API responses vs actual `AgentRuntime` behavior (until F1/F2 land).
- **Approval UX**: `GET/POST /api/approvals/*`, dashboard pending counts, **Content** publish / **GTM** outreach flows that enqueue jobs → `AwaitingApproval` — ensure UI copy references **jobs**, not tool strings.
- **MCP admin**: `/api/mcp-servers` CRUD and **`build_mcp_config_for_engine`** consumers (if any UI or CLI) — clarify **external Claude** vs **in-process** agent.
- **Hooks**: `hook_dispatch` on `{dept}.chat.completed` — payloads avoid full transcripts if webhooks are untrusted.
- **Jobs listing**: `/api/jobs` filters — operators use this alongside approvals for operational abuse detection.
- **Capability / build flows**: `/api/capability/build` and `!build` — tool-like power distinct from department chat; include in 09 if “agentic product surface” scope covers them.
