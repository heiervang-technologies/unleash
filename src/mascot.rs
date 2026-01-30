//! Mascot preset system for Agent Unleashed
//!
//! Provides swappable mascot heads and agent-specific color presets.
//! Built-in presets: Claude, Qwen, OpenAI, Gemini, Generic.
//! Users can define custom presets via TOML files in
//! `~/.config/agent-unleashed/mascots/`.

use crate::theme::{ColorScheme, GradientDef};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

// ── Head assets ────────────────────────────────────────────────────────

/// A head asset: either embedded ANSI art or a PixelArt grid definition.
#[derive(Debug, Clone)]
pub enum HeadAsset {
    /// Embedded ANSI art content (from .ans file compiled into binary)
    AnsiArt(String),
    /// No custom head — use the default body art unmodified
    Default,
}

/// Where a head overlays onto the body art
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct HeadBounds {
    /// Character offset from left edge of body
    pub x_offset: u16,
    /// Line offset from top of body
    pub y_offset: u16,
    /// Width of the head region in characters
    pub width: u16,
    /// Height of the head region in lines
    pub height: u16,
}

impl Default for HeadBounds {
    fn default() -> Self {
        // Default bounds covering the head region of the unleashed mascot
        Self {
            x_offset: 10,
            y_offset: 0,
            width: 30,
            height: 20,
        }
    }
}

// ── Mascot presets ─────────────────────────────────────────────────────

/// A complete mascot preset: head + color scheme + metadata
#[derive(Debug, Clone)]
pub struct MascotPreset {
    /// Unique identifier (e.g. "claude", "openai")
    pub id: String,
    /// Display name for the UI
    pub display_name: String,
    /// Description shown in preset selector
    #[allow(dead_code)]
    pub description: String,
    /// Head asset to overlay onto the right-facing body
    pub head_right: HeadAsset,
    /// Head asset to overlay onto the left-facing body
    pub head_left: HeadAsset,
    /// Color scheme (solid hue shift or gradient)
    pub color_scheme: ColorScheme,
    /// Bounding box for head overlay on right-facing body
    pub head_bounds: HeadBounds,
    /// Whether this is a built-in preset (not user-defined)
    #[allow(dead_code)]
    pub builtin: bool,
}

impl MascotPreset {
    /// UI accent color for this preset
    pub fn accent_rgb(&self) -> (u8, u8, u8) {
        self.color_scheme.accent_rgb()
    }
}

// ── Built-in presets ───────────────────────────────────────────────────

/// Claude preset: orange, default head (the original unleashed mascot)
fn preset_claude() -> MascotPreset {
    MascotPreset {
        id: "claude".to_string(),
        display_name: "Claude".to_string(),
        description: "Anthropic Claude - Orange".to_string(),
        head_right: HeadAsset::Default,
        head_left: HeadAsset::Default,
        color_scheme: ColorScheme::identity(),
        head_bounds: HeadBounds::default(),
        builtin: true,
    }
}

/// Qwen preset: purple
fn preset_qwen() -> MascotPreset {
    MascotPreset {
        id: "qwen".to_string(),
        display_name: "Qwen".to_string(),
        description: "Alibaba Qwen - Purple".to_string(),
        head_right: HeadAsset::AnsiArt(include_str!("assets/heads/qwen-right.ans").to_string()),
        head_left: HeadAsset::AnsiArt(include_str!("assets/heads/qwen-left.ans").to_string()),
        color_scheme: ColorScheme::Solid {
            hue_shift: 260.0,
            sat_scale: 1.0,
        },
        head_bounds: HeadBounds::default(),
        builtin: true,
    }
}

/// OpenAI preset: grey
fn preset_openai() -> MascotPreset {
    MascotPreset {
        id: "openai".to_string(),
        display_name: "OpenAI".to_string(),
        description: "OpenAI - Grey".to_string(),
        head_right: HeadAsset::AnsiArt(include_str!("assets/heads/openai-right.ans").to_string()),
        head_left: HeadAsset::AnsiArt(include_str!("assets/heads/openai-left.ans").to_string()),
        color_scheme: ColorScheme::Solid {
            hue_shift: 0.0,
            sat_scale: 0.08, // near-zero saturation = grey
        },
        head_bounds: HeadBounds::default(),
        builtin: true,
    }
}

/// Gemini preset: diagonal gradient blue → green → yellow → purple
fn preset_gemini() -> MascotPreset {
    MascotPreset {
        id: "gemini".to_string(),
        display_name: "Gemini".to_string(),
        description: "Google Gemini - Gradient".to_string(),
        head_right: HeadAsset::AnsiArt(include_str!("assets/heads/gemini-right.ans").to_string()),
        head_left: HeadAsset::AnsiArt(include_str!("assets/heads/gemini-left.ans").to_string()),
        color_scheme: ColorScheme::Gradient(GradientDef::gemini()),
        head_bounds: HeadBounds::default(),
        builtin: true,
    }
}

/// Generic preset: follows current theme color, neutral head shape
fn preset_generic() -> MascotPreset {
    MascotPreset {
        id: "generic".to_string(),
        display_name: "Generic".to_string(),
        description: "Generic agent - Theme color".to_string(),
        head_right: HeadAsset::AnsiArt(include_str!("assets/heads/generic-right.ans").to_string()),
        head_left: HeadAsset::AnsiArt(include_str!("assets/heads/generic-left.ans").to_string()),
        color_scheme: ColorScheme::identity(), // uses whatever theme color is active
        head_bounds: HeadBounds::default(),
        builtin: true,
    }
}

// ── Head entries ──────────────────────────────────────────────────────

/// A standalone head entry: head overlay without any color binding.
#[derive(Debug, Clone)]
pub struct HeadEntry {
    /// Unique identifier (e.g. "default", "qwen")
    pub id: String,
    /// Display name for the UI
    pub display_name: String,
    /// Head asset to overlay onto the body (right-facing)
    pub head_right: HeadAsset,
    /// Bounding box for head overlay
    pub head_bounds: HeadBounds,
}

// ── Preset registry ────────────────────────────────────────────────────

/// Registry of all available mascot presets (built-in + user-defined)
pub struct MascotRegistry {
    presets: HashMap<String, MascotPreset>,
    /// Ordered list of preset IDs for display
    order: Vec<String>,
    /// Ordered list of head entries (independent of presets)
    heads: Vec<HeadEntry>,
}

impl MascotRegistry {
    /// Create a registry with all built-in presets
    pub fn new() -> Self {
        let builtins = vec![
            preset_claude(),
            preset_qwen(),
            preset_openai(),
            preset_gemini(),
            preset_generic(),
        ];

        let mut presets = HashMap::new();
        let mut order = Vec::new();

        for preset in &builtins {
            order.push(preset.id.clone());
            presets.insert(preset.id.clone(), preset.clone());
        }

        // Build head entries from built-in presets.
        // The first entry is "default" (no head overlay), then one per non-default preset.
        let mut heads = vec![HeadEntry {
            id: "default".to_string(),
            display_name: "Default".to_string(),
            head_right: HeadAsset::Default,
            head_bounds: HeadBounds::default(),
        }];
        for preset in &builtins {
            if matches!(preset.head_right, HeadAsset::AnsiArt(_)) {
                heads.push(HeadEntry {
                    id: preset.id.clone(),
                    display_name: preset.display_name.clone(),
                    head_right: preset.head_right.clone(),
                    head_bounds: preset.head_bounds,
                });
            }
        }

        Self { presets, order, heads }
    }

    /// Create registry and load user presets from config directory
    pub fn with_user_presets() -> Self {
        let mut registry = Self::new();
        registry.load_user_presets();
        registry
    }

    /// Get a preset by ID
    pub fn get(&self, id: &str) -> Option<&MascotPreset> {
        self.presets.get(id)
    }

    /// Get the default preset
    #[allow(dead_code)]
    pub fn default_preset(&self) -> &MascotPreset {
        self.presets.get("claude").expect("claude preset must exist")
    }

    /// List all presets in display order
    pub fn all(&self) -> Vec<&MascotPreset> {
        self.order.iter().filter_map(|id| self.presets.get(id)).collect()
    }

    /// Number of registered presets
    pub fn len(&self) -> usize {
        self.presets.len()
    }

    /// List all head entries in display order
    pub fn all_heads(&self) -> &[HeadEntry] {
        &self.heads
    }

    /// Get a head entry by ID
    pub fn get_head(&self, id: &str) -> Option<&HeadEntry> {
        self.heads.iter().find(|h| h.id == id)
    }

    /// Number of registered head entries
    pub fn head_count(&self) -> usize {
        self.heads.len()
    }

    /// Register a user-defined preset
    pub fn register(&mut self, preset: MascotPreset) {
        // Also register as a head entry if it has a custom head
        if matches!(preset.head_right, HeadAsset::AnsiArt(_)) {
            if !self.heads.iter().any(|h| h.id == preset.id) {
                self.heads.push(HeadEntry {
                    id: preset.id.clone(),
                    display_name: preset.display_name.clone(),
                    head_right: preset.head_right.clone(),
                    head_bounds: preset.head_bounds,
                });
            }
        }

        if !self.order.contains(&preset.id) {
            self.order.push(preset.id.clone());
        }
        self.presets.insert(preset.id.clone(), preset);
    }

    /// Load user presets from ~/.config/agent-unleashed/mascots/
    fn load_user_presets(&mut self) {
        let mascots_dir = match user_mascots_dir() {
            Some(d) => d,
            None => return,
        };

        if !mascots_dir.exists() {
            return;
        }

        let entries = match fs::read_dir(&mascots_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "toml") {
                if let Some(preset) = load_user_preset(&path) {
                    self.register(preset);
                }
            }
        }
    }
}

impl Default for MascotRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ── User preset loading ────────────────────────────────────────────────

/// TOML structure for user-defined mascot presets
#[derive(Debug, Deserialize)]
struct UserPresetConfig {
    id: String,
    display_name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    head_file: Option<String>,
    #[serde(flatten)]
    color: UserColorConfig,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "color_scheme", rename_all = "lowercase")]
enum UserColorConfig {
    Solid {
        #[serde(default)]
        color: Option<String>,
    },
    Gradient {
        #[serde(flatten)]
        gradient: GradientDef,
    },
}

impl Default for UserColorConfig {
    fn default() -> Self {
        UserColorConfig::Solid { color: None }
    }
}

fn user_mascots_dir() -> Option<PathBuf> {
    let config_base = dirs::config_dir()?;
    let new_path = config_base.join("agent-unleashed").join("mascots");
    let legacy_path = config_base.join("claude-unleashed").join("mascots");

    if new_path.exists() {
        Some(new_path)
    } else if legacy_path.exists() {
        Some(legacy_path)
    } else {
        Some(new_path) // default path for new installs
    }
}

fn load_user_preset(path: &std::path::Path) -> Option<MascotPreset> {
    let content = fs::read_to_string(path).ok()?;
    let config: UserPresetConfig = toml::from_str(&content).ok()?;

    let (head_right, head_left) = match config.head_file {
        Some(ref file) => {
            let head_path = path.parent()?.join(file);
            let head_content = fs::read_to_string(&head_path).ok()?;
            // Try to find a left variant: replace "-right" with "-left" in filename
            let left_content = {
                let left_file = file.replace("-right", "-left");
                if left_file != *file {
                    let left_path = path.parent()?.join(&left_file);
                    fs::read_to_string(left_path).ok()
                } else {
                    None
                }
            };
            let left = left_content
                .map(|s| HeadAsset::AnsiArt(s))
                .unwrap_or_else(|| HeadAsset::AnsiArt(head_content.clone()));
            (HeadAsset::AnsiArt(head_content), left)
        }
        None => (HeadAsset::Default, HeadAsset::Default),
    };

    let color_scheme = match config.color {
        UserColorConfig::Solid { color } => {
            if let Some(hex) = color {
                if let Some((r, g, b)) = crate::theme::parse_hex_color(&hex) {
                    let (target_h, target_s, _) = crate::theme::rgb_to_hsl(r, g, b);
                    let base_h = 14.77;
                    let base_s = 0.631;
                    ColorScheme::Solid {
                        hue_shift: (target_h - base_h).rem_euclid(360.0),
                        sat_scale: if base_s > f64::EPSILON { target_s / base_s } else { 1.0 },
                    }
                } else {
                    ColorScheme::identity()
                }
            } else {
                ColorScheme::identity()
            }
        }
        UserColorConfig::Gradient { gradient } => ColorScheme::Gradient(gradient),
    };

    Some(MascotPreset {
        id: config.id,
        display_name: config.display_name,
        description: config.description,
        head_right,
        head_left,
        color_scheme,
        head_bounds: HeadBounds::default(),
        builtin: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_builtins() {
        let registry = MascotRegistry::new();
        assert!(registry.get("claude").is_some());
        assert!(registry.get("qwen").is_some());
        assert!(registry.get("openai").is_some());
        assert!(registry.get("gemini").is_some());
        assert!(registry.get("generic").is_some());
        assert_eq!(registry.len(), 5);
    }

    #[test]
    fn test_registry_order() {
        let registry = MascotRegistry::new();
        let all = registry.all();
        assert_eq!(all[0].id, "claude");
        assert_eq!(all[1].id, "qwen");
        assert_eq!(all[2].id, "openai");
        assert_eq!(all[3].id, "gemini");
        assert_eq!(all[4].id, "generic");
    }

    #[test]
    fn test_default_preset_is_claude() {
        let registry = MascotRegistry::new();
        assert_eq!(registry.default_preset().id, "claude");
    }

    #[test]
    fn test_all_heads_order() {
        let registry = MascotRegistry::new();
        let heads = registry.all_heads();
        assert_eq!(heads[0].id, "default");
        assert_eq!(heads[1].id, "qwen");
        assert_eq!(heads[2].id, "openai");
        assert_eq!(heads[3].id, "gemini");
        assert_eq!(heads[4].id, "generic");
        assert_eq!(heads.len(), 5);
    }

    #[test]
    fn test_get_head() {
        let registry = MascotRegistry::new();
        assert!(registry.get_head("default").is_some());
        assert!(registry.get_head("qwen").is_some());
        assert!(registry.get_head("nonexistent").is_none());
    }

    #[test]
    fn test_head_count() {
        let registry = MascotRegistry::new();
        assert_eq!(registry.head_count(), 5);
    }

    #[test]
    fn test_claude_preset_identity() {
        let preset = preset_claude();
        assert!(!preset.color_scheme.is_gradient());
        let shift = preset.color_scheme.as_theme_shift();
        assert!(shift.is_identity());
    }

    #[test]
    fn test_gemini_preset_is_gradient() {
        let preset = preset_gemini();
        assert!(preset.color_scheme.is_gradient());
    }

    #[test]
    fn test_openai_preset_low_saturation() {
        let preset = preset_openai();
        if let ColorScheme::Solid { sat_scale, .. } = preset.color_scheme {
            assert!(sat_scale < 0.2, "OpenAI should be near-grey: sat_scale={}", sat_scale);
        } else {
            panic!("OpenAI should be solid");
        }
    }

    #[test]
    fn test_register_custom_preset() {
        let mut registry = MascotRegistry::new();
        let custom = MascotPreset {
            id: "custom-test".to_string(),
            display_name: "Test".to_string(),
            description: "A test preset".to_string(),
            head_right: HeadAsset::Default,
            head_left: HeadAsset::Default,
            color_scheme: ColorScheme::identity(),
            head_bounds: HeadBounds::default(),
            builtin: false,
        };
        registry.register(custom);
        assert_eq!(registry.len(), 6);
        assert!(registry.get("custom-test").is_some());
    }

    #[test]
    fn test_preset_accent_colors() {
        let claude = preset_claude();
        let (r, _, _) = claude.accent_rgb();
        assert_eq!(r, 217, "Claude accent should be base orange");

        let gemini = preset_gemini();
        let (r, _, _) = gemini.accent_rgb();
        assert_eq!(r, 66, "Gemini accent should be first gradient stop (blue)");
    }
}
