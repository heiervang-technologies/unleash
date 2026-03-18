# Unleash

<img width="720" height="480" alt="unleash" src="https://github.com/user-attachments/assets/0b8ff3af-90e8-4d7d-8204-33a159ae0835" />


<p align="center">
  <img src="demo-animation.gif" alt="Unleash - Smooth menu animations" width="900">
</p>

A powerful extension framework for Claude Code with auto-mode, version management, and plugin support.

## Quick Install

```bash
# Using gh CLI (recommended - handles auth automatically)
gh repo clone heiervang-technologies/unleash /tmp/unleash && bash /tmp/unleash/scripts/install.sh && rm -rf /tmp/unleash
```
Or with GitHub token if repo is still private:
```bash
# export GH_TOKEN=ghp_xxx
curl -fsSL -H "Authorization: token $GH_TOKEN" \
  https://raw.githubusercontent.com/heiervang-technologies/unleash/main/scripts/install-remote.sh | bash
```

This installs/updates both **Claude Code** and **Unleash**.

**After install:**
```bash
unleash          # Launch TUI interface (profiles & version management)
unleash claude   # Start Claude with unleash features
```

> **Already have it installed?** Run the same command to update to latest versions.

---
<p align="center">
  <img src="demo-tui.gif" alt="Unleash TUI Demo" width="800">
</p>
---

## Overview

**Unleash** is a wrapper around Anthropic's official [Claude Code](https://github.com/anthropics/claude-code) CLI that adds auto-mode, version management, and a plugin system — without modifying Claude Code itself.

This approach provides:
- **Zero upstream conflicts**: Uses Claude Code as-is via native binary or npm install
- **Auto-mode**: Stop hook + flag file system for autonomous operation (no cli.js patching)
- **Plugin ecosystem**: Add custom features, integrations, and workflows as plugins
- **Version management**: Install, switch, and manage Claude Code versions with whitelist/blacklist filtering
- **Team collaboration**: Share plugins across your organization

## Architecture

```mermaid
graph TD
    subgraph Unleash
        A[src/ - Rust TUI & CLI] --> B[Cargo.toml - Config & Versions]
        A --> C[scripts/ - Shell Installers/Wrappers]
        A --> D[docs/ - Documentation]
        A --> E[tests/ - Test Scripts]
        A --> F[plugins/bundled/ - Extension Layer]
    end

    subgraph Plugins
        F --> G[auto-mode]
        F --> H[mcp-refresh]
        F --> I[process-restart]
        F --> J[voice-output]
    end
```

### How It Works

Unleash wraps Claude Code (installed separately via native binary or npm) and extends it through:

```mermaid
graph LR
    subgraph Wrapper Layer
        W1[Rust TUI Profile/Version Manager]
        W2[Launch with --dangerously-skip-permissions]
        W3[Auto-mode via Stop Hook + Flags]
        W4[Plugin Loading via --plugin-dir]
    end

    subgraph Extension Layer
        E1[Custom functionality as plugins]
        E2[Organization Integrations]
        E3[Team Workflows & Automations]
    end

    WrapperLayer --> ExtensionLayer
```

## Extension Approach: Plugin-First

All customizations are implemented as plugins. This keeps the core clean and makes features:
- **Modular**: Enable/disable features independently
- **Portable**: Share plugins across repositories
- **Maintainable**: Update plugins without touching core code
- **Testable**: Each plugin is isolated and testable

### Available Plugins

- **auto-mode**: Autonomous operation mode for Claude
- **mcp-refresh**: Automatically detect MCP configuration changes and notify for reload
- **process-restart**: Restart Claude Code while preserving session state and conversation history
- **voice-output**: Multi-provider text-to-speech for Claude's responses (VibeVoice, OpenAI, ElevenLabs)

## Version Management

Unleash manages Claude Code versions with configurable filtering:

- **Blacklist mode** (default for Claude): All versions allowed except known-bad ones
- **Whitelist mode** (default for Codex): Only verified versions allowed
- Version lists are maintained in `Cargo.toml` and compiled into the binary

## Quick Start

### Prerequisites

- curl (for native Claude Code binary download) or Node.js/npm (fallback)
- Git
- Rust/Cargo (optional, for building TUI from source)
- Claude Pro or Max subscription (required for authentication)

### Headless Environments

If you're running in a headless environment (Docker containers, Kubernetes pods, CI/CD pipelines), build without TUI support to avoid terminal dependencies:

```bash
cargo build --release --no-default-features
```

This creates a minimal binary without crossterm/ratatui dependencies that works perfectly in non-interactive environments. All commands (`auth`, `version`, `go`) work normally - only the `tui` command is disabled.

### One-Line Installation (Recommended)

Install everything with a single command:

```bash
curl -fsSL https://raw.githubusercontent.com/heiervang-technologies/unleash/main/scripts/install-remote.sh | bash
```

This will:
- Install Claude Code (native binary preferred, npm fallback)
- Download the pre-built TUI binary
- Set up the `unleash` command

### Installation Options

#### Option 1: gh CLI (recommended for private repo)
```bash
# Clone, install, cleanup
gh repo clone heiervang-technologies/unleash /tmp/unleash && \
  bash /tmp/unleash/scripts/install.sh && \
  rm -rf /tmp/unleash

# With specific Claude Code version
gh repo clone heiervang-technologies/unleash /tmp/unleash && \
  bash /tmp/unleash/scripts/install.sh --claude-version 2.1.5 && \
  rm -rf /tmp/unleash
```

#### Option 2: curl with GitHub token
```bash
# Set your GitHub token (needs repo access)
export GH_TOKEN=ghp_xxxxxxxxxxxx

# Install latest
curl -fsSL -H "Authorization: token $GH_TOKEN" \
  https://raw.githubusercontent.com/heiervang-technologies/unleash/main/scripts/install-remote.sh | bash

# Install specific Claude Code version
CLAUDE_CODE_VERSION=2.1.5 curl -fsSL -H "Authorization: token $GH_TOKEN" \
  https://raw.githubusercontent.com/heiervang-technologies/unleash/main/scripts/install-remote.sh | bash
```

#### Option 3: Clone and build from source
```bash
# Clone (SSH for private repo)
git clone git@github.com:heiervang-technologies/unleash.git
cd unleash

# Build TUI and install
cargo build --release
./scripts/install.sh

# Or without TUI
./scripts/install.sh --no-build
```

### Authentication Setup

Unleash requires authentication with Claude Code. You have two options:

#### Option 1: OAuth Token (Recommended for Automation)

Generate a long-lived OAuth token and set it as an environment variable:

```bash
# Generate the token
claude setup-token

# Copy the output token and export it
export CLAUDE_CODE_OAUTH_TOKEN=<your-token-here>

# Add to your shell profile for persistence
echo 'export CLAUDE_CODE_OAUTH_TOKEN=<your-token-here>' >> ~/.bashrc
# or ~/.zshrc for zsh
```

**Advantages:**
- Works in headless/non-interactive environments
- Suitable for CI/CD pipelines and containers
- No browser authentication needed
- Token persists across sessions when exported in shell profile

**Note:** The OAuth token takes precedence over credentials stored in `~/.claude/.credentials.json`.

#### Option 2: Interactive Authentication

Run Claude Code once to authenticate via browser:

```bash
claude
# Follow the browser authentication flow
# Credentials will be stored in ~/.claude/.credentials.json (Linux/Ubuntu)
# or macOS Keychain (macOS)
```

#### Verifying Authentication

Unleash automatically checks for authentication on startup. You can also verify authentication status manually:

```bash
# Quick check
unleash auth
# ✓ Authentication configured

# Detailed check
unleash auth --verbose
# ✓ Authentication configured
#
# Authentication method:
#   • OAuth token from CLAUDE_CODE_OAUTH_TOKEN environment variable
#   • Token preview: sk-ant-oat...g-1JzO1QAA
#
# Status: Ready to use Claude Code

# JSON output (for scripting)
unleash auth --json
# {"authenticated":true,"method":"oauth_token","details":null}

# Quiet mode (only exit code, no output)
unleash auth -q
# (no output, only exit code: 0=success, 1=failure)
```

The auth command verifies authentication without launching Claude, making it perfect for:
- CI/CD pipelines and automation scripts
- Pre-flight checks before running Claude
- Debugging authentication issues
- Integration with other tools

For more details, see the [Claude Code IAM documentation](https://code.claude.com/docs/en/iam).

### Add to PATH

After installation, add `~/.local/bin` to your PATH if not already:

```bash
export PATH="$HOME/.local/bin:$PATH"
# Add to your shell profile (~/.bashrc or ~/.zshrc) for persistence
```

## CLI Usage

### Command Overview

```bash
unleash                    # Launch TUI interface (default)
unleash claude             # Start Claude with unleash features
unleash claude --auto      # Start in autonomous mode
unleash auth               # Check authentication status
unleash auth -v            # Check with detailed information
unleash auth -q            # Check quietly (only exit code)
unleash auth --json        # Output as JSON for scripting
unleash version            # Show installed version
unleash version --list     # List available versions
restart-claude             # Restart Claude (preserves session)
exit-claude                # Exit Claude cleanly
```

### Configuration Options

#### Stop Prompt Customization

Customize the message Claude receives when auto-mode blocks it from exiting:

```bash
# Set a custom prompt
unleash claude --stop-prompt="Keep working until tests pass!"

# Edit with your $EDITOR
unleash claude --stop-prompt-edit

# Reset to default
unleash claude --stop-prompt-clear
```

You can also configure this via the TUI:
```bash
unleash  # Navigate to Settings > Stop Prompt
```

The prompt is stored globally in `~/.config/unleash/config.toml` and applies to all future auto-mode sessions.

**Priority order:**
1. Session-specific override (programmatic)
2. Global config (CLI/TUI)
3. Default hardcoded message

For detailed configuration options, see [docs/extensions/configuration.md](docs/extensions/configuration.md).

## TUI Features

The TUI (`unleash`) provides a graphical interface for managing Unleash:

### Profile Management
- Create and manage environment profiles
- Store API keys and environment variables securely
- Switch between profiles quickly

### Claude Code Version Management
- View currently installed Claude Code version
- Browse available versions from npm registry and GCS
- **Switch between versions** with a single selection
- Whitelist/blacklist filtering to avoid known-bad versions

Navigate with:
- `j/k` or `↑/↓` - Move selection
- `Enter` - Select/Confirm
- `Esc` - Go back
- `?` - Help

## How to Add Plugins

### Creating a New Plugin

1. **Create plugin directory**
   ```bash
   mkdir -p plugins/my-plugin
   cd plugins/my-plugin
   ```

2. **Add plugin manifest** (`plugin.json`)
   ```json
   {
     "name": "my-plugin",
     "version": "1.0.0",
     "description": "Description of what your plugin does",
     "author": "Your Name",
     "main": "index.js",
     "hooks": {
       "pre-command": "./hooks/pre-command.js",
       "post-command": "./hooks/post-command.js"
     }
   }
   ```

3. **Implement plugin logic** (`index.js`)
   ```javascript
   module.exports = {
     name: 'my-plugin',

     async initialize(context) {
       // Setup code
       console.log('Plugin initialized');
     },

     async execute(command, args) {
       // Main plugin logic
       return { success: true };
     }
   };
   ```

4. **Enable in configuration**

   Add to `.claude/settings.json`:
   ```json
   {
     "plugins": {
       "enabled": ["my-plugin"]
     }
   }
   ```

### Plugin Development Best Practices

- Keep plugins focused on a single responsibility
- Document all configuration options
- Include tests for your plugin
- Follow semantic versioning
- Add a README.md to your plugin directory

See `docs/extensions/` for detailed plugin development guides.

## Documentation

- **Plugin Development**: `docs/extensions/plugin-development.md`
- **MCP Refresh & Process Restart**: `docs/extensions/restart-refresh.md`
- **GitHub Integration**: `docs/extensions/snail-integration.md`
- **Agent Instructions**: `CLAUDE.md`

## Contributing

We welcome contributions to both the plugin ecosystem and the wrapper infrastructure!

### Contribution Guidelines

1. **For new plugins:**
   - Create a new directory in `plugins/`
   - Include a README.md with usage instructions
   - Add tests for your plugin
   - Submit a PR with the plugin

2. **For wrapper/TUI improvements:**
   - Focus on the Rust source in `src/`
   - Update documentation
   - Add tests for new functionality

3. **For upstream improvements:**
   - Contribute directly to [anthropics/claude-code](https://github.com/anthropics/claude-code)

### Development Workflow

```bash
# 1. Create feature branch
git checkout -b feature/my-enhancement

# 2. Make changes
# - Add plugins in plugins/bundled/
# - Modify Rust source in src/
# - Update configuration

# 3. Test your changes
cargo test

# 4. Commit with conventional commits
git commit -m "feat: add new plugin for X"

# 5. Push and create PR
git push origin feature/my-enhancement
```

### Code of Conduct

- Be respectful and inclusive
- Provide constructive feedback
- Help others learn and grow
- Maintain professional communication

## Troubleshooting

### Plugin not loading

1. Check `.claude/settings.json` - is it in `enabled` array?
2. Verify plugin structure - does it have `plugin.json` and `index.js`?
3. Check plugin logs for errors

## Organization

This repository is maintained by **Heiervang Technologies**.

- **Organization**: heiervang-technologies
- **GitHub**: [@heiervang-technologies](https://github.com/heiervang-technologies)

## License

This project maintains the same license as the upstream Claude Code project. See `LICENSE.md` for details.

## Acknowledgments

- **Anthropic** for creating and maintaining Claude Code
- **Heiervang Technologies** for the plugin architecture and wrapper infrastructure
- All contributors to the plugin ecosystem

## Links

- [Upstream Repository (anthropics/claude-code)](https://github.com/anthropics/claude-code)
- [Plugin Development Guide](docs/extensions/plugin-development.md)
- [Issue Tracker](https://github.com/heiervang-technologies/unleash/issues)
- [Discussions](https://github.com/heiervang-technologies/unleash/discussions)

---

**Ready to extend Claude Code?** Start by exploring the available plugins or create your own!
