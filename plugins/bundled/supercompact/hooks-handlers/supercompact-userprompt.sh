#!/usr/bin/env bash
# supercompact-userprompt.sh — Layer 1: Preemptive compaction trigger
#
# Runs on every UserPromptSubmit. Does a fast file-size check to determine
# if the conversation has grown large enough to need compaction.
#
# This is intentionally CHEAP — just a stat call (~microseconds).
# If threshold is exceeded, delegates to the shared compaction pipeline.
#
# Phase 2 will replace stat with `unleash token-count` (Rust tokenizer)
# for more accurate estimation.

set -uo pipefail

LOG_DIR="${HOME}/.cache/supercompact"
mkdir -p "${LOG_DIR}" 2>/dev/null || true
log() { echo "$(date -Iseconds) [userprompt] $1" >> "${LOG_DIR}/hook.log" 2>/dev/null || true; }

# Read hook input from stdin
HOOK_INPUT=$(cat)

# Extract transcript path
JSONL_FILE=$(echo "${HOOK_INPUT}" | jq -r '.transcript_path // empty' 2>/dev/null) || JSONL_FILE=""

if [[ -z "${JSONL_FILE}" || ! -f "${JSONL_FILE}" ]]; then
  exit 0
fi

# Fast file-size check
FILE_BYTES=$(stat -c %s "${JSONL_FILE}" 2>/dev/null || echo 0)

# Threshold: 60% of effective context window × ~4 bytes/token
# For 200K model: (200000 - 20000) × 0.60 × 4 = 432,000 bytes
# Configurable via env var for different model windows
THRESHOLD=${PLUGIN_SETTING_THRESHOLD_BYTES:-${SUPERCOMPACT_THRESHOLD_BYTES:-432000}}

if (( FILE_BYTES > THRESHOLD )); then
  log "Threshold exceeded (${FILE_BYTES} > ${THRESHOLD} bytes) — triggering preemptive compaction"

  # Resolve plugin root (CLAUDE_PLUGIN_ROOT is set by Claude Code for plugin hooks)
  SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

  # Delegate to shared pipeline
  "${SCRIPT_DIR}/supercompact-compact.sh" \
      --jsonl "${JSONL_FILE}" \
      --trigger "preemptive" &

  # Don't wait for the pipeline — it handles kill/restart itself
  # Exit 0 so Claude continues processing this prompt while compaction starts
  # (The pipeline will kill Claude shortly anyway)
fi

exit 0
