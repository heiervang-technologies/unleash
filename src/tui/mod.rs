//! TUI module for Claude Unleashed
//!
//! Provides profile management, version management, and launcher UI.

mod app;

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
             Use non-TUI commands instead: cu auth, cu version, cu patch, cu go",
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
                // Ensure patches are applied before launching
                if let Err(e) = crate::patcher::check_and_patch() {
                    eprintln!("Warning: Failed to check/apply patches: {}", e);
                }

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
                        eprintln!("Make sure 'claude' is in your PATH or set claude_path in config.toml");
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

fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> io::Result<Option<AppAction>> {
    loop {
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
