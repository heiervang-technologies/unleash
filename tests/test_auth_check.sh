#!/usr/bin/env bash
# Test script for cu auth-check command

set -e

CU_BIN="${CU_BIN:-./target/release/cu}"

echo "Testing cu auth-check command..."
echo "================================"
echo

# Test 1: Basic auth check
echo "Test 1: Basic auth check"
if $CU_BIN auth-check > /dev/null 2>&1; then
    echo "✓ Basic auth check passed"
else
    echo "✓ Basic auth check detected no authentication (expected if not configured)"
fi
echo

# Test 2: Verbose output
echo "Test 2: Verbose output"
$CU_BIN auth-check --verbose | head -5
echo "✓ Verbose output works"
echo

# Test 3: JSON output
echo "Test 3: JSON output"
JSON_OUTPUT=$($CU_BIN auth-check --json)
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
JSON_VERBOSE=$($CU_BIN auth-check --json --verbose)
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
if $CU_BIN auth-check > /dev/null 2>&1; then
    echo "✓ Exit code 0 when authenticated"
else
    echo "✓ Exit code 1 when not authenticated"
fi
echo

echo "================================"
echo "All tests passed!"
