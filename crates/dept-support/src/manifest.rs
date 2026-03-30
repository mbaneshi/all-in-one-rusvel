//! Static manifest for the Support Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_SUPPORT;
use rusvel_core::department::*;

pub fn support_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_SUPPORT.into(),
        name: "Support Department".into(),
        description: "Customer support tickets, knowledge base, NPS tracking, auto-triage, customer success".into(),
        icon: "?".into(),
        color: "yellow".into(),

        system_prompt: "You are the Support department of RUSVEL.\n\nFocus: customer support tickets, knowledge base, NPS tracking, auto-triage, customer success.".into(),

        capabilities: vec!["ticket".into(), "knowledge".into(), "nps".into()],

        quick_actions: vec![
            QuickAction {
                label: "Open tickets".into(),
                prompt: "Show all open support tickets prioritized by urgency.".into(),
            },
            QuickAction {
                label: "Write KB article".into(),
                prompt: "Write a knowledge base article. Ask me for the topic.".into(),
            },
            QuickAction {
                label: "NPS survey".into(),
                prompt: "Analyze recent NPS survey results with score breakdown and themes.".into(),
            },
        ],

        routes: vec![],
        commands: vec![],
        tools: vec![],
        personas: vec![],
        skills: vec![],
        rules: vec![],
        jobs: vec![],

        ui: UiContribution {
            tabs: vec![
                "actions".into(),
                "agents".into(),
                "skills".into(),
                "rules".into(),
                "events".into(),
            ],
            dashboard_cards: vec![],
            has_settings: false,
            custom_components: vec![],
        },

        events_produced: vec![],
        events_consumed: vec![],

        requires_ports: vec![
            PortRequirement { port: "StoragePort".into(), optional: false },
            PortRequirement { port: "EventPort".into(), optional: false },
            PortRequirement { port: "AgentPort".into(), optional: false },
            PortRequirement { port: "JobPort".into(), optional: false },
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
        let m = support_manifest();
        assert_eq!(m.id, "support");
        assert_eq!(m.icon, "?");
        assert_eq!(m.color, "yellow");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = support_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = support_manifest();
        assert!(m.depends_on.is_empty());
    }
}
