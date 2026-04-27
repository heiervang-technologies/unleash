#!/usr/bin/env bash
# set-budget.sh — Persist supercompact configuration for future sessions.
#
# Two distinct knobs:
#   - THRESHOLD: token count at which preemptive auto-compaction TRIGGERS.
#     Should be near the model's context window so it fires just before
#     the native API auto-compact would. Default: 180000 (90% of 200k).
#   - BUDGET: target token count compaction COMPRESSES TO.
#     Should be much smaller than THRESHOLD to leave headroom for new work.
#     Default: 50000 (or auto = percentage of current size).
#
# These MUST be different — THRESHOLD > BUDGET, with healthy headroom
# (we enforce THRESHOLD >= BUDGET + 30000).
#
# Writes key=value lines to ~/.config/unleash/plugins/supercompact/settings.env,
# which the compaction pipeline sources before applying its own defaults.
#
# Usage (invoked from the /supercompact-budget slash command):
#   set-budget.sh                       — show current effective settings
#   set-budget.sh <N>                   — set BUDGET (compression target)
#   set-budget.sh budget <N>            — set BUDGET (alias)
#   set-budget.sh auto                  — switch BUDGET to auto (% of current)
#   set-budget.sh threshold <N>         — set THRESHOLD (auto-compact trigger)
#   set-budget.sh threshold default     — restore default THRESHOLD (180000)
#   set-budget.sh floor <N>             — set BUDGET_FLOOR
#   set-budget.sh ceiling <N>           — set BUDGET_CEILING
#   set-budget.sh reset                 — clear all overrides

set -uo pipefail

DEFAULT_THRESHOLD=180000
MIN_HEADROOM=30000

SETTINGS_DIR="${HOME}/.config/unleash/plugins/supercompact"
SETTINGS_FILE="${SETTINGS_DIR}/settings.env"
mkdir -p "${SETTINGS_DIR}" 2>/dev/null || true
touch "${SETTINGS_FILE}" 2>/dev/null || true

set_kv() {
  local key="$1" val="$2"
  if grep -q "^${key}=" "${SETTINGS_FILE}" 2>/dev/null; then
    sed -i "s|^${key}=.*|${key}=${val}|" "${SETTINGS_FILE}"
  else
    echo "${key}=${val}" >> "${SETTINGS_FILE}"
  fi
}

unset_kv() {
  sed -i "/^${1}=/d" "${SETTINGS_FILE}" 2>/dev/null || true
}

read_kv() {
  grep "^${1}=" "${SETTINGS_FILE}" 2>/dev/null | tail -1 | cut -d= -f2-
}

show() {
  echo "Supercompact settings (${SETTINGS_FILE}):"
  if [[ -s "${SETTINGS_FILE}" ]]; then
    sed 's/^/  /' "${SETTINGS_FILE}"
  else
    echo "  (no overrides — using plugin defaults)"
  fi
  echo
  echo "Effective:"
  local mode budget threshold
  mode=$(read_kv PLUGIN_SETTING_BUDGET_MODE)
  budget=$(read_kv PLUGIN_SETTING_BUDGET)
  threshold=$(read_kv PLUGIN_SETTING_THRESHOLD_TOKENS)
  echo "  THRESHOLD (auto-compact trigger) : ${threshold:-${DEFAULT_THRESHOLD} (default)}"
  echo "  BUDGET    (compression target)   : ${budget:-auto} (mode=${mode:-auto})"
}

# Warn (don't fail) when threshold and budget have insufficient headroom.
check_headroom() {
  local threshold budget
  threshold=$(read_kv PLUGIN_SETTING_THRESHOLD_TOKENS)
  threshold="${threshold:-${DEFAULT_THRESHOLD}}"
  budget=$(read_kv PLUGIN_SETTING_BUDGET)
  [[ -z "${budget}" ]] && return 0
  if (( threshold < budget + MIN_HEADROOM )); then
    echo "warning: THRESHOLD (${threshold}) should be at least ${MIN_HEADROOM} above BUDGET (${budget})." >&2
    echo "         Otherwise compaction will re-trigger immediately after running." >&2
  fi
}

die() { echo "error: $*" >&2; exit 1; }
is_int() { [[ "$1" =~ ^[0-9]+$ ]]; }

cmd="${1:-show}"

case "${cmd}" in
  show|"")
    show
    ;;
  auto)
    set_kv PLUGIN_SETTING_BUDGET_MODE auto
    unset_kv PLUGIN_SETTING_BUDGET
    echo "Switched BUDGET to auto (percentage-of-current) mode."
    show
    ;;
  reset)
    : > "${SETTINGS_FILE}"
    echo "Cleared all supercompact overrides."
    show
    ;;
  floor)
    val="${2:-}"; is_int "${val}" || die "floor requires a positive integer (got: '${val}')"
    set_kv PLUGIN_SETTING_BUDGET_FLOOR "${val}"
    echo "Budget floor set to ${val} tokens."
    show
    ;;
  ceiling)
    val="${2:-}"; is_int "${val}" || die "ceiling requires a positive integer (got: '${val}')"
    set_kv PLUGIN_SETTING_BUDGET_CEILING "${val}"
    echo "Budget ceiling set to ${val} tokens."
    show
    ;;
  threshold)
    val="${2:-}"
    if [[ "${val}" == "default" || -z "${val}" && "${cmd}" == "threshold" ]]; then
      unset_kv PLUGIN_SETTING_THRESHOLD_TOKENS
      echo "THRESHOLD reset to default (${DEFAULT_THRESHOLD} tokens)."
    elif is_int "${val}"; then
      set_kv PLUGIN_SETTING_THRESHOLD_TOKENS "${val}"
      echo "THRESHOLD set to ${val} tokens (auto-compaction triggers above this)."
      check_headroom
    else
      die "threshold requires a positive integer or 'default' (got: '${val}')"
    fi
    show
    ;;
  budget)
    val="${2:-}"
    if [[ "${val}" == "auto" ]]; then
      set_kv PLUGIN_SETTING_BUDGET_MODE auto
      unset_kv PLUGIN_SETTING_BUDGET
      echo "Switched BUDGET to auto (percentage-of-current) mode."
    elif is_int "${val}"; then
      set_kv PLUGIN_SETTING_BUDGET_MODE manual
      set_kv PLUGIN_SETTING_BUDGET "${val}"
      echo "Manual BUDGET set to ${val} tokens (compression target, mode=manual)."
      check_headroom
    else
      die "budget requires a positive integer or 'auto' (got: '${val}')"
    fi
    show
    ;;
  *)
    if is_int "${cmd}"; then
      set_kv PLUGIN_SETTING_BUDGET_MODE manual
      set_kv PLUGIN_SETTING_BUDGET "${cmd}"
      echo "Manual BUDGET set to ${cmd} tokens (compression target, mode=manual)."
      check_headroom
      show
    else
      die "unknown argument '${cmd}'. Try: <N> | budget <N> | budget auto | threshold <N> | threshold default | floor <N> | ceiling <N> | reset | show"
    fi
    ;;
esac
