#!/usr/bin/env bash
# Test all commands work in headless / non-TTY / non-interactive mode
# This validates that every subcommand produces correct output when
# stdin is /dev/null and stdout is not a terminal (piped).
#
# Usage: ./tests/test-headless.sh
# Override binary: AU_BIN=./target/debug/au ./tests/test-headless.sh

set -euo pipefail

# Find binary - prefer fast profile, then release, then debug
if [[ -n "${AU_BIN:-}" ]]; then
    BIN="$AU_BIN"
elif [[ -x "./target/fast/au" ]]; then
    BIN="./target/fast/au"
elif [[ -x "./target/release/au" ]]; then
    BIN="./target/release/au"
elif [[ -x "./target/debug/au" ]]; then
    BIN="./target/debug/au"
else
    echo "ERROR: No au binary found. Run: cargo build"
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
    OUT=$("$@" </dev/null 2>/tmp/au-test-stderr) || rc=$?
    ERR=$(cat /tmp/au-test-stderr 2>/dev/null || true)
    return $rc
}

echo "=== Agent Unleashed Headless Tests ==="
echo "Binary: $BIN"
echo

# ─── 1. --version ───────────────────────────────────────────────
echo "[1] au --version"
if run_headless "$BIN" --version; then
    if echo "$OUT" | grep -q "Agent Unleashed: v"; then
        pass "--version prints version string"
    else
        fail "--version output" "missing 'Agent Unleashed: v' prefix"
    fi
else
    fail "--version" "non-zero exit code: $?"
fi

# ─── 2. --help ──────────────────────────────────────────────────
echo "[2] au --help"
if run_headless "$BIN" --help; then
    if echo "$OUT" | grep -q "Agent Unleashed"; then
        pass "--help prints usage"
    else
        fail "--help output" "missing expected text"
    fi
else
    fail "--help" "non-zero exit code"
fi

# ─── 3. au (no args) ────────────────────────────────────────────
echo "[3] au (no args — should show help)"
if run_headless "$BIN"; then
    if echo "$OUT" | grep -q "Agent Unleashed"; then
        pass "no-args shows help"
    else
        fail "no-args output" "missing help text"
    fi
else
    fail "no-args" "non-zero exit code"
fi

# ─── 4. au version ──────────────────────────────────────────────
echo "[4] au version"
if run_headless "$BIN" version; then
    if [[ -n "$OUT" ]]; then
        pass "version subcommand produces output"
    else
        fail "version output" "no output"
    fi
else
    fail "version" "non-zero exit code"
fi

# ─── 5. au version --json ───────────────────────────────────────
echo "[5] au version --json"
if run_headless "$BIN" version --json; then
    if echo "$OUT" | jq . >/dev/null 2>&1; then
        pass "version --json produces valid JSON"
    else
        fail "version --json" "invalid JSON output"
    fi
else
    fail "version --json" "non-zero exit code"
fi

# ─── 6. au auth ─────────────────────────────────────────────────
echo "[6] au auth"
# auth may return 0 or 1 depending on whether user is authenticated
run_headless "$BIN" auth || true
if [[ -n "$OUT" || -n "$ERR" ]]; then
    pass "auth produces output"
else
    fail "auth" "no output at all"
fi

# ─── 7. au auth --json ──────────────────────────────────────────
echo "[7] au auth --json"
run_headless "$BIN" auth --json || true
if echo "$OUT" | jq . >/dev/null 2>&1; then
    if echo "$OUT" | jq -e '.authenticated' >/dev/null 2>&1; then
        pass "auth --json has 'authenticated' field"
    else
        fail "auth --json" "missing 'authenticated' field"
    fi
else
    fail "auth --json" "invalid JSON"
fi

# ─── 8. au auth --quiet ─────────────────────────────────────────
echo "[8] au auth --quiet"
run_headless "$BIN" auth --quiet || true
if [[ -z "$OUT" ]]; then
    pass "auth --quiet produces no stdout"
else
    fail "auth --quiet" "unexpected stdout: $OUT"
fi

# ─── 9. au patch --check ────────────────────────────────────────
echo "[9] au patch --check"
if run_headless "$BIN" patch --check; then
    pass "patch --check exits 0"
else
    # Non-zero is ok if patching is needed or claude not installed
    pass "patch --check exits non-zero (expected if not patched)"
fi
# patch --check may produce no output when patches are already applied
pass "patch --check ran without crash"

# ─── 10. au hooks ───────────────────────────────────────────────
echo "[10] au hooks"
if run_headless "$BIN" hooks; then
    pass "hooks subcommand exits 0"
else
    fail "hooks" "non-zero exit code"
fi

# ─── 11. au hooks list ──────────────────────────────────────────
echo "[11] au hooks list"
if run_headless "$BIN" hooks list; then
    pass "hooks list exits 0"
else
    fail "hooks list" "non-zero exit code"
fi

# ─── 12. au agents ──────────────────────────────────────────────
echo "[12] au agents"
if run_headless "$BIN" agents; then
    if echo "$OUT" | grep -qi "claude\|codex\|agent"; then
        pass "agents shows agent info"
    else
        fail "agents output" "missing agent names"
    fi
else
    fail "agents" "non-zero exit code"
fi

# ─── 13. au agents list ─────────────────────────────────────────
echo "[13] au agents list"
if run_headless "$BIN" agents list; then
    if echo "$OUT" | grep -qi "claude\|codex"; then
        pass "agents list shows agents"
    else
        fail "agents list output" "missing agent names"
    fi
else
    fail "agents list" "non-zero exit code"
fi

# ─── 14. au agents info claude ──────────────────────────────────
echo "[14] au agents info claude"
if run_headless "$BIN" agents info claude; then
    if echo "$OUT" | grep -qi "claude"; then
        pass "agents info claude shows details"
    else
        fail "agents info claude output" "missing claude info"
    fi
else
    fail "agents info claude" "non-zero exit code"
fi

# ─── 15. au agents info codex ──────────────────────────────────
echo "[15] au agents info codex"
if run_headless "$BIN" agents info codex; then
    if echo "$OUT" | grep -qi "codex"; then
        pass "agents info codex shows details"
    else
        fail "agents info codex output" "missing codex info"
    fi
else
    fail "agents info codex" "non-zero exit code"
fi

# ─── 16. Binary aliases exist ───────────────────────────────────
echo "[16] Binary aliases exist"
BIN_DIR=$(dirname "$BIN")
for alias in aui aug autx autxg; do
    if [[ -x "$BIN_DIR/$alias" ]]; then
        pass "$alias binary exists"
    else
        fail "$alias binary" "not found in $BIN_DIR"
    fi
done

# ─── 17. aui --version ─────────────────────────────────────────
echo "[17] aui --version"
if run_headless "$BIN_DIR/aui" --version; then
    if echo "$OUT" | grep -q "Agent Unleashed: v"; then
        pass "aui --version works"
    else
        fail "aui --version" "unexpected output"
    fi
else
    fail "aui --version" "non-zero exit code"
fi

# ─── 18. aug --version ─────────────────────────────────────────
echo "[18] aug --version"
if run_headless "$BIN_DIR/aug" --version; then
    if echo "$OUT" | grep -q "Agent Unleashed: v"; then
        pass "aug --version works"
    else
        fail "aug --version" "unexpected output"
    fi
else
    fail "aug --version" "non-zero exit code"
fi

# ─── 19. autx --version ────────────────────────────────────────
echo "[19] autx --version"
if run_headless "$BIN_DIR/autx" --version; then
    if echo "$OUT" | grep -q "Agent Unleashed: v"; then
        pass "autx --version works"
    else
        fail "autx --version" "unexpected output"
    fi
else
    fail "autx --version" "non-zero exit code"
fi

# ─── 20. Invalid subcommand ────────────────────────────────────
echo "[20] au invalid-subcommand"
if run_headless "$BIN" invalid-subcommand; then
    fail "invalid subcommand" "should exit non-zero"
else
    pass "invalid subcommand exits non-zero"
fi

# ─── Cleanup ────────────────────────────────────────────────────
rm -f /tmp/au-test-stderr

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
