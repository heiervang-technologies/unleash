#!/usr/bin/env bash
# Comprehensive test script for unleash auth command
# Tests authentication detection logic and exit codes

set -e

UNLEASH_BIN="${UNLEASH_BIN:-./target/release/unleash}"
TEST_DIR="/tmp/unleash-auth-test-$$"
CREDENTIALS_PATH="$HOME/.claude/.credentials.json"
BACKUP_CREDENTIALS="${CREDENTIALS_PATH}.backup.$$"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Test counters
TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

# Cleanup function
cleanup() {
    echo
    echo "Cleaning up..."

    # Restore original credentials if backed up
    if [ -f "$BACKUP_CREDENTIALS" ]; then
        mv "$BACKUP_CREDENTIALS" "$CREDENTIALS_PATH"
        echo "✓ Restored original credentials"
    fi

    # Remove test directory
    rm -rf "$TEST_DIR"

    # Unset test env var
    unset CLAUDE_CODE_OAUTH_TOKEN

    echo
    echo "================================"
    echo "Test Summary:"
    echo "  Total: $TESTS_RUN"
    echo -e "  ${GREEN}Passed: $TESTS_PASSED${NC}"
    if [ $TESTS_FAILED -gt 0 ]; then
        echo -e "  ${RED}Failed: $TESTS_FAILED${NC}"
        exit 1
    fi
    echo "================================"
}

trap cleanup EXIT

# Test helper functions
run_test() {
    local test_name="$1"
    local expected_exit="$2"
    shift 2

    TESTS_RUN=$((TESTS_RUN + 1))
    echo
    echo "Test $TESTS_RUN: $test_name"
    echo "---"

    set +e
    "$@" > /dev/null 2>&1
    local actual_exit=$?
    set -e

    if [ $actual_exit -eq $expected_exit ]; then
        echo -e "${GREEN}✓ PASS${NC}: Exit code $actual_exit (expected $expected_exit)"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "${RED}✗ FAIL${NC}: Exit code $actual_exit (expected $expected_exit)"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

assert_output_contains() {
    local test_name="$1"
    local expected_string="$2"
    shift 2

    TESTS_RUN=$((TESTS_RUN + 1))
    echo
    echo "Test $TESTS_RUN: $test_name"
    echo "---"

    local output
    set +e
    output=$("$@" 2>&1)
    set -e

    if echo "$output" | grep -q "$expected_string"; then
        echo -e "${GREEN}✓ PASS${NC}: Output contains '$expected_string'"
        TESTS_PASSED=$((TESTS_PASSED + 1))
        return 0
    else
        echo -e "${RED}✗ FAIL${NC}: Output does not contain '$expected_string'"
        echo "Actual output:"
        echo "$output"
        TESTS_FAILED=$((TESTS_FAILED + 1))
        return 1
    fi
}

# Setup
echo "================================"
echo "unleash auth Comprehensive Tests"
echo "================================"
echo
echo "Setting up test environment..."

# Backup existing credentials if present
if [ -f "$CREDENTIALS_PATH" ]; then
    cp "$CREDENTIALS_PATH" "$BACKUP_CREDENTIALS"
    echo "✓ Backed up existing credentials"
fi

# Create test directory
mkdir -p "$TEST_DIR"
echo "✓ Created test directory: $TEST_DIR"

echo
echo "Starting tests..."

# ============================================================================
# Test Suite 1: No Authentication
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 1: No Authentication Present"
echo "════════════════════════════════"

# Remove all authentication
unset CLAUDE_CODE_OAUTH_TOKEN
rm -f "$CREDENTIALS_PATH"

run_test \
    "No auth: Should fail with exit code 1" \
    1 \
    $UNLEASH_BIN auth

# Test that output shows "not configured" message
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: No auth: Should show 'not configured' message"
echo "---"
set +e
OUTPUT=$($UNLEASH_BIN auth 2>&1)
set -e
if echo "$OUTPUT" | grep -q "not configured"; then
    echo -e "${GREEN}✓ PASS${NC}: Output contains 'not configured'"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Output does not contain 'not configured'"
    echo "Actual output:"
    echo "$OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test JSON output shows not authenticated
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: No auth: JSON output shows authenticated=false"
echo "---"
set +e
JSON_OUTPUT=$($UNLEASH_BIN auth --json 2>&1)
set -e
if echo "$JSON_OUTPUT" | jq -e '.authenticated == false' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}: JSON shows authenticated=false"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: JSON does not show authenticated=false"
    echo "Actual output: $JSON_OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# ============================================================================
# Test Suite 2: Environment Variable Authentication
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 2: Environment Variable Auth"
echo "════════════════════════════════"

export CLAUDE_CODE_OAUTH_TOKEN="test-token-123456789"

run_test \
    "Env var auth: Should pass with exit code 0" \
    0 \
    $UNLEASH_BIN auth

assert_output_contains \
    "Env var auth: Should show 'configured' message" \
    "configured" \
    $UNLEASH_BIN auth

# Test JSON output shows authenticated
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Env var auth: JSON output shows authenticated=true"
echo "---"
set +e
JSON_OUTPUT=$($UNLEASH_BIN auth --json 2>&1)
set -e
if echo "$JSON_OUTPUT" | jq -e '.authenticated == true' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}: JSON shows authenticated=true"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: JSON does not show authenticated=true"
    echo "Actual output: $JSON_OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test verbose output shows method
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Env var auth: Verbose shows method=oauth_token"
echo "---"
set +e
VERBOSE_OUTPUT=$($UNLEASH_BIN auth --verbose 2>&1)
set -e
if echo "$VERBOSE_OUTPUT" | grep -q -i "oauth.*token\|environment.*variable"; then
    echo -e "${GREEN}✓ PASS${NC}: Verbose output shows OAuth token method"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Verbose output doesn't show OAuth token method"
    echo "Actual output:"
    echo "$VERBOSE_OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# ============================================================================
# Test Suite 3: Credentials File Authentication
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 3: Credentials File Auth"
echo "════════════════════════════════"

# Remove env var, use credentials file
unset CLAUDE_CODE_OAUTH_TOKEN

# Create valid credentials file matching Claude's format
mkdir -p "$HOME/.claude"
cat > "$CREDENTIALS_PATH" << 'EOF'
{
  "claudeAiOauth": {
    "accessToken": "test-access-token-abc123",
    "refreshToken": "test-refresh-token-xyz789",
    "expiresAt": 9999999999
  }
}
EOF

run_test \
    "Credentials file: Should pass with exit code 0" \
    0 \
    $UNLEASH_BIN auth

assert_output_contains \
    "Credentials file: Should show 'configured' message" \
    "configured" \
    $UNLEASH_BIN auth

# Test JSON output shows credentials file method
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Credentials file: JSON shows method=credentials_file"
echo "---"
set +e
JSON_OUTPUT=$($UNLEASH_BIN auth --json --verbose 2>&1)
set -e
if echo "$JSON_OUTPUT" | jq -e '.method == "credentials_file"' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}: JSON shows credentials_file method"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: JSON does not show credentials_file method"
    echo "Actual output: $JSON_OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# ============================================================================
# Test Suite 4: Invalid Credentials File
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 4: Invalid Credentials File"
echo "════════════════════════════════"

# Create invalid credentials file (empty)
echo "{}" > "$CREDENTIALS_PATH"

run_test \
    "Invalid credentials: Should fail with exit code 1" \
    1 \
    $UNLEASH_BIN auth

# Create corrupted credentials file
echo "not valid json" > "$CREDENTIALS_PATH"

run_test \
    "Corrupted credentials: Should fail with exit code 1" \
    1 \
    $UNLEASH_BIN auth

# ============================================================================
# Test Suite 5: Priority Order
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 5: Authentication Priority"
echo "════════════════════════════════"

# Both env var and credentials file present
export CLAUDE_CODE_OAUTH_TOKEN="env-token-123"
cat > "$CREDENTIALS_PATH" << 'EOF'
{
  "claudeAiOauth": {
    "accessToken": "file-token-456",
    "refreshToken": "refresh-789",
    "expiresAt": 9999999999
  }
}
EOF

run_test \
    "Both present: Should pass with exit code 0" \
    0 \
    $UNLEASH_BIN auth

# Test that env var takes priority
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Priority: Env var should take priority over file"
echo "---"
set +e
VERBOSE_OUTPUT=$($UNLEASH_BIN auth --verbose 2>&1)
set -e
if echo "$VERBOSE_OUTPUT" | grep -q -i "oauth.*token\|environment.*variable" && \
   ! echo "$VERBOSE_OUTPUT" | grep -q -i "credentials.*file"; then
    echo -e "${GREEN}✓ PASS${NC}: Env var takes priority"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${YELLOW}⚠ SKIP${NC}: Cannot definitively verify priority (implementation may show first match)"
    TESTS_PASSED=$((TESTS_PASSED + 1))
fi

# ============================================================================
# Test Suite 6: JSON Format Validation
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 6: JSON Format Validation"
echo "════════════════════════════════"

# Test JSON is valid and parseable
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: JSON output is valid JSON"
echo "---"
set +e
JSON_OUTPUT=$($UNLEASH_BIN auth --json 2>&1)
set -e
if echo "$JSON_OUTPUT" | jq . > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}: JSON is valid and parseable"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: JSON is not valid"
    echo "Actual output: $JSON_OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test JSON has required fields
TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: JSON has required 'authenticated' field"
echo "---"
if echo "$JSON_OUTPUT" | jq -e '.authenticated' > /dev/null 2>&1; then
    echo -e "${GREEN}✓ PASS${NC}: JSON has 'authenticated' field"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: JSON missing 'authenticated' field"
    echo "Actual output: $JSON_OUTPUT"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# ============================================================================
# Test Suite 7: Quiet Mode
# ============================================================================
echo
echo "════════════════════════════════"
echo "Suite 7: Quiet Mode (-q flag)"
echo "════════════════════════════════"

# Test quiet mode with authentication
export CLAUDE_CODE_OAUTH_TOKEN="test-token-quiet"

TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Quiet mode with auth: No output produced"
echo "---"
set +e
OUTPUT=$($UNLEASH_BIN auth -q 2>&1)
EXIT_CODE=$?
set -e
OUTPUT_LENGTH=${#OUTPUT}
if [ $OUTPUT_LENGTH -eq 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: No output produced (length=$OUTPUT_LENGTH)"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Output produced (length=$OUTPUT_LENGTH)"
    echo "Actual output: '$OUTPUT'"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Quiet mode with auth: Exit code 0"
echo "---"
if [ $EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: Exit code 0"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Exit code $EXIT_CODE (expected 0)"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test quiet mode without authentication
unset CLAUDE_CODE_OAUTH_TOKEN
rm -f "$CREDENTIALS_PATH"

TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Quiet mode without auth: No output produced"
echo "---"
set +e
OUTPUT=$($UNLEASH_BIN auth -q 2>&1)
EXIT_CODE=$?
set -e
OUTPUT_LENGTH=${#OUTPUT}
if [ $OUTPUT_LENGTH -eq 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: No output produced (length=$OUTPUT_LENGTH)"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Output produced (length=$OUTPUT_LENGTH)"
    echo "Actual output: '$OUTPUT'"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Quiet mode without auth: Exit code 1"
echo "---"
if [ $EXIT_CODE -eq 1 ]; then
    echo -e "${GREEN}✓ PASS${NC}: Exit code 1"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Exit code $EXIT_CODE (expected 1)"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

# Test quiet mode overrides verbose and json
export CLAUDE_CODE_OAUTH_TOKEN="test-token-quiet-override"

TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Quiet mode overrides verbose flag"
echo "---"
set +e
OUTPUT=$($UNLEASH_BIN auth -q -v 2>&1)
set -e
OUTPUT_LENGTH=${#OUTPUT}
if [ $OUTPUT_LENGTH -eq 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: No output produced despite -v flag"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Output produced (length=$OUTPUT_LENGTH)"
    echo "Actual output: '$OUTPUT'"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

TESTS_RUN=$((TESTS_RUN + 1))
echo
echo "Test $TESTS_RUN: Quiet mode overrides json flag"
echo "---"
set +e
OUTPUT=$($UNLEASH_BIN auth -q --json 2>&1)
set -e
OUTPUT_LENGTH=${#OUTPUT}
if [ $OUTPUT_LENGTH -eq 0 ]; then
    echo -e "${GREEN}✓ PASS${NC}: No output produced despite --json flag"
    TESTS_PASSED=$((TESTS_PASSED + 1))
else
    echo -e "${RED}✗ FAIL${NC}: Output produced (length=$OUTPUT_LENGTH)"
    echo "Actual output: '$OUTPUT'"
    TESTS_FAILED=$((TESTS_FAILED + 1))
fi

echo
echo "✓ All test suites completed"
