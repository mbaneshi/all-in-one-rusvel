//! Layered config — three-tier cascade: global → department → session.
//!
//! Each layer uses `Option<T>` fields. `None` means "inherit from parent".
//! `resolve()` merges a child layer onto a parent, producing a `ResolvedConfig`
//! with all fields guaranteed present.

use serde::{Deserialize, Serialize};

use crate::domain::ToolPermissionMode;

/// Per-department toggles for which sections appear in the session context pack (S-045).
/// Each field is optional; unset inherits from the parent layer in [`LayeredConfig::overlay`].
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextPackFlags {
    #[serde(default)]
    pub session_name: Option<bool>,
    #[serde(default)]
    pub goals: Option<bool>,
    #[serde(default)]
    pub events: Option<bool>,
    #[serde(default)]
    pub metrics: Option<bool>,
}

impl ContextPackFlags {
    #[must_use]
    pub fn overlay(&self, parent: &ContextPackFlags) -> ContextPackFlags {
        ContextPackFlags {
            session_name: self.session_name.or(parent.session_name),
            goals: self.goals.or(parent.goals),
            events: self.events.or(parent.events),
            metrics: self.metrics.or(parent.metrics),
        }
    }

    pub fn resolved(&self) -> ResolvedContextPackFlags {
        ResolvedContextPackFlags {
            session_name: self.session_name.unwrap_or(true),
            goals: self.goals.unwrap_or(true),
            events: self.events.unwrap_or(true),
            metrics: self.metrics.unwrap_or(false),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[allow(clippy::struct_excessive_bools)] // Resolved UI section toggles from layered config
pub struct ResolvedContextPackFlags {
    pub session_name: bool,
    pub goals: bool,
    pub events: bool,
    pub metrics: bool,
}

impl Default for ResolvedContextPackFlags {
    fn default() -> Self {
        Self {
            session_name: true,
            goals: true,
            events: true,
            metrics: false,
        }
    }
}

/// Effective context-pack section flags from a stored [`LayeredConfig`] (S-045).
pub fn resolve_context_pack_flags(layered: &LayeredConfig) -> ResolvedContextPackFlags {
    layered
        .context_pack
        .as_ref()
        .map(ContextPackFlags::resolved)
        .unwrap_or_default()
}

/// Config layer — any field set to None inherits from the parent layer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LayeredConfig {
    pub model: Option<String>,
    pub effort: Option<String>,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: Option<String>,
    pub allowed_tools: Option<Vec<String>>,
    pub disallowed_tools: Option<Vec<String>>,
    pub system_prompt: Option<String>,
    pub add_dirs: Option<Vec<String>>,
    pub max_turns: Option<u32>,
    #[serde(default)]
    pub context_pack: Option<ContextPackFlags>,
    /// `discuss` | `agentic` — steers system prompt (Claude-style modes); default discuss.
    #[serde(default)]
    pub chat_mode: Option<String>,
}

/// Fully resolved config — all fields present, ready to use.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedConfig {
    pub model: String,
    pub effort: String,
    pub max_budget_usd: Option<f64>,
    pub permission_mode: String,
    pub allowed_tools: Vec<String>,
    pub disallowed_tools: Vec<String>,
    pub system_prompt: String,
    pub add_dirs: Vec<String>,
    pub max_turns: Option<u32>,
    /// `discuss` or `agentic`.
    pub chat_mode: String,
}

impl LayeredConfig {
    /// Merge self on top of parent. Self's non-None values win.
    #[must_use]
    pub fn overlay(&self, parent: &LayeredConfig) -> LayeredConfig {
        LayeredConfig {
            model: self.model.clone().or(parent.model.clone()),
            effort: self.effort.clone().or(parent.effort.clone()),
            max_budget_usd: self.max_budget_usd.or(parent.max_budget_usd),
            permission_mode: self
                .permission_mode
                .clone()
                .or(parent.permission_mode.clone()),
            allowed_tools: self.allowed_tools.clone().or(parent.allowed_tools.clone()),
            disallowed_tools: self
                .disallowed_tools
                .clone()
                .or(parent.disallowed_tools.clone()),
            system_prompt: self.system_prompt.clone().or(parent.system_prompt.clone()),
            add_dirs: self.add_dirs.clone().or(parent.add_dirs.clone()),
            max_turns: self.max_turns.or(parent.max_turns),
            context_pack: match (&self.context_pack, &parent.context_pack) {
                (Some(a), Some(b)) => Some(a.overlay(b)),
                (Some(a), None) => Some(a.clone()),
                (None, Some(b)) => Some(b.clone()),
                (None, None) => None,
            },
            chat_mode: self.chat_mode.clone().or(parent.chat_mode.clone()),
        }
    }

    /// Resolve to a concrete config, filling in defaults for anything still None.
    pub fn resolve(&self) -> ResolvedConfig {
        ResolvedConfig {
            model: self.model.clone().unwrap_or_else(|| "sonnet".into()),
            effort: self.effort.clone().unwrap_or_else(|| "medium".into()),
            max_budget_usd: self.max_budget_usd,
            permission_mode: self
                .permission_mode
                .clone()
                .unwrap_or_else(|| "plan".into()),
            allowed_tools: self.allowed_tools.clone().unwrap_or_default(),
            disallowed_tools: self.disallowed_tools.clone().unwrap_or_default(),
            system_prompt: self.system_prompt.clone().unwrap_or_default(),
            add_dirs: self.add_dirs.clone().unwrap_or_default(),
            max_turns: self.max_turns,
            chat_mode: self.chat_mode.clone().unwrap_or_else(|| "discuss".into()),
        }
    }
}

impl ResolvedConfig {
    /// Convert to Claude CLI args.
    pub fn to_claude_args(&self) -> Vec<String> {
        let mut args = vec![
            "--model".into(),
            self.model.clone(),
            "--effort".into(),
            self.effort.clone(),
            "--permission-mode".into(),
            self.permission_mode.clone(),
        ];
        if let Some(budget) = self.max_budget_usd {
            args.extend(["--max-budget-usd".into(), budget.to_string()]);
        }
        if !self.allowed_tools.is_empty() {
            args.extend(["--allowedTools".into(), self.allowed_tools.join(" ")]);
        }
        if !self.disallowed_tools.is_empty() {
            args.extend(["--disallowedTools".into(), self.disallowed_tools.join(" ")]);
        }
        for dir in &self.add_dirs {
            args.extend(["--add-dir".into(), dir.clone()]);
        }
        if let Some(turns) = self.max_turns {
            args.extend(["--max-turns".into(), turns.to_string()]);
        }
        args
    }
}

/// Map stored department [`ResolvedConfig::permission_mode`] string to agent tool policy.
pub fn tool_permission_mode_from_dept_config_str(s: &str) -> ToolPermissionMode {
    match s.to_ascii_lowercase().as_str() {
        "supervised" => ToolPermissionMode::Supervised,
        "locked" => ToolPermissionMode::Locked,
        _ => ToolPermissionMode::Auto,
    }
}

/// Resolve three layers: global → department → session.
pub fn resolve_cascade(
    global: &LayeredConfig,
    dept: &LayeredConfig,
    session: &LayeredConfig,
) -> ResolvedConfig {
    let with_global = global.overlay(&LayeredConfig::default());
    let merged = dept.overlay(&with_global);
    let merged = session.overlay(&merged);
    merged.resolve()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_resolves_to_sonnet() {
        let config = LayeredConfig::default().resolve();
        assert_eq!(config.model, "sonnet");
        assert_eq!(config.effort, "medium");
        assert_eq!(config.permission_mode, "plan");
        assert_eq!(config.chat_mode, "discuss");
    }

    #[test]
    fn overlay_child_wins() {
        let parent = LayeredConfig {
            model: Some("sonnet".into()),
            effort: Some("low".into()),
            ..Default::default()
        };
        let child = LayeredConfig {
            model: Some("opus".into()),
            ..Default::default()
        };
        let merged = child.overlay(&parent);
        assert_eq!(merged.model, Some("opus".into()));
        assert_eq!(merged.effort, Some("low".into())); // inherited
    }

    #[test]
    fn three_layer_cascade() {
        let global = LayeredConfig {
            model: Some("sonnet".into()),
            effort: Some("medium".into()),
            ..Default::default()
        };
        let dept = LayeredConfig {
            effort: Some("high".into()),
            add_dirs: Some(vec![".".into()]),
            ..Default::default()
        };
        let session = LayeredConfig {
            model: Some("opus".into()),
            ..Default::default()
        };
        let resolved = resolve_cascade(&global, &dept, &session);
        assert_eq!(resolved.model, "opus"); // session wins
        assert_eq!(resolved.effort, "high"); // dept wins over global
        assert_eq!(resolved.add_dirs, vec!["."]); // dept, session didn't set
    }

    #[test]
    fn to_claude_args_produces_valid_args() {
        let config = ResolvedConfig {
            model: "opus".into(),
            effort: "high".into(),
            max_budget_usd: Some(1.0),
            permission_mode: "default".into(),
            allowed_tools: vec![],
            disallowed_tools: vec!["bash".into()],
            system_prompt: String::new(),
            add_dirs: vec![".".into()],
            max_turns: None,
            chat_mode: "discuss".into(),
        };
        let args = config.to_claude_args();
        assert!(args.contains(&"--model".to_string()));
        assert!(args.contains(&"opus".to_string()));
        assert!(args.contains(&"--add-dir".to_string()));
    }
}
