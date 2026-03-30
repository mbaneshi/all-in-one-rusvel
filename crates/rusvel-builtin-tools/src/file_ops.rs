//! File operation tools: read_file, write_file, edit_file, glob, grep.

use std::path::PathBuf;
use std::sync::Arc;

use rusvel_core::domain::{Content, ToolDefinition, ToolResult};
use rusvel_tool::ToolRegistry;
use serde_json::json;

fn validate_path(path: &str) -> Result<PathBuf, rusvel_core::error::RusvelError> {
    let p = PathBuf::from(path);
    let canonical = if p.exists() {
        p.canonicalize()
    } else {
        p.parent()
            .unwrap_or(std::path::Path::new("."))
            .canonicalize()
            .map(|parent| parent.join(p.file_name().unwrap_or_default()))
    }
    .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("path validation: {e}")))?;

    let base = std::env::current_dir()
        .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("cwd: {e}")))?
        .canonicalize()
        .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("cwd canonicalize: {e}")))?;

    if !canonical.starts_with(&base) {
        return Err(rusvel_core::error::RusvelError::Tool(format!(
            "path escapes allowed base directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

pub async fn register(registry: &ToolRegistry) {
    // ── read_file ────────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "read_file".into(),
                description: "Read file contents from disk. Returns numbered lines (cat -n format).\n\n\
                    WHEN TO USE: Reading known files, inspecting code, checking config.\n\
                    WHEN NOT TO USE: Searching for files (use glob), searching content (use grep).\n\n\
                    TIPS:\n\
                    - For files >500 lines, use offset+limit to read sections\n\
                    - Returns error if file doesn't exist — check with glob first if unsure\n\
                    - Binary files return raw bytes (not useful) — skip them".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Absolute or relative path to the file"
                        },
                        "offset": {
                            "type": "integer",
                            "description": "Line number to start reading from (1-based). Optional."
                        },
                        "limit": {
                            "type": "integer",
                            "description": "Maximum number of lines to read. Optional."
                        }
                    },
                    "required": ["path"]
                }),
                searchable: false,
                metadata: json!({"category": "file", "read_only": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let path = validate_path(args["path"].as_str().unwrap_or(""))?;
                    let content = tokio::fs::read_to_string(&path).await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("read_file: {e}"))
                    })?;

                    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(1) as usize;
                    let limit = args
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as usize);

                    let lines: Vec<&str> = content.lines().collect();
                    let start = offset.saturating_sub(1).min(lines.len());
                    let end = match limit {
                        Some(l) => (start + l).min(lines.len()),
                        None => lines.len(),
                    };

                    let result: String = lines[start..end]
                        .iter()
                        .enumerate()
                        .map(|(i, line)| format!("{:>6}\t{line}", start + i + 1))
                        .collect::<Vec<_>>()
                        .join("\n");

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(result),
                        metadata: json!({"lines_read": end - start}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── write_file ───────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "write_file".into(),
                description: "Write content to a file (creates or overwrites).\n\n\
                    WHEN TO USE: Creating new files, full file rewrites.\n\
                    WHEN NOT TO USE: Small edits to existing files (use edit_file instead).\n\n\
                    TIPS:\n\
                    - Creates parent directories automatically\n\
                    - Overwrites entire file — use edit_file for targeted changes\n\
                    - Path must be within the project directory"
                    .into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to write"
                        },
                        "content": {
                            "type": "string",
                            "description": "The content to write"
                        }
                    },
                    "required": ["path", "content"]
                }),
                searchable: false,
                metadata: json!({"category": "file", "destructive": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let path = validate_path(args["path"].as_str().unwrap_or(""))?;
                    let content = args["content"].as_str().unwrap_or("");

                    if let Some(parent) = path.parent() {
                        tokio::fs::create_dir_all(parent).await.ok();
                    }

                    tokio::fs::write(&path, content).await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("write_file: {e}"))
                    })?;

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(format!(
                            "Wrote {} bytes to {}",
                            content.len(),
                            path.display()
                        )),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── edit_file ────────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "edit_file".into(),
                description: "Search-and-replace edit on a file. The old_string must match exactly one location.\n\n\
                    WHEN TO USE: Modifying existing code, updating config values, fixing bugs.\n\
                    WHEN NOT TO USE: Creating new files (use write_file), large rewrites (use write_file).\n\n\
                    TIPS:\n\
                    - old_string must be unique in the file — include surrounding context if needed\n\
                    - If found 0 times: check for whitespace/indentation differences\n\
                    - If found >1 times: add more surrounding lines to make it unique\n\
                    - Read the file first to get the exact text to match".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "path": {
                            "type": "string",
                            "description": "Path to the file to edit"
                        },
                        "old_string": {
                            "type": "string",
                            "description": "The exact text to find and replace"
                        },
                        "new_string": {
                            "type": "string",
                            "description": "The replacement text"
                        }
                    },
                    "required": ["path", "old_string", "new_string"]
                }),
                searchable: false,
                metadata: json!({"category": "file", "destructive": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let path = validate_path(args["path"].as_str().unwrap_or(""))?;
                    let old_string = args["old_string"].as_str().unwrap_or("");
                    let new_string = args["new_string"].as_str().unwrap_or("");

                    let content = tokio::fs::read_to_string(&path)
                        .await
                        .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("edit_file read: {e}")))?;

                    let count = content.matches(old_string).count();
                    if count == 0 {
                        return Err(rusvel_core::error::RusvelError::Tool(
                            "old_string not found in file".into(),
                        ));
                    }
                    if count > 1 {
                        return Err(rusvel_core::error::RusvelError::Tool(format!(
                            "old_string found {count} times — must be unique. Add more context."
                        )));
                    }

                    let new_content = content.replacen(old_string, new_string, 1);
                    tokio::fs::write(&path, &new_content)
                        .await
                        .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("edit_file write: {e}")))?;

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(format!("Edited {}: replaced 1 occurrence", path.display())),
                        metadata: json!({}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── glob ─────────────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "glob".into(),
                description: "Find files matching a glob pattern. Returns matching file paths.\n\n\
                    WHEN TO USE: Discovering files by name/extension, finding all files of a type.\n\
                    WHEN NOT TO USE: Searching file contents (use grep), reading a known file (use read_file).\n\n\
                    PATTERNS: **/*.rs (all Rust files), src/**/*.ts (TS in src/), Cargo.toml (exact name)\n\
                    TIPS:\n\
                    - Use ** for recursive matching across directories\n\
                    - Set 'path' to narrow the search base directory\n\
                    - Returns empty list if no matches — not an error".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Glob pattern (e.g. '**/*.rs', 'src/**/*.ts')"
                        },
                        "path": {
                            "type": "string",
                            "description": "Base directory to search from. Defaults to current directory."
                        }
                    },
                    "required": ["pattern"]
                }),
                searchable: false,
                metadata: json!({"category": "file", "read_only": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let pattern = args["pattern"].as_str().unwrap_or("");
                    let base_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let base = validate_path(base_str)?;

                    let full_pattern = if pattern.starts_with('/') {
                        pattern.to_string()
                    } else {
                        format!("{}/{pattern}", base.display())
                    };

                    let paths: Vec<String> = glob::glob(&full_pattern)
                        .map_err(|e| rusvel_core::error::RusvelError::Tool(format!("glob: {e}")))?
                        .filter_map(|entry| entry.ok())
                        .map(|p| p.display().to_string())
                        .collect();

                    let result = if paths.is_empty() {
                        "No files found".into()
                    } else {
                        paths.join("\n")
                    };

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(result),
                        metadata: json!({"count": paths.len()}),
                    })
                })
            }),
        )
        .await
        .unwrap();

    // ── grep ─────────────────────────────────────────────────────
    registry
        .register_with_handler(
            ToolDefinition {
                name: "grep".into(),
                description: "Search file contents for a regex pattern. Returns matching lines with file paths and line numbers.\n\n\
                    WHEN TO USE: Finding where a function/variable/string is used, searching code.\n\
                    WHEN NOT TO USE: Finding files by name (use glob), reading a known file (use read_file).\n\n\
                    TIPS:\n\
                    - Pattern is regex: use literal strings or regex syntax (e.g. 'fn\\s+main')\n\
                    - Set 'path' to search a specific directory/file\n\
                    - Use 'include' to filter file types (e.g. '*.rs')\n\
                    - Returns file:line:content format".into(),
                parameters: json!({
                    "type": "object",
                    "properties": {
                        "pattern": {
                            "type": "string",
                            "description": "Regex pattern to search for"
                        },
                        "path": {
                            "type": "string",
                            "description": "File or directory to search in. Defaults to current directory."
                        },
                        "glob_filter": {
                            "type": "string",
                            "description": "Glob pattern to filter files (e.g. '*.rs'). Optional."
                        }
                    },
                    "required": ["pattern"]
                }),
                searchable: false,
                metadata: json!({"category": "file", "read_only": true}),
            },
            Arc::new(|args| {
                Box::pin(async move {
                    let pattern = args["pattern"].as_str().unwrap_or("");
                    let path_str = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                    let path = validate_path(path_str)?;
                    let glob_filter = args.get("glob_filter").and_then(|v| v.as_str());

                    // Use ripgrep if available, fall back to grep.
                    let mut cmd = tokio::process::Command::new("rg");
                    cmd.arg("--line-number")
                        .arg("--no-heading")
                        .arg("--max-count=50");

                    if let Some(g) = glob_filter {
                        cmd.arg("--glob").arg(g);
                    }

                    cmd.arg(pattern).arg(&path);

                    let output = cmd.output().await.map_err(|e| {
                        rusvel_core::error::RusvelError::Tool(format!("grep: {e}"))
                    })?;

                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let result = if stdout.is_empty() {
                        "No matches found".into()
                    } else {
                        stdout.into_owned()
                    };

                    Ok(ToolResult {
                        success: true,
                        output: Content::text(result),
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

    /// Temp dir under process `current_dir` so `validate_path` (cwd-based sandbox) accepts paths.
    fn test_tempdir() -> tempfile::TempDir {
        tempfile::tempdir_in(std::env::current_dir().expect("cwd")).expect("tempdir")
    }

    fn extract_text(content: &Content) -> String {
        content
            .parts
            .iter()
            .filter_map(|p| match p {
                rusvel_core::domain::Part::Text(t) => Some(t.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[tokio::test]
    async fn read_file_works() {
        let dir = test_tempdir();
        let file = dir.path().join("test.txt");
        std::fs::write(&file, "line1\nline2\nline3\n").unwrap();

        let registry = ToolRegistry::new();
        register(&registry).await;

        let result = registry
            .call("read_file", json!({"path": file.to_str().unwrap()}))
            .await
            .unwrap();

        assert!(result.success);
        let text = extract_text(&result.output);
        assert!(text.contains("line1"));
        assert!(text.contains("line2"));
    }

    #[tokio::test]
    async fn write_and_read_roundtrip() {
        let dir = test_tempdir();
        let file = dir.path().join("out.txt");

        let registry = ToolRegistry::new();
        register(&registry).await;

        registry
            .call(
                "write_file",
                json!({"path": file.to_str().unwrap(), "content": "hello world"}),
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "hello world");
    }

    #[tokio::test]
    async fn edit_file_replaces() {
        let dir = test_tempdir();
        let file = dir.path().join("edit.txt");
        std::fs::write(&file, "foo bar baz").unwrap();

        let registry = ToolRegistry::new();
        register(&registry).await;

        registry
            .call(
                "edit_file",
                json!({
                    "path": file.to_str().unwrap(),
                    "old_string": "bar",
                    "new_string": "qux"
                }),
            )
            .await
            .unwrap();

        let content = std::fs::read_to_string(&file).unwrap();
        assert_eq!(content, "foo qux baz");
    }

    #[tokio::test]
    async fn glob_finds_files() {
        let dir = test_tempdir();
        std::fs::write(dir.path().join("a.rs"), "").unwrap();
        std::fs::write(dir.path().join("b.rs"), "").unwrap();
        std::fs::write(dir.path().join("c.txt"), "").unwrap();

        let registry = ToolRegistry::new();
        register(&registry).await;

        let result = registry
            .call(
                "glob",
                json!({"pattern": "*.rs", "path": dir.path().to_str().unwrap()}),
            )
            .await
            .unwrap();

        let text = extract_text(&result.output);
        assert!(text.contains("a.rs"));
        assert!(text.contains("b.rs"));
        assert!(!text.contains("c.txt"));
    }
}
