//! Theme color presets and hue rotation logic
//!
//! The mascot art uses orange tones (~20 deg HSL hue). Rather than maintaining
//! separate art files per color, we rotate the hue at parse time:
//! 1. Convert each RGB pixel to HSL
//! 2. Detect orange-family pixels (hue ~10-40 deg)
//! 3. Shift hue by a fixed offset per theme
//! 4. Convert back to RGB

/// Available color theme presets
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemePreset {
    Orange,
    Blue,
    Green,
    Purple,
    Red,
    Cyan,
    Pink,
}

impl ThemePreset {
    /// All available presets in display order
    pub fn all() -> &'static [ThemePreset] {
        &[
            ThemePreset::Orange,
            ThemePreset::Blue,
            ThemePreset::Green,
            ThemePreset::Purple,
            ThemePreset::Red,
            ThemePreset::Cyan,
            ThemePreset::Pink,
        ]
    }

    /// Human-readable display name
    pub fn display_name(&self) -> &'static str {
        match self {
            ThemePreset::Orange => "Orange",
            ThemePreset::Blue => "Blue",
            ThemePreset::Green => "Green",
            ThemePreset::Purple => "Purple",
            ThemePreset::Red => "Red",
            ThemePreset::Cyan => "Cyan",
            ThemePreset::Pink => "Pink",
        }
    }

    /// Serialization key (lowercase)
    pub fn as_str(&self) -> &'static str {
        match self {
            ThemePreset::Orange => "orange",
            ThemePreset::Blue => "blue",
            ThemePreset::Green => "green",
            ThemePreset::Purple => "purple",
            ThemePreset::Red => "red",
            ThemePreset::Cyan => "cyan",
            ThemePreset::Pink => "pink",
        }
    }

    /// Look up a preset by name (case-insensitive)
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "orange" => Some(ThemePreset::Orange),
            "blue" => Some(ThemePreset::Blue),
            "green" => Some(ThemePreset::Green),
            "purple" => Some(ThemePreset::Purple),
            "red" => Some(ThemePreset::Red),
            "cyan" => Some(ThemePreset::Cyan),
            "pink" => Some(ThemePreset::Pink),
            _ => None,
        }
    }

    /// Hue shift in degrees from the base orange (~20 deg)
    pub fn hue_shift(&self) -> f64 {
        match self {
            ThemePreset::Orange => 0.0,
            ThemePreset::Blue => 200.0,
            ThemePreset::Green => 140.0,
            ThemePreset::Purple => 260.0,
            ThemePreset::Red => -10.0,
            ThemePreset::Cyan => 170.0,
            ThemePreset::Pink => 310.0,
        }
    }

    /// UI accent color RGB for this theme
    pub fn accent_rgb(&self) -> (u8, u8, u8) {
        // Pre-computed from applying hue_shift to base orange (217, 119, 87)
        match self {
            ThemePreset::Orange => (217, 119, 87),
            ThemePreset::Blue => (87, 142, 217),
            ThemePreset::Green => (87, 217, 162),
            ThemePreset::Purple => (162, 87, 217),
            ThemePreset::Red => (217, 97, 87),
            ThemePreset::Cyan => (87, 207, 217),
            ThemePreset::Pink => (217, 87, 163),
        }
    }
}

/// The base orange hue of the mascot art (~14.77 degrees)
const BASE_ORANGE_HUE: f64 = 14.769230769230768;
/// The base orange saturation (~0.631)
const BASE_ORANGE_SAT: f64 = 0.6311475409836066;

/// Describes how to transform orange-tone pixels for a theme.
/// Presets only rotate hue; custom colors also scale saturation
/// so achromatic targets (white, gray, black) desaturate correctly.
#[derive(Debug, Clone, Copy)]
pub struct ThemeShift {
    pub hue: f64,
    pub sat_scale: f64,
}

impl ThemeShift {
    /// No-op transform (default orange theme)
    pub fn identity() -> Self {
        Self {
            hue: 0.0,
            sat_scale: 1.0,
        }
    }

    /// Whether this is effectively no change
    pub fn is_identity(&self) -> bool {
        self.hue == 0.0 && (self.sat_scale - 1.0).abs() < f64::EPSILON
    }
}

/// A resolved theme color: either a named preset or a custom RGB.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ThemeColor {
    Preset(ThemePreset),
    Custom(u8, u8, u8),
}

impl ThemeColor {
    /// Parse from config string: preset name or "#RRGGBB" hex
    pub fn from_config(s: &str) -> Option<Self> {
        if let Some(preset) = ThemePreset::from_name(s) {
            return Some(ThemeColor::Preset(preset));
        }
        if let Some((r, g, b)) = parse_hex_color(s) {
            return Some(ThemeColor::Custom(r, g, b));
        }
        None
    }

    /// Serialize to config string
    pub fn to_config(self) -> String {
        match self {
            ThemeColor::Preset(p) => p.as_str().to_string(),
            ThemeColor::Custom(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
        }
    }

    /// Full theme shift (hue rotation + saturation scaling)
    pub fn theme_shift(&self) -> ThemeShift {
        match self {
            ThemeColor::Preset(p) => ThemeShift {
                hue: p.hue_shift(),
                sat_scale: 1.0, // presets are all saturated, hue-only rotation
            },
            ThemeColor::Custom(r, g, b) => {
                let (target_h, target_s, _) = rgb_to_hsl(*r, *g, *b);
                ThemeShift {
                    hue: (target_h - BASE_ORANGE_HUE).rem_euclid(360.0),
                    sat_scale: if BASE_ORANGE_SAT > f64::EPSILON {
                        target_s / BASE_ORANGE_SAT
                    } else {
                        1.0
                    },
                }
            }
        }
    }

    /// UI accent color RGB
    pub fn accent_rgb(&self) -> (u8, u8, u8) {
        match self {
            ThemeColor::Preset(p) => p.accent_rgb(),
            ThemeColor::Custom(r, g, b) => (*r, *g, *b),
        }
    }

    /// Display name for status messages
    pub fn display_name(&self) -> String {
        match self {
            ThemeColor::Preset(p) => p.display_name().to_string(),
            ThemeColor::Custom(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
        }
    }

    /// Whether this matches a specific preset
    pub fn is_preset(&self, preset: ThemePreset) -> bool {
        matches!(self, ThemeColor::Preset(p) if *p == preset)
    }

    /// Whether this is a custom color
    pub fn is_custom(&self) -> bool {
        matches!(self, ThemeColor::Custom(..))
    }
}

/// Parse a hex color string like "#FF5500", "FF5500", "#f50", or "f50".
/// Accepts 1-6 hex digits:
///   - 3 digits: CSS shorthand (each digit doubled, e.g. "f50" -> "ff5500")
///   - 6 digits: standard RRGGBB
///   - 1, 2, 4, 5 digits: left-padded with zeros to 6 (e.g. "FFFFF" -> "0FFFFF")
pub fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let s = s.trim().trim_start_matches('#');
    if s.is_empty() || s.len() > 6 {
        return None;
    }
    // Validate all chars are hex digits
    if !s.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    if s.len() == 3 {
        // CSS shorthand: "f50" -> "ff5500"
        let r = u8::from_str_radix(&s[0..1].repeat(2), 16).ok()?;
        let g = u8::from_str_radix(&s[1..2].repeat(2), 16).ok()?;
        let b = u8::from_str_radix(&s[2..3].repeat(2), 16).ok()?;
        Some((r, g, b))
    } else {
        // Left-pad to 6 digits (treat as a 24-bit hex number)
        let padded = format!("{:0>6}", s);
        let r = u8::from_str_radix(&padded[0..2], 16).ok()?;
        let g = u8::from_str_radix(&padded[2..4], 16).ok()?;
        let b = u8::from_str_radix(&padded[4..6], 16).ok()?;
        Some((r, g, b))
    }
}

/// Transform a color by rotating its hue and scaling saturation,
/// only affecting orange-tone pixels.
pub fn transform_theme_color(r: u8, g: u8, b: u8, shift: ThemeShift) -> (u8, u8, u8) {
    if shift.is_identity() {
        return (r, g, b);
    }

    let (h, s, l) = rgb_to_hsl(r, g, b);

    if is_orange_tone(h, s) {
        let new_h = (h + shift.hue).rem_euclid(360.0);
        let new_s = (s * shift.sat_scale).clamp(0.0, 1.0);
        hsl_to_rgb(new_h, new_s, l)
    } else {
        (r, g, b)
    }
}

/// A color stop in a gradient: RGB target color at a position.
#[derive(Debug, Clone, Copy)]
pub struct GradientStop {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// A multi-stop diagonal gradient applied to orange-tone pixels.
/// The gradient interpolates between stops based on diagonal position (x + y).
#[derive(Debug, Clone)]
pub struct GradientTheme {
    /// Color stops (at least 2). Evenly distributed along the diagonal.
    pub stops: Vec<GradientStop>,
}

impl GradientTheme {
    /// Create a gradient from RGB tuples.
    pub fn new(stops: &[(u8, u8, u8)]) -> Self {
        Self {
            stops: stops.iter().map(|&(r, g, b)| GradientStop { r, g, b }).collect(),
        }
    }

    /// The Gemini CLI gradient: blue → purple → pink
    pub fn gemini() -> Self {
        Self::new(&[
            (0x47, 0x96, 0xE4), // #4796E4 blue
            (0x84, 0x7A, 0xCE), // #847ACE purple
            (0xC3, 0x67, 0x7F), // #C3677F pink
        ])
    }
}

/// Transform a color using a diagonal gradient.
/// `t` is the gradient position in 0.0..=1.0 (typically `(x + y) / (width + height)`).
/// Only affects orange-tone pixels (same detection as ThemeShift).
pub fn transform_gradient_color(r: u8, g: u8, b: u8, gradient: &GradientTheme, t: f64) -> (u8, u8, u8) {
    let (h, s, l) = rgb_to_hsl(r, g, b);

    if !is_orange_tone(h, s) {
        return (r, g, b);
    }

    let stops = &gradient.stops;
    if stops.is_empty() {
        return (r, g, b);
    }
    if stops.len() == 1 {
        let stop = &stops[0];
        let shift = shift_for_target(stop.r, stop.g, stop.b);
        let new_h = (h + shift.hue).rem_euclid(360.0);
        let new_s = (s * shift.sat_scale).clamp(0.0, 1.0);
        return hsl_to_rgb(new_h, new_s, l);
    }

    // Interpolate between stops
    let t = t.clamp(0.0, 1.0);
    let segment_count = stops.len() - 1;
    let scaled = t * segment_count as f64;
    let idx = (scaled.floor() as usize).min(segment_count - 1);
    let frac = scaled - idx as f64;

    let a = &stops[idx];
    let b_stop = &stops[idx + 1];

    // Lerp the target RGB
    let tr = lerp_u8(a.r, b_stop.r, frac);
    let tg = lerp_u8(a.g, b_stop.g, frac);
    let tb = lerp_u8(a.b, b_stop.b, frac);

    // Compute shift for this interpolated target color
    let shift = shift_for_target(tr, tg, tb);
    let new_h = (h + shift.hue).rem_euclid(360.0);
    let new_s = (s * shift.sat_scale).clamp(0.0, 1.0);
    hsl_to_rgb(new_h, new_s, l)
}

fn shift_for_target(r: u8, g: u8, b: u8) -> ThemeShift {
    let (target_h, target_s, _) = rgb_to_hsl(r, g, b);
    ThemeShift {
        hue: (target_h - BASE_ORANGE_HUE).rem_euclid(360.0),
        sat_scale: if BASE_ORANGE_SAT > f64::EPSILON {
            target_s / BASE_ORANGE_SAT
        } else {
            1.0
        },
    }
}

fn lerp_u8(a: u8, b: u8, t: f64) -> u8 {
    let a = a as f64;
    let b = b as f64;
    (a + (b - a) * t).round().clamp(0.0, 255.0) as u8
}

/// Detect if a color is in the warm/orange family that should be recolored.
/// The mascot art uses hundreds of warm tones spanning hue 0-45 degrees:
/// bright oranges, red-oranges, dark browns, peach/skin tones.
/// We also catch near-red tones wrapping around 360 (hue > 350).
fn is_orange_tone(h: f64, s: f64) -> bool {
    // Warm hues: 0-50 degrees (red through orange-yellow) or 350-360 (near-red wrap)
    // Saturation > 0.10 excludes grays/neutrals that happen to land in this hue range
    (h <= 50.0 || h >= 350.0) && s > 0.10
}

/// Convert RGB (0-255) to HSL (h: 0-360, s: 0-1, l: 0-1)
fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let r = r as f64 / 255.0;
    let g = g as f64 / 255.0;
    let b = b as f64 / 255.0;

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if (max - min).abs() < f64::EPSILON {
        // Achromatic
        return (0.0, 0.0, l);
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    let h = if (max - r).abs() < f64::EPSILON {
        let mut h = (g - b) / d;
        if g < b {
            h += 6.0;
        }
        h
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };

    (h * 60.0, s, l)
}

/// Convert HSL (h: 0-360, s: 0-1, l: 0-1) to RGB (0-255)
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s.abs() < f64::EPSILON {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let h_norm = h / 360.0;

    let r = hue_to_rgb(p, q, h_norm + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h_norm);
    let b = hue_to_rgb(p, q, h_norm - 1.0 / 3.0);

    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn hue_to_rgb(p: f64, q: f64, mut t: f64) -> f64 {
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_presets_have_unique_names() {
        let presets = ThemePreset::all();
        for (i, a) in presets.iter().enumerate() {
            for (j, b) in presets.iter().enumerate() {
                if i != j {
                    assert_ne!(a.as_str(), b.as_str());
                }
            }
        }
    }

    #[test]
    fn test_roundtrip_str() {
        for preset in ThemePreset::all() {
            let s = preset.as_str();
            let parsed = ThemePreset::from_name(s).expect("should parse");
            assert_eq!(*preset, parsed);
        }
    }

    #[test]
    fn test_orange_identity() {
        // Orange theme (identity shift) should not change any color
        let (r, g, b) = transform_theme_color(217, 119, 87, ThemeShift::identity());
        assert_eq!((r, g, b), (217, 119, 87));
    }

    #[test]
    fn test_non_orange_unchanged() {
        // Gray pixels should never be changed regardless of shift
        let shift = ThemeShift {
            hue: 200.0,
            sat_scale: 1.0,
        };
        let (r, g, b) = transform_theme_color(128, 128, 128, shift);
        assert_eq!((r, g, b), (128, 128, 128));
    }

    #[test]
    fn test_hue_shift_changes_color() {
        // Blue shift should produce a noticeably different color from orange
        let shift = ThemeShift {
            hue: 200.0,
            sat_scale: 1.0,
        };
        let (r, g, b) = transform_theme_color(217, 119, 87, shift);
        // The result should be blue-ish, not orange
        assert!(
            b > r,
            "blue shift should increase blue component: r={}, g={}, b={}",
            r,
            g,
            b
        );
    }

    #[test]
    fn test_white_theme_desaturates() {
        // White target: sat_scale=0 should produce grayscale from orange pixels
        let shift = ThemeColor::Custom(255, 255, 255).theme_shift();
        assert!(
            shift.sat_scale.abs() < 0.01,
            "white should have ~0 sat_scale: {}",
            shift.sat_scale
        );
        let (r, g, b) = transform_theme_color(217, 119, 87, shift);
        // Result should be grayscale (all channels equal or near-equal)
        let spread = (r.max(g).max(b) as i16) - (r.min(g).min(b) as i16);
        assert!(
            spread <= 1,
            "white theme should desaturate orange: got ({}, {}, {})",
            r,
            g,
            b
        );
    }

    #[test]
    fn test_rgb_hsl_roundtrip() {
        let test_colors: Vec<(u8, u8, u8)> = vec![
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (217, 119, 87),
            (128, 128, 128),
            (0, 0, 0),
            (255, 255, 255),
        ];

        for (r, g, b) in test_colors {
            let (h, s, l) = rgb_to_hsl(r, g, b);
            let (r2, g2, b2) = hsl_to_rgb(h, s, l);
            assert!(
                (r as i16 - r2 as i16).abs() <= 1
                    && (g as i16 - g2 as i16).abs() <= 1
                    && (b as i16 - b2 as i16).abs() <= 1,
                "roundtrip failed for ({}, {}, {}): got ({}, {}, {})",
                r,
                g,
                b,
                r2,
                g2,
                b2
            );
        }
    }

    #[test]
    fn test_parse_hex_color() {
        // 6-digit standard
        assert_eq!(parse_hex_color("#ff5500"), Some((255, 85, 0)));
        assert_eq!(parse_hex_color("FF5500"), Some((255, 85, 0)));
        assert_eq!(parse_hex_color("FFFFFF"), Some((255, 255, 255)));
        // 3-digit CSS shorthand
        assert_eq!(parse_hex_color("#f50"), Some((255, 85, 0)));
        assert_eq!(parse_hex_color("abc"), Some((170, 187, 204)));
        assert_eq!(parse_hex_color("FFF"), Some((255, 255, 255)));
        // 5-digit: left-padded to 6 -> "0FFFFF"
        assert_eq!(parse_hex_color("FFFFF"), Some((15, 255, 255)));
        // 4-digit: left-padded to 6 -> "00FFFF"
        assert_eq!(parse_hex_color("FFFF"), Some((0, 255, 255)));
        // 2-digit: left-padded to 6 -> "0000FF"
        assert_eq!(parse_hex_color("FF"), Some((0, 0, 255)));
        // 1-digit: left-padded to 6 -> "00000F"
        assert_eq!(parse_hex_color("F"), Some((0, 0, 15)));
        // Invalid
        assert_eq!(parse_hex_color(""), None);
        assert_eq!(parse_hex_color("zzzzzz"), None);
        assert_eq!(parse_hex_color("1234567"), None); // too long
    }

    #[test]
    fn test_theme_color_from_config() {
        // Preset names
        assert!(matches!(
            ThemeColor::from_config("blue"),
            Some(ThemeColor::Preset(ThemePreset::Blue))
        ));
        // Hex colors
        assert!(matches!(
            ThemeColor::from_config("#ff0000"),
            Some(ThemeColor::Custom(255, 0, 0))
        ));
        // Invalid
        assert!(ThemeColor::from_config("bogus").is_none());
    }

    #[test]
    fn test_theme_color_roundtrip() {
        let custom = ThemeColor::Custom(100, 200, 50);
        let s = custom.to_config();
        let parsed = ThemeColor::from_config(&s).unwrap();
        assert_eq!(custom, parsed);

        let preset = ThemeColor::Preset(ThemePreset::Cyan);
        let s = preset.to_config();
        let parsed = ThemeColor::from_config(&s).unwrap();
        assert_eq!(preset, parsed);
    }

    #[test]
    fn test_custom_theme_shift_identity() {
        // Using the base orange as custom color should give ~identity shift
        let shift = ThemeColor::Custom(217, 119, 87).theme_shift();
        assert!(
            shift.hue.abs() < 1.0,
            "base orange should have near-zero hue shift: {}",
            shift.hue
        );
        assert!(
            (shift.sat_scale - 1.0).abs() < 0.01,
            "base orange should have ~1.0 sat_scale: {}",
            shift.sat_scale
        );
    }

    #[test]
    fn test_accent_colors_match_transform() {
        // Verify hardcoded accent_rgb() values match what transform_theme_color
        // actually produces from the base orange (217, 119, 87)
        for preset in ThemePreset::all() {
            let expected = preset.accent_rgb();
            let shift = ThemeColor::Preset(*preset).theme_shift();
            let actual = transform_theme_color(217, 119, 87, shift);
            assert!(
                (expected.0 as i16 - actual.0 as i16).abs() <= 1
                    && (expected.1 as i16 - actual.1 as i16).abs() <= 1
                    && (expected.2 as i16 - actual.2 as i16).abs() <= 1,
                "accent_rgb for {:?} drifted from transform: expected {:?}, got {:?}",
                preset,
                expected,
                actual
            );
        }
    }
}
