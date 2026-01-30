//! Sprite cache: pre-composites mascot body + head into .ans files
//!
//! Writes composites to `~/.cache/agent-unleashed/sprites/` so the TUI
//! can load them from disk instead of compositing every frame.

use crate::mascot::{HeadAsset, MascotPreset, MascotRegistry};
use crate::pixel_art::CellGrid;
use std::fs;
use std::path::PathBuf;

/// Return the sprite cache directory (`~/.cache/agent-unleashed/sprites/`)
pub fn sprite_cache_dir() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("agent-unleashed").join("sprites"))
}

/// Ensure the cache directory exists
pub fn ensure_dir() -> Option<PathBuf> {
    let dir = sprite_cache_dir()?;
    fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Path for a composite file: `<cache>/full-<preset_id>.ans`
pub fn composite_path(preset_id: &str) -> Option<PathBuf> {
    sprite_cache_dir().map(|d| d.join(format!("full-{}.ans", preset_id)))
}

/// Read a cached composite from disk
pub fn read_composite(preset_id: &str) -> Option<String> {
    let path = composite_path(preset_id)?;
    fs::read_to_string(path).ok()
}

/// Write a composite ANSI string to disk
pub fn write_composite(preset_id: &str, content: &str) -> Option<()> {
    ensure_dir()?;
    let path = composite_path(preset_id)?;
    fs::write(path, content).ok()
}

/// Generate a composite for the given preset and write it to disk.
///
/// Loads the compiled-in `ct4-full.ans` body, overlays the right-facing
/// head onto the right half and the left-facing head onto the left half,
/// then serializes back to ANSI and writes to cache.
pub fn generate_composite(preset: &MascotPreset) -> Option<String> {
    let body_ansi = include_str!("assets/ct4-full.ans");
    let mut grid = CellGrid::from_ansi(body_ansi);
    let bounds = &preset.head_bounds;

    // Overlay right-facing head onto right half (columns 53+)
    if let HeadAsset::AnsiArt(ref head) = preset.head_right {
        let head_grid = CellGrid::from_ansi(head);
        grid.overlay(
            &head_grid,
            53 + bounds.x_offset as usize,
            bounds.y_offset as usize,
        );
    }

    // Overlay left-facing head onto left half (columns 0..53)
    if let HeadAsset::AnsiArt(ref head) = preset.head_left {
        let head_grid = CellGrid::from_ansi(head);
        grid.overlay(&head_grid, bounds.x_offset as usize, bounds.y_offset as usize);
    }

    let ansi = grid.to_ansi();
    write_composite(&preset.id, &ansi);
    Some(ansi)
}

/// Load a composite from cache, or generate + cache it if missing.
/// Returns the full 106-col ANSI string.
pub fn load_or_generate(preset: &MascotPreset) -> Option<String> {
    if let Some(cached) = read_composite(&preset.id) {
        return Some(cached);
    }
    generate_composite(preset)
}

/// Generate composites for all presets in the registry.
/// If `force` is true, regenerate even if cached.
#[allow(dead_code)]
pub fn generate_all(registry: &MascotRegistry, force: bool) {
    for preset in registry.all() {
        if force || read_composite(&preset.id).is_none() {
            generate_composite(preset);
        }
    }
}
