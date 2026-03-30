//! Which process backs [`rusvel_core::domain::ModelProvider::Claude`] at server boot.

fn env_truthy(name: &str) -> bool {
    std::env::var(name)
        .map(|v| {
            let t = v.trim().to_ascii_lowercase();
            matches!(t.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn anthropic_key_present() -> bool {
    std::env::var("ANTHROPIC_API_KEY")
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false)
}

/// When `true`, register [`crate::ClaudeCliProvider`] for Claude; otherwise [`crate::ClaudeProvider`].
///
/// - `RUSVEL_USE_CLAUDE_CLI=1` (or `true` / `yes` / `on`) forces the `claude -p` path even if
///   `ANTHROPIC_API_KEY` is set (e.g. key present but API credits exhausted — use Max subscription).
/// - Otherwise: HTTP API if the key is non-empty; CLI if the key is unset or blank.
pub fn claude_transport_is_cli() -> bool {
    if env_truthy("RUSVEL_USE_CLAUDE_CLI") {
        return true;
    }
    !anthropic_key_present()
}

/// `true` when [`claude_transport_is_cli`] is due to `RUSVEL_USE_CLAUDE_CLI`, not missing key.
pub fn claude_cli_forced_by_env() -> bool {
    env_truthy("RUSVEL_USE_CLAUDE_CLI")
}
