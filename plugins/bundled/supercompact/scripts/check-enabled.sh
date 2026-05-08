#!/usr/bin/env bash
# check-enabled.sh — exit 0 if the named plugin is enabled in the unleash config,
# 1 otherwise. Used by hook scripts as a self-disable guard against stale
# registrations in ~/.claude/settings.json that the wrapper failed to prune.
#
# Delegates to `unleash config is-plugin-enabled <name>` so the TOML parsing
# lives in Rust (where it is type-checked) instead of fragile awk/grep.

set -uo pipefail

PLUGIN_NAME="${1:-supercompact}"

# If unleash is not on PATH (e.g. plugin running outside the wrapper), fail
# safe: treat as enabled. The hook will run; worst case is a no-op.
if ! command -v unleash >/dev/null 2>&1; then
  exit 0
fi

# Hooks inherit AGENT_CMD/AGENT_UNLEASH from the wrapped agent. Clear them so
# this helper cannot be mistaken for a wrapper reentry and relaunch the agent.
env -u AGENT_CMD -u AGENT_UNLEASH unleash config is-plugin-enabled "${PLUGIN_NAME}"
