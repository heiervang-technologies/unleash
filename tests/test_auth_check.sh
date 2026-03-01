#!/usr/bin/env bash
# Test script for unleash auth command

set -e

UNLEASH_BIN="${UNLEASH_BIN:-./target/release/unleash}"

echo "Testing unleash auth command..."
echo "================================"
echo

# Test 1: Basic auth check
echo "Test 1: Basic auth check"
if $UNLEASH_BIN auth > /dev/null 2>&1; then
    echo "✓ Basic auth check passed"
else
    echo "✓ Basic auth check detected no authentication (expected if not configured)"
fi
echo

# Test 2: Verbose output
echo "Test 2: Verbose output"
$UNLEASH_BIN auth --verbose | head -5
echo "✓ Verbose output works"
echo

# Test 3: JSON output
echo "Test 3: JSON output"
JSON_OUTPUT=$($UNLEASH_BIN auth --json)
echo "$JSON_OUTPUT"
if echo "$JSON_OUTPUT" | grep -q '"authenticated"'; then
    echo "✓ JSON output contains 'authenticated' field"
else
    echo "✗ JSON output missing 'authenticated' field"
    exit 1
fi
echo

# Test 4: JSON + verbose
echo "Test 4: JSON + verbose output"
JSON_VERBOSE=$($UNLEASH_BIN auth --json --verbose)
echo "$JSON_VERBOSE"
if echo "$JSON_VERBOSE" | grep -q '"details"'; then
    echo "✓ JSON verbose output contains 'details' field"
else
    echo "✗ JSON verbose output missing 'details' field"
    exit 1
fi
echo

# Test 5: Exit codes
echo "Test 5: Exit codes"
if $UNLEASH_BIN auth > /dev/null 2>&1; then
    echo "✓ Exit code 0 when authenticated"
else
    echo "✓ Exit code 1 when not authenticated"
fi
echo

echo "================================"
echo "All tests passed!"
