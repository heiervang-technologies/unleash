# Agent Instructions for Claude Unleashed

This file provides context and instructions for AI agents (including Claude, GitHub Copilot, and Snail agents) working on the Claude Unleashed repository.

## Self-Restart Capability

**IMPORTANT**: If you are running under the `claude-unleashed` wrapper, you can restart yourself to reload MCP servers, apply configuration changes, or recover from issues.

### How to Check if You Can Restart

Check the environment variable:
```bash
echo $CLAUDE_UNLEASHED
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
| `scripts/wrapper.sh` | The wrapper script |

## Repository Overview

**Claude Unleashed** is a fork of Anthropic's official Claude Code CLI that extends functionality through a plugin-first architecture while maintaining zero-conflict upstream synchronization.

### Key Principles

1. **Never modify the `claude-code/` submodule** - It contains the upstream Anthropic code and must remain pristine
2. **All extensions are plugins** - Custom functionality goes in `plugins/` directory
3. **Configuration over code** - Use `.claude/settings.json` for preferences
4. **Daily upstream sync** - Changes from Anthropic flow in automatically
5. **Plugin isolation** - Each plugin is self-contained and independently testable

## Repository Structure

```
claude-unleashed/
├── src/                         # Rust TUI source (main entry point)
│   └── main.rs
├── Cargo.toml                   # TUI build configuration
├── scripts/                     # All shell scripts consolidated here
│   ├── install.sh              # Installation script
│   ├── wrapper.sh              # Bash wrapper for self-restart
│   ├── restart-claude          # Restart command
│   ├── exit-claude             # Exit command
│   ├── patch-claude.sh         # Apply Claude Code patches
│   ├── unpatch-claude.sh       # Remove patches
│   └── check-and-patch.sh      # Auto-patch on version change
├── plugins/unleashed/           # Plugin extensions
│   ├── auto-mode/              # Autonomous operation mode
│   ├── mcp-refresh/            # MCP config change detection
│   ├── process-restart/        # Self-restart hooks and commands
│   └── voice-output/           # Text-to-speech output
├── claude-code/                 # Git submodule (DO NOT MODIFY)
├── docs/                        # Documentation
├── tests/                       # Test scripts
├── .github/workflows/           # CI/CD workflows
└── CLAUDE.md                    # This file - agent instructions
```

## Understanding the Fork Structure

### Three-Layer Architecture

1. **Upstream Layer** (`claude-code/` submodule)
   - Official Anthropic repository
   - Never modify directly
   - Updated via `git submodule update --remote`
   - Commit hash tracked in parent repository

2. **Fork Layer** (this repository)
   - Manages submodule reference
   - Hosts plugin infrastructure
   - Organizational configuration
   - GitHub Actions workflows

3. **Extension Layer** (`plugins/`)
   - Custom functionality
   - Team-specific integrations
   - Workflow automations
   - Each plugin is independent

### Why This Matters for Agents

When working on this repository:

- **Reading upstream code**: Navigate to `claude-code/` to understand base functionality
- **Adding features**: Create or modify plugins in `plugins/`
- **Configuration changes**: Edit `.claude/settings.json`
- **Workflow changes**: Modify files in `.github/workflows/`
- **Documentation**: Update `README.md` or `docs/extensions/`

**CRITICAL**: If you find yourself about to modify a file inside `claude-code/`, STOP. The correct approach is to create or extend a plugin instead.

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
- Modify files in `claude-code/` submodule
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

## Sync Process Awareness

### Daily Upstream Sync

The repository has (or will have) automated daily syncs with upstream:

```
.github/workflows/sync-upstream.yml runs daily:
1. Fetch anthropics/claude-code latest
2. Update submodule reference
3. Run compatibility tests
4. Create PR if changes detected
5. Auto-merge if tests pass
```

### What This Means for Agents

- **Check for sync PRs**: Look for automated PRs titled "chore: sync with upstream"
- **Review compatibility**: Ensure plugins still work after upstream changes
- **Update documentation**: If upstream adds features, document in README.md
- **Handle conflicts**: If sync fails, investigate plugin incompatibilities

### Manual Sync

If a user requests manual sync:

```bash
# Update submodule to latest
git submodule update --remote claude-code

# Verify no local changes in submodule
cd claude-code
git status  # Should be clean

# Commit the update
cd ..
git add claude-code
git commit -m "chore: sync with upstream claude-code"
git push
```

## Working with Snail Integration

This repository is integrated with Heiervang's Snail AI agent system.

### Snail-Specific Plugins

- **heiervang-snail-integration**: Core integration with Snail
- **heiervang-workflows**: GitHub Actions for Snail automation

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

1. Check submodule: `cd claude-code && git log`
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

### Submodule Issues

**Check:**
1. Is submodule initialized? `git submodule status`
2. Are there local modifications? `cd claude-code && git status`
3. Is it on correct commit? `git submodule`

**Solution:**
```bash
# Reinitialize submodule
git submodule update --init --recursive

# Reset to tracked commit
git submodule update --force

# Sync with upstream
git submodule update --remote claude-code
```

### Upstream Sync Failures

**Check:**
1. Are tests failing?
2. Plugin incompatibilities?
3. Configuration conflicts?

**Solution:**
1. Review test output
2. Update plugins for compatibility
3. Adjust `.claude/settings.json` if needed
4. Create issue if upstream breaking change

## Links to Documentation

### Internal Documentation
- **Plugin Development**: `docs/extensions/plugin-development.md` (future)
- **Upstream Sync**: `docs/extensions/upstream-sync.md` (future)
- **GitHub Integration**: `docs/extensions/github-integration.md` (future)

### External Resources
- **Upstream Repository**: [anthropics/claude-code](https://github.com/anthropics/claude-code)
- **Claude API Docs**: [Anthropic Documentation](https://docs.anthropic.com/)
- **Organization**: [heiervang-technologies](https://github.com/heiervang-technologies)

## Quick Reference Commands

```bash
# Initialize submodules
git submodule update --init --recursive

# Update to latest upstream
git submodule update --remote claude-code

# Create new plugin
mkdir -p plugins/my-plugin && cd plugins/my-plugin

# Run Claude Code
cd claude-code && npm start

# Run tests
npm test

# View enabled plugins
cat .claude/settings.json | grep -A 10 "plugins"

# Check submodule status
git submodule status
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

### When Asked to Modify Upstream Code

```markdown
I notice you're asking me to modify code in the `claude-code/` submodule. This directory contains the upstream Anthropic code and should not be modified directly.

Instead, I recommend:
1. If this is a bug fix or general improvement: Contribute to [anthropics/claude-code](https://github.com/anthropics/claude-code)
2. If this is organization-specific: Create a plugin in `plugins/` directory
3. If this is configuration: Update `.claude/settings.json`

Which approach would you prefer?
```

### When Investigating Issues

```markdown
I'll investigate this issue systematically:

1. Checking plugin configuration in `.claude/settings.json`
2. Reviewing relevant plugin code in `plugins/`
3. Examining upstream submodule if needed (read-only)
4. Testing the scenario
5. Proposing a solution (plugin update or new plugin)

Let me start by examining...
```

## Final Notes

- **Think plugin-first**: Always consider if a plugin is the right solution
- **Respect the architecture**: The three-layer design is intentional
- **Document everything**: Future agents and users will thank you
- **Test thoroughly**: Plugins should be reliable and well-tested
- **Keep it clean**: Don't modify the submodule, ever

When in doubt, create a plugin. It's easier to merge plugins later than to untangle modifications from the upstream codebase.

---

**For questions or clarifications**, refer to the main README.md or create a discussion in the repository.
