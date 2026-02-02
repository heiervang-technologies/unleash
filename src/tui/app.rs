//! Main TUI application

use crate::agents::{AgentManager, AgentType};
use crate::config::{AppConfig, Profile, ProfileManager};
use crate::input::{key_to_action, MenuState, NavAction};
use crate::pixel_art::mascots;
use crate::text_input::{censor_sensitive, is_sensitive_key, TextInput};
use crate::theme::{ThemeColor, ThemePreset};
use crate::version::{get_version_filter_mode_for, is_version_allowed_for, InstallResult, VersionFilterMode, VersionInfo, VersionManager};
#[cfg(test)]
use crate::version::{is_whitelisted_for, is_blacklisted_for};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::collections::HashMap;
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// Width of the ANSI art sidebar (both left and right versions are the same width)
const ART_WIDTH: u16 = 53;

/// Duration of slide animation in milliseconds
const ANIMATION_DURATION_MS: u64 = 600;

/// State of the art slide animation
///
/// The animation slides Claude from one side of the screen to the other.
/// The sprite starts at its current render position and ends at its destination position.
/// Progress 0.0 = start position, 1.0 = end position.
#[derive(Debug, Clone)]
pub struct ArtAnimation {
    /// When the animation started
    pub start_time: Instant,
    /// Duration of the animation
    pub duration: Duration,
    /// True if moving from right side to left side
    pub to_left_side: bool,
    /// X position where the art is rendered in the source screen (content_width or 0)
    pub start_art_x: u16,
    /// X position where the art is rendered in the destination screen (0 or content_width)
    pub end_art_x: u16,
}

impl ArtAnimation {
    /// Create a new slide animation
    pub fn new(to_left_side: bool, start_art_x: u16, end_art_x: u16) -> Self {
        Self {
            start_time: Instant::now(),
            duration: Duration::from_millis(ANIMATION_DURATION_MS),
            to_left_side,
            start_art_x,
            end_art_x,
        }
    }

    /// Calculate figure_x position based on animation progress
    /// Returns the X coordinate for the left edge of the full 106-char sprite
    pub fn figure_x(&self) -> i32 {
        let progress = self.progress();
        let art_w = ART_WIDTH as i32;

        // The full sprite is 106 chars (2 * ART_WIDTH).
        // At start: we want the visible half to align with start_art_x
        // At end: we want the visible half to align with end_art_x
        //
        // When art is on right (start_art_x = content_width):
        //   - Left half is visible, figure_x = start_art_x (left half at start_art_x..start_art_x+53)
        // When art is on left (end_art_x = 0):
        //   - Right half is visible, figure_x = -ART_WIDTH (right half at 0..53)
        let (start_x, end_x) = if self.to_left_side {
            // Moving right to left: start with left half visible, end with right half visible
            (self.start_art_x as i32, -art_w)
        } else {
            // Moving left to right: start with right half visible, end with left half visible
            (-art_w, self.end_art_x as i32)
        };

        start_x + ((end_x - start_x) as f64 * progress) as i32
    }

    /// Get animation progress (0.0 to 1.0) with easing
    pub fn progress(&self) -> f64 {
        let elapsed = self.start_time.elapsed();
        if elapsed >= self.duration {
            return 1.0;
        }

        // Calculate raw progress (0.0 to 1.0)
        let progress = elapsed.as_secs_f64() / self.duration.as_secs_f64();

        // Apply ease-in-out cubic easing for smooth acceleration and deceleration
        if progress < 0.5 {
            4.0 * progress * progress * progress
        } else {
            1.0 - (-2.0 * progress + 2.0).powi(3) / 2.0
        }
    }

    /// Check if the animation is complete
    pub fn is_complete(&self) -> bool {
        self.start_time.elapsed() >= self.duration
    }
}

/// Application screens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Profiles,
    ProfileEdit,
    EnvVarEdit,
    Settings,
    Theme,
    Help,
    ConfirmDelete,
    Updating,
    VersionManagement,
    VersionInstalling,
}

/// What we're currently editing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditField {
    None,
    #[allow(dead_code)]
    ProfileName,
    #[allow(dead_code)]
    ProfileDescription,
    EnvKey,
    EnvValue,
    ClaudePath,
    ClaudeArgs,
    StopPrompt,
    ThemeHex,
}

/// State for async version installation
pub struct InstallState {
    pub agent_type: AgentType,
    pub version: String,
    pub is_allowed: bool,
    pub receiver: Receiver<InstallStepResult>,
    pub _handle: JoinHandle<()>,
    pub start_time: Instant,
    pub current_step: InstallStep,
    pub install_result: Option<InstallResult>,
    pub patch_result: Option<InstallResult>,
}

/// Current step in the installation process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStep {
    Installing,
    Patching,
    Done,
}

/// Result from a single installation step
pub enum InstallStepResult {
    InstallComplete(InstallResult),
    PatchComplete(InstallResult),
}

/// Spinner animation frames
const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

/// Art layout configuration
/// Controls where Claude mascot appears relative to content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ArtLayout {
    /// Left-facing Claude on right side of content (default for main view)
    #[default]
    ArtRight,
    /// Right-facing Claude on left side of content
    ArtLeft,
}

/// Main application state
pub struct App {
    pub running: bool,
    pub screen: Screen,
    pub main_menu: MenuState,
    pub profile_menu: MenuState,
    pub settings_menu: MenuState,
    pub profile_manager: ProfileManager,
    pub app_config: AppConfig,
    pub profiles: Vec<Profile>,
    pub selected_profile: Option<Profile>,
    pub status_message: Option<String>,

    // Profile editing
    pub editing_profile: Option<Profile>,
    pub env_vars_list: Vec<(String, String)>,
    pub env_menu: MenuState,

    // Text input
    pub edit_field: EditField,
    pub key_input: TextInput,
    pub value_input: TextInput,
    pub editing_env_index: Option<usize>,

    // Version management
    pub version_manager: VersionManager,
    pub version_menu: MenuState,
    pub versions: Vec<VersionInfo>,
    pub selected_version: Option<String>,
    /// Which agent CLI is selected in the version management screen
    pub version_agent: AgentType,
    /// Cached installed version per agent type
    pub cached_agent_versions: HashMap<AgentType, Option<String>>,
    /// Cached version lists per agent type (avoids blocking npm queries)
    cached_version_lists: HashMap<AgentType, Vec<VersionInfo>>,
    /// Cached installed Claude version for main menu display
    pub cached_installed_version: Option<String>,
    /// Receiver for async installed-version fetch at startup (None once done)
    version_fetch_receiver: Option<Receiver<(AgentType, Option<String>)>>,
    /// Receiver for async version-list fetch (None when not fetching)
    version_list_receiver: Option<Receiver<(AgentType, Vec<VersionInfo>)>>,
    /// Async installation state
    pub install_state: Option<InstallState>,
    /// Animation frame counter (increments each tick)
    pub animation_frame: usize,
    /// Art layout preference for main view (non-main views use the opposite)
    pub art_layout: ArtLayout,
    /// Current art slide animation (if any)
    pub art_animation: Option<ArtAnimation>,
    /// Whether animations are enabled
    pub animations_enabled: bool,
    /// Pending screen transition (waits for animation to complete)
    pub pending_screen: Option<Screen>,
    /// Pending external edit - content to edit in external editor
    pub pending_external_edit: Option<String>,
    /// Screen to return to when leaving Help (so ? works from any screen)
    pub help_return_screen: Option<Screen>,
    /// Scroll offset for help screen content
    pub help_scroll_offset: u16,

    // Easter egg: Konami code triggers lava lamp mode (idea by cac taurus)
    /// Whether lava lamp color cycling is active
    pub lava_mode: bool,
    /// Progress through Konami code sequence (0-10)
    pub konami_progress: usize,

    // Theme
    /// Menu state for theme selection screen (presets + Custom entry)
    pub theme_menu: MenuState,
    /// Currently active color theme (preset or custom RGB)
    pub theme_color: ThemeColor,
}

impl App {
    pub fn new() -> io::Result<Self> {
        let profile_manager = ProfileManager::new()?;
        let app_config = profile_manager.load_app_config().unwrap_or_default();
        let profiles = profile_manager.load_all_profiles().unwrap_or_default();

        let selected_profile = profiles
            .iter()
            .find(|p| p.name == app_config.current_profile)
            .cloned()
            .or_else(|| profiles.first().cloned());

        let version_manager = VersionManager::new();

        // Spawn a background thread to fetch installed versions for all agents
        // This prevents blocking the TUI startup
        let (version_tx, version_rx) = mpsc::channel();
        thread::spawn(move || {
            // Claude version
            let claude_version = VersionManager::new().get_installed_version();
            let _ = version_tx.send((AgentType::Claude, claude_version));

            // Codex version
            let codex_version = AgentManager::new()
                .ok()
                .and_then(|mut m| m.get_installed_version(AgentType::Codex).ok().flatten());
            let _ = version_tx.send((AgentType::Codex, codex_version));
        });

        let theme_color = ThemeColor::from_config(&app_config.theme).unwrap_or(ThemeColor::Preset(ThemePreset::Orange));

        Ok(Self {
            running: true,
            screen: Screen::Main,
            main_menu: MenuState::new(8), // Start, Profiles, Agent Versions, Settings, Theme, Update, Help, Quit
            profile_menu: MenuState::new(profiles.len()),
            settings_menu: MenuState::new(4), // Entry Point, Arguments, Stop Prompt, Reset
            profile_manager,
            app_config,
            profiles,
            selected_profile,
            status_message: None,
            editing_profile: None,
            env_vars_list: Vec::new(),
            env_menu: MenuState::new(0),
            edit_field: EditField::None,
            key_input: TextInput::new(),
            value_input: TextInput::new(),
            editing_env_index: None,
            version_manager,
            version_menu: MenuState::new(0),
            versions: Vec::new(),
            selected_version: None,
            version_agent: AgentType::Claude,
            cached_agent_versions: HashMap::new(),
            cached_version_lists: HashMap::new(),
            cached_installed_version: None, // Will be populated async
            version_fetch_receiver: Some(version_rx),
            version_list_receiver: None,
            install_state: None,
            animation_frame: 0,
            art_layout: ArtLayout::ArtRight,
            art_animation: None,
            animations_enabled: true,
            pending_screen: None,
            pending_external_edit: None,
            help_return_screen: None,
            help_scroll_offset: 0,
            lava_mode: false,
            konami_progress: 0,
            theme_menu: MenuState::new(ThemePreset::all().len() + 1), // presets + Custom
            theme_color,
        })
    }

    /// Refresh the cached installed version for a specific agent
    pub fn refresh_cached_version_for(&mut self, agent_type: AgentType) {
        let version = match agent_type {
            AgentType::Claude => {
                let v = self.version_manager.get_installed_version();
                self.cached_installed_version = v.clone();
                v
            }
            AgentType::Codex => {
                AgentManager::new()
                    .ok()
                    .and_then(|mut m| m.get_installed_version(AgentType::Codex).ok().flatten())
            }
        };
        self.cached_agent_versions.insert(agent_type, version);
    }

    /// Refresh the cached installed version (call after installing a new version)
    #[allow(dead_code)]
    pub fn refresh_cached_version(&mut self) {
        self.refresh_cached_version_for(AgentType::Claude);
    }

    /// Check for Konami code sequence: Up, Up, Down, Down, Left, Right, Left, Right, B, A
    /// Activates lava lamp easter egg when completed (idea by cac taurus)
    fn check_konami_code(&mut self, code: KeyCode) {
        const KONAMI: [KeyCode; 10] = [
            KeyCode::Up,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Char('b'),
            KeyCode::Char('a'),
        ];

        // Check if current key matches the next expected key in sequence
        let expected = KONAMI.get(self.konami_progress);
        let matches = match (expected, code) {
            (Some(KeyCode::Char(expected_c)), KeyCode::Char(actual_c)) => {
                expected_c.eq_ignore_ascii_case(&actual_c)
            }
            (Some(expected_code), actual_code) => *expected_code == actual_code,
            _ => false,
        };

        if matches {
            self.konami_progress += 1;
            if self.konami_progress >= KONAMI.len() {
                // Konami code complete! Toggle lava mode
                self.lava_mode = !self.lava_mode;
                self.konami_progress = 0;
                self.status_message = Some(if self.lava_mode {
                    "🌋 Lava lamp mode activated!".to_string()
                } else {
                    "Lava lamp mode deactivated".to_string()
                });
            }
        } else {
            // Reset progress if wrong key (but check if it starts a new sequence)
            self.konami_progress = if code == KeyCode::Up { 1 } else { 0 };
        }
    }

    /// Called on each tick to advance animation and poll async operations
    pub fn tick(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);

        // Poll async version fetch (drains all available agent version messages)
        if let Some(ref receiver) = self.version_fetch_receiver {
            loop {
                match receiver.try_recv() {
                    Ok((agent_type, version)) => {
                        if agent_type == AgentType::Claude {
                            self.cached_installed_version = version.clone();
                        }
                        self.cached_agent_versions.insert(agent_type, version);
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.version_fetch_receiver = None;
                        break;
                    }
                }
            }
        }

        // Poll async version list fetch
        if let Some(ref receiver) = self.version_list_receiver {
            match receiver.try_recv() {
                Ok((agent_type, versions)) => {
                    self.cached_version_lists.insert(agent_type, versions.clone());
                    // Update displayed list if we're still viewing this agent
                    if self.screen == Screen::VersionManagement && self.version_agent == agent_type {
                        let prev_selected = self.version_menu.selected;
                        self.versions = versions;
                        self.version_menu.set_items_count(self.versions.len());
                        // Preserve selection if possible
                        if prev_selected < self.versions.len() {
                            self.version_menu.selected = prev_selected;
                        }
                        self.status_message = Some(format!("{} versions loaded", agent_type.display_name()));
                    }
                    self.version_list_receiver = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.version_list_receiver = None;
                }
            }
        }

        // Clear completed art animations and complete pending screen transitions
        if let Some(ref animation) = self.art_animation {
            if animation.is_complete() {
                self.art_animation = None;
                // Complete pending screen transition
                if let Some(next_screen) = self.pending_screen.take() {
                    self.screen = next_screen;
                    self.refresh_screen_data();
                }
            }
        } else if let Some(next_screen) = self.pending_screen.take() {
            // No animation (animations disabled) - complete transition immediately
            self.screen = next_screen;
            self.refresh_screen_data();
        }

        // Poll installation progress
        if let Some(ref mut state) = self.install_state {
            // Try to receive results without blocking
            while let Ok(result) = state.receiver.try_recv() {
                match result {
                    InstallStepResult::InstallComplete(install_result) => {
                        state.install_result = Some(install_result.clone());
                        if install_result.success {
                            // Only Claude Code has a patching step
                            if state.agent_type == AgentType::Claude {
                                state.current_step = InstallStep::Patching;
                            } else {
                                state.current_step = InstallStep::Done;
                            }
                        } else {
                            state.current_step = InstallStep::Done;
                        }
                    }
                    InstallStepResult::PatchComplete(patch_result) => {
                        state.patch_result = Some(patch_result);
                        state.current_step = InstallStep::Done;
                    }
                }
            }

            // If done, update status and return to version list
            if state.current_step == InstallStep::Done {
                let version = state.version.clone();
                let agent_type = state.agent_type;
                let agent_name = agent_type.display_name();
                let is_allowed = state.is_allowed;
                let install_ok = state.install_result.as_ref().is_some_and(|r| r.success);
                let patch_ok = state.patch_result.as_ref().is_some_and(|r| r.success);

                self.status_message = Some(if !install_ok {
                    let err = state
                        .install_result
                        .as_ref()
                        .and_then(|r| r.error.clone())
                        .unwrap_or_else(|| "unknown error".to_string());
                    format!("{} install failed: {}", agent_name, err)
                } else if agent_type == AgentType::Claude {
                    // Claude-specific messages include patch status
                    if patch_ok {
                        if !is_allowed {
                            format!("Installed and patched v{} (not recommended)", version)
                        } else {
                            format!("Installed and patched v{}", version)
                        }
                    } else {
                        if !is_allowed {
                            format!("Installed v{} (not recommended, patch unavailable)", version)
                        } else {
                            format!("Installed v{} (patch unavailable)", version)
                        }
                    }
                } else {
                    format!("{} v{} installed", agent_name, version)
                });

                self.install_state = None;
                self.refresh_versions();
                self.refresh_cached_version_for(agent_type);
                self.screen = Screen::VersionManagement;
            }
        }
    }

    /// Get the current spinner frame
    pub fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.animation_frame % SPINNER_FRAMES.len()]
    }

    /// Trigger a slide animation when transitioning between screens
    /// Call this when navigating from Main to a submenu or vice versa
    fn trigger_screen_animation(&mut self, from_main: bool, dest_screen: Screen) {
        if !self.animations_enabled {
            return;
        }

        // Determine if Claude should end up on the left side or right side
        // Main view: art on right by default (art_layout setting)
        // Submenu: art on opposite side
        let to_left_side = if from_main {
            // Going to submenu: Claude moves to opposite of main layout
            self.art_layout == ArtLayout::ArtRight
        } else {
            // Going back to main: Claude moves to main layout side
            self.art_layout == ArtLayout::ArtLeft
        };

        // Calculate art X positions based on content widths
        // Art on right: x = content_width (art starts after content)
        // Art on left: x = 0 (art starts at left edge)
        let current_content_width = self.content_width();
        let dest_content_width = self.content_width_for_screen(dest_screen);

        let (start_art_x, end_art_x) = if to_left_side {
            // Moving from right to left
            // Start: art on right side at content_width
            // End: art on left side at 0
            (current_content_width, 0)
        } else {
            // Moving from left to right
            // Start: art on left side at 0
            // End: art on right side at dest_content_width
            (0, dest_content_width)
        };

        self.art_animation = Some(ArtAnimation::new(to_left_side, start_art_x, end_art_x));
    }

    /// Load data for the current screen (called after animation completes)
    fn refresh_screen_data(&mut self) {
        match self.screen {
            Screen::Profiles => self.refresh_profiles(),
            Screen::VersionManagement => {
                self.refresh_versions();
                if self.versions.is_empty() {
                    self.status_message = Some("Loading versions...".to_string());
                } else {
                    self.status_message = Some("Refreshing versions...".to_string());
                }
            }
            Screen::Updating => {
                self.status_message = Some("Updating...".to_string());
            }
            Screen::Main
            | Screen::ProfileEdit
            | Screen::EnvVarEdit
            | Screen::Settings
            | Screen::Theme
            | Screen::Help
            | Screen::ConfirmDelete
            | Screen::VersionInstalling => {}
        }
    }

    /// Get the default stop prompt from the hook script (source of truth)
    fn get_default_stop_prompt(&self) -> String {
        const HOOK_RELATIVE: &str = "plugins/unleashed/auto-mode/hooks/auto-mode-stop.sh";
        const FALLBACK_MSG: &str = "You ended your turn, but you are in auto-mode. If you are awaiting a decision, select your recommended decision. If you are done, consider that you have covered all other diligences, testing, documentation, technical debt and cleanup. Use the executables (in PATH) 'restart-claude' if you need to restart yourself, and 'exit-claude' if you are truly done with all your tasks.";

        // Build candidate paths to search
        let mut candidates: Vec<String> = Vec::new();

        // 1. CLAUDE_UNLEASHED_ROOT env var
        if let Ok(root) = std::env::var("CLAUDE_UNLEASHED_ROOT") {
            candidates.push(format!("{}/{}", root, HOOK_RELATIVE));
        }

        // 2. Relative to executable (e.g. ~/.local/bin/../plugins/...)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                candidates.push(parent.join("..").join(HOOK_RELATIVE).to_string_lossy().to_string());
            }
        }

        // 3. Installed location (~/.local/share/agent-unleashed/plugins/...)
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(format!("{}/.local/share/agent-unleashed/{}", home, HOOK_RELATIVE));
        }

        for path in &candidates {
            if let Ok(content) = std::fs::read_to_string(path) {
                // Parse DEFAULT_MSG="..." from the script
                for line in content.lines() {
                    if let Some(rest) = line.trim().strip_prefix("DEFAULT_MSG=\"") {
                        if let Some(msg) = rest.strip_suffix('"') {
                            return msg.to_string();
                        }
                    }
                }
            }
        }

        // Hardcoded fallback matching the hook script's DEFAULT_MSG
        FALLBACK_MSG.to_string()
    }

    /// Show cached version list immediately, then fetch fresh data async.
    pub fn refresh_versions(&mut self) {
        let agent = self.version_agent;

        // Show cached list instantly (may be empty on first load)
        if let Some(cached) = self.cached_version_lists.get(&agent) {
            self.versions = cached.clone();
        }
        self.version_menu.set_items_count(self.versions.len());

        // Kick off async refresh (both Claude and Codex fetch from network)
        self.start_async_version_fetch(agent);
    }

    /// Spawn a background thread to fetch the version list for an agent
    fn start_async_version_fetch(&mut self, agent: AgentType) {
        let (tx, rx) = mpsc::channel();
        match agent {
            AgentType::Claude => {
                thread::spawn(move || {
                    let vm = VersionManager::new();
                    let versions = vm.get_version_list();
                    let _ = tx.send((AgentType::Claude, versions));
                });
            }
            AgentType::Codex => {
                let installed = self
                    .cached_agent_versions
                    .get(&AgentType::Codex)
                    .and_then(|v| v.clone());
                thread::spawn(move || {
                    let vm = VersionManager::new();
                    let versions = vm.get_codex_version_list(installed.as_deref());
                    let _ = tx.send((AgentType::Codex, versions));
                });
            }
        }
        self.version_list_receiver = Some(rx);
    }

    /// Build version list for Codex (synchronous fallback before async fetch completes)
    #[cfg(test)]
    fn get_codex_version_list(&self) -> Vec<VersionInfo> {
        let installed = self
            .cached_agent_versions
            .get(&AgentType::Codex)
            .and_then(|v| v.clone());

        let mut versions = Vec::new();

        // Show installed version if present
        if let Some(v) = installed {
            versions.push(VersionInfo {
                version: v.clone(),
                is_installed: true,
                has_patch: false,
                is_whitelisted: is_whitelisted_for(&v, AgentType::Codex),
                is_blacklisted: is_blacklisted_for(&v, AgentType::Codex),
            });
        }

        versions
    }

    pub fn refresh_profiles(&mut self) {
        self.profiles = self.profile_manager.load_all_profiles().unwrap_or_default();
        self.profile_menu.set_items_count(self.profiles.len());
    }

    fn load_profile_for_editing(&mut self, profile: Profile) {
        self.env_vars_list = profile.env.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        self.env_vars_list.sort_by(|a, b| a.0.cmp(&b.0));
        self.env_menu.set_items_count(self.env_vars_list.len() + 1); // +1 for "Add new"
        self.env_menu.selected = 0;
        self.editing_profile = Some(profile);
    }

    fn save_editing_profile(&mut self) -> io::Result<()> {
        if let Some(ref mut profile) = self.editing_profile {
            profile.env.clear();
            for (k, v) in &self.env_vars_list {
                profile.env.insert(k.clone(), v.clone());
            }
            self.profile_manager.save_profile(profile)?;
        }

        // Get the name before refreshing
        let edited_name = self.editing_profile.as_ref().map(|p| p.name.clone());

        self.refresh_profiles();

        // Update selected profile if it's the one we edited
        if let Some(name) = edited_name {
            if self.selected_profile.as_ref().is_some_and(|p| p.name == name) {
                self.selected_profile = self.profiles.iter().find(|p| p.name == name).cloned();
            }
            // Also update editing_profile from refreshed profiles
            self.editing_profile = self.profiles.iter().find(|p| p.name == name).cloned();
        }

        Ok(())
    }

    /// Handle input events
    pub fn handle_event(&mut self, event: Event) -> io::Result<Option<AppAction>> {
        if let Event::Key(key) = event {
            // Global quit with Ctrl+C (except when editing)
            if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                if self.edit_field == EditField::None {
                    self.running = false;
                    return Ok(None);
                }
            }

            // Easter egg: Konami code detection (idea by cac taurus)
            // Up, Up, Down, Down, Left, Right, Left, Right, B, A
            self.check_konami_code(key.code);

            // If we're editing text, handle text input
            if self.edit_field != EditField::None {
                return Ok(self.handle_text_input(key));
            }

            let action = key_to_action(key);

            // Global help: '?' opens help from any navigable screen
            if action == NavAction::Help && self.screen != Screen::Help && self.screen != Screen::VersionInstalling {
                // Only animate when transitioning from Main (mascot changes sides)
                if self.screen == Screen::Main {
                    self.trigger_screen_animation(true, Screen::Help);
                }
                self.help_return_screen = Some(self.screen);
                self.pending_screen = Some(Screen::Help);
                // If no animation was triggered, apply immediately
                if self.art_animation.is_none() {
                    self.screen = Screen::Help;
                    self.refresh_screen_data();
                }
                return Ok(None);
            }

            match self.screen {
                Screen::Main => return self.handle_main_input(action),
                Screen::Profiles => self.handle_profiles_input(action),
                Screen::ProfileEdit => self.handle_profile_edit_input(action, key),
                Screen::EnvVarEdit => self.handle_env_var_edit_input(action, key),
                Screen::Settings => self.handle_settings_input(action),
                Screen::Theme => self.handle_theme_input(action),
                Screen::Help => self.handle_help_input(action),
                Screen::ConfirmDelete => self.handle_confirm_delete_input(action),
                Screen::Updating => return self.handle_updating_input(action),
                Screen::VersionManagement => self.handle_version_input(action),
                Screen::VersionInstalling => {} // Non-interactive while installing
            }
        }
        Ok(None)
    }

    fn handle_text_input(&mut self, key: KeyEvent) -> Option<AppAction> {
        let input = match self.edit_field {
            EditField::EnvKey => &mut self.key_input,
            EditField::EnvValue => &mut self.value_input,
            EditField::ProfileName | EditField::ProfileDescription => &mut self.key_input,
            EditField::ClaudePath | EditField::ClaudeArgs | EditField::StopPrompt | EditField::ThemeHex => &mut self.key_input,
            EditField::None => return None,
        };

        match key.code {
            KeyCode::Char(c) => {
                // Handle Ctrl+key shortcuts
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' => input.move_home(),      // Ctrl+A: go to start
                        'e' => input.move_end(),      // Ctrl+E: go to end
                        'w' => input.delete_word_back(), // Ctrl+W: delete word
                        'u' => input.delete_to_start(), // Ctrl+U: delete to start
                        'k' => input.delete_to_end(),  // Ctrl+K: delete to end
                        _ => {} // Ignore other ctrl combinations
                    }
                } else {
                    input.insert(c);
                }
            }
            KeyCode::Backspace => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    input.delete_word_back(); // Ctrl+Backspace: delete word
                } else {
                    input.backspace();
                }
            }
            KeyCode::Delete => input.delete(),
            KeyCode::Left => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    input.move_word_left(); // Ctrl+Left: word left
                } else {
                    input.move_left();
                }
            }
            KeyCode::Right => {
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    input.move_word_right(); // Ctrl+Right: word right
                } else {
                    input.move_right();
                }
            }
            KeyCode::Home => input.move_home(),
            KeyCode::End => input.move_end(),
            KeyCode::Enter => {
                match self.edit_field {
                    EditField::EnvKey => {
                        // Move to value input
                        if !self.key_input.is_empty() {
                            self.edit_field = EditField::EnvValue;
                            // Check if this key is sensitive
                            if is_sensitive_key(&self.key_input.value) {
                                self.value_input.hidden = true;
                            }
                        }
                    }
                    EditField::EnvValue => {
                        // Save the env var
                        self.save_env_var();
                        self.edit_field = EditField::None;
                        self.screen = Screen::ProfileEdit;
                    }
                    EditField::ClaudePath => {
                        // Save claude_path
                        self.app_config.claude_path = self.key_input.value.clone();
                        let _ = self.profile_manager.save_app_config(&self.app_config);
                        self.status_message = Some("Entry point saved".to_string());
                        self.edit_field = EditField::None;
                    }
                    EditField::ClaudeArgs => {
                        // Save claude_args (space-separated)
                        self.app_config.claude_args = self.key_input.value
                            .split_whitespace()
                            .map(|s| s.to_string())
                            .collect();
                        let _ = self.profile_manager.save_app_config(&self.app_config);
                        self.status_message = Some("Arguments saved".to_string());
                        self.edit_field = EditField::None;
                    }
                    EditField::StopPrompt => {
                        // Save stop_prompt (empty string = None/default)
                        let value = self.key_input.value.trim().to_string();
                        self.app_config.stop_prompt = if value.is_empty() {
                            None
                        } else {
                            Some(value)
                        };
                        let _ = self.profile_manager.save_app_config(&self.app_config);
                        self.status_message = Some("Stop prompt saved".to_string());
                        self.edit_field = EditField::None;
                    }
                    EditField::ThemeHex => {
                        let hex = self.key_input.value.trim().to_string();
                        if let Some((r, g, b)) = crate::theme::parse_hex_color(&hex) {
                            self.theme_color = ThemeColor::Custom(r, g, b);
                            self.app_config.theme = self.theme_color.to_config();
                            let _ = self.profile_manager.save_app_config(&self.app_config);
                            self.status_message = Some(format!("Theme: #{:02X}{:02X}{:02X}", r, g, b));
                            self.edit_field = EditField::None;
                            self.trigger_screen_animation(false, Screen::Main);
                            self.pending_screen = Some(Screen::Main);
                        } else {
                            self.status_message = Some("Invalid hex color — use 3 or 6 hex digits (e.g. FFF or FF5500)".to_string());
                        }
                    }
                    _ => {
                        self.edit_field = EditField::None;
                    }
                }
            }
            KeyCode::Esc => {
                self.edit_field = EditField::None;
                if self.screen == Screen::EnvVarEdit {
                    self.screen = Screen::ProfileEdit;
                }
            }
            KeyCode::Tab => {
                // Toggle between key and value
                if self.edit_field == EditField::EnvKey && !self.key_input.is_empty() {
                    self.edit_field = EditField::EnvValue;
                    if is_sensitive_key(&self.key_input.value) {
                        self.value_input.hidden = true;
                    }
                } else if self.edit_field == EditField::EnvValue {
                    self.edit_field = EditField::EnvKey;
                }
            }
            _ => {}
        }
        None
    }

    fn save_env_var(&mut self) {
        let key = self.key_input.value.clone();
        let value = self.value_input.value.clone();

        if key.is_empty() {
            return;
        }

        if let Some(index) = self.editing_env_index {
            // Update existing
            if index < self.env_vars_list.len() {
                self.env_vars_list[index] = (key, value);
            }
        } else {
            // Check if key already exists
            if let Some(pos) = self.env_vars_list.iter().position(|(k, _)| k == &key) {
                self.env_vars_list[pos] = (key, value);
            } else {
                self.env_vars_list.push((key, value));
            }
        }

        // Re-sort and update menu
        self.env_vars_list.sort_by(|a, b| a.0.cmp(&b.0));
        self.env_menu.set_items_count(self.env_vars_list.len() + 1);

        // Save to file
        let _ = self.save_editing_profile();

        self.status_message = Some("Saved".to_string());
        self.editing_env_index = None;
        self.key_input.clear();
        self.value_input.clear();
        self.value_input.hidden = false;
    }

    fn handle_main_input(&mut self, action: NavAction) -> io::Result<Option<AppAction>> {
        match action {
            NavAction::Up | NavAction::Down => {
                self.main_menu.handle_action(action);
            }
            NavAction::Select => {
                match self.main_menu.selected {
                    0 => {
                        // Start Session
                        if let Some(profile) = &self.selected_profile {
                            return Ok(Some(AppAction::Launch(LaunchRequest {
                                profile: profile.clone(),
                                claude_path: self.app_config.claude_path.clone(),
                                claude_args: self.app_config.claude_args.clone(),
                            })));
                        } else {
                            self.status_message = Some("No profile selected!".to_string());
                        }
                    }
                    1 => {
                        // Profiles
                        self.trigger_screen_animation(true, Screen::Profiles);
                        self.pending_screen = Some(Screen::Profiles);
                    }
                    2 => {
                        // Agent Versions
                        self.trigger_screen_animation(true, Screen::VersionManagement);
                        self.pending_screen = Some(Screen::VersionManagement);
                    }
                    3 => {
                        // Settings
                        self.trigger_screen_animation(true, Screen::Settings);
                        self.pending_screen = Some(Screen::Settings);
                    }
                    4 => {
                        // Theme
                        // Pre-select the current theme in the menu
                        let idx = if self.theme_color.is_custom() {
                            ThemePreset::all().len() // "Custom" is the last entry
                        } else {
                            ThemePreset::all()
                                .iter()
                                .position(|t| self.theme_color.is_preset(*t))
                                .unwrap_or(0)
                        };
                        self.theme_menu.selected = idx;
                        self.trigger_screen_animation(true, Screen::Theme);
                        self.pending_screen = Some(Screen::Theme);
                    }
                    5 => {
                        // Update TUI
                        self.trigger_screen_animation(true, Screen::Updating);
                        self.pending_screen = Some(Screen::Updating);
                    }
                    6 => {
                        // Help
                        self.help_return_screen = Some(Screen::Main);
                        self.trigger_screen_animation(true, Screen::Help);
                        self.pending_screen = Some(Screen::Help);
                    }
                    7 => {
                        // Quit
                        self.running = false;
                    }
                    _ => {}
                }
            }
            NavAction::Quit | NavAction::Back => {
                // Back on main menu = quit
                self.running = false;
            }
            _ => {}
        }
        Ok(None)
    }

    fn handle_version_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.version_menu.handle_action(action);
            }
            NavAction::Left | NavAction::Right => {
                // Cycle between agent CLIs
                let agents = AgentType::all();
                let current_idx = agents
                    .iter()
                    .position(|a| *a == self.version_agent)
                    .unwrap_or(0);
                let new_idx = match action {
                    NavAction::Right => (current_idx + 1) % agents.len(),
                    NavAction::Left => {
                        current_idx.checked_sub(1).unwrap_or(agents.len() - 1)
                    }
                    _ => unreachable!(),
                };
                self.version_agent = agents[new_idx];
                self.version_menu.selected = 0;
                self.version_menu.scroll_offset = 0;
                self.refresh_versions();
                self.status_message =
                    Some(format!("Switched to {}", self.version_agent.display_name()));
            }
            NavAction::Select => {
                match self.version_agent {
                    AgentType::Claude => self.install_claude_version(),
                    AgentType::Codex => self.install_codex_version(),
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
            }
            _ => {}
        }
    }

    /// Install a selected Claude Code version (npm + patch)
    fn install_claude_version(&mut self) {
        if let Some(version_info) = self.versions.get(self.version_menu.selected) {
            let version = version_info.version.clone();
            let is_allowed = is_version_allowed_for(&version, AgentType::Claude);
            let is_reinstall = version_info.is_installed;

            self.selected_version = Some(version.clone());
            self.screen = Screen::VersionInstalling;

            let action = if is_reinstall {
                "Reinstalling"
            } else {
                "Installing"
            };
            let warning = if !is_allowed {
                " (WARNING: not recommended)"
            } else {
                ""
            };
            self.status_message = Some(format!("{} v{}{}...", action, version, warning));

            let (tx, rx) = mpsc::channel();

            let version_clone = version.clone();
            let handle = thread::spawn(move || {
                let vm = VersionManager::new();

                // Step 1: Install
                let install_result =
                    vm.install_version(&version_clone).unwrap_or_else(|e| InstallResult {
                        success: false,
                        stdout: String::new(),
                        stderr: String::new(),
                        error: Some(e.to_string()),
                    });
                let install_ok = install_result.success;
                let _ = tx.send(InstallStepResult::InstallComplete(install_result));

                // Step 2: Patch (only if install succeeded)
                if install_ok {
                    let patch_result = vm.run_patch().unwrap_or_else(|e| InstallResult {
                        success: false,
                        stdout: String::new(),
                        stderr: String::new(),
                        error: Some(e.to_string()),
                    });
                    let _ = tx.send(InstallStepResult::PatchComplete(patch_result));
                }
            });

            self.install_state = Some(InstallState {
                agent_type: AgentType::Claude,
                version,
                is_allowed,
                receiver: rx,
                _handle: handle,
                start_time: Instant::now(),
                current_step: InstallStep::Installing,
                install_result: None,
                patch_result: None,
            });
        }
    }

    /// Install a specific Codex version (build from source at tag)
    fn install_codex_version(&mut self) {
        if let Some(version_info) = self.versions.get(self.version_menu.selected) {
            let version = version_info.version.clone();
            let is_allowed = is_version_allowed_for(&version, AgentType::Codex);
            let is_rebuild = version_info.is_installed;

            self.selected_version = Some(version.clone());
            self.screen = Screen::VersionInstalling;

            let action = if is_rebuild { "Rebuilding" } else { "Building" };
            let warning = if !is_allowed {
                " (WARNING: not recommended)"
            } else {
                ""
            };
            self.status_message = Some(format!("{} Codex v{}{}...", action, version, warning));

            let (tx, rx) = mpsc::channel();

            let version_clone = version.clone();
            let handle = thread::spawn(move || {
                let vm = VersionManager::new();
                let result = vm.install_codex_version(&version_clone).unwrap_or_else(|e| InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    error: Some(e.to_string()),
                });
                let _ = tx.send(InstallStepResult::InstallComplete(result));
            });

            self.install_state = Some(InstallState {
                agent_type: AgentType::Codex,
                version,
                is_allowed,
                receiver: rx,
                _handle: handle,
                start_time: Instant::now(),
                current_step: InstallStep::Installing,
                install_result: None,
                patch_result: None,
            });
        }
    }

    fn handle_profiles_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.profile_menu.handle_action(action);
            }
            NavAction::Select => {
                if let Some(profile) = self.profiles.get(self.profile_menu.selected) {
                    self.selected_profile = Some(profile.clone());
                    self.app_config.current_profile = profile.name.clone();
                    let _ = self.profile_manager.save_app_config(&self.app_config);
                    self.status_message = Some(format!("Selected: {}", profile.name));
                    self.screen = Screen::Main;
                }
            }
            NavAction::Edit => {
                if let Some(profile) = self.profiles.get(self.profile_menu.selected).cloned() {
                    self.load_profile_for_editing(profile);
                    self.screen = Screen::ProfileEdit;
                }
            }
            NavAction::New => {
                let name = format!("profile-{}", self.profiles.len() + 1);
                let profile = Profile::new(&name);
                if self.profile_manager.save_profile(&profile).is_ok() {
                    self.refresh_profiles();
                    self.status_message = Some(format!("Created: {}", name));
                }
            }
            NavAction::Delete => {
                if let Some(profile) = self.profiles.get(self.profile_menu.selected) {
                    if profile.name != "default" {
                        self.screen = Screen::ConfirmDelete;
                    } else {
                        self.status_message = Some("Cannot delete default profile".to_string());
                    }
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
            }
            _ => {}
        }
    }

    fn handle_profile_edit_input(&mut self, action: NavAction, _key: KeyEvent) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.env_menu.handle_action(action);
            }
            NavAction::Select | NavAction::Edit => {
                let selected = self.env_menu.selected;
                if selected < self.env_vars_list.len() {
                    // Edit existing env var
                    let (key, value) = &self.env_vars_list[selected];
                    self.key_input = TextInput::new().with_value(key);
                    self.value_input = TextInput::new().with_value(value);
                    if is_sensitive_key(key) {
                        self.value_input.hidden = true;
                    }
                    self.editing_env_index = Some(selected);
                    self.edit_field = EditField::EnvKey;
                    self.screen = Screen::EnvVarEdit;
                } else {
                    // Add new env var
                    self.key_input = TextInput::new().with_placeholder("VARIABLE_NAME");
                    self.value_input = TextInput::new().with_placeholder("value");
                    self.editing_env_index = None;
                    self.edit_field = EditField::EnvKey;
                    self.screen = Screen::EnvVarEdit;
                }
            }
            NavAction::New => {
                self.key_input = TextInput::new().with_placeholder("VARIABLE_NAME");
                self.value_input = TextInput::new().with_placeholder("value");
                self.editing_env_index = None;
                self.edit_field = EditField::EnvKey;
                self.screen = Screen::EnvVarEdit;
            }
            NavAction::Delete => {
                let selected = self.env_menu.selected;
                if selected < self.env_vars_list.len() {
                    let key = self.env_vars_list[selected].0.clone();
                    self.env_vars_list.remove(selected);
                    self.env_menu.set_items_count(self.env_vars_list.len() + 1);
                    let _ = self.save_editing_profile();
                    self.status_message = Some(format!("Deleted: {}", key));
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.editing_profile = None;
                self.screen = Screen::Profiles;
            }
            _ => {}
        }
    }

    fn handle_env_var_edit_input(&mut self, action: NavAction, _key: KeyEvent) {
        match action {
            NavAction::Back | NavAction::Quit => {
                self.edit_field = EditField::None;
                self.screen = Screen::ProfileEdit;
            }
            _ => {}
        }
    }

    fn handle_settings_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.settings_menu.handle_action(action);
            }
            NavAction::Select | NavAction::Edit => {
                match self.settings_menu.selected {
                    0 => {
                        // Edit entry point
                        self.key_input = TextInput::new().with_value(&self.app_config.claude_path);
                        self.edit_field = EditField::ClaudePath;
                    }
                    1 => {
                        // Edit arguments
                        self.key_input = TextInput::new().with_value(&self.app_config.claude_args.join(" "));
                        self.edit_field = EditField::ClaudeArgs;
                    }
                    2 => {
                        // Edit stop prompt - open directly in $EDITOR
                        let default_prompt = self.get_default_stop_prompt();
                        let current = self.app_config.stop_prompt.clone().unwrap_or(default_prompt);
                        self.pending_external_edit = Some(current);
                    }
                    3 => {
                        // Reset settings to defaults
                        self.app_config = AppConfig::default();
                        if let Err(e) = self.profile_manager.save_app_config(&self.app_config) {
                            self.status_message = Some(format!("Failed to reset: {}", e));
                        } else {
                            self.status_message = Some("Settings reset to defaults".to_string());
                        }
                    }
                    _ => {}
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
            }
            _ => {}
        }
    }

    fn handle_theme_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.theme_menu.handle_action(action);
            }
            NavAction::Select => {
                let presets = ThemePreset::all();
                if let Some(preset) = presets.get(self.theme_menu.selected) {
                    // Selected a preset
                    self.theme_color = ThemeColor::Preset(*preset);
                    self.app_config.theme = self.theme_color.to_config();
                    let _ = self.profile_manager.save_app_config(&self.app_config);
                    self.status_message = Some(format!("Theme: {}", preset.display_name()));
                    self.trigger_screen_animation(false, Screen::Main);
                    self.pending_screen = Some(Screen::Main);
                } else {
                    // "Custom" entry (last item, past all presets)
                    // Pre-fill with current custom hex or empty
                    let initial = if let ThemeColor::Custom(r, g, b) = self.theme_color {
                        format!("{:02X}{:02X}{:02X}", r, g, b)
                    } else {
                        String::new()
                    };
                    self.key_input.clear();
                    self.key_input.value = initial;
                    self.key_input.cursor = self.key_input.value.len();
                    self.key_input.placeholder = "RRGGBB".to_string();
                    self.edit_field = EditField::ThemeHex;
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
            }
            _ => {}
        }
    }

    /// Get the current accent color based on theme
    fn accent_color(&self) -> Color {
        let (r, g, b) = self.theme_color.accent_rgb();
        Color::Rgb(r, g, b)
    }

    fn handle_help_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up => {
                self.help_scroll_offset = self.help_scroll_offset.saturating_sub(1);
            }
            NavAction::Down => {
                self.help_scroll_offset += 1;
            }
            NavAction::Back | NavAction::Quit | NavAction::Select => {
                self.help_scroll_offset = 0;
                let return_to = self.help_return_screen.take().unwrap_or(Screen::Main);
                // Only animate when returning to Main (mascot changes sides)
                if return_to == Screen::Main {
                    self.trigger_screen_animation(false, return_to);
                }
                self.pending_screen = Some(return_to);
                // If no animation was triggered, apply immediately
                if self.art_animation.is_none() {
                    self.screen = return_to;
                    self.refresh_screen_data();
                }
            }
            _ => {}
        }
    }

    fn handle_confirm_delete_input(&mut self, action: NavAction) {
        match action {
            NavAction::Select => {
                // Confirm delete
                if let Some(profile) = self.profiles.get(self.profile_menu.selected) {
                    let name = profile.name.clone();
                    if self.profile_manager.delete_profile(&name).is_ok() {
                        self.refresh_profiles();
                        self.status_message = Some(format!("Deleted: {}", name));
                    }
                }
                self.screen = Screen::Profiles;
            }
            NavAction::Back | NavAction::Quit => {
                self.screen = Screen::Profiles;
            }
            _ => {}
        }
    }

    fn handle_updating_input(&mut self, action: NavAction) -> io::Result<Option<AppAction>> {
        match action {
            NavAction::Select => {
                // Find the repo directory
                let exe_path = std::env::current_exe().ok();
                let repo_dir = exe_path
                    .as_ref()
                    .and_then(|p| p.parent()) // target/release
                    .and_then(|p| p.parent()) // target
                    .and_then(|p| p.parent()) // repo root
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));

                return Ok(Some(AppAction::Update(UpdateRequest { repo_dir })));
            }
            NavAction::Back | NavAction::Quit => {
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
            }
            _ => {}
        }
        Ok(None)
    }

    /// Calculate the minimum content width needed for the current screen
    fn content_width(&self) -> u16 {
        self.content_width_for_screen(self.screen)
    }

    /// Calculate the minimum content width needed for a specific screen
    fn content_width_for_screen(&self, screen: Screen) -> u16 {
        match screen {
            Screen::Main => {
                // Calculate based on actual menu content
                let menu_items = [
                    ("Start Session", "Launch Claude with selected profile".to_string()),
                    ("Profiles", "Manage environment profiles".to_string()),
                    ("Agent Versions", "Manage installed agent CLIs".to_string()),
                    ("Settings", "Configure launcher settings".to_string()),
                    ("Theme", "Customize mascot and UI colors".to_string()),
                    ("Update TUI", "Pull latest and recompile".to_string()),
                    ("Help", "Keyboard shortcuts and tips".to_string()),
                    ("Quit", "Exit the launcher".to_string()),
                ];
                let max_name = menu_items.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
                let max_desc = menu_items.iter().map(|(_, d)| d.len()).max().unwrap_or(0);
                // "> " prefix (2) + name, or "    " prefix (4) + desc
                let name_width = 2 + max_name;
                let desc_width = 4 + max_desc;
                (name_width.max(desc_width) + 2) as u16
            }
            Screen::Profiles | Screen::ConfirmDelete => {
                // Based on profile names + " *" marker + "    X env vars"
                let max_name = self.profiles.iter().map(|p| p.name.len()).max().unwrap_or(10);
                let name_width = 2 + max_name + 2; // "> " + name + " *"
                let desc_width = 4 + 12; // "    X env vars"
                (name_width.max(desc_width) + 2) as u16
            }
            Screen::Settings => {
                // Settings items
                let items = ["Claude Entry Point", "Arguments", "Auto-stop Prompt", "Reset to Defaults"];
                let max_len = items.iter().map(|s| s.len()).max().unwrap_or(20);
                (2 + max_len + 2) as u16
            }
            Screen::Theme => {
                // Theme list with color swatches
                35
            }
            Screen::Help => {
                // Help screen has fixed text
                40
            }
            Screen::Updating => {
                // Update status messages
                35
            }
            Screen::VersionManagement | Screen::VersionInstalling => {
                // Version list: "1.0.xxx [installed]"
                45
            }
            Screen::ProfileEdit | Screen::EnvVarEdit => {
                // Profile editing needs more space for env var keys/values
                50
            }
        }
    }

    /// Render the UI
    pub fn render(&mut self, frame: &mut Frame) {
        // Main layout: content area + status bar at bottom
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(frame.area());

        // Determine layout for current screen:
        // - Main screen uses art_layout setting directly
        // - All other screens use the opposite layout
        let use_art_left = match self.screen {
            Screen::Main => self.art_layout == ArtLayout::ArtLeft,
            _ => self.art_layout == ArtLayout::ArtRight, // Flip for non-main screens
        };

        let content_width = self.content_width();

        // Check if animation is in progress
        if let Some(ref animation) = self.art_animation {
            // During animation: Show the FULL 106-char merged sprite sliding across
            // Sprite starts at its render position, becomes fully visible in the middle,
            // and ends at its destination render position with clipping at art boundaries
            let figure_width = ART_WIDTH * 2; // 106 chars for full sprite

            // Calculate figure position based on animation progress
            let figure_x = animation.figure_x();

            // Define clipping boundaries (the "invisible borders"):
            // - Left boundary: always at x=0
            // - Right boundary: right edge of the right-side art area
            // The visible area during animation is the union of both art areas
            let right_boundary = animation.start_art_x.max(animation.end_art_x) + ART_WIDTH;

            // Calculate visible portion with clipping at both boundaries
            let (render_x, scroll_x, render_width) = {
                // Left clipping: if figure starts before x=0
                let left_clip = if figure_x < 0 { (-figure_x) as u16 } else { 0 };
                let visible_start = figure_x.max(0) as u16;

                // Right clipping: figure can't extend beyond right_boundary
                let figure_right = (figure_x + figure_width as i32) as u16;
                let visible_end = figure_right.min(right_boundary);

                // Calculate final render parameters
                let width = visible_end.saturating_sub(visible_start);
                (visible_start, left_clip, width)
            };

            // Clamp figure_rect to not exceed the available frame area
            let frame_right_edge = main_chunks[0].x + main_chunks[0].width;
            let figure_start_x = main_chunks[0].x + render_x;
            let figure_end_x = (figure_start_x + render_width).min(frame_right_edge);
            let clamped_width = figure_end_x.saturating_sub(figure_start_x);

            let figure_rect = Rect {
                x: figure_start_x,
                y: main_chunks[0].y,
                width: clamped_width,
                height: main_chunks[0].height,
            };

            let max_lines = figure_rect.height as usize;
            let shift = self.theme_color.theme_shift();
            let art_lines: Vec<Line> = if !shift.is_identity() {
                mascots::unleashed_claude_full_ratatui_themed(max_lines, shift)
            } else {
                mascots::unleashed_claude_full_ratatui(max_lines)
            };
            let art_widget = Paragraph::new(art_lines).scroll((0, scroll_x));
            frame.render_widget(art_widget, figure_rect);
        } else {
            // Not animating: render the appropriate half
            if use_art_left {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(ART_WIDTH),
                        Constraint::Length(content_width),
                        Constraint::Min(0),
                    ])
                    .split(main_chunks[0]);

                self.render_art_sidebar(frame, content_chunks[0]); // Right-facing on left
                self.render_screen_content(frame, content_chunks[1]);
            } else {
                let content_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([
                        Constraint::Length(content_width),
                        Constraint::Length(ART_WIDTH),
                        Constraint::Min(0),
                    ])
                    .split(main_chunks[0]);

                self.render_art_sidebar_left(frame, content_chunks[1]); // Left-facing on right
                self.render_screen_content(frame, content_chunks[0]);
            }
        }

        self.render_status_bar(frame, main_chunks[1]);
    }

    /// Render the content for the current screen
    fn render_screen_content(&mut self, frame: &mut Frame, area: Rect) {
        match self.screen {
            Screen::Main => self.render_main_menu(frame, area),
            Screen::Profiles => self.render_profiles(frame, area),
            Screen::ProfileEdit => self.render_profile_edit(frame, area),
            Screen::EnvVarEdit => {
                self.render_profile_edit(frame, area);
                self.render_env_var_dialog(frame, frame.area());
            }
            Screen::Settings => self.render_settings(frame, area),
            Screen::Theme => self.render_theme(frame, area),
            Screen::Help => self.render_help(frame, area),
            Screen::ConfirmDelete => {
                self.render_profiles(frame, area);
                self.render_confirm_delete_dialog(frame, frame.area());
            }
            Screen::VersionManagement => self.render_version_management(frame, area),
            Screen::VersionInstalling => self.render_version_installing(frame, area),
            Screen::Updating => self.render_updating(frame, area),
        }
    }

    fn render_art_sidebar(&self, frame: &mut Frame, area: Rect) {
        // Render muscular Claude ANSI art (right-facing)
        // Lava lamp mode is an easter egg triggered by Konami code (idea by cac taurus)
        let max_lines = area.height as usize;
        let shift = self.theme_color.theme_shift();
        let art_lines: Vec<Line> = if self.lava_mode {
            mascots::unleashed_claude_ratatui_lava(max_lines, self.animation_frame)
        } else if !shift.is_identity() {
            mascots::unleashed_claude_ratatui_themed(max_lines, shift)
        } else {
            mascots::unleashed_claude_ratatui(max_lines)
        };
        let art_widget = Paragraph::new(art_lines);
        frame.render_widget(art_widget, area);
    }

    fn render_art_sidebar_left(&self, frame: &mut Frame, area: Rect) {
        // Render muscular Claude ANSI art (left-facing)
        // Lava lamp mode is an easter egg triggered by Konami code (idea by cac taurus)
        let max_lines = area.height as usize;
        let shift = self.theme_color.theme_shift();
        let art_lines: Vec<Line> = if self.lava_mode {
            mascots::unleashed_claude_left_ratatui_lava(max_lines, self.animation_frame)
        } else if !shift.is_identity() {
            mascots::unleashed_claude_left_ratatui_themed(max_lines, shift)
        } else {
            mascots::unleashed_claude_left_ratatui(max_lines)
        };
        let art_widget = Paragraph::new(art_lines);
        frame.render_widget(art_widget, area);
    }

    fn render_main_menu(&mut self, frame: &mut Frame, area: Rect) {
        // Split area for title and menu
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(5), // Title area
                Constraint::Min(10),   // Menu area
            ])
            .split(area);

        // Render title
        let title_text = vec![
            Line::from(Span::styled(
                "Agent Unleashed",
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("Profile: {}", self.selected_profile.as_ref().map(|p| p.name.as_str()).unwrap_or("none")),
                Style::default().fg(Color::Yellow),
            )),
        ];
        let title = Paragraph::new(title_text);
        frame.render_widget(title, chunks[0]);

        let menu_items = [
            ("Start Session", "Launch Claude with selected profile".to_string()),
            ("Profiles", "Manage environment profiles".to_string()),
            ("Agent Versions", "Manage installed agent CLIs".to_string()),
            ("Settings", "Configure launcher settings".to_string()),
            ("Theme", "Customize mascot and UI colors".to_string()),
            ("Update TUI", "Pull latest and recompile".to_string()),
            ("Help", "Keyboard shortcuts and tips".to_string()),
            ("Quit", "Exit the launcher".to_string()),
        ];

        // Each menu item takes 2 lines, calculate visible count
        // Area height minus 2 for borders, divided by 2 for lines per item
        let menu_area = chunks[1];
        let visible_items = (menu_area.height.saturating_sub(2) / 2) as usize;

        // Ensure selected item is visible
        self.main_menu.ensure_visible(visible_items);
        let scroll_offset = self.main_menu.scroll_offset;

        let items: Vec<ListItem> = menu_items
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_items)
            .map(|(i, (name, desc))| {
                let style = if i == self.main_menu.selected {
                    Style::default()
                        .fg(self.accent_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if i == self.main_menu.selected { "> " } else { "  " };
                ListItem::new(vec![
                    Line::from(Span::styled(format!("{}{}", prefix, name), style)),
                    Line::from(Span::styled(format!("    {}", desc), Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect();

        // Show scroll indicator if needed
        let _scroll_hint = if menu_items.len() > visible_items {
            format!(" [{}/{}]", scroll_offset + 1, menu_items.len().saturating_sub(visible_items) + 1)
        } else {
            String::new()
        };

        let menu = List::new(items);
        frame.render_widget(menu, menu_area);
    }

    fn render_profiles(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self
            .profiles
            .iter()
            .enumerate()
            .map(|(i, profile)| {
                let is_current = self.selected_profile.as_ref().is_some_and(|p| p.name == profile.name);
                let style = if i == self.profile_menu.selected {
                    Style::default()
                        .fg(self.accent_color())
                        .add_modifier(Modifier::BOLD)
                } else if is_current {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };
                let prefix = if i == self.profile_menu.selected { "> " } else { "  " };
                let current_marker = if is_current { " *" } else { "" };
                let env_count = profile.env.len();
                ListItem::new(vec![
                    Line::from(Span::styled(format!("{}{}{}", prefix, profile.name, current_marker), style)),
                    Line::from(Span::styled(format!("    {} env vars", env_count), Style::default().fg(Color::DarkGray))),
                ])
            })
            .collect();

        let menu = List::new(items);
        frame.render_widget(menu, area);
    }

    fn render_profile_edit(&self, frame: &mut Frame, area: Rect) {
        let _profile = match &self.editing_profile {
            Some(p) => p,
            None => return,
        };

        let mut items: Vec<ListItem> = self
            .env_vars_list
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                let style = if i == self.env_menu.selected {
                    Style::default().fg(self.accent_color()).add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if i == self.env_menu.selected { "> " } else { "  " };

                let display_value = if is_sensitive_key(key) {
                    censor_sensitive(value, 7, 4)
                } else {
                    value.clone()
                };

                ListItem::new(Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled(key, style),
                    Span::styled("=", Style::default().fg(Color::DarkGray)),
                    Span::styled(display_value, Style::default().fg(Color::Cyan)),
                ]))
            })
            .collect();

        // Add "Add new" option
        let add_style = if self.env_menu.selected == self.env_vars_list.len() {
            Style::default().fg(Color::Green).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let add_prefix = if self.env_menu.selected == self.env_vars_list.len() { "> " } else { "  " };
        items.push(ListItem::new(Line::from(Span::styled(
            format!("{}+ Add new variable", add_prefix),
            add_style,
        ))));

        let menu = List::new(items);
        frame.render_widget(menu, area);
    }

    fn render_env_var_dialog(&self, frame: &mut Frame, area: Rect) {
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = 9;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let _title = if self.editing_env_index.is_some() { " Edit Variable " } else { " New Variable " };

        let key_style = if self.edit_field == EditField::EnvKey {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let value_style = if self.edit_field == EditField::EnvValue {
            Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let key_display = if self.key_input.is_empty() {
            Span::styled(&self.key_input.placeholder, Style::default().fg(Color::DarkGray))
        } else {
            Span::styled(&self.key_input.value, key_style)
        };

        let value_display = if self.value_input.is_empty() {
            Span::styled(&self.value_input.placeholder, Style::default().fg(Color::DarkGray))
        } else if self.value_input.hidden && self.edit_field != EditField::EnvValue {
            // Show censored when not actively editing
            Span::styled(censor_sensitive(&self.value_input.value, 7, 4), value_style)
        } else if self.value_input.hidden {
            // Show asterisks when actively editing hidden field
            Span::styled("*".repeat(self.value_input.value.len()), value_style)
        } else {
            Span::styled(&self.value_input.value, value_style)
        };

        let cursor_indicator = "█";

        let lines = vec![
            Line::from(""),
            Line::from(vec![
                Span::styled("  Key:   ", Style::default()),
                key_display,
                if self.edit_field == EditField::EnvKey {
                    Span::styled(cursor_indicator, Style::default().fg(Color::Yellow))
                } else {
                    Span::raw("")
                },
            ]),
            Line::from(""),
            Line::from(vec![
                Span::styled("  Value: ", Style::default()),
                value_display,
                if self.edit_field == EditField::EnvValue {
                    Span::styled(cursor_indicator, Style::default().fg(Color::Yellow))
                } else {
                    Span::raw("")
                },
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "  [Tab=switch field] [Enter=save] [Esc=cancel]",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let dialog = Paragraph::new(lines)
            .block(Block::default().style(
                Style::default().bg(Color::Black),
            ))
            .wrap(Wrap { trim: false });

        frame.render_widget(dialog, dialog_area);
    }

    fn render_confirm_delete_dialog(&self, frame: &mut Frame, area: Rect) {
        let dialog_width = 40.min(area.width.saturating_sub(4));
        let dialog_height = 7;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let profile_name = self
            .profiles
            .get(self.profile_menu.selected)
            .map(|p| p.name.as_str())
            .unwrap_or("?");

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  Delete profile '{}'?", profile_name),
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  [Enter=confirm] [Esc=cancel]",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let dialog = Paragraph::new(lines)
            .block(
                Block::default()
                    .style(Style::default().bg(Color::Black).fg(Color::Red)),
            );

        frame.render_widget(dialog, dialog_area);
    }

    fn render_settings(&mut self, frame: &mut Frame, area: Rect) {
        let args_str = self.app_config.claude_args.join(" ");
        let stop_prompt_display = self.app_config.stop_prompt
            .clone()
            .unwrap_or_else(|| "(default)".to_string());
        let settings: Vec<(&str, String, &str)> = vec![
            ("Entry Point", self.app_config.claude_path.clone(), "Command to launch (e.g., claude)"),
            ("Arguments", args_str, "Additional command-line arguments"),
            ("Stop Prompt", stop_prompt_display, "Opens in $EDITOR (empty = default)"),
            ("Reset Settings", "".to_string(), "Reset all settings to defaults"),
        ];

        let cursor_indicator = "█";

        // Calculate viewport width based on area (leave room for label and padding)
        let viewport_width = area.width.saturating_sub(20) as usize; // Account for prefix, label, padding
        self.key_input.set_viewport_width(viewport_width.max(30));

        let items: Vec<ListItem> = settings
            .iter()
            .enumerate()
            .map(|(i, (name, value, desc))| {
                let is_selected = i == self.settings_menu.selected;
                let is_editing = is_selected && match i {
                    0 => self.edit_field == EditField::ClaudePath,
                    1 => self.edit_field == EditField::ClaudeArgs,
                    2 => self.edit_field == EditField::StopPrompt,
                    _ => false,
                };

                let style = if is_selected {
                    Style::default()
                        .fg(self.accent_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "> " } else { "  " };

                // Use viewport-aware display when editing
                let display_value = if is_editing {
                    let visible = self.key_input.visible_value();
                    let left = if self.key_input.has_left_overflow() { "..." } else { "" };
                    let right = if self.key_input.has_right_overflow() { "..." } else { "" };
                    format!("{}{}{}{}", left, visible, cursor_indicator, right)
                } else {
                    // Truncate non-editing values if too long
                    let max_display = viewport_width.saturating_sub(3);
                    if value.len() > max_display {
                        format!("{}...", &value[..max_display])
                    } else {
                        value.to_string()
                    }
                };

                let value_style = if is_editing {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };

                // Show value on separate line if it's long (> 30 chars) for better visibility
                // But use the full display_value length for decision (includes indicators)
                let effective_len = display_value.len();
                let value_on_new_line = effective_len > 30;

                if value_on_new_line {
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(prefix, style),
                            Span::styled(*name, style),
                            Span::styled(":", Style::default().fg(Color::DarkGray)),
                        ]),
                        Line::from(vec![
                            Span::raw("    "),
                            Span::styled(display_value, value_style),
                        ]),
                        Line::from(Span::styled(format!("    {}", desc), Style::default().fg(Color::DarkGray))),
                    ])
                } else {
                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(prefix, style),
                            Span::styled(*name, style),
                            Span::styled(": ", Style::default().fg(Color::DarkGray)),
                            Span::styled(display_value, value_style),
                        ]),
                        Line::from(Span::styled(format!("    {}", desc), Style::default().fg(Color::DarkGray))),
                    ])
                }
            })
            .collect();

        let mut menu_items = items;

        // Add config file info at the bottom
        menu_items.push(ListItem::new(vec![
            Line::from(""),
            Line::from(Span::styled("Config file:", Style::default().fg(Color::DarkGray))),
            Line::from(Span::styled(
                format!("  {}/config.toml", self.profile_manager.config_dir().display()),
                Style::default().fg(Color::DarkGray),
            )),
        ]));

        let _hint = if self.edit_field != EditField::None {
            " [Enter=save Esc=cancel] "
        } else {
            " Settings [Enter=edit Esc=back] "
        };

        let menu = List::new(menu_items);
        frame.render_widget(menu, area);
    }

    fn render_theme(&self, frame: &mut Frame, area: Rect) {
        let presets = ThemePreset::all();
        let custom_index = presets.len();

        let mut items: Vec<ListItem> = presets
            .iter()
            .enumerate()
            .map(|(i, preset)| {
                let is_selected = i == self.theme_menu.selected;
                let is_active = self.theme_color.is_preset(*preset);
                let (r, g, b) = preset.accent_rgb();
                let preview_color = Color::Rgb(r, g, b);

                let style = if is_selected {
                    Style::default()
                        .fg(preview_color)
                        .add_modifier(Modifier::BOLD)
                } else if is_active {
                    Style::default().fg(preview_color)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "> " } else { "  " };
                let active_marker = if is_active { " *" } else { "" };

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled("\u{2588}\u{2588}", Style::default().fg(preview_color)),
                        Span::styled(format!(" {}{}", preset.display_name(), active_marker), style),
                    ]),
                ])
            })
            .collect();

        // "Custom" entry
        let custom_selected = self.theme_menu.selected == custom_index;
        let custom_active = self.theme_color.is_custom();
        let custom_accent = if custom_active {
            let (r, g, b) = self.theme_color.accent_rgb();
            Color::Rgb(r, g, b)
        } else {
            Color::White
        };

        let custom_style = if custom_selected {
            Style::default().fg(custom_accent).add_modifier(Modifier::BOLD)
        } else if custom_active {
            Style::default().fg(custom_accent)
        } else {
            Style::default()
        };

        let custom_prefix = if custom_selected { "> " } else { "  " };

        if self.edit_field == EditField::ThemeHex {
            // Show hex input inline
            let cursor = "\u{2588}";
            items.push(ListItem::new(vec![
                Line::from(vec![
                    Span::styled(custom_prefix, custom_style),
                    Span::styled("# ", custom_style),
                    Span::styled(&self.key_input.value, Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                    Span::styled(cursor, Style::default().fg(Color::Yellow)),
                ]),
                Line::from(Span::styled(
                    "    Enter hex color (RRGGBB), Esc to cancel",
                    Style::default().fg(Color::DarkGray),
                )),
            ]));
        } else {
            let active_marker = if custom_active {
                format!(" {} *", self.theme_color.display_name())
            } else {
                String::new()
            };
            let swatch = if custom_active {
                vec![
                    Span::styled(custom_prefix, custom_style),
                    Span::styled("\u{2588}\u{2588}", Style::default().fg(custom_accent)),
                    Span::styled(format!(" Custom...{}", active_marker), custom_style),
                ]
            } else {
                vec![
                    Span::styled(custom_prefix, custom_style),
                    Span::styled("   Custom...", custom_style),
                ]
            };
            items.push(ListItem::new(vec![Line::from(swatch)]));
        }

        let menu = List::new(items);
        frame.render_widget(menu, area);
    }

    fn render_help(&mut self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled("Keyboard Shortcuts", Style::default().add_modifier(Modifier::BOLD))),
            Line::from(""),
            Line::from("  j/↓      Move down"),
            Line::from("  k/↑      Move up"),
            Line::from("  Enter    Select/Edit"),
            Line::from("  e        Edit item"),
            Line::from("  n        New item"),
            Line::from("  d        Delete item"),
            Line::from("  Esc/q    Go back/Quit"),
            Line::from("  ?        This help"),
            Line::from(""),
            Line::from(Span::styled("In edit dialog:", Style::default().add_modifier(Modifier::BOLD))),
            Line::from("  Tab      Switch field"),
            Line::from("  Enter    Save"),
            Line::from("  Esc      Cancel"),
        ];

        let total_lines = lines.len() as u16;
        let visible_height = area.height;
        let max_scroll = total_lines.saturating_sub(visible_height);

        // Clamp scroll offset
        if self.help_scroll_offset > max_scroll {
            self.help_scroll_offset = max_scroll;
        }

        let content = Paragraph::new(lines)
            .scroll((self.help_scroll_offset, 0))
            .wrap(Wrap { trim: false });
        frame.render_widget(content, area);

        // Show scroll indicator when content overflows
        if max_scroll > 0 {
            let indicator = format!("[{}/{}]", self.help_scroll_offset + 1, max_scroll + 1);
            let indicator_width = indicator.len() as u16;
            let indicator_x = area.x + area.width.saturating_sub(indicator_width);
            let indicator_y = area.y + area.height.saturating_sub(1);
            let indicator_area = Rect::new(indicator_x, indicator_y, indicator_width, 1);
            let indicator_widget = Paragraph::new(Line::from(Span::styled(
                indicator,
                Style::default().fg(Color::DarkGray),
            )));
            frame.render_widget(indicator_widget, indicator_area);
        }
    }

    fn render_updating(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "Updating TUI...",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("This will:"),
            Line::from("  1. Pull latest changes from git"),
            Line::from("  2. Recompile with cargo build --release"),
            Line::from("  3. Replace current binary and restart"),
            Line::from(""),
            Line::from(Span::styled(
                "Press Enter to continue, Esc to cancel",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: false });
        frame.render_widget(content, area);
    }

    fn render_version_management(&mut self, frame: &mut Frame, area: Rect) {
        // Split: agent tab bar + version list
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Agent tab bar
                Constraint::Min(5),   // Version list
            ])
            .split(area);

        self.render_agent_tabs(frame, chunks[0]);
        self.render_version_list(frame, chunks[1]);
    }

    /// Render the agent CLI tab bar (Claude Code | Codex)
    fn render_agent_tabs(&self, frame: &mut Frame, area: Rect) {
        let agents = AgentType::all();
        let mut spans = Vec::new();

        spans.push(Span::raw("  "));

        for (i, agent) in agents.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            }

            let style = if *agent == self.version_agent {
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let installed = self
                .cached_agent_versions
                .get(agent)
                .and_then(|v| v.clone())
                .map(|v| format!(" (v{})", v))
                .unwrap_or_default();

            spans.push(Span::styled(
                format!("{}{}", agent.display_name(), installed),
                style,
            ));
        }

        spans.push(Span::styled(
            "  [←/→: switch agent]",
            Style::default().fg(Color::DarkGray),
        ));

        let tabs = Paragraph::new(Line::from(spans));
        frame.render_widget(tabs, area);
    }

    /// Render the version list for the currently selected agent
    fn render_version_list(&mut self, frame: &mut Frame, area: Rect) {
        let is_claude = self.version_agent == AgentType::Claude;

        // Calculate visible height (area minus legend lines)
        let legend_lines: u16 = 3;
        let visible_height = area.height.saturating_sub(legend_lines) as usize;

        // Ensure selected item is visible
        self.version_menu.ensure_visible(visible_height);
        let scroll_offset = self.version_menu.scroll_offset;

        // Build items with scroll awareness
        let items: Vec<ListItem> = self
            .versions
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_height)
            .map(|(i, version_info)| {
                let is_selected = i == self.version_menu.selected;

                {
                    // Show whitelist/blacklist markers for all agents
                    let agent = self.version_agent;
                    let is_allowed = is_version_allowed_for(&version_info.version, agent);

                    let style = if is_selected {
                        if !is_allowed {
                            Style::default()
                                .fg(Color::Yellow)
                                .add_modifier(Modifier::BOLD | Modifier::CROSSED_OUT)
                        } else {
                            Style::default()
                                .fg(self.accent_color())
                                .add_modifier(Modifier::BOLD)
                        }
                    } else if !is_allowed {
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::CROSSED_OUT)
                    } else if version_info.is_installed {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default()
                    };

                    let prefix = if is_selected { "> " } else { "  " };
                    let installed_marker =
                        if version_info.is_installed { " [installed]" } else { "" };
                    let patch_marker = if version_info.has_patch { " *" } else { "" };
                    let whitelist_marker = if version_info.is_whitelisted { " ✓" } else { "" };
                    let blacklist_marker = if version_info.is_blacklisted { " ⛔" } else { "" };

                    ListItem::new(vec![Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(format!("v{}", version_info.version), style),
                        Span::styled(installed_marker, Style::default().fg(Color::Green)),
                        Span::styled(patch_marker, Style::default().fg(Color::Yellow)),
                        Span::styled(whitelist_marker, Style::default().fg(Color::Green)),
                        Span::styled(blacklist_marker, Style::default().fg(Color::Red)),
                    ])])
                }
            })
            .collect();

        let mut list_items = items;

        // Add legend at the bottom
        if !self.versions.is_empty() {
            list_items.push(ListItem::new(Line::from("")));
            let legend = if is_claude {
                "  * = has auto-mode patch  ✓ = whitelisted  ⛔ = blacklisted"
            } else {
                "  ✓ = whitelisted  ⛔ = blacklisted"
            };
            list_items.push(ListItem::new(Line::from(Span::styled(
                legend,
                Style::default().fg(Color::DarkGray),
            ))));
            let filter_mode = get_version_filter_mode_for(self.version_agent);
            let mode_hint = match filter_mode {
                VersionFilterMode::Whitelist => {
                    "  Mode: whitelist (only ✓ versions allowed)"
                }
                VersionFilterMode::Blacklist => {
                    "  Mode: blacklist (all except ⛔ allowed)"
                }
            };
            list_items.push(ListItem::new(Line::from(Span::styled(
                mode_hint,
                Style::default().fg(Color::DarkGray),
            ))));
        }

        let menu = List::new(list_items);
        frame.render_widget(menu, area);
    }

    fn render_version_installing(&self, frame: &mut Frame, area: Rect) {
        let version = self.selected_version.as_deref().unwrap_or("?");
        let spinner = self.spinner_frame();
        let agent_name = self
            .install_state
            .as_ref()
            .map(|s| s.agent_type.display_name())
            .unwrap_or(self.version_agent.display_name());

        // Determine current step info based on agent type
        let (step_text, command_text) = if let Some(ref state) = self.install_state {
            let install_cmd = match state.agent_type {
                AgentType::Claude => "npm install -g @anthropic-ai/claude-code@...".to_string(),
                AgentType::Codex => "cargo build --release -p codex-cli".to_string(),
            };
            match state.current_step {
                InstallStep::Installing => (
                    format!("{} Installing {} {}...", spinner, agent_name, version),
                    install_cmd,
                ),
                InstallStep::Patching => (
                    format!("{} Applying patches for {}...", spinner, version),
                    "patch-claude.sh".to_string(),
                ),
                InstallStep::Done => (
                    format!("✓ {} {} installation complete", agent_name, version),
                    "Done!".to_string(),
                ),
            }
        } else {
            (
                format!("{} Installing {} {}...", spinner, agent_name, version),
                "...".to_string(),
            )
        };

        // Calculate elapsed time
        let elapsed = self
            .install_state
            .as_ref()
            .map(|s| s.start_time.elapsed().as_secs())
            .unwrap_or(0);
        let elapsed_text = if elapsed > 0 {
            format!("  Elapsed: {}s", elapsed)
        } else {
            String::new()
        };

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", step_text),
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  This may take a moment."),
            Line::from(Span::styled(elapsed_text, Style::default().fg(Color::DarkGray))),
            Line::from(""),
            Line::from(Span::styled(
                format!("  Running: {}", command_text),
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: false });
        frame.render_widget(content, area);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status = self.status_message.as_deref().unwrap_or("Press ? for help");
        let config_hint = format!("Config: {}", self.profile_manager.config_dir().display());

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(20), Constraint::Length(config_hint.len() as u16 + 2)])
            .split(area);

        let status_line = Paragraph::new(Line::from(Span::styled(
            format!(" {}", status),
            Style::default().fg(Color::DarkGray),
        )))
        .block(Block::default().borders(Borders::TOP));
        frame.render_widget(status_line, chunks[0]);

        let config_line = Paragraph::new(Line::from(Span::styled(
            config_hint,
            Style::default().fg(Color::DarkGray),
        )))
        .block(Block::default().borders(Borders::TOP));
        frame.render_widget(config_line, chunks[1]);
    }
}

/// Actions that can be returned from the app
#[derive(Debug, Clone)]
pub enum AppAction {
    Launch(LaunchRequest),
    Update(UpdateRequest),
}

/// Request to launch Claude with a specific profile
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub profile: Profile,
    pub claude_path: String,
    pub claude_args: Vec<String>,
}

/// Request to update the TUI
#[derive(Debug, Clone)]
pub struct UpdateRequest {
    pub repo_dir: PathBuf,
}

impl LaunchRequest {
    pub fn execute(&self) -> io::Result<std::process::ExitStatus> {
        use std::os::unix::process::CommandExt;

        let mut cmd = Command::new(&self.claude_path);

        for (key, value) in &self.profile.env {
            cmd.env(key, value);
        }

        let wrapper_pid = std::process::id();
        cmd.env("CLAUDE_WRAPPER_PID", wrapper_pid.to_string());

        // Only override arg0 for direct claude invocations.
        // When launching cug/cu, we must preserve argv[0] so the binary
        // can detect it was invoked as "cug" and run the launcher mode.
        let cmd_name = std::path::Path::new(&self.claude_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        if cmd_name == "claude" {
            // Set process name to include wrapper PID for identification
            // Format: "claude:<pid>" - allows correlating with conversation later
            cmd.arg0(format!("claude:{}", wrapper_pid));
        }
        // For cug/cu, let the binary see its natural argv[0]

        cmd.args(&self.claude_args);
        cmd.status()
    }
}

impl UpdateRequest {
    /// Execute the update: git pull, cargo build, replace binary and re-exec
    pub fn execute(&self) -> io::Result<()> {
        use std::os::unix::process::CommandExt;

        let tui_dir = self.repo_dir.clone();

        println!("\n=== Updating Agent Unleashed TUI ===\n");

        // Step 1: Git pull
        println!("Pulling latest changes...");
        let git_status = Command::new("git")
            .arg("pull")
            .current_dir(&self.repo_dir)
            .status()?;

        if !git_status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "git pull failed",
            ));
        }

        // Step 2: Cargo build --release
        println!("\nRecompiling...");
        let build_status = Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(&tui_dir)
            .status()?;

        if !build_status.success() {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                "cargo build failed",
            ));
        }

        // Step 3: Re-exec the new binary
        println!("\nRestarting with new binary...\n");
        let new_binary = tui_dir.join("target/release/unleashed-tui");

        let err = Command::new(&new_binary).exec();
        // exec() only returns on error
        Err(io::Error::new(
            io::ErrorKind::Other,
            format!("Failed to exec new binary: {}", err),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_app() -> (App, TempDir) {
        let temp = TempDir::new().unwrap();
        let profile_manager = ProfileManager::with_config_dir(temp.path().to_path_buf()).unwrap();
        let app_config = profile_manager.load_app_config().unwrap_or_default();
        let profiles = profile_manager.load_all_profiles().unwrap_or_default();

        let app = App {
            running: true,
            screen: Screen::Main,
            main_menu: MenuState::new(8),
            profile_menu: MenuState::new(profiles.len()),
            settings_menu: MenuState::new(2),
            profile_manager,
            app_config,
            profiles: profiles.clone(),
            selected_profile: profiles.first().cloned(),
            status_message: None,
            editing_profile: None,
            env_vars_list: Vec::new(),
            env_menu: MenuState::new(0),
            edit_field: EditField::None,
            key_input: TextInput::new(),
            value_input: TextInput::new(),
            editing_env_index: None,
            version_manager: VersionManager::new(),
            version_menu: MenuState::new(0),
            versions: Vec::new(),
            selected_version: None,
            version_agent: AgentType::Claude,
            cached_agent_versions: HashMap::new(),
            cached_version_lists: HashMap::new(),
            cached_installed_version: None,
            version_fetch_receiver: None,
            version_list_receiver: None,
            install_state: None,
            animation_frame: 0,
            art_layout: ArtLayout::default(),
            art_animation: None,
            animations_enabled: true,
            pending_screen: None,
            pending_external_edit: None,
            help_return_screen: None,
            help_scroll_offset: 0,
            lava_mode: false,
            konami_progress: 0,
            theme_menu: MenuState::new(ThemePreset::all().len() + 1),
            theme_color: ThemeColor::Preset(ThemePreset::Orange),
        };

        (app, temp)
    }

    #[test]
    fn test_app_creation() {
        let (app, _temp) = test_app();
        assert!(app.running);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn test_art_layout_default() {
        assert_eq!(ArtLayout::default(), ArtLayout::ArtRight);
    }

    #[test]
    fn test_content_width_main_screen() {
        let (app, _temp) = test_app();
        let width = app.content_width();
        // Main menu should have reasonable width (based on menu item text)
        assert!(width >= 30 && width <= 50);
    }

    #[test]
    fn test_content_width_varies_by_screen() {
        let (mut app, _temp) = test_app();

        let main_width = app.content_width();

        app.screen = Screen::Settings;
        let settings_width = app.content_width();

        app.screen = Screen::Help;
        let help_width = app.content_width();

        // Different screens can have different widths
        assert!(main_width > 0);
        assert!(settings_width > 0);
        assert!(help_width > 0);
    }

    #[test]
    fn test_navigation() {
        let (mut app, _temp) = test_app();
        assert_eq!(app.main_menu.selected, 0);

        app.main_menu.handle_action(NavAction::Down);
        assert_eq!(app.main_menu.selected, 1);
    }

    #[test]
    fn test_screen_transitions() {
        let (mut app, _temp) = test_app();

        // Disable animations for instant transitions in test
        app.animations_enabled = false;

        app.main_menu.selected = 1;
        let _ = app.handle_main_input(NavAction::Select);
        app.tick(); // Complete pending transition
        assert_eq!(app.screen, Screen::Profiles);

        app.handle_profiles_input(NavAction::Back);
        app.tick(); // Complete pending transition
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn test_help_from_main() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        assert_eq!(app.screen, Screen::Main);
        let _ = app.handle_event(Event::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)));
        app.tick();
        assert_eq!(app.screen, Screen::Help);
        assert_eq!(app.help_return_screen, Some(Screen::Main));

        // Leaving help returns to Main
        app.handle_help_input(NavAction::Back);
        app.tick();
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn test_help_from_subscreen_returns_to_subscreen() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        // Navigate to Settings
        app.main_menu.selected = 3;
        let _ = app.handle_main_input(NavAction::Select);
        app.tick();
        assert_eq!(app.screen, Screen::Settings);

        // Press ? to open help
        let _ = app.handle_event(Event::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)));
        app.tick();
        assert_eq!(app.screen, Screen::Help);
        assert_eq!(app.help_return_screen, Some(Screen::Settings));

        // Leaving help returns to Settings, not Main
        app.handle_help_input(NavAction::Back);
        app.tick();
        assert_eq!(app.screen, Screen::Settings);
    }

    #[test]
    fn test_help_from_profiles() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        // Navigate to Profiles
        app.main_menu.selected = 1;
        let _ = app.handle_main_input(NavAction::Select);
        app.tick();
        assert_eq!(app.screen, Screen::Profiles);

        // Press ? to open help
        let _ = app.handle_event(Event::Key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE)));
        app.tick();
        assert_eq!(app.screen, Screen::Help);

        // Leaving help returns to Profiles
        app.handle_help_input(NavAction::Back);
        app.tick();
        assert_eq!(app.screen, Screen::Profiles);
    }

    #[test]
    fn test_env_var_editing() {
        let (mut app, _temp) = test_app();

        // Load profile for editing
        let profile = app.profiles[0].clone();
        app.load_profile_for_editing(profile);

        // Start adding new env var
        app.key_input = TextInput::new().with_value("TEST_KEY");
        app.value_input = TextInput::new().with_value("test_value");
        app.editing_env_index = None;
        app.save_env_var();

        assert!(app.env_vars_list.iter().any(|(k, _)| k == "TEST_KEY"));
    }

    #[test]
    fn test_sensitive_key_detection() {
        let (mut app, _temp) = test_app();

        app.key_input = TextInput::new().with_value("ANTHROPIC_API_KEY");
        assert!(is_sensitive_key(&app.key_input.value));

        app.key_input = TextInput::new().with_value("HOME");
        assert!(!is_sensitive_key(&app.key_input.value));
    }

    #[test]
    fn test_konami_code_starts_inactive() {
        let (app, _temp) = test_app();
        assert!(!app.lava_mode);
        assert_eq!(app.konami_progress, 0);
    }

    #[test]
    fn test_konami_code_partial_sequence_no_activation() {
        let (mut app, _temp) = test_app();

        // Enter partial sequence: ↑↑↓↓←→←→B (missing A)
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Down);
        app.check_konami_code(KeyCode::Down);
        app.check_konami_code(KeyCode::Left);
        app.check_konami_code(KeyCode::Right);
        app.check_konami_code(KeyCode::Left);
        app.check_konami_code(KeyCode::Right);
        app.check_konami_code(KeyCode::Char('b'));

        // Should not activate with incomplete sequence
        assert!(!app.lava_mode);
        assert_eq!(app.konami_progress, 9); // Progress at 9, waiting for 'a'
    }

    #[test]
    fn test_konami_code_full_sequence_activates() {
        let (mut app, _temp) = test_app();

        // Full Konami code: ↑↑↓↓←→←→BA
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Down);
        app.check_konami_code(KeyCode::Down);
        app.check_konami_code(KeyCode::Left);
        app.check_konami_code(KeyCode::Right);
        app.check_konami_code(KeyCode::Left);
        app.check_konami_code(KeyCode::Right);
        app.check_konami_code(KeyCode::Char('b'));
        app.check_konami_code(KeyCode::Char('a'));

        // Should activate lava mode
        assert!(app.lava_mode);
        assert_eq!(app.konami_progress, 0); // Reset after completion
    }

    #[test]
    fn test_konami_code_toggles_on_repeat() {
        let (mut app, _temp) = test_app();

        // Helper to enter full sequence
        let enter_konami = |app: &mut App| {
            app.check_konami_code(KeyCode::Up);
            app.check_konami_code(KeyCode::Up);
            app.check_konami_code(KeyCode::Down);
            app.check_konami_code(KeyCode::Down);
            app.check_konami_code(KeyCode::Left);
            app.check_konami_code(KeyCode::Right);
            app.check_konami_code(KeyCode::Left);
            app.check_konami_code(KeyCode::Right);
            app.check_konami_code(KeyCode::Char('b'));
            app.check_konami_code(KeyCode::Char('a'));
        };

        // First time: activates
        enter_konami(&mut app);
        assert!(app.lava_mode);

        // Second time: deactivates
        enter_konami(&mut app);
        assert!(!app.lava_mode);

        // Third time: activates again
        enter_konami(&mut app);
        assert!(app.lava_mode);
    }

    #[test]
    fn test_konami_code_wrong_key_resets_progress() {
        let (mut app, _temp) = test_app();

        // Start sequence correctly: ↑↑↓
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Down);
        assert_eq!(app.konami_progress, 3);

        // Wrong key (expected: Down)
        app.check_konami_code(KeyCode::Left);

        // Progress should reset (to 0 since Left != Up)
        assert_eq!(app.konami_progress, 0);
        assert!(!app.lava_mode);
    }

    #[test]
    fn test_konami_code_wrong_key_restart_with_up() {
        let (mut app, _temp) = test_app();

        // Start sequence correctly: ↑↑↓
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Up);
        app.check_konami_code(KeyCode::Down);
        assert_eq!(app.konami_progress, 3);

        // Wrong key that happens to be Up (start of sequence)
        app.check_konami_code(KeyCode::Up);

        // Progress should be 1 (restarted with Up)
        assert_eq!(app.konami_progress, 1);
        assert!(!app.lava_mode);
    }

    #[test]
    fn test_help_from_main_menu_item() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        // Select Help menu item (index 6)
        app.main_menu.selected = 6;
        let _ = app.handle_main_input(NavAction::Select);
        app.tick();
        assert_eq!(app.screen, Screen::Help);
        assert_eq!(app.help_return_screen, Some(Screen::Main));
    }

    #[test]
    fn test_help_scroll() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::Help;

        assert_eq!(app.help_scroll_offset, 0);

        // Scroll down
        app.handle_help_input(NavAction::Down);
        assert_eq!(app.help_scroll_offset, 1);

        app.handle_help_input(NavAction::Down);
        assert_eq!(app.help_scroll_offset, 2);

        // Scroll back up
        app.handle_help_input(NavAction::Up);
        assert_eq!(app.help_scroll_offset, 1);

        // Scroll up at 0 stays at 0
        app.handle_help_input(NavAction::Up);
        assert_eq!(app.help_scroll_offset, 0);
        app.handle_help_input(NavAction::Up);
        assert_eq!(app.help_scroll_offset, 0);
    }

    #[test]
    fn test_help_scroll_resets_on_leave() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::Help;
        app.help_return_screen = Some(Screen::Main);

        // Scroll down
        app.handle_help_input(NavAction::Down);
        app.handle_help_input(NavAction::Down);
        assert_eq!(app.help_scroll_offset, 2);

        // Leave help
        app.handle_help_input(NavAction::Back);
        app.tick();
        assert_eq!(app.help_scroll_offset, 0);
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn test_quit_is_now_index_7() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        // Selecting index 7 should quit
        app.main_menu.selected = 7;
        let _ = app.handle_main_input(NavAction::Select);
        assert!(!app.running);
    }

    #[test]
    fn test_agent_cycle_right() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        // Navigate to version management screen
        app.screen = Screen::VersionManagement;
        assert_eq!(app.version_agent, AgentType::Claude);

        // Cycle right: Claude -> Codex
        app.handle_version_input(NavAction::Right);
        assert_eq!(app.version_agent, AgentType::Codex);

        // Cycle right wraps: Codex -> Claude
        app.handle_version_input(NavAction::Right);
        assert_eq!(app.version_agent, AgentType::Claude);
    }

    #[test]
    fn test_agent_cycle_left() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.screen = Screen::VersionManagement;
        assert_eq!(app.version_agent, AgentType::Claude);

        // Cycle left wraps: Claude -> Codex
        app.handle_version_input(NavAction::Left);
        assert_eq!(app.version_agent, AgentType::Codex);

        // Cycle left: Codex -> Claude
        app.handle_version_input(NavAction::Left);
        assert_eq!(app.version_agent, AgentType::Claude);
    }

    #[test]
    fn test_agent_cycle_resets_selection() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.screen = Screen::VersionManagement;
        app.version_menu.selected = 3;
        app.version_menu.scroll_offset = 2;

        // Switching agent resets selection and scroll
        app.handle_version_input(NavAction::Right);
        assert_eq!(app.version_menu.selected, 0);
        assert_eq!(app.version_menu.scroll_offset, 0);
    }

    #[test]
    fn test_codex_version_list_no_installed() {
        let (app, _temp) = test_app();

        // No cached Codex version -> empty fallback list
        let versions = app.get_codex_version_list();
        assert_eq!(versions.len(), 0);
    }

    #[test]
    fn test_codex_version_list_with_installed() {
        let (mut app, _temp) = test_app();

        // Cache a Codex installed version
        app.cached_agent_versions
            .insert(AgentType::Codex, Some("0.93.0".to_string()));

        let versions = app.get_codex_version_list();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].version, "0.93.0");
        assert!(versions[0].is_installed);
        // Whitelisted version should show marker
        assert!(versions[0].is_whitelisted);
    }

    #[test]
    fn test_codex_install_sets_install_state() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_agent = AgentType::Codex;

        // Populate Codex version list with a whitelisted version
        app.versions = vec![VersionInfo {
            version: "0.93.0".to_string(),
            is_installed: false,
            has_patch: false,
            is_whitelisted: true,
            is_blacklisted: false,
        }];
        app.version_menu.set_items_count(app.versions.len());

        // Select and install
        app.install_codex_version();

        assert_eq!(app.screen, Screen::VersionInstalling);
        assert_eq!(app.selected_version, Some("0.93.0".to_string()));
        let state = app.install_state.as_ref().unwrap();
        assert_eq!(state.agent_type, AgentType::Codex);
        assert_eq!(state.version, "0.93.0");
        assert!(state.is_allowed);
        assert_eq!(state.current_step, InstallStep::Installing);
    }

    #[test]
    fn test_codex_install_skips_patching() {
        let (mut app, _temp) = test_app();

        // Simulate a successful Codex install completing
        let (tx, rx) = mpsc::channel();
        app.install_state = Some(InstallState {
            agent_type: AgentType::Codex,
            version: "latest".to_string(),
            is_allowed: true,
            receiver: rx,
            _handle: thread::spawn(|| {}),
            start_time: Instant::now(),
            current_step: InstallStep::Installing,
            install_result: None,
            patch_result: None,
        });

        // Send successful install result
        tx.send(InstallStepResult::InstallComplete(InstallResult {
            success: true,
            stdout: "done".to_string(),
            stderr: String::new(),
            error: None,
        }))
        .unwrap();

        app.tick();

        // Codex should skip Patching and go straight to Done
        // (tick processes Done and clears install_state)
        assert!(app.install_state.is_none());
        assert_eq!(app.screen, Screen::VersionManagement);
    }

    #[test]
    fn test_agent_version_menu_navigates_to_version_screen() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        // Select "Agent Versions" (index 2) from main menu
        app.main_menu.selected = 2;
        let _ = app.handle_main_input(NavAction::Select);
        app.tick();
        assert_eq!(app.screen, Screen::VersionManagement);
    }
}
