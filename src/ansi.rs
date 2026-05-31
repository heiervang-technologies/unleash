//! Minimal ANSI escape parser for the install log panel.
//!
//! The hermes/pi/opencode installers stream colored output via `\x1b[...m`
//! SGR sequences. Rendering those raw produces lines like `[0;32m✓[0m` —
//! readable but ugly. This module parses SGR codes (colors, bold, italic,
//! underline) into ratatui spans and silently strips non-SGR CSI/OSC
//! sequences (cursor movement, clear-screen, hyperlinks) that aren't useful
//! in a scrolling log.
//!
//! Per-line state only — colors set on one log line do not carry into the
//! next. Installers occasionally split a styled multi-line banner across
//! several stdout lines, so the middle rows render in `base_style`; this is
//! a known tradeoff against the complexity of cross-line state.
//!
//! Not a full terminal emulator. We accept whatever subset of SGR the
//! installer scripts actually emit and ignore the rest.
//!
//! The `nu_ansi_term`/`vte` crates would do more, but adding a dependency
//! for ~80 lines of straightforward parsing isn't worth it.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Parse a single log line and produce styled spans.
///
/// `base_style` is applied to every span and may be overlaid by SGR codes
/// found in the line (fg/bg/modifiers). Non-SGR escape sequences are
/// stripped.
pub fn parse_line(line: &str, base_style: Style) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current = base_style;
    let mut buf = String::new();
    let bytes = line.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        // ESC = 0x1b
        if b == 0x1b && i + 1 < bytes.len() {
            // Flush buffered text under current style.
            if !buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut buf), current));
            }
            match bytes[i + 1] {
                b'[' => {
                    // CSI: ESC [ <params> <final>
                    let mut j = i + 2;
                    while j < bytes.len() {
                        let c = bytes[j];
                        // Final byte: 0x40..=0x7e
                        if (0x40..=0x7e).contains(&c) {
                            if c == b'm' {
                                let params = &line[i + 2..j];
                                apply_sgr(params, &mut current, base_style);
                            }
                            // Drop the whole sequence whether SGR or not.
                            j += 1;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                }
                b']' => {
                    // OSC: ESC ] ... (BEL | ESC \)
                    let mut j = i + 2;
                    while j < bytes.len() {
                        if bytes[j] == 0x07 {
                            j += 1;
                            break;
                        }
                        if bytes[j] == 0x1b && j + 1 < bytes.len() && bytes[j + 1] == b'\\' {
                            j += 2;
                            break;
                        }
                        j += 1;
                    }
                    i = j;
                }
                _ => {
                    // Two-byte sequences (ESC <letter>) and anything else:
                    // drop the ESC and the next byte.
                    i += 2;
                }
            }
            continue;
        }
        // Skip stray carriage returns; they confuse line rendering when the
        // installer uses \r for progress redraws.
        if b == b'\r' {
            i += 1;
            continue;
        }
        // Pull a full UTF-8 codepoint so multi-byte chars (✓, →, ⚕) survive.
        let ch_len = utf8_char_len(b);
        let end = (i + ch_len).min(bytes.len());
        if let Ok(s) = std::str::from_utf8(&bytes[i..end]) {
            buf.push_str(s);
        }
        i = end;
    }

    if !buf.is_empty() {
        spans.push(Span::styled(buf, current));
    }
    if spans.is_empty() {
        spans.push(Span::styled(String::new(), base_style));
    }
    Line::from(spans)
}

fn utf8_char_len(first: u8) -> usize {
    if first < 0x80 {
        1
    } else if first < 0xc0 {
        // Continuation byte in isolation — treat as one to keep progress.
        1
    } else if first < 0xe0 {
        2
    } else if first < 0xf0 {
        3
    } else {
        4
    }
}

fn apply_sgr(params: &str, current: &mut Style, base_style: Style) {
    // Empty params (`ESC [ m`) is shorthand for reset.
    if params.is_empty() {
        *current = base_style;
        return;
    }
    let codes: Vec<u16> = params
        .split(';')
        .map(|s| s.trim().parse::<u16>().unwrap_or(0))
        .collect();

    let mut i = 0;
    while i < codes.len() {
        let code = codes[i];
        match code {
            0 => *current = base_style,
            1 => *current = current.add_modifier(Modifier::BOLD),
            2 => *current = current.add_modifier(Modifier::DIM),
            3 => *current = current.add_modifier(Modifier::ITALIC),
            4 => *current = current.add_modifier(Modifier::UNDERLINED),
            7 => *current = current.add_modifier(Modifier::REVERSED),
            22 => {
                *current = current
                    .remove_modifier(Modifier::BOLD)
                    .remove_modifier(Modifier::DIM)
            }
            23 => *current = current.remove_modifier(Modifier::ITALIC),
            24 => *current = current.remove_modifier(Modifier::UNDERLINED),
            27 => *current = current.remove_modifier(Modifier::REVERSED),
            30..=37 => *current = current.fg(basic_color(code as u8 - 30, false)),
            38 => {
                if let Some((color, consumed)) = parse_extended_color(&codes[i + 1..]) {
                    *current = current.fg(color);
                    i += consumed;
                }
            }
            39 => *current = current.fg(base_style.fg.unwrap_or(Color::Reset)),
            40..=47 => *current = current.bg(basic_color(code as u8 - 40, false)),
            48 => {
                if let Some((color, consumed)) = parse_extended_color(&codes[i + 1..]) {
                    *current = current.bg(color);
                    i += consumed;
                }
            }
            49 => *current = current.bg(base_style.bg.unwrap_or(Color::Reset)),
            90..=97 => *current = current.fg(basic_color(code as u8 - 90, true)),
            100..=107 => *current = current.bg(basic_color(code as u8 - 100, true)),
            _ => {}
        }
        i += 1;
    }
}

/// Parse the tail of a 38/48 SGR: either `5;N` (256-color) or `2;r;g;b`
/// (truecolor). Returns the resolved color and how many params were
/// consumed *after* the leading 38/48.
fn parse_extended_color(rest: &[u16]) -> Option<(Color, usize)> {
    match rest.first()? {
        5 => {
            let n = *rest.get(1)? as u8;
            Some((Color::Indexed(n), 2))
        }
        2 => {
            let r = *rest.get(1)? as u8;
            let g = *rest.get(2)? as u8;
            let b = *rest.get(3)? as u8;
            Some((Color::Rgb(r, g, b), 4))
        }
        _ => None,
    }
}

fn basic_color(idx: u8, bright: bool) -> Color {
    match (idx, bright) {
        (0, false) => Color::Black,
        (1, false) => Color::Red,
        (2, false) => Color::Green,
        (3, false) => Color::Yellow,
        (4, false) => Color::Blue,
        (5, false) => Color::Magenta,
        (6, false) => Color::Cyan,
        (7, false) => Color::Gray,
        (0, true) => Color::DarkGray,
        (1, true) => Color::LightRed,
        (2, true) => Color::LightGreen,
        (3, true) => Color::LightYellow,
        (4, true) => Color::LightBlue,
        (5, true) => Color::LightMagenta,
        (6, true) => Color::LightCyan,
        (7, true) => Color::White,
        _ => Color::Reset,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat_text(line: &Line<'_>) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn plain_text_passes_through() {
        let line = parse_line("hello world", Style::default());
        assert_eq!(flat_text(&line), "hello world");
        assert_eq!(line.spans.len(), 1);
    }

    #[test]
    fn basic_color_codes_become_styled_spans() {
        // Hermes installer:  \x1b[0;32m✓\x1b[0m Detected: linux
        let line = parse_line("\x1b[0;32m✓\x1b[0m Detected: linux", Style::default());
        assert_eq!(flat_text(&line), "✓ Detected: linux");
        let colored = line
            .spans
            .iter()
            .find(|s| s.content == "✓")
            .expect("checkmark span exists");
        assert_eq!(colored.style.fg, Some(Color::Green));
    }

    #[test]
    fn bold_and_color_combine() {
        let line = parse_line("\x1b[1;35mhi\x1b[0m", Style::default());
        let span = &line.spans[0];
        assert_eq!(span.content, "hi");
        assert_eq!(span.style.fg, Some(Color::Magenta));
        assert!(span.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn reset_returns_to_base_style() {
        let base = Style::default().fg(Color::DarkGray);
        let line = parse_line("\x1b[31mred\x1b[0m gray", base);
        assert_eq!(line.spans.len(), 2);
        assert_eq!(line.spans[0].style.fg, Some(Color::Red));
        assert_eq!(line.spans[1].style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn empty_params_are_treated_as_reset() {
        // Some shell scripts emit a bare `\x1b[m` to reset.
        let base = Style::default().fg(Color::DarkGray);
        let line = parse_line("\x1b[31mred\x1b[m gray", base);
        assert_eq!(line.spans.last().unwrap().style.fg, Some(Color::DarkGray));
    }

    #[test]
    fn non_sgr_csi_sequences_are_stripped() {
        // ESC[2K = erase line, ESC[H = cursor home.
        let line = parse_line("\x1b[2K\x1b[Hclean", Style::default());
        assert_eq!(flat_text(&line), "clean");
    }

    #[test]
    fn osc_sequences_are_stripped() {
        // ESC]0;title\x07 — set terminal title; should leave no trace.
        let line = parse_line("\x1b]0;the title\x07payload", Style::default());
        assert_eq!(flat_text(&line), "payload");
    }

    #[test]
    fn truecolor_rgb_is_parsed() {
        let line = parse_line("\x1b[38;2;10;20;30mhi", Style::default());
        assert_eq!(line.spans[0].style.fg, Some(Color::Rgb(10, 20, 30)));
    }

    #[test]
    fn indexed_256_color_is_parsed() {
        let line = parse_line("\x1b[38;5;208morange", Style::default());
        assert_eq!(line.spans[0].style.fg, Some(Color::Indexed(208)));
    }

    #[test]
    fn carriage_returns_are_stripped() {
        // Installers often emit \r for progress; we want clean log lines.
        let line = parse_line("downloading...\rdone", Style::default());
        assert_eq!(flat_text(&line), "downloading...done");
    }

    #[test]
    fn malformed_escape_at_end_does_not_panic() {
        let _ = parse_line("\x1b[", Style::default());
        let _ = parse_line("\x1b", Style::default());
        let _ = parse_line("\x1b]0;unterminated", Style::default());
    }

    #[test]
    fn bright_colors_use_light_variants() {
        let line = parse_line("\x1b[91mbright red\x1b[0m", Style::default());
        assert_eq!(line.spans[0].style.fg, Some(Color::LightRed));
    }
}
