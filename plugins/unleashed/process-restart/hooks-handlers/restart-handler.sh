#!/usr/bin/env bash
# Stop hook: Handle Claude Code restart with session preservation
#
# This hook intercepts the exit process when /restart command has been used.
# It saves session state and spawns a new Claude Code process before allowing exit.

set -euo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/agent-unleashed/process-restart"
STATE_FILE="${CACHE_DIR}/restart-state.json"
TRIGGER_FILE="${CACHE_DIR}/restart-trigger"
PLUGIN_SETTING_STATE_EXPIRY="${PLUGIN_SETTING_STATE_EXPIRY:-300}"  # 5 minutes default

# Read hook input from stdin
HOOK_INPUT=$(cat)

# Check if restart was requested
if [[ ! -f "${TRIGGER_FILE}" ]]; then
  # No restart requested - allow normal exit
  exit 0
fi

# Restart was requested - save state and spawn new process

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

# Get enabled plugins from settings
ENABLED_PLUGINS="[]"
if [[ -f ".claude/settings.json" ]]; then
  ENABLED_PLUGINS=$(jq -r '.plugins.enabled // []' .claude/settings.json 2>/dev/null || echo "[]")
elif [[ -f "${HOME}/.claude/settings.json" ]]; then
  ENABLED_PLUGINS=$(jq -r '.plugins.enabled // []' "${HOME}/.claude/settings.json" 2>/dev/null || echo "[]")
fi

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

# Determine Claude Code executable path
CLAUDE_CMD="claude"
if command -v claude-code &> /dev/null; then
  CLAUDE_CMD="claude-code"
elif command -v claude &> /dev/null; then
  CLAUDE_CMD="claude"
else
  # Fallback: Try common installation paths
  if [[ -x "${HOME}/.local/bin/claude" ]]; then
    CLAUDE_CMD="${HOME}/.local/bin/claude"
  elif [[ -x "/usr/local/bin/claude" ]]; then
    CLAUDE_CMD="/usr/local/bin/claude"
  else
    # Error: Can't find Claude Code executable
    echo "{\"decision\": \"allow\", \"error\": \"Claude Code executable not found. Restart aborted.\"}" >&2
    rm -f "${STATE_FILE}"
    exit 0
  fi
fi

# Spawn new Claude Code process in background
# The new process will:
# 1. Start normally
# 2. SessionStart hook will detect the state file
# 3. Restore session state
# 4. Resume the session

# Build command args
CMD_ARGS=()

# Add working directory
if [[ -n "${WORKING_DIR}" ]]; then
  CMD_ARGS+=("--cwd" "${WORKING_DIR}")
fi

# Add model if specified
if [[ -n "${MODEL}" ]] && [[ "${MODEL}" != "claude-sonnet-4-5" ]]; then
  CMD_ARGS+=("--model" "${MODEL}")
fi

# Add session resume if we have a session ID
if [[ -n "${SESSION_ID}" ]]; then
  CMD_ARGS+=("--resume" "${SESSION_ID}")
fi

# Spawn new process in background using nohup
# Redirect output to avoid hanging
nohup "${CLAUDE_CMD}" "${CMD_ARGS[@]}" > /dev/null 2>&1 &

# Give the new process a moment to start
sleep 0.5

# Notify about successful restart initiation
echo "✅ Restart initiated. New Claude Code process started."
echo "   Session will be restored automatically."

# Allow current process to exit gracefully
exit 0
