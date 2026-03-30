//! Static manifest for the Finance Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_FINANCE;
use rusvel_core::department::*;

pub fn finance_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_FINANCE.into(),
        name: "Finance Department".into(),
        description: "Revenue tracking, expense management, tax optimization, runway forecasting, P&L reports, unit economics".into(),
        icon: "%".into(),
        color: "green".into(),

        system_prompt: "You are the Finance department of RUSVEL.\n\nFocus: revenue tracking, expense management, tax optimization, runway forecasting, P&L reports, unit economics.".into(),

        capabilities: vec!["ledger".into(), "tax".into(), "runway".into()],

        quick_actions: vec![
            QuickAction {
                label: "Record income".into(),
                prompt: "Record a new income transaction. Ask me for amount, source, category, and date.".into(),
            },
            QuickAction {
                label: "Log expense".into(),
                prompt: "Log a new expense. Ask me for amount, description, category, and date.".into(),
            },
            QuickAction {
                label: "Calculate runway".into(),
                prompt: "Calculate the current runway based on cash on hand and burn rate.".into(),
            },
            QuickAction {
                label: "P&L report".into(),
                prompt: "Generate a profit & loss report by category.".into(),
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
        let m = finance_manifest();
        assert_eq!(m.id, "finance");
        assert_eq!(m.icon, "%");
        assert_eq!(m.color, "green");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = finance_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = finance_manifest();
        assert!(m.depends_on.is_empty());
    }
}
