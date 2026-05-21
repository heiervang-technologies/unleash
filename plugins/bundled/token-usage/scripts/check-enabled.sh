#!/usr/bin/env bash
# check-enabled.sh — exit 0 if the named plugin is enabled in the unleash
# config, 1 otherwise. Same self-disable guard pattern used by supercompact —
# protects against stale Claude settings.json registrations the wrapper failed
# to prune.

set -uo pipefail

PLUGIN_NAME="${1:-token-usage}"

# Outside the wrapper (unleash not on PATH) fail-safe: treat as enabled. The
# hook will run; worst case is a no-op log line.
if ! command -v unleash >/dev/null 2>&1; then
  exit 0
fi

# Hooks inherit AGENT_CMD/AGENT_UNLEASH from the wrapped agent. Strip them so
# the helper cannot be mistaken for a wrapper reentry and relaunch the agent.
env -u AGENT_CMD -u AGENT_UNLEASH unleash config is-plugin-enabled "${PLUGIN_NAME}"
