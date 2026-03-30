//! Static manifest for the Content Department.
//!
//! No side effects — pure data declaration.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_CONTENT;
use rusvel_core::department::*;

pub fn content_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_CONTENT.into(),
        name: "Content Department".into(),
        description: "Content creation, platform adaptation, publishing strategy".into(),
        icon: "*".into(),
        color: "purple".into(),

        system_prompt: concat!(
            "You are the Content department of RUSVEL.\n\n",
            "Focus: content creation, platform adaptation, publishing strategy.\n",
            "Draft in Markdown. Adapt for LinkedIn, Twitter/X, DEV.to, Substack.",
        )
        .into(),

        capabilities: vec!["content_creation".into()],

        quick_actions: vec![
            QuickAction {
                label: "Draft blog post".into(),
                prompt: "Draft a blog post. Ask me for the topic, audience, and key points.".into(),
            },
            QuickAction {
                label: "Adapt for Twitter".into(),
                prompt: "Adapt the latest content piece into a Twitter/X thread.".into(),
            },
            QuickAction {
                label: "Content calendar".into(),
                prompt: "Show the content calendar for this week with scheduled and draft posts."
                    .into(),
            },
        ],

        routes: vec![
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/content/draft".into(),
                description: "Draft content from a topic".into(),
            },
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/content/from-code".into(),
                description: "Generate content from code analysis".into(),
            },
            RouteContribution {
                method: "PATCH".into(),
                path: "/api/dept/content/{id}/approve".into(),
                description: "Approve content for publishing".into(),
            },
            RouteContribution {
                method: "POST".into(),
                path: "/api/dept/content/publish".into(),
                description: "Publish approved content to a platform".into(),
            },
            RouteContribution {
                method: "GET".into(),
                path: "/api/dept/content/list".into(),
                description: "List all content items".into(),
            },
        ],

        commands: vec![CommandContribution {
            name: "draft".into(),
            description: "Draft content from a topic".into(),
            args: vec![ArgDef {
                name: "topic".into(),
                description: "Topic to write about".into(),
                required: true,
                default: None,
            }],
        }],

        tools: vec![
            ToolContribution {
                name: "content.draft".into(),
                description: "Draft a blog post or article on a given topic".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Session UUID" },
                        "topic": { "type": "string", "description": "Topic to write about" },
                        "kind": {
                            "type": "string",
                            "description": "Content kind",
                            "enum": ["LongForm", "Tweet", "Thread", "LinkedInPost", "Blog", "VideoScript", "Email", "Proposal"]
                        }
                    },
                    "required": ["session_id", "topic"]
                }),
            },
            ToolContribution {
                name: "content.adapt".into(),
                description: "Adapt content for a specific platform".into(),
                parameters_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "session_id": { "type": "string", "description": "Session UUID" },
                        "content_id": { "type": "string", "description": "ID of content to adapt" },
                        "platform": {
                            "type": "string",
                            "description": "Target platform",
                            "enum": ["twitter", "linkedin", "devto", "medium", "youtube", "substack", "email"]
                        }
                    },
                    "required": ["session_id", "content_id", "platform"]
                }),
            },
        ],

        personas: vec![PersonaContribution {
            name: "content-strategist".into(),
            role: "Content strategist and writer".into(),
            default_model: "sonnet".into(),
            allowed_tools: vec![
                "content.draft".into(),
                "content.adapt".into(),
                "web_search".into(),
            ],
        }],

        skills: vec![SkillContribution {
            name: "Blog Draft".into(),
            description: "Draft a blog post from topic and key points".into(),
            prompt_template: concat!(
                "Write a blog post about: {{topic}}\n\n",
                "Key points:\n{{points}}\n\n",
                "Audience: {{audience}}"
            )
            .into(),
        }],

        rules: vec![RuleContribution {
            name: "Human Approval Gate".into(),
            content: "All content must be approved before publishing. Never auto-publish.".into(),
            enabled: true,
        }],

        jobs: vec![JobContribution {
            kind: "content.publish".into(),
            description: "Publish approved content to target platforms".into(),
            requires_approval: true,
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
                title: "Content Pipeline".into(),
                description: "Drafts, scheduled, and published content".into(),
                size: "medium".into(),
            }],
            has_settings: true,
            custom_components: vec![],
        },

        events_produced: vec![
            "content.drafted".into(),
            "content.adapted".into(),
            "content.scheduled".into(),
            "content.published".into(),
            "content.reviewed".into(),
            "content.cancelled".into(),
            "content.metrics_recorded".into(),
        ],
        events_consumed: vec![
            "code.analyzed".into(), // Code-to-content pipeline
        ],

        requires_ports: vec![
            PortRequirement {
                port: "AgentPort".into(),
                optional: false,
            },
            PortRequirement {
                port: "EventPort".into(),
                optional: false,
            },
            PortRequirement {
                port: "StoragePort".into(),
                optional: false,
            },
            PortRequirement {
                port: "JobPort".into(),
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
                "devto_api_key": { "type": "string", "description": "DEV.to API key" },
                "twitter_bearer_token": { "type": "string", "description": "Twitter/X bearer token" },
                "linkedin_bearer_token": { "type": "string", "description": "LinkedIn bearer token" },
                "default_format": { "type": "string", "enum": ["markdown", "html"] }
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
        let m = content_manifest();
        assert_eq!(m.id, "content");
        assert_eq!(m.icon, "*");
        assert_eq!(m.color, "purple");
    }

    #[test]
    fn manifest_declares_routes() {
        let m = content_manifest();
        assert_eq!(m.routes.len(), 5);
        assert!(m.routes.iter().any(|r| r.path.contains("draft")));
        assert!(m.routes.iter().any(|r| r.path.contains("publish")));
    }

    #[test]
    fn manifest_declares_tools() {
        let m = content_manifest();
        assert_eq!(m.tools.len(), 2);
        assert_eq!(m.tools[0].name, "content.draft");
        assert_eq!(m.tools[1].name, "content.adapt");
    }

    #[test]
    fn manifest_declares_events() {
        let m = content_manifest();
        assert_eq!(m.events_produced.len(), 7);
        assert!(m.events_produced.contains(&"content.drafted".into()));
        assert_eq!(m.events_consumed, vec!["code.analyzed"]);
    }

    #[test]
    fn manifest_requires_agent_port() {
        let m = content_manifest();
        assert!(
            m.requires_ports
                .iter()
                .any(|p| p.port == "AgentPort" && !p.optional)
        );
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = content_manifest();
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn manifest_serializes() {
        let m = content_manifest();
        let json = serde_json::to_string(&m).unwrap();
        assert!(json.contains("content.draft"));
    }
}
