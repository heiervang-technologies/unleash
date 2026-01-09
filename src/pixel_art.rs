//! ANSI Pixel Art Renderer
//!
//! Renders arbitrary "images" as colored ASCII grids in the terminal.
//! Supports 24-bit RGB colors via ANSI escape sequences.

use std::collections::HashMap;
use std::io::{self, Write};

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
        let width = self.grid.iter().map(|row| row.chars().count()).max().unwrap_or(0);
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
                    if c == ' ' || c == '.' { None } else { self.palette.get(&c) }
                });
                let bot_color = bot_char.and_then(|c| {
                    if c == ' ' || c == '.' { None } else { self.palette.get(&c) }
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

/// Pre-built mascots and logos
pub mod mascots {
    use super::*;

    /// Orange snail mascot for Claude Unleashed
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
            .color('@', dark_orange)      // antenna
            .color('S', dark_orange)      // shell outline
            .color('*', orange)           // shell fill
            .color('O', light_orange)     // shell spiral
            .color('b', orange)           // body
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
        self.render_halfblock().lines().map(|s| s.to_string()).collect()
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
}
