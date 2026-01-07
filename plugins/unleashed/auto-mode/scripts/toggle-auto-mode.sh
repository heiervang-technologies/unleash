#!/usr/bin/env bash
# toggle-auto-mode.sh - Toggle auto mode on/off (wrapper-specific)

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/claude-unleashed/auto-mode"

# Use wrapper-specific flag file for session isolation
WRAPPER_PID="${CLAUDE_WRAPPER_PID:-}"
if [[ -z "${WRAPPER_PID}" ]]; then
    echo "Error: CLAUDE_WRAPPER_PID not set. Run under claude-unleashed wrapper."
    exit 1
fi

AUTO_MODE_FILE="${AUTO_MODE_DIR}/active-${WRAPPER_PID}"
mkdir -p "${AUTO_MODE_DIR}"

if [[ -f "${AUTO_MODE_FILE}" ]]; then
    # Currently active - deactivate
    rm -f "${AUTO_MODE_FILE}"
    echo "AUTO MODE: OFF"
    echo "Normal operation resumed. You can end your turn when appropriate."
else
    # Currently inactive - activate
    echo "${CLAUDE_SESSION_ID:-unknown}" > "${AUTO_MODE_FILE}"
    echo "AUTO MODE: ON"
    echo "Stop hook enforcement active. You cannot end your turn voluntarily."
    echo "Toggle off with /auto or exit with exit-claude"
fi
