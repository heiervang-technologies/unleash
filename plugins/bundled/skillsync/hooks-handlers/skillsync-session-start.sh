#!/usr/bin/env bash
# SessionStart hook: synchronize skills when the plugin setting allows it.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CHECK_ENABLED="${SCRIPT_DIR}/../scripts/check-enabled.sh"

if [[ -x "${CHECK_ENABLED}" ]] && ! "${CHECK_ENABLED}" skillsync; then
  exit 0
fi

SETTINGS_FILE="${HOME}/.config/unleash/plugins/skillsync/settings.env"
PLUGIN_SETTING_SYNC_ON_LAUNCH="on"
PLUGIN_SETTING_SOURCE="claude"

if [[ -r "${SETTINGS_FILE}" ]]; then
  # shellcheck disable=SC1090
  source "${SETTINGS_FILE}"
fi

if [[ "${PLUGIN_SETTING_SYNC_ON_LAUNCH,,}" != "on" ]]; then
  exit 0
fi

if ! command -v unleash >/dev/null 2>&1; then
  exit 0
fi

env -u AGENT_CMD -u AGENT_UNLEASH unleash skills sync --from "${PLUGIN_SETTING_SOURCE:-claude}" >/dev/null 2>&1 || true
exit 0
