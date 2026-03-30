//! Static manifest for the Messaging department (outbound channels, notifications).

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_MESSAGING;
use rusvel_core::department::*;

pub fn messaging_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_MESSAGING.into(),
        name: "Messaging Department".into(),
        description: "Outbound notifications and channel adapters (e.g. Telegram) when configured"
            .into(),
        icon: "@".into(),
        color: "violet".into(),

        system_prompt: "You are the Messaging department of RUSVEL.\n\nFocus: outbound notifications, channel configuration, delivery status, and integration with external chat platforms when available."
            .into(),

        capabilities: vec!["notify".into(), "channel".into()],

        quick_actions: vec![
            QuickAction {
                label: "Notify status".into(),
                prompt: "Summarize configured outbound channels and recent notification delivery health."
                    .into(),
            },
            QuickAction {
                label: "Channel setup".into(),
                prompt: "Explain how to configure outbound messaging (e.g. Telegram) via environment and config."
                    .into(),
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
        let m = messaging_manifest();
        assert_eq!(m.id, "messaging");
        assert_eq!(m.icon, "@");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = messaging_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }
}
