#!/bin/bash
# Test script for JSON output feature

set -e

CU="./target/release/cu"

echo "========================================"
echo "Testing JSON Output Feature"
echo "========================================"
echo

echo "1. Testing cu --version --json"
echo "--------------------------------------"
$CU --version --json | jq '.'
echo

echo "2. Extracting specific version with jq"
echo "--------------------------------------"
CLAUDE_CODE_VERSION=$($CU --version --json | jq -r '.claude_code_version')
echo "Claude Code version: $CLAUDE_CODE_VERSION"
echo

echo "3. Testing cu version --list --json (first 3 versions)"
echo "--------------------------------------"
$CU version --list --json | jq '.versions[:3]'
echo

echo "4. Finding currently installed version"
echo "--------------------------------------"
$CU version --list --json | jq -r '.currently_installed'
echo

echo "5. Listing versions with patches available"
echo "--------------------------------------"
$CU version --list --json | jq -r '.versions[] | select(.has_patch == true) | .version' | head -5
echo

echo "6. Counting whitelisted versions"
echo "--------------------------------------"
WHITELISTED_COUNT=$($CU version --list --json | jq '[.versions[] | select(.is_whitelisted == true)] | length')
echo "Number of whitelisted versions: $WHITELISTED_COUNT"
echo

echo "6b. Counting blacklisted versions"
echo "--------------------------------------"
BLACKLISTED_COUNT=$($CU version --list --json | jq '[.versions[] | select(.is_blacklisted == true)] | length')
echo "Number of blacklisted versions: $BLACKLISTED_COUNT"
echo

echo "6c. Checking version filter mode"
echo "--------------------------------------"
FILTER_MODE=$($CU version --list --json | jq -r '.filter_mode')
echo "Current filter mode: $FILTER_MODE"
echo

echo "7. Testing cu auth-check --json"
echo "--------------------------------------"
$CU auth-check --json | jq '.'
echo

echo "8. Checking authentication status programmatically"
echo "--------------------------------------"
if $CU auth-check --json | jq -e '.authenticated' > /dev/null; then
    echo "✓ Authentication configured"
    METHOD=$($CU auth-check --json | jq -r '.method')
    echo "  Method: $METHOD"
else
    echo "✗ Authentication not configured"
fi
echo

echo "9. Testing cu auth-check --json --verbose"
echo "--------------------------------------"
$CU auth-check --json --verbose | jq '.'
echo

echo "10. Comparing normal vs JSON output"
echo "--------------------------------------"
echo "Normal output:"
$CU --version
echo
echo "JSON output:"
$CU --version --json | jq -c '.'
echo

echo "========================================"
echo "All tests passed! ✓"
echo "========================================"
