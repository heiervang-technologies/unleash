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

# Timeout configuration
# Set to "disabled" or "false" to completely disable timeouts
# BASH_DEFAULT_TIMEOUT_MS: Controls when bash commands auto-background (default: 120000ms / 2min)
# BASH_MAX_TIMEOUT_MS: Maximum allowed timeout value
# MCP_TOOL_TIMEOUT: MCP tool execution timeout

# Helper function to parse timeout value
parse_timeout() {
    local value="$1"
    local default="$2"

    # If not set, use default
    if [[ -z "$value" ]]; then
        echo "$default"
        return
    fi

    # Check for disable keywords
    case "${value,,}" in  # Convert to lowercase
        disabled|false|no|off)
            echo "0"  # 0 typically means unlimited/no timeout
            ;;
        *)
            echo "$value"
            ;;
    esac
}

export BASH_DEFAULT_TIMEOUT_MS=$(parse_timeout "${BASH_DEFAULT_TIMEOUT_MS:-}" "999999999")
export BASH_MAX_TIMEOUT_MS=$(parse_timeout "${BASH_MAX_TIMEOUT_MS:-}" "999999999")
export MCP_TOOL_TIMEOUT=$(parse_timeout "${MCP_TOOL_TIMEOUT:-}" "999999999")

# Ensure cache directory exists
mkdir -p "${CACHE_DIR}"

# Clean up any stale trigger files on start
rm -f "${TRIGGER_FILE}" "${RESTART_MESSAGE_FILE}"

# Track if this is a restart
RESTART_COUNT=0

# Authentication check - only on first run
check_authentication() {
    # Check if CLAUDE_CODE_OAUTH_TOKEN is set
    if [[ -n "${CLAUDE_CODE_OAUTH_TOKEN:-}" ]]; then
        echo "✓ Using OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable"
        return 0
    fi

    # Check for credentials file (Linux/Ubuntu)
    CREDENTIALS_FILE="${HOME}/.claude/.credentials.json"
    if [[ -f "$CREDENTIALS_FILE" ]]; then
        # Verify the credentials file has valid OAuth data
        if command -v jq &>/dev/null; then
            if jq -e '.claudeAiOauth.accessToken' "$CREDENTIALS_FILE" &>/dev/null; then
                echo "✓ Using credentials from ~/.claude/.credentials.json"
                return 0
            fi
        else
            # If jq not available, just check file exists and is non-empty
            if [[ -s "$CREDENTIALS_FILE" ]]; then
                echo "✓ Found credentials file at ~/.claude/.credentials.json"
                return 0
            fi
        fi
    fi

    # Check macOS Keychain (on macOS systems)
    if [[ "$(uname)" == "Darwin" ]]; then
        if security find-generic-password -s "claude" &>/dev/null; then
            echo "✓ Found credentials in macOS Keychain"
            return 0
        fi
    fi

    # No authentication found
    echo ""
    echo "⚠ WARNING: Claude Code authentication not configured"
    echo ""
    echo "To authenticate, you have two options:"
    echo ""
    echo "1. Generate a long-lived OAuth token (recommended for automation):"
    echo "   Run: claude setup-token"
    echo "   Then export: export CLAUDE_CODE_OAUTH_TOKEN=<your-token>"
    echo ""
    echo "2. Authenticate interactively:"
    echo "   Run: claude"
    echo "   Follow the browser authentication flow"
    echo ""
    echo "For more info, see: https://code.claude.com/docs/en/iam"
    echo ""

    # Continue anyway - Claude will prompt for auth
    return 1
}

# Check authentication on first startup (not on restarts)
if [[ ${RESTART_COUNT} -eq 0 ]]; then
    check_authentication || true
fi

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

# Treat SIGTERM (exit code 143 = 128 + 15) as clean exit
# This happens when Claude is gracefully terminated via exit_claude MCP tool
if [[ ${EXIT_CODE} -eq 143 ]]; then
    exit 0
fi

exit ${EXIT_CODE}
