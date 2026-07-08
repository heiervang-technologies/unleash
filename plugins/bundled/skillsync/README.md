# SkillSync Plugin for Unleash

Synchronize agent skills, custom commands, and references automatically across all your terminal AI harnesses.

This plugin is bundled with **Unleash** and enables you to define a custom skill or prompt once (e.g. for Claude Code or OpenCode) and propagate it to all other CLI agents (Gemini, Codex, Pi, Hermes, and Agy) with appropriate format translations.

## Features

*   **Zero-Config Synchronization**: Synchronizes your active skills from a designated source (default: Claude) to all other installed CLIs.
*   **Fidelity Translation**: Automatically degrades complex skills into format-native representations (e.g. custom prompts for Codex, custom TOML commands for Gemini/Agy, and context-file references for Pi/Hermes).
*   **Automated Triggers**: Syncs on session startup (if enabled) so your workspace is always up-to-date.
*   **Slash Command**: Provides the `/skillsync` command in your active session to trigger syncs on-demand and check status.

## Configuration

You can configure the plugin in your Unleash settings (or via the TUI plugin settings panel).

| Setting | Type | Allowed Values | Default | Description |
|---|---|---|---|---|
| `source` | Choice | `claude`, `codex`, `gemini`, `opencode`, `agy`, `pi`, `hermes`, `hub` | `claude` | The primary CLI from which active skills are discovered. |
| `sync_on_launch` | Choice | `on`, `off` | `on` | Whether to automatically run `unleash skills sync` when launching any agent CLI. |
| `delete_orphans` | Choice | `on`, `off` | `off` | If enabled, uninstalls/deletes target files when they are removed from the source. |

## Commands

Within an active session, use the following slash commands:

*   `/skillsync` -- Triggers an immediate skill synchronization.
*   `/skillsync status` -- Displays the fidelity status matrix for all synchronized skills.

From your shell, you can use:

*   `unleash skills list` -- Lists all synchronized skills and their targets.
*   `unleash skills status` -- Displays the tabular status matrix.
*   `unleash skills sync` -- Runs synchronization on-demand.
*   `unleash skills sync --delete-orphans` -- Removes target copies that disappeared from the source.
*   `unleash skills diff --delete-orphans` -- Performs a dry-run showing proposed changes, including orphan cleanup.

## Native vs. Degraded Fidelity

*   **Native** (`🟢`): Synced as a full native skill directory. Claude (`~/.claude/skills/`) supports this lossless mode, including auto-activation triggers.
*   **Degraded** (`🟡`): Converted into a custom prompt template for Codex (`~/.codex/prompts/<name>.md`) or a custom slash command for Gemini/Agy (`~/.gemini/commands/<name>.toml`). Instructions are fully preserved, but automatic file-matching triggers are lost.
*   **Reference** (`⚪`): Appended to the global context file (`AGENTS.md`) for OpenCode, Pi, and Hermes as a direct instruction reference block.
