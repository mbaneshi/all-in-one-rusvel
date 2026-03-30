//! Static manifest for the go-to-market department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_GTM;
use rusvel_core::department::*;

pub fn gtm_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_GTM.into(),
        name: "GoToMarket Department".into(),
        description: "CRM, outreach sequences, deal management, invoicing".into(),
        icon: "^".into(),
        color: "cyan".into(),

        system_prompt: "You are the GoToMarket department of RUSVEL.\n\nFocus: CRM, outreach sequences, deal management, invoicing.".into(),

        capabilities: vec!["outreach".into(), "crm".into(), "invoicing".into()],

        quick_actions: vec![
            QuickAction {
                label: "List contacts".into(),
                prompt: "List all contacts in the CRM. Show name, company, status, and last interaction.".into(),
            },
            QuickAction {
                label: "Draft outreach".into(),
                prompt: "Draft a multi-step outreach sequence for a prospect.".into(),
            },
            QuickAction {
                label: "Deal pipeline".into(),
                prompt: "Show the current deal pipeline with stages, values, and next actions.".into(),
            },
            QuickAction {
                label: "Generate invoice".into(),
                prompt: "Generate an invoice. Ask me for client details and line items.".into(),
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
                "workflows".into(),
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
        let m = gtm_manifest();
        assert_eq!(m.id, "gtm");
        assert_eq!(m.icon, "^");
        assert_eq!(m.color, "cyan");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = gtm_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = gtm_manifest();
        assert!(m.depends_on.is_empty());
    }
}
