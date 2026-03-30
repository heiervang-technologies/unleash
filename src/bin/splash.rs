//! splash — Interactive agent picker for the unleash installer
//!
//! Lightweight ratatui TUI: displays mascot ANSI art recolored per agent theme.
//! Arrow keys cycle agents, Enter confirms, q/Esc quits.
//! Prints the selected agent name to stdout on exit.

use std::fs::File;
use std::io::{self, Write};
use std::os::unix::io::AsRawFd;

extern crate libc;

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

enum AgentTheme {
    Shift(ThemeShift),
    Gradient(unleash::theme::GradientTheme),
}

struct Agent {
    name: &'static str,
    theme: AgentTheme,
    accent: Color,
}

fn agents() -> Vec<Agent> {
    vec![
        Agent {
            name: "claude",
            theme: AgentTheme::Shift(ThemeShift { hue: 0.0, sat_scale: 1.0 }),
            accent: Color::Rgb(217, 119, 87),
        },
        Agent {
            name: "codex",
            theme: AgentTheme::Shift(ThemeShift { hue: 345.23, sat_scale: 0.0 }),
            accent: Color::Rgb(140, 140, 140),
        },
        Agent {
            name: "gemini",
            theme: AgentTheme::Gradient(unleash::theme::GradientTheme::gemini()),
            accent: Color::Rgb(0x84, 0x7A, 0xCE), // middle of gradient (purple)
        },
        Agent {
            name: "opencode",
            theme: AgentTheme::Shift(ThemeShift { hue: 200.0, sat_scale: 1.0 }),
            accent: Color::Rgb(87, 142, 217),
        },
    ]
}

use unleash::pixel_art::mascots;

fn main() -> io::Result<()> {
    // Render TUI to /dev/tty so stdout stays clean for the selection output.
    // This allows `SELECTED=$("$SPLASH_BIN")` to capture just the agent name.
    let tty = File::options().read(true).write(true).open("/dev/tty")?;

    // Replace stdin with the TTY so crossterm reads keys from the terminal,
    // not from whatever pipe the shell set up for stdout capture.
    // SAFETY: dup2 on valid fds; stdin fd 0 is always open.
    let rc = unsafe { libc::dup2(tty.as_raw_fd(), libc::STDIN_FILENO) };
    if rc < 0 {
        return Err(io::Error::last_os_error());
    }

    enable_raw_mode()?;
    let mut tty_write = tty.try_clone()?;
    execute!(tty_write, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(tty_write);
    let mut terminal = Terminal::new(backend)?;

    let agents = agents();
    let selection = run_loop(&mut terminal, &agents);

    // Cleanup on the TTY
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    // Write selection to real stdout (captured by the shell)
    match selection? {
        Selection::Agent(idx) => {
            println!("{}", agents[idx].name);
            Ok(())
        }
        Selection::Cancelled => {
            eprintln!("Installation cancelled.");
            std::process::exit(1);
        }
    }
}

enum Selection {
    Agent(usize),
    Cancelled,
}

fn run_loop<W: Write>(terminal: &mut Terminal<CrosstermBackend<W>>, agents: &[Agent]) -> io::Result<Selection> {
    let mut current: usize = 0;

    loop {
        terminal.draw(|frame| render(frame, current, agents))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                    return Ok(Selection::Cancelled);
                }
                KeyCode::Right | KeyCode::Down => {
                    current = (current + 1) % agents.len();
                }
                KeyCode::Left | KeyCode::Up => {
                    current = (current + agents.len() - 1) % agents.len();
                }
                KeyCode::Enter => {
                    return Ok(Selection::Agent(current));
                }
                _ => {}
            }
        }
    }
}

fn render(frame: &mut Frame, current: usize, agents: &[Agent]) {
    let agent = &agents[current];
    let area = frame.area();

    // Count actual mascot art lines to size the layout tightly (no bottom gap).
    let cols = area.width as usize;
    let full = mascots::full_art(agent.name);
    let art_str = if cols >= 106 { full.to_string() } else { mascots::right_half(agent.name) };
    let art_height = art_str.lines()
        .skip_while(|l| l.trim().is_empty())
        .count() as u16;

    // Layout: title (2) + mascot (art_height) + input box (3) + hint (2) + spacer
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2),          // title
            Constraint::Length(art_height), // mascot art (exact fit)
            Constraint::Length(3),          // input box
            Constraint::Length(2),          // hint
            Constraint::Min(0),            // spacer pushes everything to top
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

    // Select the right mascot art for this agent, then pick full or right half
    let full = mascots::full_art(agent.name);
    let art_str = if cols >= 106 {
        full.to_string()
    } else {
        mascots::right_half(agent.name)
    };

    // Parse ANSI art to ratatui Lines with theme recoloring
    let art_lines: Vec<Line> = match &agent.theme {
        AgentTheme::Shift(shift) if shift.is_identity() => {
            pixel_art::parse_ansi_to_ratatui(&art_str)
        }
        AgentTheme::Shift(shift) => {
            pixel_art::parse_ansi_to_ratatui_themed(&art_str, *shift)
        }
        AgentTheme::Gradient(gradient) => {
            let height = art_str.lines().count();
            // Use visible column width, not byte length (ANSI escapes inflate byte count ~5x)
            let width = cols;
            pixel_art::parse_ansi_to_ratatui_gradient(&art_str, gradient, width, height)
        }
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
    let label = " unleash ";
    // Display width: "❯" is 1 column, plus label plus agent name
    let display_width = 1 + label.len() + agent.name.len();
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
            Span::raw(label),
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
