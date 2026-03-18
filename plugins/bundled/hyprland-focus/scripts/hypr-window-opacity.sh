#!/usr/bin/env bash
# hypr-window-opacity.sh - Set opacity on the terminal window hosting this agent
#
# Usage:
#   hypr-window-opacity.sh set <opacity>   # Set opacity (0.0-1.0)
#   hypr-window-opacity.sh reset           # Reset to fully opaque
#   hypr-window-opacity.sh address         # Print window address
#
# Environment:
#   AU_HYPRLAND_FOCUS=0              Disable (default: enabled when Hyprland detected)
#   AU_FOCUS_OPACITY_ACTIVE=0.7      Focused window opacity while agent runs (default: 0.7)
#   AU_FOCUS_OPACITY_INACTIVE=0.4    Unfocused window opacity while agent runs (default: 0.4)
#
# State file: /tmp/unleash-hyprfocus-<wrapper_pid>

set -euo pipefail

# --- Guards ---

# Disabled by user?
if [[ "${AU_HYPRLAND_FOCUS:-}" == "0" ]]; then
    exit 0
fi

# Running under Hyprland?
if [[ -z "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    exit 0
fi

# jq required
if ! command -v jq &>/dev/null; then
    exit 0
fi

# --- State ---

# Use wrapper PID for state file so restarts share the same window
STATE_DIR="/tmp/unleash-hyprfocus"
mkdir -p "$STATE_DIR"

WRAPPER_PID="${AGENT_WRAPPER_PID:-$$}"
STATE_FILE="$STATE_DIR/$WRAPPER_PID"

# --- Functions ---

# Walk a PID up its process tree looking for a Hyprland window match.
walk_pid_to_window() {
    local pid="$1"
    local clients="$2"

    while [[ "$pid" -gt 1 ]]; do
        local addr
        addr=$(echo "$clients" | jq -r --argjson pid "$pid" '.[] | select(.pid==$pid) | .address' 2>/dev/null)
        if [[ -n "$addr" && "$addr" != "null" ]]; then
            echo "$addr"
            return 0
        fi
        pid=$(ps -o ppid= -p "$pid" 2>/dev/null | tr -d ' ') || break
        [[ -z "$pid" ]] && break
    done

    return 1
}

# Find the Hyprland window address for our terminal.
# Strategy 1: Walk up from our PID (works for direct terminals)
# Strategy 2: If in tmux, get the tmux client PID and walk from there
find_window_address() {
    local clients
    clients=$(hyprctl clients -j 2>/dev/null) || return 1

    # Try direct PID walk first
    local addr
    addr=$(walk_pid_to_window $$ "$clients") && { echo "$addr"; return 0; }

    # If in tmux, find the terminal hosting our pane
    if [[ -n "${TMUX_PANE:-}" ]]; then
        local client_pid
        client_pid=$(tmux display-message -p -t "$TMUX_PANE" '#{client_pid}' 2>/dev/null) || return 1
        if [[ -n "$client_pid" && "$client_pid" -gt 1 ]]; then
            addr=$(walk_pid_to_window "$client_pid" "$clients") && { echo "$addr"; return 0; }
        fi
    fi

    return 1
}

# Get or discover the window address (cached in state file)
get_address() {
    # Check state file first
    if [[ -f "$STATE_FILE" ]]; then
        cat "$STATE_FILE"
        return 0
    fi

    # Discover and cache
    local addr
    addr=$(find_window_address) || return 1
    echo "$addr" > "$STATE_FILE"
    echo "$addr"
}

set_opacity() {
    local active="$1"
    local inactive="$2"
    local addr
    addr=$(get_address) || return 1
    # opacity = focused/hovered, opacity_inactive = unfocused/unhovered
    hyprctl --batch \
        "dispatch setprop address:$addr opacity $active override ; \
         dispatch setprop address:$addr opacity_inactive $inactive override" \
        &>/dev/null || true
}

# --- Main ---

case "${1:-}" in
    set)
        set_opacity "${AU_FOCUS_OPACITY_ACTIVE:-0.7}" "${AU_FOCUS_OPACITY_INACTIVE:-0.4}"
        ;;
    reset)
        # Fully opaque so user can read output clearly
        set_opacity 1.0 1.0
        ;;
    address)
        get_address
        ;;
    *)
        echo "Usage: $0 {set [opacity]|reset|address}" >&2
        exit 1
        ;;
esac
