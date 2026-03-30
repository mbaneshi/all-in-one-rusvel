# 10 — Operations, configuration & supply chain

## Context (for next agents)

- **Binary:** `rusvel-app` embeds `frontend/build` via rust-embed; config via `rusvel-config` + env.
- **Supply chain:** Rust crates + pnpm lockfile; advisories (`cargo audit`, `pnpm audit` / OSV) as available in environment.
- **Inputs:** Flaky CI or test runtime from audit 08; logging/redaction themes from 03 and 06.

---

## Agent prompt (copy below)

```
Operations and supply-chain audit for RUSVEL.

Review: config loading and validation at boot; env var documentation vs usage; logging and secret redaction; release/build path (rust-embed); dependency advisories for Rust and Node.

Output in docs/audit/agents/10-operations-supply-chain.md Report:
- Executive summary.
- Findings with severity.
- Fix proposals (secrets management, SBOM/version policy, boot validation).
- Space for improvement suitable for a solo builder but industry-sane.

Final handoff: consolidated themes to merge with audits 01–09 for a single roadmap doc (suggest filename under docs/plans/).
```

---

## Report

### Executive summary

RUSVEL boots with **strong ADR-014 department graph validation** (unique IDs, dependency order) and **lenient TOML config** (parse-only; no schema or semantic checks). **Environment documentation is fragmented and partially wrong** versus code (SMTP password var name, documented-but-unimplemented `RUSVEL_DB_PATH` / `RUSVEL_SEED_DEV`). **Logging** uses standard `tracing` with `RUST_LOG`; there is **no dedicated secret-redaction layer**—LLM paths mostly log lengths, but agent/tool and hook paths can surface payload content at debug levels. **Release embedding** uses `rust-embed` on `frontend/build/` with a clear disk-then-embed fallback and temp extraction; empty or stale builds are an operational risk, not a secret leak. **Supply chain:** `pnpm audit` reports **1 low** (transitive `cookie`); `cargo audit` (**cargo-audit 0.22.1**, needs writable `~/.cargo` for DB fetch) reports **no critical CVEs** in this scan but flags **6 warnings** (unmaintained transitives + **RUSTSEC-2026-0002** unsound `lru` under `tantivy` / `ratatui` paths)—local output noted “allowed warnings,” likely from a user/global `cargo-audit` policy.

### Findings

| Severity | Topic | Evidence | Notes |
|----------|-------|----------|-------|
| **High** | Documented env vars not implemented in Rust | `PROJECT_CONTEXT.md`, `docs-site/src/reference/configuration.md` cite `RUSVEL_DB_PATH`; `docs/plans/fix-seed-data.md` cites `RUSVEL_SEED_DEV`; **no** `RUSVEL_DB_PATH` / `RUSVEL_SEED_DEV` in `crates/**/*.rs` | Operators may set vars expecting behavior that never runs; fix docs or implement. |
| **High** | Wrong SMTP secret var in canonical testing doc | `docs/testing/manual-testing-playbook.md`: `RUSVEL_SMTP_PASS` | Code in `gtm-engine/src/email.rs` uses `RUSVEL_SMTP_PASSWORD` (+ `RUSVEL_SMTP_USER`). `docs-site`/`docs/departments/gtm.md` use USERNAME/PASSWORD naming—closer but still inconsistent with `_USER`. |
| **Medium** | `config.toml` `log.level` not wired to tracing | `rusvel-config` `default_config()` sets `log.level`; `main.rs` uses only `EnvFilter::try_from_default_env()` (`RUST_LOG`) | Dead or misleading config key; either read `log.level` at boot or document that only `RUST_LOG` applies. |
| **Medium** | No systematic log redaction | Grep shows no redact/sanitize layer; `rusvel-agent` `content_to_plain` can include tool args/results; hook dispatch interpolates JSON payload into prompts | Align with themes from audits 03/06: structured logging + field allowlist or tracing `Layer` for sensitive keys. |
| **Medium** | Transitive **unsound** advisory (`lru`) | `cargo audit`: RUSTSEC-2026-0002 via `tantivy` → Lance stack and `ratatui` → TUI | Mitigation: upgrade when upstream publishes fixes; track Lance/ratatui releases; consider `cargo deny` policy. |
| **Low** | Transitive unmaintained crates | `cargo audit`: `instant`, `number_prefix`, `paste`, `rustls-pemfile`, `serial` (various trees: Lance, fastembed, terminal PTY) | No immediate CVE in scan; monitor and bump ecosystem deps periodically. |
| **Low** | Frontend advisory | `pnpm audit`: `cookie` below 0.7.0 via SvelteKit adapter chain (GHSA-pxg6-pf52-xh8x), severity **low** | Resolve when Kit bumps dependency or override if safe. |
| **Low** | `rust-embed` temp extraction path | `main.rs` `extract_embedded_frontend()` → `std::env::temp_dir().join("rusvel-frontend")` | Fixed dirname: concurrent versions could stomp; minor hardening = per-process subdir or content hash. |
| **Info** | Department boot vs config boot | `boot.rs`: `validate_unique_ids`, `resolve_dependency_order`; failed `register()` collected, not always fatal | Good for modularity; consider surfacing `failed_departments` prominently at startup in production. |
| **Info** | `cargo audit` in restricted environments | First run failed with advisory-db lock under `~/.cargo` (read-only sandbox) | CI/agents need `CARGO_HOME` writable or pre-seeded advisory DB. |

### Fix proposals

| ID | Proposal | Effort | Owner / area |
|----|----------|--------|----------------|
| F1 | **Secrets:** Add a short “runtime secrets” table in one canonical doc (`docs-site` reference + pointer from `CLAUDE.md`) listing all `RUSVEL_*`, `ANTHROPIC_*`, `OPENAI_*`, `OLLAMA_HOST`, `FLY_*`; remove or implement `RUSVEL_DB_PATH` / `RUSVEL_SEED_DEV`. | S | Docs + optional `rusvel-app` |
| F2 | **SMTP docs:** Replace `RUSVEL_SMTP_PASS` with `RUSVEL_SMTP_PASSWORD`; align USER vs USERNAME wording with code (`RUSVEL_SMTP_USER`). | S | Docs |
| F3 | **Boot validation:** Optional `TomlConfig::validate_known_keys()` or serde schema for known dotted keys; fail fast on unknown critical keys in strict mode (`RUSVEL_STRICT_CONFIG=1`). | M | `rusvel-config`, `rusvel-app` |
| F4 | **Logging:** Wire `log.level` from config when `RUST_LOG` unset; add tracing layer or structured field filtering for `Authorization`, `password`, `token`, `secret`. | M | `rusvel-app`, `rusvel-api` |
| F5 | **SBOM / version policy:** Run `cargo audit` (and optionally `cargo deny check`) in CI with writable cache; generate CycloneDX (`cargo cyclonedx`) on release tags; pin major bumps via PR checklist. | M | CI / release |
| F6 | **Embed path:** Document release checklist: `pnpm build` before `cargo build --release`; optional unique temp subdir for embedded extract. | S | Docs + `rusvel-app` |
| F7 | **Node supply chain:** Track SvelteKit/cookie advisory; `pnpm update` when upstream fixes land. | S | `frontend/` |

### Space for improvement

- **Single source of truth for env** — generate a fragment from a small Rust test or `grep` script in CI to prevent doc drift.
- **`InMemoryAuthAdapter::from_env`** — document `RUSVEL_KEY_*` next to API tokens in the same table; clarify behavior vs bearer auth (`RUSVEL_API_TOKEN`).
- **MCP HTTP** — `RUSVEL_MCP_HTTP_AUTH` / `RUSVEL_MCP_HTTP_TOKEN` are easy to miss; include in security hardening checklist.
- **Solo-builder balance:** full vault integration is optional; minimum bar is accurate docs, CI `cargo audit`, and no secret-shaped fields in default `info` logs.

### Handoff — consolidated roadmap suggestion

- **Suggested merge doc:** `docs/plans/audit-consolidated-roadmap-2026-03-30.md` (new file merging themes from audits **01–10**; alternatively extend `docs/plans/roadmap-v2.md` with an “Audit backlog” subsection if you want one canonical plan).
- **Themes to roll up from 01–09:** Carry forward **authZ middleware**, **test/CI flake** (audit 08), **logging/redaction** (03/06), **frontend/API consistency**, plus this file’s **env truth table**, **SBOM/audit CI**, and **transitive Rust advisories** (Lance/TUI/embed stacks).
