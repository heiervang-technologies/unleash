# unleash Test Suite

This directory contains test scripts for validating unleash functionality.

## Test Scripts

### `test_auth_check.sh`
Basic test script for `unleash auth` command functionality.

**Usage:**
```bash
./tests/test_auth_check.sh
```

**Tests:**
- Basic auth check
- Verbose output format
- JSON output format
- Exit codes

### `test_auth_check_comprehensive.sh`
Comprehensive test suite that validates authentication checking logic and exit codes.

**Usage:**
```bash
./tests/test_auth_check_comprehensive.sh
```

**Test Suites:**

#### Suite 1: No Authentication Present
- Exit code 1 when no auth configured
- Error message shows "not configured"
- JSON output shows `authenticated: false`

#### Suite 2: Environment Variable Authentication
- Exit code 0 when `CLAUDE_CODE_OAUTH_TOKEN` is set
- Success message shows "configured"
- JSON output shows `authenticated: true`
- Verbose output shows OAuth token method

#### Suite 3: Credentials File Authentication
- Exit code 0 with valid `~/.claude/.credentials.json`
- Success message shows "configured"
- JSON output shows `method: "credentials_file"`

#### Suite 4: Invalid Credentials File
- Exit code 1 for empty credentials file (`{}`)
- Exit code 1 for corrupted/invalid JSON

#### Suite 5: Authentication Priority
- Both env var and file present: exit code 0
- Environment variable takes priority over credentials file

#### Suite 6: JSON Format Validation
- JSON output is valid and parseable by `jq`
- JSON output contains required `authenticated` field

#### Suite 7: Quiet Mode
- With auth: No output produced
- With auth: Exit code 0
- Without auth: No output produced
- Without auth: Exit code 1
- Quiet mode overrides verbose flag
- Quiet mode overrides JSON flag

## Running All Tests

```bash
# Run basic tests
./tests/test_auth_check.sh

# Run comprehensive tests
./tests/test_auth_check_comprehensive.sh
```

## Test Results

All tests use color-coded output:
- Green: Test passed
- Red: Test failed
- Yellow: Test skipped (edge case)

### Exit Codes

Test scripts exit with:
- `0`: All tests passed
- `1`: One or more tests failed

## Requirements

The comprehensive test suite requires:
- `bash` 4.0+
- `jq` for JSON parsing
- `unleash` binary built and available (uses `./target/release/unleash` by default)

## Environment Variables

**`UNLEASH_BIN`**: Override the unleash binary path
```bash
UNLEASH_BIN=/usr/local/bin/unleash ./tests/test_auth_check_comprehensive.sh
```

## Test Safety

The comprehensive test script:
- Backs up existing credentials before testing
- Restores credentials after testing (even on failure)
- Uses temporary directories for test artifacts
- Cleans up all test files on exit

## Implementation Details

### Authentication Check Logic

The `unleash auth` command checks authentication in this order:

1. **Environment Variable**: `CLAUDE_CODE_OAUTH_TOKEN`
2. **Credentials File**: `~/.claude/.credentials.json`
   - Must contain `claudeAiOauth` and `accessToken` fields
   - Empty or invalid files are rejected
3. **macOS Keychain**: `claude` service (macOS only)

### Credentials File Format

Valid credentials file format:
```json
{
  "claudeAiOauth": {
    "accessToken": "...",
    "refreshToken": "...",
    "expiresAt": 9999999999
  }
}
```

Invalid formats (will fail auth check):
```json
{}                    // Empty object
{"foo": "bar"}       // Missing required fields
"not valid json"     // Corrupted file
```

## CI/CD Integration

These tests can be integrated into CI/CD pipelines:

```yaml
# GitHub Actions example
- name: Run Authentication Tests
  run: |
    cargo build --release
    ./tests/test_auth_check_comprehensive.sh
```

## Debugging Failed Tests

If tests fail:

1. **Check build**: Ensure `cargo build --release` succeeded
2. **Check binary**: Verify `./target/release/unleash` exists
3. **Manual test**: Run `unleash auth --verbose` manually
4. **Check output**: Failed tests show actual vs expected output
5. **Isolated test**: Comment out passing tests to focus on failures

## Contributing

When adding new features to `unleash auth`:

1. Add corresponding tests to `test_auth_check_comprehensive.sh`
2. Test both success and failure cases
3. Verify exit codes are correct
4. Test JSON output format
5. Run full test suite before submitting PR

---

For questions or issues with tests, see the main [README.md](../README.md).
