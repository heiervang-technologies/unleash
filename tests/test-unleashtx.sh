#!/usr/bin/env bash
# test-unleashtx.sh - Test suite for unleashtx headless tmux mode
#
# Tests:
# 1. Help command functionality
# 2. Status command when no session exists
# 3. Invalid session name rejection
# 4. Session creation and detection
# 5. Send command without session fails
# 6. Environment variable handling
# 7. Read command without output file
# 8. Stop command cleanup

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
AUTX="${SCRIPT_DIR}/../scripts/unleashtx"
TEST_SESSION="unleashtx-test-$$"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

passed=0
failed=0
skipped=0

# Assertion functions
assert_eq() {
    if [[ "$1" == "$2" ]]; then
        echo -e "${GREEN}PASS${NC}: $3"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: $3 (expected '$2', got '$1')"
        ((failed++))
    fi
}

assert_contains() {
    if [[ "$1" == *"$2"* ]]; then
        echo -e "${GREEN}PASS${NC}: $3"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: $3 (expected to contain '$2')"
        ((failed++))
    fi
}

assert_not_contains() {
    if [[ "$1" != *"$2"* ]]; then
        echo -e "${GREEN}PASS${NC}: $3"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: $3 (expected NOT to contain '$2')"
        ((failed++))
    fi
}

assert_success() {
    if [[ $1 -eq 0 ]]; then
        echo -e "${GREEN}PASS${NC}: $2"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: $2 (exit code $1)"
        ((failed++))
    fi
}

assert_failure() {
    if [[ $1 -ne 0 ]]; then
        echo -e "${GREEN}PASS${NC}: $2"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: $2 (expected failure but succeeded)"
        ((failed++))
    fi
}

skip() {
    echo -e "${YELLOW}SKIP${NC}: $1"
    ((skipped++))
}

# Cleanup function
cleanup() {
    # Kill test tmux session if it exists
    tmux kill-session -t "$TEST_SESSION" 2>/dev/null || true
    # Clean up cache files
    rm -f "${HOME}/.cache/unleash/unleashtx/${TEST_SESSION}.output" 2>/dev/null || true
    rm -f "${HOME}/.cache/unleash/unleashtx/${TEST_SESSION}.marker" 2>/dev/null || true
}
trap cleanup EXIT

# Check prerequisites
check_prerequisites() {
    echo "=== Checking prerequisites ==="

    if [[ ! -f "$AUTX" ]]; then
        echo -e "${RED}ERROR${NC}: unleashtx script not found at $AUTX"
        exit 1
    fi

    if [[ ! -x "$AUTX" ]]; then
        echo -e "${RED}ERROR${NC}: unleashtx script is not executable"
        exit 1
    fi

    if ! command -v tmux &>/dev/null; then
        echo -e "${RED}ERROR${NC}: tmux is required but not installed"
        exit 1
    fi

    echo "All prerequisites met"
    echo ""
}

# Test 1: Help command
test_help_command() {
    echo "=== Test: help command ==="

    local output
    output=$("$AUTX" help 2>&1)
    local exit_code=$?

    assert_success $exit_code "help command exits successfully"
    assert_contains "$output" "USAGE" "help shows USAGE section"
    assert_contains "$output" "COMMANDS" "help shows COMMANDS section"
    assert_contains "$output" "start" "help mentions start command"
    assert_contains "$output" "send" "help mentions send command"
    assert_contains "$output" "attach" "help mentions attach command"
    assert_contains "$output" "stop" "help mentions stop command"
    assert_contains "$output" "AUTX_SESSION_NAME" "help mentions environment variable"
    echo ""
}

# Test 2: Status when no session exists
test_status_no_session() {
    echo "=== Test: status with no session ==="

    # Ensure no session exists
    tmux kill-session -t "$TEST_SESSION" 2>/dev/null || true

    local output
    output=$(AUTX_SESSION_NAME="$TEST_SESSION" "$AUTX" status 2>&1)

    assert_contains "$output" "not running" "status shows not running when session absent"
    echo ""
}

# Test 3: Session creation and detection
test_session_lifecycle() {
    echo "=== Test: session creation and detection ==="

    # Ensure clean state
    tmux kill-session -t "$TEST_SESSION" 2>/dev/null || true

    # Create a dummy tmux session (simulates what unleashtx start does with tmux)
    tmux new-session -d -s "$TEST_SESSION" "sleep 60" 2>/dev/null
    local create_exit=$?

    assert_success $create_exit "tmux session created successfully"

    # Check status detects it
    local output
    output=$(AUTX_SESSION_NAME="$TEST_SESSION" "$AUTX" status 2>&1)

    assert_contains "$output" "running" "status detects running session"
    assert_not_contains "$output" "not running" "status does not say 'not running' when session exists"

    # Stop should work
    AUTX_SESSION_NAME="$TEST_SESSION" "$AUTX" stop 2>/dev/null
    local stop_exit=$?

    assert_success $stop_exit "stop command exits successfully"

    # Verify session is gone
    if tmux has-session -t "$TEST_SESSION" 2>/dev/null; then
        echo -e "${RED}FAIL${NC}: session still exists after stop"
        ((failed++))
    else
        echo -e "${GREEN}PASS${NC}: session terminated after stop"
        ((passed++))
    fi

    echo ""
}

# Test 4: Send without session fails
test_send_without_session() {
    echo "=== Test: send without session fails ==="

    # Ensure no session exists
    local unique_name
    unique_name="nonexistent-$$-$(date +%s)"

    local output
    local exit_code=0
    output=$(AUTX_SESSION_NAME="$unique_name" "$AUTX" send "test message" 2>&1) || exit_code=$?

    assert_failure $exit_code "send fails without active session"
    assert_contains "$output" "No active session" "send shows appropriate error message"
    echo ""
}

# Test 5: Send without message fails
test_send_without_message() {
    echo "=== Test: send without message fails ==="

    # Create a session first
    tmux new-session -d -s "$TEST_SESSION" "sleep 60" 2>/dev/null || true

    local output
    local exit_code=0
    output=$(AUTX_SESSION_NAME="$TEST_SESSION" "$AUTX" send 2>&1) || exit_code=$?

    assert_failure $exit_code "send fails without message"
    assert_contains "$output" "No message" "send shows 'No message' error"

    # Cleanup
    tmux kill-session -t "$TEST_SESSION" 2>/dev/null || true
    echo ""
}

# Test 6: Environment variables are respected
test_environment_variables() {
    echo "=== Test: environment variables ==="

    local custom_name="custom-session-$$"

    # Test that custom session name is accepted
    local output
    output=$(AUTX_SESSION_NAME="$custom_name" "$AUTX" status 2>&1)
    local exit_code=$?

    assert_success $exit_code "custom session name accepted"
    assert_contains "$output" "$custom_name" "output mentions custom session name"

    # Test custom timeout (just check script doesn't fail with it set)
    output=$(AUTX_WAIT_TIMEOUT=60 AUTX_SESSION_NAME="$custom_name" "$AUTX" help 2>&1)
    exit_code=$?

    assert_success $exit_code "custom timeout variable accepted"
    echo ""
}

# Test 7: Read without output file
test_read_without_output() {
    echo "=== Test: read without output file ==="

    # Use a session name that won't have an output file
    local unique_name
    unique_name="no-output-$$-$(date +%s)"

    local output
    local exit_code=0
    output=$(AUTX_SESSION_NAME="$unique_name" "$AUTX" read 2>&1) || exit_code=$?

    assert_failure $exit_code "read fails without output file"
    assert_contains "$output" "No output file" "read shows appropriate error"
    echo ""
}

# Test 8: Attach without session fails
test_attach_without_session() {
    echo "=== Test: attach without session fails ==="

    local unique_name
    unique_name="no-attach-$$-$(date +%s)"

    local output
    local exit_code=0
    output=$(AUTX_SESSION_NAME="$unique_name" "$AUTX" attach 2>&1) || exit_code=$?

    assert_failure $exit_code "attach fails without active session"
    assert_contains "$output" "No active session" "attach shows appropriate error"
    echo ""
}

# Test 9: Stop when no session (should succeed gracefully)
test_stop_no_session() {
    echo "=== Test: stop with no session ==="

    local unique_name
    unique_name="no-stop-$$-$(date +%s)"

    local output
    local exit_code=0
    output=$(AUTX_SESSION_NAME="$unique_name" "$AUTX" stop 2>&1) || exit_code=$?

    assert_success $exit_code "stop succeeds gracefully when no session"
    assert_contains "$output" "No active session" "stop indicates no session was running"
    echo ""
}

# Test 10: Empty command shows help
test_empty_command() {
    echo "=== Test: empty command shows help ==="

    local output
    output=$("$AUTX" 2>&1)
    local exit_code=$?

    assert_success $exit_code "empty command exits successfully"
    assert_contains "$output" "USAGE" "empty command shows usage"
    echo ""
}

# Test 11: Cache directory creation
test_cache_directory() {
    echo "=== Test: cache directory handling ==="

    local cache_dir="${HOME}/.cache/unleash/unleashtx"

    # The script should create cache dir on any operation
    "$AUTX" help >/dev/null 2>&1

    if [[ -d "$cache_dir" ]]; then
        echo -e "${GREEN}PASS${NC}: cache directory exists at $cache_dir"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: cache directory not created"
        ((failed++))
    fi
    echo ""
}

# Test 12: Wait without session fails
test_wait_without_session() {
    echo "=== Test: wait without session fails ==="

    local unique_name
    unique_name="no-wait-$$-$(date +%s)"

    local output
    local exit_code=0
    output=$(AUTX_SESSION_NAME="$unique_name" "$AUTX" wait 1 2>&1) || exit_code=$?

    assert_failure $exit_code "wait fails without active session"
    assert_contains "$output" "No active session" "wait shows appropriate error"
    echo ""
}

# Test 13: Script has valid bash syntax
test_script_syntax() {
    echo "=== Test: script syntax validation ==="

    if bash -n "$AUTX" 2>/dev/null; then
        echo -e "${GREEN}PASS${NC}: unleashtx has valid bash syntax"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: unleashtx has invalid bash syntax"
        ((failed++))
    fi
    echo ""
}

# Test 14: Session name injection attempts
test_session_name_injection() {
    echo "=== Test: session name injection prevention ==="

    # Try various injection patterns
    local malicious_names=(
        "test; rm -rf /"
        "test\$(whoami)"
        "test|cat /etc/passwd"
        "test&& echo pwned"
        "test\`id\`"
        "test;whoami"
        "../../../etc/passwd"
        "test@domain.com"
        "test with spaces"
        "test/with/slashes"
    )

    local injection_blocked=0
    local injection_total=${#malicious_names[@]}

    for name in "${malicious_names[@]}"; do
        local output
        local exit_code=0
        output=$(AUTX_SESSION_NAME="$name" "$AUTX" status 2>&1) || exit_code=$?

        if [[ $exit_code -ne 0 ]] && [[ "$output" == *"Invalid session name"* ]]; then
            ((injection_blocked++))
        fi
    done

    if [[ $injection_blocked -eq $injection_total ]]; then
        echo -e "${GREEN}PASS${NC}: All $injection_total injection attempts blocked"
        ((passed++))
    else
        echo -e "${RED}FAIL${NC}: Only $injection_blocked/$injection_total injection attempts blocked"
        ((failed++))
    fi
    echo ""
}

# Test 15: Invalid numeric environment variables
test_invalid_numeric_env_vars() {
    echo "=== Test: invalid numeric environment variables ==="

    # Test invalid AUTX_WAIT_TIMEOUT
    local output
    local exit_code=0
    output=$(AUTX_WAIT_TIMEOUT="not-a-number" "$AUTX" help 2>&1) || exit_code=$?

    assert_failure $exit_code "script rejects non-numeric AUTX_WAIT_TIMEOUT"
    assert_contains "$output" "must be a positive integer" "error message mentions integer requirement"

    # Test invalid AUTX_TERM_WIDTH
    exit_code=0
    output=$(AUTX_TERM_WIDTH="abc" "$AUTX" help 2>&1) || exit_code=$?

    assert_failure $exit_code "script rejects non-numeric AUTX_TERM_WIDTH"
    assert_contains "$output" "must be a positive integer" "error message mentions integer requirement"

    echo ""
}

# Run all tests
main() {
    echo "========================================"
    echo "unleashtx Test Suite"
    echo "========================================"
    echo ""

    check_prerequisites

    test_script_syntax
    test_session_name_injection
    test_invalid_numeric_env_vars
    test_help_command
    test_empty_command
    test_status_no_session
    test_session_lifecycle
    test_send_without_session
    test_send_without_message
    test_environment_variables
    test_read_without_output
    test_attach_without_session
    test_stop_no_session
    test_wait_without_session
    test_cache_directory

    echo "========================================"
    echo "=== Results ==="
    echo -e "Passed:  ${GREEN}${passed}${NC}"
    echo -e "Failed:  ${RED}${failed}${NC}"
    if [[ $skipped -gt 0 ]]; then
        echo -e "Skipped: ${YELLOW}${skipped}${NC}"
    fi
    echo "========================================"

    exit $failed
}

main "$@"
