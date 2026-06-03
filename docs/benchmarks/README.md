# Benchmarks

Reproducible measurements of unleash's runtime characteristics. Each entry is
a frozen artifact (JSON + HTML report) tied to a date and a git revision so
later runs can be compared apples-to-apples.

## How to run

```bash
# Wrapper-overhead benchmark across all installed agent CLIs.
./scripts/bench-overhead.sh -n 15 --json docs/benchmarks/overhead-$(date +%F).json

# Render the HTML report (bar plots + conclusions).
python3 scripts/make-overhead-report.py \
  docs/benchmarks/overhead-$(date +%F).json \
  docs/benchmarks/overhead-$(date +%F).html
```

`./scripts/bench-overhead.sh --help` documents the full flag set
(iterations, warmup, timeout, per-agent command overrides).

## What we measure today

- **Startup overhead per agent CLI** — wall clock, user+sys CPU, peak RSS for
  running `<agent> --version` directly vs. through `unleash <agent>`.
  Captures the fixed cost every interactive session pays at launch.

## What we don't measure yet

- Per-turn overhead during a live session (hook dispatch, plugin callbacks,
  supercompact). Needs a synthetic turn loop.
- Memory growth across long sessions.
- TTY rendering / TUI latency.

## Two ways to interpret the numbers

Run with two baselines:

- **Raw** (default) — direct = bare agent invocation. The delta mixes
  wrapper cost with the cost of any startup paths the wrapper's flags
  trigger in the agent itself.
- **Equivalent** (`--equivalent`) — direct also receives the flags
  unleash would add (e.g. `--dangerously-skip-permissions` for claude),
  isolating the wrapper's own cost.

```bash
./scripts/bench-overhead.sh             -n 15 --json docs/benchmarks/overhead-$(date +%F).json
./scripts/bench-overhead.sh --equivalent -n 15 --json docs/benchmarks/overhead-$(date +%F)-equivalent.json
```

## Reports on file

| Date | Mode | Report | Raw data | Headline |
|------|------|--------|----------|----------|
| 2026-06-01 | raw | [overhead-2026-06-01.html](./overhead-2026-06-01.html) | [overhead-2026-06-01.json](./overhead-2026-06-01.json) | Claude pays +130 ms / +53 MB; 6 other agents within noise. Cause: any `--plugin-dir` or `--dangerously-skip-permissions` triggers claude's plugin/permissions init. |
| 2026-06-01 | equivalent | [overhead-2026-06-01-equivalent.html](./overhead-2026-06-01-equivalent.html) | [overhead-2026-06-01-equivalent.json](./overhead-2026-06-01-equivalent.json) | Apples-to-apples: every agent's wrapper-only cost is ≤25 ms wall and ≤±0.5 MB RSS. |
