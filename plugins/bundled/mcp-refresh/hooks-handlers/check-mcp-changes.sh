#!/usr/bin/env bash
# PreToolUse hook: Check for MCP configuration changes
#
# This hook runs before each tool use to detect changes in MCP configuration files.
# If changes are detected, it notifies the user without interrupting the workflow.

set -euo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/unleash/mcp-refresh"
HASH_FILE="${CACHE_DIR}/config-hashes.txt"
PLUGIN_SETTING_AUTO_DETECT="${PLUGIN_SETTING_AUTO_DETECT:-true}"

# Only run if auto-detect is enabled
if [[ "${PLUGIN_SETTING_AUTO_DETECT}" != "true" ]]; then
  exit 0
fi

# Create cache directory if it doesn't exist
mkdir -p "${CACHE_DIR}"

# Function to compute hash of MCP config files
compute_config_hash() {
  local hash=""

  # Project-level .mcp.json
  if [[ -f ".mcp.json" ]]; then
    hash+=$(cat ".mcp.json" | sha256sum)
  fi

  # User-level .claude.json
  if [[ -f "${HOME}/.claude.json" ]]; then
    hash+=$(cat "${HOME}/.claude.json" | sha256sum)
  fi

  # Plugin-level .mcp.json files
  if [[ -d "plugins" ]]; then
    while IFS= read -r -d '' file; do
      hash+=$(cat "$file" | sha256sum)
    done < <(find plugins -name ".mcp.json" -print0 2>/dev/null || true)
  fi

  # Compute final hash
  echo -n "$hash" | sha256sum | awk '{print $1}'
}

# Get current hash
current_hash=$(compute_config_hash)

# Check if this is first run
if [[ ! -f "${HASH_FILE}" ]]; then
  # First run - save hash and exit silently
  echo "${current_hash}" > "${HASH_FILE}"
  exit 0
fi

# Load previous hash
previous_hash=$(cat "${HASH_FILE}")

# Compare hashes
if [[ "${current_hash}" != "${previous_hash}" ]]; then
  # Configuration changed - notify user
  # Using JSON output format as per Claude Code hook specification
  cat <<EOF
{
  "type": "prompt",
  "content": "MCP configuration files have changed since session start. New servers or configuration updates detected.\\n\\nUse \`/reload-mcps\` to see what changed, or \`/restart\` to apply changes while preserving your session.\\n\\nThis is an automatic notification from the mcp-refresh plugin. You can disable it by setting \`autoDetect: false\` in plugin settings."
}
EOF

  # Update hash for next check
  echo "${current_hash}" > "${HASH_FILE}"
else
  # No changes - exit silently
  exit 0
fi
