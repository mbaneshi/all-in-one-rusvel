//! Static manifest for the Legal Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_LEGAL;
use rusvel_core::department::*;

pub fn legal_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_LEGAL.into(),
        name: "Legal Department".into(),
        description: "Contracts, IP protection, terms of service, GDPR compliance, licensing, privacy policies".into(),
        icon: "\u{00a7}".into(),
        color: "slate".into(),

        system_prompt: "You are the Legal department of RUSVEL.\n\nFocus: contracts, IP protection, terms of service, GDPR compliance, licensing, privacy policies.".into(),

        capabilities: vec!["contract".into(), "compliance".into(), "ip".into()],

        quick_actions: vec![
            QuickAction {
                label: "Draft contract".into(),
                prompt: "Draft a contract. Ask me for type, parties, and terms.".into(),
            },
            QuickAction {
                label: "Compliance check".into(),
                prompt: "Run a compliance check for GDPR and privacy policy.".into(),
            },
            QuickAction {
                label: "IP review".into(),
                prompt: "Review intellectual property assets.".into(),
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
        let m = legal_manifest();
        assert_eq!(m.id, "legal");
        assert_eq!(m.icon, "\u{00a7}");
        assert_eq!(m.color, "slate");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = legal_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = legal_manifest();
        assert!(m.depends_on.is_empty());
    }
}
