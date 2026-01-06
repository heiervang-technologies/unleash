#!/usr/bin/env bash
# Trigger restart script
#
# This script handles the complete restart process:
# 1. Finds current session ID from project files
# 2. Saves state
# 3. Spawns new Claude process
# 4. Terminates current process
#
# This approach doesn't rely on Stop hooks (which don't fire on SIGTERM).

set -euo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/claude-unleashed/process-restart"
STATE_FILE="${CACHE_DIR}/restart-state.json"
TRIGGER_FILE="${CACHE_DIR}/restart-trigger"

# Parse command line arguments
FORCE=false
CLEAN=false

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      FORCE=true
      shift
      ;;
    --clean)
      CLEAN=true
      shift
      ;;
    *)
      echo "Unknown option: $1"
      exit 1
      ;;
  esac
done

# Create cache directory
mkdir -p "${CACHE_DIR}"

# Get working directory
WORKING_DIR=$(pwd)

# Find current Claude PID
CLAUDE_PID=$(ps aux | grep '[c]laude' | grep -v defunct | head -1 | awk '{print $2}')
if [[ -z "${CLAUDE_PID}" ]]; then
  echo "Error: Could not find running Claude process"
  exit 1
fi

# Find session ID from project files
SESSION_ID=""
if [[ "${CLEAN}" != "true" ]]; then
  # Convert working dir to Claude's project path format (slashes become dashes)
  PROJECT_PATH=$(echo "${WORKING_DIR}" | sed 's|^/||; s|/|-|g')
  PROJECT_DIR="${HOME}/.claude/projects/-${PROJECT_PATH}"

  if [[ -d "${PROJECT_DIR}" ]]; then
    # Find the most recently modified session file (excluding agent files)
    SESSION_FILE=$(find "${PROJECT_DIR}" -maxdepth 1 -name "*.jsonl" ! -name "agent-*.jsonl" -type f -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)
    if [[ -n "${SESSION_FILE}" ]]; then
      SESSION_ID=$(basename "${SESSION_FILE}" .jsonl)
    fi
  fi
fi

# Get model from environment or use default
MODEL="${CLAUDE_MODEL:-claude-sonnet-4-5}"

# Get git branch if in a git repository
GIT_BRANCH=""
if git rev-parse --git-dir > /dev/null 2>&1; then
  GIT_BRANCH=$(git branch --show-current 2>/dev/null || echo "")
fi

# Output confirmation message
if [[ "${FORCE}" == "true" ]]; then
  echo "🔄 Restart triggered (forced)"
else
  echo "🔄 Restart triggered"
fi

if [[ "${CLEAN}" == "true" ]]; then
  echo "   Clean restart: Session state will NOT be preserved"
  rm -f "${STATE_FILE}"
else
  echo "   Session ID: ${SESSION_ID:-<not found>}"
  echo "   Working directory: ${WORKING_DIR}"
  echo "   Claude PID: ${CLAUDE_PID}"
fi

# If not clean restart, save state file
if [[ "${CLEAN}" != "true" ]] && [[ -n "${SESSION_ID}" ]]; then
  # Get enabled plugins from settings
  ENABLED_PLUGINS="[]"
  if [[ -f ".claude/settings.json" ]]; then
    ENABLED_PLUGINS=$(jq -r '.plugins.enabled // []' .claude/settings.json 2>/dev/null || echo "[]")
  elif [[ -f "${HOME}/.claude/settings.json" ]]; then
    ENABLED_PLUGINS=$(jq -r '.plugins.enabled // []' "${HOME}/.claude/settings.json" 2>/dev/null || echo "[]")
  fi

  # Create state file
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

  chmod 600 "${STATE_FILE}"
  echo "   State saved to: ${STATE_FILE}"
fi

# Determine Claude Code executable path
CLAUDE_CMD="claude"
if command -v claude-code &> /dev/null; then
  CLAUDE_CMD="claude-code"
elif command -v claude &> /dev/null; then
  CLAUDE_CMD="claude"
elif [[ -x "${HOME}/.local/bin/claude" ]]; then
  CLAUDE_CMD="${HOME}/.local/bin/claude"
elif [[ -x "/usr/local/bin/claude" ]]; then
  CLAUDE_CMD="/usr/local/bin/claude"
else
  echo "Error: Claude Code executable not found"
  exit 1
fi

# Build command args for new process
CMD_ARGS=("--cwd" "${WORKING_DIR}")

# Add session resume if we have a session ID (and not clean restart)
if [[ "${CLEAN}" != "true" ]] && [[ -n "${SESSION_ID}" ]]; then
  CMD_ARGS+=("--resume" "${SESSION_ID}")
fi

echo ""
echo "Spawning new Claude process..."

# Spawn new process using setsid for complete detachment
# setsid creates a new session, completely independent from current process tree
# This ensures the new process survives when we kill the current Claude
setsid "${CLAUDE_CMD}" "${CMD_ARGS[@]}" < /dev/null > /dev/null 2>&1 &

# Give it a moment to start
sleep 0.5

# Check if new process started
if pgrep -f "claude.*--resume" > /dev/null 2>&1 || pgrep -f "^claude$" > /dev/null 2>&1; then
  echo "✅ New Claude process spawned (detached via setsid)"
else
  echo "⚠️  Warning: Could not verify new process started"
fi

echo "   Terminating current process (PID ${CLAUDE_PID})..."

# Remove trigger file (not needed anymore since we handle everything here)
rm -f "${TRIGGER_FILE}"

# Kill current Claude process
# Use SIGTERM for graceful shutdown
kill -TERM "${CLAUDE_PID}" 2>/dev/null || true

echo ""
echo "Restart complete. New session should be active."

exit 0
