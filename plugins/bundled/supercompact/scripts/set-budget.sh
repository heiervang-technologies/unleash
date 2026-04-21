#!/usr/bin/env bash
# set-budget.sh — Persist supercompact budget defaults for future sessions.
#
# Writes key=value lines to ~/.config/unleash/plugins/supercompact/settings.env,
# which the compaction pipeline sources before applying its own defaults.
#
# Usage (invoked from the /supercompact-budget slash command):
#   set-budget.sh                 — show current effective settings
#   set-budget.sh <N>             — set manual budget to N, switch to manual mode
#   set-budget.sh auto            — switch back to auto (percentage) mode
#   set-budget.sh floor <N>       — set BUDGET_FLOOR
#   set-budget.sh ceiling <N>     — set BUDGET_CEILING
#   set-budget.sh reset           — clear all overrides

set -uo pipefail

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

show() {
  echo "Supercompact settings (${SETTINGS_FILE}):"
  if [[ -s "${SETTINGS_FILE}" ]]; then
    sed 's/^/  /' "${SETTINGS_FILE}"
  else
    echo "  (no overrides — using plugin defaults: mode=auto, floor=100000, ceiling=150000)"
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
    echo "Switched to auto (percentage-of-current) budget mode."
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
  *)
    if is_int "${cmd}"; then
      set_kv PLUGIN_SETTING_BUDGET_MODE manual
      set_kv PLUGIN_SETTING_BUDGET "${cmd}"
      echo "Manual budget set to ${cmd} tokens (mode=manual)."
      show
    else
      die "unknown argument '${cmd}'. Try: <N> | auto | floor <N> | ceiling <N> | reset | show"
    fi
    ;;
esac
