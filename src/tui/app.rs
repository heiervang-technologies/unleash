//! Main TUI application

use crate::agents::{AgentDefinition, AgentManager, AgentType};
use crate::config::{AppConfig, Profile, ProfileManager};
use crate::input::{key_to_action, MenuState, NavAction};
use crate::pixel_art::mascots;
use crate::text_input::{censor_sensitive, is_sensitive_key, TextInput};
use crate::theme::{ThemeColor, ThemePreset};
use crate::version::{ConflictEntry, InstallResult, VersionInfo, VersionManager};

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
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

/// Receiver type for async version-list fetches.
type VersionListReceiver = Receiver<(AgentType, Vec<VersionInfo>, Vec<ConflictEntry>)>;

/// Width of the ANSI art sidebar — derived from the shared mascot constant.
const ART_WIDTH: u16 = crate::pixel_art::mascots::HALF_WIDTH as u16;

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
    Theme,
    Help,
    ConfirmDelete,
    VersionManagement,
    Features,
}

/// Main menu items — order here defines display order in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MainMenuItem {
    Start,
    Profiles,
    Versions,
    Features,
    Help,
    Quit,
}

/// Single source of truth for main menu layout.
const MAIN_MENU: &[(MainMenuItem, &str, &str)] = &[
    (
        MainMenuItem::Start,
        "Start Session",
        "Launch with selected profile",
    ),
    (
        MainMenuItem::Profiles,
        "Profiles",
        "Manage profiles and their settings",
    ),
    (
        MainMenuItem::Versions,
        "Versions & Updates",
        "Manage unleash and agent CLI versions",
    ),
    (
        MainMenuItem::Features,
        "Features & Plugins",
        "Toggle plugins and experimental features",
    ),
    (MainMenuItem::Help, "Help", "Keyboard shortcuts and tips"),
    (MainMenuItem::Quit, "Quit", "Exit the launcher"),
];

/// Focus zone within the unified version management screen
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VersionFocus {
    /// Focus on the unleash (parent) section
    Unleash,
    /// Focus on the agent picker
    AgentPicker,
    /// Focus on the version list for the selected agent
    VersionList,
}

/// What we're currently editing
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditField {
    None,
    ProfileName,
    #[allow(dead_code)]
    ProfileDescription,
    EnvKey,
    EnvValue,
    AgentCliPath,
    ClaudeArgs,
    StopPrompt,
    ThemeHex,
}

/// State for async version installation
pub struct InstallState {
    pub agent_type: AgentType,
    pub version: String,
    pub receiver: Receiver<InstallStepResult>,
    pub _handle: JoinHandle<()>,
    pub start_time: Instant,
    pub current_step: InstallStep,
    pub install_result: Option<InstallResult>,
}

/// Current step in the installation process
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallStep {
    Installing,
    Done,
}

/// Result from a single installation step
pub enum InstallStepResult {
    /// A line of log output from the install process
    LogLine(String),
    /// Installation has completed
    InstallComplete(InstallResult),
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

/// Targets that can be activated by a mouse click
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClickTarget {
    MainMenuItem(usize),
    ProfileItem(usize),
    ProfileEditItem(usize),
    UnleashSection,
    VersionAgentItem(usize),
    VersionListItem(usize),
    ThemeItem(usize),
    FeatureItem(usize),
    /// The Claude mascot / avatar art sidebar
    AvatarArt,
    DialogYes,
    DialogNo,
}

/// Main application state
pub struct App {
    pub running: bool,
    pub last_frame_area: Rect,
    pub screen: Screen,
    pub main_menu: MenuState,
    pub profile_menu: MenuState,
    pub profile_manager: ProfileManager,
    pub app_config: AppConfig,
    pub profiles: Vec<Profile>,
    pub selected_profile: Option<Profile>,
    pub status_message: Option<String>,
    /// Profile search/filter query
    pub profile_search_query: String,
    /// Whether search input is active
    pub profile_search_active: bool,

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
    version_list_receiver: Option<VersionListReceiver>,
    /// Last time we polled version lists per agent (for 10-minute TTL)
    last_version_poll: HashMap<AgentType, std::time::Instant>,
    /// Async installation state
    pub install_state: Option<InstallState>,

    // Conflict detection
    pub conflict_entries: Vec<ConflictEntry>,
    pub conflict_warning_open: bool,
    /// Suppress conflict dialog after cleanup attempt (prevents infinite loop)
    pub conflict_dismissed: bool,
    // npm install dialog
    pub npm_dialog_open: bool,
    /// Pending install to resume after npm is installed (agent, version)
    pub npm_dialog_pending: Option<(AgentType, String)>,
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
    /// Pending profile file edit - path to open directly in external editor
    pub pending_profile_file_edit: Option<std::path::PathBuf>,
    /// Screen to return to when leaving Help (so ? works from any screen)
    pub help_return_screen: Option<Screen>,
    /// Scroll offset for help screen content
    pub help_scroll_offset: u16,

    // Unified version management
    /// Which section has focus in the unified version view
    pub version_focus: VersionFocus,
    /// Current unleash version (from CARGO_PKG_VERSION)
    pub unleash_version: String,
    /// Menu state for agent picker
    pub agent_picker_menu: MenuState,
    /// Index of version currently being installed (for inline spinner)
    pub installing_version_index: Option<usize>,
    /// Accumulated install log lines for the log panel
    pub install_log_lines: Vec<String>,
    /// Whether the install log panel is visible
    pub show_install_log: bool,
    /// Whether 'g' was pressed (waiting for second 'g' for gg jump-to-top)
    pub g_pending: bool,

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

    // Features screen
    pub feature_menu: MenuState,
    pub discovered_plugins: Vec<crate::config::PluginMeta>,

    // Mouse support
    /// Clickable regions registered during the last render pass for hit-testing
    clickable_areas: Vec<(Rect, ClickTarget)>,

    /// All available agent types (built-in + custom from config)
    available_agents: Vec<AgentType>,
}

impl App {
    pub fn new() -> io::Result<Self> {
        // Signal to child functions that we're in TUI mode (stdin is in raw mode)
        std::env::set_var("UNLEASH_TUI", "1");
        let profile_manager = ProfileManager::new()?;
        let app_config = profile_manager.load_app_config().unwrap_or_default();
        let profiles = profile_manager.load_all_profiles().unwrap_or_default();

        let selected_profile = profiles
            .iter()
            .find(|p| p.name == app_config.current_profile)
            .cloned()
            .or_else(|| profiles.first().cloned());

        let version_manager = VersionManager::new();

        // Build the full list of agent types (built-in + custom from config)
        let custom_defs: Vec<AgentDefinition> = app_config
            .custom_agents
            .iter()
            .filter(|a| a.enabled)
            .map(AgentDefinition::from_custom_config)
            .collect();
        let available_agents = AgentType::all_with_custom(&custom_defs);

        // Pre-populate version caches from embedded (compiled-in) version lists.
        // This makes version lists appear instantly — no network fetch needed.
        let embedded = crate::version::load_embedded_versions();
        let mut cached_version_lists: HashMap<AgentType, Vec<VersionInfo>> = HashMap::new();
        let agent_keys: &[(&str, AgentType)] = &[
            ("claude", AgentType::Claude),
            ("codex", AgentType::Codex),
            ("gemini", AgentType::Gemini),
            ("opencode", AgentType::OpenCode),
        ];
        for (key, agent_type) in agent_keys {
            if let Some(versions) = embedded.get(*key) {
                cached_version_lists.insert(
                    agent_type.clone(),
                    versions
                        .iter()
                        .map(|v| VersionInfo {
                            version: v.clone(),
                            is_installed: false, // updated once async version check completes
                        })
                        .collect(),
                );
            }
        }

        // Spawn a background thread to fetch installed versions for all agents
        // This prevents blocking the TUI startup
        let (version_tx, version_rx) = mpsc::channel();
        thread::spawn(move || {
            // Claude version
            let claude_version = VersionManager::new().get_installed_version();
            let _ = version_tx.send((AgentType::Claude, claude_version));

            // All other agents via AgentManager
            if let Ok(mut mgr) = AgentManager::new() {
                for agent_type in &[AgentType::Codex, AgentType::Gemini, AgentType::OpenCode] {
                    let v = mgr.get_installed_version(agent_type.clone()).ok().flatten();
                    let _ = version_tx.send((agent_type.clone(), v));
                }
            }
        });

        let theme_color = selected_profile
            .as_ref()
            .and_then(|p| ThemeColor::from_config(&p.theme))
            .unwrap_or(ThemeColor::Preset(ThemePreset::Orange));

        let animations_enabled =
            app_config.animations || std::env::var("UNLEASH_ANIMATIONS").is_ok_and(|v| v == "1");

        Ok(Self {
            running: true,
            last_frame_area: Rect::default(),
            screen: Screen::Main,
            main_menu: MenuState::new(MAIN_MENU.len()),
            profile_menu: MenuState::new(profiles.len()),
            profile_manager,
            app_config,
            profiles,
            selected_profile,
            status_message: None,
            profile_search_query: String::new(),
            profile_search_active: false,
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
            cached_version_lists,
            cached_installed_version: None, // Will be populated async
            version_fetch_receiver: Some(version_rx),
            version_list_receiver: None,
            last_version_poll: HashMap::new(),
            install_state: None,
            conflict_entries: Vec::new(),
            conflict_warning_open: false,
            conflict_dismissed: false,
            npm_dialog_open: false,
            npm_dialog_pending: None,
            animation_frame: 0,
            art_layout: ArtLayout::ArtRight,
            art_animation: None,
            animations_enabled,
            pending_screen: None,
            pending_external_edit: None,
            pending_profile_file_edit: None,
            help_return_screen: None,
            help_scroll_offset: 0,
            version_focus: VersionFocus::Unleash,
            unleash_version: env!("CARGO_PKG_VERSION").to_string(),
            agent_picker_menu: MenuState::new(available_agents.len()),
            installing_version_index: None,
            install_log_lines: Vec::new(),
            show_install_log: false,
            g_pending: false,
            lava_mode: false,
            konami_progress: 0,
            theme_menu: MenuState::new(ThemePreset::all().len() + 1), // presets + Custom
            theme_color,
            feature_menu: MenuState::new(0),
            discovered_plugins: Vec::new(),
            clickable_areas: Vec::new(),
            available_agents,
        })
    }

    /// Refresh the cached installed version for a specific agent
    pub fn refresh_cached_version_for(&mut self, agent_type: AgentType) {
        let version = match &agent_type {
            AgentType::Claude => {
                let v = self.version_manager.get_installed_version();
                self.cached_installed_version = v.clone();
                v
            }
            _ => AgentManager::new()
                .ok()
                .and_then(|mut m| m.get_installed_version(agent_type.clone()).ok().flatten()),
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
        if self.animations_enabled {
            self.animation_frame = self.animation_frame.wrapping_add(1);
        }

        // Poll async version fetch (drains all available agent version messages)
        if let Some(ref receiver) = self.version_fetch_receiver {
            loop {
                match receiver.try_recv() {
                    Ok((agent_type, version)) => {
                        if agent_type == AgentType::Claude {
                            self.cached_installed_version = version.clone();
                        }
                        self.cached_agent_versions
                            .insert(agent_type.clone(), version.clone());

                        // Update is_installed flags in cached version lists (embedded or fetched)
                        let mut needs_save = false;
                        if let Some(list) = self.cached_version_lists.get_mut(&agent_type) {
                            let mut found = false;
                            for vi in list.iter_mut() {
                                if version.as_deref() == Some(vi.version.as_str()) {
                                    vi.is_installed = true;
                                    found = true;
                                } else {
                                    vi.is_installed = false;
                                }
                            }
                            if !found {
                                if let Some(v) = &version {
                                    list.insert(
                                        0,
                                        VersionInfo {
                                            version: v.clone(),
                                            is_installed: true,
                                        },
                                    );
                                    needs_save = true;
                                }
                            }
                        }

                        if needs_save {
                            crate::version::save_embedded_versions(&self.cached_version_lists);
                        }

                        // Also update the currently displayed list if viewing this agent
                        if self.version_agent == agent_type {
                            if let Some(list) = self.cached_version_lists.get(&agent_type) {
                                self.versions = list.clone();
                                self.version_menu.set_items_count(self.versions.len());
                            }
                        }
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
                Ok((agent_type, versions, conflict_entries)) => {
                    self.cached_version_lists
                        .insert(agent_type.clone(), versions.clone());
                    crate::version::save_embedded_versions(&self.cached_version_lists);

                    // Record successful poll timestamp
                    self.last_version_poll
                        .insert(agent_type.clone(), std::time::Instant::now());

                    // Update displayed list if we're still viewing this agent
                    if self.screen == Screen::VersionManagement && self.version_agent == agent_type
                    {
                        let prev_selected = self.version_menu.selected;
                        self.versions = versions;
                        self.version_menu.set_items_count(self.versions.len());
                        // Preserve selection if possible
                        if prev_selected < self.versions.len() {
                            self.version_menu.selected = prev_selected;
                        }
                        self.conflict_entries = conflict_entries;
                        if self.conflict_entries.len() > 1 && !self.conflict_dismissed {
                            self.conflict_warning_open = true;
                        }
                        self.status_message =
                            Some(format!("{} versions loaded", agent_type.display_name()));
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
                    InstallStepResult::LogLine(line) => {
                        self.install_log_lines.push(line);
                    }
                    InstallStepResult::InstallComplete(install_result) => {
                        state.install_result = Some(install_result);
                        state.current_step = InstallStep::Done;
                    }
                }
            }

            // If done, update status and return to version list
            if state.current_step == InstallStep::Done {
                let version = state.version.clone();
                let agent_type = state.agent_type.clone();
                let agent_name = agent_type.display_name();
                let install_ok = state.install_result.as_ref().is_some_and(|r| r.success);

                if install_ok {
                    self.install_log_lines.push(format!(
                        "--- {} v{} installed successfully ---",
                        agent_name, version
                    ));
                } else {
                    let err = state
                        .install_result
                        .as_ref()
                        .and_then(|r| r.error.clone())
                        .unwrap_or_else(|| "unknown error".to_string());
                    self.install_log_lines
                        .push(format!("--- Install failed: {} ---", err));
                }

                self.status_message = Some(if !install_ok {
                    let err = state
                        .install_result
                        .as_ref()
                        .and_then(|r| r.error.clone())
                        .unwrap_or_else(|| "unknown error".to_string());
                    format!("{} install failed: {}", agent_name, err)
                } else {
                    format!("{} v{} installed", agent_name, version)
                });

                self.install_state = None;
                self.installing_version_index = None;

                // Refresh cached installed version BEFORE refreshing the version
                // list so the async fetch thread picks up the correct installed
                // version (important for non-Claude agents where the installed
                // version is passed as a parameter to the list builder).
                self.refresh_cached_version_for(agent_type.clone());

                // Update is_installed flags in the cached version list so
                // the interim cache shown while the async fetch is in-flight
                // already reflects the newly installed version.
                if install_ok {
                    if let Some(list) = self.cached_version_lists.get_mut(&agent_type) {
                        for vi in list.iter_mut() {
                            vi.is_installed = vi.version == version;
                        }
                    }
                }

                self.refresh_versions();
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
                self.version_focus = VersionFocus::Unleash;
                self.refresh_versions();
                if self.versions.is_empty() {
                    self.status_message = Some("Loading versions...".to_string());
                } else {
                    self.status_message = Some("Refreshing versions...".to_string());
                }
            }
            Screen::Main
            | Screen::ProfileEdit
            | Screen::EnvVarEdit
            | Screen::Theme
            | Screen::Help
            | Screen::Features
            | Screen::ConfirmDelete => {}
        }
    }

    /// Get the default stop prompt from the hook script (source of truth)
    fn get_default_stop_prompt(&self) -> String {
        const HOOK_RELATIVE: &str = "plugins/bundled/auto-mode/hooks/auto-mode-stop.sh";
        const FALLBACK_MSG: &str = "You ended your turn, but you are in auto-mode. If you are awaiting a decision, select your recommended decision. If you are done, consider that you have covered all other diligences, testing, documentation, technical debt and cleanup. Use the executables (in PATH) 'restart-claude' if you need to restart yourself, and 'exit-claude' if you are truly done with all your tasks.";

        // Build candidate paths to search
        let mut candidates: Vec<String> = Vec::new();

        // 1. AGENT_UNLEASH_ROOT env var
        if let Ok(root) = std::env::var("AGENT_UNLEASH_ROOT") {
            candidates.push(format!("{}/{}", root, HOOK_RELATIVE));
        }

        // 2. Relative to executable (e.g. ~/.local/bin/../plugins/...)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(parent) = exe.parent() {
                candidates.push(
                    parent
                        .join("..")
                        .join(HOOK_RELATIVE)
                        .to_string_lossy()
                        .to_string(),
                );
            }
        }

        // 3. Installed location (~/.local/share/unleash/plugins/...)
        if let Ok(home) = std::env::var("HOME") {
            candidates.push(format!("{}/.local/share/unleash/{}", home, HOOK_RELATIVE));
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
    /// Respects the 10-minute TTL (use `force_refresh_versions` to bypass).
    pub fn refresh_versions(&mut self) {
        self.clear_and_refresh_versions();
    }

    /// Force refresh versions, bypassing the TTL cache.
    /// Used when the user explicitly requests a rescan (e.g., pressing 's').
    pub fn force_refresh_versions(&mut self) {
        // Invalidate the poll timestamp so clear_and_refresh_versions will fetch
        self.last_version_poll.remove(&self.version_agent);
        self.clear_and_refresh_versions();
    }

    /// Version poll TTL: only re-fetch from network if >10 minutes since last poll.
    const VERSION_POLL_TTL: std::time::Duration = std::time::Duration::from_secs(10 * 60);

    /// Clear version list and show loading state, then fetch fresh data.
    /// Prevents stale data from a previous agent being shown after switching.
    /// Respects a 10-minute TTL to avoid excessive network polling.
    fn clear_and_refresh_versions(&mut self) {
        let agent = self.version_agent.clone();

        // Clear displayed list immediately to prevent stale data from wrong agent
        self.versions.clear();
        self.version_menu.set_items_count(0);
        self.version_menu.selected = 0;
        self.version_menu.scroll_offset = 0;

        // Always show cached data immediately if available
        if let Some(cached) = self.cached_version_lists.get(&agent) {
            if !cached.is_empty() {
                self.versions = cached.clone();
                self.version_menu.set_items_count(self.versions.len());
            }
        }

        // Check if we need to poll (TTL expired or never polled)
        let should_poll = self
            .last_version_poll
            .get(&agent)
            .map(|last| last.elapsed() > Self::VERSION_POLL_TTL)
            .unwrap_or(true);

        if should_poll {
            self.status_message = if self.versions.is_empty() {
                Some(format!("Loading {} versions...", agent.display_name()))
            } else {
                Some(format!("Syncing {} versions...", agent.display_name()))
            };
            self.start_async_version_fetch(agent);
        } else {
            self.status_message = Some(format!("{} versions (cached)", agent.display_name()));
        }
    }

    /// Spawn a background thread to fetch the version list for an agent
    fn start_async_version_fetch(&mut self, agent: AgentType) {
        let (tx, rx) = mpsc::channel();
        let installed = self
            .cached_agent_versions
            .get(&agent)
            .and_then(|v| v.clone());
        match agent {
            AgentType::Claude => {
                thread::spawn(move || {
                    let vm = VersionManager::new();
                    let versions = vm.get_version_list();
                    let conflicts = vm.detect_conflicts("claude");
                    let _ = tx.send((AgentType::Claude, versions, conflicts));
                });
            }
            AgentType::Codex => {
                thread::spawn(move || {
                    let vm = VersionManager::new();
                    let versions = vm.get_codex_version_list(installed.as_deref());
                    let conflicts = vm.detect_conflicts("codex");
                    let _ = tx.send((AgentType::Codex, versions, conflicts));
                });
            }
            AgentType::Gemini => {
                thread::spawn(move || {
                    let vm = VersionManager::new();
                    let versions = vm.get_gemini_version_list(installed.as_deref());
                    let conflicts = vm.detect_conflicts("gemini");
                    let _ = tx.send((AgentType::Gemini, versions, conflicts));
                });
            }
            AgentType::OpenCode => {
                thread::spawn(move || {
                    let vm = VersionManager::new();
                    let versions = vm.get_opencode_version_list(installed.as_deref());
                    let conflicts = vm.detect_conflicts("opencode");
                    let _ = tx.send((AgentType::OpenCode, versions, conflicts));
                });
            }
            AgentType::Custom(_) => {
                // Version management not yet supported for custom agents
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
            });
        }

        versions
    }

    pub fn refresh_profiles(&mut self) {
        self.profiles = self.profile_manager.load_all_profiles().unwrap_or_default();
        self.profile_menu.set_items_count(self.profiles.len());
    }

    pub fn load_profile_for_editing(&mut self, profile: Profile) {
        self.env_vars_list = profile
            .env
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        self.env_vars_list.sort_by(|a, b| a.0.cmp(&b.0));
        // Menu items: 4 settings + N env vars + 1 "Add new"
        self.env_menu
            .set_items_count(Self::PROFILE_SETTINGS_COUNT + self.env_vars_list.len() + 1);
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
            if self
                .selected_profile
                .as_ref()
                .is_some_and(|p| p.name == name)
            {
                self.selected_profile = self.profiles.iter().find(|p| p.name == name).cloned();
            }
            // Also update editing_profile from refreshed profiles
            self.editing_profile = self.profiles.iter().find(|p| p.name == name).cloned();
        }

        Ok(())
    }

    // ── Mouse support ──────────────────────────────────────────────────────────

    /// Dispatch a mouse event to the appropriate handler
    fn handle_mouse(&mut self, mouse: MouseEvent) -> io::Result<Option<AppAction>> {
        match mouse.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                let col = mouse.column;
                let row = mouse.row;
                if let Some(target) = self
                    .clickable_areas
                    .iter()
                    .find(|(rect, _)| {
                        col >= rect.x
                            && col < rect.x + rect.width
                            && row >= rect.y
                            && row < rect.y + rect.height
                    })
                    .map(|(_, t)| *t)
                {
                    return self.handle_click(target);
                }
            }
            MouseEventKind::ScrollUp => self.handle_scroll(NavAction::Up),
            MouseEventKind::ScrollDown => self.handle_scroll(NavAction::Down),
            _ => {}
        }
        Ok(None)
    }

    /// Handle a click on a registered ClickTarget
    fn handle_click(&mut self, target: ClickTarget) -> io::Result<Option<AppAction>> {
        match (target, self.screen) {
            (ClickTarget::MainMenuItem(i), Screen::Main) => {
                if self.main_menu.selected == i {
                    return self.handle_main_input(NavAction::Select);
                }
                self.main_menu.selected = i;
            }
            (ClickTarget::ProfileItem(i), Screen::Profiles | Screen::ConfirmDelete) => {
                if self.profile_menu.selected == i {
                    self.handle_profiles_input(
                        NavAction::Select,
                        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
                    );
                } else {
                    self.profile_menu.selected = i;
                }
            }
            (ClickTarget::ProfileEditItem(i), Screen::ProfileEdit | Screen::EnvVarEdit) => {
                if self.env_menu.selected == i {
                    let dummy_key = KeyEvent::new(KeyCode::Null, KeyModifiers::NONE);
                    self.handle_profile_edit_input(NavAction::Select, dummy_key);
                } else {
                    self.env_menu.selected = i;
                }
            }
            (ClickTarget::UnleashSection, Screen::VersionManagement) => {
                self.version_focus = VersionFocus::Unleash;
            }
            (ClickTarget::VersionAgentItem(i), Screen::VersionManagement) => {
                if self.agent_picker_menu.selected != i {
                    self.switch_to_agent_index(i);
                }
                self.version_focus = VersionFocus::AgentPicker;
            }
            (ClickTarget::VersionListItem(i), Screen::VersionManagement) => {
                if self.version_menu.selected == i
                    && self.version_focus == VersionFocus::VersionList
                {
                    let dummy_key = KeyEvent::new(KeyCode::Null, KeyModifiers::NONE);
                    let _ = self.handle_version_input(NavAction::Select, dummy_key);
                } else {
                    self.version_menu.selected = i;
                    self.version_focus = VersionFocus::VersionList;
                }
            }
            (ClickTarget::ThemeItem(i), Screen::Theme) => {
                if self.theme_menu.selected == i {
                    self.handle_theme_input(NavAction::Select);
                } else {
                    self.theme_menu.selected = i;
                }
            }
            (ClickTarget::FeatureItem(i), Screen::Features) => {
                if self.feature_menu.selected == i {
                    self.handle_features_input(NavAction::Select);
                } else {
                    self.feature_menu.selected = i;
                }
            }
            (ClickTarget::AvatarArt, screen) if screen != Screen::Main => {
                // Clicking the avatar from any sub-screen returns to Main
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
                if self.art_animation.is_none() {
                    self.screen = Screen::Main;
                    self.refresh_screen_data();
                }
            }
            (ClickTarget::DialogYes, Screen::VersionManagement) => {
                let dummy_key = KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE);
                let _ = self.handle_version_input(NavAction::Select, dummy_key);
            }
            (ClickTarget::DialogNo, Screen::VersionManagement) => {
                let dummy_key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
                let _ = self.handle_version_input(NavAction::Select, dummy_key);
            }
            _ => {}
        }
        Ok(None)
    }

    /// Handle scroll wheel events — navigates the active list on the current screen
    fn handle_scroll(&mut self, action: NavAction) {
        match self.screen {
            Screen::Main => {
                self.main_menu.handle_action(action);
            }
            Screen::Profiles => {
                self.profile_menu.handle_action(action);
            }
            Screen::ProfileEdit | Screen::EnvVarEdit => {
                self.env_menu.handle_action(action);
            }
            Screen::Theme => {
                self.theme_menu.handle_action(action);
            }
            Screen::VersionManagement => {
                match self.version_focus {
                    VersionFocus::Unleash => {} // no scroll in unleash section
                    VersionFocus::AgentPicker => {
                        let current_idx = self.available_agents
                            .iter()
                            .position(|a| *a == self.version_agent)
                            .unwrap_or(0);
                        let new_idx = match action {
                            NavAction::Down => {
                                (current_idx + 1).min(self.available_agents.len().saturating_sub(1))
                            }
                            NavAction::Up => current_idx.saturating_sub(1),
                            _ => current_idx,
                        };
                        if new_idx != current_idx {
                            self.switch_to_agent_index(new_idx);
                        }
                    }
                    VersionFocus::VersionList => {
                        self.version_menu.handle_action(action);
                    }
                }
            }
            Screen::Help => match action {
                NavAction::Up => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_sub(1);
                }
                NavAction::Down => {
                    self.help_scroll_offset = self.help_scroll_offset.saturating_add(1);
                }
                _ => {}
            },
            _ => {}
        }
    }

    // ── Input events ───────────────────────────────────────────────────────────

    /// Handle input events
    pub fn handle_event(&mut self, event: Event) -> io::Result<Option<AppAction>> {
        if let Event::Mouse(mouse) = event {
            return self.handle_mouse(mouse);
        }
        if let Event::Key(key) = event {
            // Global quit with Ctrl+C (except when editing)
            if key.code == KeyCode::Char('c')
                && key.modifiers.contains(KeyModifiers::CONTROL)
                && self.edit_field == EditField::None
            {
                self.running = false;
                return Ok(None);
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
            if action == NavAction::Help && self.screen != Screen::Help {
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
                Screen::Profiles => self.handle_profiles_input(action, key),
                Screen::ProfileEdit => self.handle_profile_edit_input(action, key),
                Screen::EnvVarEdit => self.handle_env_var_edit_input(action, key),
                Screen::Theme => self.handle_theme_input(action),
                Screen::Help => self.handle_help_input(action),
                Screen::ConfirmDelete => self.handle_confirm_delete_input(action),
                Screen::VersionManagement => return self.handle_version_input(action, key),
                Screen::Features => self.handle_features_input(action),
            }
        }
        Ok(None)
    }

    fn handle_text_input(&mut self, key: KeyEvent) -> Option<AppAction> {
        let input = match self.edit_field {
            EditField::EnvKey => &mut self.key_input,
            EditField::EnvValue => &mut self.value_input,
            EditField::ProfileName | EditField::ProfileDescription => &mut self.key_input,
            EditField::AgentCliPath
            | EditField::ClaudeArgs
            | EditField::StopPrompt
            | EditField::ThemeHex => &mut self.key_input,
            EditField::None => return None,
        };

        match key.code {
            KeyCode::Char(c) => {
                // Handle Ctrl+key shortcuts
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' => input.move_home(),        // Ctrl+A: go to start
                        'e' => input.move_end(),         // Ctrl+E: go to end
                        'w' => input.delete_word_back(), // Ctrl+W: delete word
                        'u' => input.delete_to_start(),  // Ctrl+U: delete to start
                        'k' => input.delete_to_end(),    // Ctrl+K: delete to end
                        _ => {}                          // Ignore other ctrl combinations
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
                    EditField::AgentCliPath => {
                        // Save agent_cli_path to editing profile
                        if let Some(ref mut profile) = self.editing_profile {
                            profile.agent_cli_path = self.key_input.value.clone();
                            let _ = self.profile_manager.save_profile(profile);
                        }
                        self.sync_editing_to_selected();
                        self.status_message = Some("Agent CLI saved".to_string());
                        self.edit_field = EditField::None;
                    }
                    EditField::ClaudeArgs => {
                        // Save agent_cli_args (space-separated) to editing profile (raw agent-specific args)
                        if let Some(ref mut profile) = self.editing_profile {
                            profile.agent_cli_args = self
                                .key_input
                                .value
                                .split_whitespace()
                                .map(|s| s.to_string())
                                .collect();
                            let _ = self.profile_manager.save_profile(profile);
                        }
                        self.sync_editing_to_selected();
                        self.status_message = Some("Arguments saved".to_string());
                        self.edit_field = EditField::None;
                    }
                    EditField::StopPrompt => {
                        // Save stop_prompt (empty string = None/default) to editing profile
                        let value = self.key_input.value.trim().to_string();
                        if let Some(ref mut profile) = self.editing_profile {
                            profile.stop_prompt = if value.is_empty() { None } else { Some(value) };
                            let _ = self.profile_manager.save_profile(profile);
                        }
                        self.sync_editing_to_selected();
                        self.status_message = Some("Stop prompt saved".to_string());
                        self.edit_field = EditField::None;
                    }
                    EditField::ThemeHex => {
                        let hex = self.key_input.value.trim().to_string();
                        if let Some((r, g, b)) = crate::theme::parse_hex_color(&hex) {
                            self.theme_color = ThemeColor::Custom(r, g, b);
                            if let Some(ref mut profile) = self.editing_profile {
                                profile.theme = self.theme_color.to_config();
                                let _ = self.profile_manager.save_profile(profile);
                            }
                            self.sync_editing_to_selected();
                            self.status_message =
                                Some(format!("Theme: #{:02X}{:02X}{:02X}", r, g, b));
                            self.edit_field = EditField::None;
                            self.screen = Screen::ProfileEdit;
                        } else {
                            self.status_message = Some(
                                "Invalid hex color — use 3 or 6 hex digits (e.g. FFF or FF5500)"
                                    .to_string(),
                            );
                        }
                    }
                    EditField::ProfileName => {
                        let new_name = self.key_input.value.trim().to_string();
                        if !new_name.is_empty() {
                            if let Some(ref mut profile) = self.editing_profile {
                                let old_name = profile.name.clone();
                                if new_name != old_name {
                                    // Check reserved name BEFORE deleting old profile
                                    if ProfileManager::is_reserved_name(&new_name) {
                                        self.status_message = Some(format!(
                                            "Cannot rename to '{}': reserved name",
                                            new_name
                                        ));
                                    } else {
                                        // Save with new name FIRST, then delete old
                                        profile.name = new_name.clone();
                                        match self.profile_manager.save_profile(profile) {
                                            Ok(()) => {
                                                // Now safe to delete the old file
                                                let _ =
                                                    self.profile_manager.delete_profile(&old_name);
                                                // Update app config if this was the active profile
                                                if self.app_config.current_profile == old_name {
                                                    self.app_config.current_profile =
                                                        new_name.clone();
                                                    let _ = self
                                                        .profile_manager
                                                        .save_app_config(&self.app_config);
                                                }
                                                self.refresh_profiles();
                                                self.status_message = Some(format!(
                                                    "Renamed: {} -> {}",
                                                    old_name, new_name
                                                ));
                                            }
                                            Err(e) => {
                                                // Restore old name — old file still on disk
                                                profile.name = old_name;
                                                self.status_message =
                                                    Some(format!("Failed to save profile: {}", e));
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        self.sync_editing_to_selected();
                        self.edit_field = EditField::None;
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
        self.env_menu
            .set_items_count(Self::PROFILE_SETTINGS_COUNT + self.env_vars_list.len() + 1);

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
                let item = MAIN_MENU.get(self.main_menu.selected).map(|(id, _, _)| *id);
                match item {
                    Some(MainMenuItem::Start) => {
                        if let Some(profile) = &self.selected_profile {
                            return Ok(Some(AppAction::Launch(Box::new(LaunchRequest {
                                profile: profile.clone(),
                            }))));
                        } else {
                            self.status_message = Some("No profile selected!".to_string());
                        }
                    }
                    Some(MainMenuItem::Profiles) => {
                        self.trigger_screen_animation(true, Screen::Profiles);
                        self.pending_screen = Some(Screen::Profiles);
                    }
                    Some(MainMenuItem::Versions) => {
                        self.trigger_screen_animation(true, Screen::VersionManagement);
                        self.pending_screen = Some(Screen::VersionManagement);
                    }
                    Some(MainMenuItem::Features) => {
                        self.discovered_plugins = crate::config::discover_plugins();
                        self.feature_menu = MenuState::new(self.discovered_plugins.len());
                        self.trigger_screen_animation(true, Screen::Features);
                        self.pending_screen = Some(Screen::Features);
                    }
                    Some(MainMenuItem::Help) => {
                        self.help_return_screen = Some(Screen::Main);
                        self.trigger_screen_animation(true, Screen::Help);
                        self.pending_screen = Some(Screen::Help);
                    }
                    Some(MainMenuItem::Quit) => {
                        self.running = false;
                    }
                    None => {}
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

    fn handle_version_input(
        &mut self,
        action: NavAction,
        key: KeyEvent,
    ) -> io::Result<Option<AppAction>> {
        // Handle npm install dialog if open
        if self.npm_dialog_open {
            let mut accepted = false;
            let mut rejected = false;
            match action {
                NavAction::Select => accepted = true,
                NavAction::Back | NavAction::Quit => rejected = true,
                _ => {}
            }
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => accepted = true,
                KeyCode::Char('n') | KeyCode::Char('N') => rejected = true,
                _ => {}
            }
            if accepted {
                self.npm_dialog_open = false;
                // Install nvm + node in background, then retry
                self.status_message = Some("Installing Node.js via nvm...".into());
                let pending = self.npm_dialog_pending.take();
                let (log_tx, log_rx) = std::sync::mpsc::channel::<String>();
                // Show install log
                self.install_log_lines.clear();
                self.show_install_log = true;
                // Bridge log lines
                let (step_tx, step_rx) = std::sync::mpsc::channel();
                let step_tx2 = step_tx.clone();
                // Capture agent/version BEFORE moving `pending` into the thread below.
                // `.take()` already cleared `npm_dialog_pending`, so we must snapshot here.
                let install_agent_version = pending.as_ref().map(|(a, v)| (a.clone(), v.clone()));
                std::thread::spawn(move || {
                    for line in log_rx {
                        let _ = step_tx2.send(InstallStepResult::LogLine(line));
                    }
                });
                std::thread::spawn(move || {
                    let _ = log_tx.send("Installing nvm...".into());
                    let nvm_ok = std::process::Command::new("bash")
                        .args(["-c", "curl -fsSL https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash"])
                        .output()
                        .is_ok_and(|o| o.status.success());
                    if !nvm_ok {
                        let _ = log_tx.send("Failed to install nvm".into());
                        let _ = step_tx.send(InstallStepResult::InstallComplete(
                            crate::version::InstallResult {
                                success: false,
                                stdout: String::new(),
                                stderr: String::new(),
                                error: Some("Failed to install nvm".into()),
                            },
                        ));
                        return;
                    }
                    let _ = log_tx.send("Installing Node.js LTS...".into());
                    let node_ok = std::process::Command::new("bash")
                        .args(["-c", "export NVM_DIR=\"$HOME/.nvm\" && . \"$NVM_DIR/nvm.sh\" && nvm install --lts"])
                        .output()
                        .is_ok_and(|o| o.status.success());
                    if !node_ok {
                        let _ = log_tx.send("Failed to install Node.js".into());
                        let _ = step_tx.send(InstallStepResult::InstallComplete(
                            crate::version::InstallResult {
                                success: false,
                                stdout: String::new(),
                                stderr: String::new(),
                                error: Some("Failed to install Node.js via nvm".into()),
                            },
                        ));
                        return;
                    }
                    // Find npm and add to PATH
                    if let Ok(output) = std::process::Command::new("bash")
                        .args([
                            "-c",
                            "export NVM_DIR=\"$HOME/.nvm\" && . \"$NVM_DIR/nvm.sh\" && which npm",
                        ])
                        .output()
                    {
                        if output.status.success() {
                            let npm_path =
                                String::from_utf8_lossy(&output.stdout).trim().to_string();
                            if let Some(bin_dir) = std::path::Path::new(&npm_path).parent() {
                                let current = std::env::var("PATH").unwrap_or_default();
                                std::env::set_var(
                                    "PATH",
                                    format!("{}:{}", bin_dir.display(), current),
                                );
                            }
                        }
                    }
                    let _ = log_tx.send("Node.js installed successfully".into());

                    // Now install the pending agent
                    if let Some((agent, version)) = pending {
                        let _ = log_tx.send(format!(
                            "Installing {} v{}...",
                            agent.display_name(),
                            version
                        ));
                        let vm = VersionManager::new();
                        let result = match agent {
                            AgentType::Gemini => {
                                vm.install_gemini_version_streaming(&version, log_tx)
                            }
                            AgentType::OpenCode => {
                                vm.install_opencode_version_streaming(&version, log_tx)
                            }
                            _ => Ok(crate::version::InstallResult {
                                success: false,
                                stdout: String::new(),
                                stderr: String::new(),
                                error: Some("Unexpected agent".into()),
                            }),
                        };
                        let install_result =
                            result.unwrap_or_else(|e| crate::version::InstallResult {
                                success: false,
                                stdout: String::new(),
                                stderr: e.to_string(),
                                error: Some(e.to_string()),
                            });
                        let _ = step_tx.send(InstallStepResult::InstallComplete(install_result));
                    }
                });
                // `npm_dialog_pending` is already None (consumed by `.take()` above).
                // Use the pre-captured agent/version snapshot instead.
                if let Some((agent, version)) = install_agent_version {
                    self.install_state = Some(InstallState {
                        agent_type: agent,
                        version,
                        receiver: step_rx,
                        _handle: std::thread::spawn(|| {}),
                        start_time: std::time::Instant::now(),
                        current_step: InstallStep::Installing,
                        install_result: None,
                    });
                }
            } else if rejected {
                self.npm_dialog_open = false;
                self.npm_dialog_pending = None;
            }
            return Ok(None);
        }

        // Handle conflict dialog if open
        if self.conflict_warning_open {
            let mut accepted = false;
            let mut rejected = false;

            match action {
                NavAction::Select => accepted = true,
                NavAction::Back | NavAction::Quit => rejected = true,
                _ => {}
            }

            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => accepted = true,
                KeyCode::Char('n') | KeyCode::Char('N') => rejected = true,
                _ => {}
            }

            if accepted {
                // Enter / Y: clean up
                let agent_str_owned;
                let agent_str = match &self.version_agent {
                    AgentType::Claude => "claude",
                    AgentType::Codex => "codex",
                    AgentType::Gemini => "gemini",
                    AgentType::OpenCode => "opencode",
                    AgentType::Custom(name) => {
                        agent_str_owned = name.clone();
                        &agent_str_owned
                    }
                };
                let _ = self.version_manager.cleanup_conflicts(agent_str);
                self.conflict_warning_open = false;
                self.conflict_dismissed = true;
                self.conflict_entries.clear();
                self.refresh_versions();
            } else if rejected {
                // Esc / N: close dialog
                self.conflict_warning_open = false;
                self.conflict_dismissed = true;
            }
            return Ok(None);
        }

        // Handle 'gg' two-key sequence for jump-to-top
        if self.g_pending {
            self.g_pending = false;
            if key.code == KeyCode::Char('g') {
                // gg: jump to top of whichever panel is focused
                match self.version_focus {
                    VersionFocus::Unleash => {} // nothing to jump
                    VersionFocus::AgentPicker => self.switch_to_agent_index(0),
                    VersionFocus::VersionList => self.version_menu.select_first(),
                }
                return Ok(None);
            }
            // Not 'g' — fall through to handle the key normally
        }

        // Handle raw key shortcuts that don't map to NavAction
        match key.code {
            KeyCode::Char('s') if self.version_focus != VersionFocus::Unleash => {
                // Manual rescan — bypass TTL cache
                self.force_refresh_versions();
                return Ok(None);
            }
            KeyCode::Char('G') => {
                // Jump to bottom of focused panel
                match self.version_focus {
                    VersionFocus::Unleash => {} // nothing to jump
                    VersionFocus::AgentPicker => {
                        let last = self.available_agents.len().saturating_sub(1);
                        self.switch_to_agent_index(last);
                    }
                    VersionFocus::VersionList => self.version_menu.select_last(),
                }
                return Ok(None);
            }
            KeyCode::Char('g') if self.version_focus != VersionFocus::Unleash => {
                self.g_pending = true;
                return Ok(None);
            }
            _ => {}
        }

        match self.version_focus {
            VersionFocus::Unleash => {
                match action {
                    NavAction::Select => {
                        // Trigger unleash self-update
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
                    NavAction::Down | NavAction::Tab => {
                        self.version_focus = VersionFocus::AgentPicker;
                    }
                    NavAction::Back | NavAction::Quit => {
                        self.trigger_screen_animation(false, Screen::Main);
                        self.pending_screen = Some(Screen::Main);
                    }
                    _ => {}
                }
            }
            VersionFocus::AgentPicker => {
                match action {
                    NavAction::Up | NavAction::Down => {
                        let current_idx = self.available_agents
                            .iter()
                            .position(|a| *a == self.version_agent)
                            .unwrap_or(0);
                        let new_idx = match action {
                            NavAction::Down => (current_idx + 1).min(self.available_agents.len() - 1),
                            NavAction::Up => {
                                if current_idx == 0 {
                                    // At top of agent list, move focus to unleash
                                    self.version_focus = VersionFocus::Unleash;
                                    return Ok(None);
                                }
                                current_idx.saturating_sub(1)
                            }
                            _ => unreachable!(),
                        };
                        if new_idx != current_idx {
                            self.switch_to_agent_index(new_idx);
                        }
                    }
                    NavAction::Tab | NavAction::Select => {
                        self.version_focus = VersionFocus::VersionList;
                    }
                    NavAction::BackTab => {
                        self.version_focus = VersionFocus::Unleash;
                    }
                    NavAction::Back | NavAction::Quit => {
                        self.version_focus = VersionFocus::Unleash;
                    }
                    _ => {}
                }
            }
            VersionFocus::VersionList => {
                match action {
                    NavAction::Up | NavAction::Down => {
                        self.version_menu.handle_action(action);
                    }
                    NavAction::Tab => {
                        self.version_focus = VersionFocus::Unleash;
                    }
                    NavAction::BackTab => {
                        self.version_focus = VersionFocus::AgentPicker;
                    }
                    NavAction::Select => {
                        self.install_version_for_agent();
                    }
                    NavAction::Back | NavAction::Quit => {
                        // Dismiss install log panel first if visible and install is done
                        if self.show_install_log && self.install_state.is_none() {
                            self.show_install_log = false;
                            self.install_log_lines.clear();
                        } else {
                            self.version_focus = VersionFocus::AgentPicker;
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(None)
    }

    /// Switch to the agent at the given index, refreshing versions
    fn switch_to_agent_index(&mut self, idx: usize) {
        if idx >= self.available_agents.len() {
            return;
        }
        self.version_agent = self.available_agents[idx].clone();
        self.agent_picker_menu.selected = idx;
        self.version_menu.selected = 0;
        self.version_menu.scroll_offset = 0;
        self.conflict_dismissed = false;
        self.refresh_versions();
        self.status_message = Some(format!("Switched to {}", self.version_agent.display_name()));
    }

    /// Install the selected version for the current agent with streaming log output
    fn install_version_for_agent(&mut self) {
        if self.install_state.is_some() {
            return;
        }
        if let Some(version_info) = self.versions.get(self.version_menu.selected) {
            // Check if agent needs npm and it's missing
            let needs_npm = matches!(self.version_agent, AgentType::Gemini | AgentType::OpenCode);
            if needs_npm && !VersionManager::has_npm() {
                // Try sourcing nvm first
                if let Ok(output) = std::process::Command::new("bash")
                    .args(["-c", "export NVM_DIR=\"$HOME/.nvm\" && . \"$NVM_DIR/nvm.sh\" 2>/dev/null && which npm"])
                    .output()
                {
                    if output.status.success() {
                        let npm_path = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        if let Some(bin_dir) = std::path::Path::new(&npm_path).parent() {
                            let current_path = std::env::var("PATH").unwrap_or_default();
                            std::env::set_var("PATH", format!("{}:{}", bin_dir.display(), current_path));
                        }
                    }
                }
                // Still no npm? Show dialog
                if !VersionManager::has_npm() {
                    self.npm_dialog_open = true;
                    self.npm_dialog_pending =
                        Some((self.version_agent.clone(), version_info.version.clone()));
                    return;
                }
            }
            let version = version_info.version.clone();
            let agent = self.version_agent.clone();
            let is_reinstall = version_info.is_installed;

            self.selected_version = Some(version.clone());
            self.installing_version_index = Some(self.version_menu.selected);
            self.install_log_lines.clear();
            self.show_install_log = true;

            let action = if is_reinstall {
                "Reinstalling"
            } else {
                "Installing"
            };
            self.status_message = Some(format!(
                "{} {} v{}...",
                action,
                agent.display_name(),
                version
            ));

            let (tx, rx) = mpsc::channel();

            let version_clone = version.clone();
            let agent_for_state = agent.clone();
            let handle = thread::spawn(move || {
                // Skip real downloads in test mode to prevent overwriting real installations
                if std::env::var("UNLEASH_SKIP_NATIVE_INSTALL").is_ok() {
                    let _ = tx.send(InstallStepResult::InstallComplete(InstallResult {
                        success: true,
                        stdout: "skipped (test mode)".into(),
                        stderr: String::new(),
                        error: None,
                    }));
                    return;
                }
                let vm = VersionManager::new();

                // Bridge channel: forward String log lines as InstallStepResult::LogLine
                let (log_tx, log_rx) = mpsc::channel::<String>();
                let tx_bridge = tx.clone();
                let bridge = thread::spawn(move || {
                    for line in log_rx {
                        let _ = tx_bridge.send(InstallStepResult::LogLine(line));
                    }
                });

                let result = match agent {
                    AgentType::Claude => vm.install_version_streaming(&version_clone, log_tx),
                    AgentType::Codex => vm.install_codex_version_streaming(&version_clone, log_tx),
                    AgentType::Gemini => {
                        vm.install_gemini_version_streaming(&version_clone, log_tx)
                    }
                    AgentType::OpenCode => {
                        vm.install_opencode_version_streaming(&version_clone, log_tx)
                    }
                    AgentType::Custom(_) => Ok(InstallResult {
                        success: false,
                        stdout: String::new(),
                        stderr: String::new(),
                        error: Some("Version management not yet supported for custom agents".into()),
                    }),
                };

                // Wait for bridge to flush all log lines before sending completion
                let _ = bridge.join();

                let install_result = result.unwrap_or_else(|e| InstallResult {
                    success: false,
                    stdout: String::new(),
                    stderr: e.to_string(),
                    error: Some(e.to_string()),
                });
                let _ = tx.send(InstallStepResult::InstallComplete(install_result));
            });

            self.install_state = Some(InstallState {
                agent_type: agent_for_state,
                version,
                receiver: rx,
                _handle: handle,
                start_time: Instant::now(),
                current_step: InstallStep::Installing,
                install_result: None,
            });
        }
    }

    /// Resolve the actual profile index from the current filtered selection
    fn selected_profile_index(&self) -> Option<usize> {
        let filtered: Vec<usize> = if self.profile_search_query.is_empty() {
            (0..self.profiles.len()).collect()
        } else {
            let query = self.profile_search_query.to_lowercase();
            self.profiles
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.name.to_lowercase().contains(&query)
                        || p.description.to_lowercase().contains(&query)
                })
                .map(|(i, _)| i)
                .collect()
        };
        filtered.get(self.profile_menu.selected).copied()
    }

    fn handle_profiles_input(&mut self, action: NavAction, key: KeyEvent) {
        // Search mode: capture typed characters
        if self.profile_search_active {
            match key.code {
                KeyCode::Esc => {
                    self.profile_search_active = false;
                    self.profile_search_query.clear();
                    self.profile_menu.selected = 0;
                    self.profile_menu.scroll_offset = 0;
                    return;
                }
                KeyCode::Enter => {
                    self.profile_search_active = false;
                    // Keep filter, proceed with selection
                    return;
                }
                KeyCode::Backspace => {
                    self.profile_search_query.pop();
                    self.profile_menu.selected = 0;
                    self.profile_menu.scroll_offset = 0;
                    return;
                }
                KeyCode::Char(c) => {
                    self.profile_search_query.push(c);
                    self.profile_menu.selected = 0;
                    self.profile_menu.scroll_offset = 0;
                    return;
                }
                KeyCode::Up => {
                    self.profile_menu.select_prev();
                    return;
                }
                KeyCode::Down => {
                    self.profile_menu.select_next();
                    return;
                }
                _ => return,
            }
        }

        // Activate search with '/'
        if key.code == KeyCode::Char('/') {
            self.profile_search_active = true;
            self.profile_search_query.clear();
            self.profile_menu.selected = 0;
            self.profile_menu.scroll_offset = 0;
            return;
        }

        // Duplicate with 'D' (uppercase)
        if key.code == KeyCode::Char('D') {
            if let Some(idx) = self.selected_profile_index() {
                if let Some(source) = self.profiles.get(idx).cloned() {
                    let new_name = format!("{}-copy", source.name);
                    let mut new_profile = source.clone();
                    new_profile.name = new_name.clone();
                    if self.profile_manager.save_profile(&new_profile).is_ok() {
                        self.refresh_profiles();
                        self.status_message =
                            Some(format!("Duplicated: {} -> {}", source.name, new_name));
                    }
                }
            }
            return;
        }

        match action {
            NavAction::Up | NavAction::Down => {
                self.profile_menu.handle_action(action);
            }
            NavAction::Select | NavAction::Edit => {
                if let Some(idx) = self.selected_profile_index() {
                    if let Some(profile) = self.profiles.get(idx).cloned() {
                        self.profile_search_query.clear();
                        self.profile_search_active = false;
                        self.load_profile_for_editing(profile);
                        self.screen = Screen::ProfileEdit;
                    }
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
                if let Some(idx) = self.selected_profile_index() {
                    if let Some(profile) = self.profiles.get(idx) {
                        let is_default = Profile::default_profiles()
                            .iter()
                            .any(|p| p.name == profile.name);
                        if !is_default {
                            self.screen = Screen::ConfirmDelete;
                        } else {
                            self.status_message =
                                Some(format!("Cannot delete default profile '{}'", profile.name));
                        }
                    }
                }
            }
            NavAction::Back | NavAction::Quit => {
                if !self.profile_search_query.is_empty() {
                    // First Esc clears the search filter
                    self.profile_search_query.clear();
                    self.profile_menu.selected = 0;
                    self.profile_menu.scroll_offset = 0;
                } else {
                    self.trigger_screen_animation(false, Screen::Main);
                    self.pending_screen = Some(Screen::Main);
                }
            }
            _ => {}
        }
    }

    /// Number of settings fields shown at the top of profile edit
    const PROFILE_SETTINGS_COUNT: usize = 5;

    fn handle_profile_edit_input(&mut self, action: NavAction, _key: KeyEvent) {
        let num_settings = Self::PROFILE_SETTINGS_COUNT;
        let num_env = self.env_vars_list.len();
        let add_new_idx = num_settings + num_env;

        match action {
            NavAction::Up | NavAction::Down => {
                self.env_menu.handle_action(action);
            }
            NavAction::Select | NavAction::Edit => {
                let selected = self.env_menu.selected;
                match selected {
                    0 => {
                        // Edit profile name
                        let current = self
                            .editing_profile
                            .as_ref()
                            .map(|p| p.name.clone())
                            .unwrap_or_default();
                        self.key_input = TextInput::new().with_value(&current);
                        self.edit_field = EditField::ProfileName;
                    }
                    1 => {
                        // Edit Agent CLI path
                        let current = self
                            .editing_profile
                            .as_ref()
                            .map(|p| p.agent_cli_path.clone())
                            .unwrap_or_default();
                        self.key_input = TextInput::new().with_value(&current);
                        self.edit_field = EditField::AgentCliPath;
                    }
                    2 => {
                        // Edit arguments
                        let current = self
                            .editing_profile
                            .as_ref()
                            .map(|p| p.agent_cli_args.join(" "))
                            .unwrap_or_default();
                        self.key_input = TextInput::new().with_value(&current);
                        self.edit_field = EditField::ClaudeArgs;
                    }
                    3 => {
                        // Theme selection — go to Theme sub-screen
                        let theme_str = self
                            .editing_profile
                            .as_ref()
                            .map(|p| p.theme.as_str())
                            .unwrap_or("orange");
                        let theme_color = ThemeColor::from_config(theme_str)
                            .unwrap_or(ThemeColor::Preset(ThemePreset::Orange));
                        let idx = if theme_color.is_custom() {
                            ThemePreset::all().len()
                        } else {
                            ThemePreset::all()
                                .iter()
                                .position(|t| theme_color.is_preset(*t))
                                .unwrap_or(0)
                        };
                        self.theme_menu.selected = idx;
                        self.screen = Screen::Theme;
                    }
                    4 => {
                        // Stop prompt — open in $EDITOR
                        let default_prompt = self.get_default_stop_prompt();
                        let current = self
                            .editing_profile
                            .as_ref()
                            .and_then(|p| p.stop_prompt.clone())
                            .unwrap_or(default_prompt);
                        self.pending_external_edit = Some(current);
                    }
                    idx if idx >= num_settings && idx < add_new_idx => {
                        // Edit existing env var
                        let env_idx = idx - num_settings;
                        let (key, value) = &self.env_vars_list[env_idx];
                        self.key_input = TextInput::new().with_value(key);
                        self.value_input = TextInput::new().with_value(value);
                        if is_sensitive_key(key) {
                            self.value_input.hidden = true;
                        }
                        self.editing_env_index = Some(env_idx);
                        self.edit_field = EditField::EnvKey;
                        self.screen = Screen::EnvVarEdit;
                    }
                    idx if idx == add_new_idx => {
                        // Add new env var
                        self.key_input = TextInput::new().with_placeholder("VARIABLE_NAME");
                        self.value_input = TextInput::new().with_placeholder("value");
                        self.editing_env_index = None;
                        self.edit_field = EditField::EnvKey;
                        self.screen = Screen::EnvVarEdit;
                    }
                    _ => {}
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
                if selected >= num_settings && selected < add_new_idx {
                    let env_idx = selected - num_settings;
                    let key = self.env_vars_list[env_idx].0.clone();
                    self.env_vars_list.remove(env_idx);
                    self.env_menu
                        .set_items_count(num_settings + self.env_vars_list.len() + 1);
                    let _ = self.save_editing_profile();
                    self.status_message = Some(format!("Deleted: {}", key));
                }
            }
            NavAction::ExternalEdit => {
                // Open profile TOML in external editor
                if let Some(ref profile) = self.editing_profile {
                    let path = self
                        .profile_manager
                        .config_dir()
                        .join("profiles")
                        .join(format!("{}.toml", profile.name));
                    self.pending_profile_file_edit = Some(path);
                }
            }
            NavAction::Back | NavAction::Quit => {
                // Activate the edited profile as current
                if let Some(ref profile) = self.editing_profile {
                    let name = profile.name.clone();
                    self.selected_profile = Some(profile.clone());
                    self.app_config.current_profile = name;
                    let _ = self.profile_manager.save_app_config(&self.app_config);
                    self.sync_theme_from_profile();
                }
                self.editing_profile = None;
                self.screen = Screen::Profiles;
            }
            _ => {}
        }
    }

    /// If editing_profile matches selected_profile, copy editing_profile to selected_profile and sync theme.
    /// This is borrow-safe since it doesn't take a reference parameter.
    pub fn sync_editing_to_selected(&mut self) {
        if let Some(ref editing) = self.editing_profile {
            if self.selected_profile.as_ref().map(|p| &p.name) == Some(&editing.name) {
                self.selected_profile = Some(editing.clone());
            }
        }
        self.sync_theme_from_profile();
    }

    /// Derive theme_color from the currently selected profile
    fn sync_theme_from_profile(&mut self) {
        if let Some(ref profile) = self.selected_profile {
            self.theme_color = ThemeColor::from_config(&profile.theme)
                .unwrap_or(ThemeColor::Preset(ThemePreset::Orange));
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

    fn handle_theme_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.theme_menu.handle_action(action);
            }
            NavAction::Select => {
                let presets = ThemePreset::all();
                if let Some(preset) = presets.get(self.theme_menu.selected) {
                    // Selected a preset — save to editing_profile
                    self.theme_color = ThemeColor::Preset(*preset);
                    if let Some(ref mut profile) = self.editing_profile {
                        profile.theme = self.theme_color.to_config();
                        let _ = self.profile_manager.save_profile(profile);
                    }
                    self.sync_editing_to_selected();
                    self.status_message = Some(format!("Theme: {}", preset.display_name()));
                    self.screen = Screen::ProfileEdit;
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
                self.screen = Screen::ProfileEdit;
            }
            _ => {}
        }
    }

    fn handle_features_input(&mut self, action: NavAction) {
        match action {
            NavAction::Up | NavAction::Down => {
                self.feature_menu.handle_action(action);
            }
            NavAction::Select => {
                // Toggle the selected plugin
                if let Some(plugin) = self.discovered_plugins.get(self.feature_menu.selected) {
                    let plugin_name = plugin.name.clone();
                    let all_names: Vec<String> = self
                        .discovered_plugins
                        .iter()
                        .map(|p| p.name.clone())
                        .collect();

                    if self.app_config.enabled_plugins.is_empty() {
                        // Empty = all enabled. To disable one, populate with all-but-selected.
                        self.app_config.enabled_plugins = all_names
                            .into_iter()
                            .filter(|n| *n != plugin_name)
                            .collect();
                    } else if self.app_config.enabled_plugins.contains(&plugin_name) {
                        // Currently enabled — disable it
                        self.app_config
                            .enabled_plugins
                            .retain(|n| *n != plugin_name);
                    } else {
                        // Currently disabled — enable it
                        self.app_config.enabled_plugins.push(plugin_name.clone());
                    }

                    // If all plugins are now enabled, clear the list (back to "all enabled" default)
                    if !self.app_config.enabled_plugins.is_empty() {
                        let all_enabled = self
                            .discovered_plugins
                            .iter()
                            .all(|p| self.app_config.enabled_plugins.contains(&p.name));
                        if all_enabled {
                            self.app_config.enabled_plugins.clear();
                        }
                    }

                    let _ = self.profile_manager.save_app_config(&self.app_config);
                    self.status_message = Some(format!("Toggled: {}", plugin_name));

                    // Immediately sync hooks to ensure any statically injected hooks (from ~/.claude/settings.json)
                    // accurately reflect the enabled/disabled state.
                    if let Ok(manager) = crate::hooks::HookManager::new() {
                        let _ = manager.sync_plugin_hooks(&crate::launcher::find_plugin_dirs());
                    }
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.trigger_screen_animation(false, Screen::Main);
                self.pending_screen = Some(Screen::Main);
                if self.art_animation.is_none() {
                    self.screen = Screen::Main;
                    self.refresh_screen_data();
                }
            }
            _ => {}
        }
    }

    /// Check if a plugin is enabled based on current config
    fn is_plugin_enabled(&self, name: &str) -> bool {
        if self.app_config.enabled_plugins.is_empty() {
            true // empty = all enabled
        } else {
            self.app_config.enabled_plugins.contains(&name.to_string())
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
                // Confirm delete — use selected_profile_index() to account for search filter
                if let Some(idx) = self.selected_profile_index() {
                    if let Some(profile) = self.profiles.get(idx) {
                        let name = profile.name.clone();
                        if self.profile_manager.delete_profile(&name).is_ok() {
                            self.refresh_profiles();
                            self.status_message = Some(format!("Deleted: {}", name));
                        }
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

    /// Calculate the minimum content width needed for the current screen
    fn content_width(&self) -> u16 {
        self.content_width_for_screen(self.screen)
    }

    /// Calculate the minimum content width needed for a specific screen
    fn content_width_for_screen(&self, screen: Screen) -> u16 {
        match screen {
            Screen::Main => {
                let max_name = MAIN_MENU.iter().map(|(_, n, _)| n.len()).max().unwrap_or(0);
                let max_desc = MAIN_MENU.iter().map(|(_, _, d)| d.len()).max().unwrap_or(0);
                // "> " prefix (2) + name, or "    " prefix (4) + desc
                let name_width = 2 + max_name;
                let desc_width = 4 + max_desc;
                (name_width.max(desc_width) + 2) as u16
            }
            Screen::Profiles | Screen::ConfirmDelete => {
                // Based on profile names + " *" marker + "    X env vars"
                let max_name = self
                    .profiles
                    .iter()
                    .map(|p| p.name.len())
                    .max()
                    .unwrap_or(10);
                let name_width = 2 + max_name + 2; // "> " + name + " *"
                let desc_width = 4 + 12; // "    X env vars"
                (name_width.max(desc_width) + 2) as u16
            }
            Screen::Theme => {
                // Theme list with color swatches
                35
            }
            Screen::Help => {
                // Help screen has fixed text
                40
            }
            Screen::VersionManagement => {
                // Wider to fit agent versions
                55
            }
            Screen::ProfileEdit | Screen::EnvVarEdit => {
                // Profile editing needs more space for env var keys/values
                50
            }
            Screen::Features => 50,
        }
    }

    /// Render the UI
    pub fn render(&mut self, frame: &mut Frame) {
        // Clear clickable areas from the previous frame before registering new ones
        self.clickable_areas.clear();

        self.last_frame_area = frame.area();
        // Main layout: content area + status bar at bottom
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(10), Constraint::Length(3)])
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
            let agent = self.app_config.current_profile.as_str();
            let shift = self.theme_color.theme_shift();
            let art_lines: Vec<Line> = if self.is_gemini_profile() {
                let gradient = crate::theme::GradientTheme::gemini();
                mascots::unleashed_claude_full_ratatui_gradient(max_lines, &gradient)
            } else if !shift.is_identity() {
                mascots::full_ratatui_themed(agent, max_lines, shift)
            } else {
                mascots::full_ratatui(agent, max_lines)
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
                self.clickable_areas
                    .push((content_chunks[0], ClickTarget::AvatarArt));
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
                self.clickable_areas
                    .push((content_chunks[1], ClickTarget::AvatarArt));
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
            Screen::Theme => self.render_theme(frame, area),
            Screen::Help => self.render_help(frame, area),
            Screen::ConfirmDelete => {
                self.render_profiles(frame, area);
                self.render_confirm_delete_dialog(frame, frame.area());
            }
            Screen::VersionManagement => {
                self.render_version_management(frame, area);
                if self.npm_dialog_open {
                    self.render_npm_dialog(frame, frame.area());
                } else if self.conflict_warning_open {
                    self.render_conflict_dialog(frame, frame.area());
                }
            }
            Screen::Features => self.render_features(frame, area),
        }
    }

    /// Check if the current profile is a Gemini agent (uses gradient)
    fn is_gemini_profile(&self) -> bool {
        self.selected_profile
            .as_ref()
            .and_then(|p| p.agent_type())
            .is_some_and(|t| t == crate::agents::AgentType::Gemini)
    }

    fn render_art_sidebar(&self, frame: &mut Frame, area: Rect) {
        // Render mascot ANSI art (right-facing) for the current agent profile
        // Lava lamp mode is an easter egg triggered by Konami code (idea by cac taurus)
        let max_lines = area.height as usize;
        let agent = self.app_config.current_profile.as_str();
        let shift = self.theme_color.theme_shift();
        let art_lines: Vec<Line> = if self.lava_mode {
            mascots::right_ratatui_lava(agent, max_lines, self.animation_frame)
        } else if self.is_gemini_profile() {
            let gradient = crate::theme::GradientTheme::gemini();
            mascots::unleashed_claude_ratatui_gradient(max_lines, &gradient)
        } else if !shift.is_identity() {
            mascots::right_ratatui_themed(agent, max_lines, shift)
        } else {
            mascots::right_ratatui(agent, max_lines)
        };
        let art_widget = Paragraph::new(art_lines);
        frame.render_widget(art_widget, area);
    }

    fn render_art_sidebar_left(&self, frame: &mut Frame, area: Rect) {
        // Render mascot ANSI art (left-facing) for the current agent profile
        // Lava lamp mode is an easter egg triggered by Konami code (idea by cac taurus)
        let max_lines = area.height as usize;
        let agent = self.app_config.current_profile.as_str();
        let shift = self.theme_color.theme_shift();
        let art_lines: Vec<Line> = if self.lava_mode {
            mascots::left_ratatui_lava(agent, max_lines, self.animation_frame)
        } else if self.is_gemini_profile() {
            let gradient = crate::theme::GradientTheme::gemini();
            mascots::unleashed_claude_left_ratatui_gradient(max_lines, &gradient)
        } else if !shift.is_identity() {
            mascots::left_ratatui_themed(agent, max_lines, shift)
        } else {
            mascots::left_ratatui(agent, max_lines)
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
                "unleash",
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!(
                    "Profile: {}",
                    self.selected_profile
                        .as_ref()
                        .map(|p| p.name.as_str())
                        .unwrap_or("none")
                ),
                Style::default().fg(Color::Yellow),
            )),
        ];
        let title = Paragraph::new(title_text);
        frame.render_widget(title, chunks[0]);

        // Each menu item takes 2 lines, calculate visible count
        // Area height minus 2 for borders, divided by 2 for lines per item
        let menu_area = chunks[1];
        let visible_items = (menu_area.height.saturating_sub(2) / 2) as usize;

        // Ensure selected item is visible
        self.main_menu.ensure_visible(visible_items);
        let scroll_offset = self.main_menu.scroll_offset;

        let items: Vec<ListItem> = MAIN_MENU
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_items)
            .map(|(i, (_, name, desc))| {
                let style = if i == self.main_menu.selected {
                    Style::default()
                        .fg(self.accent_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };
                let prefix = if i == self.main_menu.selected {
                    "> "
                } else {
                    "  "
                };
                ListItem::new(vec![
                    Line::from(Span::styled(format!("{}{}", prefix, name), style)),
                    Line::from(Span::styled(
                        format!("    {}", desc),
                        Style::default().fg(Color::DarkGray),
                    )),
                ])
            })
            .collect();

        // Show scroll indicator if needed
        let _scroll_hint = if MAIN_MENU.len() > visible_items {
            format!(
                " [{}/{}]",
                scroll_offset + 1,
                MAIN_MENU.len().saturating_sub(visible_items) + 1
            )
        } else {
            String::new()
        };

        // Register clickable areas for mouse: each item takes 2 rows
        let visible_count = visible_items.min(MAIN_MENU.len().saturating_sub(scroll_offset));
        for j in 0..visible_count {
            let item_idx = scroll_offset + j;
            let row = menu_area.y + (j as u16 * 2);
            if row < menu_area.y + menu_area.height {
                let height = 2.min(menu_area.y + menu_area.height - row);
                self.clickable_areas.push((
                    Rect::new(menu_area.x, row, menu_area.width, height),
                    ClickTarget::MainMenuItem(item_idx),
                ));
            }
        }

        let menu = List::new(items);
        frame.render_widget(menu, menu_area);
    }

    fn render_profiles(&mut self, frame: &mut Frame, area: Rect) {
        let key_style = Style::default()
            .fg(self.accent_color())
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(Color::DarkGray);

        // Switch to vertical layout when terminal is too narrow for horizontal hints
        let vertical_hints = area.width < 34;
        let hint_height = if vertical_hints { 6 } else { 2 };

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(hint_height), Constraint::Min(1)])
            .split(area);

        // Filter profiles by search query
        let filtered_indices: Vec<usize> = if self.profile_search_query.is_empty() {
            (0..self.profiles.len()).collect()
        } else {
            let query = self.profile_search_query.to_lowercase();
            self.profiles
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.name.to_lowercase().contains(&query)
                        || p.description.to_lowercase().contains(&query)
                })
                .map(|(i, _)| i)
                .collect()
        };

        // Update menu item count to match filtered list
        self.profile_menu.set_items_count(filtered_indices.len());

        // Each profile item takes 2 lines, calculate visible count
        let list_area = chunks[1];

        // Reserve lines for search bar and scroll indicators
        let search_height: u16 =
            if self.profile_search_active || !self.profile_search_query.is_empty() {
                2
            } else {
                0
            };
        let available_height = list_area.height.saturating_sub(search_height);
        let visible_items = (available_height / 2) as usize;

        // Ensure selected item is visible (scrolls to keep selection in view)
        self.profile_menu.ensure_visible(visible_items);
        let scroll_offset = self.profile_menu.scroll_offset;

        let total = filtered_indices.len();
        let has_above = scroll_offset > 0;
        let has_below = scroll_offset + visible_items < total;

        // Build profile item lines (2 lines per profile: name + env count)
        let mut item_lines: Vec<(Line, Line)> = Vec::new();
        for (filter_idx, &profile_idx) in filtered_indices
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(visible_items)
        {
            let profile = &self.profiles[profile_idx];
            let is_current = self
                .selected_profile
                .as_ref()
                .is_some_and(|p| p.name == profile.name);
            let style = if filter_idx == self.profile_menu.selected {
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else if is_current {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };
            let prefix = if filter_idx == self.profile_menu.selected {
                "> "
            } else {
                "  "
            };
            let current_marker = if is_current { " *" } else { "" };
            let env_count = profile.env.len();
            item_lines.push((
                Line::from(Span::styled(
                    format!("{}{}{}", prefix, profile.name, current_marker),
                    style,
                )),
                Line::from(Span::styled(
                    format!("    {} env vars", env_count),
                    Style::default().fg(Color::DarkGray),
                )),
            ));
        }

        let hints = if vertical_hints {
            Paragraph::new(vec![
                Line::from(vec![
                    Span::styled(" n", key_style),
                    Span::styled(" new  ", desc_style),
                    Span::styled("D", key_style),
                    Span::styled(" dup", desc_style),
                ]),
                Line::from(vec![
                    Span::styled(" e", key_style),
                    Span::styled(" edit  ", desc_style),
                    Span::styled("d", key_style),
                    Span::styled(" del", desc_style),
                ]),
                Line::from(vec![
                    Span::styled(" /", key_style),
                    Span::styled(" search", desc_style),
                ]),
                Line::from(vec![
                    Span::styled(" esc", key_style),
                    Span::styled(" back", desc_style),
                ]),
            ])
        } else {
            Paragraph::new(Line::from(vec![
                Span::styled(" n", key_style),
                Span::styled(" new  ", desc_style),
                Span::styled("D", key_style),
                Span::styled(" dup  ", desc_style),
                Span::styled("e", key_style),
                Span::styled(" edit  ", desc_style),
                Span::styled("d", key_style),
                Span::styled(" delete  ", desc_style),
                Span::styled("/", key_style),
                Span::styled(" search  ", desc_style),
                Span::styled("esc", key_style),
                Span::styled(" back", desc_style),
            ]))
        };
        frame.render_widget(hints, chunks[0]);

        // Build lines for rendering (scroll indicators + items + search bar)
        let mut lines: Vec<Line> = Vec::new();

        if has_above {
            lines.push(Line::from(Span::styled(
                format!("  \u{25b2} {} more", scroll_offset),
                Style::default().fg(Color::DarkGray),
            )));
            lines.push(Line::from(""));
        }

        // Render profile items as raw lines (2 per item)
        for (name_line, env_line) in &item_lines {
            lines.push(name_line.clone());
            lines.push(env_line.clone());
        }

        if has_below {
            let remaining = total - scroll_offset - visible_items;
            lines.push(Line::from(Span::styled(
                format!("  \u{25bc} {} more", remaining),
                Style::default().fg(Color::DarkGray),
            )));
        }

        // Register clickable areas: each profile item takes 2 rows
        let click_y_offset: u16 = if has_above { 2 } else { 0 };
        let visible_count = visible_items.min(total.saturating_sub(scroll_offset));
        for j in 0..visible_count {
            let filter_idx = scroll_offset + j;
            if filter_idx < filtered_indices.len() {
                let profile_idx = filtered_indices[filter_idx];
                let row = list_area.y + click_y_offset + (j as u16 * 2);
                if row >= list_area.y + list_area.height {
                    break;
                }
                let height = 2.min(list_area.y + list_area.height - row);
                self.clickable_areas.push((
                    Rect::new(list_area.x, row, list_area.width, height),
                    ClickTarget::ProfileItem(profile_idx),
                ));
            }
        }

        // Search bar
        if self.profile_search_active || !self.profile_search_query.is_empty() {
            lines.push(Line::from(""));
            let search_prefix = Span::styled(" / ", key_style);
            let query_text = if self.profile_search_query.is_empty() && self.profile_search_active {
                Span::styled("type to filter...", Style::default().fg(Color::DarkGray))
            } else {
                Span::styled(self.profile_search_query.clone(), Style::default())
            };
            let cursor = if self.profile_search_active {
                Span::styled("\u{2588}", Style::default().fg(self.accent_color()))
            } else {
                Span::raw("")
            };
            lines.push(Line::from(vec![search_prefix, query_text, cursor]));
        }

        let content = Paragraph::new(lines);
        frame.render_widget(content, list_area);
    }

    fn render_profile_edit(&mut self, frame: &mut Frame, area: Rect) {
        let profile = match &self.editing_profile {
            Some(p) => p,
            None => return,
        };

        let num_settings = Self::PROFILE_SETTINGS_COUNT;

        let key_style = Style::default()
            .fg(self.accent_color())
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(Color::DarkGray);

        // Split area: hints, settings section, then env vars section
        let outer_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),                       // Key hints
                Constraint::Length(num_settings as u16 + 1), // Settings + separator
                Constraint::Min(3),                          // Env vars
            ])
            .split(area);

        let hints = Paragraph::new(Line::from(vec![
            Span::styled(" o", key_style),
            Span::styled(" open in $EDITOR  ", desc_style),
            Span::styled("n", key_style),
            Span::styled(" new env  ", desc_style),
            Span::styled("d", key_style),
            Span::styled(" delete  ", desc_style),
            Span::styled("esc", key_style),
            Span::styled(" back", desc_style),
        ]));
        frame.render_widget(hints, outer_chunks[0]);

        let chunks = [outer_chunks[1], outer_chunks[2]];

        // --- Settings fields (indices 0-4) ---
        let settings: Vec<(&str, String)> = vec![
            ("Name", profile.name.clone()),
            ("Agent CLI", profile.agent_cli_path.clone()),
            (
                "Arguments",
                if profile.agent_cli_args.is_empty() {
                    "(none)".to_string()
                } else {
                    profile.agent_cli_args.join(" ")
                },
            ),
            ("Theme", {
                ThemeColor::from_config(&profile.theme)
                    .map(|tc| match tc {
                        ThemeColor::Preset(p) => p.display_name().to_string(),
                        ThemeColor::Custom(r, g, b) => format!("#{:02X}{:02X}{:02X}", r, g, b),
                    })
                    .unwrap_or_else(|| profile.theme.clone())
            }),
            (
                "Stop Prompt",
                profile
                    .stop_prompt
                    .clone()
                    .unwrap_or_else(|| "(default)".to_string()),
            ),
        ];

        let mut settings_items: Vec<ListItem> = Vec::new();
        for (i, (name, value)) in settings.iter().enumerate() {
            let is_selected = i == self.env_menu.selected;
            let is_editing = is_selected
                && match i {
                    0 => self.edit_field == EditField::ProfileName,
                    1 => self.edit_field == EditField::AgentCliPath,
                    2 => self.edit_field == EditField::ClaudeArgs,
                    4 => self.edit_field == EditField::StopPrompt,
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

            let value_style = if is_editing {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(Color::Cyan)
            };

            // Calculate available width for the value (area width - prefix - label - ": ")
            let label_width = prefix.len() + name.len() + 2; // 2 for ": "
            let max_value_width = (area.width as usize).saturating_sub(label_width + 1);

            let mut spans = vec![
                Span::styled(prefix, style),
                Span::styled(*name, style),
                Span::styled(": ", Style::default().fg(Color::DarkGray)),
            ];

            if is_editing {
                self.key_input.set_viewport_width(max_value_width);
                let (before, at_cursor, after) = self.key_input.render_parts();
                let cursor_style = Style::default().fg(Color::Black).bg(Color::Yellow);
                spans.push(Span::styled(before, value_style));
                match at_cursor {
                    Some(c) => spans.push(Span::styled(c.to_string(), cursor_style)),
                    None => spans.push(Span::styled(" ", cursor_style)),
                }
                spans.push(Span::styled(after, value_style));
            } else {
                // Truncate display value to fit available width
                let display_value =
                    if value.chars().count() > max_value_width && max_value_width > 1 {
                        let truncated: String = value.chars().take(max_value_width - 1).collect();
                        format!("{}\u{2026}", truncated)
                    } else {
                        value.clone()
                    };
                spans.push(Span::styled(display_value, value_style));
            }

            settings_items.push(ListItem::new(Line::from(spans)));
        }

        // Separator line below settings
        settings_items.push(ListItem::new(Line::from(Span::styled(
            "  --- Environment Variables ---",
            Style::default().fg(Color::DarkGray),
        ))));

        // Register clickable areas for settings (1 row each)
        let settings_area = chunks[0];
        for i in 0..settings.len() {
            let row = settings_area.y + i as u16;
            if row < settings_area.y + settings_area.height {
                self.clickable_areas.push((
                    Rect::new(settings_area.x, row, settings_area.width, 1),
                    ClickTarget::ProfileEditItem(i),
                ));
            }
        }

        let settings_list = List::new(settings_items);
        frame.render_widget(settings_list, chunks[0]);

        // --- Env vars + Add new (separate list, separate area) ---
        let mut env_items: Vec<ListItem> = Vec::new();

        for (i, (key, value)) in self.env_vars_list.iter().enumerate() {
            let menu_idx = num_settings + i;
            let is_selected = menu_idx == self.env_menu.selected;
            let style = if is_selected {
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };
            let prefix = if is_selected { "> " } else { "  " };

            let raw_value = if is_sensitive_key(key) {
                censor_sensitive(value, 7, 4)
            } else {
                value.clone()
            };

            // Truncate env var value to fit available width
            let env_label_width = prefix.len() + key.len() + 1; // 1 for "="
            let max_env_width = (area.width as usize).saturating_sub(env_label_width + 1);
            let display_value = if raw_value.chars().count() > max_env_width && max_env_width > 1 {
                let truncated: String = raw_value.chars().take(max_env_width - 1).collect();
                format!("{}\u{2026}", truncated)
            } else {
                raw_value
            };

            env_items.push(ListItem::new(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(key, style),
                Span::styled("=", Style::default().fg(Color::DarkGray)),
                Span::styled(display_value, Style::default().fg(Color::Cyan)),
            ])));
        }

        // Add new variable option
        let add_idx = num_settings + self.env_vars_list.len();
        let add_style = if self.env_menu.selected == add_idx {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::Green)
        };
        let add_prefix = if self.env_menu.selected == add_idx {
            "> "
        } else {
            "  "
        };
        env_items.push(ListItem::new(Line::from(Span::styled(
            format!("{}+ Add new variable", add_prefix),
            add_style,
        ))));

        // Register clickable areas for env vars and "Add new" (1 row each)
        let env_area = chunks[1];
        for i in 0..=self.env_vars_list.len() {
            let menu_idx = num_settings + i;
            let row = env_area.y + i as u16;
            if row < env_area.y + env_area.height {
                self.clickable_areas.push((
                    Rect::new(env_area.x, row, env_area.width, 1),
                    ClickTarget::ProfileEditItem(menu_idx),
                ));
            }
        }

        let env_list = List::new(env_items);
        frame.render_widget(env_list, chunks[1]);
    }

    fn render_env_var_dialog(&self, frame: &mut Frame, area: Rect) {
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = 9;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let _title = if self.editing_env_index.is_some() {
            " Edit Variable "
        } else {
            " New Variable "
        };

        let key_style = if self.edit_field == EditField::EnvKey {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let value_style = if self.edit_field == EditField::EnvValue {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };

        let cursor_style = Style::default().fg(Color::Black).bg(Color::Yellow);

        // Build key field spans with proper cursor positioning
        let mut key_spans = vec![Span::styled("  Key:   ", Style::default())];
        if self.edit_field == EditField::EnvKey {
            let (before, at_cursor, after) = self.key_input.render_parts();
            if before.is_empty() && at_cursor.is_none() && self.key_input.is_empty() {
                // Show placeholder with cursor
                key_spans.push(Span::styled(" ", cursor_style));
                key_spans.push(Span::styled(
                    &self.key_input.placeholder,
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                key_spans.push(Span::styled(before, key_style));
                match at_cursor {
                    Some(c) => key_spans.push(Span::styled(c.to_string(), cursor_style)),
                    None => key_spans.push(Span::styled(" ", cursor_style)),
                }
                key_spans.push(Span::styled(after, key_style));
            }
        } else if self.key_input.is_empty() {
            key_spans.push(Span::styled(
                &self.key_input.placeholder,
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            key_spans.push(Span::styled(&self.key_input.value, key_style));
        }

        // Build value field spans with proper cursor positioning
        let mut value_spans = vec![Span::styled("  Value: ", Style::default())];
        if self.edit_field == EditField::EnvValue {
            let (before, at_cursor, after) = self.value_input.render_parts();
            if before.is_empty() && at_cursor.is_none() && self.value_input.is_empty() {
                // Show placeholder with cursor
                value_spans.push(Span::styled(" ", cursor_style));
                value_spans.push(Span::styled(
                    &self.value_input.placeholder,
                    Style::default().fg(Color::DarkGray),
                ));
            } else {
                value_spans.push(Span::styled(before, value_style));
                match at_cursor {
                    Some(c) => value_spans.push(Span::styled(c.to_string(), cursor_style)),
                    None => value_spans.push(Span::styled(" ", cursor_style)),
                }
                value_spans.push(Span::styled(after, value_style));
            }
        } else if self.value_input.is_empty() {
            value_spans.push(Span::styled(
                &self.value_input.placeholder,
                Style::default().fg(Color::DarkGray),
            ));
        } else if self.value_input.hidden {
            // Show censored when not actively editing
            value_spans.push(Span::styled(
                censor_sensitive(&self.value_input.value, 7, 4),
                value_style,
            ));
        } else {
            value_spans.push(Span::styled(&self.value_input.value, value_style));
        }

        let lines = vec![
            Line::from(""),
            Line::from(key_spans),
            Line::from(""),
            Line::from(value_spans),
            Line::from(""),
            Line::from(Span::styled(
                "  [Tab=switch field] [Enter=save] [Esc=cancel]",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let dialog = Paragraph::new(lines)
            .block(Block::default().style(Style::default().bg(Color::Black)))
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
            .block(Block::default().style(Style::default().bg(Color::Black).fg(Color::Red)));

        frame.render_widget(dialog, dialog_area);
    }

    fn render_npm_dialog(&self, frame: &mut Frame, area: Rect) {
        let dialog_width = 50.min(area.width.saturating_sub(4));
        let dialog_height = 8;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;
        let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let agent_name = self
            .npm_dialog_pending
            .as_ref()
            .map(|(a, _)| a.display_name())
            .unwrap_or(std::borrow::Cow::Borrowed("this agent"));

        let lines = vec![
            Line::default(),
            Line::from(Span::styled(
                "  npm not found",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::default(),
            Line::from(format!("  Node.js is required to install {}.", agent_name)),
            Line::default(),
            Line::from("  Install Node.js via nvm?"),
            Line::default(),
            Line::from(Span::styled(
                "  [Y=install] [N/Esc=cancel]",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let dialog = Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Install Node.js "),
        );
        frame.render_widget(dialog, dialog_area);
    }

    fn render_conflict_dialog(&mut self, frame: &mut Frame, area: Rect) {
        // Build the conflict entry lines dynamically
        let mut entry_lines: Vec<Line<'static>> = Vec::new();
        for entry in &self.conflict_entries {
            let path_str = entry.path.display().to_string();
            let ver = if entry.version.is_empty() {
                "unknown".to_string()
            } else {
                entry.version.clone()
            };
            let active_marker = if entry.active { " (active)" } else { "" };
            entry_lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    entry.source.clone(),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" v{} {}{}", ver, path_str, active_marker)),
            ]));
        }

        // Total height: 1 blank + 1 title + 1 subtitle + 1 blank
        //             + N entries + 1 blank + 1 prompt + 1 blank + 1 buttons + 2 border = 9 + N
        let entry_count = entry_lines.len() as u16;
        let dialog_width = 72.min(area.width.saturating_sub(4));
        let dialog_height = (9 + entry_count).min(area.height.saturating_sub(2));
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let mut lines: Vec<Line<'static>> = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Warning: Multiple conflicting installations detected!",
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::Yellow),
            )),
            Line::from("  Different install methods may collide and cause issues."),
            Line::from(""),
        ];

        // Append each conflict entry
        lines.append(&mut entry_lines);

        lines.push(Line::from(""));
        lines.push(Line::from(
            "  Do you want to clean up the conflicting installs?",
        ));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                "[Y] Yes (Clean up)",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("    "),
            Span::styled(
                "[N] No (Ignore)",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ]));

        let dialog = Paragraph::new(lines).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(" Conflict Warning — [Y]es / [N]o ")
                .title_alignment(Alignment::Center)
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .style(Style::default().bg(Color::Black)),
        );

        frame.render_widget(dialog, dialog_area);

        // Register clickable areas for Yes/No buttons
        // Buttons are on the last content line before the bottom border
        let button_row = dialog_y + dialog_height - 2;
        let yes_rect = Rect::new(dialog_x + 3, button_row, 18, 1);
        let no_rect = Rect::new(dialog_x + 25, button_row, 15, 1);

        self.clickable_areas
            .push((yes_rect, ClickTarget::DialogYes));
        self.clickable_areas.push((no_rect, ClickTarget::DialogNo));
    }

    fn render_features(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Title
                Constraint::Min(5),    // Plugin list
                Constraint::Length(2), // Help hint
            ])
            .split(area);

        // Title
        let title = Paragraph::new(vec![
            Line::from(Span::styled(
                "Features & Plugins",
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
        ]);
        frame.render_widget(title, chunks[0]);

        if self.discovered_plugins.is_empty() {
            let empty = Paragraph::new(Span::styled(
                "  No plugins found",
                Style::default().fg(Color::DarkGray),
            ));
            frame.render_widget(empty, chunks[1]);
        } else {
            let items: Vec<ListItem> = self
                .discovered_plugins
                .iter()
                .enumerate()
                .map(|(i, plugin)| {
                    let is_selected = i == self.feature_menu.selected;
                    let is_on = self.is_plugin_enabled(&plugin.name);

                    let checkbox = if is_on { "[x]" } else { "[ ]" };
                    let prefix = if is_selected { "> " } else { "  " };

                    let name_style = if is_selected {
                        Style::default()
                            .fg(self.accent_color())
                            .add_modifier(Modifier::BOLD)
                    } else if is_on {
                        Style::default().fg(Color::White)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let check_style = if is_on {
                        Style::default().fg(Color::Green)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let desc_style = Style::default().fg(Color::DarkGray);

                    ListItem::new(vec![
                        Line::from(vec![
                            Span::styled(prefix, name_style),
                            Span::styled(checkbox, check_style),
                            Span::styled(format!(" {}", plugin.name), name_style),
                            Span::styled(format!("  v{}", plugin.version), desc_style),
                        ]),
                        Line::from(vec![
                            Span::raw("      "),
                            Span::styled(&plugin.description, desc_style),
                        ]),
                    ])
                })
                .collect();

            // Register clickable areas (each plugin takes 2 rows)
            for i in 0..self.discovered_plugins.len() {
                let row = chunks[1].y + (i as u16 * 2);
                if row + 1 < chunks[1].y + chunks[1].height {
                    self.clickable_areas.push((
                        Rect::new(chunks[1].x, row, chunks[1].width, 2),
                        ClickTarget::FeatureItem(i),
                    ));
                }
            }

            let menu = List::new(items);
            frame.render_widget(menu, chunks[1]);
        }

        // Help hint
        let hint = Paragraph::new(Line::from(vec![
            Span::styled("  Enter", Style::default().fg(Color::Yellow)),
            Span::styled(" toggle  ", Style::default().fg(Color::DarkGray)),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::styled(" back", Style::default().fg(Color::DarkGray)),
        ]));
        frame.render_widget(hint, chunks[2]);
    }

    fn render_theme(&mut self, frame: &mut Frame, area: Rect) {
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

                ListItem::new(vec![Line::from(vec![
                    Span::styled(prefix, style),
                    Span::styled("\u{2588}\u{2588}", Style::default().fg(preview_color)),
                    Span::styled(
                        format!(" {}{}", preset.display_name(), active_marker),
                        style,
                    ),
                ])])
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
            Style::default()
                .fg(custom_accent)
                .add_modifier(Modifier::BOLD)
        } else if custom_active {
            Style::default().fg(custom_accent)
        } else {
            Style::default()
        };

        let custom_prefix = if custom_selected { "> " } else { "  " };

        if self.edit_field == EditField::ThemeHex {
            // Show hex input inline with proper cursor positioning
            let hex_style = Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD);
            let cursor_style = Style::default().fg(Color::Black).bg(Color::Yellow);
            let (before, at_cursor, after) = self.key_input.render_parts();
            let mut hex_spans = vec![
                Span::styled(custom_prefix, custom_style),
                Span::styled("# ", custom_style),
                Span::styled(before, hex_style),
            ];
            match at_cursor {
                Some(c) => hex_spans.push(Span::styled(c.to_string(), cursor_style)),
                None => hex_spans.push(Span::styled(" ", cursor_style)),
            }
            hex_spans.push(Span::styled(after, hex_style));
            items.push(ListItem::new(vec![
                Line::from(hex_spans),
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

        // Register clickable areas: presets take 1 row each, Custom takes 1 or 2 rows
        let presets_count = ThemePreset::all().len();
        for i in 0..=presets_count {
            let row = area.y + i as u16;
            if row < area.y + area.height {
                self.clickable_areas.push((
                    Rect::new(area.x, row, area.width, 1),
                    ClickTarget::ThemeItem(i),
                ));
            }
        }

        let menu = List::new(items);
        frame.render_widget(menu, area);
    }

    fn render_help(&mut self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(Span::styled(
                "Keyboard Shortcuts",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  j/↓      Move down"),
            Line::from("  k/↑      Move up"),
            Line::from("  Enter    Select/Edit"),
            Line::from("  e        Edit item"),
            Line::from("  n        New item"),
            Line::from("  d        Delete item"),
            Line::from("  o        Open profile TOML in $EDITOR"),
            Line::from("  Esc/q    Go back/Quit"),
            Line::from("  ?        This help"),
            Line::from(""),
            Line::from(Span::styled(
                "In text fields:",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from("  ←/→      Move cursor"),
            Line::from("  Ctrl+←/→ Move by word"),
            Line::from("  Ctrl+A   Jump to start"),
            Line::from("  Ctrl+E   Jump to end"),
            Line::from("  Ctrl+W   Delete word back"),
            Line::from("  Ctrl+U   Delete to start"),
            Line::from("  Ctrl+K   Delete to end"),
            Line::from("  Tab      Switch field"),
            Line::from("  Enter    Save"),
            Line::from("  Esc      Cancel"),
            Line::from(""),
            Line::from(Span::styled(
                "Mouse",
                Style::default().add_modifier(Modifier::BOLD),
            )),
            Line::from("  Click    Select item (click again to activate)"),
            Line::from("  Scroll   Navigate lists"),
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

    fn render_version_management(&mut self, frame: &mut Frame, area: Rect) {
        let unleash_height = 4; // 2 lines content + 2 for borders
        let agent_height = (self.available_agents.len() as u16) + 2; // agents + borders

        if self.show_install_log {
            // 4-panel layout: unleash, agent picker, version list (shrunk), install log
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(unleash_height),
                    Constraint::Length(agent_height),
                    Constraint::Min(3),
                    Constraint::Length(10),
                ])
                .split(area);

            self.render_unleash_section(frame, chunks[0]);
            self.render_agent_picker(frame, chunks[1]);
            self.render_version_panel(frame, chunks[2]);
            self.render_install_log_panel(frame, chunks[3]);
        } else {
            // 3-panel layout: unleash, agent picker, version list
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(unleash_height),
                    Constraint::Length(agent_height),
                    Constraint::Min(5),
                ])
                .split(area);

            self.render_unleash_section(frame, chunks[0]);
            self.render_agent_picker(frame, chunks[1]);
            self.render_version_panel(frame, chunks[2]);
        }
    }

    /// Render the unleash (parent) section showing version and auto-update toggle
    fn render_unleash_section(&mut self, frame: &mut Frame, area: Rect) {
        let is_focused = self.version_focus == VersionFocus::Unleash;
        let border_color = if is_focused {
            self.accent_color()
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(Span::styled(
                " unleash ",
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD),
            ));

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let version_line = Line::from(vec![
            Span::styled("  ", Style::default()),
            Span::styled(
                format!("v{}", self.unleash_version),
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        let hints_line = if is_focused {
            Line::from(vec![
                Span::styled("  ", Style::default()),
                Span::styled("[Enter] ", Style::default().fg(self.accent_color())),
                Span::styled("Update", Style::default().fg(Color::DarkGray)),
            ])
        } else {
            Line::from("")
        };

        let content = Paragraph::new(vec![version_line, hints_line]);
        frame.render_widget(content, inner);

        // Register clickable area
        self.clickable_areas.push((
            Rect::new(inner.x, inner.y, inner.width, inner.height),
            ClickTarget::UnleashSection,
        ));
    }

    /// Render the agent picker as a compact list with the selected agent highlighted
    fn render_agent_picker(&mut self, frame: &mut Frame, area: Rect) {
        let border_color = if self.version_focus == VersionFocus::AgentPicker {
            self.accent_color()
        } else {
            Color::DarkGray
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(" Agent CLIs ");

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let mut lines: Vec<Line> = Vec::new();

        for agent in self.available_agents.iter() {
            let is_selected = *agent == self.version_agent;

            let (prefix, style) = if is_selected {
                (
                    "> ",
                    Style::default()
                        .fg(self.accent_color())
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                ("  ", Style::default().fg(Color::DarkGray))
            };

            // Show installed version
            let version_str = self
                .cached_agent_versions
                .get(agent)
                .and_then(|v| v.as_ref())
                .map(|v| format!("  v{}", v))
                .unwrap_or_default();

            let spans = vec![
                Span::styled(prefix, style),
                Span::styled(format!("{:<12}", agent.display_name()), style),
                Span::styled(
                    format!("{:<12}", version_str),
                    Style::default().fg(Color::Green),
                ),
            ];

            lines.push(Line::from(spans));
        }

        // Register clickable areas: one row per agent
        for (i, _) in self.available_agents.iter().enumerate() {
            let row = inner.y + i as u16;
            if row < inner.y + inner.height {
                self.clickable_areas.push((
                    Rect::new(inner.x, row, inner.width, 1),
                    ClickTarget::VersionAgentItem(i),
                ));
            }
        }

        let content = Paragraph::new(lines);
        frame.render_widget(content, inner);
    }

    /// Render the version list inside a bordered panel
    fn render_version_panel(&mut self, frame: &mut Frame, area: Rect) {
        let agent_name = self.version_agent.display_name();
        let border_color = if self.version_focus == VersionFocus::VersionList {
            self.accent_color()
        } else {
            Color::DarkGray
        };

        let title = if self.version_list_receiver.is_some() {
            format!(
                " {} Versions {} ",
                agent_name,
                SPINNER_FRAMES[self.animation_frame % SPINNER_FRAMES.len()]
            )
        } else {
            format!(" {} Versions ", agent_name)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        self.render_version_list(frame, inner);
    }

    /// Render the version list as a scrollable drum picker, responsive to available height
    fn render_version_list(&mut self, frame: &mut Frame, area: Rect) {
        // Show loading spinner when no versions are available
        if self.versions.is_empty() {
            let spinner = self.spinner_frame();
            let loading = Paragraph::new(Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{} Loading versions...", spinner),
                    Style::default().fg(Color::DarkGray),
                ),
            ]));
            frame.render_widget(loading, area);
            return;
        }

        let total = self.versions.len();
        // Reserve 1 line each for scroll indicators when needed
        let max_visible = (area.height as usize).saturating_sub(2).max(1);
        let selected = self.version_menu.selected;

        // Calculate window start to center the selection
        let half = max_visible / 2;
        let start = if total <= max_visible || selected <= half {
            0
        } else if selected >= total - half {
            total - max_visible
        } else {
            selected - half
        };
        let end = (start + max_visible).min(total);

        // Show scroll indicators
        let has_above = start > 0;
        let has_below = end < total;

        let mut lines: Vec<Line> = Vec::new();

        if has_above {
            lines.push(Line::from(Span::styled(
                "  ▲ more",
                Style::default().fg(Color::DarkGray),
            )));
        }

        for i in start..end {
            let version_info = &self.versions[i];
            let is_selected = i == selected;

            let is_focused = self.version_focus == VersionFocus::VersionList;
            let style = if is_selected && is_focused {
                Style::default()
                    .fg(self.accent_color())
                    .add_modifier(Modifier::BOLD)
            } else if is_selected {
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if version_info.is_installed {
                Style::default().fg(Color::Green)
            } else {
                Style::default()
            };

            let is_installing = self.installing_version_index == Some(i);
            let prefix = if is_selected && is_focused {
                "> "
            } else {
                "  "
            };
            let installed_marker = if version_info.is_installed {
                " [installed]"
            } else {
                ""
            };

            let mut spans = vec![
                Span::styled(prefix, style),
                Span::styled(format!("v{}", version_info.version), style),
                Span::styled(installed_marker, Style::default().fg(Color::Green)),
            ];

            if is_installing {
                let spinner = self.spinner_frame();
                spans.push(Span::styled(
                    format!(" {} installing...", spinner),
                    Style::default().fg(Color::Yellow),
                ));
            }

            // Register clickable area for this version item (1 row each)
            let row_offset = if has_above { i - start + 1 } else { i - start };
            let row = area.y + row_offset as u16;
            if row < area.y + area.height {
                self.clickable_areas.push((
                    Rect::new(area.x, row, area.width, 1),
                    ClickTarget::VersionListItem(i),
                ));
            }

            lines.push(Line::from(spans));
        }

        if has_below {
            lines.push(Line::from(Span::styled(
                "  ▼ more",
                Style::default().fg(Color::DarkGray),
            )));
        }

        let content = Paragraph::new(lines);
        frame.render_widget(content, area);
    }

    /// Render the install log panel showing live subprocess output
    fn render_install_log_panel(&self, frame: &mut Frame, area: Rect) {
        let is_active = self.install_state.is_some();
        let border_color = if is_active {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        let title = if let Some(ref state) = self.install_state {
            let elapsed = state.start_time.elapsed().as_secs();
            format!(" {} Install Log ({}s) ", self.spinner_frame(), elapsed)
        } else {
            " Install Log (Esc to dismiss) ".to_string()
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .title(title);

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let visible_height = inner.height as usize;
        let total = self.install_log_lines.len();

        // Auto-scroll: show the last N lines that fit
        let start = total.saturating_sub(visible_height);
        let visible_lines: Vec<Line> = self.install_log_lines[start..]
            .iter()
            .map(|line| {
                let style = if line.starts_with("---") {
                    if line.contains("successfully") {
                        Style::default().fg(Color::Green)
                    } else if line.contains("failed") || line.contains("Failed") {
                        Style::default().fg(Color::Red)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    }
                } else {
                    Style::default().fg(Color::DarkGray)
                };
                Line::from(Span::styled(format!(" {}", line), style))
            })
            .collect();

        let content = Paragraph::new(visible_lines);
        frame.render_widget(content, inner);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status = self.status_message.as_deref().unwrap_or("Press ? for help");
        let config_hint = format!("Config: {}", self.profile_manager.config_dir().display());

        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Min(20),
                Constraint::Length(config_hint.len() as u16 + 2),
            ])
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
    Launch(Box<LaunchRequest>),
    Update(UpdateRequest),
}

/// Request to launch Claude with a specific profile
#[derive(Debug, Clone)]
pub struct LaunchRequest {
    pub profile: Profile,
}

/// Request to update the TUI
#[derive(Debug, Clone)]
pub struct UpdateRequest {
    pub repo_dir: PathBuf,
}

impl LaunchRequest {
    pub fn execute(&self) -> io::Result<std::process::ExitStatus> {
        use std::os::unix::process::CommandExt;

        // If the profile points to a known agent binary, route through the wrapper
        // so it gets focus, restart, and plugin features automatically.
        // Unknown CLIs or "unleash" are launched directly.
        let is_known_agent = self.profile.agent_type().is_some();
        let cmd_name = std::path::Path::new(&self.profile.agent_cli_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let is_wrapper = cmd_name == "unleash";

        let mut cmd = if is_known_agent && !is_wrapper {
            // Re-invoke ourselves as "unleash" with AGENT_CMD pointing to the native binary.
            // This gives all agents wrapper features (focus, restart, plugins for Claude).
            let exe = std::env::current_exe()?;
            let mut c = Command::new(&exe);
            c.arg0("unleash");
            c.env("AGENT_CMD", &self.profile.agent_cli_path);
            // Signal wrapper reentry so the new process enters launcher mode
            // instead of showing the TUI again
            c.env(crate::launcher::UNLEASHED_ENV_VAR, "1");
            c
        } else {
            Command::new(&self.profile.agent_cli_path)
        };

        for (key, value) in &self.profile.env {
            cmd.env(key, value);
        }

        cmd.args(&self.profile.agent_cli_args);
        cmd.status()
    }
}

impl UpdateRequest {
    /// Execute the update: git pull, cargo build, replace binary and re-exec
    pub fn execute(&self) -> io::Result<()> {
        use std::os::unix::process::CommandExt;

        let tui_dir = self.repo_dir.clone();

        println!("\n=== Updating unleash TUI ===\n");

        // Step 1: Git pull
        println!("Pulling latest changes...");
        let git_status = Command::new("git")
            .arg("pull")
            .current_dir(&self.repo_dir)
            .status()?;

        if !git_status.success() {
            return Err(io::Error::other("git pull failed"));
        }

        // Step 2: Cargo build --release
        println!("\nRecompiling...");
        let build_status = Command::new("cargo")
            .args(["build", "--release"])
            .current_dir(&tui_dir)
            .status()?;

        if !build_status.success() {
            return Err(io::Error::other("cargo build failed"));
        }

        // Step 3: Re-exec the new binary
        println!("\nRestarting with new binary...\n");
        let new_binary = tui_dir.join("target/release/unleash");

        let err = Command::new(&new_binary).exec();
        // exec() only returns on error
        Err(io::Error::other(format!(
            "Failed to exec new binary: {}",
            err
        )))
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
            last_frame_area: Rect::default(),
            screen: Screen::Main,
            main_menu: MenuState::new(6), // Start, Profiles, Features, Versions, Help, Quit
            profile_menu: MenuState::new(profiles.len()),
            profile_manager,
            app_config,
            profiles: profiles.clone(),
            selected_profile: profiles.first().cloned(),
            status_message: None,
            profile_search_query: String::new(),
            profile_search_active: false,
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
            last_version_poll: HashMap::new(),
            install_state: None,
            conflict_entries: Vec::new(),
            conflict_warning_open: false,
            conflict_dismissed: false,
            npm_dialog_open: false,
            npm_dialog_pending: None,
            animation_frame: 0,
            art_layout: ArtLayout::default(),
            art_animation: None,
            animations_enabled: true,
            pending_screen: None,
            pending_external_edit: None,
            pending_profile_file_edit: None,
            help_return_screen: None,
            help_scroll_offset: 0,
            version_focus: VersionFocus::Unleash,
            unleash_version: env!("CARGO_PKG_VERSION").to_string(),
            agent_picker_menu: MenuState::new(AgentType::builtin().len()),
            installing_version_index: None,
            install_log_lines: Vec::new(),
            show_install_log: false,
            g_pending: false,
            lava_mode: false,
            konami_progress: 0,
            theme_menu: MenuState::new(ThemePreset::all().len() + 1),
            theme_color: ThemeColor::Preset(ThemePreset::Orange),
            feature_menu: MenuState::new(0),
            discovered_plugins: Vec::new(),
            clickable_areas: Vec::new(),
            available_agents: AgentType::builtin().to_vec(),
        };

        (app, temp)
    }

    /// Create a KeyEvent from a NavAction for testing handle_version_input
    fn key_for(action: NavAction) -> KeyEvent {
        match action {
            NavAction::Up => KeyEvent::new(KeyCode::Up, KeyModifiers::NONE),
            NavAction::Down => KeyEvent::new(KeyCode::Down, KeyModifiers::NONE),
            NavAction::Select => KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE),
            NavAction::Back => KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
            NavAction::Tab => KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE),
            NavAction::BackTab => KeyEvent::new(KeyCode::BackTab, KeyModifiers::NONE),
            NavAction::Quit => KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE),
            _ => KeyEvent::new(KeyCode::Null, KeyModifiers::NONE),
        }
    }

    /// Find the index of a MainMenuItem in the MAIN_MENU array.
    fn menu_index(item: MainMenuItem) -> usize {
        MAIN_MENU.iter().position(|(id, _, _)| *id == item).unwrap()
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

        app.screen = Screen::Help;
        let help_width = app.content_width();

        app.screen = Screen::ProfileEdit;
        let edit_width = app.content_width();

        // Different screens can have different widths
        assert!(main_width > 0);
        assert!(help_width > 0);
        assert!(edit_width > 0);
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

        app.handle_profiles_input(
            NavAction::Back,
            KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE),
        );
        app.tick(); // Complete pending transition
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn test_help_from_main() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        assert_eq!(app.screen, Screen::Main);
        let _ = app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        )));
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

        // Navigate to Profiles (index 1)
        app.main_menu.selected = 1;
        let _ = app.handle_main_input(NavAction::Select);
        app.tick();
        assert_eq!(app.screen, Screen::Profiles);

        // Press ? to open help
        let _ = app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        )));
        app.tick();
        assert_eq!(app.screen, Screen::Help);
        assert_eq!(app.help_return_screen, Some(Screen::Profiles));

        // Leaving help returns to Profiles, not Main
        app.handle_help_input(NavAction::Back);
        app.tick();
        assert_eq!(app.screen, Screen::Profiles);
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
        let _ = app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('?'),
            KeyModifiers::NONE,
        )));
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

        app.main_menu.selected = menu_index(MainMenuItem::Help);
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
    fn test_quit_menu_item() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.main_menu.selected = menu_index(MainMenuItem::Quit);
        let _ = app.handle_main_input(NavAction::Select);
        assert!(!app.running);
    }

    #[test]
    fn test_agent_navigate_down() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;
        assert_eq!(app.version_agent, AgentType::Claude);

        // Navigate down: Claude -> Codex -> Gemini -> OpenCode
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert_eq!(app.version_agent, AgentType::Codex);

        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert_eq!(app.version_agent, AgentType::Gemini);

        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert_eq!(app.version_agent, AgentType::OpenCode);

        // Clamp at bottom (no wrap)
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert_eq!(app.version_agent, AgentType::OpenCode);
    }

    #[test]
    fn test_agent_navigate_up() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Start at OpenCode
        app.switch_to_agent_index(3);
        assert_eq!(app.version_agent, AgentType::OpenCode);

        // Navigate up: OpenCode -> Gemini -> Codex -> Claude
        let _ = app.handle_version_input(NavAction::Up, key_for(NavAction::Up));
        assert_eq!(app.version_agent, AgentType::Gemini);

        let _ = app.handle_version_input(NavAction::Up, key_for(NavAction::Up));
        assert_eq!(app.version_agent, AgentType::Codex);

        let _ = app.handle_version_input(NavAction::Up, key_for(NavAction::Up));
        assert_eq!(app.version_agent, AgentType::Claude);

        // Clamp at top (no wrap)
        let _ = app.handle_version_input(NavAction::Up, key_for(NavAction::Up));
        assert_eq!(app.version_agent, AgentType::Claude);
    }

    #[test]
    fn test_agent_cycle_resets_selection() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker; // Focus on agent picker
        app.version_menu.selected = 3;
        app.version_menu.scroll_offset = 2;

        // Switching agent resets selection and scroll
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
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
    }

    #[test]
    fn test_codex_install_sets_install_state() {
        // Prevent real downloads from overwriting installed binaries
        unsafe { std::env::set_var("UNLEASH_SKIP_NATIVE_INSTALL", "1"); }
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_agent = AgentType::Codex;

        // Populate Codex version list
        app.versions = vec![VersionInfo {
            version: "0.93.0".to_string(),
            is_installed: false,
        }];
        app.version_menu.set_items_count(app.versions.len());

        // Select and install
        app.install_version_for_agent();

        assert_eq!(app.screen, Screen::VersionManagement);
        assert_eq!(app.installing_version_index, Some(0));
        assert_eq!(app.selected_version, Some("0.93.0".to_string()));
        let state = app.install_state.as_ref().unwrap();
        assert_eq!(state.agent_type, AgentType::Codex);
        assert_eq!(state.version, "0.93.0");
        assert_eq!(state.current_step, InstallStep::Installing);
    }

    #[test]
    fn test_codex_install_completes_on_success() {
        let (mut app, _temp) = test_app();
        app.screen = Screen::VersionManagement;
        app.installing_version_index = Some(0);

        // Simulate a successful Codex install completing
        let (tx, rx) = mpsc::channel();
        app.install_state = Some(InstallState {
            agent_type: AgentType::Codex,
            version: "latest".to_string(),
            receiver: rx,
            _handle: thread::spawn(|| {}),
            start_time: Instant::now(),
            current_step: InstallStep::Installing,
            install_result: None,
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

        // Install should complete and clear install_state
        assert!(app.install_state.is_none());
        assert_eq!(app.screen, Screen::VersionManagement);
    }

    #[test]
    fn test_agent_version_menu_navigates_to_version_screen() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;

        app.main_menu.selected = menu_index(MainMenuItem::Versions);
        let _ = app.handle_main_input(NavAction::Select);
        app.tick();
        assert_eq!(app.screen, Screen::VersionManagement);
    }

    #[test]
    fn test_agent_switch_clears_stale_versions() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Pre-populate Claude versions
        app.versions = vec![VersionInfo {
            version: "2.1.12".to_string(),
            is_installed: true,
        }];
        app.version_menu.set_items_count(1);
        assert_eq!(app.version_agent, AgentType::Claude);

        // Switch to Codex (no cache exists for Codex)
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert_eq!(app.version_agent, AgentType::Codex);

        // After the fix, versions should be cleared (no stale Claude data)
        assert!(
            !app.versions.iter().any(|v| v.version == "2.1.12"),
            "Claude version 2.1.12 should not be visible after switching to Codex"
        );
        assert_eq!(app.version_menu.selected, 0);
        assert_eq!(app.version_menu.scroll_offset, 0);
    }

    #[test]
    fn test_agent_switch_shows_cached_data_for_correct_agent() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Pre-cache Codex versions
        let codex_versions = vec![VersionInfo {
            version: "0.98.0".to_string(),
            is_installed: true,
        }];
        app.cached_version_lists
            .insert(AgentType::Codex, codex_versions);

        // Switch to Codex
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert_eq!(app.version_agent, AgentType::Codex);

        // Should show cached Codex version immediately
        assert_eq!(app.versions.len(), 1);
        assert_eq!(app.versions[0].version, "0.98.0");
    }

    #[test]
    fn test_rapid_agent_switch_replaces_receiver() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Switch to Codex (starts async fetch)
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert!(app.version_list_receiver.is_some());
        assert_eq!(app.version_agent, AgentType::Codex);

        // Immediately switch to Gemini (should replace receiver)
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert!(app.version_list_receiver.is_some());
        assert_eq!(app.version_agent, AgentType::Gemini);

        // Switch again to OpenCode
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert!(app.version_list_receiver.is_some());
        assert_eq!(app.version_agent, AgentType::OpenCode);
    }

    #[test]
    fn test_gemini_install_sets_install_state() {
        // Prevent real downloads from overwriting installed binaries
        unsafe { std::env::set_var("UNLEASH_SKIP_NATIVE_INSTALL", "1"); }
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_agent = AgentType::Gemini;

        app.versions = vec![VersionInfo {
            version: "1.0.0".to_string(),
            is_installed: false,
        }];
        app.version_menu.set_items_count(1);

        app.install_version_for_agent();

        assert_eq!(app.screen, Screen::VersionManagement);
        assert_eq!(app.installing_version_index, Some(0));
        assert_eq!(app.selected_version, Some("1.0.0".to_string()));
        let state = app.install_state.as_ref().unwrap();
        assert_eq!(state.agent_type, AgentType::Gemini);
        assert_eq!(state.version, "1.0.0");
    }

    #[test]
    fn test_opencode_install_sets_install_state() {
        // Prevent real downloads from overwriting installed binaries
        unsafe { std::env::set_var("UNLEASH_SKIP_NATIVE_INSTALL", "1"); }
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_agent = AgentType::OpenCode;

        app.versions = vec![VersionInfo {
            version: "0.5.0".to_string(),
            is_installed: false,
        }];
        app.version_menu.set_items_count(1);

        app.install_version_for_agent();

        assert_eq!(app.screen, Screen::VersionManagement);
        assert_eq!(app.installing_version_index, Some(0));
        assert_eq!(app.selected_version, Some("0.5.0".to_string()));
        let state = app.install_state.as_ref().unwrap();
        assert_eq!(state.agent_type, AgentType::OpenCode);
        assert_eq!(state.version, "0.5.0");
    }

    #[test]
    fn test_all_agent_types_in_cycle() {
        let agents = AgentType::builtin();
        assert_eq!(agents.len(), 4);
        assert_eq!(agents[0], AgentType::Claude);
        assert_eq!(agents[1], AgentType::Codex);
        assert_eq!(agents[2], AgentType::Gemini);
        assert_eq!(agents[3], AgentType::OpenCode);
    }

    #[test]
    fn test_agent_display_names() {
        assert_eq!(AgentType::Claude.display_name(), "Claude Code");
        assert_eq!(AgentType::Codex.display_name(), "Codex");
        assert_eq!(AgentType::Gemini.display_name(), "Gemini CLI");
        assert_eq!(AgentType::OpenCode.display_name(), "OpenCode");
    }

    #[test]
    fn test_agent_from_str() {
        assert_eq!(AgentType::from_str("claude"), Some(AgentType::Claude));
        assert_eq!(AgentType::from_str("codex"), Some(AgentType::Codex));
        assert_eq!(AgentType::from_str("gemini"), Some(AgentType::Gemini));
        assert_eq!(AgentType::from_str("gemini-cli"), Some(AgentType::Gemini));
        assert_eq!(AgentType::from_str("opencode"), Some(AgentType::OpenCode));
        assert_eq!(AgentType::from_str("open-code"), Some(AgentType::OpenCode));
        assert_eq!(AgentType::from_str("unknown"), None);
    }

    #[test]
    fn test_focus_toggle_between_picker_and_list() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Tab moves from picker to version list
        let _ = app.handle_version_input(NavAction::Tab, key_for(NavAction::Tab));
        assert!(app.version_focus != VersionFocus::AgentPicker);

        // Back goes from version list to picker
        let _ = app.handle_version_input(NavAction::Back, key_for(NavAction::Back));
        assert!(app.version_focus == VersionFocus::AgentPicker);
    }

    #[test]
    fn test_select_in_picker_moves_focus_to_list() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Select in picker should move focus to version list
        let _ = app.handle_version_input(NavAction::Select, key_for(NavAction::Select));
        assert!(app.version_focus != VersionFocus::AgentPicker);
    }

    #[test]
    fn test_back_from_list_goes_to_picker() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::VersionList;

        // Back from version list goes to agent picker
        let _ = app.handle_version_input(NavAction::Back, key_for(NavAction::Back));
        assert!(app.version_focus == VersionFocus::AgentPicker);
        assert_eq!(app.screen, Screen::VersionManagement);

        // Back from agent picker goes to unleash section
        let _ = app.handle_version_input(NavAction::Back, key_for(NavAction::Back));
        assert!(app.version_focus == VersionFocus::Unleash);
        assert_eq!(app.screen, Screen::VersionManagement);

        // Back from unleash goes to main screen
        let _ = app.handle_version_input(NavAction::Back, key_for(NavAction::Back));
        app.tick();
        assert_eq!(app.screen, Screen::Main);
    }

    #[test]
    fn test_version_screen_starts_with_unleash_focused() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.version_focus = VersionFocus::VersionList; // Simulate leftover state

        // Enter version management screen
        app.screen = Screen::VersionManagement;
        app.refresh_screen_data();

        assert!(app.version_focus == VersionFocus::Unleash);
    }

    #[test]
    fn test_g_g_jumps_to_top() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Navigate to OpenCode (bottom)
        app.switch_to_agent_index(3);
        assert_eq!(app.version_agent, AgentType::OpenCode);

        // Press 'g' then 'g' to jump to top
        let g_key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let _ = app.handle_version_input(NavAction::None, g_key);
        assert!(app.g_pending);
        let _ = app.handle_version_input(NavAction::None, g_key);
        assert!(!app.g_pending);
        assert_eq!(app.version_agent, AgentType::Claude);
    }

    #[test]
    fn test_shift_g_jumps_to_bottom() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        assert_eq!(app.version_agent, AgentType::Claude);

        // Press 'G' to jump to bottom
        let big_g_key = KeyEvent::new(KeyCode::Char('G'), KeyModifiers::SHIFT);
        let _ = app.handle_version_input(NavAction::None, big_g_key);
        assert_eq!(app.version_agent, AgentType::OpenCode);
    }

    #[test]
    fn test_s_rescans_versions() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::VersionList;

        // Press 's' triggers a rescan (version_list_receiver gets set)
        let s_key = KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE);
        let _ = app.handle_version_input(NavAction::None, s_key);
        assert!(app.version_list_receiver.is_some());
    }

    #[test]
    fn test_g_pending_clears_on_non_g_key() {
        let (mut app, _temp) = test_app();
        app.animations_enabled = false;
        app.screen = Screen::VersionManagement;
        app.version_focus = VersionFocus::AgentPicker;

        // Press 'g' — should set pending
        let g_key = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::NONE);
        let _ = app.handle_version_input(NavAction::None, g_key);
        assert!(app.g_pending);

        // Press something else — should clear pending and handle normally
        let _ = app.handle_version_input(NavAction::Down, key_for(NavAction::Down));
        assert!(!app.g_pending);
    }
}
