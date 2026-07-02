#!/usr/bin/env bash
# check-enabled.sh — exit 0 if the named plugin is enabled in unleash config.

set -uo pipefail

PLUGIN_NAME="${1:-skillsync}"

if ! command -v unleash >/dev/null 2>&1; then
  exit 0
fi

env -u AGENT_CMD -u AGENT_UNLEASH unleash config is-plugin-enabled "${PLUGIN_NAME}"
