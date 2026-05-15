#!/usr/bin/env bash
# focus-stop.sh - Hook: restore window opacity and play sound when agent stops
#
# Called by Claude Code on Stop via plugin hooks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Self-disable guard: claude --plugin-dir keeps stale plugins loaded across
# unleash-refresh restarts even after the user disables them in the TUI.
# Exit 0 (continue) without firing the rest of the hook if disabled.
if ! "$SCRIPT_DIR/scripts/check-enabled.sh" hyprland-focus 2>/dev/null; then
    cat <<'EOF'
{
  "continue": true
}
EOF
    exit 0
fi

# Restore window to fully opaque (timeout to prevent hook from hanging forever)
timeout 3 "$SCRIPT_DIR/scripts/hypr-window-opacity.sh" reset 2>/dev/null || true

# Play notification sound (async, don't block the hook)
# Skip sound when called from SessionStart (HOOK_NO_SOUND=1)
if [[ "${HOOK_NO_SOUND:-}" != "1" && "${AU_HYPRLAND_FOCUS:-}" != "0" && -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    (
        SOUND_FILE="$SCRIPT_DIR/sounds/idle.wav"
        if [[ -f "$SOUND_FILE" ]]; then
            if command -v pw-play &>/dev/null; then
                timeout 5 pw-play "$SOUND_FILE" 2>/dev/null || true
            elif command -v paplay &>/dev/null; then
                timeout 5 paplay "$SOUND_FILE" 2>/dev/null || true
            elif command -v play &>/dev/null; then
                timeout 5 play -q "$SOUND_FILE" 2>/dev/null || true
            fi
        fi
    ) &
    disown
fi

# Surface the agent's terminal window: move it to the user's current
# workspace (only if elsewhere) and focus it. Async + best-effort.
if [[ -x /home/me/ht/agent-tools/bin/focus-self && -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
    /home/me/ht/agent-tools/bin/focus-self >/dev/null 2>&1 &
    disown
fi

# Hook output: allow stop to proceed
cat <<'EOF'
{
  "continue": true
}
EOF
