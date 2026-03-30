//! splash — Interactive agent picker for the unleash installer
//!
//! Lightweight TUI: displays mascot ANSI art recolored per agent theme.
//! Arrow keys cycle agents, Enter confirms, q/Esc quits.
//! Prints the selected agent name to stdout on exit.

use std::io::{self, Write};

use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{self, ClearType},
    ExecutableCommand,
};
use unleash::theme::{transform_theme_color, ThemeShift};

/// Embedded mascot art (compile-time)
const MASCOT_ART: &[u8] = include_bytes!("../assets/mascot.claude.ans");

struct Agent {
    name: &'static str,
    shift: ThemeShift,
    accent: (u8, u8, u8),
}

const AGENTS: [Agent; 4] = [
    Agent {
        name: "claude",
        shift: ThemeShift {
            hue: 0.0,
            sat_scale: 1.0,
        },
        accent: (217, 119, 87),
    },
    Agent {
        name: "codex",
        shift: ThemeShift {
            hue: 345.23,
            sat_scale: 0.0,
        },
        accent: (140, 140, 140),
    },
    Agent {
        name: "gemini",
        shift: ThemeShift {
            hue: 260.0,
            sat_scale: 1.0,
        },
        accent: (162, 87, 217),
    },
    Agent {
        name: "opencode",
        shift: ThemeShift {
            hue: 200.0,
            sat_scale: 1.0,
        },
        accent: (87, 142, 217),
    },
];

// ── ANSI recoloring ────────────────────────────────────────────

/// Recolor all 24-bit RGB ANSI escapes using the given theme shift.
fn recolor_art(art: &[u8], shift: ThemeShift) -> Vec<u8> {
    if shift.is_identity() {
        return art.to_vec();
    }

    let mut out = Vec::with_capacity(art.len());
    let mut i = 0;

    while i < art.len() {
        if art[i] == 0x1b && i + 1 < art.len() && art[i + 1] == b'[' {
            let start = i;
            i += 2;
            let params_start = i;
            while i < art.len() && art[i] != b'm' {
                i += 1;
            }
            if i < art.len() {
                let params = &art[params_start..i];
                i += 1;
                if let Some(recolored) = try_recolor_sequence(params, shift) {
                    out.extend_from_slice(b"\x1b[");
                    out.extend_from_slice(&recolored);
                    out.push(b'm');
                } else {
                    out.extend_from_slice(&art[start..i]);
                }
            } else {
                out.extend_from_slice(&art[start..i]);
            }
        } else {
            out.push(art[i]);
            i += 1;
        }
    }
    out
}

fn try_recolor_sequence(params: &[u8], shift: ThemeShift) -> Option<Vec<u8>> {
    let s = std::str::from_utf8(params).ok()?;
    let parts: Vec<&str> = s.split(';').collect();
    if parts.len() == 5 && parts[1] == "2" && (parts[0] == "38" || parts[0] == "48") {
        let r: u8 = parts[2].parse().ok()?;
        let g: u8 = parts[3].parse().ok()?;
        let b: u8 = parts[4].parse().ok()?;
        let (nr, ng, nb) = transform_theme_color(r, g, b, shift);
        Some(format!("{};2;{};{};{}", parts[0], nr, ng, nb).into_bytes())
    } else {
        None
    }
}

// ── Responsive cropping ────────────────────────────────────────

fn visible_width(line: &[u8]) -> usize {
    let mut w = 0;
    let mut i = 0;
    while i < line.len() {
        if line[i] == 0x1b && i + 1 < line.len() && line[i + 1] == b'[' {
            i += 2;
            while i < line.len() && line[i] != b'm' {
                i += 1;
            }
            if i < line.len() {
                i += 1;
            }
        } else {
            w += 1;
            i += 1;
        }
    }
    w
}

/// Skip `n` visible characters, preserving ANSI color state.
fn skip_visible(line: &[u8], n: usize) -> Vec<u8> {
    let mut skipped = 0;
    let mut i = 0;
    let mut last_fg: Vec<u8> = Vec::new();
    let mut last_bg: Vec<u8> = Vec::new();

    while i < line.len() && skipped < n {
        if line[i] == 0x1b && i + 1 < line.len() && line[i + 1] == b'[' {
            let start = i;
            i += 2;
            while i < line.len() && line[i] != b'm' {
                i += 1;
            }
            if i < line.len() {
                i += 1;
            }
            let seq = &line[start..i];
            if seq.windows(4).any(|w| w == b"38;2") {
                last_fg = seq.to_vec();
            } else if seq.windows(4).any(|w| w == b"48;2") {
                last_bg = seq.to_vec();
            } else if seq == b"\x1b[0m" {
                last_fg.clear();
                last_bg.clear();
            }
        } else {
            skipped += 1;
            i += 1;
        }
    }

    let mut out = Vec::new();
    out.extend_from_slice(&last_fg);
    out.extend_from_slice(&last_bg);
    out.extend_from_slice(&line[i..]);
    out
}

/// Truncate to `max_cols` visible characters.
fn truncate_visible(line: &[u8], max_cols: usize) -> Vec<u8> {
    let mut out = Vec::new();
    let mut visible = 0;
    let mut i = 0;

    while i < line.len() {
        if line[i] == 0x1b && i + 1 < line.len() && line[i + 1] == b'[' {
            let start = i;
            i += 2;
            while i < line.len() && line[i] != b'm' {
                i += 1;
            }
            if i < line.len() {
                i += 1;
            }
            out.extend_from_slice(&line[start..i]);
        } else {
            if visible >= max_cols {
                break;
            }
            out.push(line[i]);
            visible += 1;
            i += 1;
        }
    }
    out.extend_from_slice(b"\x1b[0m");
    out
}

/// Crop art to terminal dimensions, centering horizontally.
fn crop_art(lines: &[Vec<u8>], cols: usize, rows: usize) -> Vec<Vec<u8>> {
    if lines.is_empty() {
        return Vec::new();
    }

    let mid = lines.len() / 2;
    let art_width = visible_width(&lines[mid.min(lines.len() - 1)]);

    let h_skip = art_width.saturating_sub(cols) / 2;

    lines
        .iter()
        .take(rows)
        .map(|line| {
            let line = if h_skip > 0 {
                skip_visible(line, h_skip)
            } else {
                line.clone()
            };
            truncate_visible(&line, cols)
        })
        .collect()
}

// ── Rendering ──────────────────────────────────────────────────

fn fg(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Layout overhead: blank + title(2) + blank + input(3) + hint(2) = 8 lines
const LAYOUT_OVERHEAD: usize = 9;

fn render(
    out: &mut impl Write,
    agent: &Agent,
    art_lines: &[Vec<u8>],
    cols: u16,
    rows: u16,
) -> io::Result<()> {
    let mascot_rows = (rows as usize).saturating_sub(LAYOUT_OVERHEAD);
    let cropped = crop_art(art_lines, cols as usize, mascot_rows);

    let (ar, ag, ab) = agent.accent;
    let accent = fg(ar, ag, ab);
    let reset = "\x1b[0m";
    let dim = "\x1b[2m";
    let bold = "\x1b[1m";

    // Clear + home
    out.execute(terminal::Clear(ClearType::All))?;
    out.execute(cursor::MoveTo(0, 0))?;

    // Title
    writeln!(out)?;
    writeln!(out, "  {bold}{accent}unleash{reset}  {dim}installer{reset}")?;
    writeln!(out)?;

    // Mascot
    for line in &cropped {
        out.write_all(line)?;
        writeln!(out)?;
    }
    writeln!(out)?;

    // Input box
    let text = format!("unleash {}", agent.name);
    let inner_width: usize = 32;
    let pad = inner_width.saturating_sub(text.len() + 3);
    let padding = " ".repeat(pad);
    let bar = "─".repeat(inner_width);

    writeln!(out, "  {accent}╭{bar}╮{reset}")?;
    writeln!(
        out,
        "  {accent}│{reset} {accent}❯{reset} {bold}{accent}{text}{reset}{padding} {accent}│{reset}"
    )?;
    writeln!(out, "  {accent}╰{bar}╯{reset}")?;

    // Hint
    writeln!(out)?;
    write!(
        out,
        "  {dim}←/→ cycle agents    Enter confirm    q quit{reset}"
    )?;

    out.flush()
}

// ── Main ───────────────────────────────────────────────────────

fn main() -> io::Result<()> {
    // Pre-render all 4 color variants (split into lines)
    let variants: Vec<Vec<Vec<u8>>> = AGENTS
        .iter()
        .map(|a| {
            let recolored = recolor_art(MASCOT_ART, a.shift);
            recolored
                .split(|&b| b == b'\n')
                .filter(|l| !l.is_empty())
                .map(|l| l.to_vec())
                .collect()
        })
        .collect();

    // Enter raw mode (crossterm restores on drop via the guard)
    terminal::enable_raw_mode()?;
    let mut stderr = io::stderr();
    stderr.execute(cursor::Hide)?;

    // Ensure cleanup on panic/exit
    let cleanup = || {
        let _ = terminal::disable_raw_mode();
        let _ = io::stderr().execute(cursor::Show);
    };

    let result = run_loop(&variants, &mut stderr);

    cleanup();
    result
}

fn run_loop(variants: &[Vec<Vec<u8>>], out: &mut impl Write) -> io::Result<()> {
    let mut current: usize = 0;
    let (mut cols, mut rows) = terminal::size()?;

    render(out, &AGENTS[current], &variants[current], cols, rows)?;

    loop {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
                    let _ = out.execute(terminal::Clear(ClearType::All));
                    let _ = out.execute(cursor::MoveTo(0, 0));
                    eprintln!("Installation cancelled.");
                    std::process::exit(0);
                }
                KeyCode::Right | KeyCode::Down => {
                    current = (current + 1) % AGENTS.len();
                    render(out, &AGENTS[current], &variants[current], cols, rows)?;
                }
                KeyCode::Left | KeyCode::Up => {
                    current = (current + AGENTS.len() - 1) % AGENTS.len();
                    render(out, &AGENTS[current], &variants[current], cols, rows)?;
                }
                KeyCode::Enter => {
                    let _ = out.execute(terminal::Clear(ClearType::All));
                    let _ = out.execute(cursor::MoveTo(0, 0));
                    // Print selected agent to stdout (not stderr)
                    println!("{}", AGENTS[current].name);
                    return Ok(());
                }
                _ => {}
            },
            Event::Resize(c, r) => {
                cols = c;
                rows = r;
                render(out, &AGENTS[current], &variants[current], cols, rows)?;
            }
            _ => {}
        }
    }
}
