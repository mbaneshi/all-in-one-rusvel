# `rusvel-evals` fixture cassettes

Recorded LLM responses replayed by [`ScriptedAgent`](../src/stubs.rs). Each
file here is included by an eval via `include_str!`, so adding a new
fixture is a two-line code change *plus* a new file in this directory.

| File                     | Used by                              | Format                |
| ------------------------ | ------------------------------------ | --------------------- |
| `forge_daily_plan.json`  | `src/fixtures/forge.rs`              | JSON (plan response)  |
| `content_tweet.md`       | `src/fixtures/content.rs`            | Markdown (tweet body) |

Evals that do **not** need a recorded LLM response (harvest, code, flow)
exercise deterministic non-LLM paths (keyword scoring, tree-sitter parse,
code-node DAG) and have no entry here by design.
