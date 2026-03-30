#!/usr/bin/env bash
# onboarding.sh - Shared onboarding bypass functions for unleash
#
# This library ensures Claude Code's .claude.json is configured to skip
# interactive onboarding prompts and acknowledge bypass permissions mode.
#
# Usage:
#   source "$(dirname "${BASH_SOURCE[0]}")/lib/onboarding.sh"
#   ensure_onboarding_complete [claude_cmd]
#
# Required fields in ~/.claude.json:
#   - hasCompletedOnboarding: true - skips initial onboarding wizard
#   - bypassPermissionsModeAccepted: true - acknowledges --dangerously-skip-permissions
#   - lastOnboardingVersion: <version> - prevents version-based re-onboarding

# Ensure onboarding is completed and bypass mode is acknowledged
# This prevents interactive prompts during headless/automated runs
#
# Args:
#   $1 - Claude command to use for version detection (default: claude)
ensure_onboarding_complete() {
    local claude_json="${HOME}/.claude.json"
    local claude_dir="${HOME}/.claude"
    local claude_cmd="${1:-claude}"

    # Ensure .claude directory exists
    mkdir -p "$claude_dir"

    # Get current Claude version for lastOnboardingVersion
    local claude_version
    claude_version=$("$claude_cmd" --version 2>/dev/null | head -1 | sed 's/ (Claude Code)//' || echo "2.1.0")

    if [[ -f "$claude_json" ]]; then
        # File exists - update required fields using jq or sed
        if command -v jq &>/dev/null; then
            local tmp_file
            tmp_file=$(mktemp)
            if jq --arg ver "$claude_version" '
                .hasCompletedOnboarding = true |
                .bypassPermissionsModeAccepted = true |
                .lastOnboardingVersion = $ver
            ' "$claude_json" > "$tmp_file" 2>/dev/null; then
                mv "$tmp_file" "$claude_json"
            else
                rm -f "$tmp_file"
            fi
        else
            # Fallback: use sed for simple updates
            if grep -q '"hasCompletedOnboarding"' "$claude_json"; then
                sed -i 's/"hasCompletedOnboarding":\s*false/"hasCompletedOnboarding": true/g' "$claude_json"
            fi
            if grep -q '"bypassPermissionsModeAccepted"' "$claude_json"; then
                sed -i 's/"bypassPermissionsModeAccepted":\s*false/"bypassPermissionsModeAccepted": true/g' "$claude_json"
            fi
        fi
    else
        # Create new file with required fields
        cat > "$claude_json" << EOF
{
  "hasCompletedOnboarding": true,
  "lastOnboardingVersion": "${claude_version}",
  "bypassPermissionsModeAccepted": true,
  "numStartups": 1,
  "installMethod": "unleash"
}
EOF
    fi
}
