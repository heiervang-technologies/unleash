# Snail Integration Guide

## Overview

The agent-unleashed repository integrates with snail-core-template to enable GitHub Actions-based AI agent automation. This guide explains how the snail system works, how to configure workflows, and how plugins enhance the agent's capabilities.

## Table of Contents

1. [Snail-Core Architecture](#snail-core-architecture)
2. [GitHub Actions Workflow Integration](#github-actions-workflow-integration)
3. [How Plugins Enhance Workflows](#how-plugins-enhance-workflows)
4. [Available MCP Servers](#available-mcp-servers)
5. [Example Commands and Agents](#example-commands-and-agents)
6. [Configuration and Secrets](#configuration-and-secrets)
7. [Workflow Customization](#workflow-customization)

## Snail-Core Architecture

### What is Snail?

Snail is an AI agent automation system that runs Claude Code in GitHub Actions workflows. When users interact with the agent through GitHub (mentions, assignments, comments), snail spawns a Claude Code session to handle the request.

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    GitHub Repository                        │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  User Action:                                               │
│  - Mention @agent in issue/PR                               │
│  - Assign issue/PR to agent                                 │
│  - Comment on issue/PR                                      │
│                                                             │
│         │                                                   │
│         ▼                                                   │
│  ┌─────────────────────────┐                                │
│  │   Workflow Trigger      │                                │
│  │  (mention-trigger.yml)  │                                │
│  │  (assignment-trigger.yml)│                                │
│  └──────────┬──────────────┘                                │
│             │                                                │
│             │ Extract context, build prompt                 │
│             ▼                                                │
│  ┌─────────────────────────────────────────────────────┐    │
│  │     Reusable Workflow (spawn-agent.yml)             │    │
│  │     Location: heiervang-technologies/core           │    │
│  │                                                      │    │
│  │  1. Pull snail Docker container                      │    │
│  │  2. Mount repository                                 │    │
│  │  3. Run Claude Code with prompt                      │    │
│  │  4. Update progress comments                         │    │
│  │  5. Post results to issue/PR                         │    │
│  └──────────────────────────────────────────────────────┘    │
│                                                             │
│  Results:                                                   │
│  - Comments on issues/PRs                                   │
│  - Code changes via PRs                                     │
│  - Progress tracking                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Key Components

1. **Workflow Triggers**: GitHub Actions workflows that detect user interactions
2. **Context Extraction**: Parse issue/PR content and build prompt for Claude
3. **Spawn Agent**: Reusable workflow that runs Claude Code in container
4. **Progress Tracking**: Real-time comments showing agent status
5. **Result Posting**: Agent responses posted as GitHub comments

## GitHub Actions Workflow Integration

### Available Workflows

The agent-unleashed repository includes these workflows:

#### 1. Mention Trigger (`mention-trigger.yml`)

Triggers when agent is mentioned in:
- Issue bodies (when created)
- Issue comments
- Pull request review comments

**Example**:
```
@marksverdhai please help fix this bug
```

**Flow**:
1. User mentions @agent
2. Workflow extracts mention and context
3. Builds prompt: "User mentioned you in issue #123: [content]"
4. Spawns agent with prompt
5. Agent reads issue, analyzes, responds
6. Posts comment with findings

#### 2. Assignment Trigger (`assignment-trigger.yml`)

Triggers when issue or PR is assigned to agent.

**Example**:
1. Create issue
2. Assign to @marksverdhai
3. Agent automatically works on it

**Flow**:
1. User assigns issue/PR to agent
2. Workflow detects assignment
3. Builds prompt: "You've been assigned to issue #123: [title]"
4. Spawns agent with prompt
5. Agent analyzes, implements fix
6. Creates PR or comments with solution

#### 3. Setup Check (`setup-check.yml`)

Runs on first push to verify credentials and configuration.

**Flow**:
1. Checks for required secrets
2. Validates GitHub PAT permissions
3. Validates Claude credentials
4. Creates setup issue if anything missing
5. Closes setup issue when complete

### Workflow File Structure

**Mention Trigger Example** (`.github/workflows/mention-trigger.yml`):

```yaml
name: Mention Trigger

on:
  issues:
    types: [opened]
  issue_comment:
    types: [created]
  pull_request_review_comment:
    types: [created]
  workflow_dispatch:  # Manual trigger for testing

permissions:
  contents: read
  issues: write
  pull-requests: write

jobs:
  extract-context:
    runs-on: ubuntu-latest
    # Check if agent was mentioned
    if: contains(github.event.issue.body, '@marksverdhai') ||
        contains(github.event.comment.body, '@marksverdhai')
    outputs:
      prompt: ${{ steps.build-prompt.outputs.prompt }}
      issue_number: ${{ steps.extract.outputs.issue_number }}

    steps:
      - name: Extract context
        id: extract
        # Parse issue/comment, extract content

      - name: Post progress tracking comment
        # Create comment showing agent is working

      - name: Build prompt
        id: build-prompt
        # Build prompt for Claude with context

  spawn-agent:
    needs: extract-context
    uses: heiervang-technologies/core/.github/workflows/spawn-agent.yml@main
    with:
      prompt: ${{ needs.extract-context.outputs.prompt }}
      agent_name: 'marksverdhai'
      agent_type: 'single-task'
    secrets: inherit
```

### Key Workflow Features

#### Progress Tracking

The workflows post progress comments that update in real-time:

```markdown
## 🟡 Agent Pending

**Issue #123**: Fix login bug
[View workflow run](https://github.com/repo/actions/runs/123)

### Progress
`██████░░░░` 60%

| Task | Status |
|------|--------|
| ✅ Analyzing code | 🟢 Complete |
| ⏳ Implementing fix | 🟡 In Progress |
| ⏸️ Running tests | 🔵 Pending |

### Time
- **Started**: 2026-01-01T10:00:00Z
- **Estimated remaining**: 5 minutes
```

#### Error Handling

On failure, progress comment updates:

```markdown
## 🔴 Agent Failed

[View workflow run](https://github.com/repo/actions/runs/123)

### Progress
`░░░░░░░░░░` Failed

| Task | Status |
|------|--------|
| Agent execution | 🔴 Failed |

### Time
- **Failed at**: 2026-01-01T10:15:00Z

---
*Please check the workflow logs for details.*
```

#### Manual Testing

Workflows include `workflow_dispatch` for manual testing:

```bash
# Trigger workflow manually for testing
gh workflow run mention-trigger.yml \
  -f issue_number=123 \
  -f test_prompt="Test the agent with custom prompt"
```

## How Plugins Enhance Workflows

Plugins significantly enhance what the agent can do in GitHub workflows.

### Workflow-Relevant Plugins

#### 1. Auto Mode Plugin

**Location**: `plugins/unleashed/auto-mode/`

**Enhances**: Autonomous operation

**Commands**:
- `/auto`: Toggle autonomous mode
- `/auto:status`: Check auto-mode status

**Usage in Workflow**:
Enables Claude to work through multiple steps without requiring user confirmation for every tool use.

#### 2. Process Restart Plugin

**Location**: `plugins/unleashed/process-restart/`

**Enhances**: Session stability

**Commands**:
- `/restart`: Restart Claude while preserving session state

**Usage in Workflow**:
Allows Claude to self-restart to apply configuration changes (like new MCP servers) without losing the current task context.

#### 3. MCP Refresh Plugin

**Location**: `plugins/unleashed/mcp-refresh/`

**Enhances**: MCP configuration management

**Commands**:
- `/reload-mcps`: Detect and report MCP configuration changes
- `/mcp-status`: Show current MCP server status

**Usage in Workflow**:
Ensures the agent is aware of available tools and can detect when new capabilities are added.

### Custom Plugins for Workflows

Create workflow-specific plugins:

#### Example: Issue Template Validator

**Plugin**: `issue-validator`
**Purpose**: Ensure issues follow templates

**Hook** (`hooks/hooks.json`):
```json
{
  "hooks": {
    "SessionStart": [{
      "matcher": "*",
      "hooks": [{
        "type": "command",
        "command": "bash ${CLAUDE_PLUGIN_ROOT}/scripts/check-issue-template.sh"
      }]
    }]
  }
}
```

**Script** (`scripts/check-issue-template.sh`):
```bash
#!/bin/bash
# Check if issue follows template
# Post reminder comment if not
# Provide template to agent
```

#### Example: PR Size Checker

**Plugin**: `pr-size-checker`
**Purpose**: Warn about large PRs

**Hook**:
```json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Bash",
      "hooks": [{
        "type": "prompt",
        "prompt": "If creating PR, check diff size. Warn if >500 lines. Suggest splitting."
      }]
    }]
  }
}
```

## Available MCP Servers

MCP (Model Context Protocol) servers extend agent capabilities with external tools.

### GitHub MCP Server

**Purpose**: Enhanced GitHub API access

**Configuration** (`.mcp.json`):
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

**Tools Provided**:
- `mcp__github__create_issue`
- `mcp__github__update_issue`
- `mcp__github__create_pull_request`
- `mcp__github__create_or_update_file`
- `mcp__github__search_repositories`
- `mcp__github__get_issue`
- `mcp__github__list_commits`

**Usage in Agent**:
```
Agent can:
- Create issues automatically
- Update issue labels/assignees
- Search codebase
- Get commit history
- Create files directly via API
```

### Asana MCP Server

**Purpose**: Project management integration

**Configuration**:
```json
{
  "mcpServers": {
    "asana": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-asana"],
      "env": {
        "ASANA_ACCESS_TOKEN": "${ASANA_ACCESS_TOKEN}"
      }
    }
  }
}
```

**Use Case**:
- Agent can sync GitHub issues with Asana tasks
- Update project status
- Track time estimates

### Slack MCP Server

**Purpose**: Team communication

**Configuration**:
```json
{
  "mcpServers": {
    "slack": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-slack"],
      "env": {
        "SLACK_BOT_TOKEN": "${SLACK_BOT_TOKEN}"
      }
    }
  }
}
```

**Use Case**:
- Agent posts updates to Slack
- Notifies team of PR creation
- Escalates blockers

### Database MCP Server

**Purpose**: Direct database access

**Configuration**:
```json
{
  "mcpServers": {
    "postgres": {
      "command": "npx",
      "args": ["-y", "@modelcontextprotocol/server-postgres"],
      "env": {
        "POSTGRES_CONNECTION_STRING": "${DATABASE_URL}"
      }
    }
  }
}
```

**Use Case**:
- Agent can query database for debugging
- Analyze data issues
- Generate reports

### Custom MCP Servers

Create custom servers for your workflow:

**Example: Deployment Server**

```javascript
// servers/deployment-server.js
import { Server } from '@modelcontextprotocol/sdk/server/index.js';
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js';

const server = new Server({
  name: 'deployment-server',
  version: '1.0.0',
});

server.setRequestHandler('tools/list', async () => ({
  tools: [
    {
      name: 'deploy_to_staging',
      description: 'Deploy application to staging environment',
      inputSchema: {
        type: 'object',
        properties: {
          version: { type: 'string' },
          skipTests: { type: 'boolean', default: false },
        },
      },
    },
    {
      name: 'rollback_deployment',
      description: 'Rollback to previous deployment',
      inputSchema: {
        type: 'object',
        properties: {
          environment: { type: 'string', enum: ['staging', 'production'] },
        },
      },
    },
  ],
}));

server.setRequestHandler('tools/call', async (request) => {
  if (request.params.name === 'deploy_to_staging') {
    // Deployment logic
    return {
      content: [{ type: 'text', text: 'Deployed successfully!' }],
    };
  }
  // ... other tools
});

const transport = new StdioServerTransport();
await server.connect(transport);
```

**Configuration**:
```json
{
  "mcpServers": {
    "deployment": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/servers/deployment-server.js"],
      "env": {
        "DEPLOY_API_KEY": "${DEPLOY_API_KEY}"
      }
    }
  }
}
```

## Example Commands and Agents

### Example 1: Issue Triage Command

**Command**: `/triage-issue`

**File**: `plugins/issue-toolkit/commands/triage-issue.md`

```markdown
---
name: triage-issue
description: Analyze and triage GitHub issue
argument-hint: "[issue_number]"
allowed-tools: ["Bash", "Read", "mcp__github__get_issue"]
---

# Issue Triage Command

Analyze a GitHub issue and provide triage recommendations.

## Process

1. **Fetch Issue Details**
```bash
gh issue view $ISSUE_NUMBER --json title,body,labels,comments
```

2. **Analyze Content**
   - Severity: How critical is this?
   - Impact: How many users affected?
   - Clarity: Is the issue well-described?
   - Reproducibility: Can we reproduce it?

3. **Determine Priority**
   - **P0**: Critical, system down, data loss
   - **P1**: High impact, major feature broken
   - **P2**: Medium impact, minor bugs
   - **P3**: Low impact, enhancements

4. **Suggest Labels**
   - Type: bug, feature, documentation, etc.
   - Component: auth, api, ui, etc.
   - Priority: p0, p1, p2, p3

5. **Recommend Assignment**
   Based on component and expertise

6. **Update Issue**
```bash
gh issue edit $ISSUE_NUMBER \
  --add-label "bug,p1,auth" \
  --add-assignee "@expert-user"
```

7. **Comment with Analysis**
```bash
gh issue comment $ISSUE_NUMBER --body "
## Triage Analysis

**Priority**: P1 (High)
**Type**: Bug
**Component**: Authentication

**Reasoning**:
- Affects login for all users (high impact)
- Clear reproduction steps provided
- Appears to be regression from recent change

**Recommendation**:
Assign to @auth-team for immediate investigation.
Estimated effort: 2-4 hours.
"
```
```

**Usage in Workflow**:
```
New issue created: #456

Agent workflow:
1. Detects new issue
2. Uses /triage-issue 456
3. Posts triage analysis
4. Updates labels and assignment
5. Notifies appropriate team
```

### Example 2: PR Review Agent

**Agent**: `pr-review-agent`

**File**: `plugins/pr-toolkit/agents/pr-review-agent.md`

```markdown
---
name: PR Review Agent
description: |
  Comprehensive PR review agent. Use when:
  <example>reviewing pull requests</example>
  <example>analyzing code changes</example>
  <example>checking PR quality</example>
model: claude-sonnet-4-5-20250929
color: purple
allowed-tools: ["Bash", "Read", "Grep", "Glob", "mcp__github__*"]
---

# Pull Request Review Expert

You are an expert code reviewer specializing in comprehensive PR analysis.

## Review Process

### 1. Fetch PR Context
```bash
gh pr view $PR_NUMBER --json title,body,files,commits,reviews,comments
gh pr diff $PR_NUMBER
```

### 2. Analyze Changes

**Code Quality**:
- Are changes well-structured and maintainable?
- Is code style consistent with project?
- Are there any code smells?

**Testing**:
- Are there tests for new functionality?
- Do tests cover edge cases?
- Are existing tests updated if needed?

**Documentation**:
- Is new functionality documented?
- Are comments clear and necessary?
- Is README updated if needed?

**Security**:
- Any security vulnerabilities introduced?
- Sensitive data properly handled?
- Input validation present?

**Performance**:
- Any performance concerns?
- Database queries optimized?
- Are there N+1 query issues?

### 3. Check PR Metadata

- Does title follow conventional commits?
- Is description clear and complete?
- Are related issues linked?
- Is size reasonable (<500 lines)?

### 4. Provide Structured Feedback

**Format**:
```markdown
## PR Review Summary

### Overall Assessment
[APPROVE / REQUEST_CHANGES / COMMENT]

### Strengths
- Well-tested with comprehensive coverage
- Clear documentation added
- Follows project conventions

### Issues Found

#### Critical 🔴
- Security: SQL injection vulnerability in line 45
- Bug: Null pointer exception not handled in line 67

#### Important 🟡
- Performance: Consider caching database query results
- Testing: Missing edge case tests for empty input

#### Suggestions 💡
- Consider extracting helper function for repeated logic
- Add inline comments explaining complex algorithm

### Detailed Comments

**File: src/auth/login.ts**
- Line 45: Use parameterized queries to prevent SQL injection
- Line 67: Add null check before accessing user.email

**File: src/auth/login.test.ts**
- Add test case for empty email input
- Add test case for malformed password

### Checklist
- [x] Code quality is good
- [x] Tests are present
- [ ] Security issues need addressing
- [x] Documentation is complete
- [ ] Performance could be improved
```

### 5. Post Review

```bash
gh pr review $PR_NUMBER \
  --comment \
  --body "$(cat review.md)"
```

### 6. Request Changes if Needed

If critical issues found:
```bash
gh pr review $PR_NUMBER \
  --request-changes \
  --body "Critical security issues must be addressed before merge."
```

## Best Practices

- Be specific with feedback (line numbers, examples)
- Explain why, not just what
- Suggest solutions, don't just criticize
- Acknowledge good work
- Prioritize issues (critical vs. suggestions)
- Be constructive and respectful
```

**Usage in Workflow**:
```
PR created or assigned to agent

Agent workflow:
1. pr-review-agent activates
2. Fetches PR details
3. Analyzes all changes
4. Posts comprehensive review
5. Requests changes or approves
```

### Example 3: Automated Bug Fix Agent

**Agent**: `bugfix-agent`

**File**: `plugins/bug-toolkit/agents/bugfix-agent.md`

```markdown
---
name: Bug Fix Agent
description: |
  Automatically analyzes and fixes bugs. Use when:
  <example>issue labeled with "bug"</example>
  <example>assigned issue is a bug report</example>
  <example>reproducing and fixing bugs</example>
model: claude-sonnet-4-5-20250929
allowed-tools: ["Read", "Write", "Edit", "Bash", "Grep", "Glob"]
---

# Automated Bug Fix Agent

You are an expert at analyzing bug reports, reproducing issues, and implementing fixes.

## Bug Fix Workflow

### Phase 1: Understand the Bug

1. **Read Issue Details**:
```bash
gh issue view $ISSUE_NUMBER
```

2. **Extract Information**:
   - What is the expected behavior?
   - What is the actual behavior?
   - Steps to reproduce?
   - Error messages or stack traces?
   - Environment details?

3. **Ask Questions if Unclear**:
   If reproduction steps or expected behavior unclear, comment:
   ```bash
   gh issue comment $ISSUE_NUMBER --body "
   To better understand and fix this bug, could you clarify:
   - What specific action triggers the error?
   - What error message do you see?
   - What version are you using?
   "
   ```

### Phase 2: Reproduce the Bug

1. **Set Up Environment**:
   Follow project setup instructions

2. **Reproduce Issue**:
   Execute steps from bug report

3. **Verify Bug Exists**:
   Document actual behavior

4. **Update Issue**:
   ```bash
   gh issue comment $ISSUE_NUMBER --body "
   ✅ Bug reproduced successfully

   Confirmed behavior:
   - [Actual behavior observed]

   Investigating root cause...
   "
   ```

### Phase 3: Identify Root Cause

1. **Analyze Code**:
   - Use Grep to find relevant code
   - Read implementation
   - Check recent changes (git blame)

2. **Identify Issue**:
   - What is causing the bug?
   - Is it a logic error, typo, edge case?

3. **Plan Fix**:
   - How to address root cause?
   - Are tests needed?
   - Any breaking changes?

### Phase 4: Implement Fix

1. **Create Fix Branch**:
```bash
git checkout -b fix/issue-$ISSUE_NUMBER
```

2. **Make Changes**:
   - Edit code to fix bug
   - Keep changes minimal and focused
   - Add comments if logic is complex

3. **Add Tests**:
   - Add test that reproduces bug (should fail before fix)
   - Verify test passes after fix
   - Add edge case tests

### Phase 5: Verify Fix

1. **Run Tests**:
```bash
npm test
# or pytest, cargo test, etc.
```

2. **Manual Testing**:
   - Verify original bug is fixed
   - Check no regressions introduced
   - Test edge cases

3. **Code Review Self-Check**:
   - Is fix minimal and focused?
   - Are edge cases handled?
   - Is code style consistent?

### Phase 6: Submit Fix

1. **Commit Changes**:
```bash
git add .
git commit -m "fix: [brief description of fix]

Fixes #$ISSUE_NUMBER

[Detailed explanation of root cause and fix]
"
```

2. **Push and Create PR**:
```bash
git push -u origin fix/issue-$ISSUE_NUMBER
gh pr create --title "fix: [description]" --body "
## Summary
Fixes #$ISSUE_NUMBER

## Root Cause
[Explanation of what was causing the bug]

## Changes
- [List of changes made]

## Testing
- Added test case that reproduces original bug
- Verified test fails before fix, passes after
- All existing tests pass
- Manual testing confirms bug is fixed

## Screenshots/Logs
[If applicable]
"
```

3. **Link PR to Issue**:
   PR description includes "Fixes #$ISSUE_NUMBER"

4. **Update Issue**:
```bash
gh issue comment $ISSUE_NUMBER --body "
✅ Fix implemented and tested

Created PR #$PR_NUMBER with fix.

**Root cause**: [Brief explanation]
**Solution**: [Brief description of fix]

Please review and let me know if this resolves the issue!
"
```

## Special Cases

### Cannot Reproduce Bug

```bash
gh issue comment $ISSUE_NUMBER --body "
❌ Unable to reproduce this bug

I followed these steps:
1. [Step 1]
2. [Step 2]

Expected to see: [Expected behavior from issue]
Actually saw: [What actually happened]

Could you provide:
- More detailed reproduction steps?
- Screenshots or error messages?
- Your exact environment (OS, browser, version)?
"

gh issue edit $ISSUE_NUMBER --add-label "needs-info,cannot-reproduce"
```

### Complex Bug Requiring Design Discussion

```bash
gh issue comment $ISSUE_NUMBER --body "
This bug involves [complex aspect]. The fix requires architectural changes:

**Options**:
1. [Option 1]: [pros/cons]
2. [Option 2]: [pros/cons]

Recommend discussing approach before implementing.
"

gh issue edit $ISSUE_NUMBER --add-label "needs-design-decision"
```
```

**Usage in Workflow**:
```
Issue created: "Login button doesn't work"
Labeled: bug
Assigned to agent

Agent workflow:
1. bugfix-agent activates
2. Reads issue
3. Reproduces bug
4. Finds root cause
5. Implements fix with tests
6. Creates PR
7. Comments on issue with PR link
```

## Configuration and Secrets

### Required Secrets

Configure these as GitHub organization or repository secrets:

#### 1. HAI_GH_PAT

**Purpose**: GitHub Personal Access Token for agent actions

**Scopes Required**:
- `repo`: Full control of repositories
- `workflow`: Update GitHub Actions workflows

**Setup**:
1. Go to GitHub Settings > Developer settings > Personal access tokens
2. Generate new token (classic)
3. Select scopes: `repo`, `workflow`
4. Copy token
5. Add as secret: `HAI_GH_PAT`

**Used For**:
- Creating issues/PRs
- Commenting on issues/PRs
- Updating labels/assignees
- Pushing code changes

#### 2. HEI_DOCKER_PAT

**Purpose**: Docker Hub access for pulling snail container

**Setup**:
1. Go to Docker Hub > Account Settings > Security
2. Create new access token
3. Select: Read-only access
4. Copy token
5. Add as secret: `HEI_DOCKER_PAT`

**Used For**:
- Pulling snail Docker image
- No write access needed

#### 3. CLAUDE_CREDENTIALS_JSON

**Purpose**: Claude API credentials for agent

**Format**:
```json
{
  "claudeAiOauth": {
    "accessToken": "your-access-token",
    "refreshToken": "your-refresh-token",
    "expiresAt": 1234567890000
  }
}
```

**Setup**:
1. Obtain credentials from Claude Code
2. Format as JSON
3. Add as secret: `CLAUDE_CREDENTIALS_JSON`

**Used For**:
- Authenticating with Claude API
- Running Claude Code in container

### Optional Secrets

For MCP servers and integrations:

```yaml
# Asana integration
ASANA_ACCESS_TOKEN: "..."

# Slack integration
SLACK_BOT_TOKEN: "xoxb-..."

# Database access
DATABASE_URL: "postgres://..."

# Deployment
DEPLOY_API_KEY: "..."
```

### Environment Variables

Available in agent workflows:

- `GITHUB_TOKEN`: Automatic GitHub Actions token
- `GITHUB_REPOSITORY`: Current repository (owner/repo)
- `GITHUB_ACTOR`: User who triggered workflow
- `GITHUB_EVENT_NAME`: Event that triggered (issues, issue_comment, etc.)

## Workflow Customization

### Customizing Agent Username

**In all workflow files**, update agent username:

```yaml
# mention-trigger.yml
if: contains(github.event.issue.body, '@YOUR-AGENT-NAME')

# Later in file:
PROMPT=$(printf '%s' "$BODY" | sed 's/@YOUR-AGENT-NAME//g')

# In spawn-agent call:
agent_name: 'YOUR-AGENT-NAME'
```

### Customizing Assignment Filters

**In assignment-trigger.yml**, update filters:

```yaml
if: |
  github.event.assignee.login == 'YOUR-AGENT-NAME' &&
  (github.event.sender.login == 'ALLOWED-USER-1' ||
   github.event.sender.login == 'ALLOWED-USER-2')
```

### Adding Custom Labels

Filter by labels to specialize agent:

```yaml
# Only handle bugs
if: |
  contains(github.event.issue.body, '@agent') &&
  contains(github.event.issue.labels.*.name, 'bug')
```

### Custom Prompts

Customize the prompt built for Claude:

```yaml
- name: Build prompt
  env:
    ISSUE_NUMBER: ${{ steps.extract.outputs.issue_number }}
  run: |
    PROMPT="Custom instructions for your agent:

    You've been mentioned in issue #${ISSUE_NUMBER}.

    Your specific instructions:
    1. Always check for test coverage
    2. Follow our coding standards in CONTRIBUTING.md
    3. Update documentation in docs/
    4. Create PRs with detailed descriptions

    Now handle the issue:
    $(gh issue view ${ISSUE_NUMBER})
    "

    echo "prompt=$PROMPT" >> $GITHUB_OUTPUT
```

### Agent Types

Configure agent behavior:

```yaml
agent_type: 'single-task'  # Completes one task and stops
# or
agent_type: 'worker'        # Can iterate on feedback
```

## Best Practices

### Workflow Design

- ✅ Use progress comments for transparency
- ✅ Handle failures gracefully
- ✅ Update issue status
- ✅ Link PRs to issues
- ✅ Test workflows manually first

### Agent Behavior

- ✅ Read issue/PR context thoroughly
- ✅ Ask clarifying questions if unclear
- ✅ Update progress regularly
- ✅ Test changes before submitting
- ✅ Write clear PR descriptions

### Security

- ✅ Use minimal PAT permissions
- ✅ Rotate credentials regularly
- ✅ Don't log secrets
- ✅ Validate user inputs
- ✅ Limit agent capabilities with `allowed-tools`

### Plugin Integration

- ✅ Use plugins for complex workflows
- ✅ Create custom plugins for domain logic
- ✅ Document plugin requirements
- ✅ Test plugins before deployment

## Troubleshooting

### Agent Not Responding

**Check**:
1. Workflow triggered? (Actions tab)
2. Agent username matches?
3. Secrets configured?
4. Workflow logs for errors?

### Authentication Errors

**Solutions**:
- Refresh `CLAUDE_CREDENTIALS_JSON`
- Verify PAT hasn't expired
- Check PAT has required scopes

### Agent Makes Wrong Changes

**Solutions**:
- Improve prompts with more context
- Add validation hooks
- Use more specific plugins
- Include CLAUDE.md with guidelines

## Additional Resources

- [Plugin Development Guide](./plugin-development.md)
- [Testing Guide](./testing-guide.md)
- GitHub Actions Documentation
- Claude Code Documentation
