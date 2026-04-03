#!/usr/bin/env bash
# focus-start.sh - Hook: make terminal window transparent when agent starts
#
# Called by Claude Code on UserPromptSubmit via plugin hooks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Set window to transparent (timeout to prevent hook from hanging)
timeout 5 "$SCRIPT_DIR/scripts/hypr-window-opacity.sh" set 2>/dev/null || true

# Hook output: continue normally
cat <<'EOF'
{
  "continue": true
}
EOF
