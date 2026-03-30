# Repository status

The **canonical** metrics, feature inventory, and gaps for the RUSVEL codebase live in the main repository under `docs/`. This page summarizes what shipped as of the last audit and links to the full Markdown (always current on `main`).

## Canonical sources (GitHub)

| Document | Purpose |
|----------|---------|
| [docs/status/current-state.md](https://github.com/mbaneshi/rusvel/blob/main/docs/status/current-state.md) | **Single written source of truth** — numbers, what works E2E, gaps, re-verify commands |
| [docs/README.md](https://github.com/mbaneshi/rusvel/blob/main/docs/README.md) | Documentation index — which folder is truth vs plans vs scratch |
| [docs/status/verification-log-2026-03-30.md](https://github.com/mbaneshi/rusvel/blob/main/docs/status/verification-log-2026-03-30.md) | Claim → evidence for the latest metrics snapshot |

When this mdBook page and the repo diverge, **trust the repo files above** and refresh the tables below on the next audit.

---

## Metric definitions (abbrev.)

| Term | Meaning |
|------|---------|
| **Workspace members** | Packages in root `Cargo.toml` `[workspace].members` |
| **HTTP route chains** | Lines with `.route(` in `crates/rusvel-api/src/lib.rs` (one line can register `get().post()`) |
| **API modules** | `*.rs` files in `rusvel-api/src/` except `lib.rs` |

---

## Numbers at a glance (2026-03-30)

| Metric | Count |
|--------|------:|
| Workspace members | 55 |
| Rust LOC (`crates/*.rs`) | ~73,058 |
| Rust source files (`crates/`) | 301 |
| Tests (approx., full `cargo test`) | ~645 |
| HTTP route chains in API router | 153 |
| API modules (`rusvel-api`, excl. `lib.rs`) | 39 |
| Port traits (`rusvel-core/src/ports.rs`) | 22 |
| Departments / `dept-*` crates | 14 / 14 |
| Engines | 13 (6 wired + 7 skeletons) |

---

## Gaps (explicit)

- **GTM / CRM depth** — OutreachSend job path is wired (approval-gated, `gtm-engine`); more CRM surfaces and polish remain.
- **Auth** — not full API middleware; env/in-memory style for many paths.
- **Depth** — Several business engines are thinner than Forge/Code/Content/Harvest/Flow; department chat still works via `DepartmentApp`.

---

## How to re-verify

```bash
cargo build
cargo test
cargo metadata --format-version 1 --no-deps | python3 -c "import json,sys; print(len(json.load(sys.stdin)['workspace_members']))"
find crates -name '*.rs' | wc -l
wc -l $(find crates -name '*.rs') | tail -1
rg '\.route\(' crates/rusvel-api/src/lib.rs | wc -l
```

Some tests (e.g. terminal PTY) may fail in restricted environments.
