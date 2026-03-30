use chrono::Utc;
use ratatui::{Terminal, backend::TestBackend};
use rusvel_core::domain::*;
use rusvel_core::id::{EventId, GoalId, OpportunityId, SessionId, TaskId};
use rusvel_tui::widgets::{
    events_widget, goals_widget, header_widget, pipeline_stats, tasks_widget,
    terminal_output_widget, terminal_pane_list_widget,
};
use rusvel_tui::{TuiApp, TuiData, TuiTerminalPane};

fn make_task(title: &str, status: TaskStatus) -> Task {
    Task {
        id: TaskId::new(),
        session_id: SessionId::new(),
        goal_id: None,
        title: title.to_string(),
        status,
        due_at: None,
        priority: Priority::Medium,
        metadata: serde_json::json!({}),
    }
}

fn make_goal(title: &str, progress: f64) -> Goal {
    Goal {
        id: GoalId::new(),
        session_id: SessionId::new(),
        title: title.to_string(),
        description: String::new(),
        timeframe: Timeframe::Month,
        status: GoalStatus::Active,
        progress,
        metadata: serde_json::json!({}),
    }
}

fn make_event(kind: &str) -> Event {
    Event {
        id: EventId::new(),
        session_id: None,
        run_id: None,
        source: "test".to_string(),
        kind: kind.to_string(),
        payload: serde_json::json!({}),
        created_at: Utc::now(),
        metadata: serde_json::json!({}),
    }
}

fn make_opportunity(stage: OpportunityStage) -> Opportunity {
    Opportunity {
        id: OpportunityId::new(),
        session_id: SessionId::new(),
        source: OpportunitySource::Manual,
        title: "Test opp".into(),
        url: None,
        description: String::new(),
        score: 0.5,
        stage,
        value_estimate: None,
        platform_job_key: None,
        content_hash: None,
        upstream_score: None,
        budget_min: None,
        budget_max: None,
        budget_currency: None,
        hourly: None,
        client_hire_rate: None,
        client_total_spent: None,
        payment_verified: None,
        proposal_count: None,
        metadata: serde_json::json!({}),
    }
}

#[test]
fn header_renders_without_panic() {
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            let widget = header_widget("test-session", Some("brief summary text"));
            frame.render_widget(widget, frame.area());
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let text = buffer_text(&buf);
    assert!(text.contains("RUSVEL"));
    assert!(text.contains("test-session"));
}

#[test]
fn tasks_widget_renders_items() {
    let tasks = vec![
        make_task("Write tests", TaskStatus::InProgress),
        make_task("Ship feature", TaskStatus::Todo),
        make_task("Code review", TaskStatus::Done),
    ];
    let backend = TestBackend::new(60, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(tasks_widget(&tasks), frame.area());
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Write tests"));
    assert!(text.contains("Ship feature"));
    assert!(text.contains("[x]"));
}

#[test]
fn goals_widget_renders_table() {
    let goals = vec![
        make_goal("Launch MVP", 0.6),
        make_goal("Reach 100 users", 0.2),
    ];
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(goals_widget(&goals), frame.area());
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Launch MVP"));
    assert!(text.contains("Goal"));
}

#[test]
fn pipeline_stats_counts_stages() {
    let opps = vec![
        make_opportunity(OpportunityStage::Cold),
        make_opportunity(OpportunityStage::Cold),
        make_opportunity(OpportunityStage::Won),
    ];
    let stats = pipeline_stats(&opps);
    assert_eq!(stats.get("Cold"), Some(&2));
    assert_eq!(stats.get("Won"), Some(&1));
    assert_eq!(stats.get("Lost"), None);
}

#[test]
fn events_widget_renders() {
    let events = vec![
        make_event("forge.plan.created"),
        make_event("code.analyzed"),
    ];
    let backend = TestBackend::new(80, 10);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(events_widget(&events), frame.area());
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Events"));
    assert!(text.contains("forge.plan.created"));
}

#[test]
fn tui_app_constructs_from_data() {
    let data = TuiData {
        session_name: "test".into(),
        latest_brief_summary: Some("All departments green".into()),
        goals: vec![make_goal("Ship v1", 0.5)],
        tasks: vec![make_task("Test TUI", TaskStatus::Todo)],
        opportunities: vec![make_opportunity(OpportunityStage::Qualified)],
        recent_events: vec![make_event("test.event")],
        terminal_panes: vec![TuiTerminalPane {
            id: "p1".into(),
            title: "Shell".into(),
            source: "shell".into(),
            status: "running".into(),
            output_lines: vec!["hello".into()],
        }],
    };
    let _app = TuiApp::new(data);
}

#[test]
fn terminal_pane_list_empty() {
    let backend = TestBackend::new(40, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(terminal_pane_list_widget(&[], 0, false), frame.area());
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("no terminal panes"));
}

#[test]
fn terminal_output_empty_shows_hint() {
    let backend = TestBackend::new(80, 6);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            frame.render_widget(terminal_output_widget(&[], false), frame.area());
        })
        .unwrap();
    let text = buffer_text(terminal.backend().buffer());
    assert!(text.contains("Select a pane"));
}

fn buffer_text(buf: &ratatui::buffer::Buffer) -> String {
    let area = buf.area;
    let mut out = String::new();
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            out.push_str(buf.cell((x, y)).map(|c| c.symbol()).unwrap_or(" "));
        }
        out.push('\n');
    }
    out
}
