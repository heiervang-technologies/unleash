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

# Self-disable guard: bail if supercompact is disabled in unleash config.
# Robust against stale hook registrations the wrapper failed to prune.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_ENABLED="${SCRIPT_DIR}/../scripts/check-enabled.sh"
if [[ -x "${CHECK_ENABLED}" ]] && ! "${CHECK_ENABLED}" supercompact; then
  exit 0
fi

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

# Token count check — use real tokenizer if available, fall back to byte estimate
THRESHOLD_TOKENS=${PLUGIN_SETTING_THRESHOLD_TOKENS:-${SUPERCOMPACT_THRESHOLD_TOKENS:-200000}}
THRESHOLD_BYTES=${PLUGIN_SETTING_THRESHOLD_BYTES:-${SUPERCOMPACT_THRESHOLD_BYTES:-2700000}}

# Fast path: skip tokenizer if file is clearly under threshold (bytes / 16 is a safe lower bound)
FILE_BYTES=$(stat -c %s "${JSONL_FILE}" 2>/dev/null || echo 0)
if (( FILE_BYTES / 16 > THRESHOLD_TOKENS )); then
  OVER_THRESHOLD=1
elif (( FILE_BYTES / 4 < THRESHOLD_TOKENS )); then
  # Clearly under threshold even with generous estimate
  OVER_THRESHOLD=0
elif command -v unleash &>/dev/null; then
  # Ambiguous zone — use real tokenizer
  TOKEN_COUNT=$(unleash token-count "${JSONL_FILE}" 2>/dev/null) || TOKEN_COUNT=0
  if (( TOKEN_COUNT > THRESHOLD_TOKENS )); then
    OVER_THRESHOLD=1
  else
    OVER_THRESHOLD=0
  fi
else
  # No tokenizer available, fall back to byte threshold
  if (( FILE_BYTES > THRESHOLD_BYTES )); then
    OVER_THRESHOLD=1
  else
    OVER_THRESHOLD=0
  fi
fi

if (( OVER_THRESHOLD )); then
  log "Threshold exceeded (file=${FILE_BYTES} bytes, threshold=${THRESHOLD_TOKENS} tokens) — triggering preemptive compaction"

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
