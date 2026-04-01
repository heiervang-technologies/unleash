#!/usr/bin/env bash
# supercompact-precompact.sh - PreCompact hook for entity-preservation compaction
#
# Triggered when Claude Code is about to compact the conversation.
# The PreCompact hook fires inside compactConversation() AFTER the session-memory
# check but BEFORE the Anthropic API summarization call (streamCompactSummary).
# We exploit this timing window to:
#
#   1. Back up the full transcript
#   2. Run EITF compaction (sub-second, no API call)
#   3. Swap the compacted JSONL in-place
#   4. Restart via unleash-refresh — kills the process before the API call fires
#
# On restart, Claude loads the compacted JSONL with --continue. The Anthropic API
# summarization call never completes, saving time and tokens.
#
# Graceful fallback: if EITF fails, exit 0 and let Claude's built-in compact proceed.
# If unleash-refresh is not available, the compacted JSONL is on disk but Claude's
# API compact will overwrite it — user must manually restart.
#
# Configuration via environment variables:
#   PLUGIN_SETTING_METHOD             Scoring method (default: eitf)
#   PLUGIN_SETTING_BUDGET             Token budget (default: 80000)

set -euo pipefail

# Resolve supercompact repo root
# In the unleash bundled plugin, we default to the local path if the environment doesn't specify it
SUPERCOMPACT_DIR="${PLUGIN_SETTING_DIR:-/home/me/ht/supercompact}"

LOG_DIR="${HOME}/.cache/supercompact"
mkdir -p "${LOG_DIR}"

if [[ ! -f "${SUPERCOMPACT_DIR}/compact.py" ]]; then
  echo "$(date -Iseconds) ERROR: compact.py not found at ${SUPERCOMPACT_DIR}" >> "${LOG_DIR}/hook.log"
  exit 0
fi

METHOD="${PLUGIN_SETTING_METHOD:-eitf}"
BUDGET="${PLUGIN_SETTING_BUDGET:-80000}"

# Read hook input from stdin (JSON with transcript_path, session_id, trigger, etc.)
HOOK_INPUT=$(cat)

TRIGGER=$(echo "${HOOK_INPUT}" | jq -r '.trigger // "unknown"')
JSONL_FILE=$(echo "${HOOK_INPUT}" | jq -r '.transcript_path // empty')

echo "$(date -Iseconds) PreCompact hook triggered (trigger=${TRIGGER})" >> "${LOG_DIR}/hook.log"

if [[ -z "${JSONL_FILE}" || ! -f "${JSONL_FILE}" ]]; then
  echo "$(date -Iseconds) ERROR: No transcript_path in hook input or file missing" >> "${LOG_DIR}/hook.log"
  exit 0
fi

JSONL_SIZE=$(wc -l < "${JSONL_FILE}")
echo "$(date -Iseconds) Transcript: ${JSONL_FILE} (${JSONL_SIZE} lines)" >> "${LOG_DIR}/hook.log"

# 1. Back up the full transcript before Claude's compaction destroys detail
BACKUP_FILE="${JSONL_FILE}.pre-compact-full"
cp "${JSONL_FILE}" "${BACKUP_FILE}"
echo "$(date -Iseconds) Full backup saved: ${BACKUP_FILE}" >> "${LOG_DIR}/hook.log"

# 2. Run supercompact and swap the JSONL in-place
#
# Strategy: run EITF (sub-second), replace the session JSONL with the compacted
# version, then restart Claude via unleash-refresh. The restart kills the process
# before the Anthropic API summarization call fires — saving time and tokens.
# On restart, Claude loads the compacted JSONL with --continue.
SC_OUTPUT="/tmp/supercompact-output-$$.jsonl"

echo "$(date -Iseconds) Running supercompact (method=${METHOD}, budget=${BUDGET})" >> "${LOG_DIR}/hook.log"

cd "${SUPERCOMPACT_DIR}"
if uv run python compact.py "${JSONL_FILE}" \
    --method "${METHOD}" \
    --budget "${BUDGET}" \
    --output "${SC_OUTPUT}" 2>> "${LOG_DIR}/hook.log"; then

  SC_SIZE=$(wc -l < "${SC_OUTPUT}")
  echo "$(date -Iseconds) Supercompact (${METHOD}): ${JSONL_SIZE} -> ${SC_SIZE} lines" >> "${LOG_DIR}/hook.log"

  # Back up the pre-compaction JSONL (in addition to the .pre-compact-full backup above)
  cp "${JSONL_FILE}" "${JSONL_FILE}.pre-supercompact"

  # Swap in the compacted version
  mv "${SC_OUTPUT}" "${JSONL_FILE}"
  echo "$(date -Iseconds) SUCCESS: Swapped compacted JSONL in-place" >> "${LOG_DIR}/hook.log"

  # Clean up old backups (keep last 3 of each type)
  ls -t "${JSONL_FILE}.pre-compact-full"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true
  ls -t "${JSONL_FILE}.pre-supercompact"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true

  # Restart Claude to load the compacted context — this kills the process
  # before the Anthropic API compaction call fires. unleash-refresh adds
  # --continue so the session is preserved.
  if command -v unleash-refresh &>/dev/null; then
    echo "$(date -Iseconds) Restarting via unleash-refresh" >> "${LOG_DIR}/hook.log"
    unleash-refresh "COMPACT COMPLETE. Previous context has been summarized. Continue with your current task."
    
    # Block to ensure Claude Code processes the SIGINT and shuts down BEFORE this
    # script exits. This prevents a race condition where the hook completes and 
    # Claude starts the API compaction call before the SIGINT is fully handled.
    sleep 10
    exit 0
  else
    echo "$(date -Iseconds) WARNING: unleash-refresh not found — compacted JSONL is on disk but Claude's API compact will still run over it" >> "${LOG_DIR}/hook.log"
    echo "$(date -Iseconds) NOTE: Run /quit then claude --resume to load the compacted context" >> "${LOG_DIR}/hook.log"
  fi
else
  echo "$(date -Iseconds) ERROR: Supercompact (${METHOD}) failed — falling back to Claude's built-in compaction" >> "${LOG_DIR}/hook.log"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
fi

exit 0
