//! Static manifest for the Harvest Department.
//!
//! No side effects — pure data declaration.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_HARVEST;
use rusvel_core::department::*;

pub fn harvest_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_HARVEST.into(),
        name: "Harvest Department".into(),
        description: "Opportunity discovery, scoring, proposal generation, pipeline management"
            .into(),
        icon: "$".into(),
        color: "amber".into(),

        system_prompt: concat!(
            "You are the Harvest department of RUSVEL.\n\n",
            "Focus: finding opportunities, scoring gigs, drafting proposals.\n",
            "Sources: Upwork, LinkedIn, GitHub.",
        )
        .into(),

        capabilities: vec!["opportunity_discovery".into()],

        quick_actions: vec![
            QuickAction {
                label: "Scan opportunities".into(),
                prompt: "Scan for new freelance opportunities on Upwork, LinkedIn, and GitHub."
                    .into(),
            },
            QuickAction {
                label: "Score pipeline".into(),
                prompt: "Score all opportunities in the pipeline by fit, budget, and probability."
                    .into(),
            },
            QuickAction {
                label: "Draft proposal".into(),
                prompt: "Draft a proposal for an opportunity. Ask me for the gig details.".into(),
            },
        ],

        routes: vec![
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/harvest/scan".into(),
                description: "Scan sources for new opportunities".into(),
            },
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/harvest/score".into(),
                description: "Re-score an existing opportunity".into(),
            },
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/harvest/proposal".into(),
                description: "Generate a proposal for an opportunity".into(),
            },
            RouteContribution {
                method: "GET".into(),
                path: "/api/dept/harvest/pipeline".into(),
                description: "Get pipeline statistics".into(),
            },
            RouteContribution {
                method: "GET".into(),
                path: "/api/dept/harvest/list".into(),
                description: "List opportunities".into(),
            },
        ],

        commands: vec![CommandContribution {
            name: "pipeline".into(),
            description: "Show pipeline statistics".into(),
            args: vec![],
        }],

        tools: vec![
            ToolContribution {
                name: "harvest.scan".into(),
                description: "Scan sources for freelance opportunities".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Session ID" },
                        "source": { "type": "string", "description": "Source to scan" }
                    },
                    "required": ["session_id"]
                }),
            },
            ToolContribution {
                name: "harvest.opportunity.score".into(),
                description: "Re-score an opportunity".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Session ID" },
                        "opportunity_id": { "type": "string", "description": "Opportunity to score" }
                    },
                    "required": ["session_id", "opportunity_id"]
                }),
            },
            ToolContribution {
                name: "harvest.proposal.generate".into(),
                description: "Generate a proposal for an opportunity".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Session ID" },
                        "opportunity_id": { "type": "string", "description": "Opportunity ID" },
                        "profile": { "type": "string", "description": "Freelancer profile" }
                    },
                    "required": ["session_id", "opportunity_id"]
                }),
            },
        ],

        personas: vec![PersonaContribution {
            name: "opportunity-hunter".into(),
            role: "Freelance opportunity scout and proposal writer".into(),
            default_model: "sonnet".into(),
            allowed_tools: vec![
                "harvest.scan".into(),
                "harvest.opportunity.score".into(),
                "harvest.proposal.generate".into(),
                "web_search".into(),
            ],
        }],

        skills: vec![SkillContribution {
            name: "Proposal Draft".into(),
            description: "Draft a winning proposal for a freelance opportunity".into(),
            prompt_template: concat!(
                "Draft a proposal for this opportunity:\n\n",
                "Title: {{title}}\n",
                "Description: {{description}}\n\n",
                "Highlight relevant skills and past experience."
            )
            .into(),
        }],

        rules: vec![RuleContribution {
            name: "Human Approval Gate".into(),
            content: "All proposals must be reviewed before submission. Never auto-submit.".into(),
            enabled: true,
        }],

        jobs: vec![JobContribution {
            kind: "harvest.scan".into(),
            description: "Scan opportunity sources".into(),
            requires_approval: false,
        }],

        ui: UiContribution {
            tabs: vec![
                "actions".into(),
                "engine".into(),
                "agents".into(),
                "skills".into(),
                "rules".into(),
                "events".into(),
            ],
            dashboard_cards: vec![DashboardCard {
                title: "Opportunity Pipeline".into(),
                description: "Cold, warm, and hot opportunities".into(),
                size: "medium".into(),
            }],
            has_settings: true,
            custom_components: vec![],
        },

        events_produced: vec![
            "harvest.opportunity_discovered".into(),
            "harvest.opportunity_scored".into(),
            "harvest.proposal_drafted".into(),
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
                port: "ConfigPort".into(),
                optional: true,
            },
        ],

        depends_on: vec![],

        config_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "skills": { "type": "array", "items": { "type": "string" }, "description": "Skills to match" },
                "min_budget": { "type": "number", "description": "Minimum budget filter" }
            }
        }),

        default_config: LayeredConfig::default(),
        terminal: DepartmentTerminalPrefs::default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_has_correct_id() {
        let m = harvest_manifest();
        assert_eq!(m.id, "harvest");
        assert_eq!(m.icon, "$");
        assert_eq!(m.color, "amber");
    }

    #[test]
    fn manifest_declares_routes() {
        let m = harvest_manifest();
        assert_eq!(m.routes.len(), 5);
        assert!(m.routes.iter().any(|r| r.path.contains("scan")));
        assert!(m.routes.iter().any(|r| r.path.contains("pipeline")));
    }

    #[test]
    fn manifest_declares_tools() {
        let m = harvest_manifest();
        assert_eq!(m.tools.len(), 3);
        assert_eq!(m.tools[0].name, "harvest.scan");
        assert_eq!(m.tools[1].name, "harvest.opportunity.score");
        assert_eq!(m.tools[2].name, "harvest.proposal.generate");
    }

    #[test]
    fn manifest_declares_events() {
        let m = harvest_manifest();
        assert_eq!(m.events_produced.len(), 3);
        assert!(
            m.events_produced
                .contains(&"harvest.opportunity_discovered".into())
        );
        assert!(
            m.events_produced
                .contains(&"harvest.opportunity_scored".into())
        );
        assert!(
            m.events_produced
                .contains(&"harvest.proposal_drafted".into())
        );
    }

    #[test]
    fn manifest_requires_ports() {
        let m = harvest_manifest();
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
                .any(|p| p.port == "ConfigPort" && p.optional)
        );
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = harvest_manifest();
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn manifest_serializes() {
        let m = harvest_manifest();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("harvest.scan"));
    }
}
