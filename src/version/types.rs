use std::path::PathBuf;

/// Result of a checksum verification attempt.
#[derive(Debug)]
pub(super) enum ChecksumResult {
    /// Checksum matched.
    Verified,
    /// Checksum did not match.
    Mismatch { expected: String, actual: String },
    /// Verification failed because a required tool or network call failed.
    Failed(String),
}

/// GCS bucket base URL for Claude Code native releases
pub(super) const CLAUDE_GCS_BUCKET: &str = "https://storage.googleapis.com/claude-code-dist-86c565f3-f756-42ad-8dfa-d59b1c096819/claude-code-releases";

/// Information about an agent version
#[derive(Debug, Clone)]
pub struct VersionInfo {
    pub version: String,
    pub is_installed: bool,
}

/// A single conflicting binary installation found on the system
#[derive(Debug, Clone)]
pub struct ConflictEntry {
    /// Filesystem path to the binary
    pub path: PathBuf,
    /// Version string reported by the binary (empty if detection failed)
    pub version: String,
    /// Human-readable install source (e.g. "native", "npm", "PATH")
    pub source: String,
    /// Whether this is the binary that would be invoked (first in PATH)
    pub active: bool,
}

/// Result of an installation attempt
#[derive(Debug, Clone)]
pub struct InstallResult {
    pub success: bool,
    #[allow(dead_code)]
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}
