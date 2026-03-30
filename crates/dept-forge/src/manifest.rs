//! Static manifest for the Forge Department.

use forge_engine::{
    forge_route_contributions_for_manifest, mission_tool_contributions_for_manifest,
    persona_contributions_for_manifest,
};
use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_FORGE;
use rusvel_core::department::*;

pub fn forge_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_FORGE.into(),
        name: "Forge Department".into(),
        description: "Agent orchestration, goal planning, mission management, daily plans, reviews"
            .into(),
        icon: "=".into(),
        color: "indigo".into(),

        system_prompt: concat!(
            "You are the Forge department of RUSVEL.\n\n",
            "Focus: agent orchestration, goal planning, mission management, ",
            "daily plans, reviews.",
        )
        .into(),

        capabilities: vec!["planning".into(), "orchestration".into()],

        quick_actions: vec![
            QuickAction {
                label: "Daily plan".into(),
                prompt:
                    "Generate today's mission plan based on active goals and priorities.".into(),
            },
            QuickAction {
                label: "Review progress".into(),
                prompt: concat!(
                    "Review progress on all active goals. ",
                    "Summarize completed, in-progress, and blocked items."
                )
                .into(),
            },
            QuickAction {
                label: "Set new goal".into(),
                prompt:
                    "Help me define a new strategic goal. Ask me for context and desired outcome."
                        .into(),
            },
        ],

        routes: forge_route_contributions_for_manifest(),

        commands: vec![
            CommandContribution {
                name: "mission".into(),
                description: "Mission planning: today, goals, goal add, review".into(),
                args: vec![
                    ArgDef {
                        name: "subcommand".into(),
                        description: "today | goals | goal | review".into(),
                        required: true,
                        default: Some("today".into()),
                    },
                    ArgDef {
                        name: "title".into(),
                        description: "Goal title (for `goal add`)".into(),
                        required: false,
                        default: None,
                    },
                    ArgDef {
                        name: "period".into(),
                        description: "Review period: day | week | month | quarter".into(),
                        required: false,
                        default: Some("week".into()),
                    },
                ],
            },
        ],

        tools: mission_tool_contributions_for_manifest(),

        personas: persona_contributions_for_manifest(),

        skills: vec![SkillContribution {
            name: "Daily Standup".into(),
            description: "Summarize progress and plan for the day".into(),
            prompt_template: concat!(
                "Based on recent activity, generate a standup summary:\n",
                "- What was accomplished yesterday\n",
                "- What is planned for today\n",
                "- Any blockers"
            )
            .into(),
        }],

        rules: vec![
            RuleContribution {
                name: "Forge safety — budget".into(),
                content: concat!(
                    "Mission and review flows use ForgeEngine safety: enforce aggregate spend ",
                    "against the configured cost limit before starting LLM work. ",
                    "If the budget would be exceeded, stop and surface a clear error."
                )
                .into(),
                enabled: true,
            },
            RuleContribution {
                name: "Forge safety — concurrency and circuit breaker".into(),
                content: concat!(
                    "Mission LLM runs acquire a concurrency slot and respect the circuit breaker. ",
                    "After repeated failures the circuit opens; no further mission agent runs until reset. ",
                    "When the circuit opens, the engine emits forge.safety.circuit_open."
                )
                .into(),
                enabled: true,
            },
        ],

        jobs: vec![],

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
                title: "Mission Status".into(),
                description: "Active goals, today's plan, recent reviews".into(),
                size: "large".into(),
            }],
            has_settings: false,
            custom_components: vec![],
        },

        events_produced: vec![
            "forge.agent.created".into(),
            "forge.agent.started".into(),
            "forge.agent.completed".into(),
            "forge.agent.failed".into(),
            "forge.mission.plan_generated".into(),
            "forge.mission.goal_created".into(),
            "forge.mission.goal_updated".into(),
            "forge.mission.review_completed".into(),
            "forge.persona.hired".into(),
            "forge.safety.budget_warning".into(),
            "forge.safety.circuit_open".into(),
        ],
        events_consumed: vec![],

        requires_ports: vec![
            PortRequirement { port: "AgentPort".into(), optional: false },
            PortRequirement { port: "EventPort".into(), optional: false },
            PortRequirement { port: "MemoryPort".into(), optional: false },
            PortRequirement { port: "StoragePort".into(), optional: false },
            PortRequirement { port: "JobPort".into(), optional: false },
            PortRequirement { port: "SessionPort".into(), optional: false },
            PortRequirement { port: "ConfigPort".into(), optional: false },
        ],

        depends_on: vec![],

        config_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "mission_budget_ceiling": {
                    "type": "number",
                    "description": "Soft cap for mission LLM spend (safety guard default is separate)"
                }
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
        let m = forge_manifest();
        assert_eq!(m.id, "forge");
        assert_eq!(m.icon, "=");
        assert_eq!(m.color, "indigo");
    }

    #[test]
    fn manifest_declares_personas() {
        let m = forge_manifest();
        assert_eq!(m.personas.len(), 10);
        assert!(m.personas.iter().any(|p| p.name == "CodeWriter"));
        assert!(m.tools.len() >= 5);
        assert!(!m.routes.is_empty());
    }

    #[test]
    fn manifest_requires_7_ports() {
        let m = forge_manifest();
        assert_eq!(m.requires_ports.len(), 7);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = forge_manifest();
        assert!(m.depends_on.is_empty());
    }

    #[test]
    fn manifest_rules_cover_safety() {
        let m = forge_manifest();
        assert!(
            m.rules
                .iter()
                .any(|r| r.name.contains("circuit") && r.enabled)
        );
    }
}
