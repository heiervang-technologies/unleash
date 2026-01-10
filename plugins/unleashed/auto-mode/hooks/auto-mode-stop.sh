#!/usr/bin/env bash
# auto-mode-stop.sh - Stop hook that enforces auto mode (wrapper-specific)
#
# When auto mode is active for THIS wrapper, this hook blocks Claude from stopping.

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/claude-unleashed/auto-mode"

# Get wrapper PID - if not set, allow stop (not running under wrapper)
WRAPPER_PID="${CLAUDE_WRAPPER_PID:-}"
if [[ -z "${WRAPPER_PID}" ]]; then
    exit 0
fi

AUTO_MODE_FILE="${AUTO_MODE_DIR}/active-${WRAPPER_PID}"

# Check if auto mode is active for THIS wrapper
if [[ -f "${AUTO_MODE_FILE}" ]]; then
    # Check for custom reminder message
    REMINDER_FILE="${AUTO_MODE_DIR}/reminder-${WRAPPER_PID}"
    DEFAULT_MSG="To exit: run 'exit-claude' via Bash tool. Do not end your turn without taking action."

    if [[ -f "${REMINDER_FILE}" ]]; then
        REASON=$(cat "${REMINDER_FILE}")
    else
        REASON="${DEFAULT_MSG}"
    fi

    # Auto mode is active - redirect to check MCP before stopping
    cat <<EOF
{
  "decision": "block",
  "reason": "${REASON}"
}
EOF
    exit 0
fi

# Auto mode not active - allow normal stop
exit 0
