//! Authentication checking module
//!
//! Provides functionality to check Claude Code authentication status
//! without launching the full Claude CLI.

use crate::json_output::{self, AuthCheckOutput};
use std::env;
use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::ExitCode;

/// Result of an authentication check
#[derive(Debug)]
pub enum AuthStatus {
    /// Authentication found via OAuth token
    OAuthToken,
    /// Authentication found via credentials file
    CredentialsFile(PathBuf),
    /// Authentication found via macOS Keychain
    MacOSKeychain,
    /// No authentication found
    NotFound,
}

impl AuthStatus {
    /// Check if authentication is present
    pub fn is_authenticated(&self) -> bool {
        !matches!(self, AuthStatus::NotFound)
    }

    /// Get a human-readable description
    pub fn description(&self) -> String {
        match self {
            AuthStatus::OAuthToken => {
                "OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable".to_string()
            }
            AuthStatus::CredentialsFile(path) => {
                format!("Credentials file at {}", path.display())
            }
            AuthStatus::MacOSKeychain => "macOS Keychain".to_string(),
            AuthStatus::NotFound => "No authentication configured".to_string(),
        }
    }

    /// Get the authentication method name
    pub fn method_name(&self) -> Option<String> {
        match self {
            AuthStatus::OAuthToken => Some("oauth_token".to_string()),
            AuthStatus::CredentialsFile(_) => Some("credentials_file".to_string()),
            AuthStatus::MacOSKeychain => Some("macos_keychain".to_string()),
            AuthStatus::NotFound => None,
        }
    }
}

/// Check Claude Code authentication status
pub fn check_auth() -> AuthStatus {
    // 1. Check OAuth token environment variable
    if env::var("CLAUDE_CODE_OAUTH_TOKEN").is_ok() {
        return AuthStatus::OAuthToken;
    }

    // 2. Check credentials file
    if let Some(home) = dirs::home_dir() {
        let creds_file = home.join(".claude/.credentials.json");
        if creds_file.exists() && creds_file.is_file() {
            // Verify it's valid JSON with OAuth data
            if let Ok(contents) = fs::read_to_string(&creds_file) {
                // Check for required OAuth fields
                if contents.contains("claudeAiOauth") && contents.contains("accessToken") {
                    return AuthStatus::CredentialsFile(creds_file);
                }
                // File exists but doesn't have required OAuth fields - not valid
            }
            // Can't read the file or missing OAuth fields - not valid authentication
        }
    }

    // 3. Check macOS Keychain (only on macOS)
    if cfg!(target_os = "macos") && check_macos_keychain() {
        return AuthStatus::MacOSKeychain;
    }

    AuthStatus::NotFound
}

/// Check if Claude credentials exist in macOS Keychain
#[cfg(target_os = "macos")]
fn check_macos_keychain() -> bool {
    std::process::Command::new("security")
        .args(["find-generic-password", "-s", "claude"])
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

#[cfg(not(target_os = "macos"))]
fn check_macos_keychain() -> bool {
    false
}

/// Run the auth-check command
pub fn run(verbose: bool, json: bool, quiet: bool) -> io::Result<ExitCode> {
    let status = check_auth();

    // Quiet mode: no output, only exit code
    if quiet {
        return if status.is_authenticated() {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::FAILURE)
        };
    }

    if json {
        // JSON output
        let output = AuthCheckOutput {
            authenticated: status.is_authenticated(),
            method: status.method_name(),
            details: if verbose {
                Some(status.description())
            } else {
                None
            },
        };
        json_output::print_json(&output);
        if status.is_authenticated() {
            Ok(ExitCode::SUCCESS)
        } else {
            Ok(ExitCode::FAILURE)
        }
    } else if status.is_authenticated() {
        // Success - authentication found
        println!("\x1b[32m✓\x1b[0m Authentication configured");

        if verbose {
            println!("\nAuthentication method:");
            match &status {
                AuthStatus::OAuthToken => {
                    println!("  • OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable");
                    if let Ok(token) = env::var("CLAUDE_CODE_OAUTH_TOKEN") {
                        let preview = if token.len() > 20 {
                            format!("{}...{}", &token[..10], &token[token.len() - 10..])
                        } else {
                            token
                        };
                        println!("  • Token preview: {}", preview);
                    }
                }
                AuthStatus::CredentialsFile(path) => {
                    println!("  • Credentials file: {}", path.display());
                    if let Ok(metadata) = fs::metadata(path) {
                        println!("  • File size: {} bytes", metadata.len());
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(duration) = modified.elapsed() {
                                let days = duration.as_secs() / 86400;
                                println!("  • Last modified: {} days ago", days);
                            }
                        }
                    }
                }
                AuthStatus::MacOSKeychain => {
                    println!("  • macOS Keychain");
                    println!("  • Service name: claude");
                }
                AuthStatus::NotFound => unreachable!(),
            }

            println!("\n\x1b[32mStatus: Ready to use Claude Code\x1b[0m");
        }

        Ok(ExitCode::SUCCESS)
    } else {
        // No authentication found
        eprintln!("\x1b[31m✗\x1b[0m Authentication not configured");
        eprintln!();
        eprintln!("Claude Code requires authentication to function.");
        eprintln!();
        eprintln!("To authenticate, you have two options:");
        eprintln!();
        eprintln!(
            "\x1b[1m1. Generate a long-lived OAuth token\x1b[0m (recommended for automation):"
        );
        eprintln!("   Run: \x1b[36mclaude setup-token\x1b[0m");
        eprintln!("   Then export: \x1b[36mexport CLAUDE_CODE_OAUTH_TOKEN=<your-token>\x1b[0m");
        eprintln!();
        eprintln!("\x1b[1m2. Authenticate interactively\x1b[0m:");
        eprintln!("   Run: \x1b[36mclaude\x1b[0m");
        eprintln!("   Follow the browser authentication flow");
        eprintln!();

        if verbose {
            eprintln!("\nChecked locations:");
            eprintln!("  • Environment variable: CLAUDE_CODE_OAUTH_TOKEN - \x1b[31mnot set\x1b[0m");
            if let Some(home) = dirs::home_dir() {
                let creds_file = home.join(".claude/.credentials.json");
                eprintln!(
                    "  • Credentials file: {} - \x1b[31mnot found\x1b[0m",
                    creds_file.display()
                );
            }
            if cfg!(target_os = "macos") {
                eprintln!("  • macOS Keychain: service 'claude' - \x1b[31mnot found\x1b[0m");
            }
            eprintln!();
        }

        eprintln!("For more information, see: \x1b[36mhttps://code.claude.com/docs/en/iam\x1b[0m");
        eprintln!();

        Ok(ExitCode::FAILURE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_status_description() {
        let oauth = AuthStatus::OAuthToken;
        assert!(oauth.description().contains("OAuth token"));

        let creds =
            AuthStatus::CredentialsFile(PathBuf::from("/home/user/.claude/.credentials.json"));
        assert!(creds.description().contains("Credentials file"));

        let keychain = AuthStatus::MacOSKeychain;
        assert!(keychain.description().contains("Keychain"));

        let not_found = AuthStatus::NotFound;
        assert!(not_found.description().contains("No authentication"));
    }

    #[test]
    fn test_is_authenticated() {
        assert!(AuthStatus::OAuthToken.is_authenticated());
        assert!(AuthStatus::CredentialsFile(PathBuf::new()).is_authenticated());
        assert!(AuthStatus::MacOSKeychain.is_authenticated());
        assert!(!AuthStatus::NotFound.is_authenticated());
    }
}
