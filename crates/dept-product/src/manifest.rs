//! Static manifest for the Product Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_PRODUCT;
use rusvel_core::department::*;

pub fn product_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_PRODUCT.into(),
        name: "Product Department".into(),
        description: "Product roadmaps, feature prioritization, pricing strategy, user feedback analysis, A/B testing".into(),
        icon: "@".into(),
        color: "rose".into(),

        system_prompt: "You are the Product department of RUSVEL.\n\nFocus: product roadmaps, feature prioritization, pricing strategy, user feedback analysis, A/B testing.".into(),

        capabilities: vec!["roadmap".into(), "pricing".into(), "feedback".into()],

        quick_actions: vec![
            QuickAction {
                label: "View roadmap".into(),
                prompt: "Show the current product roadmap with features, milestones, and priorities.".into(),
            },
            QuickAction {
                label: "Add feature".into(),
                prompt: "Add a new feature to the roadmap. Ask me for name, description, and priority.".into(),
            },
            QuickAction {
                label: "Pricing analysis".into(),
                prompt: "Analyze current pricing tiers and suggest optimizations.".into(),
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
        let m = product_manifest();
        assert_eq!(m.id, "product");
        assert_eq!(m.icon, "@");
        assert_eq!(m.color, "rose");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = product_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = product_manifest();
        assert!(m.depends_on.is_empty());
    }
}
