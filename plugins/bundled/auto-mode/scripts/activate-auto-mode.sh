#!/usr/bin/env bash
# activate-auto-mode.sh - Activates auto mode (wrapper-specific)

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/unleash/auto-mode"

WRAPPER_PID="${AGENT_WRAPPER_PID:-${CLAUDE_WRAPPER_PID:-}}"
if [[ -z "${WRAPPER_PID}" ]]; then
    echo "Error: AGENT_WRAPPER_PID not set. Run under unleash wrapper."
    exit 1
fi

AUTO_MODE_FILE="${AUTO_MODE_DIR}/active-${WRAPPER_PID}"
mkdir -p "${AUTO_MODE_DIR}"

echo "${CLAUDE_SESSION_ID:-unknown}" > "${AUTO_MODE_FILE}"
echo "Auto mode activated for wrapper ${WRAPPER_PID}"
