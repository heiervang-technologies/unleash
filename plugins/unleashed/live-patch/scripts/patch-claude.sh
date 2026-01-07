#!/usr/bin/env bash
# patch-claude.sh - Live patch Claude Code to add auto mode
#
# This script patches the installed Claude Code cli.js to add "auto" mode
# as a cycling option (shift+tab). Auto mode = bypassPermissions + Stop hook.

set -euo pipefail

# Find Claude Code installation
CLAUDE_BIN=$(which claude 2>/dev/null || echo "")
if [[ -z "$CLAUDE_BIN" ]]; then
    echo "Error: Claude Code not found in PATH"
    exit 1
fi

CLAUDE_REAL=$(readlink -f "$CLAUDE_BIN")
CLAUDE_DIR=$(dirname "$CLAUDE_REAL")
CLI_JS="$CLAUDE_DIR/cli.js"

if [[ ! -f "$CLI_JS" ]]; then
    echo "Error: cli.js not found at $CLI_JS"
    exit 1
fi

echo "Found Claude Code at: $CLAUDE_DIR"
echo "Patching: $CLI_JS"

# Check if already patched (check modes array specifically)
if grep -q 'CT=\[.*"auto"' "$CLI_JS" 2>/dev/null; then
    echo "Already patched (auto mode exists in modes array)"
    exit 0
fi

# Create backup
BACKUP="$CLI_JS.backup.$(date +%Y%m%d%H%M%S)"
cp "$CLI_JS" "$BACKUP"
echo "Backup created: $BACKUP"

# Create temp file for patching
TEMP_FILE=$(mktemp)
cp "$CLI_JS" "$TEMP_FILE"

# Patch 1: Add "auto" to modes array
# CT=["acceptEdits","bypassPermissions","default",...] -> CT=["acceptEdits","auto","bypassPermissions","default",...]
sed -i 's/CT=\["acceptEdits","bypassPermissions"/CT=["acceptEdits","auto","bypassPermissions"/g' "$TEMP_FILE"
echo "Patch 1: Added 'auto' to modes array"

# Patch 2: Add display name for auto mode
# case"bypassPermissions":return"Bypass Permissions" -> add case"auto":return"Auto Mode";case"bypassPermissions"...
sed -i 's/case"bypassPermissions":return"Bypass Permissions"/case"auto":return"Auto Mode";case"bypassPermissions":return"Bypass Permissions"/g' "$TEMP_FILE"
echo "Patch 2: Added display name for auto mode"

# Patch 3: Add icon for auto mode (use rocket »)
# case"bypassPermissions":return"⏵⏵" -> add case"auto":return"»";case"bypassPermissions"...
sed -i 's/case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵"/case"acceptEdits":return"⏵⏵";case"auto":return"»";case"bypassPermissions":return"⏵⏵"/g' "$TEMP_FILE"
echo "Patch 3: Added icon for auto mode"

# Patch 4: Modify cycling logic - bypassPermissions now goes to auto, auto goes to default
# case"bypassPermissions":return"default" -> case"bypassPermissions":return"auto";case"auto":return"default"
sed -i 's/case"bypassPermissions":return"default"/case"bypassPermissions":return"auto";case"auto":return"default"/g' "$TEMP_FILE"
echo "Patch 4: Modified cycling logic"

# Patch 5: Make auto mode behave like bypassPermissions for permission checks
# Add auto mode to permission passthrough checks
sed -i 's/if(Q\.mode==="bypassPermissions")/if(Q.mode==="bypassPermissions"||Q.mode==="auto")/g' "$TEMP_FILE"
echo "Patch 5: Auto mode uses bypassPermissions behavior"

# Patch 6: Add auto mode flag file creation when mode is set to auto
# This is tricky - we need to inject code that creates the flag file
# For now, we'll rely on the external toggle-auto-mode.sh to manage the flag

# Verify patches applied
if ! grep -q 'CT=\[.*"auto"' "$TEMP_FILE"; then
    echo "Error: Patch verification failed - auto mode not found in modes array"
    rm "$TEMP_FILE"
    exit 1
fi

# Apply patched file
mv "$TEMP_FILE" "$CLI_JS"
chmod +x "$CLI_JS"

echo ""
echo "Patching complete!"
echo "Auto mode is now available via shift+tab cycling:"
echo "  default -> plan -> bypassPermissions -> auto -> default"
echo ""
echo "Note: The Stop hook at ~/.claude/settings.json enforces auto mode behavior."
echo "Restart Claude Code to apply changes."
