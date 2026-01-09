#!/usr/bin/env bash
# claude-wrapper.sh - Wrapper that enables Claude Code self-restart
#
# Usage: claude-wrapper [claude args...]
#
# This wrapper script enables Claude to restart itself without tmux.
# It works by:
#   1. Running Claude as a child process
#   2. When Claude exits, checking for a restart trigger file
#   3. If trigger exists, restarting Claude with --continue
#
# To restart from within Claude, simply run:
#   restart-claude
#
# Version: 1.0.0 (2026-01-06)

set -uo pipefail

# Get script directory for relative paths (resolve symlinks)
SCRIPT_PATH="$(readlink -f "${BASH_SOURCE[0]}")"
SCRIPT_DIR="$(dirname "$SCRIPT_PATH")"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"

# Export repo root so plugins/hooks can find resources
export CLAUDE_UNLEASHED_ROOT="$REPO_ROOT"

# Auto-patch Claude Code if version changed
PATCH_CHECK_SCRIPT="${SCRIPT_DIR}/check-and-patch.sh"
if [[ -x "$PATCH_CHECK_SCRIPT" ]]; then
    "$PATCH_CHECK_SCRIPT"
fi

# Configuration
CACHE_DIR="${HOME}/.cache/claude-unleashed/process-restart"
WRAPPER_PID=$$
TRIGGER_FILE="${CACHE_DIR}/restart-trigger-${WRAPPER_PID}"
RESTART_MESSAGE_FILE="${CACHE_DIR}/restart-message-${WRAPPER_PID}"

# Default Claude command
CLAUDE_CMD="${CLAUDE_CMD:-claude}"

# Build plugin directory arguments
PLUGIN_ARGS=()
PLUGINS_DIR="${REPO_ROOT}/plugins/unleashed"
if [[ -d "$PLUGINS_DIR" ]]; then
    for plugin in "$PLUGINS_DIR"/*; do
        if [[ -d "$plugin" ]]; then
            PLUGIN_ARGS+=(--plugin-dir "$plugin")
        fi
    done
fi

# Export marker so scripts know we're in the wrapper
export CLAUDE_UNLEASHED=1
export CLAUDE_WRAPPER_PID=${WRAPPER_PID}

# Ensure cache directory exists
mkdir -p "${CACHE_DIR}"

# Clean up any stale trigger files on start
rm -f "${TRIGGER_FILE}" "${RESTART_MESSAGE_FILE}"

# Track if this is a restart
RESTART_COUNT=0

while true; do
    # Clear trigger file before starting
    rm -f "${TRIGGER_FILE}"

    # Build command args
    CMD_ARGS=("$@")

    # If this is a restart, add --continue and RESURRECTED message
    if [[ ${RESTART_COUNT} -gt 0 ]]; then
        # Check if --continue or --resume already in args
        if [[ ! " ${CMD_ARGS[*]} " =~ " --continue " ]] && [[ ! " ${CMD_ARGS[*]} " =~ " --resume " ]]; then
            CMD_ARGS=("--continue" "--dangerously-skip-permissions" "${CMD_ARGS[@]}")
        fi

        # Check for custom restart message, default to RESURRECTED
        if [[ -f "${RESTART_MESSAGE_FILE}" ]]; then
            RESTART_MSG=$(cat "${RESTART_MESSAGE_FILE}")
            rm -f "${RESTART_MESSAGE_FILE}"
        else
            RESTART_MSG="RESURRECTED."
        fi

        if [[ -n "${RESTART_MSG}" ]]; then
            CMD_ARGS+=("${RESTART_MSG}")
        fi
    fi

    # Run Claude with plugins (--dangerously-skip-permissions required for hooks to work)
    "${CLAUDE_CMD}" "${PLUGIN_ARGS[@]}" --dangerously-skip-permissions "${CMD_ARGS[@]}"
    EXIT_CODE=$?

    # Check if restart was requested
    if [[ -f "${TRIGGER_FILE}" ]]; then
        rm -f "${TRIGGER_FILE}"
        RESTART_COUNT=$((RESTART_COUNT + 1))
        sleep 0.3
        continue
    fi

    # Normal exit
    break
done

exit ${EXIT_CODE}
