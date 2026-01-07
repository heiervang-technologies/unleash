# Claude Code Live Patches

This document describes all patches applied to the Claude Code `cli.js` bundle.

## Overview

The live-patch plugin modifies the installed Claude Code to add features not available upstream. Patches are applied via `sed` replacements on the bundled `cli.js` file.

**Target file:** `~/.nvm/versions/node/<version>/lib/node_modules/@anthropic-ai/claude-code/cli.js`

## Patch List

### Patch 1: Add "auto" to Modes Array

**Purpose:** Register "auto" as a valid permission mode.

**Pattern:**
```
CT=["acceptEdits","bypassPermissions"
```

**Replacement:**
```
CT=["acceptEdits","auto","bypassPermissions"
```

**Effect:** The mode enum now includes "auto" as a valid value.

---

### Patch 2: Add Display Name for Auto Mode

**Purpose:** Show "Auto Mode" in the UI when auto mode is active.

**Pattern:**
```
case"bypassPermissions":return"Bypass Permissions"
```

**Replacement:**
```
case"auto":return"Auto Mode";case"bypassPermissions":return"Bypass Permissions"
```

**Effect:** The mode name displays as "Auto Mode" in the status bar.

---

### Patch 3: Add Icon for Auto Mode

**Purpose:** Show a distinct icon for auto mode in the UI.

**Pattern:**
```
case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵"
```

**Replacement:**
```
case"acceptEdits":return"⏵⏵";case"auto":return"»»";case"bypassPermissions":return"⏵⏵"
```

**Effect:** Auto mode shows "»»" (double guillemet) icon instead of "⏵⏵".

---

### Patch 4: Modify Mode Cycling Logic

**Purpose:** Insert auto mode into the shift+tab cycling sequence.

**Pattern:**
```
case"bypassPermissions":return"default"
```

**Replacement:**
```
case"bypassPermissions":return"auto";case"auto":return"default"
```

**Effect:** Mode cycling becomes:
- `default` → `plan` → `bypassPermissions` → `auto` → `default`

---

### Patch 5: Auto Mode Permission Behavior

**Purpose:** Make auto mode behave like bypassPermissions for tool permissions.

Multiple patterns are patched:

#### Patch 5a: Main Permission Allow Check
**Pattern:**
```
Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="plan"
```
**Replacement:**
```
Z.toolPermissionContext.mode==="bypassPermissions"||Z.toolPermissionContext.mode==="auto"||Z.toolPermissionContext.mode==="plan"
```

#### Patch 5b: Q.mode Passthrough Check
**Pattern:**
```
Q.mode==="bypassPermissions"
```
**Replacement:**
```
Q.mode==="bypassPermissions"||Q.mode==="auto"
```

#### Patch 5c: Mode||V Permission Checks
**Pattern:**
```
mode==="bypassPermissions"||V)
```
**Replacement:**
```
mode==="bypassPermissions"||mode==="auto"||V)
```

**Effect:** Auto mode bypasses permission prompts just like bypassPermissions mode across all permission check locations.

---

## How Auto Mode Works

When auto mode is active:

1. **Permission bypass:** All tool permissions are automatically approved (same as bypassPermissions)
2. **Stop hook enforcement:** The Stop hook at `~/.claude/settings.json` detects auto mode via the flag file and blocks Claude from ending turns voluntarily
3. **Flag file:** `~/.cache/claude-unleashed/auto-mode/active-<WRAPPER_PID>` indicates auto mode is active for a specific session

## Managing Patches

### Apply patches
```bash
unleash patch
# or just
unleash
```

### Check status
```bash
unleash status
```

### Restore original
```bash
unleash unpatch
```

## Re-patching After Updates

When Claude Code updates via `npm update`, the patches are lost. Re-run:
```bash
unleash patch
```

## Backup Files

Each patch creates a timestamped backup:
```
cli.js.backup.YYYYMMDDHHMMSS
```

Backups are stored alongside the original `cli.js` file.

## Verification

To verify patches are applied:
```bash
grep '"auto"' ~/.nvm/versions/node/*/lib/node_modules/@anthropic-ai/claude-code/cli.js
```

If output contains `"auto"`, patches are applied.

## Risks

1. **Breaking changes:** Claude Code updates may change the patterns, causing patches to fail
2. **Incomplete patching:** If a pattern isn't found, that specific patch is skipped
3. **Bundle changes:** Major refactors of cli.js structure may require patch updates

## Rollback

If something breaks:
```bash
unleash unpatch
```

This restores the most recent backup.
