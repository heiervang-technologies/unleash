use std::fmt;
use std::io::{self, Write};
use std::time::Duration;

/// State of a single agent line in the progress display.
#[allow(dead_code)]
pub enum LineState {
    Checking,
    UpToDate(String),
    UpdateAvailable {
        from: String,
        to: String,
    },
    Downloading {
        version: String,
        progress: f32,
    },
    Installing {
        version: String,
        progress: f32,
    },
    Building {
        version: String,
        phase: String,
    },
    Complete {
        from: String,
        to: String,
        duration: Duration,
    },
    Error(String),
    NotInstalled,
}

/// A single line in the progress display, tied to one agent.
pub struct ProgressLine {
    pub agent_name: String,
    pub state: LineState,
}

/// Manages multi-line progress rendering to the terminal.
pub struct ProgressRenderer {
    lines: Vec<ProgressLine>,
    rendered_once: bool,
    is_tty: bool,
}

// ANSI color codes
const GREEN: &str = "\x1b[32m";
const CYAN: &str = "\x1b[36m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";

const BAR_FILLED: char = '█';
const BAR_EMPTY: char = '░';

/// Minimum label column width (agent name padding).
const MIN_LABEL_WIDTH: usize = 16;

impl ProgressRenderer {
    /// Create a new renderer for the given agent names.
    /// All agents start in `Checking` state.
    pub fn new(agents: &[String]) -> Self {
        let is_tty = detect_tty();
        let lines = agents
            .iter()
            .map(|name| ProgressLine {
                agent_name: name.clone(),
                state: LineState::Checking,
            })
            .collect();
        Self {
            lines,
            rendered_once: false,
            is_tty,
        }
    }

    /// Update the state of a specific line by index.
    pub fn update(&mut self, index: usize, state: LineState) {
        if let Some(line) = self.lines.get_mut(index) {
            line.state = state;
        }
    }

    /// Render all lines to stdout.
    ///
    /// On a TTY, uses ANSI cursor-up codes to overwrite previous output.
    /// On a non-TTY (piped), prints plain lines without cursor movement.
    pub fn render(&mut self) {
        let mut stdout = io::stdout().lock();
        let term_width = terminal_width() as usize;
        let label_width = self.label_width();

        if self.is_tty && self.rendered_once {
            // Move cursor up to overwrite previous lines
            let n = self.lines.len();
            if n > 0 {
                write!(stdout, "\x1b[{}A", n).ok();
            }
        }

        for line in &self.lines {
            let formatted = format_line(line, label_width, term_width, self.is_tty);
            if self.is_tty {
                // Clear line and write
                write!(stdout, "\x1b[2K\r{}\n", formatted).ok();
            } else {
                writeln!(stdout, "{}", formatted).ok();
            }
        }

        stdout.flush().ok();
        self.rendered_once = true;
    }

    /// Print a final summary line after all updates are done.
    #[allow(dead_code)]
    pub fn finish(&self) {
        let mut updated = 0u32;
        let mut up_to_date = 0u32;
        let mut errors = 0u32;
        let mut not_installed = 0u32;

        for line in &self.lines {
            match &line.state {
                LineState::Complete { .. } => updated += 1,
                LineState::UpToDate(_) => up_to_date += 1,
                LineState::Error(_) => errors += 1,
                LineState::NotInstalled => not_installed += 1,
                _ => {}
            }
        }

        let mut parts: Vec<String> = Vec::new();
        if updated > 0 {
            parts.push(format!("{} updated", updated));
        }
        if up_to_date > 0 {
            parts.push(format!("{} up to date", up_to_date));
        }
        if errors > 0 {
            parts.push(format!("{} failed", errors));
        }
        if not_installed > 0 {
            parts.push(format!("{} not installed", not_installed));
        }

        let summary = parts.join(", ");
        let mut stdout = io::stdout().lock();
        if self.is_tty {
            writeln!(stdout, "\n{}", summary).ok();
        } else {
            writeln!(stdout, "{}", summary).ok();
        }
        stdout.flush().ok();
    }

    /// Compute the label column width from agent names.
    fn label_width(&self) -> usize {
        self.lines
            .iter()
            .map(|l| l.agent_name.len())
            .max()
            .unwrap_or(0)
            .max(MIN_LABEL_WIDTH)
            + 2 // padding
    }
}

/// Format a single progress line as a string.
fn format_line(line: &ProgressLine, label_width: usize, term_width: usize, color: bool) -> String {
    let name = &line.agent_name;
    let padded_name = format!("  {:<width$}", name, width = label_width);

    match &line.state {
        LineState::Checking => {
            let status = "checking...";
            if color {
                format!("{}{}{}{}", DIM, padded_name, status, RESET)
            } else {
                format!("{}{}", padded_name, status)
            }
        }
        LineState::UpToDate(version) => {
            let status = format!("{} (up to date)", version);
            if color {
                format!("{}{}{}{}", DIM, padded_name, status, RESET)
            } else {
                format!("{}{}", padded_name, status)
            }
        }
        LineState::UpdateAvailable { from, to } => {
            let status = format!("{} -> {} (update available)", from, to);
            if color {
                format!("{}{}{}{}", CYAN, padded_name, status, RESET)
            } else {
                format!("{}{}", padded_name, status)
            }
        }
        LineState::Downloading { version, progress } => format_bar_line(
            &padded_name,
            *progress,
            &format!("downloading {}", version),
            term_width,
            color,
            CYAN,
        ),
        LineState::Installing { version, progress } => format_bar_line(
            &padded_name,
            *progress,
            &format!("installing {}", version),
            term_width,
            color,
            CYAN,
        ),
        LineState::Building { version, phase } => {
            let status = format!("building {} ({})", version, phase);
            if color {
                format!("{}{}{}{}", CYAN, padded_name, status, RESET)
            } else {
                format!("{}{}", padded_name, status)
            }
        }
        LineState::Complete { from, to, duration } => {
            let secs = duration.as_secs_f64();
            let marker = if color {
                format!("{}✓{}", GREEN, RESET)
            } else {
                "✓".to_string()
            };
            let status = format!("{} -> {} ({:.1}s)", from, to, secs);
            if color {
                format!(
                    "  {}{} {:<width$}{}{}",
                    GREEN,
                    marker,
                    name,
                    status,
                    RESET,
                    width = label_width - 2
                )
            } else {
                format!(
                    "  {} {:<width$}{}",
                    marker,
                    name,
                    status,
                    width = label_width - 2
                )
            }
        }
        LineState::Error(msg) => {
            let marker = if color {
                format!("{}✗{}", RED, RESET)
            } else {
                "✗".to_string()
            };
            if color {
                format!(
                    "  {}{} {:<width$}{}{}",
                    RED,
                    marker,
                    name,
                    msg,
                    RESET,
                    width = label_width - 2
                )
            } else {
                format!(
                    "  {} {:<width$}{}",
                    marker,
                    name,
                    msg,
                    width = label_width - 2
                )
            }
        }
        LineState::NotInstalled => {
            let status = "(not installed)";
            if color {
                format!("{}{}{}{}", DIM, padded_name, status, RESET)
            } else {
                format!("{}{}", padded_name, status)
            }
        }
    }
}

/// Format a line with a progress bar.
fn format_bar_line(
    padded_name: &str,
    progress: f32,
    status_text: &str,
    term_width: usize,
    color: bool,
    bar_color: &str,
) -> String {
    let progress = progress.clamp(0.0, 1.0);
    let pct_str = format!("{:>3}%", (progress * 100.0) as u32);

    // Calculate available width for the bar:
    // [padded_name][bar][space][pct][space][status_text]
    let fixed_width = padded_name.len() + 1 + pct_str.len() + 1 + status_text.len();
    let bar_width = if term_width > fixed_width + 4 {
        term_width - fixed_width
    } else {
        10 // minimum bar width
    };

    let filled = (progress * bar_width as f32) as usize;
    let empty = bar_width.saturating_sub(filled);

    let bar: String = BAR_FILLED.to_string().repeat(filled) + &BAR_EMPTY.to_string().repeat(empty);

    if color {
        format!(
            "{}{}{}{} {} {}",
            bar_color, padded_name, bar, RESET, pct_str, status_text
        )
    } else {
        format!("{}{} {} {}", padded_name, bar, pct_str, status_text)
    }
}

/// Detect whether stdout is a TTY.
fn detect_tty() -> bool {
    use std::os::unix::io::AsRawFd;
    unsafe { libc_isatty(io::stdout().as_raw_fd()) }
}

/// Minimal isatty wrapper without pulling in a crate.
unsafe fn libc_isatty(fd: i32) -> bool {
    extern "C" {
        fn isatty(fd: i32) -> i32;
    }
    unsafe { isatty(fd) != 0 }
}

/// Get the terminal width, using crossterm if available, falling back to 80.
fn terminal_width() -> u16 {
    #[cfg(feature = "tui")]
    {
        crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80)
    }
    #[cfg(not(feature = "tui"))]
    {
        80
    }
}

impl fmt::Display for LineState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LineState::Checking => write!(f, "checking"),
            LineState::UpToDate(v) => write!(f, "{} (up to date)", v),
            LineState::UpdateAvailable { from, to } => write!(f, "{} -> {}", from, to),
            LineState::Downloading { version, progress } => {
                write!(f, "downloading {} ({:.0}%)", version, progress * 100.0)
            }
            LineState::Installing { version, progress } => {
                write!(f, "installing {} ({:.0}%)", version, progress * 100.0)
            }
            LineState::Building { version, phase } => write!(f, "building {} ({})", version, phase),
            LineState::Complete { from, to, duration } => {
                write!(f, "{} -> {} ({:.1}s)", from, to, duration.as_secs_f64())
            }
            LineState::Error(msg) => write!(f, "error: {}", msg),
            LineState::NotInstalled => write!(f, "not installed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_creation() {
        let agents = vec!["Claude Code".to_string(), "Codex".to_string()];
        let renderer = ProgressRenderer::new(&agents);
        assert_eq!(renderer.lines.len(), 2);
        assert_eq!(renderer.lines[0].agent_name, "Claude Code");
        assert_eq!(renderer.lines[1].agent_name, "Codex");
    }

    #[test]
    fn test_update_state() {
        let agents = vec!["Claude Code".to_string()];
        let mut renderer = ProgressRenderer::new(&agents);
        renderer.update(0, LineState::UpToDate("2.1.77".to_string()));
        assert!(matches!(&renderer.lines[0].state, LineState::UpToDate(v) if v == "2.1.77"));
    }

    #[test]
    fn test_update_out_of_bounds() {
        let agents = vec!["Claude Code".to_string()];
        let mut renderer = ProgressRenderer::new(&agents);
        // Should not panic
        renderer.update(99, LineState::Checking);
    }

    #[test]
    fn test_format_bar_line_plain() {
        let result = format_bar_line("  test            ", 0.5, "downloading v1", 80, false, CYAN);
        assert!(result.contains("50%"));
        assert!(result.contains("downloading v1"));
        assert!(result.contains(BAR_FILLED));
        assert!(result.contains(BAR_EMPTY));
    }

    #[test]
    fn test_progress_clamp() {
        // Progress > 1.0 should be clamped
        let result = format_bar_line("  test  ", 1.5, "done", 80, false, CYAN);
        assert!(result.contains("100%"));
        // Progress < 0.0 should be clamped
        let result = format_bar_line("  test  ", -0.5, "start", 80, false, CYAN);
        assert!(result.contains("  0%"));
    }

    #[test]
    fn test_line_state_display() {
        let state = LineState::Complete {
            from: "1.0".to_string(),
            to: "2.0".to_string(),
            duration: Duration::from_secs_f64(3.25),
        };
        let s = format!("{}", state);
        assert!(s.contains("1.0 -> 2.0"));
        assert!(s.contains("3.2s"));
    }
}
