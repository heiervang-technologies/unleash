#!/usr/bin/env bash
# unpatch-claude.sh - Restore Claude Code to original state
#
# This script restores the most recent backup of cli.js

set -euo pipefail

# Find Claude Code installation
CLAUDE_BIN=$(which claude 2>/dev/null || echo "")
if [[ -z "$CLAUDE_BIN" ]]; then
    echo "Error: Claude Code not found in PATH"
    exit 1
fi

CLAUDE_REAL=$(readlink -f "$CLAUDE_BIN")
CLAUDE_DIR=$(dirname "$CLAUDE_REAL")
CLI_JS="$CLAUDE_DIR/cli.js"

# Find most recent backup
LATEST_BACKUP=$(ls -t "$CLI_JS.backup."* 2>/dev/null | head -1 || echo "")

if [[ -z "$LATEST_BACKUP" ]]; then
    echo "Error: No backup found"
    exit 1
fi

echo "Found backup: $LATEST_BACKUP"
echo "Restoring..."

cp "$LATEST_BACKUP" "$CLI_JS"
chmod +x "$CLI_JS"

echo "Restored to original state"
echo "Restart Claude Code to apply changes."
