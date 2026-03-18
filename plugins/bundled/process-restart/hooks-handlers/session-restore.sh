#!/usr/bin/env bash
# SessionStart hook: Restore session state after restart
#
# This hook checks for a restart state file on session start and applies
# the saved state to restore the session context.

set -euo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/unleash/process-restart"
STATE_FILE="${CACHE_DIR}/restart-state.json"
PLUGIN_SETTING_STATE_EXPIRY="${PLUGIN_SETTING_STATE_EXPIRY:-300}"  # 5 minutes default

# Check if state file exists
if [[ ! -f "${STATE_FILE}" ]]; then
  # No state file - normal session start
  exit 0
fi

# Read and validate state file
if ! jq empty "${STATE_FILE}" 2>/dev/null; then
  # Invalid JSON - remove and exit
  rm -f "${STATE_FILE}"
  exit 0
fi

# Check file age (prevent stale state restoration)
FILE_TIMESTAMP=$(jq -r '.timestamp' "${STATE_FILE}" 2>/dev/null || echo "0")
CURRENT_TIMESTAMP=$(date +%s)
AGE=$((CURRENT_TIMESTAMP - FILE_TIMESTAMP))

if [[ ${AGE} -gt ${PLUGIN_SETTING_STATE_EXPIRY} ]]; then
  # State file expired - remove and exit
  rm -f "${STATE_FILE}"
  cat <<EOF
{
  "type": "prompt",
  "content": "⚠️  Restart state file found but expired (age: ${AGE}s, max: ${PLUGIN_SETTING_STATE_EXPIRY}s).\\n\\nStarting fresh session instead."
}
EOF
  exit 0
fi

# Extract state information
SESSION_ID=$(jq -r '.sessionId // empty' "${STATE_FILE}")
WORKING_DIR=$(jq -r '.workingDir // empty' "${STATE_FILE}")
MODEL=$(jq -r '.model // empty' "${STATE_FILE}")
GIT_BRANCH=$(jq -r '.gitBranch // empty' "${STATE_FILE}")

# Build restoration message
RESTORE_INFO=""
if [[ -n "${SESSION_ID}" ]]; then
  RESTORE_INFO+="- Session ID: ${SESSION_ID}\\n"
fi
if [[ -n "${WORKING_DIR}" ]]; then
  RESTORE_INFO+="- Working directory: ${WORKING_DIR}\\n"
fi
if [[ -n "${MODEL}" ]]; then
  RESTORE_INFO+="- Model: ${MODEL}\\n"
fi
if [[ -n "${GIT_BRANCH}" ]]; then
  RESTORE_INFO+="- Git branch: ${GIT_BRANCH}\\n"
fi

# Change to working directory if specified and different
if [[ -n "${WORKING_DIR}" ]] && [[ -d "${WORKING_DIR}" ]]; then
  CURRENT_DIR=$(pwd)
  if [[ "${CURRENT_DIR}" != "${WORKING_DIR}" ]]; then
    cd "${WORKING_DIR}" || true
  fi
fi

# Output restoration notification
cat <<EOF
{
  "type": "prompt",
  "content": "🔄 Session restored from restart\\n\\nRestored state:\\n${RESTORE_INFO}\\nMCP servers reloaded with current configuration.\\n\\nYou can continue where you left off."
}
EOF

# Clean up state file
rm -f "${STATE_FILE}"

exit 0
