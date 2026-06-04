# Agent Instructions for Unleash

This file provides context and instructions for AI agents working on the Unleash repository.

## Self-Restart Capability

**IMPORTANT**: If you are running under the `unleash` wrapper, you can restart yourself to reload MCP servers, apply configuration changes, or recover from issues.

### How to Check if You Can Restart

Check the environment variable:
```bash
echo $AGENT_UNLEASH
```
If it returns `1`, you are running under the wrapper and can restart.

### How to Restart Yourself

Run this command via Bash:
```bash
unleash-refresh
```

Or with a custom message to receive after restart:
```bash
unleash-refresh "Continue working on the feature"
```

> **Note:** The old aliases `restart-claude` and `exit-claude` have been removed. Use `unleash-refresh` and `unleash-exit`.

### What Happens When You Restart

1. Your session is preserved (`--continue` flag added automatically)
2. You receive the message "RESURRECTED." (or your custom message)
3. MCP servers are reloaded with current configuration
4. You can continue where you left off

### When to Restart

- After MCP configuration changes (`.mcp.json` modified)
- When MCP servers become unresponsive
- To apply new plugin settings
- When instructed by the user

### Files Reference

| File | Purpose |
|------|---------|
| `scripts/unleash-refresh` | Restart command |
| `scripts/unleash-exit` | Exit without restart |

## Repository Overview

**Unleash** is a wrapper around Anthropic's official Claude Code CLI that adds auto-mode, version management, and a plugin system — without modifying Claude Code itself.

### Key Principles

1. **Claude Code is external** - Installed separately via native binary (GCS) or npm; never bundled or modified
2. **All extensions are plugins** - Custom functionality goes in `plugins/` directory
3. **Configuration over code** - Use profiles (`~/.config/unleash/profiles/`) and `--plugin-dir` for preferences
4. **Auto-mode via hooks** - Stop hook + flag file system, not cli.js patching
5. **Plugin isolation** - Each plugin is self-contained and independently testable

## Repository Structure

```
unleash/
├── src/                         # Rust TUI & CLI source (main entry point)
│   ├── bin/                     # CLI entrypoints
│   └── lib.rs                   # Core logic
├── Cargo.toml                   # Build configuration + version lists
├── scripts/                     # All shell scripts consolidated here
│   ├── install.sh              # Installation script
│   ├── install-remote.sh       # Remote one-line installer
│   ├── unleash-refresh         # Restart command
│   └── unleash-exit            # Exit command
├── plugins/bundled/             # Plugin extensions
│   ├── auto-mode/              # Autonomous operation mode
│   ├── hyprland-focus/         # Window transparency for Hyprland
│   ├── mcp-refresh/            # MCP config change detection
│   ├── omnihook/               # Universal hook → low-latency voice integration
│   ├── process-restart/        # Self-restart hooks and commands
│   ├── supercompact/           # Entity-preservation conversation compaction
│   └── token-usage/            # Cross-CLI token + cost accounting
├── docs/                        # Documentation
├── tests/                       # Test scripts
├── .github/workflows/           # CI/CD workflows
└── CLAUDE.md                    # This file - agent instructions
```

## Understanding the Architecture

### Two-Layer Design

1. **Wrapper Layer** (this repository)
   - Rust TUI for profile and version management
   - Launches Claude Code with `--dangerously-skip-permissions`
   - Auto-mode via Stop hook + flag file system
   - Plugin loading via `--plugin-dir`
   - Version management (install, switch, whitelist/blacklist)

2. **Extension Layer** (`plugins/`)
   - Custom functionality
   - Team-specific integrations
   - Workflow automations
   - Each plugin is independent

### Why This Matters for Agents

When working on this repository:

- **Adding features**: Create or modify plugins in `plugins/`
- **TUI/CLI changes**: Modify Rust source in `src/`
- **Configuration changes**: Edit profiles in `~/.config/unleash/profiles/`
- **Version lists**: Edit `Cargo.toml` (whitelist/blacklist sections)
- **Documentation**: Update `README.md` or `docs/extensions/`

**NOTE**: Claude Code is installed separately (via native binary or npm). This repo does not contain or modify Claude Code source.

## Plugin Development Workflow

### When a User Asks for a New Feature

1. **Assess the request**
   - Does this belong in upstream Claude Code? (Suggest they contribute to Anthropic)
   - Is this organization-specific? (Create/update a plugin)
   - Is this configuration? (Update profiles or plugin config)

2. **Create or identify target plugin**
   ```bash
   # New plugin
   mkdir -p plugins/new-feature-name

   # Or extend existing
   cd plugins/existing-plugin
   ```

3. **Implement plugin structure** — Claude Code plugins are config + scripts, not Node.js modules:
   ```
   plugins/my-plugin/
   ├── .claude-plugin/
   │   └── plugin.json      # Manifest (Claude Code reads from here)
   ├── commands/            # Slash commands (*.md files), optional
   ├── hooks/               # Lifecycle hooks, optional
   │   ├── hooks.json       # Event → script mapping
   │   └── *.sh             # Hook scripts (bash, python, anything executable)
   ├── scripts/             # Helper scripts called by hooks/commands, optional
   └── README.md
   ```

4. **Test the plugin**
   - Verify the manifest parses (`jq . plugins/my-plugin/.claude-plugin/plugin.json`)
   - Smoke-test hooks by sourcing them with the env vars Claude Code provides
     (`$CLAUDE_PLUGIN_ROOT`, `$CLAUDE_PROJECT_DIR`, etc.)
   - Launch via `unleash` and verify the plugin loads (no error in stderr)
   - Verify no conflicts with other plugins

5. **Update configuration**
   - Document in plugin README.md
   - Update main README.md if user-facing

### Plugin Development Guidelines

**DO:**
- Keep plugins focused and single-purpose
- Document all configuration options
- Include tests for your plugin
- Use semantic versioning
- Add comprehensive README.md to plugin directory
- Follow existing plugin patterns

**DON'T:**
- Modify Claude Code source files (it's installed externally)
- Create plugins that depend on specific upstream versions
- Hardcode organization-specific values (use config)
- Create circular dependencies between plugins

### Example: Creating a Simple Plugin

When asked to add a feature, create a plugin. Manifest, hook config, and a hook script — three files:

`plugins/my-feature/.claude-plugin/plugin.json`:
```json
{
  "name": "my-feature",
  "description": "Does something useful on every Stop event",
  "version": "0.1.0",
  "author": {
    "name": "Heiervang Technologies",
    "email": "support@heiervang.com"
  }
}
```

`plugins/my-feature/hooks/hooks.json`:
```json
{
  "description": "Logs every Stop event",
  "hooks": {
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/hooks/on-stop.sh"
          }
        ]
      }
    ]
  }
}
```

`plugins/my-feature/hooks/on-stop.sh`:
```bash
#!/usr/bin/env bash
# Claude Code sets CLAUDE_PLUGIN_ROOT, CLAUDE_PROJECT_DIR, and pipes the
# event payload as JSON on stdin. Read it with jq if you need fields.
set -euo pipefail
payload=$(cat)
echo "[my-feature] stop event: $(echo "$payload" | jq -c .session_id)" >> ~/.cache/my-feature.log
```

Optional settings live in the manifest under `"settings"` — see
`plugins/bundled/supercompact/.claude-plugin/plugin.json` for the schema
(`type`, `choices`/`min`/`max`, `default`, `label`, `description`). They
surface in the unleash TUI as toggles / inputs and are passed to hooks via
`PLUGIN_SETTING_<KEY>` environment variables.

## Code Style and Standards

### General Guidelines

- Use conventional commits: `feat:`, `fix:`, `docs:`, `chore:`, etc.
- Keep commits focused and atomic
- Write descriptive commit messages
- Include tests for new functionality
- Update documentation with code changes

### Plugin-Specific Standards

```bash
#!/usr/bin/env bash
# Hooks: always set strict mode, always read the JSON payload from stdin once,
# always reference $CLAUDE_PLUGIN_ROOT (set by Claude Code) instead of hardcoding
# the bundled path. Fail loud on the rare error — Claude Code surfaces non-zero
# exit codes in the session log.
set -euo pipefail
payload=$(cat)
session_id=$(echo "$payload" | jq -r .session_id)

# Settings declared in plugin.json arrive as PLUGIN_SETTING_<UPPER_KEY>:
mode="${PLUGIN_SETTING_MODE:-auto}"
```

### Testing Standards

Bundled plugins are exercised by `tests/` in this repo (shell harnesses, see
`tests/test-plugins.sh` for the pattern). Add a per-plugin smoke test that:

```bash
# tests/test_my_feature.sh
#!/usr/bin/env bash
set -euo pipefail
plugin_dir="$(git rev-parse --show-toplevel)/plugins/bundled/my-feature"

# 1. Manifest parses + has required fields
jq -e '.name and .description and .version' "$plugin_dir/.claude-plugin/plugin.json" >/dev/null

# 2. Each hook script is executable
find "$plugin_dir/hooks" -name '*.sh' -exec test -x {} \;

# 3. Hooks survive an empty JSON payload (catches missing `jq -r` defaults)
CLAUDE_PLUGIN_ROOT="$plugin_dir" \
  bash "$plugin_dir/hooks/on-stop.sh" <<<'{"session_id":"smoke"}'
```

## Common Tasks and Patterns

### Adding a New Plugin

1. Create directory: `mkdir -p plugins/bundled/plugin-name/{.claude-plugin,hooks,commands}`
2. Add manifest: `plugins/bundled/plugin-name/.claude-plugin/plugin.json`
3. Wire hooks (if any): `plugins/bundled/plugin-name/hooks/hooks.json` + executable scripts
4. Add slash commands (if any) as markdown files in `commands/`
5. Document: `plugins/bundled/plugin-name/README.md`
6. Add a smoke test in `tests/` (see `tests/test-plugins.sh` for the pattern)

### Investigating Upstream Changes

1. Check Claude Code changelog: `claude --version`
2. Review changes: `git diff HEAD~1`
3. Test compatibility: Run plugin tests
4. Update plugins: Adapt if needed

### Creating Documentation

1. Plugin README: `plugins/plugin-name/README.md`
2. Extension guides: `docs/extensions/`
3. Main README: Update if user-facing feature
4. This file: Update if affecting agent workflow

## Troubleshooting Guide

### Plugin Not Loading

**Check:**
1. Does `.claude-plugin/plugin.json` exist and parse as JSON? (`jq .` it)
2. Does the manifest have at least `name`, `description`, `version`, `author`?
3. Are `hooks/hooks.json` event keys spelled correctly (`Stop`, `PreCompact`, `UserPromptSubmit`, …)?
4. Are hook scripts marked executable (`chmod +x`)?
5. Does `unleash` log a warning at launch? (Hook sync prints "Warning: …" when settings.json can't be merged.)

**Solution:**
```bash
# Walk the plugin tree:
find plugins/bundled/problem-plugin -maxdepth 3 -type f -o -type d

# Validate the manifest:
jq -e '.name and .description and .version' plugins/bundled/problem-plugin/.claude-plugin/plugin.json
```

## Links to Documentation

### Internal Documentation
- **Plugin Development**: `docs/internal/claude-code/plugin-development.md`

### External Resources
- **Upstream Repository**: [anthropics/claude-code](https://github.com/anthropics/claude-code)
- **Claude API Docs**: [Anthropic Documentation](https://docs.anthropic.com/)
- **Organization**: [heiervang-technologies](https://github.com/heiervang-technologies)

## Quick Reference Commands

```bash
# Create new plugin
mkdir -p plugins/my-plugin && cd plugins/my-plugin

# Build and test
cargo build --release
cargo test

# List bundled plugins
ls plugins/bundled/
```

## Agent Response Templates

### When Asked to Add a Feature

```markdown
I'll create a new plugin for this feature to maintain separation from the upstream Claude Code.

1. Creating plugin structure in `plugins/feature-name/`
2. Implementing functionality
3. Adding tests
4. Updating configuration

This approach ensures:
- No conflicts with upstream updates
- Easy to enable/disable
- Isolated and testable
```

### When Investigating Issues

```markdown
I'll investigate this issue systematically:

1. Reviewing relevant plugin code in `plugins/`
2. Testing the scenario
3. Proposing a solution (plugin update or new plugin)

Let me start by examining...
```

## Final Notes

- **Think plugin-first**: Always consider if a plugin is the right solution
- **Respect the architecture**: The wrapper + plugin design is intentional
- **Document everything**: Future agents and users will thank you
- **Test thoroughly**: Plugins should be reliable and well-tested

When in doubt, create a plugin.

---

**For questions or clarifications**, refer to the main README.md or create a discussion in the repository.
