#!/usr/bin/env bash
# trigger-restart.sh - Trigger Claude Code self-restart
#
# Supports two methods:
#   1. Wrapper method (preferred) - Works if started via unleash (unleash)
#   2. tmux method (fallback) - Works if running inside tmux
#
# The script auto-detects which method is available.
#
# Version: 1.3.0 (2026-01-28)

set -uo pipefail

# Configuration
CACHE_DIR="${HOME}/.cache/unleash/process-restart"
WRAPPER_PID="${AGENT_WRAPPER_PID:-}"
TRIGGER_FILE="${CACHE_DIR}/restart-trigger${WRAPPER_PID:+-${WRAPPER_PID}}"
RESTART_MESSAGE_FILE="${CACHE_DIR}/restart-message${WRAPPER_PID:+-${WRAPPER_PID}}"

# Parse command line arguments
_FORCE=false  # Reserved for future use
CLEAN=false
INITIAL_MESSAGE=""
METHOD=""  # auto, wrapper, tmux

while [[ $# -gt 0 ]]; do
  case "$1" in
    --force)
      _FORCE=true
      shift
      ;;
    --clean)
      CLEAN=true
      shift
      ;;
    --message)
      INITIAL_MESSAGE="$2"
      shift 2
      ;;
    --method)
      METHOD="$2"
      shift 2
      ;;
    *)
      echo "Unknown option: $1"
      echo "Usage: $0 [--force] [--clean] [--message 'msg'] [--method wrapper|tmux]"
      exit 1
      ;;
  esac
done

# Create cache directory
mkdir -p "${CACHE_DIR}"

# Find Claude PID
find_claude_pid() {
    local pid
    pid=$(pgrep -f "^claude" | head -1 || true)
    if [[ -z "${pid}" ]]; then
        # shellcheck disable=SC2009
    pid=$(ps aux | grep '[c]laude' | grep -v defunct | head -1 | awk '{print $2}' || true)
    fi
    echo "${pid}"
}

CLAUDE_PID=$(find_claude_pid)
if [[ -z "${CLAUDE_PID}" ]]; then
    echo "Error: Could not find running Claude process"
    exit 1
fi

# Detect which method is available
detect_method() {
    # Check if running under wrapper (wrapper sets this in environment or we check parent)
    # Simple heuristic: check if parent process is bash running claude-wrapper
    local parent_cmd
    parent_cmd=$(ps -o args= -p $PPID 2>/dev/null | head -1 || true)

    if [[ "${parent_cmd}" == *"claude-wrapper"* ]]; then
        echo "wrapper"
        return
    fi

    # Check if trigger file mechanism would work (wrapper watches for this)
    # We can detect wrapper by checking if our grandparent is the wrapper
    local grandparent_pid
    grandparent_pid=$(ps -o ppid= -p $PPID 2>/dev/null | tr -d ' ' || true)
    if [[ -n "${grandparent_pid}" ]]; then
        local grandparent_cmd
        grandparent_cmd=$(ps -o args= -p "${grandparent_pid}" 2>/dev/null | head -1 || true)
        if [[ "${grandparent_cmd}" == *"claude-wrapper"* ]]; then
            echo "wrapper"
            return
        fi
    fi

    # Check for tmux
    if [[ -n "${TMUX:-}" ]]; then
        echo "tmux"
        return
    fi

    # No method available
    echo "none"
}

if [[ -z "${METHOD}" ]] || [[ "${METHOD}" == "auto" ]]; then
    METHOD=$(detect_method)
fi

echo "Claude PID: ${CLAUDE_PID}"
echo "Restart method: ${METHOD}"

# Handle based on method
case "${METHOD}" in
    wrapper)
        echo ""
        echo "Using wrapper method (trigger file)"

        # Create trigger file
        touch "${TRIGGER_FILE}"
        echo "Created trigger file: ${TRIGGER_FILE}"

        # Save message if provided
        if [[ -n "${INITIAL_MESSAGE}" ]]; then
            echo "${INITIAL_MESSAGE}" > "${RESTART_MESSAGE_FILE}"
            echo "Restart message: ${INITIAL_MESSAGE}"
        fi

        # If clean restart, we don't create trigger (wrapper won't add --continue)
        if [[ "${CLEAN}" == "true" ]]; then
            rm -f "${TRIGGER_FILE}"
            echo "Clean restart: trigger file removed (no --continue)"
        fi

        echo ""
        echo "Killing Claude... wrapper will restart automatically."

        # Kill Claude - wrapper will detect exit and check trigger
        kill -INT "${CLAUDE_PID}" 2>/dev/null || kill -TERM "${CLAUDE_PID}" 2>/dev/null || true
        ;;

    tmux)
        echo ""
        echo "Using tmux method (send-keys)"

        # Get tmux target
        TMUX_TARGET=$(tmux display-message -p '#{session_name}:#{window_index}.#{pane_index}')
        echo "Tmux target: ${TMUX_TARGET}"

        # Build restart command
        if [[ "${CLEAN}" == "true" ]]; then
            RESTART_CMD="claude"
        else
            RESTART_CMD="${RESTART_COMMAND:-claude --continue}"
        fi

        if [[ -n "${INITIAL_MESSAGE}" ]]; then
            RESTART_CMD="${RESTART_CMD} '${INITIAL_MESSAGE}'"
        fi

        echo "Restart command: ${RESTART_CMD}"

        # Spawn watcher
        (
            while kill -0 "${CLAUDE_PID}" 2>/dev/null; do
                sleep 0.1
            done
            sleep 0.5
            tmux send-keys -t "${TMUX_TARGET}" "${RESTART_CMD}" Enter
        ) &

        echo "Watcher spawned, killing Claude..."

        # Kill Claude
        kill -INT "${CLAUDE_PID}" 2>/dev/null || kill -TERM "${CLAUDE_PID}" 2>/dev/null || true
        ;;

    none|*)
        echo ""
        echo "Error: No restart method available"
        echo ""
        echo "Claude Code self-restart requires one of:"
        echo ""
        echo "  1. Wrapper method (recommended):"
        echo "     Start Claude with: unleash (unleash)"
        echo "     Location: scripts/unleash"
        echo ""
        echo "  2. tmux method:"
        echo "     Run Claude inside tmux: tmux new-session -s claude"
        echo ""
        exit 1
        ;;
esac

exit 0
