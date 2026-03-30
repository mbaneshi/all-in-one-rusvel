# Verification log — 2026-03-30

Evidence for claims in [`current-state.md`](current-state.md). Re-run the commands in **How to re-verify** there when updating metrics.

| Claim | Evidence |
|-------|----------|
| Workspace members | `cargo metadata --format-version 1 --no-deps` → `workspace_members` length **55**. |
| Rust LOC / files | `wc -l $(find crates -name '*.rs')` → **73,058** lines total; `find crates -name '*.rs' \| wc -l` → **301** files (counts only `crates/`, not `frontend/`). |
| Tests | `cargo test --workspace` → sum of `running N tests` lines **645**; **0 failures** (repo root, full workspace; `rusvel-api` must compile — Unix `CommandExt::exec` returns `io::Error` on Rust 1.93+). |
| Test executables | `cargo test --workspace --no-run 2>&1 \| rg '^  Executable' \| wc -l` → **102** (approximate compiled test target count). |
| HTTP `.route(` registrations | `rg '\.route\(' crates/rusvel-api/src/lib.rs \| wc -l` → **153** (main Axum API router in `build_router_with_frontend`). |
| API source files | `ls crates/rusvel-api/src/*.rs \| wc -l` → **40** including `lib.rs` → **39** handler/support modules beside `lib.rs`. |
| Port traits | `rg '^pub trait ' crates/rusvel-core/src/ports.rs \| wc -l` → **22**. |
| `pub struct` / `pub enum` in `domain.rs` | `rg '^pub (struct|enum) ' crates/rusvel-core/src/domain.rs \| wc -l` → **114**. |
| `cargo build` | **0** errors (workspace, 2026-03-30, re-verified after dev). |
