#!/usr/bin/env bash
# auto-mode-stop.sh - Stop hook that enforces auto mode (wrapper-specific)
#
# When auto mode is active for THIS wrapper, this hook blocks Claude from stopping.
#
# Message priority:
#   1. Session-specific: ~/.cache/unleash/auto-mode/reminder-${PID}
#   2. Global config:    ~/.config/unleash/config.toml (stop_prompt)
#   3. Default:          Hardcoded message

set -uo pipefail

AUTO_MODE_DIR="${HOME}/.cache/unleash/auto-mode"
CONFIG_FILE="${HOME}/.config/unleash/config.toml"

# Get wrapper PID - if not set, allow stop (not running under wrapper)
WRAPPER_PID="${AGENT_WRAPPER_PID:-${CLAUDE_WRAPPER_PID:-}}"
if [[ -z "${WRAPPER_PID}" ]]; then
    exit 0
fi

AUTO_MODE_FILE="${AUTO_MODE_DIR}/active-${WRAPPER_PID}"

# Check if auto mode is active for THIS wrapper
if [[ -f "${AUTO_MODE_FILE}" ]]; then
    # Check for custom reminder message (priority order)
    REMINDER_FILE="${AUTO_MODE_DIR}/reminder-${WRAPPER_PID}"
    DEFAULT_MSG="You ended your turn, but you are in auto-mode. If you are awaiting a decision, select your recommended decision. If you are done, consider that you have covered all other diligences, testing, documentation, technical debt and cleanup. Use the executables (in PATH) 'restart-claude' if you need to restart yourself, and 'exit-claude' if you are truly done with all your tasks."

    if [[ -f "${REMINDER_FILE}" ]]; then
        # 1. Session-specific reminder (highest priority)
        REASON=$(cat "${REMINDER_FILE}")
    elif [[ -f "${CONFIG_FILE}" ]]; then
        # 2. Global config from config.toml (handles single-line and multiline TOML)
        GLOBAL_PROMPT=$(python3 -c "
import sys, json
try:
    import tomllib
except ImportError:
    import tomli as tomllib
with open(sys.argv[1], 'rb') as f:
    c = tomllib.load(f)
v = c.get('stop_prompt', '')
if v:
    print(v, end='')
" "${CONFIG_FILE}" 2>/dev/null)
        if [[ -n "${GLOBAL_PROMPT}" ]]; then
            REASON="${GLOBAL_PROMPT}"
        else
            REASON="${DEFAULT_MSG}"
        fi
    else
        # 3. Default message
        REASON="${DEFAULT_MSG}"
    fi

    # Emit JSON with proper escaping via python
    printf '%s' "${REASON}" | python3 -c "
import json, sys
print(json.dumps({'decision': 'block', 'reason': sys.stdin.read()}))"
    exit 0
fi

# Auto mode not active - allow normal stop
exit 0
