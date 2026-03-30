use std::collections::HashMap;

use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Bar, BarChart, BarGroup, Block, Borders, List, ListItem, Paragraph, Row, Table},
};
use rusvel_core::domain::{
    Event, Goal, GoalStatus, Opportunity, OpportunityStage, Task, TaskStatus,
};

use crate::TuiTerminalPane;
use crate::tabs::{PANEL_EVENTS, PANEL_GOALS, PANEL_PIPELINE, PANEL_TASKS};

pub fn header_widget<'a>(session_name: &'a str, latest_brief: Option<&'a str>) -> Paragraph<'a> {
    let mut lines = vec![Line::from(vec![
        Span::styled(
            " RUSVEL ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("— "),
        Span::styled(session_name, Style::default().fg(Color::Yellow)),
    ])];
    if let Some(b) = latest_brief {
        let t = if b.chars().count() > 96 {
            format!("{}…", b.chars().take(96).collect::<String>())
        } else {
            b.to_string()
        };
        lines.push(Line::from(vec![
            Span::styled(" brief ", Style::default().fg(Color::Magenta)),
            Span::raw(t),
        ]));
    }
    Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan)),
    )
}

pub fn tasks_widget(tasks: &[Task]) -> List<'_> {
    let items: Vec<ListItem<'_>> = tasks
        .iter()
        .map(|t| {
            let marker = match t.status {
                TaskStatus::Done => "[x]",
                TaskStatus::InProgress => "[~]",
                TaskStatus::Cancelled => "[-]",
                TaskStatus::Todo => "[ ]",
            };
            let color = match t.status {
                TaskStatus::Done => Color::Green,
                TaskStatus::InProgress => Color::Yellow,
                _ => Color::White,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{marker} "), Style::default().fg(color)),
                Span::raw(&t.title),
            ]))
        })
        .collect();

    List::new(items).block(
        Block::default()
            .title(format!(" {PANEL_TASKS} "))
            .borders(Borders::ALL),
    )
}

pub fn goals_widget(goals: &[Goal]) -> Table<'_> {
    let rows: Vec<Row<'_>> = goals
        .iter()
        .map(|g| {
            let pct = (g.progress * 100.0) as u32;
            let bar_len = 10;
            let filled = ((g.progress * bar_len as f64) as usize).min(bar_len);
            let bar = format!(
                "{}{} {pct}%",
                "█".repeat(filled),
                "░".repeat(bar_len - filled)
            );
            let status_color = match g.status {
                GoalStatus::Active => Color::Green,
                GoalStatus::Completed => Color::Cyan,
                GoalStatus::Deferred => Color::Yellow,
                GoalStatus::Abandoned => Color::Red,
            };
            Row::new(vec![g.title.clone(), bar, format!("{:?}", g.timeframe)])
                .style(Style::default().fg(status_color))
        })
        .collect();

    Table::new(
        rows,
        [
            ratatui::layout::Constraint::Percentage(40),
            ratatui::layout::Constraint::Percentage(40),
            ratatui::layout::Constraint::Percentage(20),
        ],
    )
    .header(
        Row::new(vec!["Goal", "Progress", "Timeframe"]).style(
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(Color::Cyan),
        ),
    )
    .block(
        Block::default()
            .title(format!(" {PANEL_GOALS} "))
            .borders(Borders::ALL),
    )
}

pub fn pipeline_widget(stats: &HashMap<String, usize>) -> BarChart<'_> {
    let stages = [
        "Cold",
        "Contacted",
        "Qualified",
        "ProposalSent",
        "Won",
        "Lost",
    ];
    let bars: Vec<Bar<'_>> = stages
        .iter()
        .map(|s| {
            let val = *stats.get(*s).unwrap_or(&0);
            Bar::default().label((*s).into()).value(val as u64)
        })
        .collect();

    BarChart::default()
        .data(BarGroup::default().bars(&bars))
        .bar_width(7)
        .bar_gap(1)
        .bar_style(Style::default().fg(Color::Cyan))
        .value_style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .title(format!(" {PANEL_PIPELINE} "))
                .borders(Borders::ALL),
        )
}

pub fn events_widget(events: &[Event]) -> List<'_> {
    let items: Vec<ListItem<'_>> = events
        .iter()
        .map(|e| {
            let time = e.created_at.format("%H:%M");
            ListItem::new(Line::from(vec![
                Span::styled(format!("{time} "), Style::default().fg(Color::DarkGray)),
                Span::styled(format!("[{}] ", e.source), Style::default().fg(Color::Cyan)),
                Span::raw(&e.kind),
            ]))
        })
        .collect();

    List::new(items).block(
        Block::default()
            .title(format!(" {PANEL_EVENTS} "))
            .borders(Borders::ALL),
    )
}

pub fn terminal_pane_list_widget(
    panes: &[TuiTerminalPane],
    selected: usize,
    terminal_focus: bool,
) -> List<'_> {
    let items: Vec<ListItem<'_>> = if panes.is_empty() {
        vec![ListItem::new(Line::from("(no terminal panes)"))]
    } else {
        panes
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let sel = i == selected && terminal_focus;
                let style = if sel {
                    Style::default()
                        .bg(Color::DarkGray)
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                let line = Line::from(vec![
                    Span::styled(format!("{} ", p.title), style),
                    Span::styled(
                        format!("{} · {} ", p.source, p.status),
                        Style::default().fg(Color::DarkGray),
                    ),
                ]);
                ListItem::new(line)
            })
            .collect()
    };

    let border = if terminal_focus {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    List::new(items).block(
        Block::default()
            .title(" Terminal — panes ")
            .borders(Borders::ALL)
            .border_style(border),
    )
}

pub fn terminal_output_widget(lines: &[String], terminal_focus: bool) -> Paragraph<'_> {
    let border = if terminal_focus {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    let text: Vec<Line> = if lines.is_empty() {
        vec![Line::from(Span::styled(
            "Select a pane (↑/↓). Live PTY/CDP output streams via WebSocket or API.",
            Style::default().fg(Color::DarkGray),
        ))]
    } else {
        lines
            .iter()
            .map(|l| Line::from(Span::raw(l.as_str())))
            .collect()
    };

    Paragraph::new(text).block(
        Block::default()
            .title(" Output ")
            .borders(Borders::ALL)
            .border_style(border),
    )
}

/// Build pipeline stats from a slice of opportunities.
pub fn pipeline_stats(opps: &[Opportunity]) -> HashMap<String, usize> {
    let mut map = HashMap::new();
    for o in opps {
        let key = match o.stage {
            OpportunityStage::Cold => "Cold",
            OpportunityStage::Contacted => "Contacted",
            OpportunityStage::Qualified => "Qualified",
            OpportunityStage::ProposalSent => "ProposalSent",
            OpportunityStage::Won => "Won",
            OpportunityStage::Lost => "Lost",
        };
        *map.entry(key.to_string()).or_insert(0) += 1;
    }
    map
}
