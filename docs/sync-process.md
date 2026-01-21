# Sync Process Documentation

## Overview

The claude-unleashed repository maintains a fork of Claude Code with enhancements for GitHub Actions automation. This document describes the process for synchronizing with upstream Claude Code updates while preserving our custom functionality.

## Table of Contents

1. [Architecture](#architecture)
2. [Daily Sync Workflow](#daily-sync-workflow)
3. [Conflict Handling](#conflict-handling)
4. [AI Agent Conflict Resolution](#ai-agent-conflict-resolution)
5. [Manual Resolution Steps](#manual-resolution-steps)
6. [Rollback Procedures](#rollback-procedures)
7. [Monitoring Sync Health](#monitoring-sync-health)

## Architecture

### Repository Structure

```
claude-unleashed/
├── .github/
│   └── workflows/
│       ├── mention-trigger.yml      # Snail: Mention-based triggers
│       ├── assignment-trigger.yml   # Snail: Assignment-based triggers
│       └── setup-check.yml          # Snail: Credential verification
├── .unleashed/
│   ├── patches/                     # Core patch documentation
│   └── PATCHES.md                   # Patch changelog
├── claude-code/                     # Git submodule → upstream
│   ├── .claude/                     # Claude Code core
│   ├── plugins/                     # Our custom plugins
│   └── [upstream files]
├── docs/
│   └── extensions/                  # This documentation
├── README.md                        # Snail-core template docs
└── CLAUDE.md                        # Agent instructions
```

### Git Structure

```
┌─────────────────────────────────────────────────────────────┐
│                   claude-unleashed (Main Repo)              │
│                                                             │
│  Contains:                                                  │
│  - GitHub Actions workflows (snail integration)             │
│  - Documentation                                            │
│  - Git submodule pointer to claude-code                     │
│                                                             │
│      ┌───────────────────────────────────────────────────┐  │
│      │         claude-code/ (Git Submodule)              │  │
│      │                                                   │  │
│      │  Upstream: github.com/anthropic/claude-code      │  │
│      │  Fork: Our customized version                    │  │
│      │                                                   │  │
│      │  Contains:                                        │  │
│      │  - Core Claude Code files                        │  │
│      │  - Our plugins in plugins/                       │  │
│      │  - Potential core patches (minimize!)            │  │
│      │                                                   │  │
│      │  Sync Strategy:                                   │  │
│      │  1. Fetch upstream changes                       │  │
│      │  2. Merge into our fork                          │  │
│      │  3. Resolve conflicts (auto or manual)           │  │
│      │  4. Update submodule pointer                     │  │
│      └───────────────────────────────────────────────────┘  │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

### Sync Architecture Diagram

```
┌─────────────────────────────────────────────────────────────┐
│                    Sync Process Flow                        │
└─────────────────────────────────────────────────────────────┘

  ┌─────────────────┐
  │  Anthropic      │
  │  Claude Code    │
  │  (upstream)     │
  └────────┬────────┘
           │
           │ Daily check for updates
           │
           ▼
  ┌─────────────────┐
  │  Sync Workflow  │
  │  (GitHub Action)│
  └────────┬────────┘
           │
           ├── No changes → Exit
           │
           ├── Changes found
           │   │
           │   ├── Auto-merge attempt
           │   │   │
           │   │   ├── Success → Update submodule → Done
           │   │   │
           │   │   └── Conflicts
           │               │
           │               ▼
           │       ┌────────────────┐
           │       │  AI Agent      │
           │       │  (Claude Code) │
           │       └───────┬────────┘
           │               │
           │               ├── Analyze conflicts
           │               ├── Check patch docs
           │               ├── Attempt resolution
           │               │
           │               ├── Success → Create PR
           │               │
           │               └── Failure
           │                       │
           │                       ▼
           │               ┌────────────────┐
           │               │  Create Issue  │
           │               │  Tag: manual   │
           │               └────────────────┘
           │                       │
           │                       ▼
           │               Human reviews and resolves
           │
           └──────────────────────────────────────────┐
                                                      │
                                                      ▼
                                               ┌────────────┐
                                               │  Complete  │
                                               └────────────┘
```

## Daily Sync Workflow

### Automated Sync Process

A GitHub Action runs daily to check for and merge upstream updates.

**Workflow File**: `.github/workflows/sync-upstream.yml`

```yaml
name: Sync with Upstream

on:
  schedule:
    - cron: '0 2 * * *'  # 2 AM UTC daily
  workflow_dispatch:     # Manual trigger

permissions:
  contents: write
  pull-requests: write
  issues: write

jobs:
  sync:
    runs-on: ubuntu-latest
    steps:
      - name: Checkout repository
        uses: actions/checkout@v4
        with:
          submodules: true
          token: ${{ secrets.HAI_GH_PAT }}

      - name: Configure git
        run: |
          git config user.name "Claude Sync Bot"
          git config user.email "sync-bot@example.com"

      - name: Fetch upstream changes
        working-directory: claude-code
        run: |
          git remote add upstream https://github.com/anthropic/claude-code.git || true
          git fetch upstream main

      - name: Check for updates
        id: check-updates
        working-directory: claude-code
        run: |
          BEHIND=$(git rev-list --count HEAD..upstream/main)
          echo "commits_behind=$BEHIND" >> $GITHUB_OUTPUT

          if [ "$BEHIND" -eq 0 ]; then
            echo "status=up-to-date" >> $GITHUB_OUTPUT
          else
            echo "status=updates-available" >> $GITHUB_OUTPUT
          fi

      - name: Attempt auto-merge
        if: steps.check-updates.outputs.status == 'updates-available'
        id: auto-merge
        working-directory: claude-code
        run: |
          set +e  # Don't exit on error

          git merge upstream/main --no-edit

          if [ $? -eq 0 ]; then
            echo "result=success" >> $GITHUB_OUTPUT
            git push origin main
          else
            echo "result=conflicts" >> $GITHUB_OUTPUT
            git merge --abort
          fi

      - name: Spawn AI agent for conflict resolution
        if: steps.auto-merge.outputs.result == 'conflicts'
        id: ai-resolve
        uses: ./.github/workflows/spawn-agent.yml
        with:
          prompt: |
            Upstream Claude Code has updates that conflict with our changes.

            **Task**: Resolve merge conflicts between upstream and our fork.

            **Context**:
            - Repository: claude-code (submodule)
            - Upstream: anthropic/claude-code
            - Commits behind: ${{ steps.check-updates.outputs.commits_behind }}

            **Instructions**:
            1. Review conflict files
            2. Check .unleashed/patches/ for documentation
            3. Resolve conflicts preserving both upstream improvements and our changes
            4. Test that plugins still work
            5. Create PR with resolution

            **Priority**: Keep our plugin-based extensions intact while accepting upstream improvements.

            See docs/sync-process.md for details.
          agent_name: 'sync-resolver'
          agent_type: 'worker'
        secrets: inherit

      - name: Create manual resolution issue
        if: steps.ai-resolve.result == 'failure'
        run: |
          gh issue create \
            --title "🔄 Manual Sync Required: Upstream Merge Conflicts" \
            --label "sync,manual-review" \
            --body "$(cat <<EOF
          # Upstream Sync Conflicts

          Automatic sync with upstream Claude Code failed due to conflicts that the AI agent couldn't resolve.

          ## Details

          - **Commits behind**: ${{ steps.check-updates.outputs.commits_behind }}
          - **Auto-merge**: Failed
          - **AI resolution**: Failed
          - **Action run**: [View logs](https://github.com/${{ github.repository }}/actions/runs/${{ github.run_id }})

          ## Manual Resolution Required

          Follow the manual resolution process in [docs/sync-process.md](../docs/sync-process.md#manual-resolution-steps).

          ### Quick Start

          \`\`\`bash
          cd claude-code
          git fetch upstream main
          git merge upstream/main

          # Resolve conflicts
          # Check .unleashed/patches/ for patch documentation

          git add .
          git commit -m "sync: merge upstream changes"
          git push

          # Update submodule pointer in parent repo
          cd ..
          git add claude-code
          git commit -m "chore: update claude-code submodule"
          git push
          \`\`\`

          ## Checklist

          - [ ] Review conflicting files
          - [ ] Check patch documentation
          - [ ] Resolve conflicts
          - [ ] Test plugins
          - [ ] Verify workflows still work
          - [ ] Update submodule
          - [ ] Close this issue
          EOF
          )"
```

### What Happens on Sync

#### Case 1: No Updates

```
1. Fetch upstream
2. Check for changes
3. No changes found
4. Exit successfully
5. No action needed
```

#### Case 2: Clean Merge

```
1. Fetch upstream
2. Changes found (e.g., 5 commits behind)
3. Attempt merge
4. No conflicts
5. Push merged changes
6. Update submodule pointer
7. Success!
```

#### Case 3: Conflicts (Auto-Resolvable)

```
1. Fetch upstream
2. Changes found
3. Attempt merge
4. Conflicts detected
5. Spawn AI agent
6. Agent analyzes conflicts
7. Agent resolves (preserves our changes + upstream improvements)
8. Agent creates PR for review
9. Human approves
10. Merge PR
11. Success!
```

#### Case 4: Conflicts (Manual Required)

```
1. Fetch upstream
2. Changes found
3. Attempt merge
4. Conflicts detected
5. Spawn AI agent
6. Agent attempts resolution
7. Agent cannot resolve (too complex/ambiguous)
8. Create issue for manual resolution
9. Human resolves following documented process
10. Close issue
11. Success!
```

## Conflict Handling

### Types of Conflicts

#### 1. Plugin-Only Changes (No Conflict)

**Scenario**: We added plugins, upstream changed core

**Result**: Clean merge
**Reason**: Plugins are isolated in separate directory

**Example**:
```
Our changes:
+ plugins/unleashed/my-plugin/

Upstream changes:
  claude-code/src/core/engine.ts
  claude-code/src/tools/bash.ts

Merge: ✅ Automatic success
```

#### 2. Documentation Conflicts (Easy)

**Scenario**: Both we and upstream modified README

**Resolution**: Keep both changes, merge sections

**Example**:
```
<<<<<<< HEAD (ours)
## Snail Integration

This fork includes GitHub Actions integration.
=======
## New Features in v2.0

Claude Code now supports MCP servers.
>>>>>>> upstream/main

Resolved:
## New Features in v2.0

Claude Code now supports MCP servers.

## Snail Integration

This fork includes GitHub Actions integration.
```

#### 3. Core Patch Conflicts (Complex)

**Scenario**: We patched a file that upstream also modified

**Resolution**: Carefully merge, preserving both changes

**Example**:
```
File: src/core/hooks.ts

<<<<<<< HEAD (ours)
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  PostToolUse = 'PostToolUse',
  // CLAUDE-UNLEASHED PATCH: Added for GitHub workflows
  PreAssignment = 'PreAssignment',
}
=======
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  PostToolUse = 'PostToolUse',
  // Upstream added
  PreCompact = 'PreCompact',
}
>>>>>>> upstream/main

Resolved:
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  PostToolUse = 'PostToolUse',
  // Upstream added
  PreCompact = 'PreCompact',
  // CLAUDE-UNLEASHED PATCH: Added for GitHub workflows
  PreAssignment = 'PreAssignment',
}
```

#### 4. Architectural Conflicts (Very Complex)

**Scenario**: Upstream refactored code we patched

**Resolution**: May require rewriting our patch

**Example**:
```
Our patch: Added feature to src/old-architecture.ts
Upstream: Deleted src/old-architecture.ts, refactored to src/new-architecture.ts

Resolution:
1. Understand new architecture
2. Re-implement our feature in new structure
3. Test thoroughly
4. May need to convert patch to plugin if possible
```

### Conflict Resolution Priority

1. **Preserve Functionality**: Both ours and upstream features should work
2. **Prefer Upstream**: When in doubt, accept upstream's approach
3. **Plugin-ify Patches**: If conflict is complex, try converting our patch to plugin
4. **Document Decisions**: Record why conflicts were resolved a certain way

## AI Agent Conflict Resolution

### Agent Capabilities

The AI agent (Claude Code itself) attempts automated conflict resolution:

**Agent System Prompt** (in spawn-agent workflow):

```markdown
# Upstream Sync Conflict Resolution

You are resolving merge conflicts between our claude-code fork and upstream.

## Your Task

1. **Understand Changes**:
   - Review conflicting files
   - Identify our changes vs upstream changes
   - Check .unleashed/patches/ for documentation of our patches

2. **Resolve Conflicts**:
   - Preserve our custom functionality (plugins, workflows)
   - Accept upstream improvements and bug fixes
   - Merge both sets of changes when possible
   - Use upstream's approach for architectural changes

3. **Validate Resolution**:
   - Ensure plugins still load
   - Check workflows are intact
   - Verify no syntax errors
   - Test critical paths

4. **Document Resolution**:
   - Update .unleashed/PATCHES.md if patches changed
   - Note any manual testing needed
   - Document any risks

## Resolution Strategy

### High Priority (Keep Ours)
- Plugins in plugins/unleashed/
- GitHub workflow files in .github/workflows/
- Documentation in docs/

### Medium Priority (Merge Both)
- Core patches (documented in .unleashed/patches/)
- Configuration files
- Build scripts

### Low Priority (Accept Upstream)
- Dependency updates
- Performance improvements
- Bug fixes
- Refactoring

## Output

Create a PR with:
- Title: "sync: merge upstream claude-code [date]"
- Description: Detailed explanation of conflicts and resolutions
- Testing notes: What needs manual verification
- Risk assessment: Low/Medium/High

## Failure Conditions

If you cannot resolve automatically:
- Complex architectural conflicts
- Ambiguous intent
- Risk of breaking functionality
- Insufficient context to decide

In these cases, create detailed issue for human review.
```

### Agent Process

```
1. Agent receives conflict notification
2. Reads conflicting files
3. Checks .unleashed/patches/ for our patch docs
4. Attempts resolution using strategy above
5. Creates branch: sync/auto-merge-YYYY-MM-DD
6. Commits resolution
7. Runs basic validation
8. If successful:
   - Creates PR
   - Adds detailed description
   - Requests review
9. If failed:
   - Documents attempt
   - Creates issue
   - Includes conflict details
```

### Agent Success Criteria

Agent considers resolution successful if:
- ✅ All conflicts resolved
- ✅ Code compiles/lints
- ✅ Plugins still load
- ✅ No obvious regressions
- ✅ Both sets of changes preserved

Agent escalates to human if:
- ❌ Cannot understand intent of changes
- ❌ Architectural changes too complex
- ❌ Risk of breaking functionality
- ❌ Conflicts in multiple patches
- ❌ Test failures after resolution

## Manual Resolution Steps

When AI agent cannot resolve conflicts, follow this process:

### 1. Preparation

```bash
# Clone repository
git clone https://github.com/your-org/claude-unleashed.git
cd claude-unleashed

# Initialize submodule
git submodule update --init --recursive
cd claude-code

# Add upstream remote
git remote add upstream https://github.com/anthropic/claude-code.git

# Fetch latest
git fetch upstream main
```

### 2. Identify Conflicts

```bash
# Start merge
git merge upstream/main

# View conflicting files
git status

# Example output:
# Unmerged paths:
#   both modified:   src/core/hooks.ts
#   both modified:   src/tools/bash.ts
#   both modified:   README.md
```

### 3. Review Patch Documentation

```bash
# Check which files are documented patches
ls -la ../.unleashed/patches/

# Read relevant patch docs
cat ../.unleashed/patches/hook-event-extension.md
cat ../.unleashed/patches/PATCHES.md

# Understand why we patched each file
# This context is critical for resolution
```

### 4. Resolve Each Conflict

**For each conflicting file**:

```bash
# Open in editor
code src/core/hooks.ts

# Review conflict markers
<<<<<<< HEAD (our version)
[Our changes]
=======
[Upstream changes]
>>>>>>> upstream/main

# Decision matrix:
# 1. Can both changes coexist? → Merge both
# 2. Is upstream's approach better? → Accept upstream, adapt our patch
# 3. Is our patch critical? → Keep ours, integrate upstream changes around it
# 4. Can our patch become a plugin? → Accept upstream, convert patch to plugin
```

**Resolution Example**:

```typescript
// File: src/core/hooks.ts

// Conflict:
<<<<<<< HEAD
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  // CLAUDE-UNLEASHED: Added for workflow integration
  PreAssignment = 'PreAssignment',
}
=======
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  PreCompact = 'PreCompact',  // Upstream added
}
>>>>>>> upstream/main

// Resolution (merge both):
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  PreCompact = 'PreCompact',  // Upstream v2.0
  // CLAUDE-UNLEASHED: Added for workflow integration
  PreAssignment = 'PreAssignment',
}
```

### 5. Mark as Resolved

```bash
# After resolving conflict in file
git add src/core/hooks.ts

# Check remaining conflicts
git status

# Repeat for each file
```

### 6. Update Patch Documentation

```bash
# If patch was affected, update documentation
cd ../.unleashed/patches

# Update conflict history
cat >> hook-event-extension.md <<EOF

## Sync History

### 2026-01-01 - Upstream v2.0 Merge
- Upstream added PreCompact event
- Merged both changes successfully
- No changes needed to our PreAssignment implementation
- Risk: Low
EOF

# Update PATCHES.md
cat >> PATCHES.md <<EOF

## [2026-01-01] - Sync: Upstream v2.0
- Merged upstream changes
- Conflicts in: src/core/hooks.ts
- Resolution: Merged both event additions
- Patches affected: hook-event-extension
- Status: Resolved
EOF
```

### 7. Complete Merge

```bash
cd ../claude-code

# Commit merge
git commit -m "sync: merge upstream claude-code v2.0

Merged upstream changes from anthropic/claude-code.

Conflicts resolved in:
- src/core/hooks.ts: Merged PreCompact (upstream) and PreAssignment (ours)
- README.md: Merged documentation updates

Patches affected:
- hook-event-extension: Updated to include both events

Testing needed:
- Verify PreAssignment hook still works in workflows
- Test PreCompact hook integration

Risk assessment: Low - isolated changes, well-documented"

# Push to branch
git checkout -b sync/manual-merge-2026-01-01
git push origin sync/manual-merge-2026-01-01
```

### 8. Test Resolution

```bash
# Test plugins load
cc --plugin-dir plugins/my-plugin

# Test workflows (if applicable)
# Create test issue with agent mention

# Run any existing tests
npm test  # or appropriate test command
```

### 9. Create PR

```bash
# Create PR for review
gh pr create \
  --title "sync: merge upstream claude-code v2.0" \
  --body "$(cat <<EOF
# Upstream Sync: Claude Code v2.0

## Summary

Merged upstream changes from anthropic/claude-code v2.0 into our fork.

## Conflicts Resolved

### src/core/hooks.ts
- **Conflict**: Both added new HookEvent enum values
- **Resolution**: Merged both (PreCompact from upstream, PreAssignment from our patch)
- **Testing**: Verified both events work correctly

### README.md
- **Conflict**: Both updated documentation
- **Resolution**: Merged sections, preserved our snail integration docs
- **Testing**: Reviewed for clarity

## Patches Affected

- **hook-event-extension**: Updated to include both PreCompact and PreAssignment
- See .unleashed/patches/hook-event-extension.md for details

## Testing Completed

- [x] Plugins load successfully
- [x] Claude Code starts without errors
- [x] PreAssignment hook tested in workflow
- [x] No regressions in existing functionality

## Risk Assessment

**Overall Risk: Low**

- Changes are isolated and well-documented
- Both sets of functionality preserved
- No architectural conflicts
- Tests pass

## Manual Testing Needed

- [ ] Verify GitHub Actions workflows with PreAssignment hook
- [ ] Test PreCompact hook in production
- [ ] Validate all plugins still work

## Reviewer Checklist

- [ ] Review conflict resolutions
- [ ] Verify patch documentation updated
- [ ] Check no functionality lost
- [ ] Approve if resolution is clean
EOF
)" \
  --reviewer "@team-lead"
```

### 10. Update Submodule Pointer

After PR is merged:

```bash
# In parent repository
cd /path/to/claude-unleashed

# Update submodule pointer
git add claude-code
git commit -m "chore: update claude-code submodule to v2.0 sync"
git push
```

## Rollback Procedures

### When to Rollback

Rollback if sync introduces:
- Breaking changes to workflows
- Plugin incompatibilities
- Critical bugs
- Performance degradation

### Rollback Process

#### 1. Quick Rollback (Emergency)

```bash
# In claude-code submodule
cd claude-code

# Find last working commit
git log --oneline -10

# Reset to previous commit
git reset --hard COMMIT_HASH

# Force push (requires force-with-lease permissions)
git push --force-with-lease origin main

# Update parent repo
cd ..
git add claude-code
git commit -m "revert: rollback claude-code sync due to [reason]"
git push
```

#### 2. Proper Rollback (Preferred)

```bash
# Revert the merge commit
cd claude-code
git revert -m 1 HEAD

# -m 1 means keep our side of the merge

git push origin main

# Update parent
cd ..
git add claude-code
git commit -m "revert: rollback claude-code sync

[Detailed reason for rollback]
[Issues encountered]
[Plan for re-sync]
"
git push
```

#### 3. Document Rollback

```bash
# Update patch documentation
cat >> .unleashed/PATCHES.md <<EOF

## [2026-01-01] - ROLLBACK: Upstream v2.0 Sync
- Sync introduced breaking changes
- Issues: [List issues]
- Rolled back to previous version
- Plan: [Describe plan to address issues and re-sync]
EOF

# Create issue to track resolution
gh issue create \
  --title "Upstream sync rollback: v2.0 breaking changes" \
  --label "sync,bug" \
  --body "Rolled back sync due to: [reasons]

Need to:
1. Investigate root cause
2. Fix compatibility issues
3. Re-attempt sync
"
```

## Monitoring Sync Health

### Sync Dashboard

Create GitHub issue template for sync status:

```markdown
# Sync Health Report

**Date**: 2026-01-01
**Status**: 🟢 Healthy / 🟡 Warning / 🔴 Critical

## Metrics

- **Days since last sync**: 2
- **Commits behind upstream**: 5
- **Known conflicts**: 0
- **Patches active**: 2
- **Last sync result**: Success

## Recent Syncs

| Date | Result | Conflicts | Resolution |
|------|--------|-----------|------------|
| 2026-01-01 | ✅ Success | 0 | Automatic |
| 2025-12-28 | ✅ Success | 2 | AI Agent |
| 2025-12-25 | ⚠️ Manual | 5 | Human |

## Active Patches

1. **hook-event-extension**
   - Risk: Medium
   - Last conflict: Never
   - Status: Stable

2. **workflow-integration**
   - Risk: Low
   - Last conflict: 2025-12-25
   - Status: Stable

## Recommendations

- [ ] Monitor upcoming upstream v2.1 release
- [ ] Consider converting workflow-integration to plugin
- [ ] Review patch documentation quarterly

## Next Sync

**Scheduled**: 2026-01-02 2:00 AM UTC
**Expected complexity**: Low
```

### Automated Health Checks

```yaml
# .github/workflows/sync-health-check.yml
name: Sync Health Check

on:
  schedule:
    - cron: '0 0 * * 0'  # Weekly on Sunday

jobs:
  health-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: true

      - name: Check sync status
        run: |
          cd claude-code
          git fetch upstream main

          BEHIND=$(git rev-list --count HEAD..upstream/main)
          LAST_SYNC=$(git log --grep="sync:" -1 --format="%ar")

          echo "Commits behind: $BEHIND"
          echo "Last sync: $LAST_SYNC"

          # Alert if too far behind
          if [ "$BEHIND" -gt 20 ]; then
            echo "⚠️ WARNING: $BEHIND commits behind upstream"
            exit 1
          fi

      - name: Validate patches
        run: |
          # Check patch files exist
          for patch in .unleashed/patches/*.md; do
            echo "Checking $patch"
            # Verify files mentioned in patch still exist
          done

      - name: Create health report
        if: failure()
        run: |
          gh issue create \
            --title "Sync Health Alert" \
            --label "sync,warning" \
            --body "Sync health check failed. Review needed."
```

### Key Metrics to Track

1. **Sync Frequency**: How often syncs occur
2. **Conflict Rate**: Percentage of syncs with conflicts
3. **Auto-Resolution Rate**: Percentage resolved by AI
4. **Time Behind**: How many commits behind upstream
5. **Patch Stability**: How often patches conflict

### Success Criteria

Healthy sync process shows:
- ✅ Syncs occur at least weekly
- ✅ Auto-resolution rate >50%
- ✅ Never more than 30 commits behind
- ✅ No patches conflict repeatedly
- ✅ Rollbacks are rare (<5% of syncs)

## Best Practices

### Before Sync

- ✅ Review upcoming upstream changes
- ✅ Check patch documentation is current
- ✅ Ensure CI/CD is passing
- ✅ Notify team of planned sync

### During Sync

- ✅ Monitor workflow execution
- ✅ Review AI resolution PRs promptly
- ✅ Test changes before merging
- ✅ Document any manual interventions

### After Sync

- ✅ Update patch documentation
- ✅ Verify workflows still work
- ✅ Test plugins in production
- ✅ Update sync health metrics

### Long-Term Maintenance

- ✅ Quarterly review of all patches
- ✅ Identify patches that can become plugins
- ✅ Contribute improvements to upstream
- ✅ Keep documentation current

## Troubleshooting

### Sync Workflow Not Running

**Check**:
```bash
# Verify workflow file
cat .github/workflows/sync-upstream.yml

# Check workflow is enabled
gh workflow list

# Manually trigger
gh workflow run sync-upstream.yml
```

### AI Agent Can't Resolve

**Solutions**:
- Improve patch documentation
- Simplify patches (split complex patches)
- Convert patches to plugins
- Provide more context in .unleashed/patches/

### Frequent Conflicts in Same File

**Solutions**:
- Consider converting patch to plugin
- Contribute feature to upstream
- Refactor to reduce surface area

### Submodule Pointer Out of Sync

**Fix**:
```bash
# Update submodule
cd claude-code
git checkout main
git pull origin main

cd ..
git add claude-code
git commit -m "chore: sync submodule pointer"
git push
```

## Summary

### Sync Process TL;DR

1. **Daily check** for upstream updates
2. **Auto-merge** if no conflicts
3. **AI agent** resolves simple conflicts
4. **Human resolves** complex conflicts
5. **Test** before merging
6. **Document** all changes
7. **Monitor** sync health

### Key Principles

- **Plugin-First**: Minimize patches to reduce conflicts
- **Document Everything**: Patch docs enable automated resolution
- **Test Thoroughly**: Validate before and after sync
- **Fail Safely**: Rollback quickly if issues arise
- **Continuous Improvement**: Convert patches to plugins over time

## Additional Resources

- [Core Patches Guide](./extensions/core-patches.md)
- [Plugin Development Guide](./extensions/plugin-development.md)
- [Testing Guide](./extensions/testing-guide.md)
- [Snail Integration Guide](./extensions/snail-integration.md)
