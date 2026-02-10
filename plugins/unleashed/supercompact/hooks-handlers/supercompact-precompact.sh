#!/usr/bin/env bash
# supercompact-precompact.sh - PreCompact hook for EITF entity-preservation compaction
#
# Triggered when Claude Code is about to compact the conversation.
# The PreCompact hook CANNOT block or replace Claude's built-in compaction —
# it is notification-only. So we use it to:
#
#   1. Back up the full transcript before Claude's summarization loses detail
#   2. Run EITF compaction to produce a superior alternative saved alongside
#   3. The user can later resume from the EITF version instead of Claude's
#
# The hook receives JSON on stdin with transcript_path, session_id, trigger, etc.

set -euo pipefail

SUPERCOMPACT_DIR="/home/me/ht/supercompact"
BUDGET="${PLUGIN_SETTING_BUDGET:-80000}"
METHOD="${PLUGIN_SETTING_METHOD:-eitf}"
LOG_DIR="${HOME}/.cache/agent-unleashed/supercompact"

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

# 2. Run EITF compaction to produce a superior alternative
EITF_OUTPUT="${JSONL_FILE}.supercompact"

cd "${SUPERCOMPACT_DIR}"
if uv run python compact.py "${JSONL_FILE}" \
    --method "${METHOD}" \
    --budget "${BUDGET}" \
    --output "${EITF_OUTPUT}" 2>> "${LOG_DIR}/hook.log"; then

  EITF_SIZE=$(wc -l < "${EITF_OUTPUT}")
  echo "$(date -Iseconds) EITF alternative: ${JSONL_SIZE} -> ${EITF_SIZE} lines (saved as .supercompact)" >> "${LOG_DIR}/hook.log"

  # Clean up old backups (keep last 3 of each type)
  ls -t "${JSONL_FILE}.pre-compact-full"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true
  ls -t "${JSONL_FILE}.supercompact"* 2>/dev/null | tail -n +4 | xargs rm -f 2>/dev/null || true

  echo "$(date -Iseconds) SUCCESS: EITF alternative ready at ${EITF_OUTPUT}" >> "${LOG_DIR}/hook.log"
  echo "$(date -Iseconds) NOTE: Claude's built-in compaction will still run (hook cannot block it)" >> "${LOG_DIR}/hook.log"
  echo "$(date -Iseconds) To use EITF version instead: cp '${EITF_OUTPUT}' '${JSONL_FILE}'" >> "${LOG_DIR}/hook.log"
else
  echo "$(date -Iseconds) ERROR: EITF compaction failed (Claude's compaction will proceed)" >> "${LOG_DIR}/hook.log"
  rm -f "${EITF_OUTPUT}" 2>/dev/null || true
fi

exit 0
