# Internal Developer Documentation

Reference documentation for Unleash developers. Not user-facing.

## Contents

### CLI Formats (`cli-formats/`)

How each supported agent CLI stores conversations, sessions, and metadata.
Used as the foundation for the chat log interchange format.

- [Overview & Comparison](cli-formats/overview.md) — feature matrix across all 4 CLIs
- [Claude Code](cli-formats/claude-code.md) — JSONL transcripts, 12+ message types
- [Codex](cli-formats/codex.md) — JSONL + SQLite hybrid, event stream model
- [Gemini CLI](cli-formats/gemini-cli.md) — JSON sessions, thoughts, project hashing
- [OpenCode](cli-formats/opencode.md) — SQLite + Drizzle ORM, message/part separation

## Maintenance

These docs should be updated when:
- A CLI updates its storage format (check after major version bumps)
- A new CLI is added to Unleash
- Research reveals new details about a CLI's internals

Each document includes a "Last verified" date and CLI version.
