#!/usr/bin/env bash
# test_skillsync.sh - Smoke tests for the bundled SkillSync plugin.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(dirname "$SCRIPT_DIR")"
PLUGIN_DIR="$REPO_ROOT/plugins/bundled/skillsync"

TESTS_RUN=0
TESTS_FAILED=0

pass() {
  echo "PASS: $1"
  TESTS_RUN=$((TESTS_RUN + 1))
}

fail() {
  echo "FAIL: $1"
  TESTS_RUN=$((TESTS_RUN + 1))
  TESTS_FAILED=$((TESTS_FAILED + 1))
}

if python3 -c "import json; json.load(open('$PLUGIN_DIR/.claude-plugin/plugin.json'))"; then
  pass "plugin manifest parses"
else
  fail "plugin manifest parses"
fi

if python3 -c "import json; json.load(open('$PLUGIN_DIR/hooks/hooks.json'))"; then
  pass "hooks manifest parses"
else
  fail "hooks manifest parses"
fi

for script in "$PLUGIN_DIR/scripts/check-enabled.sh" "$PLUGIN_DIR/hooks-handlers/skillsync-session-start.sh"; do
  if [[ -x "$script" ]] && bash -n "$script"; then
    pass "hook script executable and syntactically valid: $script"
  else
    fail "hook script executable and syntactically valid: $script"
  fi
done

tmp_home="$(mktemp -d)"
PATH_BACKUP="$PATH"
BASH_BIN="$(command -v bash)"
PATH="/usr/bin:/bin"
if HOME="$tmp_home" PATH="$PATH" "$BASH_BIN" "$PLUGIN_DIR/hooks-handlers/skillsync-session-start.sh" <<<"{}"; then
  pass "SessionStart hook survives empty JSON payload without unleash on PATH"
else
  fail "SessionStart hook survives empty JSON payload without unleash on PATH"
fi
PATH="$PATH_BACKUP"
rm -rf "$tmp_home"

if [[ -s "$PLUGIN_DIR/commands/skillsync.md" ]]; then
  pass "slash command exists"
else
  fail "slash command exists"
fi

echo "SkillSync smoke tests: $((TESTS_RUN - TESTS_FAILED))/$TESTS_RUN passed"
if [[ "$TESTS_FAILED" -gt 0 ]]; then
  exit 1
fi
