mod app;
mod config;
mod input;
mod pixel_art;
mod text_input;

use app::App;
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
        Ok(Some(launch_request)) => {
            // Launch Claude directly - no transition messages for seamless flow
            match launch_request.execute() {
                Ok(_status) => {
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

fn run_app(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> io::Result<Option<app::LaunchRequest>> {
    loop {
        // Draw UI
        terminal.draw(|f| app.render(f))?;

        // Handle events with timeout for responsiveness
        if event::poll(Duration::from_millis(100))? {
            let event = event::read()?;
            if let Some(launch_request) = app.handle_event(event)? {
                return Ok(Some(launch_request));
            }
        }

        // Check if we should exit
        if !app.running {
            return Ok(None);
        }
    }
}
