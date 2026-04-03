//! TUI module for unleash
//!
//! Provides profile management, version management, and launcher UI.

mod app;
pub mod session_picker;

pub use app::{App, AppAction};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};
use std::time::Duration;

/// Check if we're running in a TTY environment
fn is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}

/// Run the TUI application
pub fn run() -> io::Result<()> {
    // Verify we have a TTY before attempting terminal operations
    if !is_tty() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "TUI requires a terminal (TTY). This command cannot run in headless environments.\n\
             Use non-TUI commands instead: unleash auth, unleash version, unleash claude",
        ));
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new()?;

    // Main loop
    let result = run_app(&mut terminal, &mut app);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Handle result
    match result {
        Ok(Some(action)) => match action {
            AppAction::Launch(launch_request) => {
                // Launch Claude directly - no transition messages for seamless flow
                match launch_request.execute() {
                    Ok(status) => {
                        // Check exit code - treat SIGTERM (143) as clean exit
                        // Exit code 143 = 128 + 15 (SIGTERM), used by exit_claude MCP tool
                        if let Some(code) = status.code() {
                            if code != 0 && code != 143 {
                                // Non-zero exit (excluding SIGTERM) - could indicate an error
                                // but we still return to TUI for seamless UX
                            }
                        }
                        // Automatically return to TUI after Claude exits
                        return run();
                    }
                    Err(e) => {
                        eprintln!("Failed to launch Claude: {}", e);
                        eprintln!("Make sure the agent CLI is in your PATH or set agent_cli_path in the profile");
                        std::process::exit(1);
                    }
                }
            }
            AppAction::Update(update_request) => {
                // Execute update - this will re-exec the new binary on success
                match update_request.execute() {
                    Ok(()) => {
                        // Should not reach here - exec replaces process
                        unreachable!("exec should not return on success");
                    }
                    Err(e) => {
                        eprintln!("Update failed: {}", e);
                        eprintln!("\nPress Enter to return to TUI...");
                        let mut input = String::new();
                        let _ = std::io::stdin().read_line(&mut input);
                        return run();
                    }
                }
            }
        },
        Ok(None) => {
            // Normal exit - no message for clean exit
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Run an external editor on an existing file (e.g., a profile TOML)
fn run_external_editor_file(path: &std::path::Path) -> io::Result<()> {
    use std::env;
    use std::process::Command;

    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    let status = Command::new(&editor).arg(path).status()?;

    if !status.success() {
        return Err(io::Error::other(format!(
            "Editor '{}' exited with error",
            editor
        )));
    }

    Ok(())
}

/// Run an external editor with the given content and return the edited content
fn run_external_editor(content: &str) -> io::Result<String> {
    use std::env;
    use std::fs;
    use std::process::Command;

    // Get editor from environment
    let editor = env::var("VISUAL")
        .or_else(|_| env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string());

    // Create temp file with content
    let temp_dir = env::temp_dir();
    let temp_path = temp_dir.join(format!("unleash-edit-{}.txt", std::process::id()));

    fs::write(&temp_path, content)?;

    // Run editor
    let status = Command::new(&editor).arg(&temp_path).status()?;

    if !status.success() {
        let _ = fs::remove_file(&temp_path);
        return Err(io::Error::other(format!(
            "Editor '{}' exited with error",
            editor
        )));
    }

    // Read back content
    let edited = fs::read_to_string(&temp_path)?;

    // Clean up
    let _ = fs::remove_file(&temp_path);

    Ok(edited.trim_end().to_string())
}

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<Option<AppAction>> {
    loop {
        // Check for pending external edit
        if let Some(content) = app.pending_external_edit.take() {
            // Leave alternate screen and disable raw mode for editor
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;

            // Run external editor
            let result = run_external_editor(&content);

            // Re-enable terminal
            enable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                EnterAlternateScreen,
                EnableMouseCapture
            )?;

            // Handle result
            match result {
                Ok(edited) => {
                    // Save the edited content to the editing profile's stop_prompt
                    let value = edited.trim().to_string();
                    if let Some(ref mut profile) = app.editing_profile {
                        profile.stop_prompt = if value.is_empty() {
                            None
                        } else {
                            Some(value.clone())
                        };
                        let _ = app.profile_manager.save_profile(profile);
                    }
                    app.status_message = Some("Stop prompt saved".to_string());
                }
                Err(e) => {
                    app.status_message = Some(format!("Editor error: {}", e));
                }
            }

            // Force redraw
            terminal.draw(|f| app.render(f))?;
            continue;
        }

        // Check for pending profile file edit (open TOML in editor)
        if let Some(path) = app.pending_profile_file_edit.take() {
            // Leave alternate screen and disable raw mode for editor
            disable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            )?;

            // Run external editor on the profile file directly
            let result = run_external_editor_file(&path);

            // Re-enable terminal
            enable_raw_mode()?;
            execute!(
                terminal.backend_mut(),
                EnterAlternateScreen,
                EnableMouseCapture
            )?;

            // Reload profile from disk after editing
            match result {
                Ok(()) => {
                    if let Some(ref profile) = app.editing_profile {
                        match app.profile_manager.load_profile(&profile.name) {
                            Ok(reloaded) => {
                                app.load_profile_for_editing(reloaded);
                                app.sync_editing_to_selected();
                                app.status_message = Some("Profile reloaded from file".to_string());
                            }
                            Err(e) => {
                                app.status_message =
                                    Some(format!("Failed to reload profile: {}", e));
                            }
                        }
                    }
                }
                Err(e) => {
                    app.status_message = Some(format!("Editor error: {}", e));
                }
            }

            // Force redraw
            terminal.draw(|f| app.render(f))?;
            continue;
        }

        // Tick to advance animations and poll async operations
        app.tick();

        // Draw UI
        terminal.draw(|f| app.render(f))?;

        // Handle events with timeout for responsiveness
        // Use shorter timeout during animations for smooth 60 FPS
        let timeout = if app.art_animation.is_some() || app.install_state.is_some() {
            Duration::from_millis(16) // ~60 FPS for smooth animation
        } else {
            Duration::from_millis(100) // Lower FPS when idle to save CPU
        };

        if event::poll(timeout)? {
            let event = event::read()?;
            if let Some(action) = app.handle_event(event)? {
                return Ok(Some(action));
            }
        }

        // Check if we should exit
        if !app.running {
            return Ok(None);
        }
    }
}
