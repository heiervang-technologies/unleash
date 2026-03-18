#!/usr/bin/env bash
# focus-start.sh - Hook: make terminal window transparent when agent starts
#
# Called by Claude Code on SessionStart via plugin hooks.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"

# Set window to transparent
"$SCRIPT_DIR/scripts/hypr-window-opacity.sh" set

# Hook output: continue normally
cat <<'EOF'
{
  "continue": true
}
EOF
