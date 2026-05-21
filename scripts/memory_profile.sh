#!/usr/bin/env bash
set -euo pipefail

# This script measures and compares the memory usage (Maximum Resident Set Size)
# of running each supported CLI directly vs running it through the unleash wrapper.

if ! command -v /usr/bin/time >/dev/null 2>&1; then
    echo "Error: /usr/bin/time is required. Please install it (e.g. apt-get install time)."
    exit 1
fi

AGENTS=("claude" "codex" "gemini" "opencode" "pi" "hermes")

echo "=========================================================="
echo " Memory Profiling: Unleash Wrapper vs Direct CLI"
echo "=========================================================="
printf "%-15s | %-15s | %-15s | %-15s\n" "Agent" "Direct (KB)" "Wrapped (KB)" "Overhead (KB)"
echo "----------------------------------------------------------"

has_measured=0

for agent in "${AGENTS[@]}"; do
    if command -v "$agent" >/dev/null 2>&1; then
        # We use an invalid flag so the agent prints an error and exits immediately
        # without hanging on interactive inputs or making API calls.
        DIRECT_CMD="--invalid-flag-for-mem-test"
        WRAPPED_CMD="-- --invalid-flag-for-mem-test"

        # Measure direct CLI
        direct_out=$(mktemp)
        env -u AGENT_UNLEASH -u AGENT_CMD /usr/bin/time -v "$agent" $DIRECT_CMD < /dev/null 2> "$direct_out" >/dev/null || true
        direct_mem=$(grep "Maximum resident set size" "$direct_out" | awk '{print $6}' || echo "")
        rm -f "$direct_out"

        # Measure wrapped CLI
        wrapped_out=$(mktemp)
        UNLEASH_CMD="unleash"
        if [ -x "./target/release/unleash" ]; then
            UNLEASH_CMD="./target/release/unleash"
        elif [ -x "./target/fast/unleash" ]; then
            UNLEASH_CMD="./target/fast/unleash"
        fi
        
        env -u AGENT_UNLEASH -u AGENT_CMD /usr/bin/time -v "$UNLEASH_CMD" "$agent" $WRAPPED_CMD < /dev/null 2> "$wrapped_out" >/dev/null || true
        wrapped_mem=$(grep "Maximum resident set size" "$wrapped_out" | awk '{print $6}' || echo "")
        rm -f "$wrapped_out"

        if [ -n "$direct_mem" ] && [ -n "$wrapped_mem" ]; then
            overhead=$((wrapped_mem - direct_mem))
            printf "%-15s | %-15s | %-15s | %+d\n" "$agent" "$direct_mem" "$wrapped_mem" "${overhead}"
            has_measured=1
        else
            printf "%-15s | %-15s | %-15s | %-15s\n" "$agent" "${direct_mem:-ERROR}" "${wrapped_mem:-ERROR}" "N/A"
        fi
    else
        printf "%-15s | %-15s | %-15s | %-15s\n" "$agent" "SKIP (missing)" "N/A" "N/A"
    fi
done

echo "=========================================================="

if [ "$has_measured" -eq 0 ]; then
    echo "No CLIs were found to measure."
    exit 0
fi
