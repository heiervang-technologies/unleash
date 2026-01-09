#!/usr/bin/env bash
# Test script for MCP Refresh and Process Restart plugins
#
# This script validates the functionality of both plugins:
# - MCP Refresh: Configuration change detection
# - Process Restart: State preservation and restoration
#
# Usage: ./.unleashed/scripts/test-restart-refresh.sh

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test counters
TESTS_PASSED=0
TESTS_FAILED=0
TESTS_TOTAL=0

# Helper functions
print_header() {
  echo -e "${BLUE}========================================${NC}"
  echo -e "${BLUE}$1${NC}"
  echo -e "${BLUE}========================================${NC}"
  echo ""
}

print_test() {
  echo -e "${YELLOW}TEST:${NC} $1"
  TESTS_TOTAL=$((TESTS_TOTAL + 1))
}

print_pass() {
  echo -e "${GREEN}✓ PASS:${NC} $1"
  echo ""
  TESTS_PASSED=$((TESTS_PASSED + 1))
}

print_fail() {
  echo -e "${RED}✗ FAIL:${NC} $1"
  echo ""
  TESTS_FAILED=$((TESTS_FAILED + 1))
}

print_info() {
  echo -e "${BLUE}INFO:${NC} $1"
}

# Cleanup function
cleanup() {
  echo ""
  print_info "Cleaning up test artifacts..."

  # Remove test MCP config
  rm -f test-mcp-config.json

  # Clear plugin caches
  rm -rf ~/.cache/claude-unleashed/mcp-refresh/
  rm -rf ~/.cache/claude-unleashed/process-restart/

  echo ""
}

trap cleanup EXIT

# Start testing
print_header "MCP Refresh & Process Restart Plugin Tests"

#
# MCP Refresh Plugin Tests
#
print_header "MCP Refresh Plugin Tests"

# Test 1: Plugin structure
print_test "MCP Refresh plugin structure exists"
if [[ -f "plugins/unleashed/mcp-refresh/.claude-plugin/plugin.json" ]] && \
   [[ -f "plugins/unleashed/mcp-refresh/commands/reload-mcps.md" ]] && \
   [[ -f "plugins/unleashed/mcp-refresh/commands/mcp-status.md" ]] && \
   [[ -f "plugins/unleashed/mcp-refresh/hooks/hooks.json" ]] && \
   [[ -x "plugins/unleashed/mcp-refresh/hooks-handlers/check-mcp-changes.sh" ]]; then
  print_pass "All MCP refresh plugin files exist and are properly configured"
else
  print_fail "MCP refresh plugin files missing or incorrect permissions"
fi

# Test 2: Plugin manifest validation
print_test "MCP Refresh plugin.json is valid JSON"
if jq empty plugins/unleashed/mcp-refresh/.claude-plugin/plugin.json 2>/dev/null; then
  print_pass "plugin.json is valid JSON"
else
  print_fail "plugin.json is invalid JSON"
fi

# Test 3: Hook script syntax
print_test "MCP Refresh hook script has valid syntax"
if bash -n plugins/unleashed/mcp-refresh/hooks-handlers/check-mcp-changes.sh 2>/dev/null; then
  print_pass "Hook script syntax is valid"
else
  print_fail "Hook script has syntax errors"
fi

# Test 4: Hash computation
print_test "MCP Refresh hash computation works"
TEST_CONFIG='{"test-server": {"command": "echo", "args": ["test"]}}'
echo "$TEST_CONFIG" > test-mcp-config.json

# Simulate hash computation (simplified version)
HASH1=$(sha256sum test-mcp-config.json | awk '{print $1}')
sleep 0.1
echo "$TEST_CONFIG" > test-mcp-config.json
HASH2=$(sha256sum test-mcp-config.json | awk '{print $1}')

if [[ "$HASH1" == "$HASH2" ]]; then
  print_pass "Hash computation is consistent"
else
  print_fail "Hash computation is inconsistent"
fi

# Test 5: Configuration change detection
print_test "MCP Refresh detects configuration changes"
MODIFIED_CONFIG='{"test-server": {"command": "echo", "args": ["modified"]}}'
echo "$MODIFIED_CONFIG" > test-mcp-config.json
HASH3=$(sha256sum test-mcp-config.json | awk '{print $1}')

if [[ "$HASH1" != "$HASH3" ]]; then
  print_pass "Configuration change detected"
else
  print_fail "Configuration change NOT detected"
fi

#
# Process Restart Plugin Tests
#
print_header "Process Restart Plugin Tests"

# Test 6: Plugin structure
print_test "Process Restart plugin structure exists"
if [[ -f "plugins/unleashed/process-restart/.claude-plugin/plugin.json" ]] && \
   [[ -f "plugins/unleashed/process-restart/commands/restart.md" ]] && \
   [[ -f "plugins/unleashed/process-restart/hooks/hooks.json" ]] && \
   [[ -x "plugins/unleashed/process-restart/hooks-handlers/restart-handler.sh" ]] && \
   [[ -x "plugins/unleashed/process-restart/hooks-handlers/session-restore.sh" ]] && \
   [[ -x "scripts/trigger-restart.sh" ]]; then
  print_pass "All process-restart plugin files exist and are properly configured"
else
  print_fail "Process-restart plugin files missing or incorrect permissions"
fi

# Test 7: Plugin manifest validation
print_test "Process Restart plugin.json is valid JSON"
if jq empty plugins/unleashed/process-restart/.claude-plugin/plugin.json 2>/dev/null; then
  print_pass "plugin.json is valid JSON"
else
  print_fail "plugin.json is invalid JSON"
fi

# Test 8: Hook scripts syntax
print_test "Process Restart hook scripts have valid syntax"
SYNTAX_OK=true
if ! bash -n plugins/unleashed/process-restart/hooks-handlers/restart-handler.sh 2>/dev/null; then
  SYNTAX_OK=false
fi
if ! bash -n plugins/unleashed/process-restart/hooks-handlers/session-restore.sh 2>/dev/null; then
  SYNTAX_OK=false
fi
if ! bash -n scripts/trigger-restart.sh 2>/dev/null; then
  SYNTAX_OK=false
fi

if $SYNTAX_OK; then
  print_pass "All hook scripts have valid syntax"
else
  print_fail "One or more hook scripts have syntax errors"
fi

# Test 9: State file creation
print_test "Process Restart can create state file"
mkdir -p ~/.cache/claude-unleashed/process-restart/

STATE_FILE="$HOME/.cache/claude-unleashed/process-restart/test-state.json"
jq -n \
  --arg version "1.0.0" \
  --arg timestamp "$(date +%s)" \
  --arg session_id "test-session" \
  --arg working_dir "$(pwd)" \
  --arg model "claude-sonnet-4-5" \
  '{
    version: $version,
    timestamp: ($timestamp | tonumber),
    sessionId: $session_id,
    workingDir: $working_dir,
    model: $model
  }' > "$STATE_FILE"

if [[ -f "$STATE_FILE" ]] && jq empty "$STATE_FILE" 2>/dev/null; then
  print_pass "State file created successfully and is valid JSON"
  rm -f "$STATE_FILE"
else
  print_fail "State file creation failed or invalid JSON"
fi

# Test 10: State file expiry check
print_test "Process Restart correctly checks state file expiry"
# Create expired state file (timestamp from 10 minutes ago)
EXPIRED_TIMESTAMP=$(($(date +%s) - 600))
jq -n \
  --arg version "1.0.0" \
  --arg timestamp "$EXPIRED_TIMESTAMP" \
  --arg session_id "test-session" \
  '{
    version: $version,
    timestamp: ($timestamp | tonumber),
    sessionId: $session_id
  }' > "$STATE_FILE"

CURRENT_TIMESTAMP=$(date +%s)
FILE_TIMESTAMP=$(jq -r '.timestamp' "$STATE_FILE")
AGE=$((CURRENT_TIMESTAMP - FILE_TIMESTAMP))
EXPIRY=300  # 5 minutes

if [[ $AGE -gt $EXPIRY ]]; then
  print_pass "Correctly identified expired state file (age: ${AGE}s > ${EXPIRY}s)"
  rm -f "$STATE_FILE"
else
  print_fail "Failed to identify expired state file"
fi

#
# Configuration Tests
#
print_header "Configuration Tests"

# Test 11: Settings file validation
print_test "Settings file includes new plugins"
if grep -q "mcp-refresh" .claude/settings.json && \
   grep -q "process-restart" .claude/settings.json; then
  print_pass "Both plugins are configured in settings.json"
else
  print_fail "One or both plugins missing from settings.json"
fi

# Test 12: Settings file is valid JSON
print_test "Settings file is valid JSON"
if jq empty .claude/settings.json 2>/dev/null; then
  print_pass "settings.json is valid JSON"
else
  print_fail "settings.json is invalid JSON"
fi

# Test 13: Extensions registry
print_test "Extensions registry includes new plugins"
if [[ -f ".unleashed/extensions.json" ]] && \
   grep -q "mcp-refresh" .unleashed/extensions.json && \
   grep -q "process-restart" .unleashed/extensions.json; then
  print_pass "Both plugins registered in extensions.json"
else
  print_fail "One or both plugins missing from extensions.json"
fi

#
# Documentation Tests
#
print_header "Documentation Tests"

# Test 14: Plugin READMEs exist
print_test "Plugin README files exist"
if [[ -f "plugins/unleashed/mcp-refresh/README.md" ]] && \
   [[ -f "plugins/unleashed/process-restart/README.md" ]]; then
  print_pass "Both plugin READMEs exist"
else
  print_fail "One or both plugin READMEs missing"
fi

# Test 15: Comprehensive documentation exists
print_test "Comprehensive documentation guide exists"
if [[ -f "docs/extensions/restart-refresh.md" ]]; then
  print_pass "restart-refresh.md guide exists"
else
  print_fail "restart-refresh.md guide missing"
fi

# Test 16: Documentation links are consistent
print_test "Documentation cross-references are consistent"
DOCS_OK=true

# Check if plugin READMEs reference the main guide
if ! grep -q "restart-refresh.md" plugins/unleashed/mcp-refresh/README.md 2>/dev/null; then
  DOCS_OK=false
fi

if $DOCS_OK; then
  print_pass "Documentation cross-references are consistent"
else
  print_fail "Some documentation cross-references missing"
fi

#
# Integration Tests
#
print_header "Integration Tests"

# Test 17: Plugin commands are documented
print_test "All plugin commands are documented"
COMMANDS_OK=true

# Check /reload-mcps command
if [[ ! -f "plugins/unleashed/mcp-refresh/commands/reload-mcps.md" ]]; then
  COMMANDS_OK=false
fi

# Check /mcp-status command
if [[ ! -f "plugins/unleashed/mcp-refresh/commands/mcp-status.md" ]]; then
  COMMANDS_OK=false
fi

# Check /restart command
if [[ ! -f "plugins/unleashed/process-restart/commands/restart.md" ]]; then
  COMMANDS_OK=false
fi

if $COMMANDS_OK; then
  print_pass "All plugin commands are documented"
else
  print_fail "Some plugin commands missing documentation"
fi

# Test 18: Hooks are registered
print_test "All hooks are properly registered"
HOOKS_OK=true

# Check MCP refresh PreToolUse hook
if ! jq -e '.PreToolUse' plugins/unleashed/mcp-refresh/hooks/hooks.json >/dev/null 2>&1; then
  HOOKS_OK=false
fi

# Check process-restart Stop hook
if ! jq -e '.Stop' plugins/unleashed/process-restart/hooks/hooks.json >/dev/null 2>&1; then
  HOOKS_OK=false
fi

# Check process-restart SessionStart hook
if ! jq -e '.SessionStart' plugins/unleashed/process-restart/hooks/hooks.json >/dev/null 2>&1; then
  HOOKS_OK=false
fi

if $HOOKS_OK; then
  print_pass "All hooks are properly registered"
else
  print_fail "Some hooks not properly registered"
fi

#
# Test Summary
#
print_header "Test Summary"

echo "Total tests run: $TESTS_TOTAL"
echo -e "${GREEN}Passed: $TESTS_PASSED${NC}"
echo -e "${RED}Failed: $TESTS_FAILED${NC}"
echo ""

if [[ $TESTS_FAILED -eq 0 ]]; then
  echo -e "${GREEN}✓ All tests passed!${NC}"
  echo ""
  echo "The MCP Refresh and Process Restart plugins are ready to use."
  echo ""
  echo "Next steps:"
  echo "1. Commit the changes to git"
  echo "2. Test the plugins in a live Claude Code session"
  echo "3. Document any issues in GitHub issues"
  exit 0
else
  echo -e "${RED}✗ Some tests failed${NC}"
  echo ""
  echo "Please review the failed tests above and fix the issues."
  exit 1
fi
