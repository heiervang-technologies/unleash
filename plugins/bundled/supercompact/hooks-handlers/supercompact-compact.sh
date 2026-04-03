#!/usr/bin/env bash
# supercompact-compact.sh — Shared compaction pipeline
#
# Used by both Layer 1 (preemptive via UserPromptSubmit) and
# Layer 2 (manual via PreCompact /compact handler).
#
# Steps:
#   1. Acquire flock (prevents concurrent compaction)
#   2. Backup full transcript
#   3. Run compact.py (EITF scoring)
#   4. Validate output (non-empty, valid JSON)
#   5. Kill Claude process (SIGINT, then SIGKILL after timeout)
#   6. Replace JSONL with compacted version (AFTER process is dead)
#   7. Restart via unleash-refresh
#
# Usage:
#   supercompact-compact.sh --jsonl <path> --trigger <preemptive|manual> [--budget <tokens>]
#
# Graceful fallback: any failure exits 0, letting Claude continue uncompacted.

# NO set -e — we want graceful fallback on any failure
set -uo pipefail

# --- Logging ---

LOG_DIR="${HOME}/.cache/supercompact"
mkdir -p "${LOG_DIR}" 2>/dev/null || true
LOG_FILE="${LOG_DIR}/hook.log"

log() { echo "$(date -Iseconds) [compact] $1" >> "${LOG_FILE}" 2>/dev/null || true; }

# --- Parse arguments ---

JSONL_FILE=""
TRIGGER="unknown"
BUDGET=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --jsonl)   JSONL_FILE="$2"; shift 2 ;;
    --trigger) TRIGGER="$2"; shift 2 ;;
    --budget)  BUDGET="$2"; shift 2 ;;
    *) shift ;;
  esac
done

if [[ -z "${JSONL_FILE}" || ! -f "${JSONL_FILE}" ]]; then
  log "ERROR: No JSONL file provided or file missing (${JSONL_FILE})"
  exit 0
fi

# --- Configuration ---

SUPERCOMPACT_DIR="${PLUGIN_SETTING_DIR:-${HOME}/ht/supercompact}"
METHOD="${PLUGIN_SETTING_METHOD:-eitf}"

# Dynamic budget based on trigger type:
#   preemptive (Layer 1): 50% of context window — gentler, leaves room for new work
#   manual (Layer 2): 80K tokens — aggressive safety net
if [[ -z "${BUDGET}" ]]; then
  if [[ "${TRIGGER}" == "preemptive" ]]; then
    BUDGET="${PLUGIN_SETTING_BUDGET_PREEMPTIVE:-90000}"
  else
    BUDGET="${PLUGIN_SETTING_BUDGET:-80000}"
  fi
fi

LOCKFILE="/tmp/supercompact.lock"
LOCK_FD=9

log "Pipeline started (trigger=${TRIGGER}, budget=${BUDGET}, file=${JSONL_FILE})"

# --- 1. Acquire lock (flock, not mkdir) ---

exec 9>"${LOCKFILE}"
if ! flock -n ${LOCK_FD}; then
  log "SKIP: Another compaction is already running (flock held)"
  exit 0
fi
# Lock auto-releases when fd 9 closes (script exit)

# --- 2. Dependency checks ---

if [[ ! -f "${SUPERCOMPACT_DIR}/compact.py" ]]; then
  log "SKIP: compact.py not found at ${SUPERCOMPACT_DIR}"
  exit 0
fi

if ! command -v jq &>/dev/null; then
  log "SKIP: jq not found"
  exit 0
fi

if ! command -v uv &>/dev/null; then
  log "SKIP: uv not found"
  exit 0
fi

# --- 3. Backup full transcript ---

BACKUP_FILE="${JSONL_FILE}.pre-compact-full"
if ! cp "${JSONL_FILE}" "${BACKUP_FILE}"; then
  log "ERROR: Failed to backup transcript"
  exit 0
fi

JSONL_LINES=$(wc -l < "${JSONL_FILE}" 2>/dev/null || echo 0)
JSONL_BYTES=$(stat -c %s "${JSONL_FILE}" 2>/dev/null || echo 0)
log "Backup saved (${JSONL_LINES} lines, ${JSONL_BYTES} bytes)"

# --- 4. Run compact.py ---

SC_OUTPUT="/tmp/supercompact-output-$$.jsonl"
rm -f "${SC_OUTPUT}" 2>/dev/null || true

log "Running compact.py (method=${METHOD}, budget=${BUDGET})"

cd "${SUPERCOMPACT_DIR}" || { log "ERROR: Cannot cd to ${SUPERCOMPACT_DIR}"; exit 0; }

if ! uv run python compact.py "${JSONL_FILE}" \
    --method "${METHOD}" \
    --budget "${BUDGET}" \
    --output "${SC_OUTPUT}" 2>> "${LOG_FILE}"; then
  log "ERROR: compact.py failed (exit $?)"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
  exit 0
fi

# --- 5. Validate output ---

if [[ ! -f "${SC_OUTPUT}" ]]; then
  log "SKIP: No output file produced (conversation may already be within budget)"
  exit 0
fi

if [[ ! -s "${SC_OUTPUT}" ]]; then
  log "ERROR: Output file is empty"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
  exit 0
fi

# Validate last line is valid JSON
LAST_LINE=$(tail -1 "${SC_OUTPUT}")
if ! echo "${LAST_LINE}" | jq empty 2>/dev/null; then
  log "ERROR: Last line of output is not valid JSON — output may be truncated"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
  exit 0
fi

SC_LINES=$(wc -l < "${SC_OUTPUT}" 2>/dev/null || echo 0)
SC_BYTES=$(stat -c %s "${SC_OUTPUT}" 2>/dev/null || echo 0)
log "Compaction complete: ${JSONL_LINES} -> ${SC_LINES} lines, ${JSONL_BYTES} -> ${SC_BYTES} bytes"

# Validate first line is also valid JSON (catch truncated-at-start corruption)
FIRST_LINE=$(head -1 "${SC_OUTPUT}")
if ! echo "${FIRST_LINE}" | jq empty 2>/dev/null; then
  log "ERROR: First line of output is not valid JSON — output may be corrupt"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
  exit 0
fi

# Minimum line count guard — output should retain at least 10% of input lines (min 20)
MIN_LINES=$((JSONL_LINES / 10))
(( MIN_LINES < 20 )) && MIN_LINES=20
if (( SC_LINES < MIN_LINES )); then
  log "ERROR: Output suspiciously small (${SC_LINES} lines, minimum expected ${MIN_LINES}) — refusing to replace"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
  exit 0
fi

# --- 6. Replace JSONL ---
# Safe to replace while Claude is running: Claude holds all messages in memory
# and only reads the JSONL at session startup. The file on disk is append-only
# during a session. After replacement, unleash-refresh will kill Claude and the
# wrapper restarts it, loading the compacted JSONL fresh.

# Keep a pre-supercompact backup
cp "${JSONL_FILE}" "${JSONL_FILE}.pre-supercompact" 2>/dev/null || true

if ! mv "${SC_OUTPUT}" "${JSONL_FILE}"; then
  log "ERROR: Failed to replace JSONL — restoring from backup"
  cp "${BACKUP_FILE}" "${JSONL_FILE}" 2>/dev/null || true
  exit 0
fi

log "JSONL replaced successfully"

# Clean up old backups (keep last 3 of each type)
for pattern in ".pre-compact-full" ".pre-supercompact"; do
  ls -t "${JSONL_FILE}${pattern}"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true
done

# --- 7. Restart via unleash-refresh ---
# unleash-refresh handles everything: finds Claude's PID via process tree,
# creates the trigger file so the wrapper knows to restart, and sends SIGINT.
# The wrapper then restarts Claude with --continue, loading the compacted JSONL.

if command -v unleash-refresh &>/dev/null; then
  log "Restarting via unleash-refresh"
  unleash-refresh "COMPACT COMPLETE. Previous context has been summarized. Continue with your current task."
  log "unleash-refresh called — wrapper should restart Claude"
else
  log "WARNING: unleash-refresh not found — compacted JSONL is on disk but Claude won't auto-restart"
  log "NOTE: Run 'claude --continue' manually to load the compacted context"
fi

log "Pipeline finished (trigger=${TRIGGER})"
exit 0
