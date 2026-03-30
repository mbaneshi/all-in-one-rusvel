//! Static manifest for the Distribution Department.

use rusvel_core::config::LayeredConfig;
use rusvel_core::constants::DEPT_DISTRO;
use rusvel_core::department::*;

pub fn distro_manifest() -> DepartmentManifest {
    DepartmentManifest {
        id: DEPT_DISTRO.into(),
        name: "Distribution Department".into(),
        description: "Marketplace listings, SEO optimization, affiliate programs, partnerships, API distribution channels".into(),
        icon: "!".into(),
        color: "teal".into(),

        system_prompt: "You are the Distribution department of RUSVEL.\n\nFocus: marketplace listings, SEO optimization, affiliate programs, partnerships, API distribution channels.".into(),

        capabilities: vec!["marketplace".into(), "seo".into(), "affiliate".into()],

        quick_actions: vec![
            QuickAction {
                label: "SEO audit".into(),
                prompt: "Run an SEO audit. Check keyword rankings and page performance.".into(),
            },
            QuickAction {
                label: "Marketplace listings".into(),
                prompt: "Review all marketplace listings with status, downloads, and revenue.".into(),
            },
            QuickAction {
                label: "Distribution strategy".into(),
                prompt: "Analyze distribution channels and recommend new ones.".into(),
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
        let m = distro_manifest();
        assert_eq!(m.id, "distro");
        assert_eq!(m.icon, "!");
        assert_eq!(m.color, "teal");
    }

    #[test]
    fn manifest_requires_4_ports() {
        let m = distro_manifest();
        assert_eq!(m.requires_ports.len(), 4);
        assert!(m.requires_ports.iter().all(|p| !p.optional));
    }

    #[test]
    fn manifest_has_no_deps() {
        let m = distro_manifest();
        assert!(m.depends_on.is_empty());
    }
}
