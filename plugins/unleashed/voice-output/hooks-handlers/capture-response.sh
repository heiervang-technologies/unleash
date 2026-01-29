#!/usr/bin/env bash
# PostToolUse hook: Capture Claude's response and synthesize to speech
#
# This hook runs after Claude generates a response and triggers TTS
# if enabled in plugin settings.

set -euo pipefail

# Read hook input from stdin (contains transcript and response info)
HOOK_INPUT=$(cat)

# Configuration
PLUGIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
CACHE_DIR="${HOME}/.cache/agent-unleashed/voice-output"
PLUGIN_SETTING_ENABLED="${PLUGIN_SETTING_ENABLED:-false}"
PLUGIN_SETTING_PROVIDER="${PLUGIN_SETTING_PROVIDER:-vibevoice}"

# Only run if TTS is enabled
if [[ "${PLUGIN_SETTING_ENABLED}" != "true" ]]; then
  exit 0
fi

# Create cache directory
mkdir -p "${CACHE_DIR}"

# Extract Claude's last response from hook input
# The hook input contains transcript_path
TRANSCRIPT_PATH=$(echo "${HOOK_INPUT}" | jq -r '.transcript_path // empty')

if [[ -z "${TRANSCRIPT_PATH}" ]] || [[ ! -f "${TRANSCRIPT_PATH}" ]]; then
  # No transcript available
  exit 0
fi

# Extract last assistant message from transcript (JSONL format)
if ! grep -q '"role":"assistant"' "${TRANSCRIPT_PATH}"; then
  # No assistant messages
  exit 0
fi

# Get last assistant message
LAST_LINE=$(grep '"role":"assistant"' "${TRANSCRIPT_PATH}" | tail -1)

if [[ -z "${LAST_LINE}" ]]; then
  exit 0
fi

# Parse JSON and extract text content
RESPONSE_TEXT=$(echo "${LAST_LINE}" | jq -r '
  .message.content |
  map(select(.type == "text")) |
  map(.text) |
  join("\n")
' 2>/dev/null)

if [[ -z "${RESPONSE_TEXT}" ]] || [[ "${RESPONSE_TEXT}" == "null" ]]; then
  # No text content
  exit 0
fi

# Save response text to temp file
RESPONSE_FILE="${CACHE_DIR}/last_response.txt"
echo "${RESPONSE_TEXT}" > "${RESPONSE_FILE}"

# Trigger TTS synthesis in background
# This allows the hook to return quickly and not block Claude
{
  "${PLUGIN_DIR}/.venv/bin/python" "${PLUGIN_DIR}/scripts/tts_engine.py" "${RESPONSE_TEXT}" \
    > "${CACHE_DIR}/tts.log" 2>&1 || true
} &

# Exit immediately (don't wait for TTS to complete)
exit 0
