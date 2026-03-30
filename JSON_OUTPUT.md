# JSON Output Feature

This document describes the `--json` flag feature for unleash CLI.

## Overview

The `--json` flag is a global flag that changes command output from human-readable text to machine-readable JSON format. This is useful for:

- Automation and scripting
- Parsing output in other programs
- Integration with CI/CD pipelines
- Monitoring and alerting systems

## Usage

Add the `--json` flag to any supported command:

```bash
unleash version --json
unleash version --list --json
unleash auth --json
unleash auth --json --verbose
```

## Supported Commands

### 1. Version Information (`unleash version --json`)

Shows version information for both unleash and Claude Code.

**Output:**
```json
{
  "unleash_version": "2.1.1",
  "claude_code_version": "2.1.4",
  "claude_code_installed": true
}
```

**Fields:**
- `unleash_version` - Version of the unleash CLI
- `claude_code_version` - Version of installed Claude Code (or "not installed")
- `claude_code_installed` - Boolean indicating if Claude Code is installed

### 2. Version List (`unleash version --list --json`)

Lists all available Claude Code versions with metadata.

**Output:**
```json
{
  "currently_installed": "2.1.4",
  "versions": [
    {
      "version": "2.1.6",
      "is_installed": false
    },
    {
      "version": "2.1.5",
      "is_installed": false
    },
    {
      "version": "2.1.4",
      "is_installed": true
    }
  ]
}
```

**Fields:**
- `currently_installed` - Currently installed version (or null if not installed)
- `versions` - Array of version objects:
  - `version` - Version number
  - `is_installed` - Whether this version is currently installed

### 3. Authentication Check (`unleash auth --json`)

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

### 4. Version Install (`unleash version --install <version> --json`)

Installs a specific version of Claude Code.

**Output (success):**
```json
{
  "success": true,
  "message": "Successfully installed Claude Code v2.1.4"
}
```

## Implementation Details

### Code Structure

- `src/json_output.rs` - JSON output structures and utilities
- `src/cli.rs` - CLI argument parsing with `--json` flag
- `src/version.rs` - Version management with JSON support
- `src/auth.rs` - Authentication checking with JSON support

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
unleash version --json | jq -r '.unleash_version'
# Output: 2.1.1
```

### Check if authenticated in script

```bash
if unleash auth --json | jq -e '.authenticated' > /dev/null; then
    echo "Authenticated!"
else
    echo "Not authenticated!"
fi
```

### List only installed versions

```bash
unleash version --list --json | jq '.versions[] | select(.is_installed == true)'
```

### Monitor authentication status

```bash
# In a monitoring script
STATUS=$(unleash auth --json)
AUTHENTICATED=$(echo "$STATUS" | jq -r '.authenticated')

if [ "$AUTHENTICATED" != "true" ]; then
    alert "Claude Code authentication failed!"
fi
```

## Testing

All JSON outputs can be tested with:

```bash
cargo build --release
./target/release/unleash version --json
./target/release/unleash version --list --json
./target/release/unleash auth --json
./target/release/unleash auth --json --verbose
```

All outputs produce valid JSON that can be parsed by `jq` and other JSON processors.
