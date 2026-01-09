#!/usr/bin/env bash
# test-patches.sh - Integration tests for patch functionality
#
# Tests:
# 1. Patch script syntax is valid
# 2. Patch script works on mock cli.js (both CT and kT variants)
# 3. Unpatch script works correctly
# 4. Patches are idempotent (running twice doesn't break anything)

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
PATCH_DIR="$REPO_ROOT/scripts"

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

# Create a mock cli.js with patterns that match the patch targets
create_mock_cli_js() {
    local variant="$1"  # "CT" or "kT"
    local mock_file="$2"

    # Create a minimal mock that contains all the patterns we patch
    cat > "$mock_file" << 'MOCK_EOF'
// Mock cli.js for testing patches
// This contains the patterns that the patch script looks for

// Modes array (VARIANT_PLACEHOLDER)
VARIANT_PLACEHOLDER=["acceptEdits","bypassPermissions","default","delegate","dontAsk","plan"];

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
if(Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="plan") {
    allowTool();
}
if(Q.mode==="bypassPermissions") {
    passthrough();
}
if(mode==="bypassPermissions"||V) {
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
MOCK_EOF

    # Variant-specific patterns
    if [[ "$variant" == "kT" ]]; then
        # v2.1.0+ patterns
        sed -i 's/VARIANT_PLACEHOLDER/kT/g' "$mock_file"
        cat >> "$mock_file" << 'EOF'

// v2.1.0+ mode transition patterns
if(JQ==="acceptEdits")O9("auto-accept-mode");
if(B.mode==="delegate"&&JQ!=="delegate")ty0(!0),_uA(!0);
EOF
    else
        # Legacy patterns
        sed -i 's/VARIANT_PLACEHOLDER/CT/g' "$mock_file"
        cat >> "$mock_file" << 'EOF'

// Legacy mode transition patterns
if(j1==="acceptEdits")v9("auto-accept-mode");
if(B.mode==="delegate"&&j1!=="delegate")YP0(!0),chA(!0);
var l9 = require("fs");
EOF
    fi
}

# Test 1: Patch script syntax is valid
test_patch_script_syntax() {
    echo ""
    echo "=== Testing patch script syntax ==="

    if bash -n "$PATCH_DIR/patch-claude.sh" 2>/dev/null; then
        pass "patch-claude.sh has valid bash syntax"
    else
        fail "patch-claude.sh has invalid bash syntax"
    fi

    if [[ -f "$PATCH_DIR/unpatch-claude.sh" ]]; then
        if bash -n "$PATCH_DIR/unpatch-claude.sh" 2>/dev/null; then
            pass "unpatch-claude.sh has valid bash syntax"
        else
            fail "unpatch-claude.sh has invalid bash syntax"
        fi
    fi
}

# Test 2: Patch works on mock cli.js (kT variant - v2.1.0+)
test_patch_kt_variant() {
    echo ""
    echo "=== Testing patches on kT variant (v2.1.0+) ==="

    local test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"
    local mock_claude="$test_dir/claude"

    # Create mock cli.js
    create_mock_cli_js "kT" "$mock_cli"

    # Create mock claude binary that points to cli.js
    cat > "$mock_claude" << EOF
#!/usr/bin/env bash
exec node "$mock_cli" "\$@"
EOF
    chmod +x "$mock_claude"

    # Run patch script with mock PATH
    export PATH="$test_dir:$PATH"

    # Patch should succeed
    if bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1; then
        pass "Patch script ran successfully (kT variant)"
    else
        fail "Patch script failed (kT variant)"
        rm -rf "$test_dir"
        return
    fi

    # Verify patches applied
    local failures=0

    if grep -q 'kT=\[.*"auto"' "$mock_cli"; then
        pass "Patch 1: auto added to modes array"
    else
        fail "Patch 1: auto NOT in modes array"
        ((failures++))
    fi

    if grep -q 'case"auto":return"Auto Mode"' "$mock_cli"; then
        pass "Patch 2: display name added"
    else
        fail "Patch 2: display name NOT added"
        ((failures++))
    fi

    if grep -q 'case"auto":return"»»"' "$mock_cli"; then
        pass "Patch 3: icon added"
    else
        fail "Patch 3: icon NOT added"
        ((failures++))
    fi

    if grep -q 'case"bypassPermissions":return"auto"' "$mock_cli"; then
        pass "Patch 4: cycling logic updated"
    else
        fail "Patch 4: cycling logic NOT updated"
        ((failures++))
    fi

    if grep -q 'mode==="auto"' "$mock_cli"; then
        pass "Patch 5: permission checks updated"
    else
        fail "Patch 5: permission checks NOT updated"
        ((failures++))
    fi

    if grep -q 'case"auto":return"warning"' "$mock_cli"; then
        pass "Patch 6: color added"
    else
        fail "Patch 6: color NOT added"
        ((failures++))
    fi

    # Test idempotency - running again should succeed without changes
    if bash "$PATCH_DIR/patch-claude.sh" 2>&1 | grep -q "Already patched"; then
        pass "Patch is idempotent"
    else
        fail "Patch is NOT idempotent"
        ((failures++))
    fi

    rm -rf "$test_dir"

    if [[ $failures -eq 0 ]]; then
        pass "All patches verified for kT variant"
    fi
}

# Test 3: Patch works on mock cli.js (CT variant - legacy)
test_patch_ct_variant() {
    echo ""
    echo "=== Testing patches on CT variant (legacy) ==="

    local test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"
    local mock_claude="$test_dir/claude"

    # Create mock cli.js
    create_mock_cli_js "CT" "$mock_cli"

    # Create mock claude binary
    cat > "$mock_claude" << EOF
#!/usr/bin/env bash
exec node "$mock_cli" "\$@"
EOF
    chmod +x "$mock_claude"

    # Run patch script with mock PATH
    export PATH="$test_dir:$PATH"

    if bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1; then
        pass "Patch script ran successfully (CT variant)"
    else
        fail "Patch script failed (CT variant)"
        rm -rf "$test_dir"
        return
    fi

    # Verify CT variant patched
    if grep -q 'CT=\[.*"auto"' "$mock_cli"; then
        pass "Patch 1: auto added to modes array (CT)"
    else
        fail "Patch 1: auto NOT in modes array (CT)"
    fi

    rm -rf "$test_dir"
}

# Test 4: Verify backup is created
test_backup_creation() {
    echo ""
    echo "=== Testing backup creation ==="

    local test_dir=$(mktemp -d)
    local mock_cli="$test_dir/cli.js"
    local mock_claude="$test_dir/claude"

    create_mock_cli_js "kT" "$mock_cli"

    cat > "$mock_claude" << EOF
#!/usr/bin/env bash
exec node "$mock_cli" "\$@"
EOF
    chmod +x "$mock_claude"

    export PATH="$test_dir:$PATH"

    bash "$PATCH_DIR/patch-claude.sh" > /dev/null 2>&1

    # Check backup exists
    if ls "$test_dir"/cli.js.backup.* 1>/dev/null 2>&1; then
        pass "Backup file created"
    else
        fail "Backup file NOT created"
    fi

    rm -rf "$test_dir"
}

# Run all tests
main() {
    echo "========================================"
    echo "Claude Unleashed Patch Integration Tests"
    echo "========================================"

    test_patch_script_syntax
    test_patch_kt_variant
    test_patch_ct_variant
    test_backup_creation

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
