//! `rusvel-tool` — [`ToolPort`] implementation for RUSVEL.
//!
//! Provides [`ToolRegistry`], a thread-safe tool registry that stores
//! tool definitions alongside async handler functions.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use futures::future::BoxFuture;

use rusvel_core::domain::{ToolDefinition, ToolPermission, ToolPermissionMode, ToolResult};
use rusvel_core::error::{Result, RusvelError};
use rusvel_core::ports::ToolPort;

// ════════════════════════════════════════════════════════════════════
//  Handler type
// ════════════════════════════════════════════════════════════════════

/// A boxed async function that receives JSON args and returns a [`ToolResult`].
pub type ToolHandler =
    Arc<dyn Fn(serde_json::Value) -> BoxFuture<'static, Result<ToolResult>> + Send + Sync>;

// ════════════════════════════════════════════════════════════════════
//  RegisteredTool
// ════════════════════════════════════════════════════════════════════

/// A tool definition paired with its handler function.
struct RegisteredTool {
    definition: ToolDefinition,
    handler: ToolHandler,
}

// ════════════════════════════════════════════════════════════════════
//  ToolRegistry
// ════════════════════════════════════════════════════════════════════

/// Thread-safe tool registry implementing [`ToolPort`].
///
/// Tools are stored in a `RwLock<HashMap>` so that `register` can use
/// `&self` (interior mutability) while remaining `Send + Sync`.
pub struct ToolRegistry {
    tools: RwLock<HashMap<String, RegisteredTool>>,
    permissions: RwLock<Vec<ToolPermission>>,
}

impl ToolRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            permissions: RwLock::new(Vec::new()),
        }
    }

    /// Add or replace a permission rule.
    pub fn set_permission(&self, permission: ToolPermission) {
        let mut perms = self.permissions.write().unwrap();
        // Replace existing rule with same pattern + department_id, or push new.
        if let Some(existing) = perms.iter_mut().find(|p| {
            p.tool_pattern == permission.tool_pattern && p.department_id == permission.department_id
        }) {
            *existing = permission;
        } else {
            perms.push(permission);
        }
    }

    /// Resolve the effective permission mode for a tool invocation.
    ///
    /// Priority: dept-specific exact > dept-specific prefix > dept-specific wildcard
    ///         > global exact > global prefix > global wildcard > Auto (default).
    pub fn check_permission(
        &self,
        tool_name: &str,
        department_id: Option<&str>,
    ) -> ToolPermissionMode {
        let perms = self.permissions.read().unwrap();

        // Score a pattern match: exact=3, prefix glob=2, wildcard=1, no match=0.
        let match_score = |pattern: &str, name: &str| -> u8 {
            if pattern == name {
                3
            } else if pattern == "*" {
                1
            } else if pattern.ends_with('*') && name.starts_with(&pattern[..pattern.len() - 1]) {
                2
            } else {
                0
            }
        };

        let mut best: Option<(u8, bool, &ToolPermissionMode)> = None; // (pattern_score, is_dept_specific, mode)

        for perm in perms.iter() {
            let score = match_score(&perm.tool_pattern, tool_name);
            if score == 0 {
                continue;
            }

            let is_dept = perm.department_id.is_some();
            let dept_matches = match (&perm.department_id, department_id) {
                (Some(pd), Some(dd)) => pd == dd,
                (Some(_), None) => false,
                (None, _) => true, // global rule applies to all
            };

            if !dept_matches {
                continue;
            }

            let better = match best {
                None => true,
                Some((prev_score, prev_dept, _)) => {
                    // dept-specific beats global; within same scope, higher score wins
                    (is_dept, score) > (prev_dept, prev_score)
                }
            };

            if better {
                best = Some((score, is_dept, &perm.mode));
            }
        }

        best.map(|(_, _, m)| m.clone())
            .unwrap_or(ToolPermissionMode::Auto)
    }

    /// Register a tool definition together with its async handler.
    ///
    /// If a tool with the same name already exists it is replaced.
    pub async fn register_with_handler(
        &self,
        definition: ToolDefinition,
        handler: ToolHandler,
    ) -> Result<()> {
        let name = definition.name.clone();
        let mut map = self.tools.write().unwrap();
        map.insert(
            name,
            RegisteredTool {
                definition,
                handler,
            },
        );
        Ok(())
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════
//  Basic JSON-schema validation
// ════════════════════════════════════════════════════════════════════

/// Validates that `args` is an object and that every `required` property
/// declared in `schema` is present. This is intentionally minimal.
fn validate_args(schema: &serde_json::Value, args: &serde_json::Value) -> Result<()> {
    // Args must be an object (or null → treat as empty object).
    let args_obj = match args {
        serde_json::Value::Object(m) => m,
        serde_json::Value::Null => return Ok(()),
        _ => {
            return Err(RusvelError::Validation(
                "tool arguments must be a JSON object".into(),
            ));
        }
    };

    // Check required fields if the schema declares them.
    if let Some(required) = schema.get("required").and_then(|v| v.as_array()) {
        for req in required {
            if let Some(key) = req.as_str()
                && !args_obj.contains_key(key)
            {
                return Err(RusvelError::Validation(format!(
                    "missing required parameter: {key}"
                )));
            }
        }
    }

    Ok(())
}

// ════════════════════════════════════════════════════════════════════
//  ToolPort implementation
// ════════════════════════════════════════════════════════════════════

#[async_trait]
impl ToolPort for ToolRegistry {
    /// Register a tool definition (without a handler).
    ///
    /// Tools registered this way will return a `Tool` error when called
    /// because no handler is attached. Prefer [`ToolRegistry::register_with_handler`].
    async fn register(&self, tool: ToolDefinition) -> Result<()> {
        let name = tool.name.clone();
        let mut map = self.tools.write().unwrap();
        // Only insert the definition if the tool isn't already registered
        // (preserves existing handler if re-registering definition).
        if map.contains_key(&name) {
            // Update definition, keep handler.
            map.get_mut(&name).unwrap().definition = tool;
        } else {
            let handler: ToolHandler = Arc::new(|_args| {
                Box::pin(async { Err(RusvelError::Tool("no handler registered".into())) })
            });
            map.insert(
                name,
                RegisteredTool {
                    definition: tool,
                    handler,
                },
            );
        }
        Ok(())
    }

    /// Look up a tool by name, validate args, check permissions, and call its handler.
    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolResult> {
        let (handler, schema) = {
            let map = self.tools.read().unwrap();
            let entry = map.get(name).ok_or_else(|| RusvelError::NotFound {
                kind: "tool".into(),
                id: name.into(),
            })?;
            (
                Arc::clone(&entry.handler),
                entry.definition.parameters.clone(),
            )
        };

        validate_args(&schema, &args)?;

        // Extract optional department context from args.
        let dept_id = args
            .get("__department_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_owned());

        match self.check_permission(name, dept_id.as_deref()) {
            ToolPermissionMode::Locked => {
                return Ok(ToolResult {
                    success: false,
                    output: rusvel_core::domain::Content::text(format!(
                        "tool '{name}' is locked and cannot be executed"
                    )),
                    metadata: serde_json::json!({"permission": "locked"}),
                });
            }
            ToolPermissionMode::Supervised => {
                return Ok(ToolResult {
                    success: false,
                    output: rusvel_core::domain::Content::text(
                        "Tool requires approval (supervised mode); not executed.",
                    ),
                    metadata: serde_json::json!({
                        "permission": "supervised",
                        "approval_required": true
                    }),
                });
            }
            ToolPermissionMode::Auto => {}
        }

        handler(args).await
    }

    /// Return definitions for all registered tools.
    fn list(&self) -> Vec<ToolDefinition> {
        let map = self.tools.read().unwrap();
        map.values().map(|r| r.definition.clone()).collect()
    }

    /// Search tools by query. Matches query words against name and description.
    /// Only returns tools marked `searchable: true`.
    fn search(&self, query: &str, limit: usize) -> Vec<ToolDefinition> {
        let query_lower = query.to_lowercase();
        let query_words: Vec<&str> = query_lower.split_whitespace().collect();
        if query_words.is_empty() {
            return vec![];
        }

        let map = self.tools.read().unwrap();
        let mut scored: Vec<(f64, &ToolDefinition)> = map
            .values()
            .filter(|r| r.definition.searchable)
            .filter_map(|r| {
                let name = r.definition.name.to_lowercase();
                let desc = r.definition.description.to_lowercase();
                let haystack = format!("{name} {desc}");

                let matches = query_words
                    .iter()
                    .filter(|w| haystack.contains(**w))
                    .count();

                if matches > 0 {
                    let score = matches as f64 / query_words.len() as f64;
                    Some((score, &r.definition))
                } else {
                    None
                }
            })
            .collect();

        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        scored
            .into_iter()
            .take(limit)
            .map(|(_, d)| d.clone())
            .collect()
    }

    /// Return the JSON Schema for a specific tool's parameters.
    fn schema(&self, name: &str) -> Option<serde_json::Value> {
        let map = self.tools.read().unwrap();
        map.get(name).map(|r| r.definition.parameters.clone())
    }
}

// ════════════════════════════════════════════════════════════════════
//  ScopedToolRegistry
// ════════════════════════════════════════════════════════════════════

/// A filtered view of a [`ToolPort`] that only exposes tools matching
/// allowed name prefixes or exact names.
///
/// Prefix patterns end with `*` (e.g. `"harvest_*"` matches all harvest tools).
/// Exact names must match fully.
pub struct ScopedToolRegistry {
    inner: Arc<dyn ToolPort>,
    allowed: Vec<String>,
}

impl ScopedToolRegistry {
    pub fn new(inner: Arc<dyn ToolPort>, allowed: Vec<String>) -> Self {
        Self { inner, allowed }
    }

    fn is_allowed(&self, name: &str) -> bool {
        if self.allowed.is_empty() {
            return true;
        }
        self.allowed.iter().any(|a| {
            if a.ends_with('*') {
                name.starts_with(&a[..a.len() - 1])
            } else {
                name == a
            }
        })
    }
}

#[async_trait]
impl ToolPort for ScopedToolRegistry {
    async fn register(&self, tool: ToolDefinition) -> Result<()> {
        self.inner.register(tool).await
    }

    async fn call(&self, name: &str, args: serde_json::Value) -> Result<ToolResult> {
        if !self.is_allowed(name) {
            return Err(RusvelError::NotFound {
                kind: "tool".into(),
                id: name.into(),
            });
        }
        self.inner.call(name, args).await
    }

    fn list(&self) -> Vec<ToolDefinition> {
        self.inner
            .list()
            .into_iter()
            .filter(|t| self.is_allowed(&t.name))
            .collect()
    }

    fn search(&self, query: &str, limit: usize) -> Vec<ToolDefinition> {
        self.inner
            .search(query, limit)
            .into_iter()
            .filter(|t| self.is_allowed(&t.name))
            .collect()
    }

    fn schema(&self, name: &str) -> Option<serde_json::Value> {
        if self.is_allowed(name) {
            self.inner.schema(name)
        } else {
            None
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use rusvel_core::domain::Content;
    use serde_json::json;

    fn echo_definition() -> ToolDefinition {
        ToolDefinition {
            name: "echo".into(),
            description: "Echoes the input message back".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            }),
            searchable: false,
            metadata: json!({}),
        }
    }

    fn echo_handler() -> ToolHandler {
        Arc::new(|args: serde_json::Value| {
            Box::pin(async move {
                let msg = args
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(empty)");
                Ok(ToolResult {
                    success: true,
                    output: Content::text(msg),
                    metadata: json!({}),
                })
            })
        })
    }

    #[tokio::test]
    async fn register_and_call() {
        let registry = ToolRegistry::new();
        registry
            .register_with_handler(echo_definition(), echo_handler())
            .await
            .unwrap();

        let result = registry
            .call("echo", json!({"message": "hello"}))
            .await
            .unwrap();

        assert!(result.success);
        match &result.output.parts[0] {
            rusvel_core::domain::Part::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("expected text part"),
        }
    }

    #[tokio::test]
    async fn call_missing_tool_returns_not_found() {
        let registry = ToolRegistry::new();
        let err = registry.call("nope", json!({})).await.unwrap_err();
        assert!(matches!(err, RusvelError::NotFound { .. }));
    }

    #[tokio::test]
    async fn call_with_missing_required_arg_returns_validation_error() {
        let registry = ToolRegistry::new();
        registry
            .register_with_handler(echo_definition(), echo_handler())
            .await
            .unwrap();

        let err = registry.call("echo", json!({})).await.unwrap_err();
        assert!(matches!(err, RusvelError::Validation(_)));
    }

    #[tokio::test]
    async fn list_returns_all_tools() {
        let registry = ToolRegistry::new();
        registry
            .register_with_handler(echo_definition(), echo_handler())
            .await
            .unwrap();

        let mut greet = echo_definition();
        greet.name = "greet".into();
        registry
            .register_with_handler(greet, echo_handler())
            .await
            .unwrap();

        let tools = registry.list();
        assert_eq!(tools.len(), 2);

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"greet"));
    }

    #[tokio::test]
    async fn schema_returns_parameters() {
        let registry = ToolRegistry::new();
        registry
            .register_with_handler(echo_definition(), echo_handler())
            .await
            .unwrap();

        let schema = registry.schema("echo").unwrap();
        assert_eq!(schema["type"], "object");
        assert!(
            schema["required"]
                .as_array()
                .unwrap()
                .contains(&json!("message"))
        );

        assert!(registry.schema("nonexistent").is_none());
    }

    #[tokio::test]
    async fn scoped_registry_filters_by_prefix() {
        let registry = Arc::new(ToolRegistry::new());
        registry
            .register_with_handler(echo_definition(), echo_handler())
            .await
            .unwrap();

        let mut greet = echo_definition();
        greet.name = "greet_hello".into();
        registry
            .register_with_handler(greet, echo_handler())
            .await
            .unwrap();

        let mut other = echo_definition();
        other.name = "other_tool".into();
        registry
            .register_with_handler(other, echo_handler())
            .await
            .unwrap();

        let scoped = ScopedToolRegistry::new(
            registry.clone() as Arc<dyn ToolPort>,
            vec!["echo".into(), "greet_*".into()],
        );

        let tools = scoped.list();
        assert_eq!(tools.len(), 2);
        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"echo"));
        assert!(names.contains(&"greet_hello"));
        assert!(!names.contains(&"other_tool"));

        // call allowed tool
        let result = scoped.call("echo", json!({"message": "hi"})).await.unwrap();
        assert!(result.success);

        // call blocked tool
        let err = scoped.call("other_tool", json!({"message": "hi"})).await;
        assert!(err.is_err());

        // schema allowed
        assert!(scoped.schema("echo").is_some());
        // schema blocked
        assert!(scoped.schema("other_tool").is_none());

        let scoped_all = ScopedToolRegistry::new(registry.clone() as Arc<dyn ToolPort>, vec![]);
        assert_eq!(scoped_all.list().len(), 3);
        let r = scoped_all.call("other_tool", json!({"message": "x"})).await.unwrap();
        assert!(r.success);
    }
}
