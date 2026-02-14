#!/usr/bin/env bash
# focus-stop.sh - Hook: restore window opacity and play sound when agent stops
#
# Called by Claude Code on Stop via plugin hooks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Restore window to fully opaque
"$SCRIPT_DIR/scripts/hypr-window-opacity.sh" reset

# Play notification sound (async, don't block the hook)
if [[ "${AU_HYPRLAND_FOCUS:-}" != "0" && -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    (
        SOUND_FILE="$SCRIPT_DIR/sounds/idle.wav"
        if [[ -f "$SOUND_FILE" ]]; then
            if command -v pw-play &>/dev/null; then
                pw-play "$SOUND_FILE" 2>/dev/null
            elif command -v paplay &>/dev/null; then
                paplay "$SOUND_FILE" 2>/dev/null
            elif command -v play &>/dev/null; then
                play -q "$SOUND_FILE" 2>/dev/null
            fi
        fi
    ) &
fi

# Hook output: allow stop to proceed
cat <<'EOF'
{
  "continue": true
}
EOF
