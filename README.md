# unleash
**unleash** your agent.

<p align="center">
  <img src="assets/demo-recording.gif" alt="unleash" width="700">
</p>

**unleash** is...
  
* an **agent CLI verison manager**. `nvm` for AI agents such as claude code, codex, gemini and opencode with a ritch TUI 
  
* a compatibility layer that lets you start in claude code, then continue where you left off in codex.  

* a **unified cli** that brings all your code agents under the same signature. No more confusion about `claude -p` vs `codex run`

* an enabler for more **advanced features** such as self-restart, auto-mode and more.

* Made for the sandbox — Use ours, Bring your own, or **take the risk**

### Install

```sh
curl -fsSL unleash.software/install | bash
```
or with docker:
```sh
docker run --rm -it marksverdhei/unleash
```
(with auth tokens)
```sh
docker run -it --rm -e ANTHROPIC_API_KEY -e CLAUDE_CODE_OAUTH_TOKEN -e OPENAI_API_KEY -e GEMINI_API_KEY -e OPENROUTER_API_KEY marksverdhei/unleash
```

> See [Installation](#installation) for build-from-source, platform details, and non-interactive mode.

> **unleash is best run in a sandbox.** Bring your own or use ours — see the [Docker + gVisor sandbox guide](docs/docker.md) for for more  hardened containers with LAN isolation.



**After install:**
```bash
unleash          # Launch TUI (profiles, versions, settings)
unleash claude   # Start Claude with unleash features
unleash codex    # Start Codex with unleash features
unleash gemini   # Start Gemini CLI with unleash features
unleash opencode # Start OpenCode with unleash features
```

> Run the same install command again to update to the latest version.

## CLI Usage

### Running Agents

```bash
unleash <profile> [unified flags] [-- agent-specific flags]
```

The first argument is always a **profile name**. The four default profiles (`claude`, `codex`, `gemini`, `opencode`) map to their respective agents. Custom profiles can target any agent with custom settings.

```bash
unleash claude -m opus -c              # Continue last Claude session with Opus
unleash codex --safe                   # Run Codex with approval prompts
unleash gemini -p "fix the tests"     # Gemini headless mode
unleash work                           # Run a custom "work" profile
```

### Unified Flags

These flags work identically across all agents. unleash translates them into the correct native syntax.

| Flag | Short | Description | Default |
|------|-------|-------------|---------|
| `--safe` | | Restore approval prompts (permissions bypassed by default) | off |
| `--prompt <prompt>` | `-p` | Run non-interactively with the given prompt | |
| `--model <model>` | `-m` | Model to use for the session | |
| `--continue` | `-c` | Continue the most recent session | |
| `--resume [id]` | `-r` | Resume a session by ID, or open picker | |
| `--fork` | | Fork the session (use with `--continue` or `--resume`) | |
| `--auto` | `-a` | Enable auto-mode (autonomous operation) | |

Anything after `--` is passed directly to the agent CLI unchanged:

```bash
unleash claude -m opus -- --effort max --verbose
#      ^^^^^^ ^^^^^^^^    ^^^^^^^^^^^^^^^^^^^^^^^^^
#      Profile  Unified    Passthrough (Claude-specific)
```

### How Translation Works

| unleash | Claude | Codex | Gemini | OpenCode |
|---------|--------|-------|--------|----------|
| `-p <prompt>` | `-p <prompt>` | `exec <prompt>` | `-p <prompt>` | `run <prompt>` |
| `-c` | `--continue` | `resume --last` | `--resume latest` | `--continue` |
| `-r [id]` | `--resume [id]` | `resume [id]` | `--resume [id]` | `--session <id>` |
| `--fork` | `--fork-session` | `fork` subcommand | *(unsupported)* | `--fork` |
| *(default)* | `--dangerously-skip-permissions` | `--dangerously-bypass-approvals-and-sandbox` | `--yolo` | *(no-op)* |

### Management Commands

```bash
unleash                    # Launch TUI
unleash update             # Update all agents (parallel progress bars)
unleash update --check     # Check for updates without installing
unleash update codex       # Update a specific agent
unleash version            # Show installed versions
unleash version --list     # List available versions
unleash auth               # Check authentication status
unleash agents status      # Show all agent versions and update status
```

## Version Management

unleash manages versions for all four agent CLIs:

- **Claude Code**: Native binary (GCS) or npm install
- **Codex**: Prebuilt binary from GitHub releases, cargo build fallback
- **Gemini CLI**: npm install
- **OpenCode**: Built-in `opencode upgrade` command

Version filtering:
- **Blacklist mode** (default for Claude): All versions allowed except known-bad ones
- **Whitelist mode** (default for Codex): Only verified versions allowed
- Version lists are maintained in `Cargo.toml` and compiled into the binary

## Extended Capabilities

Features that unleash adds on top of the base agent CLIs:

### Available Now

- **Self-restart**: Restart the agent while preserving session state (`unleash-refresh`, also available as `restart-claude`)
- **Auto-mode**: Autonomous operation via Stop hook + flag file system
- **Plugin system**: Custom functionality loaded via `--plugin-dir`
- **MCP refresh**: Detect MCP configuration changes and trigger reload
- **Voice output**: Multi-provider TTS for agent responses (VibeVoice, OpenAI, ElevenLabs)
- **Profile system**: Named configurations with per-agent settings, env vars, and themes
- **Parallel updates**: Update all agents simultaneously with progress visualization

### Cross-CLI Session Crossload

Load conversation history from any agent into any other. Browse all sessions with `unleash sessions`, then crossload with `-x`:

```bash
unleash claude -x codex:rust-eng     # Load Codex session into Claude
unleash gemini -x claude:rice-chief  # Load Claude session into Gemini
unleash claude -x                    # Interactive session picker
```

| Source → Target | Status |
|----------------|--------|
| Claude → Gemini | :green_circle: Lossless |
| Gemini → Claude | :green_circle: Lossless |
| Codex → Claude | :green_circle: Lossless |
| Claude → Codex | :green_circle: Lossless |
| OpenCode → Claude | :green_circle: Lossless |
| Claude → OpenCode | :yellow_circle: Partial |
| Codex → Gemini | :green_circle: Lossless |
| Gemini → Codex | :green_circle: Lossless |
| OpenCode → Gemini | :yellow_circle: Partial |
| OpenCode → Codex | :yellow_circle: Partial |
| Gemini → OpenCode | :yellow_circle: Partial |
| Codex → OpenCode | :yellow_circle: Partial |

:green_circle: Lossless · :yellow_circle: Partial · :white_circle: Pending — [Full matrix](docs/crossload-matrix.md)

### On the Roadmap

- Custom agent CLI support (bring your own agent binary with unified flag mapping)
- Directory navigation and workspace management
- PTY terminal middleware for session scripting

## Profiles

Profiles are TOML files in `~/.config/unleash/profiles/`. Each profile specifies an agent CLI, arguments, environment variables, and theme.

```toml
# ~/.config/unleash/profiles/work.toml
name = "work"
description = "Work profile with Claude"
agent_cli_path = "claude"
agent_args = []
theme = "blue"

[env]
ANTHROPIC_API_KEY = "sk-..."
```

Per-agent overrides allow a single profile to customize behavior for different agents:

```toml
[agents.claude]
extra_args = ["--effort", "high"]

[agents.codex]
extra_args = ["--full-auto"]
```

## TUI

The TUI (`unleash` with no arguments) provides:

- **Profile management**: Create, edit, duplicate, search profiles
- **Version management**: Browse, install, and switch agent versions
- **Settings**: Auto-update toggles, theme selection, animations

Navigate with `j/k` or arrows, `Enter` to select, `Esc` to go back, `?` for help.

## Plugins

All extended functionality is implemented as plugins in `plugins/bundled/`:

| Plugin | Description |
|--------|-------------|
| **auto-mode** | Autonomous operation via Stop hook enforcement |
| **mcp-refresh** | Detect MCP config changes and notify for reload |
| **process-restart** | Self-restart with session preservation |
| **hyprland-focus** | Window transparency on Hyprland during agent work |
| **omnihook** | Unified hook handler with voice input integration |

### Creating Plugins

```bash
mkdir -p plugins/my-plugin
```

```json
// plugins/my-plugin/plugin.json
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "What it does",
  "hooks": {
    "Stop": "./hooks/stop.sh"
  }
}
```

See the [plugin development guide](docs/internal/claude-code/plugin-development.md) for details.

## Installation

### One-liner (recommended)

```bash
curl -fsSL unleash.software/install | bash
```

Downloads a prebuilt binary for your platform (Linux x86_64/aarch64, macOS x86_64/aarch64), installs to `~/.local/bin`, and launches the interactive setup to pick your default agent.

For non-interactive installs (CI, scripts):

```bash
curl -fsSL unleash.software/install | bash -s -- --boring
```

### Docker (Sandboxed)

```bash
# One-time setup (installs gVisor, network isolation, pulls image)
sudo unleash sandbox setup

# Run an agent
unleash sandbox run claude
```

Or run the image directly:

```bash
docker run -it --rm -e ANTHROPIC_API_KEY marksverdhei/unleash
```

All 4 agent CLIs are pre-installed. See the [Docker + gVisor sandbox guide](docs/docker.md) for hardened setups with LAN isolation and named sandboxes.

### Build from source

Requires [Rust](https://rustup.rs/):

```bash
git clone https://github.com/heiervang-technologies/unleash.git
cd unleash
cargo build --release
./scripts/install.sh
```

Or force the installer to build from source instead of downloading:

```bash
BUILD_FROM_SOURCE=1 bash <(curl -fsSL unleash.software/install)
```

### Platform support

| Platform | Binary | Method |
|----------|--------|--------|
| Linux x86_64 | `unleash-linux-x86_64` | Static musl binary |
| Linux aarch64 | `unleash-linux-aarch64` | Static musl binary |
| macOS x86_64 | `unleash-macos-x86_64` | Native binary |
| macOS aarch64 | `unleash-macos-aarch64` | Native binary |

Linux binaries are statically linked (musl) — no glibc dependency. Works on any Linux including WSL, Alpine, and containers.

### Agent CLI dependencies

unleash itself has no dependencies beyond curl. Agent CLIs have their own:

| Agent | Install method | Requires |
|-------|---------------|----------|
| Claude Code | Native binary (GCS) | curl |
| Codex | Prebuilt binary (GitHub) | curl |
| Gemini CLI | npm | Node.js |
| OpenCode | Built-in upgrade / npm fallback | curl (or Node.js) |

If npm is missing when installing Gemini or OpenCode, unleash will offer to install Node.js via [nvm](https://github.com/nvm-sh/nvm).

### Authentication

Each agent CLI uses its own API key:

```bash
# Set keys as environment variables
export ANTHROPIC_API_KEY=sk-ant-...
export OPENAI_API_KEY=sk-...
export GEMINI_API_KEY=...

# Or use OAuth (Claude Code)
claude login
```

Verify with `unleash auth` or `unleash auth --verbose`.

## Architecture

```
unleash/
├── src/                    # Rust CLI & TUI
│   ├── cli.rs             # Argument parsing + polyfill flags
│   ├── polyfill.rs        # Unified flag → agent-specific translation
│   ├── launcher.rs        # Agent wrapper with restart/auto-mode
│   ├── updater.rs         # Parallel update orchestrator
│   ├── progress.rs        # Terminal progress bar renderer
│   ├── agents.rs          # Agent definitions + version management
│   ├── config.rs          # Profile + settings management
│   └── tui/               # Terminal UI (ratatui)
├── plugins/bundled/        # Plugin extensions
├── scripts/                # Install/uninstall scripts
└── docs/                   # Specs and guides
```

## Contributing

Contributions are very welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## Documentation

- [Getting Started](docs/getting-started.md)
- [CLI Reference](docs/cli-reference.md)
- [Profiles](docs/profiles.md)
- [Crossload Matrix](docs/crossload-matrix.md)
- [Docker + gVisor Sandbox](docs/docker.md)
- [Plugins](docs/plugins.md)
- [Configuration](docs/configuration.md)

## Links

- [Issue Tracker](https://github.com/heiervang-technologies/unleash/issues)
- [Claude Code](https://github.com/anthropics/claude-code) | [Codex](https://github.com/openai/codex) | [Gemini CLI](https://github.com/google-gemini/gemini-cli) | [OpenCode](https://github.com/opencode-ai/opencode)

---

Built by [Heiervang Technologies](https://github.com/heiervang-technologies)
