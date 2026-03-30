
## Overview

RUSVEL exposes a JSON REST API on port **3000** via Axum. All endpoints use the `/api/` prefix. CORS is enabled for all origins.

**Scale (verify on `main`):** the main router in `crates/rusvel-api/src/lib.rs` registers **153** `.route(` chains. Handler logic is split across **39** modules (one `*.rs` per module, excluding `lib.rs`). That is **not** the same as “153 HTTP methods” — a single chain can register `get().post()`.

**Router modules:** `agents`, `analytics`, `approvals`, `auth`, `browser`, `build_cmd`, `capability`, `chat`, `config`, `cron`, `db_routes`, `department`, `engine_routes`, `flow_routes`, `help`, `hook_dispatch`, `hooks`, `jobs`, `kits`, `knowledge`, `mcp_servers`, `pipeline_runner`, `playbooks`, `routes`, `rules`, `skills`, `system`, `terminal`, `visual_report`, `webhooks`, `workflows`.

For canonical metrics and gaps, see **[Repository status](./repository-status.md)**.

---

## Core

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Health check |
| `GET` | `/api/brief` | Executive brief |
| `POST` | `/api/brief/generate` | Generate brief |
| `GET` | `/api/config` | Global configuration |
| `PUT` | `/api/config` | Update configuration |
| `GET` | `/api/config/models` | List LLM models |
| `GET` | `/api/config/tools` | List tools |
| `GET` | `/api/analytics` | Analytics dashboard data |
| `GET` | `/api/profile` | User profile |
| `PUT` | `/api/profile` | Update profile |

## Sessions

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/sessions` | List sessions |
| `POST` | `/api/sessions` | Create session |
| `GET` | `/api/sessions/{id}` | Get session |
| `GET` | `/api/sessions/{id}/mission/today` | Daily plan |
| `GET` | `/api/sessions/{id}/mission/goals` | List goals |
| `POST` | `/api/sessions/{id}/mission/goals` | Create goal |
| `GET` | `/api/sessions/{id}/events` | Query events |

## Chat (God agent)

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/chat` | Send message (SSE) |
| `GET` | `/api/chat/conversations` | List conversations |
| `GET` | `/api/chat/conversations/{id}` | Conversation history |

## Departments

Replace `{dept}` with: `forge`, `code`, `content`, `harvest`, `gtm`, `finance`, `product`, `growth`, `distro`, `legal`, `support`, `infra`, `flow`, `messaging`.

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/departments` | List department definitions |
| `POST` | `/api/dept/{dept}/chat` | Department chat (SSE) |
| `GET` | `/api/dept/{dept}/chat/conversations` | List conversations |
| `GET` | `/api/dept/{dept}/chat/conversations/{id}` | History |
| `GET` | `/api/dept/{dept}/config` | Department config |
| `PUT` | `/api/dept/{dept}/config` | Update config |
| `GET` | `/api/dept/{dept}/events` | Department events |

## Engine-specific routes

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/dept/code/analyze` | Code analysis |
| `GET` | `/api/dept/code/search` | BM25 search |
| `POST` | `/api/dept/content/draft` | Draft content |
| `POST` | `/api/dept/content/from-code` | Content from code analysis |
| `PATCH` | `/api/dept/content/{id}/approve` | Approve content item |
| `POST` | `/api/dept/content/publish` | Publish (may require approval) |
| `GET` | `/api/dept/content/list` | List content items |
| `POST` | `/api/dept/harvest/score` | Score opportunity |
| `POST` | `/api/dept/harvest/scan` | Scan sources |
| `POST` | `/api/dept/harvest/proposal` | Generate proposal |
| `GET` | `/api/dept/harvest/pipeline` | Pipeline |
| `GET` | `/api/dept/harvest/list` | List harvest items |

## Flow engine (DAG)

| Method | Path | Description |
|--------|------|-------------|
| `GET` / `POST` | `/api/flows` | List / create flows |
| `GET` / `PUT` / `DELETE` | `/api/flows/{id}` | Get / update / delete |
| `POST` | `/api/flows/{id}/run` | Run flow |
| `GET` | `/api/flows/{id}/executions` | List executions |
| `GET` | `/api/flows/{id}/executions/{exec_id}/panes` | Execution panes |
| `GET` | `/api/flows/executions/{id}` | Get execution |
| `POST` | `/api/flows/executions/{id}/resume` | Resume |
| `POST` | `/api/flows/executions/{id}/retry/{node_id}` | Retry node |
| `GET` | `/api/flows/executions/{id}/checkpoint` | Checkpoint |
| `GET` | `/api/flows/node-types` | List node types |

## Playbooks

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/playbooks/runs` | List runs |
| `GET` | `/api/playbooks/runs/{run_id}` | Get run |
| `GET` / `POST` | `/api/playbooks` | List / create playbook |
| `GET` | `/api/playbooks/{id}` | Get playbook |
| `POST` | `/api/playbooks/{id}/run` | Run playbook |

## Starter kits

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/kits` | List kits |
| `GET` | `/api/kits/{id}` | Get kit |
| `POST` | `/api/kits/{id}/install` | Install kit |

## Knowledge / RAG

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/knowledge` | List entries |
| `POST` | `/api/knowledge/ingest` | Ingest |
| `POST` | `/api/knowledge/search` | Semantic search |
| `POST` | `/api/knowledge/hybrid-search` | Hybrid search |
| `GET` | `/api/knowledge/stats` | Stats |
| `GET` | `/api/knowledge/related` | Related entries |
| `DELETE` | `/api/knowledge/{id}` | Delete entry |

## Database (RusvelBase)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/db/tables` | List tables |
| `GET` | `/api/db/tables/{table}/schema` | Table schema |
| `GET` | `/api/db/tables/{table}/rows` | Table rows |
| `POST` | `/api/db/sql` | Run SQL |

## Approvals

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/approvals` | Pending jobs |
| `POST` | `/api/approvals/{id}/approve` | Approve |
| `POST` | `/api/approvals/{id}/reject` | Reject |

## Agents, skills, rules, workflows, MCP, hooks

Standard REST under `/api/agents`, `/api/skills`, `/api/rules`, `/api/workflows`, `/api/mcp-servers`, `/api/hooks`. Individual resource endpoints include `GET /api/mcp-servers/{id}` and `GET /api/hooks/{id}`. Workflows: `POST /api/workflows/{id}/run`. Hooks: `GET /api/hooks/events` for event types.

## Capability and help

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/capability/build` | Capability / `!build` bundle |
| `POST` | `/api/help` | AI help |

## System and visual regression

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/system/test` | Run tests |
| `POST` | `/api/system/build` | Run build |
| `GET` | `/api/system/status` | Status |
| `POST` | `/api/system/fix` | Self-fix |
| `POST` | `/api/system/ingest-docs` | Ingest docs |
| `GET` / `POST` | `/api/system/visual-report` | Visual reports |
| `POST` | `/api/system/visual-report/self-correct` | Self-correct from diffs |
| `POST` | `/api/system/visual-test` | Run visual tests |

## Terminal

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/terminal/dept/{dept_id}` | Dept terminal pane |
| `GET` | `/api/terminal/runs/{run_id}/panes` | Run panes |
| `GET` | `/api/terminal/ws` | Terminal WebSocket |

## Browser (CDP)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/browser/status` | Status |
| `POST` | `/api/browser/connect` | Connect |
| `GET` | `/api/browser/tabs` | Tabs |
| `POST` | `/api/browser/observe/{tab}` | Observe |
| `GET` | `/api/browser/captures` | Captures |
| `GET` | `/api/browser/captures/stream` | Capture stream |
| `POST` | `/api/browser/act` | Action |
