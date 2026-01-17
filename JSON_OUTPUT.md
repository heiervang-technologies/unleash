# JSON Output Feature

This document describes the `--json` flag feature for Claude Unleashed CLI.

## Overview

The `--json` flag is a global flag that changes command output from human-readable text to machine-readable JSON format. This is useful for:

- Automation and scripting
- Parsing output in other programs
- Integration with CI/CD pipelines
- Monitoring and alerting systems

## Usage

Add the `--json` flag to any supported command:

```bash
cu --version --json
cu version --json
cu version --list --json
cu auth-check --json
cu auth-check --json --verbose
```

## Supported Commands

### 1. Version Information (`cu --version --json`)

Shows version information for both Claude Unleashed and Claude Code.

**Output:**
```json
{
  "claude_unleashed_version": "2.1.1",
  "claude_code_version": "2.1.4",
  "claude_code_installed": true
}
```

**Fields:**
- `claude_unleashed_version` - Version of Claude Unleashed CLI
- `claude_code_version` - Version of installed Claude Code (or "not installed")
- `claude_code_installed` - Boolean indicating if Claude Code is installed

### 2. Version Command (`cu version --json`)

Same output as `cu --version --json`.

### 3. Version List (`cu version --list --json`)

Lists all available Claude Code versions with metadata.

**Output:**
```json
{
  "currently_installed": "2.1.4",
  "filter_mode": "whitelist",
  "versions": [
    {
      "version": "2.1.6",
      "is_installed": false,
      "has_patch": false,
      "is_whitelisted": false,
      "is_blacklisted": false
    },
    {
      "version": "2.1.5",
      "is_installed": false,
      "has_patch": true,
      "is_whitelisted": false,
      "is_blacklisted": true
    },
    {
      "version": "2.1.4",
      "is_installed": true,
      "has_patch": true,
      "is_whitelisted": true,
      "is_blacklisted": false
    }
  ]
}
```

**Fields:**
- `currently_installed` - Currently installed version (or null if not installed)
- `filter_mode` - Current version filter mode: `whitelist` or `blacklist`
- `versions` - Array of version objects:
  - `version` - Version number
  - `is_installed` - Whether this version is currently installed
  - `has_patch` - Whether auto-mode patch is available for this version
  - `is_whitelisted` - Whether this version is verified to work correctly
  - `is_blacklisted` - Whether this version has known critical issues

**Filter Mode Behavior:**
- `whitelist` (default): Only whitelisted versions are recommended for installation
- `blacklist`: All versions except blacklisted ones are allowed

### 4. Authentication Check (`cu auth-check --json`)

Checks Claude Code authentication status.

**Output (authenticated):**
```json
{
  "authenticated": true,
  "method": "oauth_token",
  "details": null
}
```

**Output (not authenticated):**
```json
{
  "authenticated": false,
  "method": null,
  "details": null
}
```

**With `--verbose` flag:**
```json
{
  "authenticated": true,
  "method": "oauth_token",
  "details": "OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable"
}
```

**Fields:**
- `authenticated` - Boolean indicating if authentication is configured
- `method` - Authentication method: `oauth_token`, `credentials_file`, `macos_keychain`, or `null`
- `details` - Human-readable description (only with `--verbose`)

**Exit Codes:**
- `0` - Authentication found
- `1` - Authentication not found

### 5. Version Install (`cu version --install <version> --json`)

Installs a specific version of Claude Code.

**Output (success):**
```json
{
  "success": true,
  "message": "Successfully installed Claude Code v2.1.4"
}
```

**Output (success with warning):**
```json
{
  "success": true,
  "message": "Successfully installed Claude Code v2.1.6 (patch not available)"
}
```

## Implementation Details

### Code Structure

- `/home/me/claude-unleashed/src/json_output.rs` - JSON output structures and utilities
- `/home/me/claude-unleashed/src/cli.rs` - CLI argument parsing with `--json` flag
- `/home/me/claude-unleashed/src/version.rs` - Version management with JSON support
- `/home/me/claude-unleashed/src/auth.rs` - Authentication checking with JSON support

### Dependencies

Added `serde_json = "1.0"` to `Cargo.toml` for JSON serialization.

### Design Principles

1. **Backward Compatibility**: All commands work without the `--json` flag
2. **Consistent Format**: All JSON outputs follow similar structures
3. **Machine Readable**: Outputs are valid JSON that can be piped to `jq` or parsed programmatically
4. **Error Handling**: Errors are also output as JSON when `--json` is used
5. **Exit Codes**: Exit codes remain consistent regardless of output format

## Examples

### Parse version with jq

```bash
cu --version --json | jq -r '.claude_code_version'
# Output: 2.1.4
```

### Check if authenticated in script

```bash
if cu auth-check --json | jq -e '.authenticated' > /dev/null; then
    echo "Authenticated!"
else
    echo "Not authenticated!"
fi
```

### List only installed versions

```bash
cu version --list --json | jq '.versions[] | select(.is_installed == true)'
```

### Find latest whitelisted version

```bash
cu version --list --json | jq -r '.versions[] | select(.is_whitelisted == true) | .version' | head -1
```

### Find versions that are not blacklisted

```bash
cu version --list --json | jq -r '.versions[] | select(.is_blacklisted == false) | .version'
```

### Check the current filter mode

```bash
cu version --list --json | jq -r '.filter_mode'
```

### Monitor authentication status

```bash
# In a monitoring script
STATUS=$(cu auth-check --json)
AUTHENTICATED=$(echo "$STATUS" | jq -r '.authenticated')

if [ "$AUTHENTICATED" != "true" ]; then
    alert "Claude Code authentication failed!"
fi
```

## Future Enhancements

Potential commands that could support `--json` in the future:

- `cu patch --check --json` - Patch status as JSON
- `cu tmux status --json` - Tmux session status
- Error messages consistently formatted as JSON when `--json` is used globally

## Testing

All JSON outputs have been tested with:

```bash
cargo build --release
./target/release/cu --version --json
./target/release/cu version --json
./target/release/cu version --list --json
./target/release/cu auth-check --json
./target/release/cu auth-check --json --verbose
```

All outputs produce valid JSON that can be parsed by `jq` and other JSON processors.
