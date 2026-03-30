//! ANSI Pixel Art Renderer
//!
//! Renders arbitrary "images" as colored ASCII grids in the terminal.
//! Supports 24-bit RGB colors via ANSI escape sequences.
//!
//! Dynamic color cycling feature inspired by cac taurus.

#![allow(dead_code)]

use std::collections::HashMap;
use std::io::{self, Write};

#[cfg(feature = "tui")]
use ratatui::{
    style::{Color as RatatuiColor, Style as RatatuiStyle},
    text::{Line as RatatuiLine, Span as RatatuiSpan},
};

/// RGB color
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    /// Create from hex string like "ff5500" or "#ff5500"
    pub fn from_hex(hex: &str) -> Option<Self> {
        let hex = hex.trim_start_matches('#');
        if hex.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
        let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
        let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }

    /// ANSI escape for foreground color
    pub fn fg_ansi(&self) -> String {
        format!("\x1b[38;2;{};{};{}m", self.r, self.g, self.b)
    }

    /// ANSI escape for background color
    pub fn bg_ansi(&self) -> String {
        format!("\x1b[48;2;{};{};{}m", self.r, self.g, self.b)
    }
}

/// Common colors
impl Color {
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);
    pub const RED: Self = Self::rgb(255, 0, 0);
    pub const GREEN: Self = Self::rgb(0, 255, 0);
    pub const BLUE: Self = Self::rgb(0, 0, 255);
    pub const YELLOW: Self = Self::rgb(255, 255, 0);
    pub const CYAN: Self = Self::rgb(0, 255, 255);
    pub const MAGENTA: Self = Self::rgb(255, 0, 255);
    pub const ORANGE: Self = Self::rgb(255, 165, 0);
    pub const PURPLE: Self = Self::rgb(128, 0, 128);
    pub const PINK: Self = Self::rgb(255, 192, 203);
    pub const BROWN: Self = Self::rgb(139, 69, 19);
    pub const GRAY: Self = Self::rgb(128, 128, 128);
    pub const LIGHT_GRAY: Self = Self::rgb(192, 192, 192);
    pub const DARK_GRAY: Self = Self::rgb(64, 64, 64);

    // Claude-inspired colors
    pub const CLAUDE_ORANGE: Self = Self::rgb(217, 119, 87);
    pub const CLAUDE_BEIGE: Self = Self::rgb(250, 240, 230);
    pub const CLAUDE_DARK: Self = Self::rgb(45, 35, 30);

    // Vibrant lava lamp palette (4 colors) - idea by cac taurus
    pub const LAVA_ORANGE: Self = Self::rgb(255, 140, 90); // Vibrant orange
    pub const LAVA_PINK: Self = Self::rgb(255, 100, 150); // Hot pink
    pub const LAVA_PURPLE: Self = Self::rgb(200, 100, 255); // Electric purple
    pub const LAVA_CYAN: Self = Self::rgb(100, 220, 255); // Bright cyan
}

/// Dynamic color palette for lava lamp effect
/// Cycles through 4 vibrant colors based on animation frame
#[cfg(feature = "tui")]
pub fn get_lava_palette(frame: usize) -> [Color; 4] {
    // Rotate the palette based on frame to create flowing effect
    let palettes: [[Color; 4]; 4] = [
        [
            Color::LAVA_ORANGE,
            Color::LAVA_PINK,
            Color::LAVA_PURPLE,
            Color::LAVA_CYAN,
        ],
        [
            Color::LAVA_PINK,
            Color::LAVA_PURPLE,
            Color::LAVA_CYAN,
            Color::LAVA_ORANGE,
        ],
        [
            Color::LAVA_PURPLE,
            Color::LAVA_CYAN,
            Color::LAVA_ORANGE,
            Color::LAVA_PINK,
        ],
        [
            Color::LAVA_CYAN,
            Color::LAVA_ORANGE,
            Color::LAVA_PINK,
            Color::LAVA_PURPLE,
        ],
    ];
    palettes[(frame / 8) % 4]
}

/// Transform a color from the original orange palette to the current lava palette
/// This creates a smooth color shift effect
#[cfg(feature = "tui")]
pub fn transform_to_lava_color(r: u8, g: u8, b: u8, frame: usize) -> (u8, u8, u8) {
    // Check if this is an orange-ish color (the figure) vs gray (background/details)
    let is_orange = r > 180 && g < 180 && b < 180;
    let is_skin = r > 200 && g > 100 && g < 200 && b > 80 && b < 160;

    if is_orange || is_skin {
        // Get current palette position based on pixel brightness and frame
        let brightness = ((r as u32 + g as u32 + b as u32) / 3) as f32 / 255.0;
        let palette = get_lava_palette(frame);

        // Use brightness to interpolate between palette colors
        let palette_idx = (brightness * 3.0) as usize;
        let palette_idx = palette_idx.min(3);

        // Add some variation based on the original color's position
        let variation = ((r as usize + frame) % 4) as f32 / 4.0;
        let color = palette[(palette_idx + (variation * 2.0) as usize) % 4];

        // Blend with original brightness for depth
        let blend = 0.7 + brightness * 0.3;
        (
            ((color.r as f32 * blend).min(255.0)) as u8,
            ((color.g as f32 * blend).min(255.0)) as u8,
            ((color.b as f32 * blend).min(255.0)) as u8,
        )
    } else {
        // Keep non-orange colors (grays, etc) unchanged
        (r, g, b)
    }
}

/// A pixel art image defined by a grid and color palette
#[derive(Clone, Debug)]
pub struct PixelArt {
    /// The character grid - each char maps to a color in the palette
    pub grid: Vec<String>,
    /// Maps characters to colors. Space = transparent.
    pub palette: HashMap<char, Color>,
    /// Character to use for rendering (default: block chars)
    pub render_char: RenderStyle,
}

/// How to render each pixel
#[derive(Clone, Debug, Default)]
pub enum RenderStyle {
    /// Use background color with spaces (2 chars wide for square pixels)
    #[default]
    BlockBg,
    /// Use foreground colored block characters
    BlockFg,
    /// Use a custom character with foreground color
    Custom(char),
    /// Half-block rendering (1 char = 2 vertical pixels)
    HalfBlock,
}

impl PixelArt {
    pub fn new() -> Self {
        Self {
            grid: Vec::new(),
            palette: HashMap::new(),
            render_char: RenderStyle::BlockBg,
        }
    }

    /// Create from a multi-line string and palette
    pub fn from_str(art: &str, palette: HashMap<char, Color>) -> Self {
        let grid: Vec<String> = art.lines().map(|s| s.to_string()).collect();
        Self {
            grid,
            palette,
            render_char: RenderStyle::BlockBg,
        }
    }

    /// Set the render style
    pub fn with_style(mut self, style: RenderStyle) -> Self {
        self.render_char = style;
        self
    }

    /// Get dimensions (width, height)
    pub fn dimensions(&self) -> (usize, usize) {
        let height = self.grid.len();
        let width = self
            .grid
            .iter()
            .map(|row| row.chars().count())
            .max()
            .unwrap_or(0);
        (width, height)
    }

    /// Render to a string with ANSI codes
    pub fn render(&self) -> String {
        let mut output = String::new();
        let reset = "\x1b[0m";

        for row in &self.grid {
            for ch in row.chars() {
                if ch == ' ' || ch == '.' {
                    // Transparent - just add spacing
                    match &self.render_char {
                        RenderStyle::BlockBg => output.push_str("  "),
                        RenderStyle::HalfBlock => output.push(' '),
                        _ => output.push(' '),
                    }
                } else if let Some(color) = self.palette.get(&ch) {
                    match &self.render_char {
                        RenderStyle::BlockBg => {
                            output.push_str(&color.bg_ansi());
                            output.push_str("  ");
                            output.push_str(reset);
                        }
                        RenderStyle::BlockFg => {
                            output.push_str(&color.fg_ansi());
                            output.push_str("\u{2588}\u{2588}"); // Full block
                            output.push_str(reset);
                        }
                        RenderStyle::Custom(c) => {
                            output.push_str(&color.fg_ansi());
                            output.push(*c);
                            output.push_str(reset);
                        }
                        RenderStyle::HalfBlock => {
                            output.push_str(&color.fg_ansi());
                            output.push('\u{2580}'); // Upper half block
                            output.push_str(reset);
                        }
                    }
                } else {
                    // Unknown char - render as-is
                    output.push(ch);
                    if matches!(self.render_char, RenderStyle::BlockBg) {
                        output.push(ch);
                    }
                }
            }
            output.push('\n');
        }

        output
    }

    /// Render half-block style (2 rows = 1 line, more compact)
    pub fn render_halfblock(&self) -> String {
        let mut output = String::new();
        let reset = "\x1b[0m";
        let (width, height) = self.dimensions();

        // Process 2 rows at a time
        let mut y = 0;
        while y < height {
            for x in 0..width {
                let top_char = self.grid.get(y).and_then(|row| row.chars().nth(x));
                let bot_char = self.grid.get(y + 1).and_then(|row| row.chars().nth(x));

                let top_color = top_char.and_then(|c| {
                    if c == ' ' || c == '.' {
                        None
                    } else {
                        self.palette.get(&c)
                    }
                });
                let bot_color = bot_char.and_then(|c| {
                    if c == ' ' || c == '.' {
                        None
                    } else {
                        self.palette.get(&c)
                    }
                });

                match (top_color, bot_color) {
                    (None, None) => output.push(' '),
                    (Some(c), None) => {
                        output.push_str(&c.fg_ansi());
                        output.push('\u{2580}'); // Upper half
                        output.push_str(reset);
                    }
                    (None, Some(c)) => {
                        output.push_str(&c.fg_ansi());
                        output.push('\u{2584}'); // Lower half
                        output.push_str(reset);
                    }
                    (Some(top), Some(bot)) => {
                        if top == bot {
                            output.push_str(&top.bg_ansi());
                            output.push(' ');
                            output.push_str(reset);
                        } else {
                            // Top = foreground, Bot = background
                            output.push_str(&top.fg_ansi());
                            output.push_str(&bot.bg_ansi());
                            output.push('\u{2580}');
                            output.push_str(reset);
                        }
                    }
                }
            }
            output.push('\n');
            y += 2;
        }

        output
    }

    /// Print directly to stdout
    pub fn print(&self) {
        print!("{}", self.render());
        io::stdout().flush().ok();
    }

    /// Print half-block version
    pub fn print_halfblock(&self) {
        print!("{}", self.render_halfblock());
        io::stdout().flush().ok();
    }
}

/// Builder for creating pixel art with a fluent API
pub struct PixelArtBuilder {
    art: PixelArt,
}

impl PixelArtBuilder {
    pub fn new() -> Self {
        Self {
            art: PixelArt::new(),
        }
    }

    /// Add a row to the grid
    pub fn row(mut self, row: &str) -> Self {
        self.art.grid.push(row.to_string());
        self
    }

    /// Define a color for a character
    pub fn color(mut self, ch: char, color: Color) -> Self {
        self.art.palette.insert(ch, color);
        self
    }

    /// Define a color from hex
    pub fn hex(mut self, ch: char, hex: &str) -> Self {
        if let Some(color) = Color::from_hex(hex) {
            self.art.palette.insert(ch, color);
        }
        self
    }

    /// Set render style
    pub fn style(mut self, style: RenderStyle) -> Self {
        self.art.render_char = style;
        self
    }

    /// Build the final PixelArt
    pub fn build(self) -> PixelArt {
        self.art
    }
}

impl Default for PixelArtBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for PixelArt {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse ANSI escape sequences and convert to ratatui styled lines
#[cfg(feature = "tui")]
pub fn parse_ansi_to_ratatui(ansi_text: &str) -> Vec<RatatuiLine<'static>> {
    // Delegate to themed parser with identity shift (no change)
    parse_ansi_to_ratatui_themed(ansi_text, crate::theme::ThemeShift::identity())
}

// ── Generic ANSI→ratatui parser ──────────────────────────────────
// The lava and themed parsers share identical state-machine logic;
// only the RGB transform differs.  This generic pair eliminates that
// duplication.

/// Parse ANSI line to ratatui spans, applying `transform` to each 24-bit RGB color.
#[cfg(feature = "tui")]
fn parse_ansi_line_to_spans_with(
    line: &str,
    transform: &impl Fn(u8, u8, u8) -> (u8, u8, u8),
) -> Vec<RatatuiSpan<'static>> {
    let mut spans: Vec<RatatuiSpan<'static>> = Vec::new();
    let mut current_style = RatatuiStyle::default();
    let mut current_text = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if let Some(&'[') = chars.peek() {
                chars.next();

                if !current_text.is_empty() {
                    spans.push(RatatuiSpan::styled(
                        std::mem::take(&mut current_text),
                        current_style,
                    ));
                }

                let mut seq = String::new();
                while let Some(&c) = chars.peek() {
                    if c == 'm' {
                        chars.next();
                        break;
                    }
                    seq.push(chars.next().unwrap());
                }

                current_style = parse_ansi_sequence_with(&seq, current_style, transform);
            } else {
                current_text.push(ch);
            }
        } else {
            current_text.push(ch);
        }
    }

    if !current_text.is_empty() {
        spans.push(RatatuiSpan::styled(current_text, current_style));
    }

    if spans.is_empty() {
        spans.push(RatatuiSpan::raw(""));
    }

    spans
}

/// Apply an ANSI CSI sequence to a style, transforming 24-bit colors via `transform`.
#[cfg(feature = "tui")]
fn parse_ansi_sequence_with(
    seq: &str,
    mut style: RatatuiStyle,
    transform: &impl Fn(u8, u8, u8) -> (u8, u8, u8),
) -> RatatuiStyle {
    let parts: Vec<&str> = seq.split(';').collect();
    let mut i = 0;

    while i < parts.len() {
        match parts[i] {
            "0" => {
                style = RatatuiStyle::default();
            }
            "38" => {
                if i + 1 < parts.len() && parts[i + 1] == "2" && i + 4 < parts.len() {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[i + 2].parse::<u8>(),
                        parts[i + 3].parse::<u8>(),
                        parts[i + 4].parse::<u8>(),
                    ) {
                        let (nr, ng, nb) = transform(r, g, b);
                        style = style.fg(RatatuiColor::Rgb(nr, ng, nb));
                    }
                    i += 4;
                }
            }
            "48" => {
                if i + 1 < parts.len() && parts[i + 1] == "2" && i + 4 < parts.len() {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[i + 2].parse::<u8>(),
                        parts[i + 3].parse::<u8>(),
                        parts[i + 4].parse::<u8>(),
                    ) {
                        let (nr, ng, nb) = transform(r, g, b);
                        style = style.bg(RatatuiColor::Rgb(nr, ng, nb));
                    }
                    i += 4;
                }
            }
            _ => {}
        }
        i += 1;
    }

    style
}

/// Parse ANSI with dynamic lava lamp color transformation
#[cfg(feature = "tui")]
pub fn parse_ansi_to_ratatui_lava(
    ansi_text: &str,
    animation_frame: usize,
) -> Vec<RatatuiLine<'static>> {
    let transform = |r, g, b| transform_to_lava_color(r, g, b, animation_frame);
    ansi_text
        .lines()
        .map(|line| RatatuiLine::from(parse_ansi_line_to_spans_with(line, &transform)))
        .collect()
}

/// Parse ANSI with theme hue rotation
#[cfg(feature = "tui")]
pub fn parse_ansi_to_ratatui_themed(
    ansi_text: &str,
    shift: crate::theme::ThemeShift,
) -> Vec<RatatuiLine<'static>> {
    use crate::theme::transform_theme_color;
    let transform = |r, g, b| transform_theme_color(r, g, b, shift);
    ansi_text
        .lines()
        .map(|line| RatatuiLine::from(parse_ansi_line_to_spans_with(line, &transform)))
        .collect()
}

/// Parse ANSI art with a diagonal gradient applied to orange-tone pixels.
/// The gradient interpolates based on (x + y) / (width + height).
#[cfg(feature = "tui")]
pub fn parse_ansi_to_ratatui_gradient(
    ansi_text: &str,
    gradient: &crate::theme::GradientTheme,
    width: usize,
    height: usize,
) -> Vec<RatatuiLine<'static>> {
    let mut lines: Vec<RatatuiLine<'static>> = Vec::new();
    let total = (width + height).max(1) as f64;

    for (y, line) in ansi_text.lines().enumerate() {
        let spans = parse_ansi_line_to_spans_gradient(line, y, total, gradient);
        lines.push(RatatuiLine::from(spans));
    }

    lines
}

/// Parse a single line of ANSI text with gradient color transform
#[cfg(feature = "tui")]
fn parse_ansi_line_to_spans_gradient(
    line: &str,
    y: usize,
    total_diag: f64,
    gradient: &crate::theme::GradientTheme,
) -> Vec<RatatuiSpan<'static>> {
    use crate::theme::transform_gradient_color;

    let mut spans: Vec<RatatuiSpan<'static>> = Vec::new();
    let mut current_fg: Option<(u8, u8, u8)> = None;
    let mut current_bg: Option<(u8, u8, u8)> = None;
    let mut current_text = String::new();
    let mut col: usize = 0; // visible column position
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            if let Some(&'[') = chars.peek() {
                chars.next();

                if !current_text.is_empty() {
                    // Flush text — apply gradient at the midpoint of this span
                    let mid_col = col.saturating_sub(current_text.len() / 2);
                    let t = (mid_col + y) as f64 / total_diag;
                    let mut style = RatatuiStyle::default();
                    if let Some((r, g, b)) = current_fg {
                        let (nr, ng, nb) = transform_gradient_color(r, g, b, gradient, t);
                        style = style.fg(RatatuiColor::Rgb(nr, ng, nb));
                    }
                    if let Some((r, g, b)) = current_bg {
                        let (nr, ng, nb) = transform_gradient_color(r, g, b, gradient, t);
                        style = style.bg(RatatuiColor::Rgb(nr, ng, nb));
                    }
                    spans.push(RatatuiSpan::styled(
                        std::mem::take(&mut current_text),
                        style,
                    ));
                }

                let mut seq = String::new();
                while let Some(&c) = chars.peek() {
                    if c == 'm' {
                        chars.next();
                        break;
                    }
                    seq.push(chars.next().unwrap());
                }

                // Parse the ANSI sequence to extract raw RGB (before gradient transform)
                let (new_fg, new_bg) = parse_gradient_sequence_colors(&seq, current_fg, current_bg);
                current_fg = new_fg;
                current_bg = new_bg;
            } else {
                current_text.push(ch);
                col += 1;
            }
        } else {
            current_text.push(ch);
            col += 1;
        }
    }

    if !current_text.is_empty() {
        let mid_col = col.saturating_sub(current_text.len() / 2);
        let t = (mid_col + y) as f64 / total_diag;
        let mut style = RatatuiStyle::default();
        if let Some((r, g, b)) = current_fg {
            let (nr, ng, nb) = transform_gradient_color(r, g, b, gradient, t);
            style = style.fg(RatatuiColor::Rgb(nr, ng, nb));
        }
        if let Some((r, g, b)) = current_bg {
            let (nr, ng, nb) = transform_gradient_color(r, g, b, gradient, t);
            style = style.bg(RatatuiColor::Rgb(nr, ng, nb));
        }
        spans.push(RatatuiSpan::styled(current_text, style));
    }

    if spans.is_empty() {
        spans.push(RatatuiSpan::raw(""));
    }

    spans
}

/// Extract raw RGB colors from an ANSI sequence without transforming them.
/// Returns (fg, bg) as Option<(u8,u8,u8)>.
#[cfg(feature = "tui")]
fn parse_gradient_sequence_colors(
    seq: &str,
    mut fg: Option<(u8, u8, u8)>,
    mut bg: Option<(u8, u8, u8)>,
) -> (Option<(u8, u8, u8)>, Option<(u8, u8, u8)>) {
    let parts: Vec<&str> = seq.split(';').collect();
    let mut i = 0;

    while i < parts.len() {
        match parts[i] {
            "0" => {
                fg = None;
                bg = None;
            }
            "38" => {
                if i + 1 < parts.len() && parts[i + 1] == "2" && i + 4 < parts.len() {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[i + 2].parse::<u8>(),
                        parts[i + 3].parse::<u8>(),
                        parts[i + 4].parse::<u8>(),
                    ) {
                        fg = Some((r, g, b));
                    }
                    i += 4;
                }
            }
            "48" => {
                if i + 1 < parts.len() && parts[i + 1] == "2" && i + 4 < parts.len() {
                    if let (Ok(r), Ok(g), Ok(b)) = (
                        parts[i + 2].parse::<u8>(),
                        parts[i + 3].parse::<u8>(),
                        parts[i + 4].parse::<u8>(),
                    ) {
                        bg = Some((r, g, b));
                    }
                    i += 4;
                }
            }
            _ => {}
        }
        i += 1;
    }

    (fg, bg)
}

/// Parse a single line of ANSI text to ratatui Spans
#[cfg(feature = "tui")]
fn parse_ansi_line_to_spans(line: &str) -> Vec<RatatuiSpan<'static>> {
    parse_ansi_line_to_spans_with(line, &|r, g, b| (r, g, b))
}

/// Parse ANSI sequence codes and update style
#[cfg(feature = "tui")]
fn parse_ansi_sequence(seq: &str, style: RatatuiStyle) -> RatatuiStyle {
    parse_ansi_sequence_with(seq, style, &|r, g, b| (r, g, b))
}

/// Split a single line of ANSI-escaped text at a visible character column.
///
/// Returns `(left, right)` where:
/// - `left` contains the first `split_col` visible characters with their escape codes,
///   terminated with a reset `\x1b[0m`.
/// - `right` starts with the ANSI state (fg/bg color) that was active at the split point,
///   then continues with the remaining visible characters.
///
/// "Visible characters" means non-escape-sequence characters — the ones that occupy
/// a column in the terminal.
fn split_ansi_line(line: &str, split_col: usize) -> (String, String) {
    let mut left = String::new();
    let mut right = String::new();
    let mut visible_count = 0;
    // Track the active ANSI state so we can replay it at the start of the right half
    let mut active_seq = String::new();
    let mut chars = line.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Collect the full escape sequence
            let mut seq = String::from(ch);
            if let Some(&'[') = chars.peek() {
                seq.push(chars.next().unwrap());
                while let Some(&c) = chars.peek() {
                    seq.push(chars.next().unwrap());
                    if c == 'm' {
                        break;
                    }
                }
                // Remember this sequence as the active state
                active_seq = seq.clone();
            }
            // Escape sequences go to whichever side we're currently writing
            if visible_count < split_col {
                left.push_str(&seq);
            } else {
                right.push_str(&seq);
            }
        } else {
            if visible_count < split_col {
                left.push(ch);
            } else {
                right.push(ch);
            }
            visible_count += 1;
        }
    }

    // Terminate left half with reset so colors don't bleed
    left.push_str("\x1b[0m");

    // Prepend the active color state to right half so it renders correctly standalone
    if !active_seq.is_empty() && active_seq != "\x1b[0m" {
        right = format!("{active_seq}{right}");
    }

    (left, right)
}

/// Split multi-line ANSI art at a visible character column.
///
/// Returns `(left_art, right_art)` where each is a complete ANSI string
/// with newline-separated lines. Used to derive left/right halves from
/// a single full-width mascot art file.
pub fn split_ansi_art(art: &str, split_col: usize) -> (String, String) {
    let mut left_lines = Vec::new();
    let mut right_lines = Vec::new();

    for line in art.lines() {
        let (l, r) = split_ansi_line(line, split_col);
        left_lines.push(l);
        right_lines.push(r);
    }

    (left_lines.join("\n"), right_lines.join("\n"))
}

/// Pre-built mascots and logos.
///
/// # Architecture: head customization
///
/// **Short-term (current):** Each agent has a pre-rendered full `.ans` file
/// embedded via `include_str!`. The `full_art()` function selects by name.
///
/// **Long-term:** Replace pre-rendered files with runtime compositing:
/// one shared body template + per-agent head patches spliced at a known
/// bounding box (cols 39-66, rows 2-15). The public API (`full_art`,
/// `right_half`, `left_half`, and the ratatui helpers) stays identical —
/// only the internal implementation changes.
pub mod mascots {
    use super::*;

    // --- Embedded art files (one per agent) ---
    const ART_CLAUDE: &str = include_str!("assets/mascot.claude.ans");
    const ART_CODEX: &str = include_str!("assets/mascot.codex.ans");
    const ART_GEMINI: &str = include_str!("assets/mascot.gemini.ans");
    const ART_OPENCODE: &str = include_str!("assets/mascot.opencode.ans");
    // const ART_JULES: &str = include_str!("assets/mascot.jules.ans");

    /// Half-width in visible columns (106 / 2 = 53).
    /// Used by both the TUI and splash binary.
    pub const HALF_WIDTH: usize = 53;

    /// Return the full-width (106 col) art for a given agent name.
    /// Falls back to Claude if the agent has no custom art yet.
    ///
    /// This is the single entry point for mascot selection. Today it
    /// indexes into pre-rendered files; later it can composite body + head.
    pub fn full_art(agent: &str) -> &'static str {
        match agent {
            "codex" => ART_CODEX,
            "gemini" | "gemini-cli" => ART_GEMINI,
            "opencode" => ART_OPENCODE,
            // "jules" => ART_JULES,
            _ => ART_CLAUDE,
        }
    }

    /// Right-facing half (columns 53..106)
    pub fn right_half(agent: &str) -> String {
        let (_, right) = split_ansi_art(full_art(agent), HALF_WIDTH);
        right
    }

    /// Left-facing half (columns 0..53)
    pub fn left_half(agent: &str) -> String {
        let (left, _) = split_ansi_art(full_art(agent), HALF_WIDTH);
        left
    }

    // --- Legacy aliases (default to Claude) ---

    pub fn unleashed_claude() -> String {
        right_half("claude")
    }

    pub fn unleashed_claude_lines(max_lines: usize) -> Vec<String> {
        unleashed_claude()
            .lines()
            .take(max_lines)
            .map(|s| s.to_string())
            .collect()
    }

    pub fn unleashed_claude_left() -> String {
        left_half("claude")
    }

    pub fn unleashed_claude_full() -> String {
        full_art("claude").to_string()
    }

    // --- Ratatui helpers ---

    #[cfg(feature = "tui")]
    fn to_ratatui(art: &str, max_lines: usize) -> Vec<RatatuiLine<'static>> {
        let all_lines = super::parse_ansi_to_ratatui(art);
        all_lines
            .into_iter()
            .skip_while(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .take(max_lines)
            .collect()
    }

    #[cfg(feature = "tui")]
    fn to_ratatui_lava(
        art: &str,
        max_lines: usize,
        animation_frame: usize,
    ) -> Vec<RatatuiLine<'static>> {
        let all_lines = super::parse_ansi_to_ratatui_lava(art, animation_frame);
        all_lines
            .into_iter()
            .skip_while(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .take(max_lines)
            .collect()
    }

    #[cfg(feature = "tui")]
    fn to_ratatui_themed(
        art: &str,
        max_lines: usize,
        shift: crate::theme::ThemeShift,
    ) -> Vec<RatatuiLine<'static>> {
        let all_lines = super::parse_ansi_to_ratatui_themed(art, shift);
        all_lines
            .into_iter()
            .skip_while(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .take(max_lines)
            .collect()
    }

    /// Helper: parse with diagonal gradient
    #[cfg(feature = "tui")]
    fn to_ratatui_gradient(
        art: &str,
        max_lines: usize,
        gradient: &crate::theme::GradientTheme,
    ) -> Vec<RatatuiLine<'static>> {
        let height = art.lines().count();
        let width = crate::pixel_art::mascots::HALF_WIDTH;
        let all_lines = super::parse_ansi_to_ratatui_gradient(art, gradient, width, height);
        all_lines
            .into_iter()
            .skip_while(|line| line.spans.iter().all(|s| s.content.trim().is_empty()))
            .take(max_lines)
            .collect()
    }

    // --- Agent-aware ratatui rendering (right half) ---

    #[cfg(feature = "tui")]
    pub fn right_ratatui(agent: &str, max_lines: usize) -> Vec<RatatuiLine<'static>> {
        to_ratatui(&right_half(agent), max_lines)
    }

    #[cfg(feature = "tui")]
    pub fn right_ratatui_lava(agent: &str, max_lines: usize, frame: usize) -> Vec<RatatuiLine<'static>> {
        to_ratatui_lava(&right_half(agent), max_lines, frame)
    }

    #[cfg(feature = "tui")]
    pub fn right_ratatui_themed(agent: &str, max_lines: usize, shift: crate::theme::ThemeShift) -> Vec<RatatuiLine<'static>> {
        to_ratatui_themed(&right_half(agent), max_lines, shift)
    }

    // --- Agent-aware ratatui rendering (left half) ---

    #[cfg(feature = "tui")]
    pub fn left_ratatui(agent: &str, max_lines: usize) -> Vec<RatatuiLine<'static>> {
        to_ratatui(&left_half(agent), max_lines)
    }

    #[cfg(feature = "tui")]
    pub fn left_ratatui_lava(agent: &str, max_lines: usize, frame: usize) -> Vec<RatatuiLine<'static>> {
        to_ratatui_lava(&left_half(agent), max_lines, frame)
    }

    #[cfg(feature = "tui")]
    pub fn left_ratatui_themed(agent: &str, max_lines: usize, shift: crate::theme::ThemeShift) -> Vec<RatatuiLine<'static>> {
        to_ratatui_themed(&left_half(agent), max_lines, shift)
    }

    // --- Agent-aware ratatui rendering (full) ---

    #[cfg(feature = "tui")]
    pub fn full_ratatui(agent: &str, max_lines: usize) -> Vec<RatatuiLine<'static>> {
        to_ratatui(full_art(agent), max_lines)
    }

    #[cfg(feature = "tui")]
    pub fn full_ratatui_themed(agent: &str, max_lines: usize, shift: crate::theme::ThemeShift) -> Vec<RatatuiLine<'static>> {
        to_ratatui_themed(full_art(agent), max_lines, shift)
    }

    // --- Legacy claude-specific aliases (kept for backward compat) ---

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_ratatui(max_lines: usize) -> Vec<RatatuiLine<'static>> {
        right_ratatui("claude", max_lines)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_ratatui_lava(max_lines: usize, animation_frame: usize) -> Vec<RatatuiLine<'static>> {
        right_ratatui_lava("claude", max_lines, animation_frame)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_ratatui_themed(max_lines: usize, shift: crate::theme::ThemeShift) -> Vec<RatatuiLine<'static>> {
        right_ratatui_themed("claude", max_lines, shift)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_ratatui_gradient(
        max_lines: usize,
        gradient: &crate::theme::GradientTheme,
    ) -> Vec<RatatuiLine<'static>> {
        to_ratatui_gradient(&unleashed_claude(), max_lines, gradient)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_left_ratatui(max_lines: usize) -> Vec<RatatuiLine<'static>> {
        left_ratatui("claude", max_lines)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_left_ratatui_lava(max_lines: usize, animation_frame: usize) -> Vec<RatatuiLine<'static>> {
        left_ratatui_lava("claude", max_lines, animation_frame)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_left_ratatui_themed(max_lines: usize, shift: crate::theme::ThemeShift) -> Vec<RatatuiLine<'static>> {
        left_ratatui_themed("claude", max_lines, shift)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_left_ratatui_gradient(
        max_lines: usize,
        gradient: &crate::theme::GradientTheme,
    ) -> Vec<RatatuiLine<'static>> {
        to_ratatui_gradient(&unleashed_claude_left(), max_lines, gradient)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_full_ratatui(max_lines: usize) -> Vec<RatatuiLine<'static>> {
        full_ratatui("claude", max_lines)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_full_ratatui_themed(max_lines: usize, shift: crate::theme::ThemeShift) -> Vec<RatatuiLine<'static>> {
        full_ratatui_themed("claude", max_lines, shift)
    }

    #[cfg(feature = "tui")]
    pub fn unleashed_claude_full_ratatui_gradient(
        max_lines: usize,
        gradient: &crate::theme::GradientTheme,
    ) -> Vec<RatatuiLine<'static>> {
        to_ratatui_gradient(ART_CLAUDE, max_lines, gradient)
    }

    /// Orange snail mascot for unleash
    pub fn orange_snail() -> PixelArt {
        let orange = Color::CLAUDE_ORANGE;
        let dark_orange = Color::rgb(180, 90, 60);
        let light_orange = Color::rgb(240, 150, 100);

        PixelArtBuilder::new()
            .row("      @@        ")
            .row("     @  @       ")
            .row("    SSSSSS      ")
            .row("   S******S     ")
            .row("  S**OOOO**S    ")
            .row(" S**O    O**S   ")
            .row(" S**O    O**Sbbb")
            .row(" S**O    O**bbbb")
            .row("  S**OOOO**bbbbb")
            .row("   S******bbbbb ")
            .row("    SSSSSSbbbb  ")
            .row("        bbbbb   ")
            .row("       bbbbb    ")
            .row("      bbbbb     ")
            .color('@', dark_orange) // antenna
            .color('S', dark_orange) // shell outline
            .color('*', orange) // shell fill
            .color('O', light_orange) // shell spiral
            .color('b', orange) // body
            .build()
    }

    /// Compact orange snail (smaller version)
    pub fn orange_snail_small() -> PixelArt {
        let orange = Color::CLAUDE_ORANGE;
        let dark_orange = Color::rgb(180, 90, 60);
        let light_orange = Color::rgb(240, 150, 100);

        PixelArtBuilder::new()
            .row("  @@    ")
            .row(" @  @   ")
            .row("  SSSS  ")
            .row(" S****S ")
            .row(" S*OO*Sbb")
            .row("  S**Sbbb")
            .row("   SSbbb ")
            .row("    bbb  ")
            .color('@', dark_orange)
            .color('S', dark_orange)
            .color('*', orange)
            .color('O', light_orange)
            .color('b', orange)
            .build()
    }

    /// Simple text banner
    pub fn text_banner(text: &str, color: Color) -> PixelArt {
        let mut art = PixelArt::new();
        art.grid.push(text.to_string());
        art.palette.insert('#', color);
        art
    }
}

/// Get lines of rendered art for embedding in TUI
impl PixelArt {
    /// Render to individual lines (for TUI integration)
    pub fn to_lines(&self) -> Vec<String> {
        self.render().lines().map(|s| s.to_string()).collect()
    }

    /// Render half-block to individual lines
    pub fn to_lines_halfblock(&self) -> Vec<String> {
        self.render_halfblock()
            .lines()
            .map(|s| s.to_string())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_color_from_hex() {
        assert_eq!(Color::from_hex("ff0000"), Some(Color::RED));
        assert_eq!(Color::from_hex("#00ff00"), Some(Color::GREEN));
    }

    #[test]
    fn test_dimensions() {
        let art = PixelArtBuilder::new()
            .row("###")
            .row("# #")
            .row("###")
            .build();
        assert_eq!(art.dimensions(), (3, 3));
    }

    #[cfg(feature = "tui")]
    mod tui_tests {
        use super::super::*;

        #[test]
        fn test_parse_ansi_sequence_reset() {
            let style = RatatuiStyle::default().fg(RatatuiColor::Red);
            let result = parse_ansi_sequence("0", style);
            assert_eq!(result, RatatuiStyle::default());
        }

        #[test]
        fn test_parse_ansi_sequence_fg_color() {
            let style = RatatuiStyle::default();
            let result = parse_ansi_sequence("38;2;255;128;64", style);
            assert_eq!(result.fg, Some(RatatuiColor::Rgb(255, 128, 64)));
        }

        #[test]
        fn test_parse_ansi_sequence_bg_color() {
            let style = RatatuiStyle::default();
            let result = parse_ansi_sequence("48;2;100;150;200", style);
            assert_eq!(result.bg, Some(RatatuiColor::Rgb(100, 150, 200)));
        }

        #[test]
        fn test_parse_ansi_line_to_spans_plain_text() {
            let spans = parse_ansi_line_to_spans("hello");
            assert_eq!(spans.len(), 1);
            assert_eq!(spans[0].content, "hello");
        }

        #[test]
        fn test_parse_ansi_line_to_spans_with_color() {
            // Test: reset + space pattern common in the art
            let line = "\x1b[0m \x1b[38;2;255;0;0mX";
            let spans = parse_ansi_line_to_spans(line);
            assert!(spans.len() >= 2);
            // First span should be space with default style
            assert_eq!(spans[0].content, " ");
            // Second span should be "X" with red foreground
            assert_eq!(spans[1].content, "X");
            assert_eq!(spans[1].style.fg, Some(RatatuiColor::Rgb(255, 0, 0)));
        }

        #[test]
        fn test_parse_ansi_to_ratatui_multiple_lines() {
            let text = "line1\nline2\nline3";
            let lines = parse_ansi_to_ratatui(text);
            assert_eq!(lines.len(), 3);
        }

        #[test]
        fn test_unleashed_claude_ratatui_respects_max_lines() {
            let lines = mascots::unleashed_claude_ratatui(10);
            assert!(lines.len() <= 10);
        }

        #[test]
        fn test_unleashed_claude_left_ratatui_respects_max_lines() {
            let lines = mascots::unleashed_claude_left_ratatui(10);
            assert!(lines.len() <= 10);
        }

        #[test]
        fn test_unleashed_claude_not_empty() {
            let art = mascots::unleashed_claude();
            assert!(!art.is_empty());
            assert!(art.contains('\x1b')); // Contains ANSI escape codes
        }

        #[test]
        fn test_unleashed_claude_left_not_empty() {
            let art = mascots::unleashed_claude_left();
            assert!(!art.is_empty());
            assert!(art.contains('\x1b')); // Contains ANSI escape codes
        }

        #[test]
        fn test_unleashed_claude_full_not_empty() {
            let art = mascots::unleashed_claude_full();
            assert!(!art.is_empty());
            assert!(art.contains('\x1b'));
        }
    }

    // --- split_ansi tests (non-TUI) ---

    #[test]
    fn test_split_ansi_line_plain_text() {
        let (left, right) = split_ansi_line("ABCDEF", 3);
        // Left gets first 3 visible chars + reset
        assert!(left.starts_with("ABC"));
        assert!(left.contains("\x1b[0m"));
        // Right gets remaining 3 visible chars
        assert!(right.contains("DEF"));
    }

    #[test]
    fn test_split_ansi_line_preserves_color_state() {
        // Red foreground applied before split point, text continues after
        let line = "\x1b[38;2;255;0;0mABCDEF";
        let (left, right) = split_ansi_line(line, 3);

        // Left should have the red escape + ABC + reset
        assert!(left.contains("ABC"));
        assert!(left.contains("\x1b[38;2;255;0;0m"));
        assert!(left.ends_with("\x1b[0m"));

        // Right should replay the red state before DEF
        assert!(right.contains("DEF"));
        assert!(right.contains("\x1b[38;2;255;0;0m"));
    }

    #[test]
    fn test_split_ansi_line_mid_color_change() {
        // Color changes mid-line: first 2 chars red, last 2 blue
        let line = "\x1b[38;2;255;0;0mAB\x1b[38;2;0;0;255mCD";
        let (left, right) = split_ansi_line(line, 2);

        // Left gets red AB
        assert!(left.contains("AB"));
        assert!(!left.contains("CD"));

        // Right gets blue CD, and the blue state should be prepended
        assert!(right.contains("CD"));
        assert!(right.contains("\x1b[38;2;0;0;255m"));
    }

    #[test]
    fn test_split_ansi_line_at_zero() {
        let line = "\x1b[38;2;255;0;0mHello";
        let (left, right) = split_ansi_line(line, 0);

        // Left should have just the escape code (no visible chars) + reset
        assert!(!left.contains("H"));
        // Right gets everything
        assert!(right.contains("Hello"));
    }

    #[test]
    fn test_split_ansi_line_at_end() {
        let line = "Hello";
        let (left, right) = split_ansi_line(line, 5);

        assert!(left.contains("Hello"));
        // Strip the reset from right — should have no visible content
        let right_visible: String = right.chars().filter(|c| *c != '\x1b')
            .collect::<String>()
            .replace("[0m", "");
        assert!(right_visible.trim().is_empty());
    }

    #[test]
    fn test_split_ansi_art_line_count() {
        let art = "line1\nline2\nline3";
        let (left, right) = split_ansi_art(art, 2);
        assert_eq!(left.lines().count(), 3);
        assert_eq!(right.lines().count(), 3);
    }

    #[test]
    fn test_split_ansi_art_visible_chars_match_full() {
        // The key invariant: stripping ANSI from left + right should equal
        // stripping ANSI from the full art, for each line
        let full = mascots::unleashed_claude_full();
        let (left, right) = split_ansi_art(&full, 53);

        fn strip_ansi(s: &str) -> String {
            let mut result = String::new();
            let mut chars = s.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\x1b' {
                    // Skip until 'm'
                    while let Some(c) = chars.next() {
                        if c == 'm' { break; }
                    }
                } else {
                    result.push(ch);
                }
            }
            result
        }

        for (i, ((full_line, left_line), right_line)) in full.lines()
            .zip(left.lines())
            .zip(right.lines())
            .enumerate()
        {
            let full_vis = strip_ansi(full_line);
            let left_vis = strip_ansi(left_line);
            let right_vis = strip_ansi(right_line);
            assert_eq!(
                format!("{}{}", left_vis, right_vis),
                full_vis,
                "Visible chars mismatch on line {i}"
            );
        }
    }

    #[test]
    fn test_split_halves_have_correct_width() {
        let full = mascots::unleashed_claude_full();
        let (left, right) = split_ansi_art(&full, 53);

        fn visible_width(line: &str) -> usize {
            let mut count = 0;
            let mut chars = line.chars().peekable();
            while let Some(ch) = chars.next() {
                if ch == '\x1b' {
                    while let Some(c) = chars.next() {
                        if c == 'm' { break; }
                    }
                } else {
                    count += 1;
                }
            }
            count
        }

        for (i, line) in left.lines().enumerate() {
            let w = visible_width(line);
            assert_eq!(w, 53, "Left half line {i} has width {w}, expected 53");
        }
        for (i, line) in right.lines().enumerate() {
            let w = visible_width(line);
            assert_eq!(w, 53, "Right half line {i} has width {w}, expected 53");
        }
    }

    #[test]
    fn test_split_right_half_has_ansi_escapes() {
        // The right half should have escape codes (colors carried across the split)
        let full = mascots::unleashed_claude_full();
        let (_, right) = split_ansi_art(&full, 53);
        assert!(right.contains('\x1b'), "Right half should contain ANSI escapes");
    }

    #[test]
    fn test_split_left_lines_end_with_reset() {
        // Every left-half line should end with a reset to prevent color bleed
        let full = mascots::unleashed_claude_full();
        let (left, _) = split_ansi_art(&full, 53);
        for (i, line) in left.lines().enumerate() {
            assert!(
                line.ends_with("\x1b[0m"),
                "Left half line {i} should end with reset"
            );
        }
    }
}
