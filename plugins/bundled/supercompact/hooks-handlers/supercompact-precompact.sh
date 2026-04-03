#!/usr/bin/env bash
# supercompact-precompact.sh — Layer 2: Manual /compact handler
#
# Triggered when user types /compact. With DISABLE_AUTO_COMPACT=1 set by
# Unleash, this hook should ONLY fire for manual /compact — never for
# auto-compaction (which is disabled).
#
# Delegates to the shared compaction pipeline with --trigger manual.
# The pipeline handles locking, backup, compaction, kill, replace, restart.
#
# Graceful fallback: if anything goes wrong, exits 0 to let Claude's
# built-in compaction proceed (though with DISABLE_AUTO_COMPACT=1,
# the manual /compact API call would still fire after this hook returns).

set -uo pipefail

LOG_DIR="${HOME}/.cache/supercompact"
mkdir -p "${LOG_DIR}" 2>/dev/null || true
log() { echo "$(date -Iseconds) [precompact] $1" >> "${LOG_DIR}/hook.log" 2>/dev/null || true; }

# Read hook input from stdin
HOOK_INPUT=$(cat)

TRIGGER=$(echo "${HOOK_INPUT}" | jq -r '.trigger // "manual"' 2>/dev/null) || TRIGGER="manual"
JSONL_FILE=$(echo "${HOOK_INPUT}" | jq -r '.transcript_path // empty' 2>/dev/null) || JSONL_FILE=""

log "PreCompact hook triggered (trigger=${TRIGGER})"

if [[ -z "${JSONL_FILE}" || ! -f "${JSONL_FILE}" ]]; then
  log "SKIP: No transcript_path or file missing"
  exit 0
fi

# Resolve script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PIPELINE="${SCRIPT_DIR}/supercompact-compact.sh"

if [[ ! -x "${PIPELINE}" ]]; then
  log "ERROR: Shared pipeline not found at ${PIPELINE}"
  exit 0
fi

# Delegate to shared pipeline
# Run in foreground — we WANT to block the hook return so the pipeline
# can kill Claude before the API compaction call fires.
log "Delegating to shared pipeline"
exec "${PIPELINE}" --jsonl "${JSONL_FILE}" --trigger "manual"
