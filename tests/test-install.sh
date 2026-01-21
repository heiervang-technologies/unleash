#!/usr/bin/env bash
# test-install.sh - Tests for install.sh functionality
#
# Tests:
# 1. Symlink creation for claude binary
# 2. Skip symlink when non-symlink file exists
# 3. Update symlink when it already exists

set -uo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m' # No Color

TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

pass() {
    echo -e "${GREEN}PASS${NC}: $1"
    ((TESTS_PASSED++))
    ((TESTS_RUN++))
}

fail() {
    echo -e "${RED}FAIL${NC}: $1"
    ((TESTS_FAILED++))
    ((TESTS_RUN++))
}

# Create a temporary test directory
setup_test_env() {
    TEST_DIR=$(mktemp -d)
    TEST_BIN_DIR="$TEST_DIR/bin"
    FAKE_CLAUDE="$TEST_DIR/fake-claude"
    mkdir -p "$TEST_BIN_DIR"

    # Create a fake claude binary
    echo '#!/bin/bash' > "$FAKE_CLAUDE"
    echo 'echo "1.0.0 (Claude Code)"' >> "$FAKE_CLAUDE"
    chmod +x "$FAKE_CLAUDE"
}

cleanup_test_env() {
    if [[ -d "$TEST_DIR" ]]; then
        rm -rf "$TEST_DIR"
    fi
}

# Helper function to simulate the symlink logic from install.sh
# This mirrors the actual implementation to ensure tests stay in sync
create_claude_symlink() {
    local CLAUDE_BIN="$1"
    local BIN_DIR="$2"

    if [[ -n "$CLAUDE_BIN" ]]; then
        # Resolve to actual binary path to avoid circular symlinks
        CLAUDE_REAL=$(readlink -f "$CLAUDE_BIN" 2>/dev/null || realpath "$CLAUDE_BIN" 2>/dev/null || echo "$CLAUDE_BIN")
        TARGET_PATH="$BIN_DIR/claude"

        # Get real path of target to compare (if it exists)
        TARGET_REAL=""
        if [[ -e "$TARGET_PATH" ]] || [[ -L "$TARGET_PATH" ]]; then
            TARGET_REAL=$(readlink -f "$TARGET_PATH" 2>/dev/null || realpath "$TARGET_PATH" 2>/dev/null || echo "$TARGET_PATH")
        fi

        if [[ "$CLAUDE_REAL" == "$TARGET_REAL" ]]; then
            echo "already_correct"
        elif [[ ! -e "$TARGET_PATH" ]] || [[ -L "$TARGET_PATH" ]]; then
            ln -sf "$CLAUDE_REAL" "$TARGET_PATH"
            echo "created"
        else
            echo "skipped_not_symlink"
        fi
    else
        echo "no_claude_bin"
    fi
}

# Test 1: Symlink is created when claude binary exists and no file at target
test_symlink_created() {
    echo ""
    echo "=== Testing symlink creation ==="

    setup_test_env

    # Simulate the symlink logic from install.sh
    RESULT=$(create_claude_symlink "$FAKE_CLAUDE" "$TEST_BIN_DIR")

    # Verify symlink was created
    if [[ -L "$TEST_BIN_DIR/claude" ]]; then
        LINK_TARGET=$(readlink "$TEST_BIN_DIR/claude")
        EXPECTED_TARGET=$(readlink -f "$FAKE_CLAUDE" 2>/dev/null || realpath "$FAKE_CLAUDE" 2>/dev/null || echo "$FAKE_CLAUDE")
        if [[ "$LINK_TARGET" == "$EXPECTED_TARGET" ]]; then
            pass "Symlink created correctly: $TEST_BIN_DIR/claude -> $LINK_TARGET"
        else
            fail "Symlink points to wrong target: $LINK_TARGET (expected $EXPECTED_TARGET)"
        fi
    else
        fail "Symlink was not created at $TEST_BIN_DIR/claude"
    fi

    cleanup_test_env
}

# Test 2: Symlink is NOT created when non-symlink file exists
test_skip_existing_file() {
    echo ""
    echo "=== Testing skip when regular file exists ==="

    setup_test_env

    # Create a real file (not a symlink) at the target
    echo "#!/bin/bash" > "$TEST_BIN_DIR/claude"
    chmod +x "$TEST_BIN_DIR/claude"

    # Get the original inode
    ORIGINAL_INODE=$(stat -c '%i' "$TEST_BIN_DIR/claude" 2>/dev/null || stat -f '%i' "$TEST_BIN_DIR/claude" 2>/dev/null)

    # Simulate the symlink logic from install.sh
    RESULT=$(create_claude_symlink "$FAKE_CLAUDE" "$TEST_BIN_DIR")

    # Verify the file was NOT replaced (not a symlink)
    if [[ ! -L "$TEST_BIN_DIR/claude" ]]; then
        CURRENT_INODE=$(stat -c '%i' "$TEST_BIN_DIR/claude" 2>/dev/null || stat -f '%i' "$TEST_BIN_DIR/claude" 2>/dev/null)
        if [[ "$ORIGINAL_INODE" == "$CURRENT_INODE" ]] && [[ "$RESULT" == "skipped_not_symlink" ]]; then
            pass "Existing regular file preserved (not replaced with symlink)"
        else
            fail "File was modified unexpectedly"
        fi
    else
        fail "Regular file was replaced with symlink (should have been skipped)"
    fi

    cleanup_test_env
}

# Test 3: Symlink is updated when symlink already exists
test_update_existing_symlink() {
    echo ""
    echo "=== Testing symlink update when symlink exists ==="

    setup_test_env

    # Create an old symlink pointing to a different location
    OLD_TARGET="$TEST_DIR/old-claude"
    echo '#!/bin/bash' > "$OLD_TARGET"
    chmod +x "$OLD_TARGET"
    ln -s "$OLD_TARGET" "$TEST_BIN_DIR/claude"

    # Verify old symlink exists
    if [[ ! -L "$TEST_BIN_DIR/claude" ]]; then
        fail "Setup failed: old symlink not created"
        cleanup_test_env
        return
    fi

    # Simulate the symlink logic from install.sh
    RESULT=$(create_claude_symlink "$FAKE_CLAUDE" "$TEST_BIN_DIR")

    # Verify symlink was updated
    if [[ -L "$TEST_BIN_DIR/claude" ]]; then
        LINK_TARGET=$(readlink "$TEST_BIN_DIR/claude")
        EXPECTED_TARGET=$(readlink -f "$FAKE_CLAUDE" 2>/dev/null || realpath "$FAKE_CLAUDE" 2>/dev/null || echo "$FAKE_CLAUDE")
        if [[ "$LINK_TARGET" == "$EXPECTED_TARGET" ]]; then
            pass "Existing symlink updated correctly: $TEST_BIN_DIR/claude -> $LINK_TARGET"
        else
            fail "Symlink not updated: still points to $LINK_TARGET"
        fi
    else
        fail "Symlink was replaced with regular file"
    fi

    cleanup_test_env
}

# Test 4: No action when claude binary not found
test_no_claude_binary() {
    echo ""
    echo "=== Testing no action when claude binary not found ==="

    setup_test_env

    # Simulate the symlink logic with empty CLAUDE_BIN
    RESULT=$(create_claude_symlink "" "$TEST_BIN_DIR")

    # Verify no symlink was created
    if [[ ! -e "$TEST_BIN_DIR/claude" ]] && [[ "$RESULT" == "no_claude_bin" ]]; then
        pass "No symlink created when claude binary not found"
    else
        fail "Symlink was created even without claude binary"
    fi

    cleanup_test_env
}

# Test 5: Verify symlink is functional (executable)
test_symlink_functional() {
    echo ""
    echo "=== Testing symlink is functional ==="

    setup_test_env

    # Simulate the symlink logic from install.sh
    create_claude_symlink "$FAKE_CLAUDE" "$TEST_BIN_DIR" > /dev/null

    # Verify the symlink is executable and works
    if [[ -x "$TEST_BIN_DIR/claude" ]]; then
        OUTPUT=$("$TEST_BIN_DIR/claude" 2>&1)
        if [[ "$OUTPUT" == "1.0.0 (Claude Code)" ]]; then
            pass "Symlink is functional and executable"
        else
            fail "Symlink execution produced unexpected output: $OUTPUT"
        fi
    else
        fail "Symlink is not executable"
    fi

    cleanup_test_env
}

# Test 6: No circular symlink when PATH includes BIN_DIR with existing symlink
test_no_circular_symlink() {
    echo ""
    echo "=== Testing no circular symlink when re-running install ==="

    setup_test_env

    # Create initial symlink (simulating first install)
    ln -s "$FAKE_CLAUDE" "$TEST_BIN_DIR/claude"

    # Verify initial symlink is correct
    if [[ ! -L "$TEST_BIN_DIR/claude" ]]; then
        fail "Setup failed: initial symlink not created"
        cleanup_test_env
        return
    fi

    # Simulate running install again with PATH including BIN_DIR
    # This is the critical scenario: command -v claude resolves to the symlink itself
    CLAUDE_BIN="$TEST_BIN_DIR/claude"  # This simulates what command -v would return

    # Run the symlink logic (should NOT create circular symlink)
    RESULT=$(create_claude_symlink "$CLAUDE_BIN" "$TEST_BIN_DIR")

    # Verify symlink still works (not circular)
    if [[ -L "$TEST_BIN_DIR/claude" ]]; then
        LINK_TARGET=$(readlink "$TEST_BIN_DIR/claude")

        # Check that it doesn't point to itself
        if [[ "$LINK_TARGET" == "$TEST_BIN_DIR/claude" ]]; then
            fail "Circular symlink created: $TEST_BIN_DIR/claude -> $LINK_TARGET"
            cleanup_test_env
            return
        fi

        # Verify the symlink is still functional
        if [[ -x "$TEST_BIN_DIR/claude" ]]; then
            OUTPUT=$("$TEST_BIN_DIR/claude" 2>&1)
            if [[ "$OUTPUT" == "1.0.0 (Claude Code)" ]]; then
                pass "No circular symlink: re-install preserved working symlink (result: $RESULT)"
            else
                fail "Symlink execution produced unexpected output: $OUTPUT"
            fi
        else
            fail "Symlink is not executable after re-install"
        fi
    else
        fail "Symlink was replaced with regular file"
    fi

    cleanup_test_env
}

# Run all tests
main() {
    echo "========================================"
    echo "Claude Unleashed Install Script Tests"
    echo "========================================"

    test_symlink_created
    test_skip_existing_file
    test_update_existing_symlink
    test_no_claude_binary
    test_symlink_functional
    test_no_circular_symlink

    echo ""
    echo "========================================"
    echo "Test Results: $TESTS_PASSED/$TESTS_RUN passed"
    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "${RED}$TESTS_FAILED tests failed${NC}"
        exit 1
    else
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    fi
}

main "$@"
