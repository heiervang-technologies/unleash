#!/usr/bin/env bash
# check-and-patch.sh - Check if Claude Code needs patching and apply if necessary
#
# This script compares the current Claude Code version against the last patched
# version. If they differ (or no patched version recorded), it runs patch-claude.sh.
#
# Designed to be called on TUI startup for automatic patch maintenance.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VERSION_CACHE_DIR="$HOME/.cache/claude-unleashed"
VERSION_FILE="$VERSION_CACHE_DIR/patched-claude-version"

# Get current Claude version
CURRENT_VERSION=$(claude --version 2>/dev/null | head -1 || echo "")

if [[ -z "$CURRENT_VERSION" ]]; then
    # Claude not installed, nothing to do
    exit 0
fi

# Get stored patched version
PATCHED_VERSION=""
if [[ -f "$VERSION_FILE" ]]; then
    PATCHED_VERSION=$(cat "$VERSION_FILE" 2>/dev/null || echo "")
fi

# Compare versions
if [[ "$CURRENT_VERSION" != "$PATCHED_VERSION" ]]; then
    echo "Claude version changed: '$PATCHED_VERSION' -> '$CURRENT_VERSION'"
    echo "Applying patches..."
    "$SCRIPT_DIR/patch-claude.sh"
else
    # Silent success - version matches
    :
fi
