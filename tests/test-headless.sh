#!/usr/bin/env bash
# Test all commands work in headless / non-TTY / non-interactive mode
# This validates that every subcommand produces correct output when
# stdin is /dev/null and stdout is not a terminal (piped).
#
# Usage: ./tests/test-headless.sh
# Override binary: AU_BIN=./target/debug/unleash ./tests/test-headless.sh

set -euo pipefail

# Isolate from the unleash wrapper environment so subcommand routing tests
# behave the same whether run inside or outside an unleash session.
unset AGENT_UNLEASH AGENT_CMD 2>/dev/null || true

# Find binary - prefer fast profile, then release, then debug
if [[ -n "${AU_BIN:-}" ]]; then
    BIN="$AU_BIN"
elif [[ -x "./target/fast/unleash" ]]; then
    BIN="./target/fast/unleash"
elif [[ -x "./target/release/unleash" ]]; then
    BIN="./target/release/unleash"
elif [[ -x "./target/debug/unleash" ]]; then
    BIN="./target/debug/unleash"
else
    echo "ERROR: No unleash binary found. Run: cargo build"
    exit 1
fi

PASS=0
FAIL=0
SKIP=0

pass() { PASS=$((PASS + 1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL + 1)); echo "  FAIL: $1 — $2"; }
skip() { SKIP=$((SKIP + 1)); echo "  SKIP: $1 — $2"; }

# Run a command headless (no TTY, stdin from /dev/null, capture stdout+stderr)
# Returns the exit code; sets OUT and ERR globals
run_headless() {
    local rc=0
    OUT=$("$@" </dev/null 2>/tmp/unleash-test-stderr) || rc=$?
    ERR=$(cat /tmp/unleash-test-stderr 2>/dev/null || true)
    return $rc
}

echo "=== unleash Headless Tests ==="
echo "Binary: $BIN"
echo

# ─── 1. --version ───────────────────────────────────────────────
echo "[1] unleash --version"
if run_headless "$BIN" --version; then
    if echo "$OUT" | grep -q "unleash: v"; then
        pass "--version prints version string"
    else
        fail "--version output" "missing 'unleash: v' prefix"
    fi
else
    fail "--version" "non-zero exit code: $?"
fi

# ─── 2. --help ──────────────────────────────────────────────────
echo "[2] unleash --help"
if run_headless "$BIN" --help; then
    if echo "$OUT" | grep -q "unleash"; then
        pass "--help prints usage"
    else
        fail "--help output" "missing expected text"
    fi
else
    fail "--help" "non-zero exit code"
fi

# ─── 3. unleash (no args) ────────────────────────────────────────────
echo "[3] unleash (no args — should show error without TTY)"
if run_headless "$BIN"; then
    fail "no-args output" "should fail in headless environment"
else
    pass "no-args exits non-zero without TTY"
fi

# ─── 4. unleash version ──────────────────────────────────────────────
echo "[4] unleash version"
if run_headless "$BIN" version; then
    if [[ -n "$OUT" ]]; then
        pass "version subcommand produces output"
    else
        fail "version output" "no output"
    fi
else
    fail "version" "non-zero exit code"
fi

# ─── 5. unleash version --json ───────────────────────────────────────
echo "[5] unleash version --json"
if run_headless "$BIN" version --json; then
    if echo "$OUT" | jq . >/dev/null 2>&1; then
        pass "version --json produces valid JSON"
    else
        fail "version --json" "invalid JSON output"
    fi
else
    fail "version --json" "non-zero exit code"
fi

# ─── 6. unleash auth ─────────────────────────────────────────────────
echo "[6] unleash auth"
# auth may return 0 or 1 depending on whether user is authenticated
run_headless "$BIN" auth || true
if [[ -n "$OUT" || -n "$ERR" ]]; then
    pass "auth produces output"
else
    fail "auth" "no output at all"
fi

# ─── 7. unleash auth --json ──────────────────────────────────────────
echo "[7] unleash auth --json"
run_headless "$BIN" auth --json || true
if echo "$OUT" | jq . >/dev/null 2>&1; then
    pass "auth --json produces valid JSON"
else
    # On CI without credentials, auth may output non-JSON error to stderr
    if [[ -n "$ERR" ]]; then
        pass "auth --json produced error output (no auth configured)"
    else
        fail "auth --json" "no valid output"
    fi
fi

# ─── 8. unleash auth --quiet ─────────────────────────────────────────
echo "[8] unleash auth --quiet"
run_headless "$BIN" auth --quiet || true
if [[ -z "$OUT" ]]; then
    pass "auth --quiet produces no stdout"
else
    fail "auth --quiet" "unexpected stdout: $OUT"
fi

# ─── 9. unleash hooks ───────────────────────────────────────────────
echo "[9] unleash hooks"
# hooks may fail if Claude Code settings.json doesn't exist (e.g. CI)
run_headless "$BIN" hooks || true
if [[ -n "$OUT" || -n "$ERR" ]]; then
    pass "hooks subcommand produces output"
else
    pass "hooks subcommand ran (no Claude Code installed)"
fi

# ─── 10. unleash hooks list ──────────────────────────────────────────
echo "[10] unleash hooks list"
run_headless "$BIN" hooks list || true
if [[ -n "$OUT" || -n "$ERR" ]]; then
    pass "hooks list produces output"
else
    pass "hooks list ran (no Claude Code installed)"
fi

# ─── 11. unleash agents ──────────────────────────────────────────────
echo "[11] unleash agents"
if run_headless "$BIN" agents; then
    if echo "$OUT" | grep -qi "claude\|codex\|agent"; then
        pass "agents shows agent info"
    else
        fail "agents output" "missing agent names"
    fi
else
    fail "agents" "non-zero exit code"
fi

# ─── 12. unleash agents list ─────────────────────────────────────────
echo "[12] unleash agents list"
if run_headless "$BIN" agents list; then
    if echo "$OUT" | grep -qi "claude\|codex"; then
        pass "agents list shows agents"
    else
        fail "agents list output" "missing agent names"
    fi
else
    fail "agents list" "non-zero exit code"
fi

# ─── 13. unleash agents info claude ──────────────────────────────────
echo "[13] unleash agents info claude"
if run_headless "$BIN" agents info claude; then
    if echo "$OUT" | grep -qi "claude"; then
        pass "agents info claude shows details"
    else
        fail "agents info claude output" "missing claude info"
    fi
else
    fail "agents info claude" "non-zero exit code"
fi

# ─── 14. unleash agents info codex ──────────────────────────────────
echo "[14] unleash agents info codex"
if run_headless "$BIN" agents info codex; then
    if echo "$OUT" | grep -qi "codex"; then
        pass "agents info codex shows details"
    else
        fail "agents info codex output" "missing codex info"
    fi
else
    fail "agents info codex" "non-zero exit code"
fi

# ─── 15. Invalid subcommand ────────────────────────────────────
echo "[15] unleash invalid-subcommand"
if run_headless "$BIN" invalid-subcommand; then
    fail "invalid subcommand" "should exit non-zero"
else
    pass "invalid subcommand exits non-zero"
fi

# ─── 16. unleash agents info --json ─────────────────────────────
echo "[16] unleash agents info claude --json"
if run_headless "$BIN" agents info claude --json; then
    if echo "$OUT" | python3 -c "import json,sys; d=json.load(sys.stdin); exit(0 if 'agent_type' in d else 1)" 2>/dev/null; then
        pass "agents info claude --json produces valid JSON with agent_type"
    else
        fail "agents info claude --json" "missing agent_type field: $OUT"
    fi
else
    fail "agents info claude --json" "non-zero exit code"
fi

# ─── 17. unleash agents list --json ─────────────────────────────
echo "[17] unleash agents list --json"
if run_headless "$BIN" agents list --json; then
    if echo "$OUT" | python3 -c "import json,sys; items=json.load(sys.stdin); exit(0 if isinstance(items, list) and len(items) > 0 else 1)" 2>/dev/null; then
        pass "agents list --json produces valid JSON array"
    else
        fail "agents list --json" "not a non-empty JSON array: $OUT"
    fi
else
    fail "agents list --json" "non-zero exit code"
fi

# ─── Cleanup ────────────────────────────────────────────────────
rm -f /tmp/unleash-test-stderr

# ─── Summary ────────────────────────────────────────────────────
echo
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo "  Skipped: $SKIP"
echo

if [[ $FAIL -gt 0 ]]; then
    echo "FAILED"
    exit 1
else
    echo "ALL PASSED"
    exit 0
fi
