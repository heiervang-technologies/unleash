# Core Patches Guide

## Overview

This document outlines the policy and procedures for making patches to the core Claude Code codebase. The agent-unleashed repository follows a **plugin-first** philosophy, where core patches should be avoided whenever possible.

## Table of Contents

1. [Policy: Plugin-First Approach](#policy-plugin-first-approach)
2. [Auto Mode Patch System](#auto-mode-patch-system)
3. [When to Use Core Patches](#when-to-use-core-patches)
4. [When NOT to Use Core Patches](#when-not-to-use-core-patches)
5. [Documentation Requirements](#documentation-requirements)
6. [Branch Strategy](#branch-strategy)
7. [Conflict Risk Assessment](#conflict-risk-assessment)
8. [Migration Path: Patches to Plugins](#migration-path-patches-to-plugins)

## Policy: Plugin-First Approach

### Core Principle

**Almost all functionality should be implemented as plugins, not core patches.**

The agent-unleashed repository maintains a fork of Claude Code to enable GitHub Actions automation and snail-core integration. To minimize merge conflicts and maintenance burden during upstream syncs, we follow these principles:

1. **Default to Plugins**: Always attempt plugin-based implementation first
2. **Minimize Core Changes**: Keep core codebase as close to upstream as possible
3. **Document All Patches**: Any core changes must be thoroughly documented
4. **Plan for Migration**: All patches should have a path to become plugins eventually

### Why Plugin-First?

**Benefits**:
- **Easy Syncing**: Fewer merge conflicts when pulling upstream changes
- **Maintainability**: Isolated changes are easier to understand and update
- **Flexibility**: Enable/disable features without rebuilding
- **Portability**: Plugins can be shared across forks
- **Testing**: Easier to test in isolation
- **Contribution**: Plugins can be contributed upstream

**Costs of Core Patches**:
- **Merge Conflicts**: Every upstream sync may conflict
- **Review Burden**: Harder to review mixed core + plugin changes
- **Maintenance Debt**: Must manually merge/resolve conflicts
- **Lock-in**: Harder to share with community
- **Testing Complexity**: Harder to isolate issues

## Auto Mode Patch System

Agent Unleashed includes a version-aware patching system for adding "Auto Mode" to Claude Code. This is an exception to the plugin-first approach because it requires modifying the minified CLI JavaScript.

### Directory Structure

```
scripts/
├── patch-claude.sh              # Main patch dispatcher
├── unpatch-claude.sh            # Restore from backup
├── check-and-patch.sh           # Auto-patch on version change
└── patches/
    └── versions/
        ├── 2.1.0.conf           # Variable mappings for 2.1.0
        ├── 2.1.2.conf           # Variable mappings for 2.1.2
        └── 2.1.3.conf           # Variable mappings for 2.1.3
```

### How It Works

1. **Version Detection**: The patch script detects the installed Claude Code version
2. **Config Selection**: Finds the appropriate `.conf` file for that version
3. **Fallback Logic**: If no exact match, uses the latest version ≤ target
4. **Variable Substitution**: Applies patches using version-specific variable names

### Version Config Format

Each `.conf` file defines the minified variable names for that version:

```bash
# scripts/patches/versions/2.1.3.conf

# Modes array variable name
MODES_ARRAY_VAR="QP"

# Mode variable in setMode handler
MODE_VAR="S0"

# Telemetry function name
TELEMETRY_FN="y9"

# Delegate check functions
DELEGATE_FN1="\$k0"
DELEGATE_FN2="PmA"

# Permission context variable
PERMISSION_CTX_VAR="PA"
```

### Adding Support for New Versions

When a new Claude Code version is released:

1. **Check if patch works**: Run `patch-claude.sh` - it will fall back to latest config
2. **If patches fail**: Create a new config file for the version
3. **Find variable names**: Search the minified `cli.js` for patterns:

```bash
# Find modes array variable
tr ';' '\n' < cli.js | grep -oP '\w+=\["acceptEdits","bypassPermissions"[^\]]*\]'

# Find setMode handler variable
tr ';' '\n' < cli.js | grep 'auto-accept-mode'

# Find delegate check pattern
tr ';' '\n' < cli.js | grep 'B\.mode==="delegate"'
```

4. **Create config**: Add `scripts/patches/versions/X.Y.Z.conf`
5. **Test**: Run patch script and verify with `claude -p "ping"`

### Version Fallback Behavior

| Target Version | Config Used | Reason |
|---------------|-------------|--------|
| 2.1.3 | 2.1.3.conf | Exact match |
| 2.1.4 | 2.1.3.conf | Latest ≤ target |
| 2.1.5 | 2.1.3.conf | Latest ≤ target |
| 2.1.1 | 2.1.0.conf | Closest lower version |

### Patches Applied

The auto mode patch adds:

1. **Modes Array**: Adds "auto" to the modes list
2. **Display Name**: "Auto Mode" label
3. **Icon**: »» indicator
4. **Cycling**: bypassPermissions → auto → default
5. **Permission Bypass**: Auto mode behaves like bypassPermissions
6. **Color**: Yellow/warning indicator
7. **Flag Files**: Creates/removes `~/.cache/agent-unleashed/auto-mode/active-{pid}`

### Testing After Patching

```bash
# Verify headless mode works
claude -p "ping"

# Verify patches applied
grep -o 'case"auto":return"Auto Mode"' /path/to/cli.js
```

## When to Use Core Patches

Core patches are appropriate **only** in these rare situations:

### 1. Critical Bug Fixes

**Scenario**: Upstream bug that blocks core functionality.

**Example**: Claude Code crashes on startup in specific environments.

**Process**:
1. File issue with upstream (Anthropic)
2. Document bug thoroughly
3. Create minimal fix
4. Submit PR to upstream
5. Apply patch locally
6. Remove patch when upstream merges fix

**Justification**: System is unusable without fix.

### 2. Performance Critical Improvements

**Scenario**: Performance issue in hot path that cannot be addressed via plugins.

**Example**: Tool execution framework has O(n²) complexity causing slowdowns.

**Process**:
1. Profile and document performance issue
2. Verify plugin approach is insufficient
3. Implement minimal optimization
4. Submit PR to upstream
5. Document performance gains

**Justification**: User experience severely degraded.

### 3. Security Vulnerabilities

**Scenario**: Security issue in core code requiring immediate fix.

**Example**: Path traversal vulnerability in file access.

**Process**:
1. **Do not** publicly disclose until fixed
2. Report to Anthropic security team
3. Apply fix locally
4. Coordinate disclosure with upstream
5. Remove patch when upstream releases fix

**Justification**: Security cannot be compromised.

### 4. Essential Integration Points

**Scenario**: Feature absolutely requires core changes and cannot be implemented via plugins.

**Example**: New hook event type needed for GitHub Actions integration.

**Process**:
1. Thoroughly document why plugin approach fails
2. Design minimal API change
3. Propose to upstream first
4. Implement with extensive documentation
5. Create abstraction layer for future plugin use

**Justification**: Core architecture prevents plugin-based solution.

### Decision Tree

```
Is this feature needed?
└─ Yes
   └─ Can it be a plugin?
      ├─ Yes → CREATE PLUGIN (99% of cases)
      └─ No → Why not?
         ├─ Missing hook event
         │  └─ Can we add hook event and then use plugin?
         │     ├─ Yes → Add hook + CREATE PLUGIN
         │     └─ No → CORE PATCH (rare)
         ├─ Performance critical hot path
         │  └─ Can we optimize via caching/lazy loading in plugin?
         │     ├─ Yes → CREATE PLUGIN with optimization
         │     └─ No → CORE PATCH (very rare)
         └─ Security vulnerability
            └─ CORE PATCH + upstream report (immediate)
```

## When NOT to Use Core Patches

### Examples of Inappropriate Core Patches

#### 1. UI Customization

**Bad**: Modify core UI components
```javascript
// DON'T: Patch core UI
// src/ui/components/Header.tsx
export function Header() {
  return <div>My Custom Header</div>; // ❌
}
```

**Good**: Use SessionStart hook to inject context
```bash
# DO: Plugin with SessionStart hook
# plugins/custom-ui/hooks/scripts/load-ui-context.sh
cat <<EOF
Your custom header message appears here via system message.
This is how the UI should behave...
EOF
```

#### 2. Custom Commands

**Bad**: Add command handling to core
```javascript
// DON'T: Patch core command handler
// src/commands/handler.ts
if (command === 'my-custom-command') { // ❌
  await handleMyCommand();
}
```

**Good**: Create plugin command
```markdown
<!-- DO: Plugin command -->
<!-- plugins/my-commands/commands/my-custom-command.md -->
---
name: my-custom-command
description: Does something useful
---

Implementation here...
```

#### 3. Additional Validation

**Bad**: Add validation to core tool execution
```javascript
// DON'T: Patch core tool validation
// src/tools/executor.ts
if (tool === 'Write' && path.includes('secret')) { // ❌
  throw new Error('Cannot write secrets');
}
```

**Good**: Use PreToolUse hook
```json
{
  "PreToolUse": [{
    "matcher": "Write",
    "hooks": [{
      "type": "prompt",
      "prompt": "Check if this write involves secrets. Return 'deny' if so."
    }]
  }]
}
```

#### 4. External Integrations

**Bad**: Add API client to core
```javascript
// DON'T: Patch core with API clients
// src/integrations/slack.ts
export class SlackClient { // ❌
  // ...
}
```

**Good**: Use MCP server
```json
{
  "mcpServers": {
    "slack": {
      "command": "python",
      "args": ["-m", "slack_mcp_server"]
    }
  }
}
```

#### 5. Custom Workflow

**Bad**: Modify core agent behavior
```javascript
// DON'T: Patch core agent logic
// src/agent/planner.ts
if (task.type === 'my-workflow') { // ❌
  return myCustomWorkflow(task);
}
```

**Good**: Create agent + command
```markdown
<!-- DO: Plugin agent -->
<!-- plugins/workflows/agents/my-workflow.md -->
---
name: My Workflow Agent
description: Handles my custom workflow
---

Workflow implementation...
```

## Documentation Requirements

Any core patch MUST include comprehensive documentation:

### 1. Patch Documentation File

Create `/.unleashed/patches/PATCH-NAME.md`:

```markdown
# Patch: [Patch Name]

## Summary
Brief description of what this patch does.

## Rationale
Detailed explanation of why this core patch is necessary and why a plugin approach is insufficient.

## Plugin Approach Attempted
Document what plugin-based approach was tried and why it failed.

## Files Modified
- `src/path/to/file1.ts`: Description of changes
- `src/path/to/file2.ts`: Description of changes

## Upstream Status
- [ ] Issue filed: #ISSUE_NUMBER
- [ ] PR submitted: #PR_NUMBER
- [ ] Upstream response: [Accepted/Rejected/Pending]

## Conflict Risk
**Risk Level**: [Low/Medium/High]

**Analysis**:
- Files modified are [stable/frequently updated]
- Changes are [isolated/pervasive]
- Upstream activity in this area: [low/medium/high]

## Merge Strategy
How to handle conflicts when syncing with upstream:

1. Specific merge approach for this patch
2. Tests to verify after merge
3. Fallback plan if conflicts are severe

## Migration Plan
Path to eventually remove this patch:

- **Target**: Convert to plugin when X becomes available
- **Dependencies**: Requires [hook event Y / API Z]
- **Timeline**: Remove when upstream [merges feature / releases version]

## Testing
How to verify this patch works correctly:

```bash
# Test commands
npm test
# Specific scenarios to test
```

## Author
- Name: [Your Name]
- Email: [your@email.com]
- Date: [YYYY-MM-DD]

## Related Issues
- Closes #123
- Related to #456
```

### 2. Inline Code Documentation

Mark all patched code clearly:

```javascript
// CLAUDE-UNLEASHED PATCH START
// Patch: hook-event-extension
// Reason: Add PreAssignment event for GitHub workflow integration
// Upstream: Issue #789, PR pending
// TODO: Remove when upstream merges hook event API
export enum HookEvent {
  PreToolUse = 'PreToolUse',
  PostToolUse = 'PostToolUse',
  // ... existing events ...
  PreAssignment = 'PreAssignment', // Added by agent-unleashed
}
// CLAUDE-UNLEASHED PATCH END
```

### 3. Changelog Entry

Add entry to `/.unleashed/PATCHES.md`:

```markdown
## [Date] - Patch: hook-event-extension

**Type**: Enhancement
**Risk**: Medium
**Files**: 3 files modified
**Upstream**: Issue #789, PR pending

Added PreAssignment hook event to support GitHub assignment workflow.
Plugin-based approach insufficient due to lack of hook event.

Migration plan: Remove when upstream merges hook event API.
```

## Branch Strategy

### Branch Naming

Use specific branch naming for patches:

```bash
# Feature patches
patch/feature-name

# Bug fix patches
patch/fix-bug-name

# Security patches
patch/security-issue-name
```

### Development Workflow

1. **Create Patch Branch**:
```bash
git checkout -b patch/my-core-fix
```

2. **Make Minimal Changes**:
```bash
# Edit only necessary files
# Keep changes focused and minimal
```

3. **Document Thoroughly**:
```bash
# Create patch documentation
mkdir -p .unleashed/patches
cat > .unleashed/patches/my-core-fix.md <<EOF
[Documentation content]
EOF
```

4. **Test Extensively**:
```bash
# Run full test suite
npm test

# Test in realistic scenarios
# Verify no regressions
```

5. **Commit with Clear Message**:
```bash
git add .
git commit -m "patch(core): add hook event for assignment workflow

CORE PATCH - Plugin approach insufficient.

- Add PreAssignment hook event
- Enable GitHub assignment integration
- Upstream issue #789 filed

See .unleashed/patches/hook-event-extension.md for details.
"
```

6. **Create PR with Justification**:
```bash
gh pr create --title "patch(core): hook event extension" --body "$(cat <<'EOF'
## Core Patch Warning

⚠️ This PR modifies core Claude Code files. Review carefully.

## Justification

Plugin-based approach attempted but failed because:
- No hook event exists for assignment workflow
- Cannot intercept assignment events via existing hooks
- Essential for GitHub Actions integration

## Changes

- Add PreAssignment hook event
- Update hook handler to support new event
- Add event to TypeScript types

## Documentation

See `.unleashed/patches/hook-event-extension.md` for complete documentation.

## Upstream

- Issue filed: anthropic/claude-code#789
- PR submitted: pending
- Will remove patch when upstream merges

## Testing

- [ ] All existing tests pass
- [ ] New event triggers correctly
- [ ] No regressions in other hook events
- [ ] GitHub workflow integration works

## Risk Assessment

**Risk Level**: Medium

Modified files are relatively stable but touched in most releases.
Conflicts likely during upstream sync but manageable.
EOF
)"
```

### Review Requirements

Core patches require stricter review:

1. **Two Approvals Required** (vs. one for plugins)
2. **Maintainer Review Required**
3. **Justification Review**:
   - Is plugin approach truly insufficient?
   - Is patch minimal and focused?
   - Is documentation complete?
4. **Conflict Risk Assessment**
5. **Migration Plan Verification**

## Conflict Risk Assessment

### Risk Levels

**Low Risk**:
- Modifies rarely-changed files
- Changes are isolated and localized
- Upstream activity in area is minimal
- Easy to reapply if conflicts occur

**Medium Risk**:
- Modifies occasionally-updated files
- Changes affect multiple related areas
- Some upstream activity expected
- Conflicts possible but resolvable

**High Risk**:
- Modifies frequently-changed core files
- Changes are pervasive or architectural
- High upstream activity in same area
- Conflicts likely and difficult to resolve

### Assessment Criteria

Evaluate each patch:

```markdown
## Conflict Risk Assessment

### Files Modified
List each file with upstream activity estimate:
- `src/core/hooks.ts` - Updated in 60% of releases (HIGH)
- `src/types/events.ts` - Updated in 20% of releases (MEDIUM)
- `src/utils/helper.ts` - Updated in 5% of releases (LOW)

### Change Scope
- **Localized**: Single function/class (LOW)
- **Regional**: Multiple related functions (MEDIUM)
- **Pervasive**: Affects multiple modules (HIGH)

### Upstream Activity
- **Low**: Feature stable, rarely updated
- **Medium**: Ongoing refinements
- **High**: Active development area

### Overall Risk
Based on above factors: [LOW/MEDIUM/HIGH]

### Mitigation Strategy
How to reduce risk:
- Keep patch minimal and well-documented
- Monitor upstream changes in this area
- Plan regular sync schedule
- Prepare for manual merge if needed
```

### Monitoring Upstream

Track upstream activity:

```bash
# Monitor upstream changes to patched files
cd claude-code
git fetch upstream

# Check commits affecting our patches
git log upstream/main --since="1 month ago" -- src/core/hooks.ts

# Review upcoming changes
gh pr list --repo anthropic/claude-code --label "hooks"
```

## Migration Path: Patches to Plugins

All patches should have a clear path to becoming plugins.

### Migration Scenarios

#### Scenario 1: Waiting for Upstream Feature

**Current State**: Core patch adding hook event
**Migration Plan**: Remove patch when upstream adds hook event API

**Steps**:
1. Monitor upstream for hook event API
2. When released, create plugin using new API
3. Test plugin provides same functionality
4. Remove core patch
5. Update documentation

#### Scenario 2: Extract to Plugin

**Current State**: Core patch adding validation logic
**Migration Plan**: Refactor into hook-based plugin

**Steps**:
1. Create plugin with PreToolUse hook
2. Move validation logic to hook script
3. Test plugin provides same functionality
4. Remove core patch
5. Update users to enable plugin

#### Scenario 3: Contribute Upstream

**Current State**: Core patch with broadly useful feature
**Migration Plan**: Get feature merged upstream

**Steps**:
1. Polish implementation
2. Add comprehensive tests
3. Submit high-quality PR to upstream
4. Address review feedback
5. When merged, remove patch
6. Update to upstream version

### Migration Checklist

When migrating patch to plugin:

- [ ] Plugin functionality tested and equivalent
- [ ] Performance is acceptable
- [ ] Documentation updated
- [ ] Users notified of change
- [ ] Core patch removed from codebase
- [ ] Patch documentation archived
- [ ] PATCHES.md updated with migration note

### Example Migration

**Before (Core Patch)**:
```javascript
// src/core/validator.ts
// CLAUDE-UNLEASHED PATCH START
export function validateWrite(path: string): boolean {
  if (path.includes('.env')) {
    throw new Error('Cannot write to .env files');
  }
  return true;
}
// CLAUDE-UNLEASHED PATCH END
```

**After (Plugin)**:
```json
// plugins/security/hooks/hooks.json
{
  "hooks": {
    "PreToolUse": [{
      "matcher": "Write",
      "hooks": [{
        "type": "prompt",
        "prompt": "Check if file path contains .env. Deny if so."
      }]
    }]
  }
}
```

**Migration Steps**:
1. Create security plugin with hook
2. Test hook blocks .env writes
3. Remove core patch
4. Document in PATCHES.md:
   ```markdown
   ## [2026-01-01] - Migrated: env-file-protection

   Migrated from core patch to plugin: security/hooks
   Functionality preserved via PreToolUse hook.
   ```

## Best Practices Summary

### Do's

- ✅ Exhaust all plugin options first
- ✅ Keep patches minimal and focused
- ✅ Document thoroughly
- ✅ File upstream issues/PRs
- ✅ Assess conflict risk
- ✅ Plan migration path
- ✅ Mark code clearly
- ✅ Test extensively
- ✅ Monitor upstream activity

### Don'ts

- ❌ Don't patch for features achievable via plugins
- ❌ Don't make architectural changes
- ❌ Don't skip documentation
- ❌ Don't ignore upstream
- ❌ Don't create high-risk patches
- ❌ Don't commit without migration plan
- ❌ Don't leave patches unmaintained

## Examples from Practice

### Good Patch Example

**Scenario**: Add hook event for GitHub assignment workflow
**Files**: 3 files, localized changes
**Risk**: Medium (hooks API is stable but touched occasionally)
**Upstream**: Issue filed, PR submitted
**Migration**: Remove when upstream merges hook event API
**Justification**: No plugin alternative exists

### Bad Patch Example

**Scenario**: Change default model to opus-4.5
**Files**: 1 file, single line
**Problem**: Can be done via plugin settings
**Alternative**: SessionStart hook to set model preference
**Verdict**: Should be plugin, not patch

## Additional Resources

- [Plugin Development Guide](./plugin-development.md) - Plugin-first approach
- [Snail Integration Guide](./snail-integration.md) - GitHub Actions context
- [Sync Process](../sync-process.md) - Upstream synchronization
- Claude Code Documentation: https://docs.claude.com/claude-code

## Questions?

Before creating a core patch, ask:

1. Can this be a plugin? (99% of the time: yes)
2. What plugin approach have you tried?
3. Why did the plugin approach fail?
4. Is the patch absolutely necessary?
5. What's the migration plan?

If you can't answer these convincingly, create a plugin instead.
