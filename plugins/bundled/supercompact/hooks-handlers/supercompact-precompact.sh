#!/usr/bin/env bash
# supercompact-precompact.sh - PreCompact hook for entity-preservation compaction
#
# Triggered when Claude Code is about to compact the conversation.
# The PreCompact hook CANNOT block or replace Claude's built-in compaction —
# it is notification-only. So we use it to:
#
#   1. Back up the full transcript before Claude's summarization loses detail
#   2. Run compaction (configurable method) to produce a superior alternative
#   3. The user can later resume from the supercompact version instead of Claude's
#
# Configuration via environment variables:
#   PLUGIN_SETTING_METHOD             Scoring method (default: eitf)
#   PLUGIN_SETTING_BUDGET             Token budget (default: 80000)
#   PLUGIN_SETTING_FALLBACK_TO_BUILTIN  Ignored here (hook can't block builtin anyway)
#
# The hook receives JSON on stdin with transcript_path, session_id, trigger, etc.

set -euo pipefail

SUPERCOMPACT_DIR="/home/me/ht/supercompact"
METHOD="${PLUGIN_SETTING_METHOD:-eitf}"
BUDGET="${PLUGIN_SETTING_BUDGET:-80000}"
LOG_DIR="${HOME}/.cache/unleash/supercompact"

mkdir -p "${LOG_DIR}"

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

# 2. Run supercompact to produce a superior alternative
SC_OUTPUT="${JSONL_FILE}.supercompact"

echo "$(date -Iseconds) Running supercompact (method=${METHOD}, budget=${BUDGET})" >> "${LOG_DIR}/hook.log"

cd "${SUPERCOMPACT_DIR}"
if uv run python compact.py "${JSONL_FILE}" \
    --method "${METHOD}" \
    --budget "${BUDGET}" \
    --output "${SC_OUTPUT}" 2>> "${LOG_DIR}/hook.log"; then

  SC_SIZE=$(wc -l < "${SC_OUTPUT}")
  echo "$(date -Iseconds) Supercompact (${METHOD}): ${JSONL_SIZE} -> ${SC_SIZE} lines (saved as .supercompact)" >> "${LOG_DIR}/hook.log"

  # Clean up old backups (keep last 3 of each type)
  ls -t "${JSONL_FILE}.pre-compact-full"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true
  ls -t "${JSONL_FILE}.supercompact"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true

  echo "$(date -Iseconds) SUCCESS: Supercompact alternative ready at ${SC_OUTPUT}" >> "${LOG_DIR}/hook.log"
  echo "$(date -Iseconds) NOTE: Claude's built-in compaction will still run (hook cannot block it)" >> "${LOG_DIR}/hook.log"
  echo "$(date -Iseconds) To use supercompact version: cp '${SC_OUTPUT}' '${JSONL_FILE}'" >> "${LOG_DIR}/hook.log"
else
  echo "$(date -Iseconds) ERROR: Supercompact (${METHOD}) failed (Claude's compaction will proceed)" >> "${LOG_DIR}/hook.log"
  rm -f "${SC_OUTPUT}" 2>/dev/null || true
fi

exit 0
