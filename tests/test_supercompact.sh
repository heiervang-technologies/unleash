#!/usr/bin/env bash
# test_supercompact.sh — Integration tests for the supercompact two-layer compaction
#
# Tests the hook scripts in isolation (without a running Claude session).
# Uses mock compact.py and mock JSONL to verify behavior.
#
# Usage: bash tests/test_supercompact.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
PLUGIN_DIR="${REPO_ROOT}/plugins/bundled/supercompact"
HOOKS_DIR="${PLUGIN_DIR}/hooks-handlers"

# Test workspace
TEST_DIR=$(mktemp -d "/tmp/test-supercompact-XXXXXX")
trap 'rm -rf "${TEST_DIR}"; rm -f /tmp/supercompact.lock /tmp/test-supercompact-refresh-called /tmp/test-supercompact-refresh-message' EXIT

PASS=0
FAIL=0

pass() { PASS=$((PASS + 1)); echo "  PASS: $1"; }
fail() { FAIL=$((FAIL + 1)); echo "  FAIL: $1"; }

# --- Setup mock environment ---

create_mock_jsonl() {
  local file="$1"
  local lines="${2:-100}"
  rm -f "$file"
  for i in $(seq 1 "$lines"); do
    echo "{\"type\":\"user\",\"message\":{\"content\":\"Message $i with some padding text to make the line realistic in size for token estimation purposes.\"},\"parentUuid\":\"uuid-$((i-1))\",\"uuid\":\"uuid-$i\"}" >> "$file"
  done
}

create_mock_compact() {
  local dir="$1"
  mkdir -p "$dir"
  cat > "$dir/compact.py" << 'PYEOF'
#!/usr/bin/env python3
"""Mock compact.py — keeps first half of lines."""
import sys, json
args = sys.argv[1:]
input_file = args[0] if args else None
output_file = None
i = 1
while i < len(args):
    if args[i] == "--output" and i + 1 < len(args):
        output_file = args[i + 1]; i += 2
    else: i += 1
if not input_file or not output_file:
    sys.exit(1)
with open(input_file) as f:
    lines = f.readlines()
kept = lines[:max(len(lines) // 2, 1)]
with open(output_file, 'w') as f:
    f.writelines(kept)
PYEOF
}

create_failing_compact() {
  local dir="$1"
  mkdir -p "$dir"
  cat > "$dir/compact.py" << 'PYEOF'
#!/usr/bin/env python3
import sys
print("ERROR: simulated failure", file=sys.stderr)
sys.exit(1)
PYEOF
}

create_mock_uv() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"
  cat > "$bin_dir/uv" << 'EOF'
#!/usr/bin/env bash
shift  # remove "run"
exec "$@"
EOF
  chmod +x "$bin_dir/uv"
}

create_mock_unleash_refresh() {
  local bin_dir="$1"
  cat > "$bin_dir/unleash-refresh" << 'EOF'
#!/usr/bin/env bash
touch /tmp/test-supercompact-refresh-called
echo "$@" > /tmp/test-supercompact-refresh-message
EOF
  chmod +x "$bin_dir/unleash-refresh"
}

# Setup shared mocks
MOCK_SC_DIR="${TEST_DIR}/mock-supercompact"
MOCK_BIN="${TEST_DIR}/mock-bin"
create_mock_compact "$MOCK_SC_DIR"
create_mock_uv "$MOCK_BIN"
create_mock_unleash_refresh "$MOCK_BIN"

echo "=== Supercompact Integration Tests ==="
echo ""

# ============================================================
# Test 1: UserPromptSubmit — below threshold exits silently
# ============================================================
echo "--- Test 1: UserPromptSubmit below threshold ---"

SMALL_JSONL="${TEST_DIR}/small.jsonl"
create_mock_jsonl "$SMALL_JSONL" 10

INPUT_JSON=$(jq -n --arg path "$SMALL_JSONL" '{transcript_path: $path}')

EXIT_CODE=0
echo "$INPUT_JSON" | SUPERCOMPACT_THRESHOLD_BYTES=999999999 \
  CLAUDE_PLUGIN_ROOT="${PLUGIN_DIR}" \
  bash "${HOOKS_DIR}/supercompact-userprompt.sh" 2>/dev/null || EXIT_CODE=$?

if [[ $EXIT_CODE -eq 0 ]]; then
  pass "Below threshold exits 0"
else
  fail "Below threshold should exit 0, got $EXIT_CODE"
fi

# ============================================================
# Test 2: UserPromptSubmit — above threshold triggers pipeline
# ============================================================
echo "--- Test 2: UserPromptSubmit above threshold ---"

BIG_JSONL="${TEST_DIR}/big.jsonl"
create_mock_jsonl "$BIG_JSONL" 5000

INPUT_JSON=$(jq -n --arg path "$BIG_JSONL" '{transcript_path: $path}')

EXIT_CODE=0
echo "$INPUT_JSON" | SUPERCOMPACT_THRESHOLD_BYTES=100 \
  CLAUDE_PLUGIN_ROOT="${PLUGIN_DIR}" \
  PLUGIN_SETTING_DIR="${TEST_DIR}/nonexistent" \
  bash "${HOOKS_DIR}/supercompact-userprompt.sh" 2>/dev/null || EXIT_CODE=$?

if [[ $EXIT_CODE -eq 0 ]]; then
  pass "Above threshold exits 0 (pipeline backgrounded)"
else
  fail "Above threshold should exit 0, got $EXIT_CODE"
fi

# ============================================================
# Test 3: Shared pipeline — successful compaction
# ============================================================
echo "--- Test 3: Shared pipeline success ---"

PIPELINE_JSONL="${TEST_DIR}/pipeline-test.jsonl"
create_mock_jsonl "$PIPELINE_JSONL" 200

rm -f /tmp/supercompact.lock /tmp/test-supercompact-refresh-called

ORIG_LINES=$(wc -l < "$PIPELINE_JSONL")

EXIT_CODE=0
PATH="${MOCK_BIN}:${PATH}" \
  PLUGIN_SETTING_DIR="${MOCK_SC_DIR}" \
  AGENT_WRAPPER_PID="999999" \
  bash "${HOOKS_DIR}/supercompact-compact.sh" \
    --jsonl "${PIPELINE_JSONL}" \
    --trigger "preemptive" 2>/dev/null || EXIT_CODE=$?

if [[ $EXIT_CODE -eq 0 ]]; then pass "Pipeline exits 0"; else fail "Pipeline should exit 0, got $EXIT_CODE"; fi

NEW_LINES=$(wc -l < "$PIPELINE_JSONL")
if (( NEW_LINES < ORIG_LINES )); then
  pass "JSONL compacted (${ORIG_LINES} -> ${NEW_LINES} lines)"
else
  fail "JSONL should be shorter (was ${ORIG_LINES}, now ${NEW_LINES})"
fi

if [[ -f "${PIPELINE_JSONL}.pre-compact-full" ]]; then
  pass "Backup file created"
else
  fail "Backup file missing"
fi

if [[ -f /tmp/test-supercompact-refresh-called ]]; then
  pass "unleash-refresh called"
else
  fail "unleash-refresh should have been called"
fi

# ============================================================
# Test 4: Pipeline — compact.py failure (graceful fallback)
# ============================================================
echo "--- Test 4: Pipeline graceful fallback ---"

FAIL_JSONL="${TEST_DIR}/fail-test.jsonl"
create_mock_jsonl "$FAIL_JSONL" 50

MOCK_FAIL_DIR="${TEST_DIR}/mock-fail-supercompact"
create_failing_compact "$MOCK_FAIL_DIR"
rm -f /tmp/supercompact.lock

ORIG_CONTENT=$(cat "$FAIL_JSONL")

EXIT_CODE=0
PATH="${MOCK_BIN}:${PATH}" \
  PLUGIN_SETTING_DIR="${MOCK_FAIL_DIR}" \
  AGENT_WRAPPER_PID="999999" \
  bash "${HOOKS_DIR}/supercompact-compact.sh" \
    --jsonl "${FAIL_JSONL}" \
    --trigger "manual" 2>/dev/null || EXIT_CODE=$?

if [[ $EXIT_CODE -eq 0 ]]; then pass "Graceful fallback (exit 0)"; else fail "Should exit 0 on failure, got $EXIT_CODE"; fi

NEW_CONTENT=$(cat "$FAIL_JSONL")
if [[ "$ORIG_CONTENT" == "$NEW_CONTENT" ]]; then
  pass "JSONL unchanged after failure"
else
  fail "JSONL should be unchanged after failure"
fi

# ============================================================
# Test 5: Lock contention — second instance skips
# ============================================================
echo "--- Test 5: Lock contention ---"

LOCK_JSONL="${TEST_DIR}/lock-test.jsonl"
create_mock_jsonl "$LOCK_JSONL" 50
rm -f /tmp/supercompact.lock

# Hold the lock
exec 8>/tmp/supercompact.lock
flock -n 8

EXIT_CODE=0
PATH="${MOCK_BIN}:${PATH}" \
  PLUGIN_SETTING_DIR="${MOCK_SC_DIR}" \
  AGENT_WRAPPER_PID="999999" \
  bash "${HOOKS_DIR}/supercompact-compact.sh" \
    --jsonl "${LOCK_JSONL}" \
    --trigger "preemptive" 2>/dev/null || EXIT_CODE=$?

flock -u 8
exec 8>&-

if [[ $EXIT_CODE -eq 0 ]]; then pass "Locked out exits 0"; else fail "Should exit 0 when locked out, got $EXIT_CODE"; fi

if grep -q "Another compaction is already running" "${HOME}/.cache/supercompact/hook.log" 2>/dev/null; then
  pass "Lock contention logged"
else
  fail "Should log lock contention"
fi

# ============================================================
# Test 6: PreCompact hook delegates correctly
# ============================================================
echo "--- Test 6: PreCompact delegation ---"

PRECOMPACT_JSONL="${TEST_DIR}/precompact-test.jsonl"
create_mock_jsonl "$PRECOMPACT_JSONL" 200
rm -f /tmp/supercompact.lock /tmp/test-supercompact-refresh-called

INPUT_JSON=$(jq -n --arg path "$PRECOMPACT_JSONL" '{transcript_path: $path, trigger: "manual"}')
ORIG_LINES=$(wc -l < "$PRECOMPACT_JSONL")

EXIT_CODE=0
echo "$INPUT_JSON" | \
  PATH="${MOCK_BIN}:${PATH}" \
  PLUGIN_SETTING_DIR="${MOCK_SC_DIR}" \
  AGENT_WRAPPER_PID="999999" \
  CLAUDE_PLUGIN_ROOT="${PLUGIN_DIR}" \
  bash "${HOOKS_DIR}/supercompact-precompact.sh" 2>/dev/null || EXIT_CODE=$?

NEW_LINES=$(wc -l < "$PRECOMPACT_JSONL")
if (( NEW_LINES < ORIG_LINES )); then
  pass "PreCompact compacted via pipeline (${ORIG_LINES} -> ${NEW_LINES})"
else
  fail "PreCompact should have compacted (was ${ORIG_LINES}, now ${NEW_LINES})"
fi

# ============================================================
# Test 7: Output validation — invalid JSON rejected
# ============================================================
echo "--- Test 7: Truncated output rejected ---"

MOCK_BAD_DIR="${TEST_DIR}/mock-bad-supercompact"
mkdir -p "$MOCK_BAD_DIR"
cat > "$MOCK_BAD_DIR/compact.py" << 'PYEOF'
#!/usr/bin/env python3
import sys
args = sys.argv[1:]
output_file = None
i = 1
while i < len(args):
    if args[i] == "--output" and i + 1 < len(args):
        output_file = args[i + 1]; i += 2
    else: i += 1
with open(output_file, 'w') as f:
    f.write('{"valid": "line"}\n')
    f.write('{"truncated": "line\n')
PYEOF

BAD_JSONL="${TEST_DIR}/bad-output-test.jsonl"
create_mock_jsonl "$BAD_JSONL" 50
rm -f /tmp/supercompact.lock

ORIG_CONTENT=$(cat "$BAD_JSONL")

EXIT_CODE=0
PATH="${MOCK_BIN}:${PATH}" \
  PLUGIN_SETTING_DIR="${MOCK_BAD_DIR}" \
  AGENT_WRAPPER_PID="999999" \
  bash "${HOOKS_DIR}/supercompact-compact.sh" \
    --jsonl "${BAD_JSONL}" \
    --trigger "manual" 2>/dev/null || EXIT_CODE=$?

NEW_CONTENT=$(cat "$BAD_JSONL")
if [[ "$ORIG_CONTENT" == "$NEW_CONTENT" ]]; then
  pass "Truncated output rejected"
else
  fail "Truncated output should be rejected"
fi

# ============================================================
# Test 8: Missing uv — graceful fallback
# ============================================================
echo "--- Test 8: Missing uv ---"

NOUV_JSONL="${TEST_DIR}/nouv-test.jsonl"
create_mock_jsonl "$NOUV_JSONL" 50
rm -f /tmp/supercompact.lock

EXIT_CODE=0
PATH="/usr/bin:/bin" \
  PLUGIN_SETTING_DIR="${MOCK_SC_DIR}" \
  AGENT_WRAPPER_PID="999999" \
  bash "${HOOKS_DIR}/supercompact-compact.sh" \
    --jsonl "${NOUV_JSONL}" \
    --trigger "manual" 2>/dev/null || EXIT_CODE=$?

if [[ $EXIT_CODE -eq 0 ]]; then pass "Missing uv fallback (exit 0)"; else fail "Should exit 0 when uv missing, got $EXIT_CODE"; fi

# ============================================================
echo ""
echo "=== Results: ${PASS} passed, ${FAIL} failed ==="
(( FAIL > 0 )) && exit 1
exit 0
