#!/usr/bin/env bash
# token-usage-stop.sh — Stop hook that logs the assistant's token usage from
# the most recent message in the active session transcript.
#
# Receives Claude Code's standard hook input on stdin:
#   { "session_id": "...", "transcript_path": "/path/to/session.jsonl", ... }
#
# Walks the JSONL backwards to find the last record with a `message.usage`
# field and appends one line to the local usage log:
#   ~/.local/share/unleash/token-usage.jsonl
#
# Designed to fail safe: any error (missing jq, malformed JSON, settings file
# unreadable) is silently dropped — token tracking must never block the agent.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# 1. Self-disable guard: skip entirely if the user disabled the plugin.
CHECK_ENABLED="${SCRIPT_DIR}/../scripts/check-enabled.sh"
if [[ -x "${CHECK_ENABLED}" ]] && ! "${CHECK_ENABLED}" token-usage; then
  exit 0
fi

# 2. Per-method toggle. Settings file is written by the unleash TUI. Default
#    matches plugin.json: method_session_tail = true.
SETTINGS_FILE="${HOME}/.config/unleash/plugins/token-usage/settings.env"
PLUGIN_SETTING_METHOD_SESSION_TAIL="true"
if [[ -r "${SETTINGS_FILE}" ]]; then
  # shellcheck disable=SC1090
  source "${SETTINGS_FILE}"
fi
if [[ "${PLUGIN_SETTING_METHOD_SESSION_TAIL,,}" != "true" ]]; then
  exit 0
fi

# 3. Required tools — jq is the only hard dependency.
if ! command -v jq >/dev/null 2>&1; then
  exit 0
fi

# 4. Read hook input.
HOOK_INPUT=$(cat)
SESSION_ID=$(jq -r '.session_id // empty' <<<"${HOOK_INPUT}" 2>/dev/null) || SESSION_ID=""
TRANSCRIPT=$(jq -r '.transcript_path // empty' <<<"${HOOK_INPUT}" 2>/dev/null) || TRANSCRIPT=""

if [[ -z "${TRANSCRIPT}" || ! -r "${TRANSCRIPT}" ]]; then
  exit 0
fi

# 5. Find the last record carrying a real usage block — meaning model is not
#    Claude Code's "<synthetic>" placeholder AND at least one token field is
#    non-zero. Scan from the bottom up so we stop at the first match — fast
#    even on multi-MB files.
USAGE_LINE=""
while IFS= read -r line; do
  if jq -e '
      (.message.usage // .usage) as $u |
      (.message.model // .model // "") as $m |
      ($u != null) and ($m != "<synthetic>") and
      ((($u.input_tokens // 0) +
        ($u.output_tokens // 0) +
        ($u.cache_creation_input_tokens // 0) +
        ($u.cache_read_input_tokens // 0)) > 0)
    ' <<<"${line}" >/dev/null 2>&1; then
    USAGE_LINE="${line}"
    break
  fi
done < <(tac "${TRANSCRIPT}" 2>/dev/null || tail -r "${TRANSCRIPT}" 2>/dev/null)

if [[ -z "${USAGE_LINE}" ]]; then
  exit 0
fi

# 6. Extract the fields we care about. The path `.message.usage` matches
#    Claude Code's `assistant` records; `.usage` is the fallback for harnesses
#    that put it at the top level.
read -r MODEL INPUT_TOKENS OUTPUT_TOKENS CACHE_CREATION CACHE_READ < <(
  jq -r '
    (.message // .) as $m |
    (.message.usage // .usage // {}) as $u |
    [
      ($m.model // "unknown"),
      ($u.input_tokens // 0),
      ($u.output_tokens // 0),
      ($u.cache_creation_input_tokens // 0),
      ($u.cache_read_input_tokens // 0)
    ] | @tsv
  ' <<<"${USAGE_LINE}" 2>/dev/null
)

# Bail if jq produced nothing useful.
if [[ -z "${INPUT_TOKENS:-}" ]]; then
  exit 0
fi

# 7. Append a record. One line per turn — append-only, atomic on POSIX for
#    short writes.
LOG_DIR="${HOME}/.local/share/unleash"
LOG_FILE="${LOG_DIR}/token-usage.jsonl"
mkdir -p "${LOG_DIR}" 2>/dev/null || exit 0

TS="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
jq -cn \
  --arg ts "${TS}" \
  --arg cli "claude" \
  --arg session_id "${SESSION_ID}" \
  --arg model "${MODEL}" \
  --argjson input "${INPUT_TOKENS}" \
  --argjson output "${OUTPUT_TOKENS}" \
  --argjson cache_creation "${CACHE_CREATION}" \
  --argjson cache_read "${CACHE_READ}" \
  --arg method "session_tail" \
  '{ts:$ts, cli:$cli, session_id:$session_id, model:$model,
    input:$input, output:$output,
    cache_creation:$cache_creation, cache_read:$cache_read,
    method:$method}' \
  >> "${LOG_FILE}" 2>/dev/null || true

exit 0
