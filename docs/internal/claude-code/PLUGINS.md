# Claude Code Plugin System — Internal Reference

Internal technical reference for the Claude Code `--plugin-dir` plugin loading system.
For the developer workflow guide, see [plugin-development.md](plugin-development.md).

Last verified: 2026-03-31 (Claude Code ~1.x)

---

## Overview

Claude Code supports loading plugins via the `--plugin-dir` flag. Each invocation of `--plugin-dir` adds one plugin directory. Multiple flags can be passed to load multiple plugins:

```bash
claude --plugin-dir /path/to/plugin-a --plugin-dir /path/to/plugin-b
```

Unleash loads all bundled plugins automatically by scanning `plugins/bundled/` and emitting one `--plugin-dir` per subdirectory (see `src/launcher.rs:find_plugins()`).

---

## Plugin Directory Layout

Claude Code scans the root of each `--plugin-dir` path for the following component directories:

```
plugin-root/
├── .claude-plugin/
│   └── plugin.json          # Manifest (required)
├── commands/                # Slash commands (.md files)
├── agents/                  # Agent definitions (.md files)
├── skills/                  # Skill directories (each with SKILL.md)
│   └── skill-name/
│       └── SKILL.md
├── hooks/
│   └── hooks.json           # Hook configuration
├── .mcp.json                # MCP server definitions
└── README.md
```

**Key rule**: Component directories (`commands/`, `agents/`, `skills/`, `hooks/`) must be at the **plugin root level**, not inside `.claude-plugin/`.

---

## Plugin Manifest

The manifest lives at `.claude-plugin/plugin.json`. It is primarily metadata — Claude Code does not currently gatekeep component loading based on manifest fields. The manifest is required for the plugin to be recognized.

```json
{
  "name": "plugin-name",
  "version": "1.0.0",
  "description": "Brief description",
  "author": {
    "name": "Author Name",
    "email": "author@example.com"
  },
  "keywords": ["optional", "tags"]
}
```

**Verified fields**: `name`, `version`, `description`, `author` (object with `name`/`email`), `keywords` (array).

No `dependencies`, `hooks`, `engines`, or other fields have been observed in the source. Keep the manifest minimal.

---

## `${CLAUDE_PLUGIN_ROOT}` Environment Variable

When Claude Code executes a hook command from a plugin, it sets `CLAUDE_PLUGIN_ROOT` to the **absolute path** of that plugin's root directory. This makes hook scripts portable regardless of working directory.

```bash
# In hooks.json — always use ${CLAUDE_PLUGIN_ROOT}
"command": "${CLAUDE_PLUGIN_ROOT}/hooks/scripts/my-hook.sh"

# Never use relative paths
"command": "./hooks/scripts/my-hook.sh"  # WRONG — breaks when CWD differs
```

Unleash canonicalizes plugin paths to absolute paths before passing them via `--plugin-dir` (see `src/launcher.rs:find_plugin_dirs()`), which ensures `${CLAUDE_PLUGIN_ROOT}` is always a valid absolute path.

---

## hooks/hooks.json Format

The hooks file uses a **wrapper object** with a `description` key at the top level, then a `hooks` key containing the event map:

```json
{
  "description": "Human-readable description of this hook set",
  "hooks": {
    "EventName": [
      {
        "matcher": "ToolName",
        "hooks": [
          {
            "type": "command",
            "command": "${CLAUDE_PLUGIN_ROOT}/hooks/scripts/handler.sh",
            "timeout": 30
          }
        ]
      }
    ]
  }
}
```

**Critical**: The `description` top-level wrapper is **required** for plugin hooks. This differs from `settings.json` hooks which use the event map directly at the top level.

### Event Names

See [HOOKS.md](HOOKS.md) for the complete list. The most commonly used in plugins:

| Event | Timing | Notes |
|-------|--------|-------|
| `PreToolUse` | Before tool runs | Can block (exit 2) |
| `PostToolUse` | After tool completes | Observational |
| `Stop` | Before agent stops | Can block (exit 2) |
| `Notification` | On agent notification | Observational |
| `SessionStart` | Session begins | Context loading |
| `UserPromptSubmit` | On user message | Can modify input |

### Matcher Field

The `matcher` field is a regex pattern matched against the tool name for `PreToolUse`/`PostToolUse`. For other events, omit `matcher` or use `"*"`.

```json
{ "matcher": "Bash" }           // exact match
{ "matcher": "Write|Edit" }     // alternation
{ "matcher": ".*" }             // all tools
```

If `matcher` is omitted entirely for tool events, the hook fires for all tools.

### Hook Types

**`command`** — Shell command executed via `/bin/sh`:
```json
{
  "type": "command",
  "command": "${CLAUDE_PLUGIN_ROOT}/hooks/scripts/handler.sh",
  "timeout": 30
}
```

**`prompt`** — Claude evaluates a prompt about the current action:
```json
{
  "type": "prompt",
  "prompt": "Evaluate this action. Return 'approve' or 'deny' with reason.",
  "timeout": 30
}
```

**`http`** — HTTP request to external endpoint (less common):
```json
{
  "type": "http",
  "url": "https://example.com/webhook",
  "timeout": 30
}
```

### Hook Exit Codes

For `command` type hooks on `PreToolUse` and `Stop`:

| Exit Code | Meaning |
|-----------|---------|
| `0` | Approve / allow |
| `1` | Non-fatal error (logged, continues) |
| `2` | Block / deny (writes stderr to Claude) |

Output JSON structure for blocking (write to stderr, exit 2):
```json
{
  "decision": "deny",
  "reason": "Human-readable reason shown to Claude",
  "systemMessage": "Optional additional context"
}
```

For `updatedInput` (modify tool input before execution), write JSON to stdout and exit 0:
```json
{
  "updatedInput": { ...modified_tool_input... }
}
```

---

## Plugin vs. settings.json Hooks

Claude Code loads hooks from two sources, merged at runtime:

| Source | Format | Purpose |
|--------|--------|---------|
| `~/.claude/settings.json` `hooks` key | Event map directly | User-level default hooks |
| Plugin `hooks/hooks.json` | `{ description, hooks: { event map } }` | Plugin-scoped hooks |

Unleash's default hooks (e.g., `PreCompact`) are installed into `settings.json` by `HookManager`. Plugin hooks are loaded by Claude Code via `--plugin-dir` independently. They do **not** conflict — both sets run.

**Important**: Unleash avoids installing plugin hooks into `settings.json`. Plugin hooks must live in `hooks/hooks.json` within the plugin directory, not in `settings.json`.

---

## MCP Server Configuration (.mcp.json)

Plugin-level MCP servers are declared in `.mcp.json` at the plugin root:

```json
{
  "mcpServers": {
    "server-name": {
      "command": "node",
      "args": ["${CLAUDE_PLUGIN_ROOT}/server/index.js"],
      "env": {
        "MY_VAR": "${MY_VAR}"
      }
    }
  }
}
```

**Transport types** supported:
- `stdio` (default): Local process, stdin/stdout communication
- `sse`: Server-Sent Events (hosted servers, OAuth flows)
- `http`: HTTP/REST based

When using `stdio`, the `command` + `args` define the server process. When using `sse` or `http`, a `url` field is used instead.

---

## Commands

Command files are Markdown with YAML frontmatter, placed in `commands/`:

```markdown
---
name: command-name
description: Brief description shown in help
argument-hint: "[optional] [args]"
allowed-tools: ["Read", "Bash", "Grep"]
---

# Command instructions for Claude...
```

**Frontmatter fields**:
- `name` (required): Slash command name. Invoked as `/command-name`.
- `description` (required): Shown in `/help` and command picker.
- `argument-hint` (optional): Displayed as usage hint.
- `allowed-tools` (optional): Restricts which tools Claude can use during this command.

**Naming**: File name (`my-command.md`) determines the slug. The `name` in frontmatter can differ but conventionally matches.

---

## Agents

Agent files are Markdown with YAML frontmatter, placed in `agents/`:

```markdown
---
name: Agent Display Name
description: |
  When to invoke this agent. Include <example> blocks:
  <example>user asks to "do X task"</example>
model: claude-sonnet-4-6
color: blue
allowed-tools: ["Read", "Write", "Edit", "Bash"]
---

# Agent system prompt...
```

**Frontmatter fields**:
- `name` (optional): Display name in UI.
- `description` (required): Used by Claude to decide when to invoke this agent. Should include `<example>` trigger phrases.
- `model` (optional): Specific Claude model ID. Defaults to session model.
- `color` (optional): UI color hint (`red`, `green`, `blue`, `yellow`, `purple`, `orange`).
- `allowed-tools` (optional): Restricts agent's available tools.

---

## Skills

Skills are auto-activating knowledge modules. Each skill is a directory with a `SKILL.md` file:

```
skills/
└── skill-name/
    └── SKILL.md
```

`SKILL.md` frontmatter:
```markdown
---
name: Skill Name
description: This skill should be used when the user asks to "X" or "Y"...
version: 1.0.0
---

# Skill content loaded into context...
```

**Activation**: Claude Code reads the `description` and includes the skill content in context when the description semantically matches the current task. The description should use natural trigger phrases.

---

## Loading Priority and Conflicts

When multiple plugins define the same command name, the last loaded plugin wins (determined by `--plugin-dir` order). Unleash loads plugins in filesystem iteration order.

Hook execution order when multiple plugins register the same event:
- All matching hooks from all plugins run
- `PreToolUse` blocks if **any** hook returns exit 2

There is no explicit plugin priority or dependency system — keep plugins independent.

---

## Unleash-Specific: Plugin Discovery

Unleash's `find_plugin_dirs()` (in `src/launcher.rs`) applies deduplication:

1. Scans `plugins/bundled/` (relative to CWD — dev repo path)
2. Falls back to `~/.local/share/unleash/plugins/` (installed path)
3. **De-duplicates by directory name** — if a plugin exists in both locations, the repo version wins

This prevents duplicate hook firing when developing locally (repo plugins shadow installed plugins of the same name).
