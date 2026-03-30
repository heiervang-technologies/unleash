#!/usr/bin/env bash
# lifecycle-exit.sh - Called by launcher on agent exit
#
# Resets window opacity, plays idle sound, cleans up state, shows exit notification.
# Receives exit code as $1 and wrapper PID as $2.

set -euo pipefail

EXIT_CODE="${1:-0}"
WRAPPER_PID="${2:-$$}"

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Reset window to opaque
if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]] && [[ "${AU_HYPRLAND_FOCUS:-}" != "0" ]]; then
    timeout 3 "$SCRIPT_DIR/scripts/hypr-window-opacity.sh" reset 2>/dev/null || true
fi

# Play idle sound (async)
if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]] && [[ "${AU_HYPRLAND_FOCUS:-}" != "0" ]]; then
    SOUND_FILE="$SCRIPT_DIR/sounds/idle.wav"
    if [[ -f "$SOUND_FILE" ]]; then
        for player in pw-play paplay play; do
            if command -v "$player" &>/dev/null; then
                (timeout 5 "$player" "$SOUND_FILE" 2>/dev/null || true) &
                disown
                break
            fi
        done
    fi
fi

# Clean up state file
rm -f "/tmp/unleash-hyprfocus/${WRAPPER_PID}" 2>/dev/null || true

# Exit notification
if [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    if [[ "$EXIT_CODE" == "0" || "$EXIT_CODE" == "143" ]]; then
        hyprctl notify 1 5000 0 "unleash stopped" 2>/dev/null || true
    else
        hyprctl notify 0 8000 0 "unleash exited with code ${EXIT_CODE}" 2>/dev/null || true
    fi
fi
