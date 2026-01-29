#!/usr/bin/env bash
# deactivate-auto-mode.sh - Deactivates auto mode (wrapper-specific)

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/agent-unleashed/auto-mode"

WRAPPER_PID="${CLAUDE_WRAPPER_PID:-}"
if [[ -z "${WRAPPER_PID}" ]]; then
    echo "Error: CLAUDE_WRAPPER_PID not set. Run under agent-unleashed wrapper."
    exit 1
fi

AUTO_MODE_FILE="${AUTO_MODE_DIR}/active-${WRAPPER_PID}"

if [[ -f "${AUTO_MODE_FILE}" ]]; then
    rm -f "${AUTO_MODE_FILE}"
    echo "Auto mode deactivated for wrapper ${WRAPPER_PID}"
else
    echo "Auto mode was not active for this session."
fi
