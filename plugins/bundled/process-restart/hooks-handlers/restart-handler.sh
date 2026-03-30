#!/usr/bin/env bash
# Stop hook: Handle Claude Code restart with session preservation
#
# This hook intercepts the exit process when /restart command has been used.
# It saves session state and lets the wrapper handle the actual restart.

set -euo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/unleash/process-restart"
STATE_FILE="${CACHE_DIR}/restart-state.json"
WRAPPER_PID="${AGENT_WRAPPER_PID:-}"
TRIGGER_FILE="${CACHE_DIR}/restart-trigger${WRAPPER_PID:+-${WRAPPER_PID}}"
PLUGIN_SETTING_STATE_EXPIRY="${PLUGIN_SETTING_STATE_EXPIRY:-300}"  # 5 minutes default

# Read hook input from stdin
HOOK_INPUT=$(cat)

# Check if restart was requested
if [[ ! -f "${TRIGGER_FILE}" ]]; then
  # No restart requested - allow normal exit
  exit 0
fi

# Restart was requested - save state for the wrapper

# Create cache directory if it doesn't exist
mkdir -p "${CACHE_DIR}"

# Extract session information from hook input
# Hook provides: transcript_path, session_id, working_dir, etc.
SESSION_ID=$(echo "${HOOK_INPUT}" | jq -r '.session_id // empty' 2>/dev/null || echo "")
WORKING_DIR=$(pwd)

# Fallback: If session ID is empty (e.g., killed by signal), find it from project files
if [[ -z "${SESSION_ID}" ]]; then
  # Convert working dir to Claude's project path format (slashes become dashes)
  PROJECT_PATH=$(echo "${WORKING_DIR}" | sed 's|^/||; s|/|-|g')
  PROJECT_DIR="${HOME}/.claude/projects/-${PROJECT_PATH}"

  if [[ -d "${PROJECT_DIR}" ]]; then
    # Find the most recently modified session file (excluding agent files)
    SESSION_FILE=$(find "${PROJECT_DIR}" -maxdepth 1 -name "*.jsonl" ! -name "agent-*.jsonl" -type f -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)
    if [[ -n "${SESSION_FILE}" ]]; then
      # Extract session ID from filename (remove path and .jsonl extension)
      SESSION_ID=$(basename "${SESSION_FILE}" .jsonl)
    fi
  fi
fi

# Get current model from environment or use default
MODEL="${CLAUDE_MODEL:-claude-sonnet-4-5}"

# Get git branch if in a git repository
GIT_BRANCH=""
if git rev-parse --git-dir > /dev/null 2>&1; then
  GIT_BRANCH=$(git branch --show-current 2>/dev/null || echo "")
fi

# Plugins are loaded via --plugin-dir, no settings.json needed
ENABLED_PLUGINS="[]"

# Create state file with current session information
jq -n \
  --arg version "1.0.0" \
  --arg timestamp "$(date +%s)" \
  --arg session_id "${SESSION_ID}" \
  --arg working_dir "${WORKING_DIR}" \
  --arg model "${MODEL}" \
  --arg git_branch "${GIT_BRANCH}" \
  --argjson enabled_plugins "${ENABLED_PLUGINS}" \
  '{
    version: $version,
    timestamp: ($timestamp | tonumber),
    sessionId: $session_id,
    workingDir: $working_dir,
    model: $model,
    gitBranch: $git_branch,
    enabledPlugins: $enabled_plugins
  }' > "${STATE_FILE}"

# Make state file readable only by owner
chmod 600 "${STATE_FILE}"

# Remove trigger file
rm -f "${TRIGGER_FILE}"

# State saved. The wrapper (unleash) will detect the state file on next
# startup and restore the session via --continue. The nohup spawn approach
# was removed because Claude cannot spawn its own replacement process —
# see HANDOFF.md for details.

# Allow current process to exit gracefully. The wrapper loop will handle restart.
exit 0
