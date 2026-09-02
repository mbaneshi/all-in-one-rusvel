# Claude Code Pattern Extraction — Raw Notes (2026-04-01)

> Historical session transcript, filed as-is from repo root (`plan3.md`). Not a
> reviewed plan. Items 1 (bootstrap fast-paths) and 2 (prompt caching boundary)
> were already implemented in commit `c4278ba` (hot-swap Claude provider,
> prompt caching, deferred tool loading). The rest is unactioned as of
> 2026-09-02 — see `docs/design/vision-v3.md` for the phased roadmap these
> ideas were meant to feed.

---


⏺ Now I see the full picture. Rusvel is already a serious system (55 crates, 73k LOC, hexagonal arch). The question is: what specific patterns from Claude Code's harness are
   worth stealing for rusvel?

  Here's what maps cleanly:

  ---
  High-Value Patterns to Extract

  1. Bootstrap Fast-Paths (immediate win)

  Claude Code has 12 phases where each can short-circuit. Rusvel runs one binary with 5 surfaces (--mcp, --tui, REPL, CLI one-shot, HTTP server). Right now they all go
  through full initialization.

  Steal this: Check flags early, skip loading all 14 departments if you only need MCP's 6 tools. Skip frontend embed if running CLI-only. Skip agent runtime if just querying
   the database.

  2. System Prompt Caching Boundary (cost reduction)

  Claude Code splits prompts at __SYSTEM_PROMPT_DYNAMIC_BOUNDARY__ — everything above is static and cacheable via the API's prompt caching. Everything below (cwd, date,
  session context) changes per turn.

  Steal this: Rusvel's department manifests each have a prompt. Structure them so the department persona + capabilities + rules are in the cacheable prefix, and
  session/mission context is injected after the boundary. With 14 departments doing LLM calls, this could cut API costs significantly.

  3. Tool Security Layers (critical for terminal/browser)

  Claude Code has 8 files just for BashTool security: bashSecurity, destructiveCommandWarning, sedValidation, pathValidation, readOnlyValidation, shouldUseSandbox,
  commandSemantics, modeValidation.

  Steal this: Rusvel has a terminal with PTY panes AND a CDP browser adapter. Both are attack surfaces. Port the security classification approach — categorize commands by
  semantics (read-only, write, destructive), validate paths, warn on destructive ops before the agent executes them.

  4. Conversation Compaction (long sessions)

  Claude Code auto-compacts after 12 turns with a sliding window. The query engine tracks token budgets and stops at limits.

  Steal this: Rusvel's agent runtime does streaming tool-use loops but doesn't appear to have automatic compaction. For department chat sessions that run long (especially
  Forge orchestrating missions), you need this or context windows blow up.

  5. Deferred Tool Loading (performance)

  Claude Code's ToolSearchTool pattern: tools are listed by name only, full schemas loaded on-demand. With 22+ registered tools in rusvel's ScopedToolRegistry, sending all
  schemas in every API call wastes tokens.

  Steal this: Only inject tool schemas for tools relevant to the current department + context. Forge doesn't need code-engine tools. Content doesn't need CRM tools. Load on
  demand.

  6. Agent Memory with Age Scoring (memdir pattern)

  Claude Code's memdir: findRelevantMemories, memoryScan, memoryAge, teamMemPaths. Memories decay over time and are scored for relevance.

  Steal this: Rusvel has FTS5 session-scoped memory. Add age-weighted relevance scoring so older memories naturally fade unless reinforced. The teamMemPaths pattern also
  maps to rusvel's department-scoped memory.

  7. Multi-Agent Coordination (for Forge)

  Claude Code has TeamCreateTool, SendMessageTool, spawnMultiAgent, coordinatorMode, agentMemory, agentMemorySnapshot, forkSubagent, resumeAgent.

  Steal this: Rusvel's Forge engine does multi-agent orchestration with 10 personas. Claude Code's patterns for agent-to-agent messaging, memory snapshots between agents,
  and forking/resuming could make Forge's delegation more robust.

  ---
  Medium-Value Patterns

  8. Skill System (lighter than departments)

  Claude Code has bundled skills (loop, simplify, verify, stuck) that are lighter than full commands. Rusvel's departments are heavy. A skill layer within departments could
  handle quick operations without spinning up full engine logic.

  9. Bridge Protocol (for multi-channel)

  Claude Code's bridge subsystem (31 modules) handles remote control via JWT + WebSocket with capacity management. Rusvel's ADR-016 plans Telegram/Discord/Slack channels.
  The bridge pattern — especially capacityWake, flushGate, pollConfig — is a proven approach for managing remote sessions.

  10. GrowthBook / Feature Flags

  Claude Code has GrowthBook experiment event schemas baked in. Rusvel could benefit from feature flags for its phased rollout — especially for gating the 7 skeleton
  departments.

  ---
  Concrete Next Step

  Want me to start with any of these? I'd suggest #2 (prompt caching boundary) and #5 (deferred tool loading) first — they're the highest ROI with the least code change, and
   I can work directly from the TS source at src/ to extract the exact implementation patterns.
