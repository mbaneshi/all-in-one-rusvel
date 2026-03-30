use clap::Parser;
use rusvel_cli::departments::DeptAction;
use rusvel_cli::{Cli, Commands};

#[test]
fn parse_forge_status() {
    let cli = Cli::try_parse_from(["rusvel", "forge", "mission", "today"]).unwrap();
    assert!(!cli.mcp);
    assert!(!cli.tui);
    assert!(matches!(cli.command, Some(Commands::Forge { .. })));
}

#[test]
fn parse_department_status() {
    let cli = Cli::try_parse_from(["rusvel", "finance", "status"]).unwrap();
    match cli.command {
        Some(Commands::Finance { action }) => {
            assert!(matches!(action, DeptAction::Status));
        }
        other => panic!("expected Finance, got {other:?}"),
    }
}

#[test]
fn parse_department_list() {
    let cli = Cli::try_parse_from(["rusvel", "growth", "list"]).unwrap();
    match cli.command {
        Some(Commands::Growth { action }) => {
            assert!(matches!(action, DeptAction::List { .. }));
        }
        other => panic!("expected Growth list, got {other:?}"),
    }
}

#[test]
fn parse_department_list_with_kind() {
    let cli = Cli::try_parse_from(["rusvel", "finance", "list", "--kind", "transactions"]).unwrap();
    match cli.command {
        Some(Commands::Finance {
            action: DeptAction::List { kind, limit },
        }) => {
            assert_eq!(kind.as_deref(), Some("transactions"));
            assert_eq!(limit, 20);
        }
        other => panic!("expected Finance list --kind, got {other:?}"),
    }
}

#[test]
fn parse_shell() {
    let cli = Cli::try_parse_from(["rusvel", "shell"]).unwrap();
    assert!(matches!(cli.command, Some(Commands::Shell)));
}

#[test]
fn parse_tui_flag() {
    let cli = Cli::try_parse_from(["rusvel", "--tui"]).unwrap();
    assert!(cli.tui);
    assert!(cli.command.is_none());
}

#[test]
fn parse_mcp_flag() {
    let cli = Cli::try_parse_from(["rusvel", "--mcp"]).unwrap();
    assert!(cli.mcp);
    assert!(cli.command.is_none());
}

#[test]
fn parse_code_analyze() {
    let cli = Cli::try_parse_from(["rusvel", "code", "analyze", "src/"]).unwrap();
    match cli.command {
        Some(Commands::Code {
            action: DeptAction::Analyze { path },
        }) => {
            assert_eq!(path, "src/");
        }
        other => panic!("expected Code analyze, got {other:?}"),
    }
}

#[test]
fn parse_code_analyze_default_path() {
    let cli = Cli::try_parse_from(["rusvel", "code", "analyze"]).unwrap();
    match cli.command {
        Some(Commands::Code {
            action: DeptAction::Analyze { path },
        }) => {
            assert_eq!(path, ".");
        }
        other => panic!("expected Code analyze with default path, got {other:?}"),
    }
}

#[test]
fn parse_content_draft() {
    let cli = Cli::try_parse_from(["rusvel", "content", "draft", "Rust async"]).unwrap();
    match cli.command {
        Some(Commands::Content {
            action: DeptAction::Draft { topic },
        }) => {
            assert_eq!(topic, "Rust async");
        }
        other => panic!("expected Content draft, got {other:?}"),
    }
}

#[test]
fn parse_nonexistent_subcommand_fails() {
    let result = Cli::try_parse_from(["rusvel", "nonexistent"]);
    assert!(result.is_err());
}

#[test]
fn parse_session_create() {
    let cli = Cli::try_parse_from(["rusvel", "session", "create", "my-project"]).unwrap();
    match cli.command {
        Some(Commands::Session {
            action: rusvel_cli::SessionCmd::Create { name },
        }) => {
            assert_eq!(name, "my-project");
        }
        other => panic!("expected Session create, got {other:?}"),
    }
}

#[test]
fn parse_no_subcommand() {
    let cli = Cli::try_parse_from(["rusvel"]).unwrap();
    assert!(cli.command.is_none());
    assert!(!cli.mcp);
    assert!(!cli.tui);
}

#[test]
fn all_departments_parse() {
    let depts = [
        "finance", "growth", "distro", "legal", "support", "infra", "product", "code", "harvest",
        "content", "gtm",
    ];
    for dept in depts {
        let cli = Cli::try_parse_from(["rusvel", dept, "status"]).unwrap();
        assert!(
            cli.command.is_some(),
            "department '{dept}' should parse status"
        );
    }
}
