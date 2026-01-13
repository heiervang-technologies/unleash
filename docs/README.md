# Claude-Unleashed Documentation

Welcome to the comprehensive documentation for the claude-unleashed repository.

## Overview

Claude-unleashed is a fork of [Claude Code](https://github.com/anthropic/claude-code) enhanced with GitHub Actions automation via [snail-core](https://github.com/heiervang-technologies/core) integration. This repository enables AI agents to work autonomously on GitHub issues and pull requests through workflow triggers.

## What is Claude-Unleashed?

**Claude-Unleashed = Claude Code + GitHub Actions + Snail Integration**

- **Claude Code**: Anthropic's official CLI for Claude AI
- **GitHub Actions**: Automated workflows triggered by GitHub events
- **Snail Integration**: Agent spawning and management system
- **Plugin Extensions**: Custom functionality via isolated plugins

## Architecture

```
claude-unleashed/
├── .github/workflows/     # GitHub Actions for agent automation
├── claude-code/           # Submodule: Fork of Claude Code
│   └── plugins/           # Custom plugin extensions
├── docs/                  # This documentation
│   ├── extensions/        # Plugin and extension guides
│   └── sync-process.md    # Upstream sync documentation
├── CLAUDE.md              # Agent instructions
└── README.md              # Repository overview
```

## Documentation Structure

### Getting Started

1. **[Repository Overview](../README.md)** - Main repository README with quick start
2. **[Agent Instructions](../CLAUDE.md)** - Guidelines for AI agents working in this repo

### Extension Development

Located in `docs/extensions/`:

1. **[Plugin Development Guide](./extensions/plugin-development.md)**
   - Complete guide to creating Claude Code plugins
   - Component types: commands, agents, skills, hooks, MCP
   - Step-by-step plugin creation
   - Testing and deployment
   - **Start here** for creating new functionality

2. **[Core Patches Guide](./extensions/core-patches.md)**
   - When and how to patch Claude Code core
   - Policy: Plugin-first approach (patches are rare)
   - Documentation requirements
   - Conflict risk assessment
   - Migration from patches to plugins

3. **[Snail Integration Guide](./extensions/snail-integration.md)**
   - GitHub Actions workflow integration
   - Agent automation via issue/PR triggers
   - MCP server configuration
   - Example commands and agents for GitHub workflows
   - Secrets and configuration management

4. **[Testing Guide](./extensions/testing-guide.md)**
   - Local plugin testing with `--plugin-dir`
   - GitHub workflow testing
   - Integration testing strategies
   - Debugging tips and common issues
   - Test automation

### Maintenance

5. **[Sync Process Documentation](./sync-process.md)**
   - Daily upstream synchronization
   - Conflict resolution (automated and manual)
   - AI agent conflict resolution
   - Rollback procedures
   - Sync health monitoring

## Quick Navigation

### I want to...

#### Create New Functionality

→ **[Plugin Development Guide](./extensions/plugin-development.md)**

Start here to learn about:
- Plugin architecture
- Component types (commands, agents, skills, hooks, MCP)
- Step-by-step creation process
- Testing locally
- Submitting PRs

#### Integrate with GitHub Workflows

→ **[Snail Integration Guide](./extensions/snail-integration.md)**

Learn about:
- Workflow triggers (mention, assignment)
- Agent automation
- Progress tracking
- MCP servers for GitHub integration
- Configuration and secrets

#### Test My Changes

→ **[Testing Guide](./extensions/testing-guide.md)**

Covers:
- Local testing with `--plugin-dir` flag
- Workflow testing
- Integration testing
- Debugging techniques
- Common issues and solutions

#### Modify Core Code

→ **[Core Patches Guide](./extensions/core-patches.md)**

**Warning**: Only for rare cases when plugins are insufficient.

Documents:
- When to use patches (almost never)
- Plugin-first policy
- Documentation requirements
- Conflict mitigation

#### Sync with Upstream

→ **[Sync Process Documentation](./sync-process.md)**

Explains:
- Automated daily sync
- Conflict handling
- AI-assisted resolution
- Manual resolution steps
- Rollback procedures

## Key Concepts

### Plugin-First Philosophy

**Default approach**: Create plugins, not core patches.

**Why?**
- ✅ Isolated, maintainable code
- ✅ Easy upstream synchronization
- ✅ Enable/disable without rebuilding
- ✅ Shareable across projects
- ✅ Minimal merge conflicts

**Plugins vs Patches**:
```
Plugin (99% of cases):
- Isolated in plugins/ directory
- No upstream conflicts
- Easy to test and maintain
- Shareable and reusable

Patch (1% of cases):
- Modifies core files
- Conflicts on every sync
- Requires extensive documentation
- Last resort only
```

### Component Types

Claude Code plugins can include:

1. **Commands**: User-invoked slash commands (`/my-command`)
2. **Agents**: Specialized AI agents for specific tasks
3. **Skills**: Auto-activating knowledge modules
4. **Hooks**: Event-driven automation (PreToolUse, Stop, etc.)
5. **MCP Servers**: External tool integrations

See [Plugin Development Guide](./extensions/plugin-development.md) for details.

### Snail Workflow Integration

GitHub Actions workflows trigger agents:

```
User mentions @agent in issue
           ↓
Workflow extracts context
           ↓
Spawns Claude Code in container
           ↓
Agent analyzes and responds
           ↓
Posts results as GitHub comment
```

See [Snail Integration Guide](./extensions/snail-integration.md) for details.

### Upstream Synchronization

Daily automated sync with upstream Claude Code:

```
Fetch upstream changes
           ↓
Auto-merge if possible
           ↓
AI agent resolves conflicts
           ↓
Manual resolution if needed
           ↓
Update submodule pointer
```

See [Sync Process](./sync-process.md) for details.

## Development Workflow

### 1. Creating a New Plugin

```bash
# Create plugin structure
mkdir -p claude-code/plugins/my-plugin/{.claude-plugin,commands,agents}

# Create manifest
cat > claude-code/plugins/my-plugin/.claude-plugin/plugin.json <<EOF
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "My custom plugin"
}
EOF

# Create command
cat > claude-code/plugins/my-plugin/commands/my-command.md <<EOF
---
name: my-command
description: Does something useful
---

Implementation here...
EOF

# Test locally
cc --plugin-dir claude-code/plugins/my-plugin
```

See full guide: [Plugin Development](./extensions/plugin-development.md)

### 2. Testing Changes

```bash
# Local testing
cc --plugin-dir /path/to/plugin

# Debug mode
cc --plugin-dir /path/to/plugin --debug

# Test in workflow
gh workflow run mention-trigger.yml -f issue_number=123
```

See full guide: [Testing](./extensions/testing-guide.md)

### 3. Submitting Changes

```bash
# Create branch
git checkout -b feature/my-plugin

# Commit with conventional commits
git commit -m "feat(plugins): add my-plugin for X functionality"

# Push and create PR
git push -u origin feature/my-plugin
gh pr create --title "feat: add my-plugin" --body "..."
```

See: [Plugin Development - Commit and PR Process](./extensions/plugin-development.md#commit-and-pr-process)

## Common Workflows

### Workflow 1: Issue Assignment Handler

**Goal**: Agent automatically works on assigned issues

**Setup**:
1. Configure `assignment-trigger.yml` workflow
2. Create plugin with issue analysis agent
3. Set up MCP GitHub server for API access

**Flow**:
```
Issue assigned to @agent
    ↓
Workflow triggers
    ↓
Agent analyzes issue
    ↓
Agent implements fix
    ↓
Agent creates PR
    ↓
Agent comments with solution
```

**Documentation**: [Snail Integration - Assignment Trigger](./extensions/snail-integration.md#2-assignment-trigger-assignment-triggeryml)

### Workflow 2: PR Review Automation

**Goal**: Agent reviews PRs when mentioned

**Setup**:
1. Install pr-review-toolkit plugin
2. Configure mention trigger
3. Create review standards in CLAUDE.md

**Flow**:
```
User comments "@agent review this PR"
    ↓
Workflow triggers
    ↓
Agent uses /pr-review-toolkit:review-pr
    ↓
Multiple review agents analyze in parallel
    ↓
Agent posts comprehensive review
```

**Documentation**: [Snail Integration - Example Commands and Agents](./extensions/snail-integration.md#example-commands-and-agents)

### Workflow 3: Custom Plugin Development

**Goal**: Create plugin for specific domain

**Setup**:
1. Design plugin structure
2. Implement components
3. Test locally
4. Deploy to repository

**Flow**:
```
Identify need for functionality
    ↓
Use /plugin-dev:create-plugin (guided workflow)
    ↓
Test with --plugin-dir
    ↓
Commit to repository
    ↓
Available in workflows
```

**Documentation**: [Plugin Development - Step-by-Step](./extensions/plugin-development.md#step-by-step-plugin-creation)

## Examples

### Example Plugin: GitHub Issue Toolkit

A complete plugin for GitHub issue management:

```
github-issue-toolkit/
├── .claude-plugin/
│   └── plugin.json
├── commands/
│   ├── create-issue.md      # /create-issue command
│   ├── triage-issue.md      # /triage-issue command
│   └── close-issue.md       # /close-issue command
├── agents/
│   ├── issue-triager.md     # Auto-triage agent
│   └── issue-analyzer.md    # Deep analysis agent
├── skills/
│   └── issue-templates/
│       └── SKILL.md         # Issue template knowledge
├── hooks/
│   ├── hooks.json
│   └── scripts/
│       └── validate-issue.sh  # Validate issue operations
└── .mcp.json                # GitHub MCP server config
```

See: [Plugin Development - Example Plugin Structure](./extensions/plugin-development.md#example-plugin-structure)

### Example Workflow: Bug Fix Automation

Complete automated bug fix workflow:

1. **Issue Created**: User creates bug report
2. **Agent Assigned**: Issue assigned to @agent
3. **Agent Analyzes**: bugfix-agent reads issue, reproduces bug
4. **Agent Fixes**: Implements fix with tests
5. **Agent Tests**: Runs test suite
6. **Agent Creates PR**: Submits PR with fix
7. **Agent Comments**: Updates issue with PR link

See: [Snail Integration - Automated Bug Fix Agent](./extensions/snail-integration.md#example-3-automated-bug-fix-agent)

## Existing Plugins

The repository includes several pre-built plugins in `claude-code/plugins/`:

| Plugin | Purpose | Components |
|--------|---------|------------|
| **agent-sdk-dev** | Agent SDK development | Commands, Agents |
| **code-review** | Automated PR review | Command, 5 Agents |
| **commit-commands** | Git workflow automation | 3 Commands |
| **feature-dev** | Structured feature development | Command, 3 Agents |
| **hookify** | Easy hook creation | 4 Commands, Agent |
| **plugin-dev** | Plugin development toolkit | Command, 3 Agents, 7 Skills |
| **pr-review-toolkit** | Comprehensive PR review | Command, 6 Agents |
| **security-guidance** | Security validation | Hooks |

See plugin README files for usage details.

## Best Practices

### Plugin Development

✅ **Do**:
- Use plugin-first approach for all functionality
- Include comprehensive README.md
- Use `${CLAUDE_PLUGIN_ROOT}` for portability
- Test thoroughly with `--plugin-dir`
- Document all configuration

❌ **Don't**:
- Modify core Claude Code files
- Use hardcoded paths
- Skip documentation
- Commit without testing

### Workflow Integration

✅ **Do**:
- Update progress comments
- Link PRs to issues
- Handle errors gracefully
- Test workflows manually first
- Document expected behavior

❌ **Don't**:
- Make breaking changes without notice
- Skip error handling
- Leave workflows untested
- Commit secrets to repository

### Upstream Synchronization

✅ **Do**:
- Sync regularly (daily automated)
- Document all patches thoroughly
- Test after merging
- Convert patches to plugins when possible
- Monitor sync health

❌ **Don't**:
- Ignore sync conflicts
- Skip patch documentation
- Merge without testing
- Create unnecessary patches

## Troubleshooting

### Plugin Not Loading

1. Check manifest exists: `.claude-plugin/plugin.json`
2. Validate JSON syntax: `jq . plugin.json`
3. Verify plugin name (kebab-case)
4. Use correct path with `--plugin-dir`

See: [Testing Guide - Common Issues](./extensions/testing-guide.md#common-issues-and-solutions)

### Workflow Not Triggering

1. Verify workflow file syntax
2. Check trigger conditions (agent username)
3. Validate secrets are configured
4. Review workflow logs

See: [Snail Integration - Troubleshooting](./extensions/snail-integration.md#troubleshooting)

### Merge Conflicts on Sync

1. Check `.unleashed/patches/` for documentation
2. Review conflicting files
3. Follow manual resolution steps
4. Update patch documentation

See: [Sync Process - Manual Resolution](./sync-process.md#manual-resolution-steps)

## Contributing

### For Plugin Developers

1. Read [Plugin Development Guide](./extensions/plugin-development.md)
2. Create plugin following structure
3. Test locally with `--plugin-dir`
4. Submit PR with documentation
5. Respond to review feedback

### For Core Contributors

1. Check if functionality can be a plugin (usually yes)
2. Read [Core Patches Guide](./extensions/core-patches.md) if patch needed
3. Document extensively
4. Test sync compatibility
5. Create detailed PR

### For Documentation

1. Use clear, practical examples
2. Include code samples
3. Keep sections focused
4. Link to related docs
5. Update when features change

## Resources

### Internal Documentation

**CLI Reference:**
- [Authentication Check Command](./auth-check-command.md) - Verify Claude Code authentication
- [JSON Output Specification](../JSON_OUTPUT.md) - JSON output format for scripting

**Extension Development:**
- [Plugin Development Guide](./extensions/plugin-development.md) - Comprehensive plugin creation
- [Core Patches Guide](./extensions/core-patches.md) - Core modification policy
- [Snail Integration Guide](./extensions/snail-integration.md) - GitHub Actions integration
- [Testing Guide](./extensions/testing-guide.md) - Testing strategies
- [Sync Process](./sync-process.md) - Upstream synchronization

### External Resources

- [Claude Code Documentation](https://docs.claude.com/claude-code)
- [Claude Code GitHub](https://github.com/anthropic/claude-code)
- [Snail Core Template](https://github.com/heiervang-technologies/core)
- [MCP Protocol](https://github.com/modelcontextprotocol)
- [GitHub Actions Documentation](https://docs.github.com/actions)

### Community

- GitHub Issues: Bug reports and feature requests
- GitHub Discussions: Questions and community
- Pull Requests: Code contributions

## Glossary

**Agent**: AI instance (Claude) running in Claude Code, capable of using tools and making decisions

**Command**: User-invoked functionality via slash commands (e.g., `/review`)

**Hook**: Event-driven automation triggered by Claude Code lifecycle events

**MCP**: Model Context Protocol - standard for connecting LLMs to external tools

**Plugin**: Self-contained extension to Claude Code with commands, agents, skills, hooks, or MCP servers

**Skill**: Auto-activating knowledge module triggered by task context

**Snail**: Agent spawning and management system for GitHub Actions

**Subagent**: Specialized agent invoked for specific tasks within a session

**Upstream**: The original anthropic/claude-code repository

## FAQ

**Q: Should I create a plugin or a patch?**
A: Almost always create a plugin. Only use patches for critical bugs, security fixes, or when plugin approach is truly impossible.

**Q: How do I test a plugin locally?**
A: Use `cc --plugin-dir /path/to/plugin` to load your plugin in isolation.

**Q: Can plugins modify Claude Code behavior?**
A: Yes, via hooks (PreToolUse, Stop, etc.) and agents. But modifications should be configurable and documented.

**Q: How often does upstream sync occur?**
A: Automatically daily at 2 AM UTC. Can also be triggered manually.

**Q: What happens if sync conflicts occur?**
A: AI agent attempts resolution first. If too complex, creates issue for manual resolution.

**Q: Can I use multiple plugins together?**
A: Yes! Plugins are designed to work together. Use multiple `--plugin-dir` flags.

**Q: How do I contribute a plugin?**
A: Follow the [Plugin Development Guide](./extensions/plugin-development.md), test thoroughly, and submit a PR.

**Q: Where do secrets go for MCP servers?**
A: Environment variables or GitHub Secrets. See [Snail Integration - Configuration](./extensions/snail-integration.md#configuration-and-secrets).

## Getting Help

1. **Check Documentation**: Start with relevant guide above
2. **Search Issues**: Look for similar problems in GitHub issues
3. **Create Issue**: If problem persists, create detailed issue
4. **Ask in Discussions**: For questions, use GitHub Discussions

## License

This repository inherits the license from Claude Code. See LICENSE.md for details.

## Acknowledgments

- **Anthropic**: For Claude Code
- **Heiervang Technologies**: For snail-core integration
- **Contributors**: Everyone who has contributed plugins and improvements

---

**Last Updated**: 2026-01-13

For questions or suggestions about this documentation, please create an issue.
