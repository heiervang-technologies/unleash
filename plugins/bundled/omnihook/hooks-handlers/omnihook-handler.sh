#!/usr/bin/env bash
# Omnihook Handler - Universal hook for message queue integration
#
# This script runs on ALL Claude Code hook events and checks for queued messages.
# If a message is waiting, it injects it into the session immediately.
#
# Hook types this handles:
# - Stop: Can block exit and inject message
# - PreToolUse/PostToolUse: Can inject prompts
# - SessionStart: Can inject initial message
# - Notification: Can inject messages
#
# Usage: Called by Claude Code hooks system with HOOK_EVENT environment variable

set -euo pipefail

# Configuration
QUEUE_DIR="${HOME}/.cache/unleash/omnihook"
WRAPPER_PID="${AGENT_WRAPPER_PID:-$$}"
QUEUE_FILE="${QUEUE_DIR}/queue-${WRAPPER_PID}"
# shellcheck disable=SC2034
FIFO_FILE="${QUEUE_DIR}/fifo-${WRAPPER_PID}"
LOCK_FILE="${QUEUE_DIR}/lock-${WRAPPER_PID}"

# Get the hook event type from environment
HOOK_EVENT="${HOOK_EVENT:-unknown}"

# Drain stdin to prevent blocking (hook input not needed by this handler)
if [[ ! -t 0 ]]; then
  exec 0</dev/null
fi

# Ensure queue directory exists
mkdir -p "${QUEUE_DIR}"

# Function to atomically read and clear the queue
read_and_clear_queue() {
  local message=""

  # Use file locking to prevent race conditions
  (
    flock -x -w 2 200 2>/dev/null || true

    if [[ -f "${QUEUE_FILE}" ]] && [[ -s "${QUEUE_FILE}" ]]; then
      # Read the first message (messages are newline-separated JSON)
      message=$(head -1 "${QUEUE_FILE}")

      # Remove the first line (consumed message)
      tail -n +2 "${QUEUE_FILE}" > "${QUEUE_FILE}.tmp" 2>/dev/null || true
      mv "${QUEUE_FILE}.tmp" "${QUEUE_FILE}" 2>/dev/null || rm -f "${QUEUE_FILE}"

      # Clean up empty file
      if [[ -f "${QUEUE_FILE}" ]] && [[ ! -s "${QUEUE_FILE}" ]]; then
        rm -f "${QUEUE_FILE}"
      fi
    fi

    echo "${message}"
  ) 200>"${LOCK_FILE}"
}

# Function to check if queue has messages (non-destructive)
queue_has_messages() {
  [[ -f "${QUEUE_FILE}" ]] && [[ -s "${QUEUE_FILE}" ]]
}

# Main logic based on hook event type
case "${HOOK_EVENT}" in
  Stop)
    # Stop hook can block exit and inject a message
    if queue_has_messages; then
      message=$(read_and_clear_queue)
      if [[ -n "${message}" ]]; then
        # Extract the text content from the queue message
        text=$(echo "${message}" | jq -r '.text // .message // .' 2>/dev/null || echo "${message}")

        # Block the stop and inject the message
        jq -n \
          --arg reason "${text}" \
          --arg system "Voice message received via omnihook" \
          '{
            "decision": "block",
            "reason": $reason,
            "systemMessage": $system
          }'
        exit 0
      fi
    fi
    # No message - allow normal exit
    exit 0
    ;;

  PreToolUse|PostToolUse|Notification)
    # These hooks can inject prompts but not block
    if queue_has_messages; then
      message=$(read_and_clear_queue)
      if [[ -n "${message}" ]]; then
        text=$(echo "${message}" | jq -r '.text // .message // .' 2>/dev/null || echo "${message}")

        # Return prompt to inject into conversation
        jq -n \
          --arg content "${text}" \
          '{
            "type": "prompt",
            "content": $content
          }'
        exit 0
      fi
    fi
    # No message - silent exit
    exit 0
    ;;

  SessionStart)
    # Check for pending messages at session start
    if queue_has_messages; then
      message=$(read_and_clear_queue)
      if [[ -n "${message}" ]]; then
        text=$(echo "${message}" | jq -r '.text // .message // .' 2>/dev/null || echo "${message}")

        jq -n \
          --arg content "Queued voice message: ${text}" \
          '{
            "type": "prompt",
            "content": $content
          }'
        exit 0
      fi
    fi
    exit 0
    ;;

  *)
    # Unknown hook type - just check and report
    exit 0
    ;;
esac
