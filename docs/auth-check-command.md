# Authentication Check Command

## Overview

The `au auth` command provides a standalone way to verify Claude Code authentication status without launching the full Claude CLI. This is particularly useful for:

- CI/CD pipelines and automation scripts
- Pre-flight checks before running Claude
- Debugging authentication issues
- Integration with other tools and scripts

## Usage

### Basic Check

```bash
au auth
```

Output:
```
✓ Authentication configured
```

Exit code: `0` if authenticated, `1` if not

### Verbose Check

```bash
au auth --verbose
```

Output:
```
✓ Authentication configured

Authentication method:
  • OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable
  • Token preview: sk-ant-oat...g-1JzO1QAA

Status: Ready to use Claude Code
```

### JSON Output

For scripting and automation:

```bash
au auth --json
```

Output:
```json
{
  "authenticated": true,
  "method": "oauth_token",
  "details": null
}
```

With verbose details:

```bash
au auth --json --verbose
```

Output:
```json
{
  "authenticated": true,
  "method": "oauth_token",
  "details": "OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable"
}
```

### Quiet Mode

For scripts where you only need the exit code:

```bash
au auth -q
```

This produces **no output** - only the exit code (0 for success, 1 for failure). Useful in conditional checks:

```bash
if au auth -q; then
    echo "Authenticated"
fi
```

### Options

- `-v, --verbose`: Show detailed information including token previews and file metadata
- `--json`: Output results as JSON for scripting and automation
- `-q, --quiet`: Quiet mode - only return exit code, no output
- `-h, --help`: Show help information

## Authentication Methods Detected

The command checks for authentication in the following order:

1. **OAuth Token**: `CLAUDE_CODE_OAUTH_TOKEN` environment variable
2. **Credentials File**: `~/.claude/.credentials.json` (Linux/Ubuntu)
3. **macOS Keychain**: Service name "claude" (macOS only)

## Exit Codes

- `0`: Authentication is configured and valid
- `1`: No authentication found

## Examples

### In Shell Scripts

```bash
#!/bin/bash
# Using quiet mode for clean conditionals
if ! au auth -q; then
    echo "Error: Claude Code authentication not configured"
    echo "Run: claude setup-token"
    exit 1
fi

# Continue with Claude operations
au --auto "Run the tests"
```

Without quiet mode (shows status message):
```bash
#!/bin/bash
if ! au auth > /dev/null 2>&1; then
    echo "Error: Claude Code authentication not configured"
    exit 1
fi
```

### In CI/CD Pipelines

```yaml
- name: Check Claude Authentication
  run: |
    if ! au auth; then
      echo "::error::Claude authentication not configured"
      exit 1
    fi

- name: Run Claude Tasks
  run: au --auto "Analyze the codebase"
```

### With JSON Output

```bash
#!/bin/bash
AUTH_STATUS=$(au auth --json)
AUTHENTICATED=$(echo "$AUTH_STATUS" | jq -r '.authenticated')
METHOD=$(echo "$AUTH_STATUS" | jq -r '.method')

if [ "$AUTHENTICATED" = "true" ]; then
    echo "Authenticated via: $METHOD"
else
    echo "Not authenticated"
    exit 1
fi
```

### Pre-flight Check

```bash
#!/bin/bash
echo "Checking prerequisites..."

# Check Claude authentication
if au auth --verbose; then
    echo "✓ Claude authentication OK"
else
    echo "✗ Claude authentication missing"
    echo ""
    echo "Please authenticate using one of these methods:"
    echo "1. claude setup-token"
    echo "2. claude (interactive)"
    exit 1
fi

# Check other prerequisites
# ...

echo "All checks passed. Starting task..."
au --auto "$@"
```

## Implementation Details

### Files Modified

- **src/cli.rs**: Added `AuthCheck` command variant
- **src/main.rs**: Added auth module and command handler
- **src/auth.rs**: New module implementing authentication checking logic
- **src/json_output.rs**: Added `AuthCheckOutput` structure (already existed)
- **README.md**: Updated documentation with auth examples

### Authentication Logic

The implementation reuses the authentication checking logic from:
- `scripts/au` (bash wrapper) - `check_authentication()` function
- `src/launcher.rs` - `check_authentication()` function

The standalone command provides the same checks without launching Claude, making it faster and suitable for automation.

### Testing

Run the test suite:

```bash
./tests/test_auth_check.sh
```

The test verifies:
- Basic authentication check
- Verbose output
- JSON output
- JSON + verbose output
- Exit codes

## Related Commands

- `au`: Launch Claude with wrapper features (includes auth check on startup).
- `claude setup-token`: Generate OAuth token
- `claude`: Interactive authentication

## References

- [Claude Code IAM Documentation](https://code.claude.com/docs/en/iam)
- [OAuth Token Setup Guide](https://code.claude.com/docs/en/iam#oauth-tokens)
