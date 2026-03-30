//! Git tools: git_status, git_diff, git_log.

use std::sync::Arc;

use rusvel_core::domain::{Content, ToolDefinition, ToolResult};
use rusvel_tool::ToolRegistry;
use serde_json::json;

pub async fn register(registry: &ToolRegistry) {
    // ── git_status ───────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "git_status".into(),
                description:
                    "Show git working tree status (staged, unstaged, untracked files).\n\n\
                    USE: Check what's changed before committing, see if files are tracked.\n\
                    OUTPUT: Short format — M=modified, A=added, ??=untracked."
                        .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Repository path. Defaults to current directory."
                        }
                    }
                }),
                searchable: false,
                metadata: json!({"category": "git", "read_only": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let output = tokio::process::Command::new("git")
                        .args(["status", "--short"])
                        .current_dir(path)
                        .output()
                        .await
                        .map_err(|e| {
                            rusvel_core::error::RusvelError::Tool(format!("git_status: {e}"))
                        })?;

                    let text = String::from_utf8_lossy(&output.stdout);
                    let result = if text.is_empty() {
                        "Working tree clean".into()
                    } else {
                        text.into_owned()
                    };

                    Ok(ToolResult {
                        success: output.status.success(),
                        output: Content::text(result),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── git_diff ─────────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "git_diff".into(),
                description:
                    "Show git diff — changes between commits, staged/unstaged changes.\n\n\
                    USE: See what code changed, review before committing.\n\
                    ARGS: 'args' can be '--staged', 'HEAD~1', a file path, or a commit range.\n\
                    TIPS: No args = unstaged changes. '--staged' = what's about to be committed."
                        .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Repository path. Defaults to current directory."
                        },
                        "staged": {
                            "type": "boolean",
                            "description": "Show staged changes (--cached). Default: false."
                        }
                    }
                }),
                searchable: false,
                metadata: json!({"category": "git", "read_only": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let staged = args
                        .get("staged")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let mut cmd = tokio::process::Command::new("git");
                    cmd.arg("diff").current_dir(path);
                    if staged {
                        cmd.arg("--cached");
                    }

                    let output = cmd.output().await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("git_diff: {e}"))
                    })?;

                    let text = String::from_utf8_lossy(&output.stdout);
                    let result = if text.is_empty() {
                        "No changes".into()
                    } else {
                        text.into_owned()
                    };

                    Ok(ToolResult {
                        success: output.status.success(),
                        output: Content::text(result),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── git_log ──────────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "git_log".into(),
                description: "Show recent git commit history (oneline format).\n\n\
                    USE: Understand recent changes, find commit hashes, check commit style.\n\
                    TIPS: Default 10 commits. Use 'count' param for more. Shows hash + message."
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Repository path. Defaults to current directory."
                        },
                        "count": {
                            "type": "integer",
                            "description": "Number of commits to show. Default: 10."
                        }
                    }
                }),
                searchable: false,
                metadata: json!({"category": "git", "read_only": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let count = args.get("count").and_then(|v| v.as_u64()).unwrap_or(10);

                    let output = tokio::process::Command::new("git")
                        .args(["log", "--oneline", &format!("-{count}")])
                        .current_dir(path)
                        .output()
                        .await
                        .map_err(|e| {
                            rusvel_core::error::RusvelError::Tool(format!("git_log: {e}"))
                        })?;

                    let text = String::from_utf8_lossy(&output.stdout);
                    Ok(ToolResult {
                        success: output.status.success(),
                        output: Content::text(text.into_owned()),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::ports::ToolPort;

    #[tokio::test]
    async fn git_status_runs() {
        let registry = ToolRegistry::new();
        register(&registry).await;

        // Should work in the repo root.
        let result = registry.call("git_status", json!({})).await.unwrap();
        assert!(result.success);
    }

    #[tokio::test]
    async fn git_log_runs() {
        let registry = ToolRegistry::new();
        register(&registry).await;

        let result = registry.call("git_log", json!({"count": 3})).await.unwrap();
        assert!(result.success);
    }
}
