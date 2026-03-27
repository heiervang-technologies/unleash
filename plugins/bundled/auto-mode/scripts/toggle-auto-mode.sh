#!/usr/bin/env bash
# toggle-auto-mode.sh - Toggle auto mode on/off (wrapper-specific)

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/unleash/auto-mode"

# Use wrapper-specific flag file for session isolation
WRAPPER_PID="${AGENT_WRAPPER_PID:-${CLAUDE_WRAPPER_PID:-}}"
if [[ -z "${WRAPPER_PID}" ]]; then
    echo "Error: AGENT_WRAPPER_PID not set. Run under unleash wrapper."
    exit 1
fi

AUTO_MODE_FILE="${AUTO_MODE_DIR}/active-${WRAPPER_PID}"
mkdir -p "${AUTO_MODE_DIR}"

# Function to check if CLI is in auto mode by looking for »» in tmux pane
cli_in_auto_mode() {
    [[ -n "${TMUX:-}" ]] && tmux capture-pane -p 2>/dev/null | grep -q '»»'
}

# Function to sync CLI visual via tmux send-keys (shift+tab = BTab)
sync_cli_visual() {
    local target_auto="$1"  # "on" or "off"

    # Only works in tmux
    [[ -z "${TMUX:-}" ]] && return

    # Give CLI a moment to settle
    sleep 0.1

    if [[ "$target_auto" == "on" ]]; then
        # Cycle until we reach auto mode (»» indicator)
        # shellcheck disable=SC2034
        for _ in {1..4}; do
            if cli_in_auto_mode; then
                echo "(CLI synced to auto mode)"
                return
            fi
            tmux send-keys BTab
            sleep 0.2
        done
    else
        # If CLI is in auto mode, cycle once to leave
        if cli_in_auto_mode; then
            tmux send-keys BTab
            sleep 0.1
            echo "(CLI synced away from auto mode)"
        fi
    fi
}

if [[ -f "${AUTO_MODE_FILE}" ]]; then
    # Currently active - deactivate
    rm -f "${AUTO_MODE_FILE}"
    echo "AUTO MODE: OFF"
    echo "Normal operation resumed. You can end your turn when appropriate."
    sync_cli_visual "off"
else
    # Currently inactive - activate
    echo "${CLAUDE_SESSION_ID:-unknown}" > "${AUTO_MODE_FILE}"
    echo "AUTO MODE: ON"
    echo "Stop hook enforcement active. You cannot end your turn voluntarily."
    echo "Toggle off with /auto or exit with exit-claude"
    sync_cli_visual "on"
fi
