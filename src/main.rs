mod app;
mod config;
mod input;
mod pixel_art;
mod text_input;
mod version;

use app::{App, AppAction};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::prelude::*;
use std::io::{self, stdout};
use std::time::Duration;

fn main() -> io::Result<()> {
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
                        return main();
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
                        return main();
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

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<Option<AppAction>> {
    loop {
        // Draw UI
        terminal.draw(|f| app.render(f))?;

        // Handle events with timeout for responsiveness
        if event::poll(Duration::from_millis(100))? {
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
