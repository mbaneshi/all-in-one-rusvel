//! Static manifest for the Flow Department.
//!
//! No side effects — pure data declaration.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_FLOW;
use rusvel_core::department::*;

pub fn flow_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_FLOW.into(),
        name: "Flow Department".into(),
        description: "DAG workflow automation — create, execute, and monitor workflows".into(),
        icon: "~".into(),
        color: "sky".into(),

        system_prompt: concat!(
            "You are the Flow department of RUSVEL.\n\n",
            "Focus: DAG-based workflow automation.\n",
            "Create, execute, and monitor directed acyclic graph workflows ",
            "with code, condition, and agent nodes.",
        )
        .into(),

        capabilities: vec!["workflow_automation".into()],

        quick_actions: vec![
            QuickAction {
                label: "Create workflow".into(),
                prompt: "Help me create a new DAG workflow. Ask me for the steps and conditions."
                    .into(),
            },
            QuickAction {
                label: "List workflows".into(),
                prompt: "List all saved workflows with their status and last execution.".into(),
            },
            QuickAction {
                label: "Run workflow".into(),
                prompt: "Execute a workflow. Ask me which one to run and with what input.".into(),
            },
        ],

        routes: vec![
            RouteContribution {
                method: "GET".into(),
                path: "/api/flows".into(),
                description: "List all flow definitions".into(),
            },
            RouteContribution {
                method: "POST".into(),
                path: "/api/flows".into(),
                description: "Create a new flow definition".into(),
            },
            RouteContribution {
                method: "GET".into(),
                path: "/api/flows/{id}".into(),
                description: "Get a flow definition by ID".into(),
            },
            RouteContribution {
                method: "PUT".into(),
                path: "/api/flows/{id}".into(),
                description: "Update a flow definition".into(),
            },
            RouteContribution {
                method: "DELETE".into(),
                path: "/api/flows/{id}".into(),
                description: "Delete a flow definition".into(),
            },
            RouteContribution {
                method: "POST".into(),
                path: "/api/flows/{id}/execute".into(),
                description: "Execute a flow".into(),
            },
            RouteContribution {
                method: "GET".into(),
                path: "/api/flows/{id}/executions".into(),
                description: "List executions for a flow".into(),
            },
        ],

        commands: vec![CommandContribution {
            name: "execute".into(),
            description: "Execute a flow by ID".into(),
            args: vec![ArgDef {
                name: "flow_id".into(),
                description: "Flow ID to execute".into(),
                required: true,
                default: None,
            }],
        }],

        tools: vec![
            ToolContribution {
                name: "flow.create".into(),
                description: "Create a new DAG workflow definition".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string", "description": "Workflow name" },
                        "description": { "type": "string", "description": "Workflow description" },
                        "nodes": { "type": "array", "description": "Node definitions" },
                        "connections": { "type": "array", "description": "Edge definitions" }
                    },
                    "required": ["name"]
                }),
            },
            ToolContribution {
                name: "flow.execute".into(),
                description: "Execute a saved workflow by ID".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "flow_id": { "type": "string", "description": "Flow ID to execute" },
                        "trigger_data": { "type": "object", "description": "Input data" }
                    },
                    "required": ["flow_id"]
                }),
            },
            ToolContribution {
                name: "flow.list".into(),
                description: "List all saved workflows".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
        ],

        personas: vec![PersonaContribution {
            name: "workflow-architect".into(),
            role: "DAG workflow designer and automation expert".into(),
            default_model: "sonnet".into(),
            allowed_tools: vec![
                "flow.create".into(),
                "flow.execute".into(),
                "flow.list".into(),
            ],
        }],

        skills: vec![SkillContribution {
            name: "Workflow Design".into(),
            description: "Design a DAG workflow from requirements".into(),
            prompt_template: concat!(
                "Design a workflow for: {{goal}}\n\n",
                "Break it into discrete steps with conditions and dependencies."
            )
            .into(),
        }],

        rules: vec![],

        jobs: vec![JobContribution {
            kind: "flow.execute".into(),
            description: "Execute a DAG workflow".into(),
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
                "events".into(),
            ],
            dashboard_cards: vec![DashboardCard {
                title: "Workflow Status".into(),
                description: "Active, completed, and failed workflow executions".into(),
                size: "medium".into(),
            }],
            has_settings: false,
            custom_components: vec![],
        },

        events_produced: vec![
            "flow.started".into(),
            "flow.completed".into(),
            "flow.failed".into(),
        ],
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
            PortRequirement {
                port: "AgentPort".into(),
                optional: false,
            },
            PortRequirement {
                port: "JobPort".into(),
                optional: false,
            },
        ],

        depends_on: vec![],

        config_schema: serde_json::json!({}),
        default_config: LayeredConfig::default(),
        terminal: DepartmentTerminalPrefs::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_correct_id() {
        let m = flow_manifest();
        assert_eq!(m.id, "flow");
        assert_eq!(m.icon, "~");
        assert_eq!(m.color, "sky");
    }

    #[test]
    fn manifest_declares_routes() {
        let m = flow_manifest();
        assert_eq!(m.routes.len(), 7);
        assert!(m.routes.iter().any(|r| r.path.contains("execute")));
        assert!(m.routes.iter().any(|r| r.path == "/api/flows"));
    }

    #[test]
    fn manifest_declares_tools() {
        let m = flow_manifest();
        assert_eq!(m.tools.len(), 3);
        assert_eq!(m.tools[0].name, "flow.create");
        assert_eq!(m.tools[1].name, "flow.execute");
        assert_eq!(m.tools[2].name, "flow.list");
    }

    #[test]
    fn manifest_declares_events() {
        let m = flow_manifest();
        assert_eq!(m.events_produced.len(), 3);
        assert!(m.events_produced.contains(&"flow.started".into()));
        assert!(m.events_produced.contains(&"flow.completed".into()));
        assert!(m.events_produced.contains(&"flow.failed".into()));
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = flow_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(
            m.requires_ports
                .iter()
                .any(|p| p.port == "StoragePort" && !p.optional)
        );
        assert!(
            m.requires_ports
                .iter()
                .any(|p| p.port == "AgentPort" && !p.optional)
        );
        assert!(
            m.requires_ports
                .iter()
                .any(|p| p.port == "JobPort" && !p.optional)
        );
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = flow_manifest();
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn manifest_serializes() {
        let m = flow_manifest();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("flow.execute"));
    }
}
