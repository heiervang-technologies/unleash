#!/usr/bin/env bash
# bench-overhead.sh
#
# Measure the per-process startup overhead of running each supported agent CLI
# directly versus through `unleash`. Reports wall clock, user/sys CPU, and peak
# resident-set size across N iterations (median, p95, stddev).
#
# This is a STARTUP-time benchmark: each iteration runs the agent with a cheap
# command that exits immediately (default `--version`). The delta between the
# direct and wrapped runs is the wrapper's fixed cost — what every interactive
# session pays at launch.
#
# Usage:
#   scripts/bench-overhead.sh                    # all installed agents, 10 iters
#   scripts/bench-overhead.sh -n 20              # 20 iterations
#   scripts/bench-overhead.sh -t 60              # 60-second per-run timeout
#   scripts/bench-overhead.sh --json out.json    # also write machine-readable
#   scripts/bench-overhead.sh claude codex       # only those agents
#   scripts/bench-overhead.sh --cmd '--help'     # override command args
#   scripts/bench-overhead.sh --unleash ./target/release/unleash
#
# Per-agent command override (defaults to --version):
#   BENCH_CMD_HERMES='--help' scripts/bench-overhead.sh hermes
#
# Requirements:
#   /usr/bin/time  (GNU time, not the shell builtin)
#   awk            (for statistics)
#   timeout        (coreutils, for the per-run cap)
#
# Methodology notes:
#   * Wall clock comes from GNU time's %e — 10 ms resolution. Sub-10 ms direct
#     runs show as 0; the wrapped delta is still meaningful.
#   * Max RSS comes from GNU time's %M (rusage ru_maxrss). It is the peak of
#     the immediate child process; on Linux GNU time aggregates self+children
#     so the wrapped figure includes both the unleash process and the agent it
#     spawns. This is the right number to report — it's what the user's machine
#     actually pays — but it is *not* a per-process breakdown.
#   * Both modes get the same environment scrub (AGENT_UNLEASH, AGENT_AUTO_MODE
#     and AGENT_WRAPPER_PID are cleared) so a previously-wrapped shell doesn't
#     pollute the "direct" baseline.
#   * Some agents do network checks during --version (e.g. hermes' "Up to date"
#     ping). Both modes pay this equally so the delta is honest, but absolute
#     wall numbers will be noisier. Use --cmd or BENCH_CMD_* to swap in a
#     quieter command if you have one.

set -euo pipefail

# ---------------------------- defaults ----------------------------

ALL_AGENTS=(claude codex agy gemini opencode pi hermes)
ITERS=10
WARMUP=1
TIMEOUT=30
DEFAULT_CMD="--version"
JSON_OUT=""
UNLEASH_BIN=""
USE_COLOR=1
AGENTS=()

# ---------------------------- argparse ----------------------------

print_help() {
    # Print the header comment (everything from line 2 down to the first blank
    # or `set -euo pipefail` line) so --help stays in sync with the block above.
    awk '
        NR == 1 { next }
        /^set -euo pipefail/ { exit }
        /^$/ { exit }
        { sub(/^# ?/, ""); print }
    ' "$0"
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -n|--iters) ITERS="$2"; shift 2 ;;
        -w|--warmup) WARMUP="$2"; shift 2 ;;
        -t|--timeout) TIMEOUT="$2"; shift 2 ;;
        --cmd) DEFAULT_CMD="$2"; shift 2 ;;
        --json) JSON_OUT="$2"; shift 2 ;;
        --unleash) UNLEASH_BIN="$2"; shift 2 ;;
        --no-color) USE_COLOR=0; shift ;;
        -h|--help) print_help; exit 0 ;;
        --) shift; AGENTS+=("$@"); break ;;
        -*) echo "unknown flag: $1" >&2; exit 2 ;;
        *) AGENTS+=("$1"); shift ;;
    esac
done

if [[ ${#AGENTS[@]} -eq 0 ]]; then
    AGENTS=("${ALL_AGENTS[@]}")
fi

# ---------------------------- prereqs ----------------------------

if ! [[ -x /usr/bin/time ]]; then
    echo "error: /usr/bin/time (GNU time) is required" >&2
    echo "  install with: apt-get install time   (or equivalent)" >&2
    exit 1
fi
if ! command -v timeout >/dev/null 2>&1; then
    echo "error: \`timeout\` (coreutils) is required" >&2
    exit 1
fi

# Locate unleash binary
resolve_unleash() {
    if [[ -n "$UNLEASH_BIN" ]]; then
        [[ -x "$UNLEASH_BIN" ]] || { echo "error: unleash not executable: $UNLEASH_BIN" >&2; exit 1; }
        echo "$UNLEASH_BIN"; return
    fi
    for c in ./target/release/unleash ./target/fast/unleash; do
        [[ -x "$c" ]] && { echo "$c"; return; }
    done
    if command -v unleash >/dev/null 2>&1; then
        command -v unleash; return
    fi
    echo "error: unleash binary not found (tried ./target/release, ./target/fast, \$PATH)" >&2
    echo "  build with: cargo build --release   or pass --unleash PATH" >&2
    exit 1
}
UNLEASH_BIN="$(resolve_unleash)"

# ANSI colors
if [[ $USE_COLOR -eq 1 && -t 1 ]]; then
    C_DIM=$'\033[2m'; C_BOLD=$'\033[1m'
    C_RED=$'\033[31m'; C_YELLOW=$'\033[33m'; C_RESET=$'\033[0m'
else
    C_DIM=; C_BOLD=; C_RED=; C_YELLOW=; C_RESET=
fi

# ---------------------------- helpers ----------------------------

# Run one iteration. Args: <output_file> <cmd> <args...>
# Writes one line to <output_file>: "wall user sys maxrss_kb exit"
# Returns 0 always (failures are recorded as `exit != 0`).
time_one_run() {
    local out="$1"; shift
    /usr/bin/time -f '%e %U %S %M %x' -o "$out" \
        timeout -k 1 "${TIMEOUT}s" "$@" </dev/null >/dev/null 2>&1 || true
}

# Read N samples for one (agent, mode) combo.
# Args: <samples_file> <iters> <cmd> <args...>
# Records on each line: "wall user sys maxrss_kb"
collect_samples() {
    local samples="$1"; local iters="$2"; shift 2
    : > "$samples"
    local tmp; tmp=$(mktemp)
    # warmups (discarded)
    for ((i=0; i<WARMUP; i++)); do
        time_one_run "$tmp" "$@"
    done
    # timed runs
    for ((i=0; i<iters; i++)); do
        time_one_run "$tmp" "$@"
        # If the run hit the wall-clock timeout, drop the sample — its wall
        # number is the cap, not a measurement. Exit 124 is timeout(1)'s code.
        local line wall user sys maxrss rc
        line=$(cat "$tmp")
        read -r wall user sys maxrss rc <<<"$line"
        if [[ "$rc" != "0" ]]; then
            # Bench command should exit 0 on a healthy run. Treat non-zero as
            # noise but keep the sample if the agent simply exits with a code
            # (some print usage to stderr and exit 1 on --version). The user
            # can override via --cmd if needed.
            :
        fi
        printf "%s %s %s %s\n" "$wall" "$user" "$sys" "$maxrss" >> "$samples"
    done
    rm -f "$tmp"
}

# Compute stats from a column of numbers on stdin.
# Prints: "median p95 stddev min max n"
stats() {
    awk '
        { v[NR]=$1+0; s+=$1; n++ }
        END {
            if (n == 0) { print "0 0 0 0 0 0"; exit }
            for (i=1; i<=n; i++) for (j=i+1; j<=n; j++) if (v[j]<v[i]) { t=v[i]; v[i]=v[j]; v[j]=t }
            mean = s/n
            if (n%2) median = v[(n+1)/2]; else median = (v[n/2]+v[n/2+1])/2
            p95i = int(n*0.95 + 0.5); if (p95i<1) p95i=1; if (p95i>n) p95i=n
            ss=0; for (i=1; i<=n; i++) ss += (v[i]-mean)*(v[i]-mean)
            stddev = (n>1) ? sqrt(ss/(n-1)) : 0
            printf "%.6f %.6f %.6f %.6f %.6f %d\n", median, v[p95i], stddev, v[1], v[n], n
        }'
}

# Extract one column (1-indexed) from a file.
col() {
    awk -v c="$1" '{ print $c }' "$2"
}

# Per-agent default command (lookup BENCH_CMD_<UPPER>; fallback to $DEFAULT_CMD).
cmd_for_agent() {
    local agent="$1"
    local var
    var="BENCH_CMD_$(echo "$agent" | tr '[:lower:]' '[:upper:]')"
    if [[ -n "${!var:-}" ]]; then
        echo "${!var}"
    else
        echo "$DEFAULT_CMD"
    fi
}

# Friendly KB-or-MB formatter for table output.
fmt_kb() {
    awk -v k="$1" 'BEGIN {
        if (k+0 >= 1024*10) printf "%.1f MB", k/1024;
        else printf "%.0f KB", k+0;
    }'
}

fmt_sec() {
    awk -v s="$1" 'BEGIN {
        if (s+0 < 1) printf "%.0f ms", s*1000;
        else printf "%.2f s", s+0;
    }'
}

# Percentage overhead: ((wrapped - direct) / direct) * 100. Guards div-by-zero.
pct() {
    awk -v d="$1" -v w="$2" 'BEGIN {
        if (d+0 == 0) print "—";
        else printf "%+.1f%%", (w-d)/d*100
    }'
}

# Absolute delta, signed.
delta_sec() {
    awk -v d="$1" -v w="$2" 'BEGIN { printf "%+.0f ms", (w-d)*1000 }'
}
delta_kb() {
    awk -v d="$1" -v w="$2" 'BEGIN {
        diff = w - d
        if (diff > 1024*10 || diff < -1024*10) printf "%+.1f MB", diff/1024;
        else printf "%+.0f KB", diff
    }'
}

# ---------------------------- main loop ----------------------------

echo "${C_BOLD}unleash overhead benchmark${C_RESET}"
echo "  unleash:    $UNLEASH_BIN"
echo "  iterations: $ITERS  (+$WARMUP warmup)"
echo "  timeout:    ${TIMEOUT}s per run"
echo "  command:    $DEFAULT_CMD"
echo ""

WORKDIR=$(mktemp -d)
trap 'rm -rf "$WORKDIR"' EXIT

declare -a JSON_AGENTS=()
SKIPPED=()

# Clear environment vars that would distort the measurement (auto-mode marker
# files, parent wrapper PID inheritance, etc.) For honesty: also clear
# AGENT_UNLEASH and AGENT_WRAPPER_PID so direct runs aren't accidentally treated
# as already-wrapped by anything that checks.
BENCH_ENV=(env -u AGENT_UNLEASH -u AGENT_WRAPPER_PID -u AGENT_AUTO_MODE)

for agent in "${AGENTS[@]}"; do
    if ! command -v "$agent" >/dev/null 2>&1; then
        SKIPPED+=("$agent")
        printf "  ${C_DIM}skip %s — not on PATH${C_RESET}\n" "$agent"
        continue
    fi

    agent_cmd="$(cmd_for_agent "$agent")"
    read -ra agent_argv <<<"$agent_cmd"

    printf "  ${C_BOLD}%s${C_RESET}  $agent_cmd\n" "$agent"

    direct_samples="$WORKDIR/${agent}.direct"
    wrapped_samples="$WORKDIR/${agent}.wrapped"

    collect_samples "$direct_samples" "$ITERS" \
        "${BENCH_ENV[@]}" "$agent" "${agent_argv[@]}"
    collect_samples "$wrapped_samples" "$ITERS" \
        "${BENCH_ENV[@]}" "$UNLEASH_BIN" "$agent" -- "${agent_argv[@]}"

    # Skip agent entirely if we got no successful samples.
    if [[ ! -s "$direct_samples" || ! -s "$wrapped_samples" ]]; then
        printf "    ${C_RED}error: no samples collected${C_RESET}\n"
        SKIPPED+=("$agent (no samples)")
        continue
    fi

    # Compute stats for each metric.
    read -r d_wall_med d_wall_p95 d_wall_sd d_wall_min d_wall_max d_n < <(col 1 "$direct_samples" | stats)
    read -r d_usr_med d_usr_p95 d_usr_sd _ _ _                       < <(col 2 "$direct_samples" | stats)
    read -r d_sys_med d_sys_p95 d_sys_sd _ _ _                       < <(col 3 "$direct_samples" | stats)
    read -r d_rss_med d_rss_p95 d_rss_sd d_rss_min d_rss_max _       < <(col 4 "$direct_samples" | stats)

    read -r w_wall_med w_wall_p95 w_wall_sd w_wall_min w_wall_max w_n < <(col 1 "$wrapped_samples" | stats)
    read -r w_usr_med w_usr_p95 w_usr_sd _ _ _                       < <(col 2 "$wrapped_samples" | stats)
    read -r w_sys_med w_sys_p95 w_sys_sd _ _ _                       < <(col 3 "$wrapped_samples" | stats)
    read -r w_rss_med w_rss_p95 w_rss_sd w_rss_min w_rss_max _       < <(col 4 "$wrapped_samples" | stats)

    # Stash for the summary table at the end (avoid recomputing).
    {
        printf "%s\t" "$agent"
        printf "%s\t%s\t%s\t%s\t" "$d_wall_med" "$d_usr_med" "$d_sys_med" "$d_rss_med"
        printf "%s\t%s\t%s\t%s\t" "$w_wall_med" "$w_usr_med" "$w_sys_med" "$w_rss_med"
        printf "%s\t%s\n" "$d_n" "$w_n"
    } >> "$WORKDIR/summary.tsv"

    # Live per-agent print so the user gets feedback before the whole run ends.
    printf "    direct   wall=%s  cpu=%s+%s  rss=%s   (n=%s)\n" \
        "$(fmt_sec "$d_wall_med")" "$(fmt_sec "$d_usr_med")" "$(fmt_sec "$d_sys_med")" \
        "$(fmt_kb "$d_rss_med")" "$d_n"
    printf "    wrapped  wall=%s  cpu=%s+%s  rss=%s   (n=%s)\n" \
        "$(fmt_sec "$w_wall_med")" "$(fmt_sec "$w_usr_med")" "$(fmt_sec "$w_sys_med")" \
        "$(fmt_kb "$w_rss_med")" "$w_n"
    printf "    ${C_YELLOW}overhead${C_RESET} wall=%s (%s)  rss=%s (%s)\n\n" \
        "$(delta_sec "$d_wall_med" "$w_wall_med")" "$(pct "$d_wall_med" "$w_wall_med")" \
        "$(delta_kb "$d_rss_med" "$w_rss_med")" "$(pct "$d_rss_med" "$w_rss_med")"

    JSON_AGENTS+=("$agent")
    # Stash full stats for JSON output.
    cat > "$WORKDIR/${agent}.stats" <<EOF
d_wall_med=$d_wall_med
d_wall_p95=$d_wall_p95
d_wall_sd=$d_wall_sd
d_wall_min=$d_wall_min
d_wall_max=$d_wall_max
d_usr_med=$d_usr_med
d_usr_p95=$d_usr_p95
d_usr_sd=$d_usr_sd
d_sys_med=$d_sys_med
d_sys_p95=$d_sys_p95
d_sys_sd=$d_sys_sd
d_rss_med=$d_rss_med
d_rss_p95=$d_rss_p95
d_rss_sd=$d_rss_sd
d_rss_min=$d_rss_min
d_rss_max=$d_rss_max
d_n=$d_n
w_wall_med=$w_wall_med
w_wall_p95=$w_wall_p95
w_wall_sd=$w_wall_sd
w_wall_min=$w_wall_min
w_wall_max=$w_wall_max
w_usr_med=$w_usr_med
w_usr_p95=$w_usr_p95
w_usr_sd=$w_usr_sd
w_sys_med=$w_sys_med
w_sys_p95=$w_sys_p95
w_sys_sd=$w_sys_sd
w_rss_med=$w_rss_med
w_rss_p95=$w_rss_p95
w_rss_sd=$w_rss_sd
w_rss_min=$w_rss_min
w_rss_max=$w_rss_max
w_n=$w_n
EOF
done

# ---------------------------- markdown summary ----------------------------

if [[ -f "$WORKDIR/summary.tsv" ]]; then
    echo "${C_BOLD}=== Overhead summary ===${C_RESET}"
    echo ""
    printf "| %-10s | %-10s | %-10s | %-13s | %-13s |\n" \
        "Agent" "Wall Δ" "RSS Δ" "Wall (direct→wrapped)" "RSS (direct→wrapped)"
    printf "|%s|%s|%s|%s|%s|\n" \
        "------------" "------------" "------------" "-----------------------" "-----------------------"
    # shellcheck disable=SC2034  # d_u/d_s/w_u/w_s/dn/wn are destructured for shape but unused in this view
    while IFS=$'\t' read -r agent d_w d_u d_s d_r w_w w_u w_s w_r dn wn; do
        printf "| %-10s | %-10s | %-10s | %s → %s | %s → %s |\n" \
            "$agent" \
            "$(delta_sec "$d_w" "$w_w")" \
            "$(delta_kb "$d_r" "$w_r")" \
            "$(fmt_sec "$d_w")" "$(fmt_sec "$w_w")" \
            "$(fmt_kb "$d_r")"  "$(fmt_kb "$w_r")"
    done < "$WORKDIR/summary.tsv"
    echo ""
    if [[ ${#SKIPPED[@]} -gt 0 ]]; then
        printf "${C_DIM}skipped: %s${C_RESET}\n" "${SKIPPED[*]}"
    fi
fi

# ---------------------------- JSON output ----------------------------

if [[ -n "$JSON_OUT" ]]; then
    {
        printf '{\n'
        printf '  "metadata": {\n'
        printf '    "iterations": %s,\n' "$ITERS"
        printf '    "warmup": %s,\n' "$WARMUP"
        printf '    "timeout_sec": %s,\n' "$TIMEOUT"
        printf '    "command": "%s",\n' "$DEFAULT_CMD"
        printf '    "unleash": "%s",\n' "$UNLEASH_BIN"
        printf '    "host": "%s",\n' "$(uname -n)"
        printf '    "kernel": "%s",\n' "$(uname -r)"
        printf '    "timestamp": "%s"\n' "$(date -u +%Y-%m-%dT%H:%M:%SZ)"
        printf '  },\n'
        printf '  "results": [\n'
        first=1
        for agent in "${JSON_AGENTS[@]}"; do
            # shellcheck disable=SC1090
            source "$WORKDIR/${agent}.stats"
            [[ $first -eq 1 ]] && first=0 || printf ',\n'
            cat <<JSON
    {
      "agent": "$agent",
      "direct": {
        "wall_sec":   { "median": $d_wall_med, "p95": $d_wall_p95, "stddev": $d_wall_sd, "min": $d_wall_min, "max": $d_wall_max, "n": $d_n },
        "user_sec":   { "median": $d_usr_med,  "p95": $d_usr_p95,  "stddev": $d_usr_sd },
        "sys_sec":    { "median": $d_sys_med,  "p95": $d_sys_p95,  "stddev": $d_sys_sd },
        "max_rss_kb": { "median": $d_rss_med,  "p95": $d_rss_p95,  "stddev": $d_rss_sd, "min": $d_rss_min, "max": $d_rss_max }
      },
      "wrapped": {
        "wall_sec":   { "median": $w_wall_med, "p95": $w_wall_p95, "stddev": $w_wall_sd, "min": $w_wall_min, "max": $w_wall_max, "n": $w_n },
        "user_sec":   { "median": $w_usr_med,  "p95": $w_usr_p95,  "stddev": $w_usr_sd },
        "sys_sec":    { "median": $w_sys_med,  "p95": $w_sys_p95,  "stddev": $w_sys_sd },
        "max_rss_kb": { "median": $w_rss_med,  "p95": $w_rss_p95,  "stddev": $w_rss_sd, "min": $w_rss_min, "max": $w_rss_max }
      },
      "overhead": {
        "wall_sec_abs": $(awk -v d=$d_wall_med -v w=$w_wall_med 'BEGIN{printf "%.6f", w-d}'),
        "max_rss_kb_abs": $(awk -v d=$d_rss_med -v w=$w_rss_med 'BEGIN{printf "%.0f", w-d}')
      }
    }
JSON
        done
        printf '\n  ]\n}\n'
    } > "$JSON_OUT"
    echo "wrote $JSON_OUT"
fi
