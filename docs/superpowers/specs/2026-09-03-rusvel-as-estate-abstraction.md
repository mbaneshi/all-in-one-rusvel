# RUSVEL as the estate's abstraction layer

**Date:** 2026-09-03
**Status:** Draft — synthesizes a single session's research and discussion; nothing here is locked or built yet.
**Scope:** RUSVEL core (`dept-*`, `content-engine`, `harvest-engine`, `gtm-engine`) + the wider estate (`capability-tenant-infra`, `skill-eco`, `autobiz-kit`, `~/infra`) + the first real venture, `mbaneshi-ir-site`.
**Supersedes (framing, not code):** ledger decision `strategy-rusvel-scope-vs-anthropic-native` (locked 2026-09-03, commit `0c64173`) sharpens into the position stated in §3 below.

---

## The thing being built

> RUSVEL is not a company-runner competing with Claude Code, Cowork, or `capability-tenant-infra`. It's the ports-and-adapters abstraction layer that every venture, every capability, and every model plugs into and out of — narrow in what it *executes*, broad in *where it reaches*. It gets proven on one real venture (`mbaneshi.ir`) before any of it is generalized.

Three quarters of what this needs already exists, spread across separate repos that nobody had said out loud were one system. This document says it, for RUSVEL's slice of the estate specifically — mirroring the move `~/meta-developer-eco`'s estate-architecture spec (2026-08-11) already made for the estate as a whole.

---

## 1. Where this comes from

A single 2026-09-03 session, triggered by two shared documents describing "run a company on AI departments, one human at the approval gate" ("Solo Unicorn Operating Manual," "Departments of One"). Checking those against the user's own history surfaced that the same idea had already been independently rebuilt at least four times — `capability-tenant-infra`/"SoloUnicorn" (2026-08-14), `zix-agent` vs. Hermes/Managed Agents (2026-08-01), RUSVEL itself, and the two shared manuals — published as the memo "Four Instruments, One Idea" (English artifact + fa digest, both 2026-09-03). The user then corrected the memo's "narrow RUSVEL to a deterministic core" framing across several turns: capabilities and RUSVEL don't compete, they complete each other; RUSVEL's job is chief-of-staff, not one more capability. This document is the resulting synthesis. Full provenance chain lives in memory: `project_rusvel_solo_unicorn_positioning`, `project_houshkar_zixlancer_departments_of_one`, `project_ubu_ecosystem_discovery`, `project_ai_run_business_external_validation`, `project_self_host_consolidation_thesis`, `project_rusvel_chief_of_staff_role`, `project_rusvel_estate_abstraction_plan`, `project_rusvel_content_pipeline_inputs`, `project_rusvel_second_priority_gtm`, `project_rusvel_lead_sourcing_domestic_split`.

---

## 2. What already exists

### RUSVEL side (this repo)

| Piece | What it is | Status |
|---|---|---|
| `dept-*` / `DepartmentApp` | 14 department wrapper crates, ports-only deps, ADR-014 pattern | Wired, but **tenant = self only** — see §4 |
| `gtm-engine` | CRM, outreach sequences (draft → ADR-008 approval → SMTP send → next step), invoicing | Wired, real business logic (`docs/departments/gtm.md`) |
| `harvest-engine` | Source scan (`&dyn HarvestSource`) → score → proposal → Kanban pipeline (Cold→Warm→Hot→Won/Lost), outcome feedback into scoring | Wired, real business logic (`docs/departments/harvest.md`); currently scoped to freelance opportunities (Upwork/LinkedIn/GitHub/RSS) |
| `content-engine` | `writer.rs` (drafts via generic `AgentPort`, text-only), `code_bridge.rs` (code→content), `platform.rs` (LinkedIn/Twitter/DEV.to), `calendar.rs`, `analytics.rs` | Wired for text; no multi-modal generation, no Instagram/Postiz distribution |
| `rusvel-llm` | Claude API/CLI, OpenAI, Ollama adapters behind `LlmPort` | Wired — the existing proof that "swap the provider behind a port" already works in this codebase |
| `RUSVEL_DEMO=harvest` preset | Nightly cron → `MockSource` scan → top-N `ProposalDraft` jobs → `/api/approvals` → Telegram notify | Working end-to-end, but the source is mock |

### Estate side (outside this repo)

| Repo | What it is | Status |
|---|---|---|
| `capability-tenant-infra` (CTI, "SoloUnicorn") | Tenant × capability engine: `ctx = Resolver().resolve(tenant, capability)`. Self-hosted Windmill (workspace-per-tenant) + Directus + Supabase, one Postgres cluster. Tenant zero `aiautobiz.ir`, tenant one `rahgar-com`. Sibling product `parvandeh` (multi-tenant patient filing via Bale) live on it. | Mid-port from `m4` to `ubu` (active branch `feat/ubu-portability-dry-run`) — not dead, not "narrow work happening on it," in motion |
| `skill-eco` | Skill curation: 71 house skills (proven) + 451 community-library skills (raw, machine-proposed/human-verdicted via a Postgres constraint), 7 business-function categories, dispatched via a Windmill `skill_runner`, scoped per tenant, budget-capped, metered | Real schema live, business catalog written; crawl/curation lab still unwritten |
| `autobiz-kit` | The "island" model — one independent stack per client: `core/` (versioned package) + `instance/business.config.ts` (single edit point) + idempotent `provision/` | 110 tests green; never connected to a real Telegram/Cloudflare API |
| `~/infra` | Agent-operable control plane: MCP server + judgment skill (`~/.claude/skills/infra/SKILL.md`) + secrets (`~/.config/infra.json`) — the human interface (CLI) and machine interface (MCP) call the same code | Cloudflare zones/DNS live; Access/Email Routing/n8n/Telegram declared next |
| `mbaneshi-ir-site` | Astro 7 + Tailwind v4, EN/FA/AR. **Git is the CMS** — `src/content/{projects,cases,now,posts}/{locale}/*.md`, Zod schema per collection, `draft: boolean` gate. Own gitflow: issue branch off `dev` → PR → merge → `scripts/promote` tags `main` → `scripts/deploy` | Live at `mbaneshi.ir` (verified 200 this session) |

**The estate's own unresolved fork** (from the 2026-08-11 spec, still open): island (cheap, Worker+D1, per-client) vs. full-stack (Postgres+Directus+Windmill, real VPS cost) per client. Likely resolution already written down: **hybrid** — light island per client (delivery) + one heavy shared control plane (CTI: capability map, labs, orchestration, catalogue) + Cloudflare Access gating each client into their slice. RUSVEL's role, below, is designed to sit inside that hybrid, not replace it.

---

## 3. RUSVEL's role, precisely

**Narrow in what it does, broad in where it reaches.** This is not a new shape — `docs/design/vision.md` already states philosophy #2: *"Ports & adapters — the core is pure Rust traits with zero framework deps. Everything pluggable. Swap Claude for Ollama. Swap SQLite for Supabase."* The change is pointing that same discipline at the estate instead of only at RUSVEL's own internals:

- **CTI** is the *hands* — per-tenant execution (Windmill jobs, Directus data, Supabase state).
- **RUSVEL** is the *coordination layer* above all tenants — holds cross-tenant context, decides what needs escalation, is the one place a human sees status and pulls the approval-gate trigger (ADR-008).
- **The LLM underneath** (Claude, Grok, whichever) stays swappable reasoning inside both layers — this part is unchanged from the locked ledger decision: RUSVEL still does not rebuild the generic agent-orchestration loop Anthropic/xAI/the OSS `company-os` ecosystem all ship natively.

This also matches the recurring shape found industry-wide this session (Grok Bot's Chief-of-Staff bot + specialists; `simonlin1212/Agent-Staff`'s CEO 参谋长 over department agents; Departments of One's Desk 01). RUSVEL's seat in that pattern is specifically the chief-of-staff seat, not the department-specialist seat.

---

## 4. The one real gap

`dept-*`/`DepartmentApp` assumes **tenant = self**. There is no axis for "this role, configured differently per venture" (`mbaneshi.ir` vs. a future foreign venture vs. a future client). Everything else asked of RUSVEL this session — adaptable, configurable, extensible, multiple roles tailored per initiative — is already true of its ports; only this axis is missing to make it literal instead of aspirational.

**One live caution for whenever this gets wired to CTI:** CTI's admin panel connects through a Postgres role (open ADR `d-021`) that bypasses row-level tenant isolation — safe today only because nothing public reaches it. Any RUSVEL port into CTI must go through the normal tenant-scoped path, never that role.

**A structural convention worth preserving:** the estate's three infra repos (`arvan-infra`, `ubu-infra`, `m4-infra`) are deliberately kept separate per machine — the user explicitly rejected merging them into one shared config folder once already (2026-08-29, "نقشهٔ رله"). If RUSVEL ever needs its own per-machine infra config, follow the same discipline rather than centralizing.

---

## 5. Candidate ports

Three new ports, none of them reimplementing what's behind them:

| Port | Talks to | Purpose |
|---|---|---|
| `CapabilityTenantInfraPort` | CTI's Windmill/Directus/Supabase, per tenant | Read/write execution state without touching the `d-021` bypass role |
| `SkillEcoPort` | skill-eco's Postgres/Directus | Pull `verified` skills per business-function category (skill-eco's 7 categories map closely onto RUSVEL's ~13 departments) instead of RUSVEL re-curating skills itself |
| `IslandPort` | `autobiz-kit` per-client islands | Drive a client's cheap delivery surface without RUSVEL owning it |

None of these are scoped in detail yet — that's plan-phase work, once the tenant axis (§4) exists to hang them on.

---

## 6. First real venture: `mbaneshi.ir` — content pipeline

Chosen deliberately as the first case to prove the abstraction against, rather than building the abstraction speculatively first. Repo already exists (`mbaneshi-ir-site`, GitHub, live), no new repo needed.

**The gap is one adapter, not a pipeline.** RUSVEL already drafts content (`content-engine::writer`, `code_bridge`) and the instance already has a real schema and gitflow (§2 table). Nothing today takes a RUSVEL-drafted item and writes it as a correctly-schemed `.md` file into `src/content/posts/{locale}/`, then opens it as a branch+PR in the instance's own flow.

**Open question, not yet decided:**
- **(a)** Extend `rusvel content from-code` in this repo with `--target-repo <path>`: RUSVEL reaches out and writes the file + opens the branch/PR itself. Keeps the git-writing logic reusable for future instances — the more estate-abstraction-aligned choice.
- **(b)** A small script *inside* `mbaneshi-ir-site` that calls RUSVEL's API/CLI and formats the result locally. Keeps the instance fully self-contained per its own "don't grow `web` into a monolith" rule; needs zero changes to RUSVEL.

---

## 7. Content pipeline — additional inputs gathered, not yet wired

- **AvalAI** — LLM gateway already owned/used, exposing text/audio/video/image generation. Already proven live: it's the caption-generation service behind `houshkar.ir` today (`src/avalai.js`). Broader than `content-engine`'s current text-only path. Candidate: a new adapter/port (either an `LlmPort` impl if chat-shaped, or a new multi-modal generation port) — not a bespoke build per venture, since it's already the shared generation layer behind one live product.
- **`mbaneshi-insta`** — existing repo, Instagram DM automation Worker (Cloudflare). A distribution channel, Instagram-specific.
- **Postiz** — self-hosted social scheduler, already forked and extended as `postiz-ecosystem` inside CTI. Broader multi-platform scheduled posting, distinct from `mbaneshi-insta`.
- **Real, external client demand:** "other people I work with also ask for social media management" — not hypothetical. Validates skill-eco's own "Content & Communication" category and means whatever gets built here should assume a second real tenant from the start, not retrofit multi-tenancy later.
- **`houshkar.ir`'s Zibal/Kavenegar/AvalAI stack** is independent confirmation that "swap in a proven, already-live service adapter" is already the house pattern in practice, not just a preference from this thread.

---

## 8. Second priority: outreach, leads, follow-up

Unlike content, this maps onto RUSVEL code that's **already more mature** — `harvest-engine` and `gtm-engine` (§2 table) have real domain logic today; the gap is a real `HarvestSource` implementation, not the pipeline shape.

**Domestic vs. non-domestic lead sourcing need fundamentally different approaches** (user's framing, not a technical convenience):

| Category | Named platforms |
|---|---|
| Domestic (Iran) | Divar (دیوار), Balad (بلد), Shypoor (شیپور), Neshan (نشان), Bale (بله), Rubika (روبیکا) |
| Cross-cutting, both categories | LinkedIn, Google Maps, Instagram, Telegram |

This is the **third** independent appearance of the same domestic/foreign axis this session (after infra egress — Arvan vs. Cloudflare lane — and money — rial/Houshkar vs. hard-currency/zixlancer). That recurrence argues for making the tag a first-class property of the *source adapter itself* (`HarvestSource` implementations tagged domestic/non-domestic/both) rather than threading it through each engine ad hoc.

**Unconfirmed:** whether Departments of One's "digital-health scoring platform... produces ranked leads" refers to the `smart-harvester-backup` repo ("autonomous freelance job intelligence") or something else. No real lead source exists yet either way — nothing to wire until this is confirmed and a source is chosen.

---

## 9. Sequencing

1. **This spec** (done, this document).
2. **Lock a refined ledger decision**, superseding `strategy-rusvel-scope-vs-anthropic-native` with §3's sharper framing.
3. **File GitHub issue(s)** for the tenant-axis work, cross-linked to this spec and the locked decision (same pattern as issue #40).
4. **Close the loop on "Four Instruments, One Idea"** (English artifact + fa digest) with a resolution section — it currently ends on an open A/B/C decision; this document's conclusion is the answer.
5. **First code spike**, only once the issue exists: add the tenant axis to `dept-*`/`DepartmentApp` alone — no CTI/skill-eco/autobiz-kit wiring, no content or GTM adapters yet. Prove the shape compiles and the existing ~645 tests still pass before adding a single port.

None of steps 2–5 are authorized by this document alone — each needs an explicit go-ahead, per this session's own pattern (offered and declined twice already: "not yet... more discussion").

---

## 10. Open decisions (not resolved here)

- §6: adapter location for the `mbaneshi.ir` content bridge — (a) RUSVEL-side flag, or (b) instance-side script.
- §8: identity of the "digital-health scoring platform" — `smart-harvester-backup` or something else.
- §8: which real lead source (domestic or non-domestic) gets built first once confirmed.
- §7: whether AvalAI becomes an `LlmPort` impl or a new dedicated multi-modal port — needs a look at AvalAI's actual API shape before deciding.
- Whether RUSVEL's tenant axis reuses CTI's `Resolver`-style tenant identifiers directly, or defines its own that maps onto CTI's — not yet examined.

---

## Sources

- Ledger: `strategy-rusvel-scope-vs-anthropic-native` (`ledger.jsonl`, commit `0c64173`); issue `mbaneshi/rusvel#40`.
- Artifact: "Four Instruments, One Idea" — `https://claude.ai/code/artifact/c2c8d993-1315-402f-ac5b-3aa5fa398079`; fa digest `~/.claude/fa/2026-09-03-four-instruments-one-idea.html`.
- fa digests: 2051, 2050, 2233, 2217, 2210, 1946, 1909, 1936, 1941, 1268, 1251, 1255, 2203/2204 ("نقشهٔ رله"), 2078, 2201, 2045.
- Repos read this session: `capability-tenant-infra` (`/data/projects/capability-tenant-infra`), `skill-eco` (`~/skill-eco`), `meta-developer-eco` (`~/meta-developer-eco`), `mbaneshi-ir-site` (`/data/mbaneshi-ir-site`), `houshkar-ir-site` (`/data/houshkar-ir-site`), `new-infra` (`~/new-infra`).
- `docs/departments/gtm.md`, `docs/departments/harvest.md`, `docs/design/vision.md`, `docs/design/decisions.md` (this repo).
