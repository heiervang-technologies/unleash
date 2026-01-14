//! Main TUI application

use crate::config::{AppConfig, Profile, ProfileManager};
use crate::input::{key_to_action, MenuState, NavAction};
use crate::pixel_art::mascots;
use crate::text_input::{censor_sensitive, is_sensitive_key, TextInput};
use crate::version::{InstallResult, VersionInfo, VersionManager};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
    Frame,
};
use std::io;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Instant;

/// Application screens
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Main,
    Profiles,
    ProfileEdit,
    EnvVarEdit,
    Settings,
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
    ProfileName,
    ProfileDescription,
    EnvKey,
    EnvValue,
    ClaudePath,
    ClaudeArgs,
    StopPrompt,
}

/// State for async version installation
pub struct InstallState {
    pub version: String,
    pub is_blacklisted: bool,
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
    /// Cached installed version to avoid calling `claude --version` on every frame
    pub cached_installed_version: Option<String>,
    /// Async installation state
    pub install_state: Option<InstallState>,
    /// Animation frame counter (increments each tick)
    pub animation_frame: usize,
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
        // Cache the installed version once at startup to avoid subprocess on every frame
        let cached_installed_version = version_manager.get_installed_version();

        Ok(Self {
            running: true,
            screen: Screen::Main,
            main_menu: MenuState::new(6), // Added "Claude Code Version" option
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
            cached_installed_version,
            install_state: None,
            animation_frame: 0,
        })
    }

    /// Refresh the cached installed version (call after installing a new version)
    pub fn refresh_cached_version(&mut self) {
        self.cached_installed_version = self.version_manager.get_installed_version();
    }

    /// Called on each tick to advance animation and poll async operations
    pub fn tick(&mut self) {
        self.animation_frame = self.animation_frame.wrapping_add(1);

        // Poll installation progress
        if let Some(ref mut state) = self.install_state {
            // Try to receive results without blocking
            while let Ok(result) = state.receiver.try_recv() {
                match result {
                    InstallStepResult::InstallComplete(install_result) => {
                        state.install_result = Some(install_result.clone());
                        if install_result.success {
                            state.current_step = InstallStep::Patching;
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
                let is_blacklisted = state.is_blacklisted;
                let install_ok = state.install_result.as_ref().is_some_and(|r| r.success);
                let patch_ok = state.patch_result.as_ref().is_some_and(|r| r.success);

                self.status_message = Some(if !install_ok {
                    let err = state
                        .install_result
                        .as_ref()
                        .and_then(|r| r.error.clone())
                        .unwrap_or_else(|| "unknown error".to_string());
                    format!("Install failed: {}", err)
                } else if patch_ok {
                    if is_blacklisted {
                        format!("Installed and patched v{} (blacklisted)", version)
                    } else {
                        format!("Installed and patched v{}", version)
                    }
                } else {
                    if is_blacklisted {
                        format!("Installed v{} (blacklisted, patch unavailable)", version)
                    } else {
                        format!("Installed v{} (patch unavailable)", version)
                    }
                });

                self.install_state = None;
                self.refresh_versions();
                self.refresh_cached_version();
                self.screen = Screen::VersionManagement;
            }
        }
    }

    /// Get the current spinner frame
    pub fn spinner_frame(&self) -> &'static str {
        SPINNER_FRAMES[self.animation_frame % SPINNER_FRAMES.len()]
    }

    /// Get the default stop prompt from the hook script (source of truth)
    fn get_default_stop_prompt(&self) -> String {
        // Read from the auto-mode-stop.sh hook script
        let hook_path = std::env::var("CLAUDE_UNLEASHED_ROOT")
            .map(|root| format!("{}/plugins/unleashed/auto-mode/hooks/auto-mode-stop.sh", root))
            .unwrap_or_else(|_| {
                // Fallback: try to find relative to executable
                let exe = std::env::current_exe().ok();
                exe.and_then(|p| p.parent().map(|p| p.to_path_buf()))
                    .map(|p| p.join("../plugins/unleashed/auto-mode/hooks/auto-mode-stop.sh").to_string_lossy().to_string())
                    .unwrap_or_default()
            });

        if let Ok(content) = std::fs::read_to_string(&hook_path) {
            // Parse DEFAULT_MSG="..." from the script
            for line in content.lines() {
                if let Some(rest) = line.trim().strip_prefix("DEFAULT_MSG=\"") {
                    if let Some(msg) = rest.strip_suffix('"') {
                        return msg.to_string();
                    }
                }
            }
        }

        // Fallback if we can't read the script
        "(unable to read default from hook script)".to_string()
    }

    /// Refresh the version list
    pub fn refresh_versions(&mut self) {
        self.versions = self.version_manager.get_version_list();
        self.version_menu.set_items_count(self.versions.len());
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

            // If we're editing text, handle text input
            if self.edit_field != EditField::None {
                return Ok(self.handle_text_input(key));
            }

            let action = key_to_action(key);

            match self.screen {
                Screen::Main => return self.handle_main_input(action),
                Screen::Profiles => self.handle_profiles_input(action),
                Screen::ProfileEdit => self.handle_profile_edit_input(action, key),
                Screen::EnvVarEdit => self.handle_env_var_edit_input(action, key),
                Screen::Settings => self.handle_settings_input(action),
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
            EditField::ClaudePath | EditField::ClaudeArgs | EditField::StopPrompt => &mut self.key_input,
            EditField::None => return None,
        };

        match key.code {
            KeyCode::Char(c) => {
                // Handle Ctrl+key shortcuts
                if key.modifiers.contains(KeyModifiers::CONTROL) {
                    match c {
                        'a' => input.move_home(),      // Ctrl+A: go to start
                        'e' => input.move_end(),       // Ctrl+E: go to end
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
                        self.screen = Screen::Profiles;
                        self.refresh_profiles();
                    }
                    2 => {
                        // Claude Code Version
                        self.screen = Screen::VersionManagement;
                        self.refresh_versions();
                        self.status_message = Some("Loading versions...".to_string());
                    }
                    3 => {
                        // Settings
                        self.screen = Screen::Settings;
                    }
                    4 => {
                        // Update TUI
                        self.screen = Screen::Updating;
                        self.status_message = Some("Updating...".to_string());
                    }
                    5 => {
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
            NavAction::Help => {
                self.screen = Screen::Help;
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
            NavAction::Select => {
                if let Some(version_info) = self.versions.get(self.version_menu.selected) {
                    if version_info.is_installed {
                        self.status_message = Some(format!("v{} is already installed", version_info.version));
                    } else {
                        // Start async installation
                        let version = version_info.version.clone();
                        let is_blacklisted = version_info.is_blacklisted;

                        self.selected_version = Some(version.clone());
                        self.screen = Screen::VersionInstalling;

                        let warning = if is_blacklisted {
                            " (WARNING: blacklisted)"
                        } else {
                            ""
                        };
                        self.status_message = Some(format!("Installing v{}{}...", version, warning));

                        // Create channel for receiving results
                        let (tx, rx) = mpsc::channel();

                        // Spawn background thread for installation
                        let version_clone = version.clone();
                        let handle = thread::spawn(move || {
                            // Create a new VersionManager in the thread
                            let vm = VersionManager::new();

                            // Step 1: Install
                            let install_result = vm.install_version(&version_clone).unwrap_or_else(|e| {
                                InstallResult {
                                    success: false,
                                    stdout: String::new(),
                                    stderr: String::new(),
                                    error: Some(e.to_string()),
                                }
                            });
                            let install_ok = install_result.success;
                            let _ = tx.send(InstallStepResult::InstallComplete(install_result));

                            // Step 2: Patch (only if install succeeded)
                            if install_ok {
                                let patch_result = vm.run_patch().unwrap_or_else(|e| {
                                    InstallResult {
                                        success: false,
                                        stdout: String::new(),
                                        stderr: String::new(),
                                        error: Some(e.to_string()),
                                    }
                                });
                                let _ = tx.send(InstallStepResult::PatchComplete(patch_result));
                            }
                        });

                        self.install_state = Some(InstallState {
                            version,
                            is_blacklisted,
                            receiver: rx,
                            _handle: handle,
                            start_time: Instant::now(),
                            current_step: InstallStep::Installing,
                            install_result: None,
                            patch_result: None,
                        });
                    }
                }
            }
            NavAction::Back | NavAction::Quit => {
                self.screen = Screen::Main;
            }
            _ => {}
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
                self.screen = Screen::Main;
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
                        // Edit stop prompt - read default from hook script (source of truth)
                        let default_prompt = self.get_default_stop_prompt();
                        let current = self.app_config.stop_prompt.clone().unwrap_or(default_prompt);
                        self.key_input = TextInput::new().with_value(&current);
                        self.edit_field = EditField::StopPrompt;
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
                self.screen = Screen::Main;
            }
            _ => {}
        }
    }

    fn handle_help_input(&mut self, action: NavAction) {
        match action {
            NavAction::Back | NavAction::Quit | NavAction::Select => {
                self.screen = Screen::Main;
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
                // Find the repo directory (parent of tui/)
                let exe_path = std::env::current_exe().ok();
                let repo_dir = exe_path
                    .as_ref()
                    .and_then(|p| p.parent()) // target/release
                    .and_then(|p| p.parent()) // target
                    .and_then(|p| p.parent()) // tui
                    .and_then(|p| p.parent()) // repo root
                    .map(|p| p.to_path_buf())
                    .unwrap_or_else(|| PathBuf::from("."));

                return Ok(Some(AppAction::Update(UpdateRequest { repo_dir })));
            }
            NavAction::Back | NavAction::Quit => {
                self.screen = Screen::Main;
            }
            _ => {}
        }
        Ok(None)
    }

    /// Render the UI
    pub fn render(&mut self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(7),
                Constraint::Min(10),
                Constraint::Length(3),
            ])
            .split(frame.area());

        self.render_header(frame, chunks[0]);

        match self.screen {
            Screen::Main => self.render_main_menu(frame, chunks[1]),
            Screen::Profiles => self.render_profiles(frame, chunks[1]),
            Screen::ProfileEdit => self.render_profile_edit(frame, chunks[1]),
            Screen::EnvVarEdit => {
                self.render_profile_edit(frame, chunks[1]);
                self.render_env_var_dialog(frame, frame.area());
            }
            Screen::Settings => self.render_settings(frame, chunks[1]),
            Screen::Help => self.render_help(frame, chunks[1]),
            Screen::ConfirmDelete => {
                self.render_profiles(frame, chunks[1]);
                self.render_confirm_delete_dialog(frame, frame.area());
            }
            Screen::Updating => self.render_updating(frame, chunks[1]),
            Screen::VersionManagement => self.render_version_management(frame, chunks[1]),
            Screen::VersionInstalling => self.render_version_installing(frame, chunks[1]),
        }

        self.render_status_bar(frame, chunks[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Length(18), Constraint::Min(30)])
            .split(area);

        let mascot = mascots::orange_snail_small();
        let mascot_lines: Vec<Line> = mascot
            .to_lines_halfblock()
            .into_iter()
            .map(Line::raw)
            .collect();
        let mascot_widget = Paragraph::new(mascot_lines);
        frame.render_widget(mascot_widget, chunks[0]);

        let title_text = vec![
            Line::from(Span::styled(
                "Claude Unleashed",
                Style::default()
                    .fg(Color::Rgb(217, 119, 87))
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                format!("Profile: {}", self.selected_profile.as_ref().map(|p| p.name.as_str()).unwrap_or("none")),
                Style::default().fg(Color::Yellow),
            )),
        ];
        let title = Paragraph::new(title_text);
        frame.render_widget(title, chunks[1]);
    }

    fn render_main_menu(&mut self, frame: &mut Frame, area: Rect) {
        let current_version = self.cached_installed_version.clone().unwrap_or_else(|| "?".to_string());
        let menu_items = [
            ("Start Session", "Launch Claude with selected profile".to_string()),
            ("Profiles", "Manage environment profiles".to_string()),
            ("Claude Code Version", format!("Currently: v{}", current_version)),
            ("Settings", "Configure launcher settings".to_string()),
            ("Update TUI", "Pull latest and recompile".to_string()),
            ("Quit", "Exit the launcher".to_string()),
        ];

        // Each menu item takes 2 lines, calculate visible count
        // Area height minus 2 for borders, divided by 2 for lines per item
        let visible_items = (area.height.saturating_sub(2) / 2) as usize;

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
                        .fg(Color::Rgb(217, 119, 87))
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
        let scroll_hint = if menu_items.len() > visible_items {
            format!(" [{}/{}]", scroll_offset + 1, menu_items.len().saturating_sub(visible_items) + 1)
        } else {
            String::new()
        };

        let menu = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Menu{} [j/k ↑/↓ Enter q ?] ", scroll_hint)),
        );
        frame.render_widget(menu, area);
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
                        .fg(Color::Rgb(217, 119, 87))
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

        let menu = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Profiles [Enter=select e=edit n=new d=delete] "),
        );
        frame.render_widget(menu, area);
    }

    fn render_profile_edit(&self, frame: &mut Frame, area: Rect) {
        let profile = match &self.editing_profile {
            Some(p) => p,
            None => return,
        };

        let mut items: Vec<ListItem> = self
            .env_vars_list
            .iter()
            .enumerate()
            .map(|(i, (key, value))| {
                let style = if i == self.env_menu.selected {
                    Style::default().fg(Color::Rgb(217, 119, 87)).add_modifier(Modifier::BOLD)
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

        let menu = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" {} [Enter=edit n=new d=delete Esc=back] ", profile.name)),
        );
        frame.render_widget(menu, area);
    }

    fn render_env_var_dialog(&self, frame: &mut Frame, area: Rect) {
        let dialog_width = 60.min(area.width.saturating_sub(4));
        let dialog_height = 9;
        let dialog_x = (area.width.saturating_sub(dialog_width)) / 2;
        let dialog_y = (area.height.saturating_sub(dialog_height)) / 2;

        let dialog_area = Rect::new(dialog_x, dialog_y, dialog_width, dialog_height);

        frame.render_widget(Clear, dialog_area);

        let title = if self.editing_env_index.is_some() { " Edit Variable " } else { " New Variable " };

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
            .block(Block::default().borders(Borders::ALL).title(title).style(
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
                    .borders(Borders::ALL)
                    .title(" Confirm Delete ")
                    .style(Style::default().bg(Color::Black).fg(Color::Red)),
            );

        frame.render_widget(dialog, dialog_area);
    }

    fn render_settings(&self, frame: &mut Frame, area: Rect) {
        let args_str = self.app_config.claude_args.join(" ");
        let stop_prompt_display = self.app_config.stop_prompt
            .clone()
            .unwrap_or_else(|| "(default)".to_string());
        let settings: Vec<(&str, String, &str)> = vec![
            ("Entry Point", self.app_config.claude_path.clone(), "Command to launch (e.g., cuw, claude)"),
            ("Arguments", args_str, "Additional command-line arguments"),
            ("Stop Prompt", stop_prompt_display, "Auto-mode stop hook message (empty = default)"),
            ("Reset Settings", "".to_string(), "Reset all settings to defaults"),
        ];

        let cursor_indicator = "█";

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
                        .fg(Color::Rgb(217, 119, 87))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "> " } else { "  " };

                let display_value = if is_editing {
                    format!("{}{}", self.key_input.value, cursor_indicator)
                } else {
                    value.to_string()
                };

                let value_style = if is_editing {
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Cyan)
                };

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(*name, style),
                        Span::styled(": ", Style::default().fg(Color::DarkGray)),
                        Span::styled(display_value, value_style),
                    ]),
                    Line::from(Span::styled(format!("    {}", desc), Style::default().fg(Color::DarkGray))),
                ])
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

        let hint = if self.edit_field != EditField::None {
            " [Enter=save Esc=cancel] "
        } else {
            " Settings [Enter=edit Esc=back] "
        };

        let menu = List::new(menu_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(hint),
        );
        frame.render_widget(menu, area);
    }

    fn render_help(&self, frame: &mut Frame, area: Rect) {
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

        let content = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Help [Esc=close] "))
            .wrap(Wrap { trim: false });
        frame.render_widget(content, area);
    }

    fn render_updating(&self, frame: &mut Frame, area: Rect) {
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                "  Updating TUI...",
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from("  This will:"),
            Line::from("    1. Pull latest changes from git"),
            Line::from("    2. Recompile with cargo build --release"),
            Line::from("    3. Replace current binary and restart"),
            Line::from(""),
            Line::from(Span::styled(
                "  Press Enter to continue, Esc to cancel",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let content = Paragraph::new(lines)
            .block(Block::default().borders(Borders::ALL).title(" Update TUI "))
            .wrap(Wrap { trim: false });
        frame.render_widget(content, area);
    }

    fn render_version_management(&mut self, frame: &mut Frame, area: Rect) {
        // Calculate visible height (area minus borders minus legend)
        let visible_height = area.height.saturating_sub(2 + 2) as usize; // 2 for borders, 2 for legend

        // Ensure selected item is visible
        self.version_menu.ensure_visible(visible_height);

        // Get the scroll offset
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
                let style = if is_selected {
                    if version_info.is_blacklisted {
                        Style::default()
                            .fg(Color::Red)
                            .add_modifier(Modifier::BOLD | Modifier::CROSSED_OUT)
                    } else {
                        Style::default()
                            .fg(Color::Rgb(217, 119, 87))
                            .add_modifier(Modifier::BOLD)
                    }
                } else if version_info.is_blacklisted {
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::CROSSED_OUT)
                } else if version_info.is_installed {
                    Style::default().fg(Color::Green)
                } else {
                    Style::default()
                };

                let prefix = if is_selected { "> " } else { "  " };
                let installed_marker = if version_info.is_installed { " [installed]" } else { "" };
                let patch_marker = if version_info.has_patch { " *" } else { "" };
                let blacklist_marker = if version_info.is_blacklisted { " ⛔" } else { "" };

                ListItem::new(vec![
                    Line::from(vec![
                        Span::styled(prefix, style),
                        Span::styled(format!("v{}", version_info.version), style),
                        Span::styled(installed_marker, Style::default().fg(Color::Green)),
                        Span::styled(patch_marker, Style::default().fg(Color::Yellow)),
                        Span::styled(blacklist_marker, Style::default().fg(Color::Red)),
                    ]),
                ])
            })
            .collect();

        let current = self.cached_installed_version.clone().unwrap_or_else(|| "?".to_string());

        // Show scroll indicator if needed
        let scroll_indicator = if self.versions.len() > visible_height {
            let pos = scroll_offset + 1;
            let total = self.versions.len().saturating_sub(visible_height) + 1;
            format!(" [{}/{}]", pos, total)
        } else {
            String::new()
        };
        let title = format!(" Claude Code Versions (current: v{}){} [Enter=install Esc=back] ", current, scroll_indicator);

        let mut list_items = items;

        // Add legend at the bottom
        if !self.versions.is_empty() {
            list_items.push(ListItem::new(Line::from("")));
            list_items.push(ListItem::new(Line::from(Span::styled(
                "  * = has auto-mode patch  ⛔ = blacklisted (known issues)",
                Style::default().fg(Color::DarkGray),
            ))));
        }

        let menu = List::new(list_items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(title),
        );
        frame.render_widget(menu, area);
    }

    fn render_version_installing(&self, frame: &mut Frame, area: Rect) {
        let version = self.selected_version.as_deref().unwrap_or("?");
        let spinner = self.spinner_frame();

        // Determine current step info
        let (step_text, command_text) = if let Some(ref state) = self.install_state {
            match state.current_step {
                InstallStep::Installing => (
                    format!("{} Installing Claude Code v{}...", spinner, version),
                    "npm install -g @anthropic-ai/claude-code@...".to_string(),
                ),
                InstallStep::Patching => (
                    format!("{} Applying patches for v{}...", spinner, version),
                    "patch-claude.sh".to_string(),
                ),
                InstallStep::Done => (
                    format!("✓ Installation complete for v{}", version),
                    "Done!".to_string(),
                ),
            }
        } else {
            (
                format!("{} Installing Claude Code v{}...", spinner, version),
                "npm install -g @anthropic-ai/claude-code@...".to_string(),
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
            .block(Block::default().borders(Borders::ALL).title(" Installing "))
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

        // Set process name to include wrapper PID for identification
        // Format: "claude:<pid>" - allows correlating with conversation later
        cmd.arg0(format!("claude:{}", wrapper_pid));

        cmd.args(&self.claude_args);
        cmd.status()
    }
}

impl UpdateRequest {
    /// Execute the update: git pull, cargo build, replace binary and re-exec
    pub fn execute(&self) -> io::Result<()> {
        use std::os::unix::process::CommandExt;

        let tui_dir = self.repo_dir.join("tui");

        println!("\n=== Updating Claude Unleashed TUI ===\n");

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
            main_menu: MenuState::new(6),
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
            cached_installed_version: None,
            install_state: None,
            animation_frame: 0,
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
    fn test_navigation() {
        let (mut app, _temp) = test_app();
        assert_eq!(app.main_menu.selected, 0);

        app.main_menu.handle_action(NavAction::Down);
        assert_eq!(app.main_menu.selected, 1);
    }

    #[test]
    fn test_screen_transitions() {
        let (mut app, _temp) = test_app();

        app.main_menu.selected = 1;
        let _ = app.handle_main_input(NavAction::Select);
        assert_eq!(app.screen, Screen::Profiles);

        app.handle_profiles_input(NavAction::Back);
        assert_eq!(app.screen, Screen::Main);
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
}
