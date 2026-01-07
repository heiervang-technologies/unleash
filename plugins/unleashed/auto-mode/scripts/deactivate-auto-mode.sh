#!/usr/bin/env bash
# deactivate-auto-mode.sh - Deactivates auto mode by removing the flag file

set -uo pipefail

AUTO_MODE_FILE="${HOME}/.cache/claude-unleashed/auto-mode/active"

if [[ -f "${AUTO_MODE_FILE}" ]]; then
    rm -f "${AUTO_MODE_FILE}"
    echo "Auto mode deactivated."
else
    echo "Auto mode was not active."
fi
