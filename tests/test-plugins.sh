#!/usr/bin/env bash
# test-plugins.sh - Integration tests for plugin functionality
#
# Tests:
# 1. Plugin JSON files are valid
# 2. Hook scripts are valid bash
# 3. Hook scripts produce expected output format
# 4. Commands/skills exist and are readable

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
PLUGINS_DIR="$REPO_ROOT/plugins"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

warn() {
    echo -e "${YELLOW}WARN${NC}: $1"
}

# Test 1: Validate all plugin.json files
test_plugin_json_valid() {
    echo ""
    echo "=== Testing plugin.json validity ==="

    while read -r json_file; do
        if python3 -c "import json; json.load(open('$json_file'))" 2>/dev/null; then
            pass "Valid JSON: $json_file"
        else
            fail "Invalid JSON: $json_file"
        fi
    done < <(find "$PLUGINS_DIR" -name "plugin.json" 2>/dev/null)
}

# Test 2: Validate all hook scripts have valid bash syntax
test_hook_scripts_syntax() {
    echo ""
    echo "=== Testing hook script syntax ==="

    while read -r hook_file; do
        if bash -n "$hook_file" 2>/dev/null; then
            pass "Valid bash: $hook_file"
        else
            fail "Invalid bash syntax: $hook_file"
        fi
    done < <(find "$PLUGINS_DIR" -type f -name "*.sh" -path "*/hooks/*" 2>/dev/null)

    while read -r hook_file; do
        if bash -n "$hook_file" 2>/dev/null; then
            pass "Valid bash: $hook_file"
        else
            fail "Invalid bash syntax: $hook_file"
        fi
    done < <(find "$PLUGINS_DIR" -type f -name "*.sh" -path "*/hooks-handlers/*" 2>/dev/null)
}

# Test 3: Test auto-mode stop hook produces valid JSON when active
test_auto_mode_hook_output() {
    echo ""
    echo "=== Testing auto-mode stop hook output ==="

    HOOK_FILE="$PLUGINS_DIR/bundled/auto-mode/hooks/auto-mode-stop.sh"

    if [[ ! -f "$HOOK_FILE" ]]; then
        fail "auto-mode-stop.sh not found"
        return
    fi

    # Test 1: Without wrapper PID, should exit cleanly (allow stop)
    output=$(AGENT_WRAPPER_PID="" bash "$HOOK_FILE" 2>&1) || true
    if [[ -z "$output" ]]; then
        pass "Hook allows stop when no wrapper PID"
    else
        fail "Hook should produce no output without wrapper PID"
    fi

    # Test 2: With wrapper PID but no flag file, should exit cleanly
    output=$(AGENT_WRAPPER_PID="99999" bash "$HOOK_FILE" 2>&1) || true
    if [[ -z "$output" ]]; then
        pass "Hook allows stop when no flag file"
    else
        fail "Hook should produce no output without flag file"
    fi

    # Test 3: With wrapper PID and flag file, should block with JSON
    FLAG_DIR="$HOME/.cache/unleash/auto-mode"
    mkdir -p "$FLAG_DIR"
    TEST_PID="$$"
    touch "$FLAG_DIR/active-$TEST_PID"

    output=$(AGENT_WRAPPER_PID="$TEST_PID" bash "$HOOK_FILE" 2>&1)
    rm -f "$FLAG_DIR/active-$TEST_PID"

    if echo "$output" | python3 -c "import json,sys; d=json.load(sys.stdin); exit(0 if d.get('decision')=='block' else 1)" 2>/dev/null; then
        pass "Hook blocks with valid JSON when flag file exists"
    else
        fail "Hook should produce JSON with decision:block"
    fi
}

# Test 4: Verify command/skill markdown files exist
test_commands_exist() {
    echo ""
    echo "=== Testing command files ==="

    while read -r cmd_file; do
        if [[ -s "$cmd_file" ]]; then
            pass "Command exists: $cmd_file"
        else
            fail "Empty command file: $cmd_file"
        fi
    done < <(find "$PLUGINS_DIR" -path "*/commands/*.md" 2>/dev/null)
}

# Test 5: Verify plugin structure
test_plugin_structure() {
    echo ""
    echo "=== Testing plugin structure ==="

    # Each plugin with plugin.json should have required fields
    while read -r json_file; do
        plugin_name=$(python3 -c "import json; print(json.load(open('$json_file')).get('name', ''))" 2>/dev/null)

        if [[ -n "$plugin_name" ]]; then
            pass "Plugin has name: $plugin_name"
        else
            fail "Plugin missing name field: $json_file"
        fi

        version=$(python3 -c "import json; print(json.load(open('$json_file')).get('version', ''))" 2>/dev/null)
        if [[ -n "$version" ]]; then
            pass "Plugin has version: $plugin_name ($version)"
        else
            warn "Plugin missing version: $plugin_name"
        fi
    done < <(find "$PLUGINS_DIR" -name "plugin.json" 2>/dev/null)
}

# Run all tests
main() {
    echo "========================================"
    echo "unleash Plugin Integration Tests"
    echo "========================================"

    test_plugin_json_valid
    test_hook_scripts_syntax
    test_auto_mode_hook_output
    test_commands_exist
    test_plugin_structure

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
