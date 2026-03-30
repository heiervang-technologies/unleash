# Profiles

Profiles control how Unleash launches agent CLIs. Each profile is a TOML file in `~/.config/unleash/profiles/`.

## Basics

Unleash creates four default profiles on first run:

| Profile    | Theme     |
|------------|-----------|
| `claude`   | `orange`  |
| `codex`    | `#aaaaaa` |
| `gemini`   | `#4285f4` |
| `opencode` | `#10b981` |

Select a profile from the TUI, or edit the TOML files directly.

**Reserved names:** `version`, `auth`, `auth-check`, `hooks`, `agents`, `install`, `uninstall`, `update`, `sessions`, `convert`, `help`, `config`, `plugins`.

## Full Example

```toml
name = "claude"
description = "Default Claude profile with auto-mode"
agent_cli_path = "unleash"       # binary to launch (alias: claude_path)
agent_cli_args = ["--verbose"]   # extra CLI args (aliases: agent_args, claude_args)
stop_prompt = "Continue working or ask the user for guidance."
theme = "orange"                 # named color or "#RRGGBB"

[env]
AU_HYPRLAND_FOCUS = "1"
AGENT_PERSONA = "CLAUDE"
MY_CUSTOM_VAR = "hello"

[defaults]
model = "opus"
safe = false       # false = yolo mode (skip permissions), true = safe mode
auto = false       # true = start in auto-mode
effort = "high"    # model effort level

[agents.claude]
extra_args = ["--allowedTools", "Bash,Read,Write"]

[agents.claude.env]
CLAUDE_CODE_MAX_TURNS = "50"

[agents.codex]
extra_args = ["--full-auto"]

[agents.codex.env]
CODEX_QUIET = "1"

[agents.gemini]
extra_args = []

[agents.gemini.env]
GEMINI_API_KEY = "sk-xxx"

[agents.opencode]
extra_args = []
```

## Defaults

The `[defaults]` section sets baseline behavior. CLI flags always override these.

| Field    | Type   | Default | Description                                |
|----------|--------|---------|--------------------------------------------|
| `model`  | string | none    | Model to use (e.g. `"opus"`, `"sonnet"`)   |
| `safe`   | bool   | `false` | `false` = yolo mode, `true` = safe mode    |
| `auto`   | bool   | `false` | Start in auto-mode                         |
| `effort` | string | none    | Model effort level                         |

## Per-Agent Overrides

The `[agents.*]` sections let you customize behavior for each supported CLI. Each agent block accepts:

- `extra_args` — additional CLI arguments appended when launching that agent
- `env` — environment variables set only for that agent

```toml
[agents.claude]
extra_args = ["--allowedTools", "Bash,Read,Write,Edit"]

[agents.claude.env]
CLAUDE_CODE_MAX_TURNS = "100"

[agents.codex]
extra_args = ["--full-auto"]
```

Supported agents: `claude`, `codex`, `gemini`, `opencode`.

## Environment Variables

Profile-level `[env]` vars apply to all agents. Per-agent `[agents.*.env]` vars apply only to that agent and take precedence over profile-level vars.

```toml
[env]
AU_HYPRLAND_FOCUS = "1"    # set for all agents

[agents.claude.env]
AU_HYPRLAND_FOCUS = "0"    # override for claude only
```

The default profiles ship with `AU_HYPRLAND_FOCUS = "1"`.

## Custom Agent CLI Path

By default, `agent_cli_path` is `"unleash"` which gives you the full wrapper experience (auto-mode, hooks, plugins). You can point it at a different binary:

```toml
agent_cli_path = "/usr/local/bin/claude"      # skip the wrapper, run claude directly
agent_cli_args = ["--dangerously-skip-permissions"]
```

The field also accepts the alias `claude_path`. Similarly, `agent_cli_args` can be written as `agent_args` or `claude_args`.

## Theme Colors

The `theme` field controls the TUI accent color for the profile. Use a named color or a hex code:

```toml
theme = "orange"      # named color
theme = "#4285f4"     # hex RGB
```

## Auto-Mode Stop Prompt

Customize the prompt shown when auto-mode pauses for user input:

```toml
stop_prompt = "Summarize what you did, then ask what to do next."
```

If omitted, the default stop prompt is used.

## Managing Profiles

**Via TUI:** Launch `unleash` and use the profile selector to switch between profiles.

**Via files:** Create or edit TOML files directly in `~/.config/unleash/profiles/`. The filename (without `.toml`) is the profile name.

```bash
cp ~/.config/unleash/profiles/claude.toml ~/.config/unleash/profiles/my-custom.toml
$EDITOR ~/.config/unleash/profiles/my-custom.toml
```

Profiles are loaded on startup — just select the updated profile.
