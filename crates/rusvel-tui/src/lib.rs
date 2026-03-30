pub mod layout;
pub mod tabs;
pub mod widgets;

use std::collections::HashMap;
use std::io;

use anyhow::Result;
use crossterm::{
    ExecutableCommand,
    event::{self, Event as CtEvent, KeyCode},
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use rusvel_core::domain::{Event, Goal, Opportunity, Task};
use rusvel_core::terminal::{Pane, PaneSource, PaneStatus};

use crate::layout::{dashboard_layout, terminal_split};
use crate::widgets::{
    events_widget, goals_widget, header_widget, pipeline_stats, pipeline_widget, tasks_widget,
    terminal_output_widget, terminal_pane_list_widget,
};

/// One terminal pane row for the TUI (PTY or browser log pane).
#[derive(Debug, Clone)]
pub struct TuiTerminalPane {
    pub id: String,
    pub title: String,
    pub source: String,
    pub status: String,
    pub output_lines: Vec<String>,
}

impl TuiTerminalPane {
    #[must_use]
    pub fn from_pane(p: &Pane) -> Self {
        Self {
            id: p.id.to_string(),
            title: p.title.clone(),
            source: format_pane_source(&p.source),
            status: format_pane_status(&p.status),
            output_lines: Vec::new(),
        }
    }
}

fn format_pane_source(s: &PaneSource) -> String {
    match s {
        PaneSource::Shell => "shell".into(),
        PaneSource::Department(id) => format!("department({id})"),
        PaneSource::AgentTool { run_id } => format!("agent-tool({run_id})"),
        PaneSource::Delegation {
            parent_run_id,
            delegated_run_id,
            persona,
        } => format!("delegation({parent_run_id}→{delegated_run_id}/{persona})"),
        PaneSource::FlowNode {
            flow_id,
            node_id,
            execution_id,
        } => format!("flow/{flow_id}/{node_id}/{execution_id}"),
        PaneSource::PlaybookStep {
            playbook_id,
            step_index,
            run_id,
        } => format!("playbook/{playbook_id}#{step_index}/{run_id}"),
        PaneSource::Browser { tab_id, platform } => {
            if platform.is_empty() {
                format!("browser({tab_id})")
            } else {
                format!("browser/{platform}/{tab_id}")
            }
        }
        PaneSource::Builder { agent_id } => format!("builder({agent_id})"),
    }
}

fn format_pane_status(s: &PaneStatus) -> String {
    match s {
        PaneStatus::Running => "running".into(),
        PaneStatus::Exited(code) => format!("exited {code}"),
        PaneStatus::Suspended => "suspended".into(),
    }
}

/// Data bundle passed into the TUI for display.
#[derive(Debug, Clone)]
pub struct TuiData {
    pub session_name: String,
    /// One-line preview from last persisted Forge executive brief (S-043).
    pub latest_brief_summary: Option<String>,
    pub goals: Vec<Goal>,
    pub tasks: Vec<Task>,
    pub opportunities: Vec<Opportunity>,
    pub recent_events: Vec<Event>,
    pub terminal_panes: Vec<TuiTerminalPane>,
}

/// Main TUI application state.
pub struct TuiApp {
    session_name: String,
    latest_brief_summary: Option<String>,
    goals: Vec<Goal>,
    tasks: Vec<Task>,
    pipeline: HashMap<String, usize>,
    recent_events: Vec<Event>,
    terminal_panes: Vec<TuiTerminalPane>,
    /// When true, arrow keys move pane selection; `t` toggles.
    terminal_focus: bool,
    selected_pane: usize,
}

impl TuiApp {
    pub fn new(data: TuiData) -> Self {
        let pipeline = pipeline_stats(&data.opportunities);
        Self {
            session_name: data.session_name,
            latest_brief_summary: data.latest_brief_summary,
            goals: data.goals,
            tasks: data.tasks,
            pipeline,
            recent_events: data.recent_events,
            terminal_panes: data.terminal_panes,
            terminal_focus: false,
            selected_pane: 0,
        }
    }

    /// Enter the terminal, render the dashboard, and block until the user presses `q`.
    pub fn run(&mut self) -> Result<()> {
        enable_raw_mode()?;
        io::stdout().execute(EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(io::stdout());
        let mut terminal = Terminal::new(backend)?;

        loop {
            terminal.draw(|frame| {
                let (header_area, grid, term_area) = dashboard_layout(frame.area());

                frame.render_widget(
                    header_widget(&self.session_name, self.latest_brief_summary.as_deref()),
                    header_area,
                );
                frame.render_widget(tasks_widget(&self.tasks), grid[0]);
                frame.render_widget(goals_widget(&self.goals), grid[1]);
                frame.render_widget(pipeline_widget(&self.pipeline), grid[2]);
                frame.render_widget(events_widget(&self.recent_events), grid[3]);

                let [list_area, out_area] = terminal_split(term_area);
                let n = self.terminal_panes.len();
                let sel = if n > 0 {
                    self.selected_pane.min(n - 1)
                } else {
                    0
                };
                let lines = self
                    .terminal_panes
                    .get(sel)
                    .map(|p| p.output_lines.as_slice())
                    .unwrap_or(&[]);

                frame.render_widget(
                    terminal_pane_list_widget(&self.terminal_panes, sel, self.terminal_focus),
                    list_area,
                );
                frame.render_widget(terminal_output_widget(lines, self.terminal_focus), out_area);
            })?;

            if !event::poll(std::time::Duration::from_millis(100))? {
                continue;
            }
            let CtEvent::Key(key) = event::read()? else {
                continue;
            };

            match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Esc if self.terminal_focus => {
                    self.terminal_focus = false;
                }
                KeyCode::Esc => break,
                KeyCode::Char('t' | 'T') => {
                    self.terminal_focus = !self.terminal_focus;
                }
                KeyCode::Up if self.terminal_focus && !self.terminal_panes.is_empty() => {
                    let n = self.terminal_panes.len();
                    self.selected_pane = self.selected_pane.saturating_sub(1).min(n - 1);
                }
                KeyCode::Down if self.terminal_focus && !self.terminal_panes.is_empty() => {
                    let n = self.terminal_panes.len();
                    self.selected_pane = (self.selected_pane + 1).min(n - 1);
                }
                _ => {}
            }
        }

        disable_raw_mode()?;
        io::stdout().execute(LeaveAlternateScreen)?;
        Ok(())
    }
}

/// Async entry point that surfaces (CLI, API) can call.
pub async fn run_tui(data: TuiData) -> Result<()> {
    let mut app = TuiApp::new(data);
    tokio::task::spawn_blocking(move || app.run()).await??;
    Ok(())
}
