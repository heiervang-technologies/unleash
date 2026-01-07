#!/usr/bin/env bash
# auto-mode-stop.sh - Stop hook that enforces auto mode
#
# When auto mode is active, this hook blocks Claude from stopping
# and forces it to continue working.

set -uo pipefail

AUTO_MODE_FILE="${HOME}/.cache/claude-unleashed/auto-mode/active"

# Check if auto mode is active
if [[ -f "${AUTO_MODE_FILE}" ]]; then
    # Read the session ID from the file to ensure we're in the right session
    STORED_SESSION=$(cat "${AUTO_MODE_FILE}" 2>/dev/null || echo "")

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
