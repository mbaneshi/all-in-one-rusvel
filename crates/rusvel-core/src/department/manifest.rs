//! Declarative manifest — everything the host needs to know about a department
//! without executing any of its code.
//!
//! Analogous to `VSCode`'s `package.json#contributes` or Django's `AppConfig`.

use serde::{Deserialize, Serialize};

use crate::config::LayeredConfig;

// ════════════════════════════════════════════════════════════════════
//  DepartmentManifest — the stability surface
// ════════════════════════════════════════════════════════════════════

/// Complete declaration of a department's identity, contributions, and
/// dependencies. The host reads this before any department code runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DepartmentManifest {
    // ── Identity ─────────────────────────────────────────────────
    /// Unique department identifier (e.g. `"content"`, `"forge"`).
    pub id: String,

    /// Human-readable name (e.g. "Content Department").
    pub name: String,

    /// Short description for UI tooltips and docs.
    pub description: String,

    // ── Visual identity ──────────────────────────────────────────
    /// Icon character for CLI and UI (e.g. "#", "*", "§").
    pub icon: String,

    /// Color token for theming (e.g. "emerald", "purple").
    pub color: String,

    // ── Chat personality ─────────────────────────────────────────
    /// System prompt for this department's chat.
    pub system_prompt: String,

    /// Capability tags (e.g. `content_creation`, `code_analysis`).
    pub capabilities: Vec<String>,

    /// Quick-action buttons shown in the department panel.
    pub quick_actions: Vec<QuickAction>,

    // ── Contributions ────────────────────────────────────────────
    /// API routes this department registers.
    pub routes: Vec<RouteContribution>,

    /// CLI subcommands this department adds.
    pub commands: Vec<CommandContribution>,

    /// Agent tools this department provides.
    pub tools: Vec<ToolContribution>,

    /// Agent personas this department defines.
    pub personas: Vec<PersonaContribution>,

    /// Reusable prompt templates (skills).
    pub skills: Vec<SkillContribution>,

    /// System prompt rules injected during agent runs.
    pub rules: Vec<RuleContribution>,

    /// Job types this department processes.
    pub jobs: Vec<JobContribution>,

    /// Frontend UI declarations.
    pub ui: UiContribution,

    // ── Events ───────────────────────────────────────────────────
    /// Event kinds this department emits (e.g. "content.drafted").
    pub events_produced: Vec<String>,

    /// Event kinds this department subscribes to.
    pub events_consumed: Vec<String>,

    // ── Dependencies ─────────────────────────────────────────────
    /// Which core ports this department requires.
    pub requires_ports: Vec<PortRequirement>,

    /// Other department IDs this one depends on (soft deps).
    /// The host ensures these are registered first.
    pub depends_on: Vec<String>,

    // ── Configuration ────────────────────────────────────────────
    /// JSON Schema for department-specific settings.
    pub config_schema: serde_json::Value,

    /// Default layered config (model, effort, permissions, etc.).
    pub default_config: LayeredConfig,
}

impl DepartmentManifest {
    /// Builder-style constructor with required fields, defaulting the rest.
    pub fn new(id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: String::new(),
            icon: "?".into(),
            color: "slate".into(),
            system_prompt: String::new(),
            capabilities: Vec::new(),
            quick_actions: Vec::new(),
            routes: Vec::new(),
            commands: Vec::new(),
            tools: Vec::new(),
            personas: Vec::new(),
            skills: Vec::new(),
            rules: Vec::new(),
            jobs: Vec::new(),
            ui: UiContribution::default(),
            events_produced: Vec::new(),
            events_consumed: Vec::new(),
            requires_ports: Vec::new(),
            depends_on: Vec::new(),
            config_schema: serde_json::json!({}),
            default_config: LayeredConfig::default(),
        }
    }
}

// ════════════════════════════════════════════════════════════════════
//  Contribution types
// ════════════════════════════════════════════════════════════════════

/// A quick-action button in the department panel.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuickAction {
    pub label: String,
    pub prompt: String,
}

/// An API route this department contributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteContribution {
    pub method: String,
    pub path: String,
    pub description: String,
}

/// A CLI subcommand this department contributes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandContribution {
    pub name: String,
    pub description: String,
    pub args: Vec<ArgDef>,
}

/// A CLI argument definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgDef {
    pub name: String,
    pub description: String,
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

/// An agent tool this department provides.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContribution {
    /// Namespaced tool name (e.g. "content.draft").
    pub name: String,
    pub description: String,
    /// JSON Schema for tool parameters.
    pub parameters_schema: serde_json::Value,
}

/// An agent persona this department defines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonaContribution {
    pub name: String,
    pub role: String,
    pub default_model: String,
    pub allowed_tools: Vec<String>,
}

/// A reusable prompt template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillContribution {
    pub name: String,
    pub description: String,
    /// Template with `{{input}}` interpolation.
    pub prompt_template: String,
}

/// A system prompt rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleContribution {
    pub name: String,
    pub content: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// A job type this department processes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobContribution {
    /// Job kind string (e.g. "content.publish").
    pub kind: String,
    pub description: String,
    #[serde(default)]
    pub requires_approval: bool,
}

/// Frontend UI declarations.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiContribution {
    /// Tab IDs shown in the department panel.
    #[serde(default)]
    pub tabs: Vec<String>,

    /// Dashboard cards for the home page.
    #[serde(default)]
    pub dashboard_cards: Vec<DashboardCard>,

    /// Whether this department has a custom settings section.
    #[serde(default)]
    pub has_settings: bool,

    /// Custom Svelte component paths for specialized UI.
    #[serde(default)]
    pub custom_components: Vec<String>,
}

/// A card on the home dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardCard {
    pub title: String,
    pub description: String,
    /// Size hint: "small", "medium", "large".
    #[serde(default = "default_card_size")]
    pub size: String,
}

fn default_card_size() -> String {
    "medium".into()
}

/// Which port a department requires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortRequirement {
    /// Port name (e.g. `AgentPort`, `EventPort`).
    pub port: String,
    /// Whether the department can function without it.
    #[serde(default)]
    pub optional: bool,
}

// ════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_new_has_defaults() {
        let m = DepartmentManifest::new("test", "Test Department");
        assert_eq!(m.id, "test");
        assert_eq!(m.name, "Test Department");
        assert_eq!(m.icon, "?");
        assert_eq!(m.color, "slate");
        assert!(m.routes.is_empty());
        assert!(m.tools.is_empty());
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn manifest_serializes_roundtrip() {
        let m = DepartmentManifest {
            id: "content".into(),
            name: "Content".into(),
            description: "Content creation".into(),
            icon: "*".into(),
            color: "purple".into(),
            system_prompt: "You are the Content department.".into(),
            capabilities: vec!["content_creation".into()],
            quick_actions: vec![QuickAction {
                label: "Draft".into(),
                prompt: "Draft a post".into(),
            }],
            routes: vec![RouteContribution {
                method: "POST".into(),
                path: "/api/dept/content/draft".into(),
                description: "Draft content".into(),
            }],
            tools: vec![ToolContribution {
                name: "content.draft".into(),
                description: "Draft a blog post".into(),
                parameters_schema: serde_json::json!({"type": "object"}),
            }],
            personas: vec![PersonaContribution {
                name: "writer".into(),
                role: "Content writer".into(),
                default_model: "sonnet".into(),
                allowed_tools: vec!["content.draft".into()],
            }],
            events_produced: vec!["content.drafted".into()],
            events_consumed: vec!["code.analyzed".into()],
            requires_ports: vec![PortRequirement {
                port: "AgentPort".into(),
                optional: false,
            }],
            depends_on: vec![],
            ..DepartmentManifest::new("content", "Content")
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: DepartmentManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "content");
        assert_eq!(back.routes.len(), 1);
        assert_eq!(back.tools.len(), 1);
        assert_eq!(back.events_produced, vec!["content.drafted"]);
    }

    #[test]
    fn rule_contribution_defaults_enabled() {
        let json = r#"{"name":"test","content":"rule content"}"#;
        let rule: RuleContribution = serde_json::from_str(json).unwrap();
        assert!(rule.enabled);
    }

    #[test]
    fn port_requirement_defaults_required() {
        let json = r#"{"port":"AgentPort"}"#;
        let req: PortRequirement = serde_json::from_str(json).unwrap();
        assert!(!req.optional);
    }

    #[test]
    fn dashboard_card_defaults_medium() {
        let json = r#"{"title":"Test","description":"desc"}"#;
        let card: DashboardCard = serde_json::from_str(json).unwrap();
        assert_eq!(card.size, "medium");
    }
}
