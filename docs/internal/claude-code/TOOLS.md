# Claude Code Built-in Tools — Internal Reference

Reference for all built-in tools available to Claude Code agents. Useful for:
- Writing hook matchers (`PreToolUse`/`PostToolUse`)
- Understanding tool input/output schemas for hook scripts
- Knowing which tools are read-only vs. write

Last verified: 2026-03-31 (Claude Code ~1.x)

---

## Tool Names (for Hook Matchers)

These are the exact tool names as they appear in the `tool_name` field of hook input JSON and in `matcher` patterns:

| Tool | Category | Mutates FS? |
|------|----------|-------------|
| `Read` | File I/O | No |
| `Write` | File I/O | Yes |
| `Edit` | File I/O | Yes |
| `MultiEdit` | File I/O | Yes |
| `NotebookRead` | Notebook | No |
| `NotebookEdit` | Notebook | Yes |
| `Bash` | Shell | Yes (potentially) |
| `Grep` | Search | No |
| `Glob` | Search | No |
| `LS` | Search | No |
| `WebFetch` | Network | No |
| `WebSearch` | Network | No |
| `Agent` | Agent | Delegated |
| `Task` | Agent | Delegated |
| `TodoRead` | State | No |
| `TodoWrite` | State | Yes |
| `ExitPlanMode` | Control | No |
| `EnterPlanMode` | Control | No |
| `Skill` | Meta | No |

MCP tools appear with the format `mcp__<server>__<tool>` (e.g., `mcp__playwright__browser_click`).

---

## Hook Input JSON Schema

For `PreToolUse` and `PostToolUse` hooks, Claude Code sends JSON to the hook's stdin:

```json
{
  "session_id": "uuid",
  "transcript_path": "/path/to/session.jsonl",
  "hook_event_name": "PreToolUse",
  "tool_name": "Bash",
  "tool_input": { ... }
}
```

For `PostToolUse`, additional fields are present:
```json
{
  "tool_response": { ... },
  "tool_error": null
}
```

---

## Individual Tool Schemas

### Read

Reads a file from the local filesystem.

```json
{
  "file_path": "/absolute/path/to/file",
  "offset": 100,
  "limit": 200
}
```

- `file_path` (required): Absolute path.
- `offset` (optional): Line number to start reading from (1-based).
- `limit` (optional): Number of lines to read.

Hook matcher: `"Read"`

### Write

Writes (or overwrites) a file.

```json
{
  "file_path": "/absolute/path/to/file",
  "content": "file content here"
}
```

Hook matcher: `"Write"`

### Edit

Performs an exact string replacement in a file.

```json
{
  "file_path": "/absolute/path/to/file",
  "old_string": "text to find",
  "new_string": "replacement text",
  "replace_all": false
}
```

- `replace_all` (optional, default false): Replace all occurrences vs. first only.

Hook matcher: `"Edit"`

### MultiEdit

Multiple edits applied to one or more files atomically.

```json
{
  "edits": [
    {
      "file_path": "/path/to/file",
      "old_string": "old text",
      "new_string": "new text",
      "replace_all": false
    }
  ]
}
```

Hook matcher: `"MultiEdit"`

### Bash

Executes a shell command.

```json
{
  "command": "ls -la",
  "timeout": 30000,
  "description": "List files"
}
```

- `command` (required): Shell command to run.
- `timeout` (optional): Timeout in milliseconds. Defaults to `BASH_DEFAULT_TIMEOUT_MS` env var (unleash sets this to 999999999).
- `description` (optional): Human-readable description (shown in UI).

Hook matcher: `"Bash"`

**Note**: This is the highest-risk tool. `PreToolUse` hooks that block dangerous Bash commands should check `tool_input.command`.

### Grep

Search file contents using regex.

```json
{
  "pattern": "function\\s+\\w+",
  "path": "/search/root",
  "glob": "*.js",
  "type": "js",
  "output_mode": "content",
  "-i": true,
  "-n": true,
  "-A": 2,
  "-B": 2,
  "head_limit": 100
}
```

- `pattern` (required): Regex pattern (ripgrep syntax).
- `path` (optional): Directory to search.
- `glob` (optional): File glob filter.
- `type` (optional): File type filter (ripgrep `--type`).
- `output_mode`: `"content"` | `"files_with_matches"` | `"count"`.

Hook matcher: `"Grep"`

### Glob

Find files by glob pattern.

```json
{
  "pattern": "**/*.ts",
  "path": "/search/root"
}
```

Hook matcher: `"Glob"`

### LS

List directory contents.

```json
{
  "path": "/directory/path"
}
```

Hook matcher: `"LS"`

### WebFetch

Fetch a URL and return its content.

```json
{
  "url": "https://example.com/page",
  "prompt": "Extract the main content"
}
```

Hook matcher: `"WebFetch"`

### WebSearch

Search the web.

```json
{
  "query": "search terms"
}
```

Hook matcher: `"WebSearch"`

### Agent

Spawn a subagent. The Agent tool is used for complex multi-step research or tasks delegated to a specialized agent.

```json
{
  "prompt": "Task description for the agent",
  "subagent_type": "general-purpose",
  "description": "Short description",
  "model": "claude-sonnet-4-6"
}
```

Hook matcher: `"Agent"` or `"Task"`

### TodoRead / TodoWrite

Manage the in-session todo list.

**TodoWrite** input:
```json
{
  "todos": [
    {
      "id": "1",
      "content": "Task description",
      "status": "pending",
      "priority": "high"
    }
  ]
}
```

`status` values: `"pending"` | `"in_progress"` | `"completed"`
`priority` values: `"high"` | `"medium"` | `"low"`

Hook matchers: `"TodoRead"`, `"TodoWrite"`

### NotebookRead / NotebookEdit

Read or edit Jupyter notebook cells.

**NotebookEdit** input:
```json
{
  "notebook_path": "/path/to/notebook.ipynb",
  "cell_id": "cell-uuid",
  "new_source": "print('hello')",
  "cell_type": "code",
  "edit_mode": "replace"
}
```

Hook matchers: `"NotebookRead"`, `"NotebookEdit"`

### Skill

Invokes a named skill (loads skill content into context).

```json
{
  "skill": "skill-name",
  "args": "optional arguments"
}
```

Hook matcher: `"Skill"`

---

## MCP Tool Names

MCP tools are namespaced: `mcp__<server-name>__<tool-name>`

Examples from the playwright MCP server:
```
mcp__playwright__browser_navigate
mcp__playwright__browser_click
mcp__playwright__browser_type
mcp__playwright__browser_screenshot
```

To match all playwright tools in a hook:
```json
{ "matcher": "mcp__playwright__.*" }
```

To match all MCP tools:
```json
{ "matcher": "mcp__.*" }
```

---

## Tool Categories for Hook Patterns

Common matcher patterns for hooks:

```json
// All file-writing tools
{ "matcher": "Write|Edit|MultiEdit|NotebookEdit" }

// All shell execution
{ "matcher": "Bash" }

// All network access
{ "matcher": "WebFetch|WebSearch|mcp__.*" }

// All read-only tools (for auditing)
{ "matcher": "Read|Grep|Glob|LS|NotebookRead|WebFetch|WebSearch|TodoRead" }

// All state-mutating tools
{ "matcher": "Write|Edit|MultiEdit|Bash|TodoWrite|NotebookEdit" }
```

---

## `allowed-tools` in Commands and Agents

Command and agent frontmatter can restrict which tools are available:

```yaml
allowed-tools: ["Read", "Bash", "Grep", "Glob"]
```

- Tools not in the list are unavailable to Claude during that command/agent session
- Omitting `allowed-tools` allows all tools
- MCP tools can be included: `"mcp__playwright__browser_navigate"`
- Use this to scope agent capabilities and reduce risk surface

---

## Hook Input: Full Example

Complete hook input for a `PreToolUse` on `Bash`:

```json
{
  "session_id": "3f7a8b2c-1234-5678-abcd-ef0123456789",
  "transcript_path": "/home/user/.claude/projects/-home-user-myproject/3f7a8b2c.jsonl",
  "hook_event_name": "PreToolUse",
  "tool_name": "Bash",
  "tool_input": {
    "command": "rm -rf /tmp/test",
    "timeout": 30000,
    "description": "Clean up temp files"
  }
}
```

Hook script to block dangerous commands:

```bash
#!/bin/bash
input=$(cat)
cmd=$(echo "$input" | jq -r '.tool_input.command // ""')

if echo "$cmd" | grep -qE 'rm -rf /[^t]|dd if=|mkfs'; then
  echo '{"decision":"deny","reason":"Dangerous command pattern detected"}' >&2
  exit 2
fi
exit 0
```

---

## Tool Response Shapes (PostToolUse)

The `tool_response` field in `PostToolUse` hooks varies by tool:

**Read**: Returns file contents as string
**Write/Edit**: Returns confirmation message
**Bash**: Returns `{ "stdout": "...", "stderr": "...", "exit_code": 0 }`
**Grep/Glob**: Returns matching lines or file paths as string
**WebFetch**: Returns page content as string

Note: The exact shape can vary across Claude Code versions. Parse defensively.
