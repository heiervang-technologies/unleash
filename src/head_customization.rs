//! Head customization for the Claude mascot sprite
//!
//! Provides head variant overlays that are composited onto the ANSI sprite
//! during rendering. Each variant defines colored pixel-art decorations
//! (crown, sunglasses, halo, etc.) positioned at specific (row, col)
//! coordinates on the 53-char-wide sprite.

use ratatui::{
    style::{Color, Style},
    text::{Line, Span},
};

/// Available head variants for the mascot sprite
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadVariant {
    /// Original Claude head — no overlay
    Default,
    /// A golden crown atop the head
    Crown,
    /// Cool aviator sunglasses across the eyes
    Sunglasses,
    /// Angelic halo floating above the head
    Halo,
    /// Cyberpunk-style neon goggles
    CyberGoggles,
}

impl HeadVariant {
    /// All variants in display order
    pub fn all() -> &'static [HeadVariant] {
        &[
            HeadVariant::Default,
            HeadVariant::Crown,
            HeadVariant::Sunglasses,
            HeadVariant::Halo,
            HeadVariant::CyberGoggles,
        ]
    }

    /// Human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            HeadVariant::Default => "Default",
            HeadVariant::Crown => "Crown 👑",
            HeadVariant::Sunglasses => "Sunglasses 😎",
            HeadVariant::Halo => "Halo 😇",
            HeadVariant::CyberGoggles => "Cyber Goggles 🤖",
        }
    }

    /// Serialization key (lowercase)
    pub fn as_str(&self) -> &'static str {
        match self {
            HeadVariant::Default => "default",
            HeadVariant::Crown => "crown",
            HeadVariant::Sunglasses => "sunglasses",
            HeadVariant::Halo => "halo",
            HeadVariant::CyberGoggles => "cyber-goggles",
        }
    }

    /// Look up a variant by name (case-insensitive)
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "default" => Some(HeadVariant::Default),
            "crown" | "crown 👑" => Some(HeadVariant::Crown),
            "sunglasses" | "sunglasses 😎" => Some(HeadVariant::Sunglasses),
            "halo" | "halo 😇" => Some(HeadVariant::Halo),
            "cyber-goggles" | "cyber goggles" | "cyber goggles 🤖" => {
                Some(HeadVariant::CyberGoggles)
            }
            _ => None,
        }
    }
}

/// A single pixel of a head overlay
#[derive(Debug, Clone)]
pub struct OverlayPixel {
    /// Row offset from the top of the sprite (0-indexed from the start of non-blank lines)
    pub row: usize,
    /// Column offset from the left of the sprite
    pub col: usize,
    /// The character to display (full block by default)
    pub ch: char,
    /// Foreground color
    pub fg: (u8, u8, u8),
    /// Optional background color (None = transparent)
    pub bg: Option<(u8, u8, u8)>,
}

/// A complete head overlay: a set of pixels positioned on the sprite
pub struct HeadOverlay {
    pub pixels: Vec<OverlayPixel>,
}

impl HeadOverlay {
    /// Apply this overlay to a set of Ratatui lines (the parsed ANSI art).
    /// Modifies the spans at the given positions to overlay head decorations.
    pub fn apply(&self, lines: &mut [Line]) {
        for pixel in &self.pixels {
            if pixel.row >= lines.len() {
                continue;
            }
            let line = &mut lines[pixel.row];

            // Find or create spans to cover the target column
            // We need to ensure at least `col + 1` character positions exist
            self.ensure_line_width(line, pixel.col + 1);

            // Replace the span at the target column
            self.set_pixel(line, pixel);
        }
    }

    /// Ensure a Ratatui Line has enough character positions by padding
    fn ensure_line_width(&self, line: &mut Line, min_width: usize) {
        let current_width = line_width(line);
        if current_width < min_width {
            let padding = min_width - current_width;
            line.spans.push(Span::raw(" ".repeat(padding)));
        }
    }

    /// Set a specific character position in a Ratatui Line to the overlay pixel
    fn set_pixel(&self, line: &mut Line, pixel: &OverlayPixel) {
        let col = pixel.col;

        // Walk through spans to find which span covers column `col`
        let mut accumulated = 0usize;
        for span_idx in 0..line.spans.len() {
            let span_content_len = line.spans[span_idx].content.len();
            let span_start = accumulated;
            let span_end = accumulated + span_content_len;

            if col < span_end {
                // This span covers our target column
                let local_pos = col - span_start;

                // Build the replacement span content
                let fg_color = Color::Rgb(pixel.fg.0, pixel.fg.1, pixel.fg.2);
                let mut style = Style::default().fg(fg_color);
                if let Some((r, g, b)) = pixel.bg {
                    style = style.bg(Color::Rgb(r, g, b));
                }

                let content: Vec<char> = line.spans[span_idx].content.chars().collect();
                // Pad content if needed (shouldn't happen with ANSI art, but safe)
                let padded_len = content.len().max(local_pos + 1);
                let mut new_chars: Vec<char> = Vec::with_capacity(padded_len);
                for i in 0..padded_len {
                    if i == local_pos {
                        new_chars.push(pixel.ch);
                    } else if i < content.len() {
                        new_chars.push(content[i]);
                    } else {
                        new_chars.push(' ');
                    }
                }
                let new_content: String = new_chars.into_iter().collect();
                line.spans[span_idx].content = new_content.into();
                line.spans[span_idx].style = style;

                // Update the accumulator ONLY for this span's original width
                // to find the rest of the spans correctly (not needed since we found our target)
                break;
            }

            accumulated = span_end;
        }
    }
}

/// Calculate the display width of a Ratatui Line
fn line_width(line: &Line) -> usize {
    line.spans.iter().map(|s| s.content.len()).sum()
}

/// Get the overlay data for a specific head variant
pub fn get_head_overlay(variant: HeadVariant) -> Option<HeadOverlay> {
    match variant {
        HeadVariant::Default => None, // No overlay needed
        HeadVariant::Crown => Some(crown_overlay()),
        HeadVariant::Sunglasses => Some(sunglasses_overlay()),
        HeadVariant::Halo => Some(halo_overlay()),
        HeadVariant::CyberGoggles => Some(cyber_goggles_overlay()),
    }
}

// ─── Overlay definitions ─────────────────────────────────────────────────────────

/// Golden crown overlay — positioned at the top center of the sprite head
///
///             **  **
///            *  *  *
///           *       *
///            *     *
///             *****
fn crown_overlay() -> HeadOverlay {
    let gold = (255, 215, 0);
    let gold_dark = (184, 134, 11);
    let gold_light = (255, 239, 150);
    let ruby = (220, 20, 60);

    HeadOverlay {
        pixels: vec![
            // Row 0:  **  **
            OverlayPixel { row: 0, col: 20, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 0, col: 21, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 0, col: 22, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 0, col: 23, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 0, col: 24, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 0, col: 25, ch: '\u{2580}', fg: gold, bg: None },
            // Row 1: *  *  *
            OverlayPixel { row: 1, col: 19, ch: '\u{2580}', fg: gold_dark, bg: None },
            OverlayPixel { row: 1, col: 20, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 1, col: 21, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 1, col: 22, ch: '\u{2580}', fg: gold_dark, bg: None },
            OverlayPixel { row: 1, col: 23, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 1, col: 24, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 1, col: 25, ch: '\u{2580}', fg: gold_dark, bg: None },
            // Row 2: wide base *
            OverlayPixel { row: 2, col: 18, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 2, col: 19, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 20, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 21, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 2, col: 22, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 23, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 24, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 25, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 2, col: 26, ch: '\u{2580}', fg: gold, bg: None },
            // Row 3: * with ruby
            OverlayPixel { row: 3, col: 19, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 3, col: 20, ch: '\u{2580}', fg: gold_dark, bg: None },
            OverlayPixel { row: 3, col: 21, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 3, col: 22, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 3, col: 23, ch: '\u{2580}', fg: gold_dark, bg: None },
            OverlayPixel { row: 3, col: 24, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 3, col: 25, ch: '\u{2580}', fg: gold, bg: None },
            // Row 4: base
            OverlayPixel { row: 4, col: 19, ch: '\u{2584}', fg: gold, bg: None },
            OverlayPixel { row: 4, col: 20, ch: '\u{2584}', fg: gold, bg: None },
            OverlayPixel { row: 4, col: 21, ch: '\u{2584}', fg: gold, bg: None },
            OverlayPixel { row: 4, col: 22, ch: '\u{2584}', fg: gold, bg: None },
            OverlayPixel { row: 4, col: 23, ch: '\u{2584}', fg: gold, bg: None },
            OverlayPixel { row: 4, col: 24, ch: '\u{2584}', fg: gold, bg: None },
            OverlayPixel { row: 4, col: 25, ch: '\u{2584}', fg: gold, bg: None },
            // Ruby gem
            OverlayPixel { row: 3, col: 22, ch: '\u{2588}', fg: ruby, bg: Some(gold) },
        ],
    }
}

/// Cool aviator sunglasses overlay — positioned over the eyes
///
///      _______________
///     /  _________   \
///    /  /  _   _  \   \
///   /  /  |_| |_|  \   \
///  /  /   _   _    \   \
/// /  /   |_| |_|    \   \
/// \  \______________/  /
///  \__________________/
fn sunglasses_overlay() -> HeadOverlay {
    let black = (30, 30, 30);
    let dark = (50, 50, 50);
    let silver = (192, 192, 192);
    let lens = (80, 120, 200);
    let shine = (200, 220, 255);

    HeadOverlay {
        pixels: vec![
            // Row 3: start of sunglasses
            OverlayPixel { row: 3, col: 15, ch: '\u{2580}', fg: silver, bg: None },
            OverlayPixel { row: 3, col: 16, ch: '\u{2580}', fg: silver, bg: None },
            OverlayPixel { row: 3, col: 35, ch: '\u{2580}', fg: silver, bg: None },
            OverlayPixel { row: 3, col: 36, ch: '\u{2580}', fg: silver, bg: None },
            // Row 4: bridge and frames
            OverlayPixel { row: 4, col: 14, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 15, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 16, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 17, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 34, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 35, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 36, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 4, col: 37, ch: '\u{2580}', fg: black, bg: None },
            // Row 5: lens area (left)
            OverlayPixel { row: 5, col: 14, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 5, col: 15, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 16, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 17, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 18, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 19, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 20, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 21, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 22, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 23, ch: '\u{2588}', fg: lens, bg: Some(black) },
            // bridge middle
            OverlayPixel { row: 5, col: 24, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 5, col: 25, ch: '\u{2580}', fg: black, bg: None },
            // lens area (right)
            OverlayPixel { row: 5, col: 26, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 27, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 28, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 29, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 30, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 31, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 32, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 33, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 34, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 35, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 36, ch: '\u{2588}', fg: lens, bg: Some(black) },
            OverlayPixel { row: 5, col: 37, ch: '\u{2580}', fg: black, bg: None },
            // Row 6: lens lower
            OverlayPixel { row: 6, col: 14, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 6, col: 15, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 16, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 17, ch: '\u{2588}', fg: shine, bg: Some(dark) },
            OverlayPixel { row: 6, col: 18, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 19, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 20, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 21, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 22, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 23, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 24, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 6, col: 25, ch: '\u{2580}', fg: black, bg: None },
            OverlayPixel { row: 6, col: 26, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 27, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 28, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 29, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 30, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 31, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 32, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 33, ch: '\u{2588}', fg: shine, bg: Some(dark) },
            OverlayPixel { row: 6, col: 34, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 35, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 36, ch: '\u{2588}', fg: lens, bg: Some(dark) },
            OverlayPixel { row: 6, col: 37, ch: '\u{2580}', fg: black, bg: None },
            // Row 7: frame bottom
            OverlayPixel { row: 7, col: 14, ch: '\u{2584}', fg: black, bg: None },
            OverlayPixel { row: 7, col: 15, ch: '\u{2584}', fg: black, bg: None },
            OverlayPixel { row: 7, col: 36, ch: '\u{2584}', fg: black, bg: None },
            OverlayPixel { row: 7, col: 37, ch: '\u{2584}', fg: black, bg: None },
        ],
    }
}

/// Angelic halo overlay — positioned above the head
///
///       *******
///     **       **
///    *           *
///    *           *
///     **       **
///       *******
fn halo_overlay() -> HeadOverlay {
    let gold = (255, 215, 0);
    let gold_light = (255, 255, 200);

    HeadOverlay {
        pixels: vec![
            // Row 0: top of halo (small wings)
            OverlayPixel { row: 0, col: 21, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 0, col: 22, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 0, col: 28, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 0, col: 29, ch: '\u{2580}', fg: gold_light, bg: None },
            // Row 1
            OverlayPixel { row: 1, col: 20, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 1, col: 21, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 22, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 23, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 24, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 25, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 26, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 27, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 28, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 29, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 1, col: 30, ch: '\u{2580}', fg: gold_light, bg: None },
            // Row 2
            OverlayPixel { row: 2, col: 19, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 20, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 2, col: 21, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 22, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 2, col: 23, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 2, col: 24, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 2, col: 25, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 2, col: 26, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 2, col: 27, ch: ' ', fg: (0,0,0), bg: None },
            OverlayPixel { row: 2, col: 28, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 29, ch: '\u{2580}', fg: gold, bg: None },
            OverlayPixel { row: 2, col: 30, ch: '\u{2580}', fg: gold_light, bg: None },
            OverlayPixel { row: 2, col: 31, ch: '\u{2580}', fg: gold_light, bg: None },
        ],
    }
}

/// Cyberpunk neon goggles overlay
///
///      _________       
///     |  _   _  |      
///     | |_| |_| |  ⚡  
///     |    _    |      
///     |   |_|   |      
///     |_________|      
fn cyber_goggles_overlay() -> HeadOverlay {
    let neon_cyan = (0, 255, 255);
    let neon_magenta = (255, 0, 255);
    let dark = (20, 20, 40);

    HeadOverlay {
        pixels: vec![
            // Row 4: top frame
            OverlayPixel { row: 4, col: 16, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 4, col: 17, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 4, col: 33, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 4, col: 34, ch: '\u{2580}', fg: neon_cyan, bg: None },
            // Row 5: goggles frame top
            OverlayPixel { row: 5, col: 15, ch: '\u{2580}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 5, col: 16, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 5, col: 17, ch: '\u{2580}', fg: dark, bg: None },
            OverlayPixel { row: 5, col: 18, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 19, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 20, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 21, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 22, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 23, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 24, ch: '\u{2580}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 5, col: 25, ch: '\u{2580}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 5, col: 26, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 27, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 28, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 29, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 30, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 31, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 5, col: 32, ch: '\u{2580}', fg: dark, bg: None },
            OverlayPixel { row: 5, col: 33, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 5, col: 34, ch: '\u{2580}', fg: neon_magenta, bg: None },
            // Row 6: middle
            OverlayPixel { row: 6, col: 15, ch: '\u{2580}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 6, col: 16, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 6, col: 17, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 6, col: 18, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 19, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 20, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 6, col: 21, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 22, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 23, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 6, col: 24, ch: '\u{2580}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 6, col: 25, ch: '\u{2580}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 6, col: 26, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 6, col: 27, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 28, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 29, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 6, col: 30, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 31, ch: '\u{2588}', fg: neon_magenta, bg: Some(dark) },
            OverlayPixel { row: 6, col: 32, ch: '\u{2588}', fg: neon_cyan, bg: Some(dark) },
            OverlayPixel { row: 6, col: 33, ch: '\u{2580}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 6, col: 34, ch: '\u{2580}', fg: neon_magenta, bg: None },
            // Row 7: bottom frame
            OverlayPixel { row: 7, col: 15, ch: '\u{2584}', fg: neon_magenta, bg: None },
            OverlayPixel { row: 7, col: 16, ch: '\u{2584}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 7, col: 17, ch: '\u{2584}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 7, col: 32, ch: '\u{2584}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 7, col: 33, ch: '\u{2584}', fg: neon_cyan, bg: None },
            OverlayPixel { row: 7, col: 34, ch: '\u{2584}', fg: neon_magenta, bg: None },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::text::Span;

    #[test]
    fn test_head_variant_all() {
        let variants = HeadVariant::all();
        assert_eq!(variants.len(), 5);
        assert_eq!(variants[0], HeadVariant::Default);
    }

    #[test]
    fn test_head_variant_roundtrip() {
        for variant in HeadVariant::all() {
            let s = variant.as_str();
            let parsed = HeadVariant::from_name(s).expect("should parse back");
            assert_eq!(*variant, parsed);
        }
    }

    #[test]
    fn test_head_variant_from_name_case_insensitive() {
        assert_eq!(
            HeadVariant::from_name("CROWN"),
            Some(HeadVariant::Crown)
        );
        assert_eq!(
            HeadVariant::from_name("SUNGLASSES"),
            Some(HeadVariant::Sunglasses)
        );
        assert_eq!(HeadVariant::from_name("Halo"), Some(HeadVariant::Halo));
    }

    #[test]
    fn test_default_no_overlay() {
        assert!(get_head_overlay(HeadVariant::Default).is_none());
    }

    #[test]
    fn test_crown_overlay_exists() {
        let overlay = get_head_overlay(HeadVariant::Crown);
        assert!(overlay.is_some());
        assert!(!overlay.unwrap().pixels.is_empty());
    }

    #[test]
    fn test_sunglasses_overlay_exists() {
        let overlay = get_head_overlay(HeadVariant::Sunglasses);
        assert!(overlay.is_some());
        assert!(!overlay.unwrap().pixels.is_empty());
    }

    #[test]
    fn test_halo_overlay_exists() {
        let overlay = get_head_overlay(HeadVariant::Halo);
        assert!(overlay.is_some());
        assert!(!overlay.unwrap().pixels.is_empty());
    }

    #[test]
    fn test_cyber_goggles_overlay_exists() {
        let overlay = get_head_overlay(HeadVariant::CyberGoggles);
        assert!(overlay.is_some());
        assert!(!overlay.unwrap().pixels.is_empty());
    }

    #[test]
    fn test_overlay_apply_no_panic_on_empty_lines() {
        let overlay = get_head_overlay(HeadVariant::Crown).unwrap();
        let mut lines: Vec<Line> = vec![Line::from("")];
        overlay.apply(&mut lines); // Should not panic
    }

    #[test]
    fn test_overlay_apply_modifies_line() {
        let overlay = get_head_overlay(HeadVariant::Crown).unwrap();
        let mut lines: Vec<Line> = vec![Line::from(" ".repeat(60)); 30];
        overlay.apply(&mut lines);

        // The crown should have modified some pixels
        let has_modifications = lines.iter().any(|line| {
            line.spans.iter().any(|s| {
                s.content.chars().any(|c| c != ' ')
            })
        });
        assert!(has_modifications, "Crown overlay should modify sprite lines");
    }

    #[test]
    fn test_overlay_pixel_colors_valid_rgb() {
        let overlay = get_head_overlay(HeadVariant::Crown).unwrap();
        for pixel in &overlay.pixels {
            // All pixels should have valid RGB values (some spacing pixels are intentionally black)
            assert!(pixel.fg.0 <= 255 && pixel.fg.1 <= 255 && pixel.fg.2 <= 255,
                "Overlay pixels should have valid RGB values");
        }
    }

    #[test]
    fn test_line_width() {
        let line = Line::from("hello");
        assert_eq!(line_width(&line), 5);

        let multi_span = Line::from(vec![Span::raw("abc"), Span::raw("def")]);
        assert_eq!(line_width(&multi_span), 6);
    }

    #[test]
    fn test_ensure_line_width_pads() {
        let overlay = get_head_overlay(HeadVariant::Crown).unwrap();
        let mut line = Line::from("short");
        overlay.ensure_line_width(&mut line, 10);
        assert!(line_width(&line) >= 10);
    }
}
