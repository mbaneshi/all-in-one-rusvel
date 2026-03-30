//! Static manifest for the Code Department.
//!
//! No side effects — pure data declaration.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_CODE;
use rusvel_core::department::*;

pub fn code_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_CODE.into(),
        name: "Code Department".into(),
        description: "Code intelligence, implementation, debugging, testing, refactoring".into(),
        icon: "#".into(),
        color: "emerald".into(),

        system_prompt: concat!(
            "You are the Code department of RUSVEL.\n\n",
            "You have full access to Claude Code tools:\n",
            "- Read, Write, Edit files across all project directories\n",
            "- Run shell commands (build, test, git, pnpm, cargo, etc.)\n",
            "- Search codebases with grep and glob\n",
            "- Fetch web content and search the web\n",
            "- Spawn sub-agents for parallel work\n",
            "- Manage background tasks\n\n",
            "Focus: code intelligence, implementation, debugging, testing, refactoring.\n",
            "When writing code, follow existing patterns. Be thorough.",
        )
        .into(),

        capabilities: vec!["code_analysis".into(), "tool_use".into()],

        quick_actions: vec![
            QuickAction {
                label: "Analyze codebase".into(),
                prompt: "Analyze the codebase structure, dependencies, and code quality.".into(),
            },
            QuickAction {
                label: "Run tests".into(),
                prompt: "Run `cargo test` and report results. If any fail, show the errors.".into(),
            },
            QuickAction {
                label: "Find TODOs".into(),
                prompt: "Find all TODO, FIXME, and HACK comments across the codebase.".into(),
            },
            QuickAction {
                label: "Self-improve".into(),
                prompt: "Read docs/status/current-state.md and docs/status/gap-analysis.md. Identify the highest-impact fix you can make right now. Implement it, run tests, and verify.".into(),
            },
            QuickAction {
                label: "Fix build warnings".into(),
                prompt: "Run `cargo build` and fix any warnings. Then run `cargo test` to verify nothing broke.".into(),
            },
        ],

        routes: vec![
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/code/analyze".into(),
                description: "Analyze a codebase directory".into(),
            },
            RouteContribution {
                method: "GET".into(),
                path: "/api/dept/code/search".into(),
                description: "Search indexed symbols".into(),
            },
        ],

        commands: vec![
            CommandContribution {
                name: "analyze".into(),
                description: "Analyze a codebase directory".into(),
                args: vec![ArgDef {
                    name: "path".into(),
                    description: "Path to analyze".into(),
                    required: false,
                    default: Some(".".into()),
                }],
            },
            CommandContribution {
                name: "search".into(),
                description: "Search indexed symbols".into(),
                args: vec![ArgDef {
                    name: "query".into(),
                    description: "Search query".into(),
                    required: true,
                    default: None,
                }],
            },
        ],

        tools: vec![
            ToolContribution {
                name: "code.analyze".into(),
                description: "Analyze a codebase directory for symbols, metrics, and dependencies"
                    .into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "Directory path to analyze" }
                    },
                    "required": ["path"]
                }),
            },
            ToolContribution {
                name: "code.search".into(),
                description: "Search previously indexed code symbols".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query" },
                        "limit": { "type": "integer", "description": "Max results", "default": 10 }
                    },
                    "required": ["query"]
                }),
            },
        ],

        personas: vec![PersonaContribution {
            name: "code-engineer".into(),
            role: "Senior software engineer with code intelligence".into(),
            default_model: "sonnet".into(),
            allowed_tools: vec![
                "code.analyze".into(),
                "code.search".into(),
                "file_read".into(),
                "file_write".into(),
                "shell".into(),
            ],
        }],

        skills: vec![SkillContribution {
            name: "Code Review".into(),
            description: "Analyze code quality and suggest improvements".into(),
            prompt_template: concat!(
                "Analyze the code at: {{path}}\n\n",
                "Focus on: code quality, patterns, potential bugs, and improvements."
            )
            .into(),
        }],

        rules: vec![],

        jobs: vec![JobContribution {
            kind: "code.analyze".into(),
            description: "Run code analysis on a directory".into(),
            requires_approval: false,
        }],

        ui: UiContribution {
            tabs: vec![
                "actions".into(),
                "engine".into(),
                "agents".into(),
                "workflows".into(),
                "skills".into(),
                "rules".into(),
                "mcp".into(),
                "hooks".into(),
                "dirs".into(),
                "events".into(),
            ],
            dashboard_cards: vec![DashboardCard {
                title: "Code Intelligence".into(),
                description: "Symbol index, metrics, and search".into(),
                size: "medium".into(),
            }],
            has_settings: true,
            custom_components: vec![],
        },

        events_produced: vec!["code.analyzed".into(), "code.searched".into()],
        events_consumed: vec![],

        requires_ports: vec![
            PortRequirement {
                port: "StoragePort".into(),
                optional: false,
            },
            PortRequirement {
                port: "EventPort".into(),
                optional: false,
            },
        ],

        depends_on: vec![],

        config_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "default_path": { "type": "string", "description": "Default directory to analyze" }
            }
        }),

        default_config: LayeredConfig {
            model: None,
            effort: Some("high".into()),
            permission_mode: Some("default".into()),
            add_dirs: Some(vec![".".into()]),
            ..Default::default()
        },

        terminal: DepartmentTerminalPrefs::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_correct_id() {
        let m = code_manifest();
        assert_eq!(m.id, "code");
        assert_eq!(m.icon, "#");
        assert_eq!(m.color, "emerald");
    }

    #[test]
    fn manifest_declares_routes() {
        let m = code_manifest();
        assert_eq!(m.routes.len(), 2);
        assert!(m.routes.iter().any(|r| r.path.contains("analyze")));
        assert!(m.routes.iter().any(|r| r.path.contains("search")));
    }

    #[test]
    fn manifest_declares_tools() {
        let m = code_manifest();
        assert_eq!(m.tools.len(), 2);
        assert_eq!(m.tools[0].name, "code.analyze");
        assert_eq!(m.tools[1].name, "code.search");
    }

    #[test]
    fn manifest_declares_events() {
        let m = code_manifest();
        assert_eq!(m.events_produced.len(), 2);
        assert!(m.events_produced.contains(&"code.analyzed".into()));
        assert!(m.events_produced.contains(&"code.searched".into()));
    }

    #[test]
    fn manifest_requires_storage_and_event_ports() {
        let m = code_manifest();
        assert_eq!(m.requires_ports.len(), 2);
        assert!(
            m.requires_ports
                .iter()
                .any(|p| p.port == "StoragePort" && !p.optional)
        );
        assert!(
            m.requires_ports
                .iter()
                .any(|p| p.port == "EventPort" && !p.optional)
        );
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = code_manifest();
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn manifest_serializes() {
        let m = code_manifest();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("code.analyze"));
    }
}
