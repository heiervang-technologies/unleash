# Testing Guide

## Overview

This guide covers comprehensive testing strategies for unleash plugins, workflows, and integrations. Proper testing ensures reliability, prevents regressions, and maintains quality standards.

## Table of Contents

1. [Local Plugin Testing](#local-plugin-testing)
2. [GitHub Workflow Testing](#github-workflow-testing)
3. [Integration Testing](#integration-testing)
4. [Debugging Tips](#debugging-tips)
5. [Common Issues and Solutions](#common-issues-and-solutions)

## Local Plugin Testing

### Testing with --plugin-dir Flag

The `--plugin-dir` flag allows isolated testing of plugins during development.

#### Basic Usage

```bash
# Test single plugin
cc --plugin-dir plugins/bundled/my-plugin

# Test multiple plugins
cc --plugin-dir /path/to/plugin1 --plugin-dir /path/to/plugin2

# Combine with debug mode
cc --plugin-dir /path/to/plugin --debug
```

#### Testing Workflow

1. **Create Test Environment**:
```bash
# Create test project
mkdir /tmp/test-project
cd /tmp/test-project

# Initialize test repository
git init
echo "# Test Project" > README.md
git add README.md
git commit -m "Initial commit"
```

2. **Load Plugin**:
```bash
# Launch Claude Code with plugin
cc --plugin-dir plugins/bundled/my-plugin
```

3. **Test Components**:

**Test Commands**:
```
# In Claude Code session
/my-command arg1 arg2

# Verify output
# Check for errors
# Test edge cases
```

**Test Agents**:
```
# Trigger agent with description match
"Can you help me with [agent domain]?"

# Verify agent activates
# Check agent behavior
# Test agent completion
```

**Test Skills**:
```
# Use trigger phrases from skill description
"I need to design an API"

# Verify skill loads (look for skill content in responses)
# Check skill guidance is helpful
```

**Test Hooks**:
```
# Perform action that should trigger hook
Write a file to .env
Run a dangerous bash command

# Verify hook executes
# Check hook output
# Confirm validation works
```

### Validation Checklist

Use this checklist for each plugin:

```markdown
## Plugin Testing Checklist

### Manifest
- [ ] plugin.json is valid JSON
- [ ] All required fields present (name, version, description)
- [ ] Author information complete
- [ ] Keywords relevant

### Commands
- [ ] All commands load without errors
- [ ] YAML frontmatter is valid
- [ ] Commands execute successfully
- [ ] Arguments parsed correctly
- [ ] Help text is clear
- [ ] Error handling works

### Agents
- [ ] All agents load without errors
- [ ] YAML frontmatter is valid
- [ ] Agents trigger on expected phrases
- [ ] Agents complete tasks successfully
- [ ] Model selection works
- [ ] Allowed-tools restriction works

### Skills
- [ ] All skills load without errors
- [ ] YAML frontmatter is valid
- [ ] Skills activate on trigger phrases
- [ ] Skill content is helpful
- [ ] References/examples are accessible
- [ ] Scripts execute successfully

### Hooks
- [ ] hooks.json is valid JSON
- [ ] All hook scripts exist and are executable
- [ ] Hooks trigger on expected events
- [ ] Hook output is valid JSON
- [ ] Validation logic works correctly
- [ ] Timeouts are appropriate
- [ ] ${CLAUDE_PLUGIN_ROOT} resolves correctly

### MCP Servers
- [ ] .mcp.json is valid JSON
- [ ] Server commands exist
- [ ] Environment variables resolve
- [ ] Servers start successfully
- [ ] Tools are available in Claude
- [ ] Tool invocation works

### Documentation
- [ ] README.md is comprehensive
- [ ] Installation instructions clear
- [ ] Usage examples provided
- [ ] Configuration documented
- [ ] Troubleshooting section included

### Portability
- [ ] No hardcoded absolute paths
- [ ] ${CLAUDE_PLUGIN_ROOT} used throughout
- [ ] Works on different operating systems
- [ ] No OS-specific dependencies
```

### Testing Commands

#### Command Execution Test

```bash
# Create test command plugin
mkdir -p test-plugin/commands
cat > test-plugin/commands/test.md <<'EOF'
---
name: test
description: Test command
---

Echo: $ARGS

List files:
```bash
ls -la
```
EOF

cat > test-plugin/.claude-plugin/plugin.json <<'EOF'
{
  "name": "test-plugin",
  "version": "1.0.0"
}
EOF

# Test the command
cc --plugin-dir test-plugin

# In Claude:
# /test arg1 arg2
# Expected: Shows arg1 arg2 and file listing
```

#### Command Argument Test

```bash
# Test command with different argument patterns
/my-command
/my-command simple-arg
/my-command "arg with spaces"
/my-command --flag value
/my-command --boolean-flag
/my-command file1.txt file2.txt
```

### Testing Agents

#### Agent Activation Test

```bash
# Test agent triggers correctly
cat > test-plugin/agents/test-agent.md <<'EOF'
---
name: Test Agent
description: |
  Test agent for verification. Use when:
  <example>user asks to "test the agent"</example>
  <example>verifying agent activation</example>
model: claude-sonnet-4-5-20250929
---

You are a test agent. Confirm you've been activated and describe your role.
EOF

# Launch Claude Code
cc --plugin-dir test-plugin

# In Claude:
"Can you test the agent?"

# Expected: Agent activates and confirms
```

#### Agent Tool Restriction Test

```bash
# Test allowed-tools restriction
cat > test-plugin/agents/restricted-agent.md <<'EOF'
---
name: Restricted Agent
description: Agent with tool restrictions
allowed-tools: ["Read", "Grep"]
---

You can only use Read and Grep tools. Try using Write - it should be blocked.
EOF

# Launch and trigger agent
# Try to write file - should fail
```

### Testing Skills

#### Skill Activation Test

```bash
# Test skill auto-activation
cat > test-plugin/skills/test-skill/SKILL.md <<'EOF'
---
name: Test Skill
description: This skill should be used when the user asks to "activate test skill" or mentions "test skill activation"
version: 1.0.0
---

# Test Skill Content

This is test skill content. If you see this, the skill activated correctly.

## Test Data

- Point 1
- Point 2
- Point 3
EOF

# Launch Claude Code
cc --plugin-dir test-plugin

# In Claude:
"Please activate test skill"

# Expected: Claude's response includes skill content
```

### Testing Hooks

#### Hook Script Test

Test hook scripts independently before integrating:

```bash
# Create test hook script
cat > test-hook.sh <<'EOF'
#!/bin/bash
set -euo pipefail

# Read input
input=$(cat)

# Extract tool name
tool_name=$(echo "$input" | jq -r '.tool_name // "unknown"')

# Log for debugging
echo "Hook received tool: $tool_name" >&2

# Validate
if [[ "$tool_name" == "Write" ]]; then
  file_path=$(echo "$input" | jq -r '.tool_input.file_path // ""')

  if [[ "$file_path" == *".env"* ]]; then
    cat <<JSON >&2
{
  "decision": "deny",
  "reason": "Cannot write to .env files",
  "systemMessage": "Writing to .env files is blocked for security."
}
JSON
    exit 2
  fi
fi

# Allow
cat <<JSON
{
  "decision": "approve",
  "systemMessage": "Validation passed"
}
JSON
exit 0
EOF

chmod +x test-hook.sh

# Test with sample input
echo '{
  "tool_name": "Write",
  "tool_input": {
    "file_path": "/tmp/.env",
    "content": "SECRET=value"
  }
}' | ./test-hook.sh

# Expected: Exit 2, deny decision
echo "Exit code: $?"

# Test with allowed file
echo '{
  "tool_name": "Write",
  "tool_input": {
    "file_path": "/tmp/test.txt",
    "content": "Hello"
  }
}' | ./test-hook.sh

# Expected: Exit 0, approve decision
echo "Exit code: $?"
```

#### Hook Integration Test

```bash
# Create hook plugin
mkdir -p test-plugin/hooks/scripts
cat > test-plugin/hooks/hooks.json <<'EOF'
{
  "description": "Test hooks",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Write",
        "hooks": [
          {
            "type": "command",
            "command": "bash ${CLAUDE_PLUGIN_ROOT}/hooks/scripts/validate.sh",
            "timeout": 10
          }
        ]
      }
    ]
  }
}
EOF

# Copy test hook script
cp test-hook.sh test-plugin/hooks/scripts/validate.sh

# Test in Claude Code
cc --plugin-dir test-plugin --debug

# In Claude:
"Write 'test' to /tmp/.env"
# Expected: Hook blocks write

"Write 'test' to /tmp/test.txt"
# Expected: Hook allows write
```

#### Prompt-Based Hook Test

```bash
# Test prompt-based hook
cat > test-plugin/hooks/hooks.json <<'EOF'
{
  "description": "Prompt-based validation",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          {
            "type": "prompt",
            "prompt": "Check if this bash command is dangerous (rm -rf, dd, mkfs, etc.). Return 'deny' if dangerous, 'approve' if safe.",
            "timeout": 30
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
            "prompt": "Check if the task is complete: tests run, build succeeded, questions answered. Return 'approve' to stop or 'block' with reason to continue."
          }
        ]
      }
    ]
  }
}
EOF

# Test dangerous command
cc --plugin-dir test-plugin

# In Claude:
"Run rm -rf /"
# Expected: Hook denies command

# Try to stop without running tests
"I'm done"
# Expected: Hook blocks stop, asks to run tests
```

### Testing MCP Servers

#### Local MCP Server Test

```bash
# Test stdio MCP server
cat > test-plugin/.mcp.json <<'EOF'
{
  "mcpServers": {
    "test-server": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-filesystem"],
      "env": {
        "ALLOWED_DIRECTORIES": "/tmp"
      }
    }
  }
}
EOF

# Launch Claude Code
cc --plugin-dir test-plugin --debug

# Look for server startup logs
# In Claude:
"List available tools"
# Expected: mcp__test-server__* tools listed

# Use MCP tool
"Read /tmp/test.txt using MCP"
# Expected: Tool invoked successfully
```

### Debug Mode Testing

Enable debug mode for detailed logging:

```bash
cc --plugin-dir /path/to/plugin --debug
```

**Debug Output Includes**:
- Plugin loading messages
- Component registration
- Hook execution logs
- MCP server startup
- Tool invocations
- Error stack traces

**Example Debug Output**:
```
[DEBUG] Loading plugin: my-plugin
[DEBUG] Registered command: /my-command
[DEBUG] Registered agent: my-agent
[DEBUG] Registered skill: my-skill
[DEBUG] Registered hook: PreToolUse (Write)
[DEBUG] Starting MCP server: my-server
[DEBUG] MCP server ready: my-server
[DEBUG] Hook triggered: PreToolUse (Write)
[DEBUG] Hook output: {"decision": "approve"}
```

## GitHub Workflow Testing

### Local Workflow Simulation

Test workflows locally before deploying:

#### 1. Install Act

```bash
# Install act for local GitHub Actions testing
brew install act
# or
curl https://raw.githubusercontent.com/nektos/act/master/install.sh | sudo bash
```

#### 2. Simulate Workflow Trigger

```bash
cd /home/me/unleash

# Simulate issue creation event
act issues -e test-event.json

# Create test event
cat > test-event.json <<'EOF'
{
  "action": "opened",
  "issue": {
    "number": 123,
    "title": "Test Issue",
    "body": "@marksverdhai please help with this test",
    "user": {
      "login": "testuser"
    }
  },
  "repository": {
    "full_name": "owner/repo"
  }
}
EOF
```

### Manual Workflow Testing

Use `workflow_dispatch` for controlled testing:

```bash
# Manually trigger mention workflow
gh workflow run mention-trigger.yml \
  -f issue_number=123 \
  -f test_prompt="Test the agent with this custom prompt"

# Monitor workflow execution
gh run watch

# View workflow logs
gh run view --log
```

### Creating Test Issues

Create test issues to verify agent behavior:

#### Test Issue Template

```markdown
Title: [TEST] Agent Response Test

Body:
@marksverdhai can you help with this test issue?

## Test Objectives
- Verify agent receives and processes mention
- Check agent reads issue context
- Confirm agent posts comment
- Validate response quality

## Expected Behavior
Agent should:
1. Post progress comment
2. Acknowledge test issue
3. Respond appropriately
4. Update progress to complete

## Actual Behavior
[To be filled by tester]
```

#### Test Scenarios

**Scenario 1: Simple Mention**
```
Issue: "@marksverdhai say hello"
Expected: Agent comments "Hello!"
```

**Scenario 2: Code Analysis**
```
Issue: "@marksverdhai analyze the auth code and suggest improvements"
Expected: Agent reads auth code, provides analysis
```

**Scenario 3: Bug Fix**
```
Issue: "@marksverdhai fix the login bug described in #456"
Expected: Agent reads #456, implements fix, creates PR
```

**Scenario 4: Assignment**
```
Issue: "Implement user export feature"
Assign to: @marksverdhai
Expected: Agent implements feature, creates PR
```

### Workflow Debugging

#### Enable Workflow Debug Logging

Add secret to enable debug logs:

```bash
# In repository settings, add secret:
ACTIONS_RUNNER_DEBUG: true
ACTIONS_STEP_DEBUG: true
```

#### View Detailed Logs

```bash
# View workflow run logs
gh run view <run-id> --log

# Download logs for analysis
gh run view <run-id> --log > workflow.log

# Search logs
grep "ERROR" workflow.log
grep "CLAUDE_PLUGIN_ROOT" workflow.log
```

#### Common Workflow Issues

**Issue: Workflow not triggering**

Check:
```yaml
# Verify trigger conditions
if: contains(github.event.issue.body, '@marksverdhai')

# Check event type
on:
  issues:
    types: [opened]  # Add other types as needed
```

**Issue: Secrets not available**

```bash
# Verify secrets exist
gh secret list

# Check secret values (in workflow)
env:
  HAS_PAT: ${{ secrets.HAI_GH_PAT != '' }}
  HAS_DOCKER: ${{ secrets.HEI_DOCKER_PAT != '' }}
  HAS_CLAUDE: ${{ secrets.CLAUDE_CREDENTIALS_JSON != '' }}
run: |
  echo "HAI_GH_PAT present: $HAS_PAT"
  echo "HEI_DOCKER_PAT present: $HAS_DOCKER"
  echo "CLAUDE_CREDENTIALS_JSON present: $HAS_CLAUDE"
```

## Integration Testing

### End-to-End Test Scenarios

Test complete workflows from trigger to completion:

#### Scenario 1: Bug Fix Workflow

```markdown
## Test: Complete Bug Fix Workflow

### Setup
1. Create issue: "Login button returns 404"
2. Label: bug
3. Assign to agent

### Steps
1. Agent receives assignment
2. Agent posts progress comment
3. Agent analyzes code
4. Agent implements fix
5. Agent runs tests
6. Agent creates PR
7. Agent comments on issue with PR link

### Verification
- [ ] Progress comment appears within 1 min
- [ ] Agent correctly identifies bug location
- [ ] Fix addresses root cause
- [ ] Tests pass
- [ ] PR description is clear
- [ ] PR linked to issue
- [ ] Code quality is good

### Cleanup
- Close issue
- Delete test branch
```

#### Scenario 2: PR Review Workflow

```markdown
## Test: PR Review Workflow

### Setup
1. Create PR with test changes
2. Mention agent: "@marksverdhai review this PR"

### Steps
1. Agent receives mention
2. Agent fetches PR details
3. Agent analyzes changes
4. Agent posts review comment
5. Agent requests changes if issues found

### Verification
- [ ] Agent reviews all changed files
- [ ] Feedback is specific and actionable
- [ ] Security issues identified
- [ ] Code quality issues noted
- [ ] Suggestions are helpful
- [ ] Review tone is constructive

### Cleanup
- Close PR
```

### Plugin + Workflow Integration

Test plugins in workflow context:

```bash
# Test that plugins work in containerized environment

# 1. Create plugin with workflow-specific functionality
mkdir -p test-plugin/commands
cat > test-plugin/commands/workflow-test.md <<'EOF'
---
name: workflow-test
description: Test workflow integration
---

# Workflow Integration Test

1. Check environment:
```bash
echo "Working directory: $PWD"
echo "Plugin root: $CLAUDE_PLUGIN_ROOT"
env | grep GITHUB
```

2. Test GitHub CLI:
```bash
gh --version
gh repo view
```

3. Test git:
```bash
git status
git log --oneline -5
```
EOF

# 2. Add plugin to repository
cp -r test-plugin plugins/bundled/

# 3. Commit and push
git add plugins/bundled/test-plugin
git commit -m "test: add workflow integration test plugin"
git push

# 4. Create test issue
gh issue create --title "[TEST] Workflow Plugin Integration" \
  --body "@marksverdhai run /workflow-test to verify plugin works in workflow"

# 5. Verify in workflow logs
gh run watch
gh run view --log | grep "CLAUDE_PLUGIN_ROOT"
```

## Debugging Tips

### General Debugging Strategies

#### 1. Start Simple

```bash
# Test with minimal plugin first
mkdir minimal-plugin
cat > minimal-plugin/.claude-plugin/plugin.json <<'EOF'
{"name": "minimal", "version": "1.0.0"}
EOF

cat > minimal-plugin/commands/hello.md <<'EOF'
---
name: hello
description: Say hello
---
Hello, world!
EOF

cc --plugin-dir minimal-plugin
# /hello
```

#### 2. Isolate Components

Test each component type separately:

```bash
# Test only commands
cc --plugin-dir plugin-with-only-commands

# Test only hooks
cc --plugin-dir plugin-with-only-hooks

# Test only MCP
cc --plugin-dir plugin-with-only-mcp
```

#### 3. Use Debug Mode

```bash
# Always use debug mode when troubleshooting
cc --plugin-dir /path/to/plugin --debug 2>&1 | tee debug.log

# Analyze debug log
grep ERROR debug.log
grep WARNING debug.log
grep "CLAUDE_PLUGIN_ROOT" debug.log
```

#### 4. Validate JSON Files

```bash
# Validate plugin.json
jq . plugin.json
echo "Valid JSON: $?"

# Validate hooks.json
jq . hooks/hooks.json
echo "Valid JSON: $?"

# Validate .mcp.json
jq . .mcp.json
echo "Valid JSON: $?"
```

#### 5. Check File Permissions

```bash
# Ensure scripts are executable
find . -name "*.sh" -exec chmod +x {} \;

# Verify permissions
ls -la hooks/scripts/
```

### Specific Debugging Scenarios

#### Commands Not Loading

**Symptoms**: Command not available in `/` menu

**Debug Steps**:
```bash
# 1. Verify file location
ls -la commands/

# 2. Check file extension
# Must be .md

# 3. Validate frontmatter
cat commands/my-command.md
# Should have --- delimiters and valid YAML

# 4. Check plugin loading
cc --plugin-dir . --debug 2>&1 | grep "Registered command"
```

#### Agents Not Triggering

**Symptoms**: Agent doesn't activate on expected phrases

**Debug Steps**:
```bash
# 1. Verify description has <example> blocks
cat agents/my-agent.md
# Should include: <example>trigger phrase</example>

# 2. Try exact trigger phrase
# In Claude: Use exact phrase from <example>

# 3. Check model selection
# Ensure model specified if needed

# 4. Verify agent loaded
cc --plugin-dir . --debug 2>&1 | grep "Registered agent"
```

#### Hooks Not Executing

**Symptoms**: Hook should trigger but doesn't

**Debug Steps**:
```bash
# 1. Validate hooks.json structure
jq . hooks/hooks.json

# 2. Check matcher pattern
# Use exact tool name: "Write", "Bash", etc.

# 3. Test script independently
echo '{"tool_name": "Write"}' | bash hooks/scripts/validate.sh
echo "Exit: $?"

# 4. Check script permissions
ls -la hooks/scripts/validate.sh
# Should be -rwxr-xr-x

# 5. Verify ${CLAUDE_PLUGIN_ROOT} resolves
cc --plugin-dir . --debug 2>&1 | grep "CLAUDE_PLUGIN_ROOT"

# 6. Check timeout isn't too short
# Increase timeout in hooks.json if needed
```

#### MCP Servers Not Starting

**Symptoms**: MCP tools not available

**Debug Steps**:
```bash
# 1. Validate .mcp.json
jq . .mcp.json

# 2. Test server command manually
npx -y @modelcontextprotocol/server-filesystem
# Should start without errors

# 3. Check environment variables
# Ensure all ${VAR} have values

# 4. Look for startup errors
cc --plugin-dir . --debug 2>&1 | grep -A 10 "MCP server"

# 5. Verify tool listing
# In Claude: "List available MCP tools"
```

#### ${CLAUDE_PLUGIN_ROOT} Not Resolving

**Symptoms**: "No such file or directory" errors

**Debug Steps**:
```bash
# 1. Check syntax in JSON
# Must be: ${CLAUDE_PLUGIN_ROOT}
# Not: $CLAUDE_PLUGIN_ROOT or {CLAUDE_PLUGIN_ROOT}

# 2. Verify used in correct context
# OK: JSON files (hooks.json, .mcp.json)
# OK: Executed scripts (as environment variable)
# Not OK: Markdown files (use literal path descriptions)

# 3. Test resolution
cc --plugin-dir . --debug 2>&1 | grep "CLAUDE_PLUGIN_ROOT"
```

### Logging and Diagnostics

#### Add Logging to Scripts

```bash
#!/bin/bash
set -euo pipefail

# Add logging
exec 2> >(tee -a /tmp/hook-debug.log >&2)

echo "[DEBUG] Hook script started at $(date)" >&2
echo "[DEBUG] Plugin root: $CLAUDE_PLUGIN_ROOT" >&2

input=$(cat)
echo "[DEBUG] Input: $input" >&2

# ... rest of script

echo "[DEBUG] Hook script completed" >&2
```

#### Capture All Output

```bash
# Run Claude Code with full logging
cc --plugin-dir . --debug 2>&1 | tee full-debug.log

# Analyze log
grep "\[DEBUG\]" full-debug.log
grep "\[ERROR\]" full-debug.log
grep "\[WARNING\]" full-debug.log
```

## Common Issues and Solutions

### Issue: Plugin Not Loading

**Symptoms**:
- Plugin doesn't appear in loaded plugins
- Components not available

**Solutions**:
```bash
# 1. Verify manifest location and format
test -f .claude-plugin/plugin.json && echo "Manifest exists"
jq . .claude-plugin/plugin.json && echo "Valid JSON"

# 2. Check plugin name
jq -r '.name' .claude-plugin/plugin.json
# Should be kebab-case, no spaces

# 3. Verify plugin directory structure
tree .
# Should have .claude-plugin/ at root

# 4. Check --plugin-dir path
# Must be absolute path or correct relative path
cc --plugin-dir "$(pwd)"
```

### Issue: Command Fails with "Tool not allowed"

**Symptoms**:
- Command tries to use tool
- Error: "Tool X is not in allowed-tools list"

**Solutions**:
```markdown
<!-- Add to command frontmatter -->
---
name: my-command
description: Command description
allowed-tools: ["Read", "Write", "Bash", "Grep", "Glob"]
---
```

### Issue: Hook Timeout

**Symptoms**:
- Hook takes too long
- Timeout error in logs

**Solutions**:
```json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Write",
      "hooks": [{
        "type": "command",
        "command": "bash script.sh",
        "timeout": 60  // Increase from default 30
      }]
    }]
  }
}
```

### Issue: MCP Server Environment Variable Not Set

**Symptoms**:
- Server fails to start
- Missing environment variable error

**Solutions**:
```bash
# 1. Set environment variable before launching
export MY_API_KEY="..."
cc --plugin-dir .

# 2. Or use .env file
cat > .env <<EOF
MY_API_KEY=...
EOF

# 3. Verify in .mcp.json
cat > .mcp.json <<EOF
{
  "mcpServers": {
    "my-server": {
      "env": {
        "API_KEY": "\${MY_API_KEY}"
      }
    }
  }
}
EOF
```

### Issue: Workflow Can't Access Plugin

**Symptoms**:
- Plugin works locally
- Fails in GitHub Actions

**Solutions**:
```bash
# 1. Ensure plugin committed to repository
git add plugins/bundled/my-plugin
git commit -m "Add my-plugin"
git push

# 2. Verify plugin in repository
gh repo view --web
# Navigate to plugins/bundled/

# 3. Check workflow mounts plugin directory
# In spawn-agent.yml, verify repository is mounted

# 4. Test workflow manually
gh workflow run mention-trigger.yml -f issue_number=123
```

### Issue: Secrets Not Available in Workflow

**Symptoms**:
- Workflow fails with authentication error
- Secrets appear empty

**Solutions**:
```bash
# 1. Verify secrets exist
gh secret list

# 2. Add missing secrets
gh secret set HAI_GH_PAT < pat.txt
gh secret set CLAUDE_CREDENTIALS_JSON < creds.json

# 3. For organization secrets, check repository access
# Go to org settings > Secrets > Select secret
# Verify repository has access

# 4. Test secret availability in workflow
# Add debug step:
steps:
  - name: Check secrets
    run: |
      echo "HAI_GH_PAT: ${{ secrets.HAI_GH_PAT != '' }}"
      echo "CLAUDE_CREDENTIALS_JSON: ${{ secrets.CLAUDE_CREDENTIALS_JSON != '' }}"
```

### Issue: Progress Comment Not Updating

**Symptoms**:
- Initial comment appears
- No updates during execution

**Solutions**:
```bash
# 1. Verify comment ID is passed
# In extract-context job:
outputs:
  progress_comment_id: ${{ steps.post-progress-comment.outputs.comment_id }}

# 2. Check agent receives comment ID
# In spawn-agent call:
with:
  progress_comment_id: ${{ needs.extract-context.outputs.progress_comment_id }}

# 3. Verify PAT has issues:write permission
gh auth status
```

### Issue: Agent Makes Unexpected Changes

**Symptoms**:
- Agent modifies wrong files
- Changes don't match request

**Solutions**:
```markdown
<!-- Create/update CLAUDE.md with guidelines -->
# Agent Guidelines

## Code Style
- Follow PEP 8 for Python
- Use ESLint rules for JavaScript
- Run formatter before committing

## Testing Requirements
- All new features need tests
- Maintain >80% coverage
- Run full test suite before PR

## File Restrictions
- Never modify files in vendor/
- Don't commit to .env files
- Ask before changing core configs

## Workflow
1. Read issue/PR carefully
2. Ask questions if unclear
3. Make minimal changes
4. Test thoroughly
5. Create descriptive PR
```

```bash
# Add validation hooks
cat > .claude/hookify.dangerous-paths.local.md <<'EOF'
---
name: protect-critical-paths
enabled: true
event: file
action: block
conditions:
  - field: file_path
    operator: regex_match
    pattern: vendor/|node_modules/|\.git/
---

**Critical path modification blocked**

These directories should not be modified directly.
EOF
```

## Test Automation

### Automated Plugin Tests

Create test suite for plugins:

```bash
#!/bin/bash
# test-plugin.sh

set -euo pipefail

PLUGIN_DIR="${1:?Plugin directory required}"

echo "Testing plugin: $PLUGIN_DIR"

# Test 1: Validate manifest
echo "Checking manifest..."
test -f "$PLUGIN_DIR/.claude-plugin/plugin.json" || {
  echo "ERROR: Missing plugin.json"
  exit 1
}
jq . "$PLUGIN_DIR/.claude-plugin/plugin.json" > /dev/null || {
  echo "ERROR: Invalid plugin.json"
  exit 1
}

# Test 2: Validate hooks if present
if [ -f "$PLUGIN_DIR/hooks/hooks.json" ]; then
  echo "Validating hooks.json..."
  jq . "$PLUGIN_DIR/hooks/hooks.json" > /dev/null || {
    echo "ERROR: Invalid hooks.json"
    exit 1
  }
fi

# Test 3: Check hook scripts are executable
if [ -d "$PLUGIN_DIR/hooks/scripts" ]; then
  echo "Checking hook script permissions..."
  find "$PLUGIN_DIR/hooks/scripts" -type f -name "*.sh" ! -executable | while read -r script; do
    echo "WARNING: Script not executable: $script"
  done
fi

# Test 4: Validate MCP config if present
if [ -f "$PLUGIN_DIR/.mcp.json" ]; then
  echo "Validating .mcp.json..."
  jq . "$PLUGIN_DIR/.mcp.json" > /dev/null || {
    echo "ERROR: Invalid .mcp.json"
    exit 1
  }
fi

# Test 5: Check for README
test -f "$PLUGIN_DIR/README.md" || {
  echo "WARNING: Missing README.md"
}

echo "✅ Plugin validation passed"
```

### Continuous Integration

Add to GitHub Actions:

```yaml
name: Test Plugins

on:
  pull_request:
    paths:
      - 'plugins/bundled/**'

jobs:
  test-plugins:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install jq
        run: sudo apt-get install -y jq

      - name: Test changed plugins
        run: |
          for plugin in plugins/bundled/*/; do
            echo "Testing $plugin"
            ./scripts/test-plugin.sh "$plugin"
          done
```

## Summary

### Testing Priorities

1. **Local Testing First**: Always test locally before deploying
2. **Incremental Testing**: Test components individually, then together
3. **Debug Mode**: Use `--debug` to understand behavior
4. **Automate Validation**: Create test scripts for repetitive checks
5. **Document Issues**: Keep track of problems and solutions

### Best Practices

- ✅ Test in clean environment
- ✅ Use debug mode liberally
- ✅ Validate JSON files
- ✅ Check file permissions
- ✅ Test error cases
- ✅ Verify environment variables
- ✅ Create reproducible tests
- ✅ Document test procedures

### Resources

- [Plugin Development Guide](./plugin-development.md)

- Claude Code Documentation
- GitHub Actions Documentation
