#!/usr/bin/env bash
# patch-claude.sh - Live patch Claude Code to add auto mode
#
# This script patches the installed Claude Code cli.js to add "auto" mode
# as a cycling option (shift+tab). Auto mode = bypassPermissions + Stop hook.
#
# Patches are organized by version in scripts/patches/versions/*.conf
# When a version doesn't have a config, it falls back to the latest known version.

set -euo pipefail

# Get script directory for relative paths
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PATCHES_DIR="$SCRIPT_DIR/patches/versions"

VERSION_CACHE_DIR="$HOME/.cache/claude-unleashed"
VERSION_FILE="$VERSION_CACHE_DIR/patched-claude-version"

# Find Claude Code installation
# Allow override via environment variable for testing
CLAUDE_BIN="${CLAUDE_BIN:-$(which claude 2>/dev/null || echo "")}"
if [[ -z "$CLAUDE_BIN" ]]; then
    echo "Error: Claude Code not found in PATH"
    exit 1
fi

# Resolve symlinks fully (handles multiple levels)
# Includes protection against infinite loops from circular symlinks
resolve_symlink() {
    local path="$1"
    local dir
    local link
    local max_depth=20
    local depth=0
    while [[ -L "$path" ]]; do
        if (( depth++ >= max_depth )); then
            echo "Error: Too many symlink levels (possible circular symlink)" >&2
            return 1
        fi
        dir="$(dirname "$path")"
        link="$(readlink "$path")"
        if [[ "$link" == /* ]]; then
            path="$link"
        else
            path="$(cd "$dir" && cd "$(dirname "$link")" && pwd -P)/$(basename "$link")"
        fi
    done
    echo "$path"
}

CLAUDE_REAL="$(resolve_symlink "$CLAUDE_BIN")"
CLAUDE_DIR=$(dirname "$CLAUDE_REAL")
CLI_JS="$CLAUDE_REAL"

# If CLAUDE_REAL is not cli.js itself, look for cli.js in same directory
if [[ "$(basename "$CLAUDE_REAL")" != "cli.js" ]]; then
    CLI_JS="$CLAUDE_DIR/cli.js"
fi

if [[ ! -f "$CLI_JS" ]]; then
    echo "Error: cli.js not found at $CLI_JS"
    exit 1
fi

echo "Found Claude Code at: $CLAUDE_DIR"

# Get current Claude version
CLAUDE_VERSION=$("$CLAUDE_BIN" --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "unknown")
echo "Detected version: $CLAUDE_VERSION"

# Find the appropriate version config
# Try exact match first, then fall back to closest lower version
find_version_config() {
    local target_version="$1"

    # Try exact match
    if [[ -f "$PATCHES_DIR/${target_version}.conf" ]]; then
        echo "$PATCHES_DIR/${target_version}.conf"
        return 0
    fi

    # Find all available versions and sort them
    local available_versions=()
    local ver
    for conf in "$PATCHES_DIR"/*.conf; do
        [[ -f "$conf" ]] || continue
        ver=$(basename "$conf" .conf)
        available_versions+=("$ver")
    done

    if [[ ${#available_versions[@]} -eq 0 ]]; then
        echo ""
        return 1
    fi

    # Sort versions and find the latest one that's <= target
    # Using sort -V for version sorting
    local sorted_versions
    sorted_versions=$(printf '%s\n' "${available_versions[@]}" | sort -V)

    # Find the best match (latest version <= target)
    local best_match=""
    for ver in $sorted_versions; do
        # Compare versions: if ver <= target, it's a candidate
        if [[ "$(printf '%s\n%s' "$ver" "$target_version" | sort -V | head -1)" == "$ver" ]]; then
            best_match="$ver"
        fi
    done

    if [[ -n "$best_match" ]]; then
        echo "$PATCHES_DIR/${best_match}.conf"
        return 0
    fi

    # Fallback to the latest available version
    local latest
    latest=$(printf '%s\n' "${available_versions[@]}" | sort -V | tail -1)
    echo "$PATCHES_DIR/${latest}.conf"
    return 0
}

# Find config for this version
CONFIG_FILE=$(find_version_config "$CLAUDE_VERSION")

if [[ -z "$CONFIG_FILE" ]] || [[ ! -f "$CONFIG_FILE" ]]; then
    echo "Error: No patch configuration found for version $CLAUDE_VERSION"
    echo "Available configs in $PATCHES_DIR:"
    ls -1 "$PATCHES_DIR"/*.conf 2>/dev/null || echo "  (none)"
    exit 1
fi

echo "Using patch config: $(basename "$CONFIG_FILE")"
echo "Patching: $CLI_JS"

# Load version-specific configuration
# shellcheck source=/dev/null
source "$CONFIG_FILE"

# Check if already patched
if grep -qE "${MODES_ARRAY_VAR}=\[.*\"auto\"" "$CLI_JS" 2>/dev/null; then
    echo "Already patched (auto mode exists in modes array)"
    mkdir -p "$VERSION_CACHE_DIR"
    echo "$CLAUDE_VERSION" > "$VERSION_FILE"
    exit 0
fi

# Create backup
BACKUP="$CLI_JS.backup.$(date +%Y%m%d%H%M%S)"
cp "$CLI_JS" "$BACKUP"
echo "Backup created: $BACKUP"

# Create temp file for patching
TEMP_FILE=$(mktemp)
cp "$CLI_JS" "$TEMP_FILE"

# ============================================================================
# PATCH 1: Add "auto" to modes array
# ============================================================================
if grep -q "${MODES_ARRAY_VAR}=\[\"acceptEdits\",\"bypassPermissions\"" "$TEMP_FILE"; then
    sed -i "s/${MODES_ARRAY_VAR}=\[\"acceptEdits\",\"bypassPermissions\"/${MODES_ARRAY_VAR}=[\"acceptEdits\",\"auto\",\"bypassPermissions\"/g" "$TEMP_FILE"
    echo "Patch 1: Added 'auto' to modes array (${MODES_ARRAY_VAR})"
else
    echo "Warning: Patch 1 - modes array pattern not found"
fi

# ============================================================================
# PATCH 2: Add display name for auto mode
# ============================================================================
sed -i 's/case"bypassPermissions":return"Bypass Permissions"/case"auto":return"Auto Mode";case"bypassPermissions":return"Bypass Permissions"/g' "$TEMP_FILE"
echo "Patch 2: Added display name for auto mode"

# ============================================================================
# PATCH 3: Add icon for auto mode (»»)
# ============================================================================
sed -i 's/case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵"/case"acceptEdits":return"⏵⏵";case"auto":return"»»";case"bypassPermissions":return"⏵⏵"/g' "$TEMP_FILE"
echo "Patch 3: Added icon for auto mode (»»)"

# ============================================================================
# PATCH 4: Modify cycling logic
# bypassPermissions -> auto -> default
# ============================================================================
sed -i 's/case"bypassPermissions":return"default"/case"bypassPermissions":return"auto";case"auto":return"default"/g' "$TEMP_FILE"
echo "Patch 4: Modified cycling logic"

# ============================================================================
# PATCH 5: Make auto mode behave like bypassPermissions for permission checks
# ============================================================================

# 5a: Main permission allow check - Z.toolPermissionContext.mode
sed -i 's/Z\.toolPermissionContext\.mode==="bypassPermissions"||Z\.toolPermissionContext\.mode==="plan"/Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="auto"||Z.toolPermissionContext.mode==="plan"/g' "$TEMP_FILE"
echo "Patch 5a: Patched main permission allow check"

# 5b: Passthrough check - Q.mode
# NOTE: We prefix with "if(" to avoid matching "PQ.mode" (which appears in v2.1.12+).
# The Q.mode pattern appears as "if(Q.mode===..." in the minified code, so this is safe.
# If future versions change this structure, this pattern may need adjustment.
sed -i 's/if(Q\.mode==="bypassPermissions"/if(Q.mode==="bypassPermissions"||Q.mode==="auto"/g' "$TEMP_FILE"
echo "Patch 5b: Patched Q.mode passthrough check"

# 5c: Mode-specific permission checks with ||BOOL pattern
# PERMISSION_BOOL_VAR defaults to "V" for backward compatibility with older configs
PERMISSION_BOOL_VAR="${PERMISSION_BOOL_VAR:-V}"
sed -i "s/${PERMISSION_CTX_VAR}\.mode===\"bypassPermissions\"||${PERMISSION_BOOL_VAR})/${PERMISSION_CTX_VAR}.mode===\"bypassPermissions\"||${PERMISSION_CTX_VAR}.mode===\"auto\"||${PERMISSION_BOOL_VAR})/g" "$TEMP_FILE"
echo "Patch 5c: Patched ${PERMISSION_CTX_VAR}.mode||${PERMISSION_BOOL_VAR} permission checks"

# ============================================================================
# PATCH 6: Add color for auto mode (yellow/warning)
# ============================================================================
sed -i 's/case"acceptEdits":return"autoAccept";case"bypassPermissions":return"error"/case"acceptEdits":return"autoAccept";case"auto":return"warning";case"bypassPermissions":return"error"/g' "$TEMP_FILE"
echo "Patch 6: Added yellow/warning color for auto mode"

# ============================================================================
# PATCH 7: Flag file integration
# Creates/removes flag file when entering/leaving auto mode via shift+tab
# ============================================================================

# 7a: Create flag file when entering auto mode
PATTERN_7A="if(${MODE_VAR}===\"acceptEdits\")${TELEMETRY_FN}(\"auto-accept-mode\")"
REPLACE_7A="if(${MODE_VAR}===\"acceptEdits\")${TELEMETRY_FN}(\"auto-accept-mode\");if(${MODE_VAR}===\"auto\")import(\"fs\").then(_fs=>{let _d=process.env.HOME+\"/.cache/claude-unleashed/auto-mode\";_fs.mkdirSync(_d,{recursive:!0});_fs.writeFileSync(_d+\"/active-\"+process.ppid,\"\")})"

if grep -qF "$PATTERN_7A" "$TEMP_FILE"; then
    sed -i "s|${PATTERN_7A}|${REPLACE_7A}|g" "$TEMP_FILE"
    echo "Patch 7a: Inject flag creation (${MODE_VAR}/${TELEMETRY_FN})"
else
    echo "Warning: Patch 7a - pattern not found"
fi

# 7b: Remove flag file when leaving auto mode
# Need to handle the $ in function names carefully
DELEGATE_FN1_ESCAPED="${DELEGATE_FN1//\$/\\\$}"
DELEGATE_FN1_GREP="${DELEGATE_FN1//\$/[\$]}"

PATTERN_7B_GREP="B\\.mode===\"delegate\"&&${MODE_VAR}!==\"delegate\")${DELEGATE_FN1_GREP}"
PATTERN_7B="B\\.mode===\"delegate\"\\&\\&${MODE_VAR}!==\"delegate\")${DELEGATE_FN1_ESCAPED}(\\!0),${DELEGATE_FN2}(\\!0)"
REPLACE_7B="B.mode===\"delegate\"\\&\\&${MODE_VAR}!==\"delegate\")${DELEGATE_FN1}(!0),${DELEGATE_FN2}(!0);if(B.mode===\"auto\"\\&\\&${MODE_VAR}!==\"auto\")import(\"fs\").then(_fs=>{try{_fs.unlinkSync(process.env.HOME+\"/.cache/claude-unleashed/auto-mode/active-\"+process.ppid)}catch(_e){}})"

if grep -qE "$PATTERN_7B_GREP" "$TEMP_FILE"; then
    sed -i "s|${PATTERN_7B}|${REPLACE_7B}|g" "$TEMP_FILE"
    echo "Patch 7b: Inject flag removal (${MODE_VAR}/${DELEGATE_FN1}/${DELEGATE_FN2})"
else
    echo "Warning: Patch 7b - pattern not found"
fi

# ============================================================================
# VERIFY AND APPLY
# ============================================================================

if ! grep -qE "${MODES_ARRAY_VAR}=\[.*\"auto\"" "$TEMP_FILE"; then
    echo "Error: Patch verification failed - auto mode not found in modes array"
    rm "$TEMP_FILE"
    exit 1
fi

# Apply patched file
mv "$TEMP_FILE" "$CLI_JS"
chmod +x "$CLI_JS"

# Store patched version
mkdir -p "$VERSION_CACHE_DIR"
echo "$CLAUDE_VERSION" > "$VERSION_FILE"
echo "Stored patched version: $CLAUDE_VERSION"

echo ""
echo "Patching complete!"
echo "Auto mode is now available via shift+tab cycling:"
echo "  default -> plan -> bypassPermissions -> auto -> default"
echo ""
echo "Note: The Stop hook at ~/.claude/settings.json enforces auto mode behavior."
echo "Restart Claude Code to apply changes."
