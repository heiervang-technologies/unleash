# Claude Code Patches

This directory contains version-specific patch configurations for adding Auto Mode to Claude Code.

## How It Works

Claude Code is distributed as minified JavaScript. The variable names change between versions, so we need version-specific configurations that map the patch targets to the correct minified variable names.

## Directory Structure

```
patches/
├── README.md           # This file
└── versions/           # Version-specific configurations
    └── 2.1.5.conf      # Config for Claude Code 2.1.5
```

## Configuration Variables

Each `.conf` file defines these variables:

| Variable | Description |
|----------|-------------|
| `MODES_ARRAY_VAR` | Variable holding the modes array `["acceptEdits","bypassPermissions",...]` |
| `PERMISSION_CTX_VAR` | Permission context variable used in `.mode==="bypassPermissions"` checks |
| `MODE_VAR` | Mode variable used in telemetry and delegate checks |
| `TELEMETRY_FN` | Function called for mode telemetry |
| `DELEGATE_FN1` | First delegate function called on mode change |
| `DELEGATE_FN2` | Second delegate function called on mode change |

## Creating a New Version Config

When a new Claude Code version is released:

1. Install the new version: `npm install -g @anthropic-ai/claude-code@latest`
2. Analyze the minified `cli.js` to find the new variable names
3. Create a new config file: `versions/X.Y.Z.conf`
4. Test with: `bash scripts/patch-claude.sh`

### Finding Variable Names

Use this Node.js script to extract variable names from `cli.js`:

```javascript
const fs = require('fs');
const content = fs.readFileSync('/path/to/cli.js', 'utf8');

// Modes array
const modeArray = content.match(/([a-zA-Z0-9_$]+)=\["acceptEdits",/);
console.log('MODES_ARRAY_VAR:', modeArray?.[1]);

// Permission context
const permV = content.match(/([a-zA-Z0-9_$]+)\.mode==="bypassPermissions"\|\|V\)/);
console.log('PERMISSION_CTX_VAR:', permV?.[1]);

// Mode and telemetry
const telemetry = content.match(/if\(([a-zA-Z0-9_$]+)==="acceptEdits"\)([a-zA-Z0-9_$]+)\("auto-accept-mode"\)/);
console.log('MODE_VAR:', telemetry?.[1]);
console.log('TELEMETRY_FN:', telemetry?.[2]);

// Delegate functions
const delegate = content.match(/B\.mode==="delegate"&&([a-zA-Z0-9_$]+)!==="delegate"\)([a-zA-Z0-9_$]+)\(!0\),([a-zA-Z0-9_$]+)\(!0\)/);
console.log('DELEGATE_FN1:', delegate?.[2]);
console.log('DELEGATE_FN2:', delegate?.[3]);
```

## Fallback Behavior

If no exact version match is found, `patch-claude.sh` will use the closest lower version configuration. This often works since variable names don't always change between minor versions.
