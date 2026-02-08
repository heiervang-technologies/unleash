#!/usr/bin/env bash
# test-patches.sh - Comprehensive integration tests for patch functionality
#
# Tests:
# 1. Patch script syntax is valid
# 2. Patch script works on all whitelisted versions
# 3. Unpatch script works correctly
# 4. Patches are idempotent (running twice doesn't break anything)
# 5. Version fallback logic works correctly
# 6. Patch 7 (flag file integration) is applied correctly
# 7. All patches are verified individually per version
# 8. Auto mode flag file integration with stop hook:
#    - Patch 7a injects flag creation code
#    - Patch 7b injects flag removal code
#    - Flag creation JS works and creates file
#    - Stop hook blocks when flag exists
#    - Flag removal JS works and removes file
#    - Stop hook allows when flag doesn't exist
# 9. Backup file is created before patching

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
PATCH_DIR="$REPO_ROOT/scripts"
PATCHES_VERSIONS_DIR="$REPO_ROOT/scripts/patches/versions"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

TESTS_RUN=0
TESTS_PASSED=0
TESTS_FAILED=0

pass() {
    echo -e "  ${GREEN}PASS${NC}: $1"
    ((TESTS_PASSED++))
    ((TESTS_RUN++))
}

fail() {
    echo -e "  ${RED}FAIL${NC}: $1"
    ((TESTS_FAILED++))
    ((TESTS_RUN++))
}

section() {
    echo ""
    echo -e "${BLUE}=== $1 ===${NC}"
}

# Discover whitelisted versions dynamically from .conf files
discover_versions() {
    local versions=()
    for conf in "$PATCHES_VERSIONS_DIR"/*.conf; do
        [[ -f "$conf" ]] || continue
        local ver
        ver=$(basename "$conf" .conf)
        versions+=("$ver")
    done
    # Sort versions
    printf '%s\n' "${versions[@]}" | sort -V
}

# Whitelisted versions to test (discovered from .conf files)
mapfile -t VERSIONS < <(discover_versions)

# Version-specific variable mappings (loaded from .conf files)
declare -A VERSION_MODES_ARRAY_VAR
declare -A VERSION_MODE_VAR
declare -A VERSION_TELEMETRY_FN
declare -A VERSION_DELEGATE_FN1
declare -A VERSION_DELEGATE_FN2
declare -A VERSION_PERMISSION_CTX_VAR
declare -A VERSION_PERMISSION_BOOL_VAR
declare -A VERSION_TOOL_PERMISSION_CTX
declare -A VERSION_PASSTHROUGH_MODE_VAR
declare -A VERSION_DELEGATE_MODE_CTX

# Load version configs
load_version_configs() {
    for version in "${VERSIONS[@]}"; do
        local conf_file="$PATCHES_VERSIONS_DIR/${version}.conf"
        if [[ -f "$conf_file" ]]; then
            # Source the config in a subshell to extract variables
            eval "$(grep -E '^(MODES_ARRAY_VAR|MODE_VAR|TELEMETRY_FN|DELEGATE_FN1|DELEGATE_FN2|PERMISSION_CTX_VAR|PERMISSION_BOOL_VAR|TOOL_PERMISSION_CTX|PASSTHROUGH_MODE_VAR|DELEGATE_MODE_CTX)=' "$conf_file")"
            VERSION_MODES_ARRAY_VAR[$version]="$MODES_ARRAY_VAR"
            # shellcheck disable=SC2153
            VERSION_MODE_VAR[$version]="$MODE_VAR"
            VERSION_TELEMETRY_FN[$version]="$TELEMETRY_FN"
            VERSION_DELEGATE_FN1[$version]="$DELEGATE_FN1"
            VERSION_DELEGATE_FN2[$version]="$DELEGATE_FN2"
            VERSION_PERMISSION_CTX_VAR[$version]="$PERMISSION_CTX_VAR"
            VERSION_PERMISSION_BOOL_VAR[$version]="${PERMISSION_BOOL_VAR:-V}"
            VERSION_TOOL_PERMISSION_CTX[$version]="${TOOL_PERMISSION_CTX:-Z}"
            VERSION_PASSTHROUGH_MODE_VAR[$version]="${PASSTHROUGH_MODE_VAR:-Q}"
            VERSION_DELEGATE_MODE_CTX[$version]="${DELEGATE_MODE_CTX:-B}"
        fi
    done
}

# Create a version-specific mock cli.js
create_mock_cli_js() {
    local version="$1"
    local mock_file="$2"

    local modes_var="${VERSION_MODES_ARRAY_VAR[$version]}"
    local mode_var="${VERSION_MODE_VAR[$version]}"
    local telemetry_fn="${VERSION_TELEMETRY_FN[$version]}"
    local delegate_fn1="${VERSION_DELEGATE_FN1[$version]}"
    local delegate_fn2="${VERSION_DELEGATE_FN2[$version]}"
    local perm_ctx_var="${VERSION_PERMISSION_CTX_VAR[$version]}"
    local perm_bool_var="${VERSION_PERMISSION_BOOL_VAR[$version]:-V}"
    local tool_perm_ctx="${VERSION_TOOL_PERMISSION_CTX[$version]:-Z}"
    local passthrough_var="${VERSION_PASSTHROUGH_MODE_VAR[$version]:-Q}"
    local delegate_ctx="${VERSION_DELEGATE_MODE_CTX[$version]:-B}"

    # Create a mock cli.js with patterns that match the patch targets
    cat > "$mock_file" << EOF
// Mock cli.js for testing patches - Version $version
// This contains the patterns that the patch script looks for

// Modes array
${modes_var}=["acceptEdits","bypassPermissions","default","delegate","dontAsk","plan"];

// Display names
function getModeName(mode) {
    switch(mode) {
        case"plan":return"Plan Mode";
        case"bypassPermissions":return"Bypass Permissions";
        case"acceptEdits":return"Accept Edits";
        default:return"Default";
    }
}

// Icons
function getModeIcon(mode) {
    switch(mode) {
        case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵";
        default:return"▶";
    }
}

// Mode cycling
function getNextMode(mode) {
    switch(mode) {
        case"default":return"plan";
        case"plan":return"bypassPermissions";
        case"bypassPermissions":return"default";
    }
}

// Permission checks
if(${tool_perm_ctx}.toolPermissionContext.mode==="bypassPermissions"||${tool_perm_ctx}.toolPermissionContext.mode==="plan") {
    allowTool();
}
if(${passthrough_var}.mode==="bypassPermissions") {
    passthrough();
}
if(${perm_ctx_var}.mode==="bypassPermissions"||${perm_bool_var}) {
    allow();
}

// Colors
function getModeColor(mode) {
    switch(mode) {
        case"plan":return"planMode";
        case"acceptEdits":return"autoAccept";case"bypassPermissions":return"error";
        default:return"default";
    }
}

// Mode transition patterns (Patch 7)
if(${mode_var}==="acceptEdits")${telemetry_fn}("auto-accept-mode");
if(${delegate_ctx}.mode==="delegate"&&${mode_var}!=="delegate")${delegate_fn1}(!0),${delegate_fn2}(!0);
EOF
}

# Create a mock Claude binary
create_mock_claude_binary() {
    local version="$1"
    local mock_dir="$2"
    local mock_cli="$mock_dir/cli.js"

    cat > "$mock_dir/claude" << EOF
#!/usr/bin/env bash
if [[ "\$1" == "--version" ]]; then
    echo "$version (Claude Code)"
    exit 0
fi
exec node "$mock_cli" "\$@"
EOF
    chmod +x "$mock_dir/claude"
}

# ============================================================================
# TEST 1: Script Syntax Validation
# ============================================================================
test_script_syntax() {
    section "Testing script syntax"

    if bash -n "$PATCH_DIR/patch-claude.sh" 2>/dev/null; then
        pass "patch-claude.sh has valid bash syntax"
    else
        fail "patch-claude.sh has invalid bash syntax"
    fi

    if bash -n "$PATCH_DIR/unpatch-claude.sh" 2>/dev/null; then
        pass "unpatch-claude.sh has valid bash syntax"
    else
        fail "unpatch-claude.sh has invalid bash syntax"
    fi

    if bash -n "$PATCH_DIR/check-and-patch.sh" 2>/dev/null; then
        pass "check-and-patch.sh has valid bash syntax"
    else
        fail "check-and-patch.sh has invalid bash syntax"
    fi
}

# ============================================================================
# TEST 2: Version Config Files Exist
# ============================================================================
test_version_configs_exist() {
    section "Testing version config files"

    for version in "${VERSIONS[@]}"; do
        local conf_file="$PATCHES_VERSIONS_DIR/${version}.conf"
        if [[ -f "$conf_file" ]]; then
            pass "Config exists for version $version"
        else
            fail "Config missing for version $version"
        fi
    done
}

# ============================================================================
# TEST 3: Patch All Whitelisted Versions
# ============================================================================
test_patch_version() {
    local version="$1"

    section "Testing patches for version $version"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    # Create mock cli.js for this version
    create_mock_cli_js "$version" "$mock_cli"
    create_mock_claude_binary "$version" "$test_dir"

    local modes_var="${VERSION_MODES_ARRAY_VAR[$version]}"
    local mode_var="${VERSION_MODE_VAR[$version]}"
    local telemetry_fn="${VERSION_TELEMETRY_FN[$version]}"
    local perm_ctx_var="${VERSION_PERMISSION_CTX_VAR[$version]}"
    local tool_perm_ctx="${VERSION_TOOL_PERMISSION_CTX[$version]:-Z}"
    local passthrough_var="${VERSION_PASSTHROUGH_MODE_VAR[$version]:-Q}"
    local delegate_ctx="${VERSION_DELEGATE_MODE_CTX[$version]:-B}"

    # Run patch script
    if CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1; then
        pass "Patch script ran successfully"
    else
        fail "Patch script failed"
        rm -rf "$test_dir"
        return 1
    fi

    local failures=0

    # Patch 1: auto added to modes array
    if grep -q "${modes_var}=\[.*\"auto\"" "$mock_cli"; then
        pass "Patch 1: 'auto' added to modes array (${modes_var})"
    else
        fail "Patch 1: 'auto' NOT in modes array (${modes_var})"
        ((failures++))
    fi

    # Patch 2: display name added
    if grep -q 'case"auto":return"Auto Mode"' "$mock_cli"; then
        pass "Patch 2: display name 'Auto Mode' added"
    else
        fail "Patch 2: display name NOT added"
        ((failures++))
    fi

    # Patch 3: icon added
    if grep -q 'case"auto":return"»»"' "$mock_cli"; then
        pass "Patch 3: icon '»»' added"
    else
        fail "Patch 3: icon NOT added"
        ((failures++))
    fi

    # Patch 4: cycling logic updated
    if grep -q 'case"bypassPermissions":return"auto"' "$mock_cli" && \
       grep -q 'case"auto":return"default"' "$mock_cli"; then
        pass "Patch 4: cycling logic updated (bypass->auto->default)"
    else
        fail "Patch 4: cycling logic NOT updated"
        ((failures++))
    fi

    # Patch 5a: Main permission check
    if grep -q "mode===\"auto\"||${tool_perm_ctx}\.toolPermissionContext\.mode===\"plan\"" "$mock_cli"; then
        pass "Patch 5a: main permission check updated"
    else
        fail "Patch 5a: main permission check NOT updated"
        ((failures++))
    fi

    # Patch 5b: passthrough mode check
    if grep -q "${passthrough_var}\.mode===\"bypassPermissions\"||${passthrough_var}\.mode===\"auto\"" "$mock_cli"; then
        pass "Patch 5b: ${passthrough_var}.mode passthrough updated"
    else
        fail "Patch 5b: ${passthrough_var}.mode passthrough NOT updated"
        ((failures++))
    fi

    # Patch 5c: Permission context variable check
    if grep -q "${perm_ctx_var}\.mode===\"auto\"" "$mock_cli"; then
        pass "Patch 5c: ${perm_ctx_var}.mode permission check updated"
    else
        fail "Patch 5c: ${perm_ctx_var}.mode permission check NOT updated"
        ((failures++))
    fi

    # Patch 6: color added
    if grep -q 'case"auto":return"warning"' "$mock_cli"; then
        pass "Patch 6: warning color added for auto mode"
    else
        fail "Patch 6: warning color NOT added"
        ((failures++))
    fi

    # Patch 7a: flag file creation
    if grep -q "${mode_var}===\"auto\")import(\"fs\")" "$mock_cli" && \
       grep -q "auto-mode" "$mock_cli" && \
       grep -q "mkdirSync" "$mock_cli"; then
        pass "Patch 7a: flag file creation injected (${mode_var})"
    else
        fail "Patch 7a: flag file creation NOT injected"
        ((failures++))
    fi

    # Patch 7b: flag file removal
    if grep -q "${delegate_ctx}\.mode===\"auto\"" "$mock_cli" && \
       grep -q "unlinkSync" "$mock_cli"; then
        pass "Patch 7b: flag file removal injected"
    else
        fail "Patch 7b: flag file removal NOT injected"
        ((failures++))
    fi

    rm -rf "$test_dir"
    return $failures
}

# ============================================================================
# TEST 4: Patch Idempotency
# ============================================================================
test_patch_idempotency() {
    local version="$1"

    section "Testing patch idempotency for version $version"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    create_mock_cli_js "$version" "$mock_cli"
    create_mock_claude_binary "$version" "$test_dir"

    # First patch
    CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Save state after first patch
    local first_patch_md5
    first_patch_md5=$(md5sum "$mock_cli" | cut -d' ' -f1)

    # Second patch
    local output
    output=$(CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" 2>&1)

    if echo "$output" | grep -q "Already patched"; then
        pass "Patch correctly detects already-patched state"
    else
        fail "Patch does not detect already-patched state"
    fi

    # Verify file unchanged
    local second_patch_md5
    second_patch_md5=$(md5sum "$mock_cli" | cut -d' ' -f1)

    if [[ "$first_patch_md5" == "$second_patch_md5" ]]; then
        pass "File unchanged after second patch attempt"
    else
        fail "File was modified by second patch attempt"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# TEST 5: Auto Mode Flag File Integration with Stop Hook
# ============================================================================
test_auto_mode_flag_integration() {
    section "Testing auto mode flag file integration with stop hook"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"
    local mock_claude="$test_dir/claude"
    local flag_dir="$HOME/.cache/agent-unleashed/auto-mode"
    local test_pid="$$"

    # Ensure cleanup on exit or error
    trap 'rm -rf "$test_dir" 2>/dev/null; rm -f "$flag_dir/active-$test_pid" 2>/dev/null' RETURN

    # Create mock cli.js with 2.1.12 patterns (current version)
    cat > "$mock_cli" << 'MOCK_EOF'
// Mock cli.js for 2.1.12
xL=["acceptEdits","bypassPermissions","default","delegate","dontAsk","plan"];

function getModeName(mode) {
    switch(mode) {
        case"plan":return"Plan Mode";
        case"bypassPermissions":return"Bypass Permissions";
        default:return"Default";
    }
}

function getModeIcon(mode) {
    switch(mode) {
        case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵";
        default:return"▶";
    }
}

function getNextMode(mode) {
    switch(mode) {
        case"default":return"plan";
        case"plan":return"bypassPermissions";
        case"bypassPermissions":return"default";
    }
}

if(Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="plan") {
    allowTool();
}
if(Q.mode==="bypassPermissions") {
    passthrough();
}
if(PQ.mode==="bypassPermissions"||K) {
    allow();
}

function getModeColor(mode) {
    switch(mode) {
        case"plan":return"planMode";
        case"acceptEdits":return"autoAccept";case"bypassPermissions":return"error";
        default:return"default";
    }
}

if(F0==="acceptEdits")b2("auto-accept-mode");
if(B.mode==="delegate"&&F0!=="delegate")rf0(!0),IdA(!0);
MOCK_EOF

    cat > "$mock_claude" << EOF
#!/usr/bin/env bash
if [[ "\$1" == "--version" ]]; then
    echo "2.1.12 (Claude Code)"
    exit 0
fi
exec node "$mock_cli" "\$@"
EOF
    chmod +x "$mock_claude"

    # Run patch script
    CLAUDE_BIN="$mock_claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Test 5a: Verify patch 7a (flag creation) is in the patched file
    if grep -q 'F0==="auto".*writeFileSync.*active-' "$mock_cli"; then
        pass "Patch 7a: Flag creation code injected"
    else
        fail "Patch 7a: Flag creation code NOT found"
    fi

    # Test 5b: Verify patch 7b (flag removal) is in the patched file
    if grep -q 'B\.mode==="auto"&&F0!=="auto".*unlinkSync' "$mock_cli"; then
        pass "Patch 7b: Flag removal code injected"
    else
        fail "Patch 7b: Flag removal code NOT found"
    fi

    # Test 5c: Extract and test the flag creation JavaScript
    # Simulate what happens when entering auto mode
    mkdir -p "$flag_dir"
    local js_create='
        const fs = require("fs");
        const d = process.env.HOME + "/.cache/agent-unleashed/auto-mode";
        fs.mkdirSync(d, {recursive: true});
        fs.writeFileSync(d + "/active-" + process.env.TEST_PID, "");
        console.log("created");
    '
    if TEST_PID="$test_pid" node -e "$js_create" 2>/dev/null; then
        if [[ -f "$flag_dir/active-$test_pid" ]]; then
            pass "Patch 7a JS: Flag file created successfully"
        else
            fail "Patch 7a JS: Flag file NOT created"
        fi
    else
        fail "Patch 7a JS: JavaScript execution failed"
    fi

    # Test 5d: Verify stop hook blocks when flag exists
    local hook_script="$REPO_ROOT/plugins/unleashed/auto-mode/hooks/auto-mode-stop.sh"
    if [[ -f "$hook_script" ]]; then
        local hook_output
        hook_output=$(CLAUDE_WRAPPER_PID="$test_pid" bash "$hook_script" 2>&1)
        if echo "$hook_output" | grep -q '"decision".*:.*"block"'; then
            pass "Stop hook: Blocks when flag file exists"
        else
            fail "Stop hook: Should block but got: $hook_output"
        fi
    else
        fail "Stop hook script not found at $hook_script"
    fi

    # Test 5e: Extract and test the flag removal JavaScript
    local js_remove='
        const fs = require("fs");
        try {
            fs.unlinkSync(process.env.HOME + "/.cache/agent-unleashed/auto-mode/active-" + process.env.TEST_PID);
            console.log("removed");
        } catch(e) {
            console.log("error: " + e.message);
        }
    '
    if TEST_PID="$test_pid" node -e "$js_remove" 2>/dev/null; then
        if [[ ! -f "$flag_dir/active-$test_pid" ]]; then
            pass "Patch 7b JS: Flag file removed successfully"
        else
            fail "Patch 7b JS: Flag file NOT removed"
        fi
    else
        fail "Patch 7b JS: JavaScript execution failed"
    fi

    # Test 5f: Verify stop hook allows when flag doesn't exist
    if [[ -f "$hook_script" ]]; then
        local hook_output
        hook_output=$(CLAUDE_WRAPPER_PID="$test_pid" bash "$hook_script" 2>&1)
        if [[ -z "$hook_output" ]]; then
            pass "Stop hook: Allows stop when no flag file"
        else
            fail "Stop hook: Should allow but got output: $hook_output"
        fi
    fi

    # Cleanup handled by trap RETURN
}

# ============================================================================
# TEST 6: Unpatch Functionality
# ============================================================================
test_unpatch() {
    local version="$1"

    section "Testing unpatch for version $version"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    create_mock_cli_js "$version" "$mock_cli"
    create_mock_claude_binary "$version" "$test_dir"

    # Save original content
    local original_md5
    original_md5=$(md5sum "$mock_cli" | cut -d' ' -f1)

    # Apply patch
    CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Verify patched
    local patched_md5
    patched_md5=$(md5sum "$mock_cli" | cut -d' ' -f1)

    if [[ "$original_md5" != "$patched_md5" ]]; then
        pass "File was modified by patch"
    else
        fail "File was NOT modified by patch"
        rm -rf "$test_dir"
        return 1
    fi

    # Check backup exists
    if ls "$test_dir"/cli.js.backup.* 1>/dev/null 2>&1; then
        pass "Backup file created"
    else
        fail "Backup file NOT created"
        rm -rf "$test_dir"
        return 1
    fi

    # Run the actual unpatch script with CLAUDE_BIN override
    local unpatch_output
    if unpatch_output=$(CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/unpatch-claude.sh" 2>&1); then
        pass "unpatch-claude.sh ran successfully"
    else
        fail "unpatch-claude.sh failed: $unpatch_output"
        rm -rf "$test_dir"
        return 1
    fi

    # Verify unpatch output mentions restoration
    if echo "$unpatch_output" | grep -qi "restored"; then
        pass "Unpatch reported restoration"
    else
        fail "Unpatch did not report restoration"
    fi

    # Verify restored
    local restored_md5
    restored_md5=$(md5sum "$mock_cli" | cut -d' ' -f1)

    if [[ "$original_md5" == "$restored_md5" ]]; then
        pass "File restored to original state"
    else
        fail "File NOT restored to original state"
    fi

    # Verify auto mode no longer exists
    local modes_var="${VERSION_MODES_ARRAY_VAR[$version]}"
    if ! grep -q "${modes_var}=\[.*\"auto\"" "$mock_cli"; then
        pass "Auto mode removed after unpatch"
    else
        fail "Auto mode still present after unpatch"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# TEST 7: Version Fallback Logic
# ============================================================================
test_version_fallback() {
    section "Testing version fallback logic"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    # Test with a version that doesn't have exact config (e.g., 2.1.1)
    # Should fall back to 2.1.0
    create_mock_cli_js "2.1.0" "$mock_cli"

    cat > "$test_dir/claude" << 'EOF'
#!/usr/bin/env bash
if [[ "$1" == "--version" ]]; then
    echo "2.1.1 (Claude Code)"
    exit 0
fi
EOF
    chmod +x "$test_dir/claude"

    local output
    output=$(CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" 2>&1)

    if echo "$output" | grep -q "Using patch config: 2.1.0.conf"; then
        pass "Version 2.1.1 falls back to 2.1.0.conf"
    else
        fail "Version 2.1.1 did not fall back correctly"
    fi

    rm -rf "$test_dir"

    # Test with a future version (e.g., 2.2.0)
    # Should fall back to the latest available
    test_dir=$(mktemp -d)
    mock_cli="$test_dir/cli.js"

    # Use the latest version config we have
    local latest_version="${VERSIONS[-1]}"
    create_mock_cli_js "$latest_version" "$mock_cli"

    cat > "$test_dir/claude" << 'EOF'
#!/usr/bin/env bash
if [[ "$1" == "--version" ]]; then
    echo "2.2.0 (Claude Code)"
    exit 0
fi
EOF
    chmod +x "$test_dir/claude"

    output=$(CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" 2>&1)

    if echo "$output" | grep -q "Using patch config: ${latest_version}.conf"; then
        pass "Version 2.2.0 falls back to latest (${latest_version}.conf)"
    else
        fail "Version 2.2.0 did not fall back to latest"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# TEST 8: Backup File Naming
# ============================================================================
test_backup_naming() {
    section "Testing backup file naming"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    # Use the latest version
    local latest_version="${VERSIONS[-1]}"
    create_mock_cli_js "$latest_version" "$mock_cli"
    create_mock_claude_binary "$latest_version" "$test_dir"

    CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Check backup exists with timestamp format
    local backup_file
    backup_file=$(ls "$test_dir"/cli.js.backup.* 2>/dev/null | head -1)

    if [[ -n "$backup_file" ]]; then
        # Verify timestamp format (YYYYMMDDHHMMSS)
        local backup_name
        backup_name=$(basename "$backup_file")
        if [[ "$backup_name" =~ ^cli\.js\.backup\.[0-9]{14}$ ]]; then
            pass "Backup file has correct timestamp format"
        else
            fail "Backup file has incorrect format: $backup_name"
        fi
    else
        fail "No backup file created"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# TEST 9: Multiple Backups Preserved
# ============================================================================
test_multiple_backups() {
    section "Testing multiple backups preserved"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    # Use the latest version
    local latest_version="${VERSIONS[-1]}"
    create_mock_cli_js "$latest_version" "$mock_cli"
    create_mock_claude_binary "$latest_version" "$test_dir"

    # First patch
    CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Remove patch marker to allow re-patching
    sed -i 's/"auto",//g' "$mock_cli"

    sleep 1  # Ensure different timestamp

    # Second patch
    CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Count backups
    local backup_count
    backup_count=$(ls "$test_dir"/cli.js.backup.* 2>/dev/null | wc -l)

    if [[ "$backup_count" -ge 2 ]]; then
        pass "Multiple backups preserved ($backup_count backups)"
    else
        fail "Multiple backups NOT preserved (only $backup_count)"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# TEST 10: Patch 7 Flag File Paths
# ============================================================================
test_patch7_flag_paths() {
    section "Testing Patch 7 flag file path generation"

    local test_dir
    test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"

    # Use the latest version
    local latest_version="${VERSIONS[-1]}"
    create_mock_cli_js "$latest_version" "$mock_cli"
    create_mock_claude_binary "$latest_version" "$test_dir"

    CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Check flag directory path
    if grep -q '\.cache/agent-unleashed/auto-mode' "$mock_cli"; then
        pass "Patch 7: correct flag directory path"
    else
        fail "Patch 7: incorrect flag directory path"
    fi

    # Check process.ppid usage
    if grep -q 'process\.ppid' "$mock_cli"; then
        pass "Patch 7: uses process.ppid for flag file name"
    else
        fail "Patch 7: does not use process.ppid"
    fi

    # Check HOME environment variable usage
    if grep -q 'process\.env\.HOME' "$mock_cli"; then
        pass "Patch 7: uses HOME env var for path"
    else
        fail "Patch 7: does not use HOME env var"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# TEST 11: Error Handling - Missing cli.js
# ============================================================================
test_error_missing_cli() {
    section "Testing error handling for missing cli.js"

    local test_dir
    test_dir=$(mktemp -d)

    # Create claude binary but no cli.js
    cat > "$test_dir/claude" << 'EOF'
#!/usr/bin/env bash
if [[ "$1" == "--version" ]]; then
    echo "2.1.5 (Claude Code)"
    exit 0
fi
EOF
    chmod +x "$test_dir/claude"

    local output
    local exit_code
    output=$(CLAUDE_BIN="$test_dir/claude" bash "$PATCH_DIR/patch-claude.sh" 2>&1) || exit_code=$?

    if [[ "${exit_code:-0}" -ne 0 ]] && echo "$output" | grep -qi "error.*cli.js"; then
        pass "Correct error for missing cli.js"
    else
        fail "Missing error handling for absent cli.js"
    fi

    rm -rf "$test_dir"
}

# ============================================================================
# MAIN
# ============================================================================
main() {
    echo "========================================"
    echo "Claude Unleashed Patch Integration Tests"
    echo "========================================"
    echo "Testing ${#VERSIONS[@]} whitelisted versions: ${VERSIONS[*]}"

    # Load version configs
    load_version_configs

    # Run syntax tests
    test_script_syntax

    # Run config existence tests
    test_version_configs_exist

    # Run version-specific tests
    for version in "${VERSIONS[@]}"; do
        test_patch_version "$version"
        test_patch_idempotency "$version"
        test_unpatch "$version"
    done

    # Run auto mode flag integration test
    test_auto_mode_flag_integration

    # Run general tests
    test_version_fallback
    test_backup_naming
    test_multiple_backups
    test_patch7_flag_paths
    test_error_missing_cli

    # Summary
    echo ""
    echo "========================================"
    echo "Test Results Summary"
    echo "========================================"
    echo "Total tests:   $TESTS_RUN"
    echo -e "Passed:        ${GREEN}$TESTS_PASSED${NC}"
    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "Failed:        ${RED}$TESTS_FAILED${NC}"
    else
        echo "Failed:        $TESTS_FAILED"
    fi
    echo "========================================"

    if [[ $TESTS_FAILED -gt 0 ]]; then
        echo -e "${RED}Some tests failed!${NC}"
        exit 1
    else
        echo -e "${GREEN}All tests passed!${NC}"
        exit 0
    fi
}

main "$@"
