# token-usage

Track token usage from agent CLIs into a single append-only log via
multiple, independently-toggleable collection methods.

**Current coverage**: Claude only — both the live Stop-hook tail and the
on-demand session scan. The other six built-in agents (Codex, Antigravity,
Gemini, OpenCode, Pi, Hermes) each have a different usage schema; extending
the scan is tracked in the [Limitations](#limitations) section.

## Why

Each agent reports usage differently, and the harnesses don't agree on schema
or storage location. This plugin centralizes the data into one append-only
log (`~/.local/share/unleash/token-usage.jsonl`) so you can answer questions
like "how much did this week cost me?" without scraping JSONL files by hand.

## Methods

Enable any combination in the Plugins tab of the unleash TUI. Defaults shown
in parentheses.

| Method | Setting | Default | What it captures |
|---|---|---|---|
| Stop-hook session tail | `method_session_tail` | ON | Live: after each Claude turn, reads the assistant message's `usage` field from the active transcript and appends a record. Zero config, no API keys. Claude only — other CLIs don't run our hooks. |
| On-demand session scan | `method_session_scan` | ON | Walks `~/.claude/projects/*.jsonl` when the report runs. Catches sessions that predate the plugin or were run with the hook disabled. Codex/Gemini/OpenCode/Pi support is a follow-up. |
| Anthropic admin API | `method_provider_api` | OFF | Reserved — settings surface only in v0.1. The toggle is visible so users can see it's coming; turning it on currently has no effect beyond a note in the report footer. Requires `ANTHROPIC_ADMIN_KEY` when implemented. |
| USD cost estimate | `estimate_cost_usd` | ON | Multiplies token counts by a hard-coded per-model price table. Estimates only — use provider billing for accounting. |
| Retention | `data_retention_days` | 90 | Records older than N days are pruned on each report run. |

## Reading the data

In a Claude session:

```
/token-usage                # default: group by CLI, all time
/token-usage --by model     # group by model
/token-usage --since 7d     # last week only
/token-usage --json         # machine-readable
```

From a shell:

```bash
~/.local/share/unleash/plugins/token-usage/scripts/report.sh
~/.local/share/unleash/plugins/token-usage/scripts/report.sh --by model --since 30d
```

Raw log lives at `~/.local/share/unleash/token-usage.jsonl` — one JSON object
per line. Safe to `tail -f` or feed into your own jq pipeline.

## Storage

- Settings: `~/.config/unleash/plugins/token-usage/settings.env`
  (env-style; written by the TUI when you toggle settings)
- Log:      `~/.local/share/unleash/token-usage.jsonl`

## Failure modes

The Stop hook fails safe — any error (missing `jq`, unreadable transcript,
malformed JSON) is silently dropped. Token tracking must never block the
agent. Same for `check-enabled.sh`: if `unleash` isn't on PATH, the hook
treats the plugin as enabled rather than crashing the session.

## Limitations

- Codex/Antigravity/Gemini/OpenCode/Pi/Hermes session-scan is not yet wired
  up. Each CLI has a different usage schema (Codex mirrors OpenAI's
  `prompt_tokens`/`completion_tokens`, Gemini uses
  `usageMetadata.{promptTokenCount,candidatesTokenCount}`, Hermes and
  Antigravity haven't been audited yet). Tracked for v0.2.
- Provider-API polling is gated on having admin keys per provider and a small
  daemon that won't get caught by network sandboxing. v0.1 reserves the
  setting; v0.2 will implement.
- Pricing table is small and hand-curated. Submit a PR if a model is missing.
