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

# Check if already patched (check modes array specifically - variable name varies by version)
if grep -qE '(CT|kT)=\[.*"auto"' "$CLI_JS" 2>/dev/null; then
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
# Variable name varies by version: CT= (older) or kT= (2.1.0+)
# Find and patch whichever pattern exists
if grep -q 'CT=\["acceptEdits","bypassPermissions"' "$TEMP_FILE"; then
    sed -i 's/CT=\["acceptEdits","bypassPermissions"/CT=["acceptEdits","auto","bypassPermissions"/g' "$TEMP_FILE"
    echo "Patch 1: Added 'auto' to modes array (CT variant)"
elif grep -q 'kT=\["acceptEdits","bypassPermissions"' "$TEMP_FILE"; then
    sed -i 's/kT=\["acceptEdits","bypassPermissions"/kT=["acceptEdits","auto","bypassPermissions"/g' "$TEMP_FILE"
    echo "Patch 1: Added 'auto' to modes array (kT variant)"
else
    echo "Warning: Patch 1 - modes array pattern not found"
fi

# Patch 2: Add display name for auto mode
# case"bypassPermissions":return"Bypass Permissions" -> add case"auto":return"Auto Mode";case"bypassPermissions"...
sed -i 's/case"bypassPermissions":return"Bypass Permissions"/case"auto":return"Auto Mode";case"bypassPermissions":return"Bypass Permissions"/g' "$TEMP_FILE"
echo "Patch 2: Added display name for auto mode"

# Patch 3: Add icon for auto mode (use double guillemet »»)
# case"bypassPermissions":return"⏵⏵" -> add case"auto":return"»»";case"bypassPermissions"...
sed -i 's/case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵"/case"acceptEdits":return"⏵⏵";case"auto":return"»»";case"bypassPermissions":return"⏵⏵"/g' "$TEMP_FILE"
echo "Patch 3: Added icon for auto mode (»»)"

# Patch 4: Modify cycling logic - bypassPermissions now goes to auto, auto goes to default
# case"bypassPermissions":return"default" -> case"bypassPermissions":return"auto";case"auto":return"default"
sed -i 's/case"bypassPermissions":return"default"/case"bypassPermissions":return"auto";case"auto":return"default"/g' "$TEMP_FILE"
echo "Patch 4: Modified cycling logic"

# Patch 5: Make auto mode behave like bypassPermissions for permission checks
# This needs to patch ALL places where bypassPermissions is checked for allowing tools

# Pattern 5a: Main permission allow check - Z.toolPermissionContext.mode
# Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="plan"
# -> Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="auto"||Z.toolPermissionContext.mode==="plan"
sed -i 's/Z\.toolPermissionContext\.mode==="bypassPermissions"||Z\.toolPermissionContext\.mode==="plan"/Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="auto"||Z.toolPermissionContext.mode==="plan"/g' "$TEMP_FILE"
echo "Patch 5a: Patched main permission allow check"

# Pattern 5b: Passthrough check - Q.mode
sed -i 's/Q\.mode==="bypassPermissions"/Q.mode==="bypassPermissions"||Q.mode==="auto"/g' "$TEMP_FILE"
echo "Patch 5b: Patched Q.mode passthrough check"

# Pattern 5c: Mode-specific permission checks with ||V pattern
sed -i 's/mode==="bypassPermissions"||V)/mode==="bypassPermissions"||mode==="auto"||V)/g' "$TEMP_FILE"
echo "Patch 5c: Patched mode||V permission checks"

# Patch 6: Add color for auto mode (yellow/warning)
# case"acceptEdits":return"autoAccept";case"bypassPermissions":return"error"
# -> case"acceptEdits":return"autoAccept";case"auto":return"warning";case"bypassPermissions":return"error"
sed -i 's/case"acceptEdits":return"autoAccept";case"bypassPermissions":return"error"/case"acceptEdits":return"autoAccept";case"auto":return"warning";case"bypassPermissions":return"error"/g' "$TEMP_FILE"
echo "Patch 6: Added yellow/warning color for auto mode"

# Patch 7: Flag file integration
# Creates/removes flag file when entering/leaving auto mode via shift+tab
# Uses dynamic import("fs") for ESM compatibility

# Patch 7a: Create flag file when entering auto mode
if grep -q 'if(j1==="acceptEdits")v9("auto-accept-mode")' "$TEMP_FILE"; then
    # Legacy variant (< 2.1.0)
    sed -i 's|if(j1==="acceptEdits")v9("auto-accept-mode")|if(j1==="acceptEdits")v9("auto-accept-mode");if(j1==="auto"){let _d=process.env.HOME+"/\.cache/claude-unleashed/auto-mode";l9.mkdirSync(_d,{recursive:\!0});l9.writeFileSync(_d+"/active-"+process.ppid,"")}|g' "$TEMP_FILE"
    echo "Patch 7a: Inject flag creation (legacy)"
elif grep -q 'if(JQ==="acceptEdits")O9("auto-accept-mode")' "$TEMP_FILE"; then
    # ESM variant (>= 2.1.0) - use dynamic import
    sed -i 's|if(JQ==="acceptEdits")O9("auto-accept-mode")|if(JQ==="acceptEdits")O9("auto-accept-mode");if(JQ==="auto")import("fs").then(_fs=>{let _d=process.env.HOME+"/.cache/claude-unleashed/auto-mode";_fs.mkdirSync(_d,{recursive:!0});_fs.writeFileSync(_d+"/active-"+process.ppid,"")})|g' "$TEMP_FILE"
    echo "Patch 7a: Inject flag creation (ESM dynamic import)"
else
    echo "Warning: Patch 7a - pattern not found"
fi

# Patch 7b: Remove flag file when leaving auto mode
if grep -q 'if(B\.mode==="delegate"&&j1!=="delegate")' "$TEMP_FILE"; then
    # Legacy variant
    sed -i 's|if(B\.mode==="delegate"\&\&j1!=="delegate")YP0(\!0),chA(\!0)|if(B.mode==="delegate"\&\&j1!=="delegate")YP0(\!0),chA(\!0);if(B.mode==="auto"\&\&j1!=="auto"){try{l9.unlinkSync(process.env.HOME+"/\.cache/claude-unleashed/auto-mode/active-"+process.ppid)}catch(_e){}}|g' "$TEMP_FILE"
    echo "Patch 7b: Inject flag removal (legacy)"
elif grep -q 'B\.mode==="delegate"&&JQ!=="delegate")ty0' "$TEMP_FILE"; then
    # ESM variant - use dynamic import
    sed -i 's|B\.mode==="delegate"\&\&JQ!=="delegate")ty0(\!0),_uA(\!0)|B.mode==="delegate"\&\&JQ!=="delegate")ty0(\!0),_uA(\!0);if(B.mode==="auto"\&\&JQ!=="auto")import("fs").then(_fs=>{try{_fs.unlinkSync(process.env.HOME+"/.cache/claude-unleashed/auto-mode/active-"+process.ppid)}catch(_e){}})|g' "$TEMP_FILE"
    echo "Patch 7b: Inject flag removal (ESM dynamic import)"
else
    echo "Warning: Patch 7b - pattern not found"
fi

# Verify patches applied (check both CT and kT variants)
if ! grep -qE '(CT|kT)=\[.*"auto"' "$TEMP_FILE"; then
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
