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
    # Auto mode is active - block the stop
    cat <<EOF
{
  "decision": "block",
  "reason": "AUTO MODE ACTIVE: You cannot stop. Continue working on pending tasks, check your todo list, or look for incomplete work. If truly idle with no work remaining, use your MCP tools to check for tasks or wait for owner input. Only the owner saying 'stop/quit/exit' or running 'exit-claude' can end this session."
}
EOF
    exit 0
fi

# Auto mode not active - allow normal stop
exit 0
