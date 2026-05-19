#!/usr/bin/env bash
# report.sh — Aggregate the token-usage log and (optionally) scan existing
# Claude session files. Honors the per-method toggles in
# ~/.config/unleash/plugins/token-usage/settings.env.
#
# Usage:
#   report.sh                # default: human-readable table
#   report.sh --json         # machine-readable summary
#   report.sh --since 7d     # only records from the last 7 days
#   report.sh --by model     # group by model instead of by CLI
#
# Exits 0 on success even if no data was collected, so it can be safely
# invoked from slash commands and shell pipelines.

set -uo pipefail

LOG_FILE="${HOME}/.local/share/unleash/token-usage.jsonl"
SETTINGS_FILE="${HOME}/.config/unleash/plugins/token-usage/settings.env"

# ─── Settings defaults (match plugin.json) ──────────────────────────────────
PLUGIN_SETTING_METHOD_SESSION_TAIL="true"
PLUGIN_SETTING_METHOD_SESSION_SCAN="true"
PLUGIN_SETTING_METHOD_PROVIDER_API="false"
PLUGIN_SETTING_ESTIMATE_COST_USD="true"
PLUGIN_SETTING_DATA_RETENTION_DAYS="90"
if [[ -r "${SETTINGS_FILE}" ]]; then
  # shellcheck disable=SC1090
  source "${SETTINGS_FILE}"
fi

# ─── Args ───────────────────────────────────────────────────────────────────
FORMAT="table"
SINCE=""
GROUP_BY="cli"
while [[ $# -gt 0 ]]; do
  case "$1" in
    --json) FORMAT="json"; shift ;;
    --since) SINCE="$2"; shift 2 ;;
    --by) GROUP_BY="$2"; shift 2 ;;
    -h|--help)
      sed -n '2,16p' "$0"
      exit 0
      ;;
    *) shift ;;
  esac
done

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi

# ─── Compute cutoff timestamp for --since ──────────────────────────────────
CUTOFF_TS=""
if [[ -n "${SINCE}" ]]; then
  # Accept Nh, Nd, Nw (hours, days, weeks). Falls back to whatever GNU date
  # understands (e.g. "2026-05-10").
  case "${SINCE}" in
    *h) CUTOFF_TS=$(date -u -d "${SINCE%h} hours ago" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null) ;;
    *d) CUTOFF_TS=$(date -u -d "${SINCE%d} days ago"  +%Y-%m-%dT%H:%M:%SZ 2>/dev/null) ;;
    *w) CUTOFF_TS=$(date -u -d "${SINCE%w} weeks ago" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null) ;;
    *)  CUTOFF_TS=$(date -u -d "${SINCE}"             +%Y-%m-%dT%H:%M:%SZ 2>/dev/null) ;;
  esac
fi

# ─── Retention prune (silent) ───────────────────────────────────────────────
# Drop records older than retention_days. Done before scanning so the summary
# matches the pruned data on disk.
if [[ -f "${LOG_FILE}" ]] && [[ "${PLUGIN_SETTING_DATA_RETENTION_DAYS}" =~ ^[0-9]+$ ]]; then
  PRUNE_CUTOFF=$(date -u -d "${PLUGIN_SETTING_DATA_RETENTION_DAYS} days ago" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null)
  if [[ -n "${PRUNE_CUTOFF}" ]]; then
    TMP="$(mktemp)"
    if jq -c --arg cutoff "${PRUNE_CUTOFF}" 'select(.ts >= $cutoff)' "${LOG_FILE}" > "${TMP}" 2>/dev/null; then
      mv "${TMP}" "${LOG_FILE}"
    else
      rm -f "${TMP}"
    fi
  fi
fi

# ─── Source 1: live hook log (method_session_tail) ─────────────────────────
RECORDS_TMP="$(mktemp)"
trap 'rm -f "${RECORDS_TMP}"' EXIT

if [[ "${PLUGIN_SETTING_METHOD_SESSION_TAIL,,}" == "true" ]] && [[ -f "${LOG_FILE}" ]]; then
  if [[ -n "${CUTOFF_TS}" ]]; then
    jq -c --arg cutoff "${CUTOFF_TS}" 'select(.ts >= $cutoff)' "${LOG_FILE}" >> "${RECORDS_TMP}" 2>/dev/null || true
  else
    cat "${LOG_FILE}" >> "${RECORDS_TMP}"
  fi
fi

# ─── Source 2: on-demand session scan (method_session_scan) ────────────────
# Walks every Claude session JSONL, dedupes by message id, and adds records
# that aren't already in the hook log. Other CLIs are stubbed — they each have
# different session schemas (Codex uses OpenAI-style prompt_tokens, Gemini
# uses usageMetadata.{promptTokenCount,candidatesTokenCount}). Extending this
# is a tracked follow-up.
if [[ "${PLUGIN_SETTING_METHOD_SESSION_SCAN,,}" == "true" ]]; then
  CLAUDE_DIR="${HOME}/.claude/projects"
  if [[ -d "${CLAUDE_DIR}" ]]; then
    while IFS= read -r -d '' f; do
      # Pull every assistant message with a usage block. Timestamp comes from
      # the record itself when present, else the file mtime.
      MTIME_TS=$(date -u -r "${f}" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo "")
      jq -c --arg fallback_ts "${MTIME_TS}" --arg method "session_scan" '
        # Skip Claude Code synthetic placeholder messages and zero-usage rows.
        (.message.usage // .usage) as $u |
        (.message.model // .model // "") as $m |
        select($u != null and $m != "<synthetic>") |
        {
          ts: (.timestamp // $fallback_ts),
          cli: "claude",
          session_id: (.sessionId // .session_id // ""),
          model: ($m | if . == "" then "unknown" else . end),
          input: ($u.input_tokens // 0),
          output: ($u.output_tokens // 0),
          cache_creation: ($u.cache_creation_input_tokens // 0),
          cache_read: ($u.cache_read_input_tokens // 0)
        } |
        select((.input + .output + .cache_creation + .cache_read) > 0) |
        . + {method: $method}
      ' "${f}" 2>/dev/null >> "${RECORDS_TMP}" || true
    done < <(find "${CLAUDE_DIR}" -name '*.jsonl' -type f -print0 2>/dev/null)
  fi
fi

# ─── Deduplicate: prefer hook records over scan when both saw the same turn.
# Key is (session_id, ts, model, input, output). The hook records get sorted
# first so unique-key behavior keeps them.
DEDUPED="$(mktemp)"
trap 'rm -f "${RECORDS_TMP}" "${DEDUPED}"' EXIT

if [[ -n "${CUTOFF_TS}" ]]; then
  jq -c --arg cutoff "${CUTOFF_TS}" 'select(.ts >= $cutoff)' "${RECORDS_TMP}" 2>/dev/null \
    | jq -s -c 'sort_by(.method) | unique_by([.session_id, .ts, .model, .input, .output]) | .[]' \
    > "${DEDUPED}" 2>/dev/null || true
else
  jq -s -c 'sort_by(.method) | unique_by([.session_id, .ts, .model, .input, .output]) | .[]' \
    "${RECORDS_TMP}" > "${DEDUPED}" 2>/dev/null || true
fi

if [[ ! -s "${DEDUPED}" ]]; then
  if [[ "${FORMAT}" == "json" ]]; then
    echo '{"records": 0, "groups": []}'
  else
    echo "No token-usage records found."
    echo
    echo "Hook collection: method_session_tail=${PLUGIN_SETTING_METHOD_SESSION_TAIL}"
    echo "Session scan  : method_session_scan=${PLUGIN_SETTING_METHOD_SESSION_SCAN}"
    echo
    echo "If both are enabled and you still see nothing, check that the plugin is"
    echo "enabled in the unleash TUI (Plugins tab) and that ~/.claude/projects"
    echo "contains session JSONL files."
  fi
  exit 0
fi

# ─── Pricing table (per-million-token rates, USD) ──────────────────────────
# Updated 2026-05-19. Approximate — use the provider-api method for billing
# truth. Keys must match the model strings emitted by each CLI.
PRICES_JSON='{
  "claude-opus-4-7":           {"input": 15.0, "output": 75.0, "cache_creation": 18.75, "cache_read": 1.5},
  "claude-opus-4-6":           {"input": 15.0, "output": 75.0, "cache_creation": 18.75, "cache_read": 1.5},
  "claude-opus-4-5":           {"input": 15.0, "output": 75.0, "cache_creation": 18.75, "cache_read": 1.5},
  "claude-sonnet-4-6":         {"input":  3.0, "output": 15.0, "cache_creation":  3.75, "cache_read": 0.3},
  "claude-sonnet-4-5":         {"input":  3.0, "output": 15.0, "cache_creation":  3.75, "cache_read": 0.3},
  "claude-haiku-4-5-20251001": {"input":  1.0, "output":  5.0, "cache_creation":  1.25, "cache_read": 0.1},
  "claude-haiku-4-5":          {"input":  1.0, "output":  5.0, "cache_creation":  1.25, "cache_read": 0.1},
  "_default":                  {"input":  3.0, "output": 15.0, "cache_creation":  3.75, "cache_read": 0.3}
}'

if [[ "${PLUGIN_SETTING_ESTIMATE_COST_USD,,}" != "true" ]]; then
  PRICES_JSON='{}'
fi

# ─── Group + summarize ─────────────────────────────────────────────────────
SUMMARY=$(
  jq -s --arg group "${GROUP_BY}" --argjson prices "${PRICES_JSON}" '
    def price_for(m): ($prices[m] // $prices._default // {input:0,output:0,cache_creation:0,cache_read:0});
    def cost(r): price_for(r.model) as $p |
      (r.input * $p.input + r.output * $p.output +
       r.cache_creation * $p.cache_creation + r.cache_read * $p.cache_read) / 1000000.0;

    group_by(.[$group]) | map({
      key: (.[0][$group] // "unknown"),
      records: length,
      input: (map(.input) | add),
      output: (map(.output) | add),
      cache_creation: (map(.cache_creation) | add),
      cache_read: (map(.cache_read) | add),
      cost_usd: (if ($prices | length) > 0 then (map(cost(.)) | add) else null end)
    }) | sort_by(-(.input + .output + .cache_creation + .cache_read))
  ' "${DEDUPED}"
)

TOTAL=$(jq -s --argjson prices "${PRICES_JSON}" '
  def price_for(m): ($prices[m] // $prices._default // {input:0,output:0,cache_creation:0,cache_read:0});
  def cost(r): price_for(r.model) as $p |
    (r.input * $p.input + r.output * $p.output +
     r.cache_creation * $p.cache_creation + r.cache_read * $p.cache_read) / 1000000.0;
  {
    records: length,
    input: (map(.input) | add),
    output: (map(.output) | add),
    cache_creation: (map(.cache_creation) | add),
    cache_read: (map(.cache_read) | add),
    cost_usd: (if ($prices | length) > 0 then (map(cost(.)) | add) else null end)
  }
' "${DEDUPED}")

# ─── Output ────────────────────────────────────────────────────────────────
if [[ "${FORMAT}" == "json" ]]; then
  jq -n --argjson groups "${SUMMARY}" --argjson total "${TOTAL}" --arg group_by "${GROUP_BY}" --arg since "${SINCE}" \
    '{group_by: $group_by, since: $since, total: $total, groups: $groups}'
  exit 0
fi

# Pretty table
echo "Token usage report"
[[ -n "${SINCE}" ]] && echo "Since : ${SINCE} (UTC cutoff ${CUTOFF_TS})"
echo "Group : ${GROUP_BY}"
echo
printf "%-30s %10s %12s %12s %12s %12s" \
  "${GROUP_BY^^}" "RECORDS" "INPUT" "OUTPUT" "CACHE_CR" "CACHE_RD"
if [[ "${PLUGIN_SETTING_ESTIMATE_COST_USD,,}" == "true" ]]; then
  printf " %10s" "COST_USD"
fi
echo
printf '%.0s─' {1..110}; echo

jq -r --argjson with_cost \
  "$([[ "${PLUGIN_SETTING_ESTIMATE_COST_USD,,}" == "true" ]] && echo true || echo false)" '
  .[] |
  if $with_cost then
    [.key, .records, .input, .output, .cache_creation, .cache_read, (.cost_usd | tonumber | . * 100 | round / 100)]
  else
    [.key, .records, .input, .output, .cache_creation, .cache_read]
  end
  | @tsv
' <<<"${SUMMARY}" | while IFS=$'\t' read -r key recs inp outp cc cr cost; do
  if [[ "${PLUGIN_SETTING_ESTIMATE_COST_USD,,}" == "true" ]]; then
    printf "%-30s %10d %12d %12d %12d %12d %10s\n" \
      "${key:0:30}" "${recs}" "${inp}" "${outp}" "${cc}" "${cr}" "\$${cost}"
  else
    printf "%-30s %10d %12d %12d %12d %12d\n" \
      "${key:0:30}" "${recs}" "${inp}" "${outp}" "${cc}" "${cr}"
  fi
done

printf '%.0s─' {1..110}; echo
TOTAL_RECS=$(jq -r '.records'        <<<"${TOTAL}")
TOTAL_IN=$(jq -r '.input'             <<<"${TOTAL}")
TOTAL_OUT=$(jq -r '.output'           <<<"${TOTAL}")
TOTAL_CC=$(jq -r '.cache_creation'    <<<"${TOTAL}")
TOTAL_CR=$(jq -r '.cache_read'        <<<"${TOTAL}")
if [[ "${PLUGIN_SETTING_ESTIMATE_COST_USD,,}" == "true" ]]; then
  TOTAL_COST=$(jq -r '.cost_usd | . * 100 | round / 100' <<<"${TOTAL}")
  printf "%-30s %10d %12d %12d %12d %12d %10s\n" \
    "TOTAL" "${TOTAL_RECS}" "${TOTAL_IN}" "${TOTAL_OUT}" "${TOTAL_CC}" "${TOTAL_CR}" "\$${TOTAL_COST}"
else
  printf "%-30s %10d %12d %12d %12d %12d\n" \
    "TOTAL" "${TOTAL_RECS}" "${TOTAL_IN}" "${TOTAL_OUT}" "${TOTAL_CC}" "${TOTAL_CR}"
fi

if [[ "${PLUGIN_SETTING_METHOD_PROVIDER_API,,}" == "true" ]]; then
  echo
  echo "(method_provider_api enabled but not yet implemented — pending follow-up)"
fi
