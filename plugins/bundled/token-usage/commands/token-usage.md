---
name: token-usage
description: Show a token-usage summary across your indexed sessions
---

# /token-usage

Run the token-usage report. By default it groups by CLI and shows the last
all-time total; pass arguments to narrow the window or change the grouping.

## Usage

- `/token-usage` — table grouped by CLI, all-time
- `/token-usage --by model` — group by model
- `/token-usage --since 7d` — last 7 days only
- `/token-usage --json` — JSON output

## What it does

Invokes `${CLAUDE_PLUGIN_ROOT}/scripts/report.sh` with the same arguments.
The plugin's settings control which collection methods feed the report:
session-tail (Stop hook, Claude only), session-scan (walks every Claude
JSONL on disk), and (opt-in) provider-API. Pricing estimates use a small
hard-coded table in the script — accurate enough for ballparking, not for
billing.
