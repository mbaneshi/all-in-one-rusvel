//! Static manifest for the Infra Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_INFRA;
use rusvel_core::department::*;

pub fn infra_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_INFRA.into(),
        name: "Infra Department".into(),
        description: "CI/CD pipelines, deployments, monitoring, incident response, performance, cost analysis".into(),
        icon: ">".into(),
        color: "red".into(),

        system_prompt: "You are the Infrastructure department of RUSVEL.\n\nFocus: CI/CD pipelines, deployments, monitoring, incident response, performance, cost analysis.".into(),

        capabilities: vec!["deploy".into(), "monitor".into(), "incident".into()],

        quick_actions: vec![
            QuickAction {
                label: "Deploy status".into(),
                prompt: "Show current deployment status across all services and environments.".into(),
            },
            QuickAction {
                label: "Health check".into(),
                prompt: "Run health checks on all monitored services.".into(),
            },
            QuickAction {
                label: "Incident report".into(),
                prompt: "Show open incidents with severity, timeline, and resolution status.".into(),
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
        let m = infra_manifest();
        assert_eq!(m.id, "infra");
        assert_eq!(m.icon, ">");
        assert_eq!(m.color, "red");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = infra_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = infra_manifest();
        assert!(m.depends_on.is_empty());
    }
}
