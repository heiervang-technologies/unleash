# Claude Unleashed

A powerful extension framework for Claude Code that maintains upstream compatibility while adding custom functionality through a plugin-first architecture.

## Overview

**Claude Unleashed** is a fork of Anthropic's official [Claude Code](https://github.com/anthropics/claude-code) CLI that enables extensibility without modifying the core codebase. Instead of patching the upstream code directly, we maintain the original repository as a Git submodule and extend it through a comprehensive plugin system.

This approach provides:
- **Zero upstream conflicts**: Pull updates from Anthropic's repository without merge conflicts
- **Clean separation**: Core functionality remains untouched in the submodule
- **Plugin ecosystem**: Add custom features, integrations, and workflows as plugins
- **Team collaboration**: Share plugins across your organization
- **Daily sync**: Automated workflows keep you up-to-date with upstream changes

## Architecture

```
claude-unleashed/
├── src/                          # Rust TUI (main entry point)
│   └── main.rs
├── Cargo.toml                    # TUI build configuration
├── scripts/                      # Shell scripts
│   ├── install.sh               # Installation script
│   ├── wrapper.sh               # Bash wrapper for self-restart
│   ├── restart-claude           # Restart command
│   ├── exit-claude              # Exit command
│   ├── patch-claude.sh          # Apply Claude Code patches
│   ├── unpatch-claude.sh        # Remove patches
│   └── check-and-patch.sh       # Auto-patch on version change
├── plugins/unleashed/            # Plugin extensions
│   ├── auto-mode/               # Autonomous operation mode
│   ├── mcp-refresh/             # MCP config change detection
│   ├── process-restart/         # Self-restart capability
│   └── voice-output/            # Text-to-speech output
├── claude-code/                  # Git submodule (upstream, never modify)
├── docs/                         # Documentation
└── tests/                        # Test scripts
```

### The Three-Layer Approach

1. **Upstream Layer** (`claude-code/` submodule)
   - Official Anthropic claude-code repository
   - Remains pristine and untouched
   - Updated daily via automated sync

2. **Fork Layer** (this repository)
   - Manages the submodule reference
   - Hosts plugin infrastructure
   - Provides organizational configuration

3. **Extension Layer** (`plugins/`)
   - Custom functionality as self-contained plugins
   - Organization-specific integrations
   - Team workflows and automations

## Extension Approach: Plugin-First

All customizations are implemented as plugins. This keeps the core clean and makes features:
- **Modular**: Enable/disable features independently
- **Portable**: Share plugins across repositories
- **Maintainable**: Update plugins without touching core code
- **Testable**: Each plugin is isolated and testable

### Available Plugins

- **heiervang-snail-integration**: Integration with Heiervang's Snail AI agent system
- **heiervang-workflows**: Custom GitHub Actions workflows for team automation
- **commit-commands**: Enhanced commit command shortcuts and templates
- **feature-dev**: Feature branch workflow automation
- **code-review**: Automated code review helpers and PR templates
- **mcp-refresh**: Automatically detect MCP configuration changes and notify for reload
- **process-restart**: Restart Claude Code while preserving session state and conversation history
- **voice-output**: Multi-provider text-to-speech for Claude's responses (VibeVoice, OpenAI, ElevenLabs)

## Daily Sync Workflow

Claude Unleashed automatically stays in sync with upstream changes:

```
Every day at 2 AM UTC:
┌──────────────────────────────────────────────────────┐
│ 1. Fetch latest from anthropics/claude-code         │
│ 2. Update submodule reference                       │
│ 3. Run compatibility tests                          │
│ 4. Create PR if changes detected                    │
│ 5. Auto-merge if tests pass                         │
└──────────────────────────────────────────────────────┘
```

This ensures you benefit from:
- Latest bug fixes from Anthropic
- New Claude Code features
- Security patches
- Performance improvements

All while maintaining your custom plugins and configurations.

## Quick Start

### Prerequisites

- [Claude Code](https://github.com/anthropics/claude-code) installed (`npm install -g @anthropic-ai/claude-code`)
- Git
- Rust/Cargo (optional, for TUI)

### Installation

1. **Clone the repository**
   ```bash
   git clone https://github.com/heiervang-technologies/claude-unleashed.git
   cd claude-unleashed
   ```

2. **Run the installer**
   ```bash
   ./scripts/install.sh
   ```

   This will:
   - Build the TUI (if Cargo is available)
   - Create symlinks in `~/.local/bin/`
   - Patch Claude Code with additional features

3. **Add to PATH** (if needed)
   ```bash
   export PATH="$HOME/.local/bin:$PATH"
   ```

4. **Start using**
   ```bash
   claude-unleashed
   # or the short alias:
   cu
   ```

### Manual Installation

If you prefer not to use the installer:

```bash
# Create symlinks manually
ln -sf ~/claude-unleashed/scripts/wrapper.sh ~/.local/bin/claude-unleashed
ln -sf ~/claude-unleashed/scripts/restart-claude ~/.local/bin/
ln -sf ~/claude-unleashed/scripts/exit-claude ~/.local/bin/

# Patch Claude Code
./scripts/patch-claude.sh

# Optional: Build TUI
cargo build --release
cp target/release/claude-unleashed ~/.local/bin/claude-unleashed-tui
```

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
- **Upstream Sync**: `docs/extensions/upstream-sync.md`
- **GitHub Integration**: `docs/extensions/github-integration.md`
- **Agent Instructions**: `CLAUDE.md`
- **Upstream Docs**: `claude-code/README.md`

## Contributing

We welcome contributions to both the plugin ecosystem and the fork infrastructure!

### Contribution Guidelines

1. **For new plugins:**
   - Create a new directory in `plugins/`
   - Include a README.md with usage instructions
   - Add tests for your plugin
   - Submit a PR with the plugin

2. **For fork improvements:**
   - Never modify code in `claude-code/` (it's a submodule)
   - Focus on plugin infrastructure and tooling
   - Update documentation
   - Ensure daily sync workflow still functions

3. **For upstream improvements:**
   - Contribute directly to [anthropics/claude-code](https://github.com/anthropics/claude-code)
   - Benefits will flow back through daily sync

### Development Workflow

```bash
# 1. Create feature branch
git checkout -b feature/my-enhancement

# 2. Make changes (outside claude-code/ submodule)
# - Add plugins
# - Update configuration
# - Improve tooling

# 3. Test your changes
npm test

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

## Sync Process

### Manual Sync

If you need to sync with upstream manually:

```bash
# Update submodule to latest upstream
git submodule update --remote claude-code

# Commit the new submodule reference
git add claude-code
git commit -m "chore: sync with upstream claude-code"
git push
```

### Automated Sync

The `.github/workflows/sync-upstream.yml` workflow handles this automatically:
- Runs daily at 2 AM UTC
- Creates PR if updates available
- Runs test suite
- Auto-merges if tests pass

## Troubleshooting

### Submodule is empty or outdated

```bash
git submodule update --init --recursive
```

### Plugin not loading

1. Check `.claude/settings.json` - is it in `enabled` array?
2. Verify plugin structure - does it have `plugin.json` and `index.js`?
3. Check plugin logs for errors

### Upstream sync conflicts

This should be rare since we don't modify the submodule. If it happens:
```bash
cd claude-code
git status  # Check for local modifications
git restore .  # Discard if any
cd ..
git submodule update --remote claude-code
```

## Organization

This repository is maintained by **Heiervang Technologies**.

- **Organization**: heiervang-technologies
- **GitHub**: [@heiervang-technologies](https://github.com/heiervang-technologies)

## License

This fork maintains the same license as the upstream project. See `LICENSE.md` for details.

The upstream Claude Code is licensed by Anthropic. See `claude-code/LICENSE.md` for upstream license information.

## Acknowledgments

- **Anthropic** for creating and maintaining Claude Code
- **Heiervang Technologies** for the plugin architecture and fork infrastructure
- All contributors to the plugin ecosystem

## Links

- [Upstream Repository (anthropics/claude-code)](https://github.com/anthropics/claude-code)
- [Plugin Development Guide](docs/extensions/plugin-development.md)
- [Issue Tracker](https://github.com/heiervang-technologies/claude-unleashed/issues)
- [Discussions](https://github.com/heiervang-technologies/claude-unleashed/discussions)

---

**Ready to extend Claude Code?** Start by exploring the available plugins or create your own!
