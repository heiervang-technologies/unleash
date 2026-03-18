# Agent Instructions for Unleash

This file provides context and instructions for AI agents (including Claude, GitHub Copilot, and Snail agents) working on the Unleash repository.

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
restart-claude
```

Or with a custom message to receive after restart:
```bash
restart-claude "Continue working on the feature"
```

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
| `scripts/restart-claude` | Restart command |
| `scripts/exit-claude` | Exit without restart |

## Repository Overview

**Unleash** is a wrapper around Anthropic's official Claude Code CLI that adds auto-mode, version management, and a plugin system — without modifying Claude Code itself.

### Key Principles

1. **Claude Code is external** - Installed separately via native binary (GCS) or npm; never bundled or modified
2. **All extensions are plugins** - Custom functionality goes in `plugins/` directory
3. **Configuration over code** - Use `.claude/settings.json` for preferences
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
│   ├── restart-claude          # Restart command
│   └── exit-claude             # Exit command
├── plugins/bundled/             # Plugin extensions
│   ├── auto-mode/              # Autonomous operation mode
│   ├── mcp-refresh/            # MCP config change detection
│   ├── process-restart/        # Self-restart hooks and commands
│   └── voice-output/           # Text-to-speech output
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
- **Configuration changes**: Edit `.claude/settings.json`
- **Version lists**: Edit `Cargo.toml` (whitelist/blacklist sections)
- **Documentation**: Update `README.md` or `docs/extensions/`

**NOTE**: Claude Code is installed separately (via native binary or npm). This repo does not contain or modify Claude Code source.

## Plugin Development Workflow

### When a User Asks for a New Feature

1. **Assess the request**
   - Does this belong in upstream Claude Code? (Suggest they contribute to Anthropic)
   - Is this organization-specific? (Create/update a plugin)
   - Is this configuration? (Update `.claude/settings.json`)

2. **Create or identify target plugin**
   ```bash
   # New plugin
   mkdir -p plugins/new-feature-name

   # Or extend existing
   cd plugins/existing-plugin
   ```

3. **Implement plugin structure**
   ```
   plugins/my-plugin/
   ├── plugin.json          # Manifest
   ├── index.js             # Main entry point
   ├── README.md            # Documentation
   ├── hooks/               # Lifecycle hooks
   │   ├── pre-command.js
   │   └── post-command.js
   └── tests/               # Plugin tests
       └── index.test.js
   ```

4. **Test the plugin**
   - Create tests in `tests/` directory
   - Test with Claude Code CLI
   - Verify no conflicts with other plugins

5. **Update configuration**
   - Add to `.claude/settings.json` if needed
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

When asked to add a feature, create a plugin:

```javascript
// plugins/my-feature/plugin.json
{
  "name": "my-feature",
  "version": "1.0.0",
  "description": "Does something useful",
  "author": "Heiervang Technologies",
  "main": "index.js",
  "hooks": {
    "pre-command": "./hooks/pre-command.js"
  }
}

// plugins/my-feature/index.js
module.exports = {
  name: 'my-feature',

  async initialize(context) {
    // Setup logic
    console.log('My feature initialized');
  },

  async execute(command, args) {
    // Main plugin logic
    return { success: true };
  }
};

// plugins/my-feature/README.md
# My Feature Plugin

Description of what this plugin does.

## Configuration

Add to `.claude/settings.json`:
```json
{
  "plugins": {
    "enabled": ["my-feature"]
  }
}
```

## Usage

Describe how to use the plugin.
```

## Working with Snail Integration

This repository is integrated with Heiervang's Snail AI agent system.

### Snail-Specific Plugins

- **auto-mode**: Core autonomous capability
- **process-restart**: Session stability for long-running workflows

### When Mentioned in Issues/PRs

1. The `mention-trigger.yml` workflow activates
2. Snail agent receives context about the repository
3. Use this CLAUDE.md file to understand the codebase
4. Respond appropriately based on the request

### Snail Best Practices

- Always check `.claude/settings.json` for configuration
- Reference documentation in `docs/extensions/`
- Create issues for complex feature requests
- Link to relevant plugin documentation
- Suggest plugin creation for new features

## Code Style and Standards

### General Guidelines

- Use conventional commits: `feat:`, `fix:`, `docs:`, `chore:`, etc.
- Keep commits focused and atomic
- Write descriptive commit messages
- Include tests for new functionality
- Update documentation with code changes

### Plugin-Specific Standards

```javascript
// Use clear, descriptive variable names
const pluginConfiguration = loadConfig();

// Add JSDoc comments for public APIs
/**
 * Initializes the plugin with the given context
 * @param {Object} context - The plugin context
 * @returns {Promise<void>}
 */
async initialize(context) {
  // Implementation
}

// Handle errors gracefully
try {
  await executePlugin();
} catch (error) {
  console.error(`Plugin failed: ${error.message}`);
  return { success: false, error };
}
```

### Testing Standards

```javascript
// plugins/my-plugin/tests/index.test.js
describe('MyPlugin', () => {
  it('should initialize correctly', async () => {
    const plugin = require('../index.js');
    const result = await plugin.initialize({});
    expect(result).toBeDefined();
  });

  it('should execute command', async () => {
    const plugin = require('../index.js');
    const result = await plugin.execute('test', []);
    expect(result.success).toBe(true);
  });
});
```

## Common Tasks and Patterns

### Adding a New Plugin

1. Create directory: `mkdir -p plugins/plugin-name`
2. Add manifest: `plugins/plugin-name/plugin.json`
3. Implement logic: `plugins/plugin-name/index.js`
4. Document: `plugins/plugin-name/README.md`
5. Enable: Update `.claude/settings.json`
6. Test: Add to `plugins/plugin-name/tests/`

### Updating Configuration

1. Read: `.claude/settings.json`
2. Modify: Add/remove from `plugins.enabled` array
3. Validate: Ensure JSON is valid
4. Document: Update README.md if user-facing

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
1. Is it in `.claude/settings.json` `enabled` array?
2. Does `plugin.json` exist and is it valid JSON?
3. Is `index.js` present with correct exports?
4. Are there errors in plugin logs?

**Solution:**
```bash
# Validate plugin structure
ls -la plugins/problem-plugin/

# Check configuration
cat .claude/settings.json

# Review logs
npm start --verbose  # or bun start
```

## Links to Documentation

### Internal Documentation
- **Plugin Development**: `docs/extensions/plugin-development.md`
- **GitHub Integration**: `docs/extensions/snail-integration.md`

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

# View enabled plugins
cat .claude/settings.json | grep -A 10 "plugins"
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

1. Checking plugin configuration in `.claude/settings.json`
2. Reviewing relevant plugin code in `plugins/`
3. Testing the scenario
4. Proposing a solution (plugin update or new plugin)

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
