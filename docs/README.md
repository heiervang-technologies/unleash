# unleash Documentation

## User Guide

| Document | Description |
|----------|-------------|
| [Getting Started](getting-started.md) | Install, first run, pick a profile |
| [CLI Reference](cli-reference.md) | All flags, subcommands, and translation tables |
| [Profiles](profiles.md) | Create and configure named agent profiles |
| [Configuration](configuration.md) | Global settings (`config.toml`) |
| [Environment Variables](environment-variables.md) | All env vars unleash sets or reads |
| [Crossload](crossload.md) | Portable conversation histories across CLIs |
| [Docker](docker.md) | Sandboxed containers and multi-agent mesh |
| [Plugins](plugins.md) | Bundled plugin index and custom plugin pointers |

## Developer Guide

| Document | Description |
|----------|-------------|
| [Plugin Development](internal/claude-code/plugin-development.md) | Claude Code plugin internals |
| [Testing Guide](extensions/testing-guide.md) | Testing strategies and CI |
| [Restart & Refresh](extensions/restart-refresh.md) | Process restart internals |

## Reference

| Document | Description |
|----------|-------------|
| [CLI Format: Claude Code](internal/claude-code/CLI_FORMAT.md) | Claude Code JSONL session format |
| [CLI Format: Codex](internal/codex/CLI_FORMAT.md) | Codex JSONL session format |
| [CLI Format: Gemini](internal/gemini/CLI_FORMAT.md) | Gemini CLI JSON session format |
| [CLI Format: OpenCode](internal/opencode/CLI_FORMAT.md) | OpenCode SQLite session format |
