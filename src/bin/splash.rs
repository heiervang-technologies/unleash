//! splash — Interactive agent picker for the unleash installer
//!
//! Lightweight ratatui TUI: displays mascot ANSI art recolored per agent theme.
//! Arrow keys cycle agents, Enter confirms, q/Esc quits.
//! Prints the selected agent name to stdout on exit.

use std::io::{self, stdout};

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    prelude::*,
    widgets::Paragraph,
};
use unleash::pixel_art;
use unleash::theme::ThemeShift;

struct Agent {
    name: &'static str,
    shift: ThemeShift,
    accent: Color,
}

const AGENTS: [Agent; 4] = [
    Agent {
        name: "claude",
        shift: ThemeShift { hue: 0.0, sat_scale: 1.0 },
        accent: Color::Rgb(217, 119, 87),
    },
    Agent {
        name: "codex",
        shift: ThemeShift { hue: 345.23, sat_scale: 0.0 },
        accent: Color::Rgb(140, 140, 140),
    },
    Agent {
        name: "gemini",
        shift: ThemeShift { hue: 260.0, sat_scale: 1.0 },
        accent: Color::Rgb(162, 87, 217),
    },
    Agent {
        name: "opencode",
        shift: ThemeShift { hue: 200.0, sat_scale: 1.0 },
        accent: Color::Rgb(87, 142, 217),
    },
];

/// The single mascot source art (embedded at compile time)
const MASCOT_ART: &str = include_str!("../assets/mascot.claude.ans");

use unleash::pixel_art::mascots::HALF_WIDTH;

fn main() -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let selection = run_loop(&mut terminal);

    // Cleanup MUST happen before any stdout output
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Now safe to write to stdout/stderr
    match selection? {
        Selection::Agent(idx) => {
            println!("{}", AGENTS[idx].name);
            Ok(())
        }
        Selection::Cancelled => {
            eprintln!("Installation cancelled.");
            std::process::exit(0);
        }
    }
}

enum Selection {
    Agent(usize),
    Cancelled,
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> io::Result<Selection> {
    let mut current: usize = 0;

    loop {
        terminal.draw(|frame| render(frame, current))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                    return Ok(Selection::Cancelled);
                }
                KeyCode::Right | KeyCode::Down => {
                    current = (current + 1) % AGENTS.len();
                }
                KeyCode::Left | KeyCode::Up => {
                    current = (current + AGENTS.len() - 1) % AGENTS.len();
                }
                KeyCode::Enter => {
                    return Ok(Selection::Agent(current));
                }
                _ => {}
            }
        }
    }
}

fn render(frame: &mut Frame, current: usize) {
    let agent = &AGENTS[current];
    let area = frame.area();

    // Layout: title (2) + mascot (flexible) + input box (3) + hint (2)
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),  // title
            Constraint::Min(5),    // mascot art (takes remaining space)
            Constraint::Length(3), // input box
            Constraint::Length(2), // hint
        ])
        .split(area);

    render_title(frame, chunks[0], agent);
    render_mascot(frame, chunks[1], agent);
    render_input_box(frame, chunks[2], agent);
    render_hint(frame, chunks[3]);
}

fn render_title(frame: &mut Frame, area: Rect, _agent: &Agent) {
    let title = Line::from(vec![
        Span::raw("  "),
        Span::styled("unleash", Style::default().bold()),
        Span::raw("  "),
        Span::styled("installer", Style::default().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(vec![Line::default(), title]), area);
}

fn render_mascot(frame: &mut Frame, area: Rect, agent: &Agent) {
    let max_lines = area.height as usize;
    let cols = area.width as usize;

    // Decide what to show based on terminal width:
    // - Full art (106 cols) if terminal is wide enough
    // - Right half (53 cols) if narrower
    // - Cropped right half if very narrow
    let art_str = if cols >= 106 {
        MASCOT_ART.to_string()
    } else {
        // Use right half (the "facing forward" side)
        let (_, right) = pixel_art::split_ansi_art(MASCOT_ART, HALF_WIDTH);
        right
    };

    // Parse ANSI art to ratatui Lines with theme recoloring
    let art_lines: Vec<Line> = if agent.shift.is_identity() {
        pixel_art::parse_ansi_to_ratatui(&art_str)
    } else {
        pixel_art::parse_ansi_to_ratatui_themed(&art_str, agent.shift)
    };

    // Skip leading blank lines, take what fits
    let art_lines: Vec<Line> = art_lines
        .into_iter()
        .skip_while(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
        .take(max_lines)
        .collect();

    frame.render_widget(Paragraph::new(art_lines), area);
}

fn render_input_box(frame: &mut Frame, area: Rect, agent: &Agent) {
    // Full-width box: area.width minus 2 for left indent
    let outer = (area.width as usize).saturating_sub(2);
    let inner = outer.saturating_sub(2); // minus 2 border chars (│...│)
    let bar = "─".repeat(inner);

    // Content: "❯ unleash <agent>" with padding to fill
    // Display width: "❯" is 1 column (not 3 bytes), plus " unleash " = 10 columns
    let display_width = 10 + agent.name.len();
    let pad = " ".repeat(inner.saturating_sub(display_width));

    let box_lines = vec![
        Line::from(vec![
            Span::raw("  ╭"),
            Span::raw(bar.clone()),
            Span::raw("╮"),
        ]),
        Line::from(vec![
            Span::raw("  │"),
            Span::styled("❯", Style::default().fg(Color::Cyan)),
            Span::raw(" unleash "),
            Span::styled(agent.name, Style::default().fg(agent.accent).bold()),
            Span::raw(pad),
            Span::raw("│"),
        ]),
        Line::from(vec![
            Span::raw("  ╰"),
            Span::raw(bar),
            Span::raw("╯"),
        ]),
    ];

    frame.render_widget(Paragraph::new(box_lines), area);
}

fn render_hint(frame: &mut Frame, area: Rect) {
    let hint = Line::from(vec![
        Span::raw("  "),
        Span::styled(
            "←/→ cycle agents    Enter confirm    q quit",
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    frame.render_widget(Paragraph::new(vec![Line::default(), hint]), area);
}
