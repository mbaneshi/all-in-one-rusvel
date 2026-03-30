# Workflows vs flows vs playbooks (migration)

RUSVEL exposes three related concepts. This note clarifies boundaries and recommends migration away from the legacy **workflows** HTTP API toward the **native automation plane** (flows + playbooks + cron + webhooks).

## Definitions

| Surface | Purpose |
|--------|---------|
| **`GET/POST /api/flows`** | DAG definitions and runs (`FlowEngine`). Best for branching, parallel steps, and visual graphs (`/flows` UI). |
| **`GET/POST /api/playbooks`** | Ordered sequences of steps (`Playbook` / `PlaybookAction`: agent, flow, approval, skill, rules, etc.). Persisted in ObjectStore (`kind: playbooks`). |
| **`/api/workflows`** | Legacy linear runner (Claude-CLI-oriented). Overlaps playbooks and single-node flows; treat as **legacy**. |

## Automation triggers (single dispatcher)

Cron and webhooks can invoke the same dispatcher as MCP tools:

- **Cron:** set `event_kind` to `rusvel.automation.v1` and put an [`AutomationTriggerPayload`](../../crates/rusvel-core/src/domain.rs) JSON object in the schedule `payload` (`action`, `target_id`, optional `variables`, `department_id` for flows). The job worker parses `payload` from `ScheduledCron` jobs and calls `dispatch_automation_trigger`.
- **Convenience:** schedules with a generic `event_kind` may still parse if `payload` contains both `action` and `target_id` (same JSON shape).
- **Webhooks:** register `event_kind` `rusvel.automation.trigger`. POST body must include `session_id` plus the same automation fields as `AutomationTriggerPayload` (flat JSON). Verified receives enqueue `JobKind::Custom("rusvel.automation.dispatch")`.

## Migration from `/api/workflows`

1. **Linear agent chains** → **Playbooks** with `PlaybookAction::Agent` / `Skill` / `RulesAppend`, or a minimal flow with one agent node.
2. **Multi-branch or tool-heavy graphs** → **Flows** (`/api/flows`).
3. **Scheduled legacy runs** → **Cron** rows with `rusvel.automation.v1` targeting a playbook id or flow UUID.

Keep using `/api/workflows` only until equivalent playbooks/flows exist; then remove or gate new usage behind feature flags in your deployment.

## UI labels

Department shell tab **`workflows`** (path `/dept/:id/workflows`) is labeled **Agent chains** in the UI to avoid confusion with DAG **Flows** (`/flows`).

## MCP parity

Tools `automation_run_flow`, `automation_run_playbook`, and `automation_upsert_cron_schedule` use the same dispatcher and cron shape as HTTP. Stdio MCP requires the HTTP router to have registered `AppState` via `register_app_state_for_worker` (start the API server or use streamable HTTP MCP).
