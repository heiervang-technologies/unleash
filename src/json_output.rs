//! JSON output structures for CLI commands
//!
//! This module provides serializable structures for outputting command results as JSON.

use serde::{Deserialize, Serialize};

/// Version information output
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionOutput {
    pub unleash_version: String,
    pub claude_code_version: String,
    pub claude_code_installed: bool,
}

/// Version list item
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionListItem {
    pub version: String,
    pub is_installed: bool,
}

/// Version list output
#[derive(Debug, Serialize, Deserialize)]
pub struct VersionListOutput {
    pub currently_installed: Option<String>,
    pub versions: Vec<VersionListItem>,
}

/// Authentication status
#[derive(Debug, Serialize, Deserialize)]
pub struct AuthCheckOutput {
    pub authenticated: bool,
    pub method: Option<String>,
    pub details: Option<String>,
}

/// Generic success response
#[derive(Debug, Serialize, Deserialize)]
pub struct SuccessOutput {
    pub success: bool,
    pub message: String,
}

/// Generic error response
#[allow(dead_code)]
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorOutput {
    pub success: bool,
    pub error: String,
}

/// Output a value as JSON to stdout
pub fn print_json<T: Serialize>(value: &T) {
    match serde_json::to_string_pretty(value) {
        Ok(json) => println!("{}", json),
        Err(e) => eprintln!("Error serializing JSON: {}", e),
    }
}

/// Output an error as JSON to stdout
#[allow(dead_code)]
pub fn print_error_json(error: &str) {
    print_json(&ErrorOutput {
        success: false,
        error: error.to_string(),
    });
}

/// Output a success message as JSON to stdout
pub fn print_success_json(message: &str) {
    print_json(&SuccessOutput {
        success: true,
        message: message.to_string(),
    });
}
