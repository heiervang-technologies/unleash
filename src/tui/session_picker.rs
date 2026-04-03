//! Interactive session picker for crossload.
//!
//! Shows a searchable list of sessions from all CLIs.
//! Arrow keys to navigate, type to filter, Enter to select, Esc to cancel.

use crate::interchange::sessions::{discover_all, SessionInfo};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{prelude::*, widgets::*};
use std::io::{self, stdout};
use std::time::Duration;

/// Run the interactive session picker. Returns the selected session or None if cancelled.
pub fn pick_session() -> io::Result<Option<SessionInfo>> {
    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Session picker requires a terminal (TTY).",
        ));
    }

    eprintln!("Discovering sessions across all CLIs...");
    let sessions = discover_all();
    if sessions.is_empty() {
        eprintln!("No sessions found.");
        return Ok(None);
    }
    eprintln!("Found {} sessions. Opening picker...", sessions.len());

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = PickerState::new(sessions);
    let result = run_picker(&mut terminal, &mut state);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

struct PickerState {
    sessions: Vec<SessionInfo>,
    query: String,
    selected: usize,
    scroll_offset: usize,
}

impl PickerState {
    fn new(sessions: Vec<SessionInfo>) -> Self {
        Self {
            sessions,
            query: String::new(),
            selected: 0,
            scroll_offset: 0,
        }
    }

    fn filtered(&self) -> Vec<&SessionInfo> {
        if self.query.is_empty() {
            self.sessions.iter().collect()
        } else {
            let q = self.query.to_lowercase();
            self.sessions
                .iter()
                .filter(|s| {
                    s.cli.to_lowercase().contains(&q)
                        || s.id.to_lowercase().contains(&q)
                        || s.name
                            .as_ref()
                            .is_some_and(|n| n.to_lowercase().contains(&q))
                        || s.title
                            .as_ref()
                            .is_some_and(|t| t.to_lowercase().contains(&q))
                        || s.directory.to_lowercase().contains(&q)
                })
                .collect()
        }
    }

    fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn move_down(&mut self) {
        let max = self.filtered().len().saturating_sub(1);
        if self.selected < max {
            self.selected += 1;
        }
    }

    fn selected_session(&self) -> Option<SessionInfo> {
        self.filtered().get(self.selected).map(|s| (*s).clone())
    }
}

fn run_picker(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut PickerState,
) -> io::Result<Option<SessionInfo>> {
    loop {
        terminal.draw(|f| render_picker(f, state))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match key.code {
                    KeyCode::Esc => return Ok(None),
                    KeyCode::Enter => return Ok(state.selected_session()),
                    KeyCode::Up => state.move_up(),
                    KeyCode::Down => state.move_down(),
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        return Ok(None)
                    }
                    KeyCode::Char(c) => {
                        state.query.push(c);
                        state.selected = 0;
                        state.scroll_offset = 0;
                    }
                    KeyCode::Backspace => {
                        state.query.pop();
                        state.selected = 0;
                        state.scroll_offset = 0;
                    }
                    _ => {}
                }
            }
        }
    }
}

fn render_picker(frame: &mut Frame, state: &mut PickerState) {
    let area = frame.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // title + search
            Constraint::Min(5),    // session list
            Constraint::Length(1), // help bar
        ])
        .split(area);

    // Title + search bar
    let search_text = if state.query.is_empty() {
        "Type to filter...".to_string()
    } else {
        state.query.clone()
    };
    let search_style = if state.query.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let search = Paragraph::new(search_text).style(search_style).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Crossload Session Picker ")
            .title_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
    );
    frame.render_widget(search, chunks[0]);

    // Session list
    let visible_height = chunks[1].height.saturating_sub(2) as usize; // borders

    // Adjust scroll to keep selected visible
    if state.selected < state.scroll_offset {
        state.scroll_offset = state.selected;
    } else if state.selected >= state.scroll_offset + visible_height {
        state.scroll_offset = state.selected.saturating_sub(visible_height - 1);
    }

    let filtered = state.filtered();

    let items: Vec<Line> = filtered
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(visible_height)
        .map(|(i, s)| {
            let is_selected = i == state.selected;

            let cli_tag = format!("[{:>8}]", s.cli);
            let name_or_id = s
                .name
                .as_ref()
                .or(s.title.as_ref())
                .cloned()
                .unwrap_or_else(|| truncate_id(&s.id, 12));
            let dir = if s.directory.is_empty() {
                String::new()
            } else {
                format!("  {}", truncate_str(&s.directory, 30))
            };
            let date = if s.updated_at.len() >= 10 {
                &s.updated_at[..10]
            } else {
                &s.updated_at
            };

            let prefix = if is_selected { "> " } else { "  " };
            let style = if is_selected {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(
                    cli_tag,
                    if is_selected {
                        style
                    } else {
                        Style::default().fg(Color::Yellow)
                    },
                ),
                Span::styled(format!(" {:<30}", truncate_str(&name_or_id, 30)), style),
                Span::styled(dir, Style::default().fg(Color::DarkGray)),
                Span::styled(format!("  {}", date), Style::default().fg(Color::DarkGray)),
            ])
        })
        .collect();

    let count_info = format!(" {} of {} sessions ", filtered.len(), state.sessions.len());
    let list = Paragraph::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(count_info)
            .title_style(Style::default().fg(Color::DarkGray)),
    );
    frame.render_widget(list, chunks[1]);

    // Help bar
    let help = Paragraph::new(Line::from(vec![
        Span::styled("↑↓", Style::default().fg(Color::Cyan)),
        Span::raw(" navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Cyan)),
        Span::raw(" select  "),
        Span::styled("Esc", Style::default().fg(Color::Cyan)),
        Span::raw(" cancel  "),
        Span::styled("Type", Style::default().fg(Color::Cyan)),
        Span::raw(" to filter"),
    ]));
    frame.render_widget(help, chunks[2]);
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

fn truncate_id(id: &str, max: usize) -> String {
    if id.len() <= max {
        id.to_string()
    } else {
        id[..max].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_picker_state_filtering() {
        let sessions = vec![
            SessionInfo {
                cli: "claude".into(),
                id: "abc-123".into(),
                name: Some("test-session".into()),
                title: Some("Fix bugs".into()),
                directory: "/home/user/project".into(),
                path: std::path::PathBuf::from("/tmp/test"),
                updated_at: "2026-03-29T12:00:00Z".into(),
                message_count: None,
            },
            SessionInfo {
                cli: "codex".into(),
                id: "def-456".into(),
                name: None,
                title: None,
                directory: "/home/user/other".into(),
                path: std::path::PathBuf::from("/tmp/test2"),
                updated_at: "2026-03-28T12:00:00Z".into(),
                message_count: None,
            },
        ];

        let mut state = PickerState::new(sessions);
        assert_eq!(state.filtered().len(), 2);

        state.query = "claude".into();
        assert_eq!(state.filtered().len(), 1);
        assert_eq!(state.filtered()[0].cli, "claude");

        state.query = "bugs".into();
        assert_eq!(state.filtered().len(), 1);
        assert_eq!(state.filtered()[0].title.as_deref(), Some("Fix bugs"));

        state.query = "nonexistent".into();
        assert_eq!(state.filtered().len(), 0);
    }

    #[test]
    fn test_picker_state_navigation() {
        let sessions = vec![
            SessionInfo {
                cli: "claude".into(),
                id: "1".into(),
                name: None,
                title: None,
                directory: String::new(),
                path: std::path::PathBuf::from("/tmp/1"),
                updated_at: String::new(),
                message_count: None,
            },
            SessionInfo {
                cli: "codex".into(),
                id: "2".into(),
                name: None,
                title: None,
                directory: String::new(),
                path: std::path::PathBuf::from("/tmp/2"),
                updated_at: String::new(),
                message_count: None,
            },
        ];

        let mut state = PickerState::new(sessions);
        assert_eq!(state.selected, 0);

        state.move_down();
        assert_eq!(state.selected, 1);

        state.move_down(); // at max, should stay
        assert_eq!(state.selected, 1);

        state.move_up();
        assert_eq!(state.selected, 0);

        state.move_up(); // at 0, should stay
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn test_truncate_str() {
        assert_eq!(truncate_str("hello", 10), "hello");
        assert_eq!(truncate_str("hello world this is long", 10), "hello w...");
    }
}
