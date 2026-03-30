# Getting Started

Unleash is a unified CLI manager for AI code agents — Claude Code, Codex, Gemini CLI, and OpenCode. It wraps these CLIs with a TUI, profiles, version management, and a plugin system.

## Prerequisites

- **curl** or **wget**
- **git**
- **Node.js / npm** — required for Claude Code and Gemini CLI
- **Rust / Cargo** — optional, only needed to build from source

## Install

Clone and run the installer:

```bash
gh repo clone heiervang-technologies/unleash /tmp/unleash \
  && bash /tmp/unleash/scripts/install.sh \
  && rm -rf /tmp/unleash
```

Re-run the same commands to update to the latest version.

## First Run

Launch the TUI:

```bash
unleash
```

Navigation:

| Key | Action |
|-----|--------|
| `j` / `k` or arrows | Move up/down |
| `Enter` | Select |
| `Esc` | Back |
| `?` | Help |

## Pick a Profile

Unleash ships with four default profiles:

| Profile | Agent |
|---------|-------|
| `claude` | Claude Code |
| `codex` | Codex CLI |
| `gemini` | Gemini CLI |
| `opencode` | OpenCode |

Launch directly from the command line:

```bash
unleash claude
unleash codex
unleash gemini
unleash opencode
```

## Key Concepts

### Profiles

Named configurations stored in `~/.config/unleash/profiles/*.toml`. Each profile defines which agent to launch and how.

### Unified Flags

Common flags work across all agents:

| Flag | Purpose |
|------|---------|
| `-p` | Non-interactive headless mode with given prompt |
| `-m` | Model override |
| `-c` | Continue last session |
| `-r` | Resume specific session |
| `--safe` | Restore permission prompts |
| `-a` / `--auto` | Enable auto-mode |
| `-e` | Reasoning effort level |
| `-x` | Crossload session from another CLI |
| `--fork` | Fork a session (with -c or -r) |
| `--dry-run` | Show resolved command without executing |

### Yolo Mode

By default, unleash bypasses permission prompts for all agents (equivalent to `--dangerously-skip-permissions` in Claude Code). Use `--safe` to restore interactive permission prompts.

### Plugins

Plugins extend agent behavior. Bundled plugins include:

- **auto-mode** — autonomous operation between prompts

- **process-restart** — session persistence and self-restart
- **mcp-refresh** — detect and reload MCP config changes
- **hyprland-focus** — window transparency on Hyprland

Plugins live in `plugins/bundled/` and are loaded via `--plugin-dir`.

## Next Steps

- [Profiles](profiles.md) — create and customize profiles
- [CLI Reference](cli-reference.md) — full flag and subcommand documentation
- [Plugins](plugins.md) — develop and configure plugins
