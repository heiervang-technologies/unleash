# Plugin Development Guide

## Overview

This guide covers developing custom plugins for the unleash repository, which is a unified CLI wrapper for AI code agents.

Plugins extend Claude Code with custom commands, agents, skills, hooks, and MCP (Model Context Protocol) integrations. The plugin-first approach ensures minimal merge conflicts when syncing with upstream Claude Code updates.

## Table of Contents

1. [Plugin Architecture](#plugin-architecture)
2. [Directory Structure](#directory-structure)
3. [Component Types](#component-types)
4. [Step-by-Step Plugin Creation](#step-by-step-plugin-creation)
5. [Testing Plugins Locally](#testing-plugins-locally)
6. [Example Plugin Structure](#example-plugin-structure)
7. [Commit and PR Process](#commit-and-pr-process)

## Plugin Architecture

### Core Concepts

Claude Code plugins are self-contained extensions that follow a standardized structure:

- **Automatic Discovery**: Components are discovered automatically based on directory structure
- **Portable Paths**: Use `${CLAUDE_PLUGIN_ROOT}` for all internal plugin references
- **Manifest-Driven**: Plugin metadata defined in `.claude-plugin/plugin.json`
- **Component-Based**: Mix and match commands, agents, skills, hooks, and MCP servers
- **Hot-Loadable**: Plugins can be enabled/disabled without rebuilding

### Why Plugin-First?

The unleash repository maintains a plugin-first philosophy:

1. **Minimal Conflicts**: Plugins live in isolated directories, avoiding merge conflicts with upstream
2. **Easy Updates**: Sync with upstream Claude Code without breaking custom functionality
3. **Modular Design**: Enable/disable features without modifying core code
4. **Shareable**: Plugins can be distributed independently
5. **Maintainable**: Clear separation between core and extensions

## Directory Structure

### Standard Plugin Layout

Every plugin follows this structure:

```
plugin-name/
├── .claude-plugin/
│   └── plugin.json          # Required: Plugin manifest
├── commands/                 # Optional: Slash commands
│   └── my-command.md
├── agents/                   # Optional: Specialized agents
│   └── my-agent.md
├── skills/                   # Optional: Auto-activating skills
│   └── skill-name/
│       └── SKILL.md
├── hooks/                    # Optional: Event handlers
│   ├── hooks.json
│   └── scripts/
│       └── validate.sh
├── .mcp.json                # Optional: MCP server config
├── scripts/                 # Optional: Utility scripts
└── README.md                # Required: Plugin documentation
```

### Critical Rules

1. **Manifest Location**: The `plugin.json` MUST be in `.claude-plugin/` directory
2. **Component Locations**: All component directories (commands, agents, skills, hooks) MUST be at plugin root level
3. **Optional Components**: Only create directories for components you actually use
4. **Naming Convention**: Use kebab-case for all directory and file names

### Example Minimal Plugin

```
hello-world/
├── .claude-plugin/
│   └── plugin.json
└── commands/
    └── hello.md
```

### Example Full-Featured Plugin

```
database-toolkit/
├── .claude-plugin/
│   └── plugin.json
├── commands/
│   ├── query.md
│   └── migrate.md
├── agents/
│   └── schema-designer.md
├── skills/
│   ├── query-optimization/
│   │   └── SKILL.md
│   └── migration-patterns/
│       └── SKILL.md
├── hooks/
│   ├── hooks.json
│   └── scripts/
│       └── validate-query.sh
├── .mcp.json
└── README.md
```

## Component Types

### 1. Commands

Slash commands that users invoke explicitly.

**Location**: `commands/` directory
**Format**: Markdown files with YAML frontmatter
**Usage**: `/command-name [args]`

**Example** (`commands/review.md`):

```markdown
---
name: review
description: Review code changes in current branch
argument-hint: "[--detailed]"
allowed-tools: ["Read", "Bash", "Grep"]
---

# Code Review Command

Review all code changes in the current git branch.

## Instructions

1. Use `git diff main...HEAD` to see all changes
2. Analyze each file for:
   - Code quality issues
   - Security vulnerabilities
   - Best practice violations
   - Performance concerns
3. Provide structured feedback
4. Suggest improvements

## Optional Arguments

- `--detailed`: Provide line-by-line analysis
```

**Frontmatter Fields**:
- `name` (required): Command name (use kebab-case)
- `description` (required): Brief description shown in help
- `argument-hint` (optional): Shows expected arguments
- `allowed-tools` (optional): Restrict which tools Claude can use

### 2. Agents

Specialized AI agents for specific tasks.

**Location**: `agents/` directory
**Format**: Markdown files with YAML frontmatter
**Usage**: Auto-invoked by Claude or manually called

**Example** (`agents/test-generator.md`):

```markdown
---
name: Test Generator
description: |
  This agent specializes in creating comprehensive test suites. Use when:
  <example>the user asks to "write tests" or "add test coverage"</example>
  <example>implementing a new feature that needs tests</example>
  <example>refactoring code and ensuring no regressions</example>
model: claude-sonnet-4-5-20250929
color: green
allowed-tools: ["Read", "Write", "Edit", "Bash", "Grep", "Glob"]
---

# Test Generation Expert

You are an expert at writing comprehensive, maintainable test suites.

## Your Responsibilities

1. **Analyze Code**: Understand the code being tested
2. **Identify Test Cases**: Consider edge cases, error conditions, and happy paths
3. **Write Tests**: Create clear, maintainable tests following project conventions
4. **Coverage**: Ensure comprehensive coverage of functionality

## Test Patterns

- Use descriptive test names
- Follow AAA pattern (Arrange, Act, Assert)
- Test one thing per test
- Include both positive and negative cases
- Mock external dependencies appropriately

## Testing Frameworks

Detect and use the project's testing framework:
- JavaScript: Jest, Mocha, Vitest
- Python: pytest, unittest
- TypeScript: Jest, Vitest
- Go: testing package

Always check existing tests to match the project's style and conventions.
```

**Frontmatter Fields**:
- `name` (optional): Agent display name
- `description` (required): When to use this agent (include `<example>` blocks)
- `model` (optional): Specific Claude model to use
- `color` (optional): UI color hint
- `allowed-tools` (optional): Restrict available tools

**Description Best Practices**:
- Include `<example>` blocks with trigger phrases
- Be specific about when to use the agent
- Describe capabilities clearly

### 3. Skills

Auto-activating knowledge modules triggered by context.

**Location**: `skills/skill-name/SKILL.md`
**Format**: Markdown with YAML frontmatter in subdirectory
**Usage**: Automatically loaded when description matches task context

**Example** (`skills/api-design/SKILL.md`):

```markdown
---
name: API Design
description: This skill should be used when the user asks to "design an API", "create REST endpoints", "build GraphQL schema", or needs guidance on API architecture, versioning, authentication patterns, or API documentation.
version: 1.0.0
---

# API Design Best Practices

## Overview

Comprehensive guidance for designing robust, scalable APIs.

## REST API Design

### Resource Naming
- Use nouns, not verbs: `/users`, not `/getUsers`
- Use plural forms: `/users`, not `/user`
- Nest resources logically: `/users/123/orders`
- Avoid deep nesting (max 2-3 levels)

### HTTP Methods
- `GET`: Retrieve resources (idempotent, cacheable)
- `POST`: Create resources (non-idempotent)
- `PUT`: Update entire resource (idempotent)
- `PATCH`: Partial update (idempotent)
- `DELETE`: Remove resource (idempotent)

### Status Codes
- `200 OK`: Successful GET, PUT, PATCH
- `201 Created`: Successful POST
- `204 No Content`: Successful DELETE
- `400 Bad Request`: Invalid client data
- `401 Unauthorized`: Authentication required
- `403 Forbidden`: Authenticated but not authorized
- `404 Not Found`: Resource doesn't exist
- `500 Internal Server Error`: Server error

### Versioning Strategies
1. **URL versioning**: `/v1/users` (recommended for simplicity)
2. **Header versioning**: `Accept: application/vnd.api.v1+json`
3. **Query parameter**: `/users?version=1` (not recommended)

## Authentication Patterns

### JWT (JSON Web Tokens)
```
Authorization: Bearer <token>
```
- Stateless, scalable
- Include only public claims
- Set appropriate expiration
- Use HTTPS only

### API Keys
```
X-API-Key: <key>
```
- Simple for service-to-service
- Rotate regularly
- Scope permissions appropriately

### OAuth 2.0
- For third-party access
- Use authorization code flow for web apps
- Use PKCE for mobile/SPAs
- Implement proper scope management

## Documentation

Always document:
- Available endpoints and methods
- Request/response schemas
- Authentication requirements
- Rate limits
- Error responses
- Example requests

Consider using:
- OpenAPI/Swagger specification
- Postman collections
- API Blueprint
```

**Supporting Files**:

Skills can include additional resources:

```
skills/api-design/
├── SKILL.md
├── references/
│   ├── openapi-template.yaml
│   └── authentication-guide.md
├── examples/
│   └── rest-api-example.md
└── scripts/
    └── validate-openapi.sh
```

**Frontmatter Fields**:
- `name` (required): Skill name
- `description` (required): When to activate (include trigger phrases)
- `version` (optional): Semantic version

### 4. Hooks

Event-driven automation that executes during Claude Code lifecycle.

**Location**: `hooks/hooks.json` and `hooks/scripts/`
**Format**: JSON configuration + executable scripts
**Usage**: Automatically triggered by events

**Example** (`hooks/hooks.json`):

```json
{
  "description": "Security validation hooks",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write|Edit",
        "hooks": [
          {
            "type": "prompt",
            "prompt": "Check if this file write involves sensitive data like API keys, passwords, or credentials. Return 'approve' if safe, 'deny' if dangerous.",
            "timeout": 30
          }
        ]
      },
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/validate-bash.sh",
            "timeout": 10
          }
        ]
      }
    ],
    "Stop": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "prompt",
            "prompt": "Verify the task is complete: tests have run, build succeeded, all questions answered. Return 'approve' to allow stopping or 'block' with reason to continue working."
          }
        ]
      }
    ],
    "SessionStart": [
      {
        "matcher": "*",
        "hooks": [
          {
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/load-context.sh",
            "timeout": 15
          }
        ]
      }
    ]
  }
}
```

**Hook Types**:

1. **Prompt-Based Hooks** (recommended for complex logic):
```json
{
  "type": "prompt",
  "prompt": "Evaluate this action and decide approve/deny/ask",
  "timeout": 30
}
```

2. **Command Hooks** (for deterministic checks):
```json
{
  "type": "command",
  "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/check.sh",
  "timeout": 60
}
```

**Available Events**:
- `PreToolUse`: Before tool execution (validation)
- `PostToolUse`: After tool completes (feedback)
- `Stop`: When Claude wants to stop (completeness check)
- `SubagentStop`: When subagent completes
- `UserPromptSubmit`: When user submits input
- `SessionStart`: Session begins (context loading)
- `SessionEnd`: Session ends (cleanup)
- `PreCompact`: Before context compaction
- `Notification`: When notifications sent

**Example Hook Script** (`hooks/scripts/validate-bash.sh`):

```bash
#!/bin/bash
set -euo pipefail

# Read input JSON from stdin
input=$(cat)

# Extract command
command=$(echo "$input" | jq -r '.tool_input.command // ""')

# Check for dangerous patterns
if [[ "$command" =~ rm[[:space:]]+-rf[[:space:]]+/ ]] || \
   [[ "$command" =~ dd[[:space:]]+if= ]] || \
   [[ "$command" =~ mkfs ]] || \
   [[ "$command" =~ :(){:\|:}; ]]; then

  # Block dangerous command
  cat <<EOF >&2
{
  "decision": "deny",
  "reason": "Dangerous command detected: $command",
  "systemMessage": "This command could cause data loss. Please verify the exact parameters."
}
EOF
  exit 2
fi

# Allow command
cat <<EOF
{
  "decision": "approve",
  "systemMessage": "Command validated"
}
EOF
exit 0
```

### 5. MCP Servers

Model Context Protocol servers for external tool integration.

**Location**: `.mcp.json` at plugin root
**Format**: JSON configuration
**Usage**: Automatically started when plugin loads

**Example** (`.mcp.json`):

```json
{
  "mcpServers": {
    "database": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/servers/database-server.js"],
      "env": {
        "DB_HOST": "${DB_HOST}",
        "DB_PORT": "${DB_PORT}",
        "DB_NAME": "${DB_NAME}",
        "DB_USER": "${DB_USER}",
        "DB_PASSWORD": "${DB_PASSWORD}"
      }
    },
    "slack": {
      "command": "python",
      "args": ["-m", "slack_mcp_server"],
      "env": {
        "SLACK_TOKEN": "${SLACK_TOKEN}"
      }
    }
  }
}
```

**Server Types**:
- **stdio**: Local process communication
- **SSE**: Server-Sent Events (for hosted/OAuth)
- **HTTP**: REST-based integration
- **WebSocket**: Real-time communication

## Step-by-Step Plugin Creation

### Phase 1: Planning

1. **Define Purpose**: What problem does this plugin solve?
2. **Identify Components**: Which component types do you need?
   - Commands: User-invoked actions
   - Agents: Specialized AI assistance
   - Skills: Auto-activating knowledge
   - Hooks: Automation and validation
   - MCP: External integrations
3. **Design Structure**: Map out directory layout
4. **List Dependencies**: External tools, APIs, credentials

### Phase 2: Scaffolding

1. **Create Plugin Directory**:
```bash
cd plugins/bundled
mkdir my-plugin
cd my-plugin
```

2. **Create Manifest**:
```bash
mkdir .claude-plugin
cat > .claude-plugin/plugin.json <<'EOF'
{
  "name": "my-plugin",
  "version": "1.0.0",
  "description": "Brief description of plugin functionality",
  "author": {
    "name": "Your Name",
    "email": "you@example.com"
  }
}
EOF
```

3. **Create Component Directories** (as needed):
```bash
mkdir -p commands
mkdir -p agents
mkdir -p skills
mkdir -p hooks/scripts
mkdir -p scripts
```

### Phase 3: Implementation

#### Creating a Command

1. **Create Command File**:
```bash
cat > commands/my-command.md <<'EOF'
---
name: my-command
description: Does something useful
---

# My Command Implementation

Instructions for Claude on how to execute this command...
EOF
```

2. **Test Command**:
```bash
cc --plugin-dir plugins/bundled/my-plugin
# In Claude: /my-command
```

#### Creating an Agent

1. **Create Agent File**:
```bash
cat > agents/my-agent.md <<'EOF'
---
name: My Agent
description: |
  This agent does X. Use when:
  <example>user asks to "do X"</example>
  <example>implementing feature Y</example>
model: claude-sonnet-4-5-20250929
---

# Agent System Prompt

You are an expert at X...
EOF
```

#### Creating a Skill

1. **Create Skill Directory**:
```bash
mkdir -p skills/my-skill
```

2. **Create SKILL.md**:
```bash
cat > skills/my-skill/SKILL.md <<'EOF'
---
name: My Skill
description: This skill should be used when the user asks to "X" or "Y"
version: 1.0.0
---

# Skill Knowledge

Detailed information about this skill...
EOF
```

#### Creating Hooks

1. **Create hooks.json**:
```bash
cat > hooks/hooks.json <<'EOF'
{
  "description": "My plugin hooks",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write",
        "hooks": [
          {
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/validate.sh"
          }
        ]
      }
    ]
  }
}
EOF
```

2. **Create Hook Script**:
```bash
cat > hooks/scripts/validate.sh <<'EOF'
#!/bin/bash
set -euo pipefail

input=$(cat)
# Validation logic here
echo '{"decision": "approve"}'
exit 0
EOF
chmod +x hooks/scripts/validate.sh
```

### Phase 4: Documentation

Create comprehensive README.md:

```markdown
# My Plugin

Brief description of what this plugin does.

## Features

- Feature 1
- Feature 2
- Feature 3

## Installation

Install from local development:
```bash
cc --plugin-dir /path/to/my-plugin
```

## Commands

### /my-command

Description of command and usage examples.

## Agents

### my-agent

Description of agent capabilities and when it's invoked.

## Skills

### My Skill

Description of skill knowledge and trigger conditions.

## Configuration

Environment variables or settings needed:
- `MY_VAR`: Description

## Examples

Common usage examples with expected output.

## Troubleshooting

Common issues and solutions.
```

### Phase 5: Testing

See [Testing Plugins Locally](#testing-plugins-locally) section.

## Testing Plugins Locally

### Local Testing Workflow

1. **Use --plugin-dir Flag**:
```bash
cc --plugin-dir plugins/bundled/my-plugin
```

This loads only your plugin for isolated testing.

2. **Test Commands**:
```
# In Claude Code session
/my-command arg1 arg2
```

3. **Test Agents**:
```
# Trigger agent by matching description
"Can you help me with [agent's domain]?"
```

4. **Test Skills**:
```
# Skills auto-activate based on context
"I need to [trigger phrase from skill description]"
```

5. **Test Hooks with Debug Mode**:
```bash
cc --plugin-dir /path/to/plugin --debug
```

Look for hook execution logs in output.

### Validation Checklist

- [ ] Plugin manifest is valid JSON
- [ ] All component files have valid YAML frontmatter
- [ ] Commands execute without errors
- [ ] Agents trigger correctly
- [ ] Skills activate on expected triggers
- [ ] Hooks validate/execute as expected
- [ ] All scripts are executable (`chmod +x`)
- [ ] ${CLAUDE_PLUGIN_ROOT} used for all internal paths
- [ ] README.md is comprehensive
- [ ] No hardcoded absolute paths

### Testing with Multiple Plugins

Load multiple plugins for integration testing:

```bash
cc --plugin-dir /path/to/plugin1 --plugin-dir /path/to/plugin2
```

### Debug Mode Testing

Enable detailed logging:

```bash
cc --plugin-dir /path/to/plugin --debug
```

Look for:
- Plugin loading messages
- Component registration
- Hook execution logs
- Error messages
- Tool invocations

### Hook Testing

Test hook scripts independently:

```bash
echo '{"tool_name": "Write", "tool_input": {"file_path": "/test.txt"}}' | \
  bash hooks/scripts/validate.sh
echo "Exit code: $?"
```

### Integration Testing

Test plugin in realistic scenarios:

1. Create test project
2. Load plugin
3. Perform realistic tasks
4. Verify expected behavior
5. Check for errors or warnings

## Example Plugin Structure

### Complete Working Example

Here's a complete example: `github-issue-toolkit`

```
github-issue-toolkit/
├── .claude-plugin/
│   └── plugin.json
├── commands/
│   ├── create-issue.md
│   ├── list-issues.md
│   └── close-issue.md
├── agents/
│   ├── issue-triager.md
│   └── issue-analyzer.md
├── skills/
│   ├── issue-templates/
│   │   ├── SKILL.md
│   │   └── templates/
│   │       ├── bug-report.md
│   │       └── feature-request.md
│   └── issue-automation/
│       └── SKILL.md
├── hooks/
│   ├── hooks.json
│   └── scripts/
│       └── validate-issue.sh
├── .mcp.json
├── scripts/
│   └── common.sh
└── README.md
```

**`.claude-plugin/plugin.json`**:
```json
{
  "name": "github-issue-toolkit",
  "version": "1.0.0",
  "description": "Comprehensive GitHub issue management toolkit",
  "author": {
    "name": "Plugin Developer",
    "email": "dev@example.com"
  },
  "keywords": ["github", "issues", "automation"]
}
```

**`commands/create-issue.md`**:
```markdown
---
name: create-issue
description: Create a new GitHub issue
argument-hint: "[title] [--body TEXT] [--labels LABELS]"
allowed-tools: ["Bash"]
---

# Create GitHub Issue

Create a new issue in the current repository.

## Usage

```
/create-issue "Bug in login" --body "Description here" --labels "bug,urgent"
```

## Implementation

1. Parse arguments
2. Use `gh issue create` command
3. Set title, body, labels
4. Return issue URL
```

**`agents/issue-triager.md`**:
```markdown
---
name: Issue Triager
description: |
  Analyzes GitHub issues for triage. Use when:
  <example>user asks to "triage issues"</example>
  <example>analyzing issue priority</example>
  <example>categorizing issues</example>
model: claude-sonnet-4-5-20250929
color: blue
---

# Issue Triage Expert

Analyze issues and provide triage recommendations.

## Analysis Process

1. **Fetch Issue**: Get issue details with `gh issue view`
2. **Analyze Content**: Review title, body, comments
3. **Determine Priority**: Based on severity, impact, frequency
4. **Suggest Labels**: Appropriate categorization
5. **Recommend Assignment**: Suggest who should handle it

## Priority Levels

- **Critical**: System down, data loss, security issue
- **High**: Major feature broken, significant impact
- **Medium**: Minor bugs, feature requests
- **Low**: Nice-to-have, documentation, cleanup
```

**`skills/issue-templates/SKILL.md`**:
```markdown
---
name: Issue Templates
description: This skill should be used when the user asks to "create an issue template", "standardize issue format", or needs guidance on GitHub issue templates and best practices.
version: 1.0.0
---

# GitHub Issue Template Best Practices

## Bug Report Template

```markdown
## Description
A clear description of the bug.

## Steps to Reproduce
1. Step one
2. Step two
3. Step three

## Expected Behavior
What should happen.

## Actual Behavior
What actually happens.

## Environment
- OS:
- Browser:
- Version:

## Additional Context
Screenshots, logs, etc.
```

## Feature Request Template

```markdown
## Feature Description
Clear description of proposed feature.

## Use Case
Why is this needed? What problem does it solve?

## Proposed Solution
How should this work?

## Alternatives Considered
Other approaches considered.

## Additional Context
Mockups, examples, etc.
```
```

**`hooks/hooks.json`**:
```json
{
  "description": "Validate issue operations",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/validate-issue.sh",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
```

**`.mcp.json`**:
```json
{
  "mcpServers": {
    "github": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-github"],
      "env": {
        "GITHUB_TOKEN": "${GITHUB_TOKEN}"
      }
    }
  }
}
```

## Commit and PR Process

### Commit Guidelines

1. **Use Conventional Commits**:
```bash
feat: add new issue creation command
fix: correct issue label parsing
docs: update README with usage examples
refactor: simplify hook validation logic
test: add tests for issue triager agent
```

2. **Commit Scope**:
```bash
feat(commands): add create-issue command
fix(hooks): correct validation script path
docs(readme): add installation instructions
```

3. **Commit Message Format**:
```
<type>(<scope>): <subject>

<body>

<footer>
```

Example:
```
feat(agents): add issue triager agent

The issue triager agent analyzes GitHub issues and provides
triage recommendations including priority, labels, and
assignment suggestions.

Closes #123
```

### Pull Request Process

1. **Create Feature Branch**:
```bash
cd /home/me/unleash
git checkout -b feature/my-plugin-name
```

2. **Develop and Test**:
```bash
# Create plugin in plugins/bundled/
# Test locally
cc --plugin-dir plugins/bundled/my-plugin

# Run any tests
# Verify functionality
```

3. **Commit Changes**:
```bash
git add plugins/bundled/my-plugin/
git commit -m "feat(plugins): add my-plugin for X functionality"
```

4. **Push Branch**:
```bash
git push -u origin feature/my-plugin-name
```

5. **Create Pull Request**:

Use GitHub CLI or web interface:

```bash
gh pr create --title "feat(plugins): add my-plugin" --body "$(cat <<'EOF'
## Summary

- Adds new plugin for X functionality
- Includes commands for Y
- Provides agent for Z

## Components

- **Commands**: /command1, /command2
- **Agents**: agent-name
- **Skills**: skill-name
- **Hooks**: PreToolUse validation

## Testing

- [ ] Tested locally with --plugin-dir
- [ ] All commands work as expected
- [ ] Agents trigger correctly
- [ ] Hooks validate properly
- [ ] Documentation is complete

## Related Issues

Closes #123
EOF
)"
```

### PR Review Checklist

Reviewers should verify:

- [ ] Plugin follows standard directory structure
- [ ] Plugin manifest is complete and valid
- [ ] All components have proper frontmatter
- [ ] ${CLAUDE_PLUGIN_ROOT} used (no hardcoded paths)
- [ ] README.md is comprehensive
- [ ] Commands documented with examples
- [ ] Agents have clear trigger descriptions
- [ ] Skills have focused, specific descriptions
- [ ] Hooks include proper error handling
- [ ] No sensitive data in code/config
- [ ] Plugin tested locally
- [ ] Follows naming conventions (kebab-case)

### Post-Merge

After merge:

1. Plugin available in plugins/bundled/
2. Users can load with `--plugin-dir`
3. Consider adding to marketplace
4. Monitor for issues/feedback
5. Iterate based on usage

### Plugin Updates

For existing plugins:

1. Increment version in plugin.json
2. Document breaking changes
3. Update README.md
4. Test thoroughly
5. Create PR with changelog

Version bump guidelines:
- **Patch** (1.0.0 → 1.0.1): Bug fixes, minor improvements
- **Minor** (1.0.0 → 1.1.0): New features, backward compatible
- **Major** (1.0.0 → 2.0.0): Breaking changes

## Best Practices Summary

### Do's

- ✅ Use plugin-first approach for all extensions
- ✅ Follow standard directory structure
- ✅ Use ${CLAUDE_PLUGIN_ROOT} for portability
- ✅ Write comprehensive README.md
- ✅ Include usage examples
- ✅ Test thoroughly before submitting PR
- ✅ Use conventional commits
- ✅ Document breaking changes
- ✅ Validate JSON configurations
- ✅ Make scripts executable

### Don'ts

- ❌ Don't modify core Claude Code files
- ❌ Don't use hardcoded absolute paths
- ❌ Don't commit sensitive credentials
- ❌ Don't skip documentation
- ❌ Don't create monolithic plugins (separate concerns)
- ❌ Don't use global state without documentation
- ❌ Don't assume OS-specific features
- ❌ Don't skip testing

## Additional Resources

- [Testing Guide](./testing-guide.md) - Comprehensive testing strategies

## Getting Help

- Review existing plugins in `plugins/bundled/`
- Ask in project discussions
- Review Claude Code documentation: https://docs.claude.com/claude-code
