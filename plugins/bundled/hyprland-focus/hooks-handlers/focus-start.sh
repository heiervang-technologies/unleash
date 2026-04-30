#!/usr/bin/env bash
# focus-start.sh - Hook: make terminal window transparent when agent starts
#
# Called by Claude Code on UserPromptSubmit via plugin hooks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Self-disable guard: skip silently when plugin is disabled in unleash config
# (stale claude --plugin-dir registrations survive TUI toggles in old sessions).
if ! "$SCRIPT_DIR/scripts/check-enabled.sh" hyprland-focus 2>/dev/null; then
    cat <<'EOF'
{
  "continue": true
}
EOF
    exit 0
fi

# Set window to transparent (timeout to prevent hook from hanging)
timeout 5 "$SCRIPT_DIR/scripts/hypr-window-opacity.sh" set 2>/dev/null || true

# Hook output: continue normally
cat <<'EOF'
{
  "continue": true
}
EOF
