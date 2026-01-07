#!/usr/bin/env bash
# activate-auto-mode.sh - Activates auto mode by creating the flag file

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/claude-unleashed/auto-mode"
AUTO_MODE_FILE="${AUTO_MODE_DIR}/active"

mkdir -p "${AUTO_MODE_DIR}"

# Store session info
echo "${CLAUDE_SESSION_ID:-unknown}" > "${AUTO_MODE_FILE}"

echo "Auto mode activated. Flag: ${AUTO_MODE_FILE}"
