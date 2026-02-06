# Claude Code Version Whitelisting

This document defines the official requirements for whitelisting a Claude Code version in Agent Unleashed.

## Overview

Agent Unleashed uses a version filter (configured in `Cargo.toml`) to control which Claude Code versions can be installed. Only whitelisted versions are installed when using `latest`. This ensures that every deployed version has been verified to work with the auto mode patch system and the `/auto` slash command fallback.

## Whitelisting Requirements

A Claude Code version **must** satisfy all of the following before being added to the whitelist.

### 1. Auto Mode Patch Compatibility

The `cli.js` bundle must contain recognizable patterns for all 8 patches defined in `scripts/patch-claude.sh`. Specifically:

| Patch | What It Does | Required Pattern |
|-------|-------------|------------------|
| **1 - Modes array** | Registers `"auto"` as a valid mode | `VAR=["acceptEdits","bypassPermissions"...` must exist so `"auto"` can be inserted |
| **2 - Display name** | Shows "Auto Mode" in the UI | `case"bypassPermissions":return"Bypass Permissions"` must exist |
| **3 - Icon** | Renders `»»` for auto mode | `case"acceptEdits":return"⏵⏵";case"bypassPermissions":return"⏵⏵"` must exist |
| **4 - Cycling** | Shift+tab cycles through auto mode | `case"bypassPermissions":return"default"` must exist |
| **5a-d - Permissions** | Auto mode bypasses permission prompts | `toolPermissionContext.mode==="bypassPermissions"` and related patterns must exist |
| **6 - Color** | Auto mode renders in yellow (warning) text | `case"acceptEdits":return"autoAccept";case"bypassPermissions":return"error"` must exist |
| **7a,b - Flag file** | Creates/removes `~/.cache/agent-unleashed/auto-mode/active-<PID>` | Telemetry and delegate patterns must exist for flag file injection |
| **8a-c - Env startup** | `CLAUDE_AUTO_MODE=1` starts in auto mode | `permissionMode:...??"default"` patterns must exist |

### 2. Version-Specific Patch Config

A `.conf` file must be created at `scripts/patches/versions/<version>.conf` with the correct minified variable names extracted from that version's `cli.js`. Required variables:

```bash
MODES_ARRAY_VAR      # Variable holding the modes array
PERMISSION_CTX_VAR   # Permission context in .mode==="bypassPermissions"||BOOL) checks
PERMISSION_BOOL_VAR  # Boolean variable in permission checks
MODE_VAR             # Mode variable in telemetry check
TELEMETRY_FN         # Telemetry function for auto-accept-mode
DELEGATE_FN1         # First delegate function on mode change
DELEGATE_FN2         # Second delegate function on mode change
TOOL_PERMISSION_CTX  # toolPermissionContext variable
PASSTHROUGH_MODE_VAR # Passthrough mode check variable
DELEGATE_MODE_CTX    # Mode context variable in delegate check
```

### 3. Successful Patch Application

Running `scripts/patch-claude.sh` against the version must:

- Apply all 8 patches without errors or "pattern not found" warnings
- Pass verification (auto mode found in modes array after patching)
- Not break Claude Code functionality (agent starts and operates normally)

### 4. Auto Mode Functional Verification

After patching, the following must work:

- **Yellow text**: Auto mode displays with yellow/warning color in the status bar
- **»» icon**: The auto mode icon renders as `»»` (double guillemet)
- **Shift+tab cycling**: Mode cycling includes auto mode in the sequence: `default` -> `plan` -> `bypassPermissions` -> `auto` -> `default`
- **Permission bypass**: Auto mode bypasses all tool permission prompts
- **Flag file sync**: Entering auto mode via shift+tab creates `~/.cache/agent-unleashed/auto-mode/active-<PID>`, leaving removes it
- **Stop hook enforcement**: The Stop hook (`auto-mode-stop.sh`) blocks Claude from ending its turn when auto mode is active
- **Env var startup**: Setting `CLAUDE_AUTO_MODE=1` starts Claude in auto mode

### 5. `/auto` Slash Command Fallback

The `/auto` slash command (defined in `plugins/unleashed/auto-mode/commands/auto.md`) must work independently of the patch as a fallback mechanism:

- Toggles auto mode via the flag file (`~/.cache/agent-unleashed/auto-mode/active-<WRAPPER_PID>`)
- The Stop hook detects the flag file and blocks turn-ending
- Works even on unpatched Claude Code versions (no yellow indicator, but core auto-mode loop functions)

## How to Whitelist a New Version

### Step 1: Extract Variable Names

Install the target version and extract minified variable names from `cli.js`:

```bash
# Install the specific version
npm install -g @anthropic-ai/claude-code@<version>

# Find cli.js
CLI_JS=$(npm root -g)/@anthropic-ai/claude-code/cli.js

# Extract each variable (examples - adjust grep patterns as needed)
grep -oP '[A-Za-z_$][A-Za-z0-9_$]*=\["acceptEdits","bypassPermissions"' "$CLI_JS"
grep -oP '[A-Za-z_$][A-Za-z0-9_$]*\.toolPermissionContext\.mode==="bypassPermissions"' "$CLI_JS"
grep -oP 'if\([A-Za-z_$][A-Za-z0-9_$]*==="acceptEdits"\)[A-Za-z_$][A-Za-z0-9_$]*\(' "$CLI_JS"
grep -oP '.mode==="delegate"&&.{1,10}!=="delegate"\).{1,30}' "$CLI_JS"
grep -oP '[A-Za-z_$][A-Za-z0-9_$]*\.mode==="bypassPermissions"\|\|[A-Za-z_$][A-Za-z0-9_$]*\)' "$CLI_JS"
```

### Step 2: Create Version Config

Create `scripts/patches/versions/<version>.conf` with the extracted variable names. Use the latest existing `.conf` file as a template.

### Step 3: Test Patching

```bash
# Apply the patch
./scripts/patch-claude.sh

# Verify all patches applied (check output for warnings)
# Start Claude Code and verify auto mode works
claude
# Press shift+tab to cycle to auto mode
# Verify yellow »» indicator appears
```

### Step 4: Test `/auto` Fallback

```bash
# Unpatch first
./scripts/unpatch-claude.sh

# Start Claude Code and run /auto
# Verify the stop hook blocks turn-ending
# Verify the flag file is created
ls ~/.cache/agent-unleashed/auto-mode/
```

### Step 5: Add to Whitelist

Edit `Cargo.toml` and add the version to the whitelist array (newest first):

```toml
[package.metadata.claude-code-whitelist]
versions = ["<new-version>", "2.1.29", ...]
```

### Step 6: Commit

```bash
git add scripts/patches/versions/<version>.conf Cargo.toml
git commit -m "feat: whitelist Claude Code v<version>"
```

## Version History

| Version | Date Whitelisted | Notes |
|---------|-----------------|-------|
| 2.1.32 | 2026-02-05 | All patches verified |
| 2.1.29 | 2026-02-02 | All patches verified |
| 2.1.22 | 2026-01-28 | Variable naming changes from 2.1.12 |
| 2.1.12 | — | Baseline version for current patch format |
| 2.1.4 | — | — |
| 2.1.3 | — | — |
| 2.1.2 | — | — |
| 2.0.77 | — | Legacy version |

## Blacklisted Versions

Versions with known critical issues are blacklisted in `Cargo.toml` under `[package.metadata.claude-code-blacklist]`. These are skipped even in blacklist mode.

| Version | Reason |
|---------|--------|
| 2.1.5 | Known issues |
| 2.1.1 | Known issues |
| 2.1.0 | ESM bundling broke flag file patches |

## Configuration

### Filter Mode

Set in `Cargo.toml` under `[package.metadata.claude-code-versions]`:

- **whitelist** (default): Only versions in the whitelist are installable
- **blacklist**: All versions except blacklisted ones are installable

### User Overrides

Users can override the whitelist/blacklist by creating:

- `~/.config/agent-unleashed/whitelist.txt` — one version per line
- `~/.config/agent-unleashed/blacklist.txt` — one version per line
- `~/.config/agent-unleashed/config.toml` with `version_filter_mode = "blacklist"` to change the mode
