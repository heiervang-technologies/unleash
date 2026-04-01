# Claude Code Hook System Reference

> Last verified: 2026-03-31, verified against source

This document provides a comprehensive reference for the Claude Code hook system,
covering all supported events, hook types, input/output schemas, execution semantics,
and permission resolution.

---

## Table of Contents

1. [Overview](#overview)
2. [Hook Events](#hook-events)
3. [Hook Types](#hook-types)
4. [Hook Schema Fields](#hook-schema-fields)
5. [Hook Input Data](#hook-input-data)
6. [Hook Output Schema](#hook-output-schema)
7. [Hook-Specific Output Variants](#hook-specific-output-variants)
8. [Hook Execution Flow](#hook-execution-flow)
9. [Hook Permission Resolution](#hook-permission-resolution)
10. [Configuration Examples](#configuration-examples)

---

## Overview

Hooks are user-defined callbacks that Claude Code invokes at specific points during a
session lifecycle. They enable custom logic such as:

- Approving or blocking tool usage
- Injecting context into the conversation
- Triggering external systems on events
- Enforcing organizational policies
- Implementing autonomous workflows (e.g., auto-mode stop hooks)

Hooks are configured in `settings.json` (under the `hooks` key), registered
programmatically via the SDK/plugin system, or attached at the session level by
agents and skills.

---

## Hook Events

There are **27** hook events. Each event fires at a specific point in the session
lifecycle.

### Lifecycle Events

| Event | When it fires |
|---|---|
| `SessionStart` | Session begins (startup, resume, clear, or compact) |
| `SessionEnd` | Session is ending |
| `Setup` | Initial setup or periodic maintenance |
| `Stop` | Model has finished responding and wants to stop |
| `StopFailure` | Stop hook itself failed |
| `PreCompact` | Before conversation compaction |
| `PostCompact` | After conversation compaction |
| `CwdChanged` | Working directory changed |
| `InstructionsLoaded` | CLAUDE.md / instructions have been loaded |
| `ConfigChange` | Configuration was modified |

### Tool Events

| Event | When it fires |
|---|---|
| `PreToolUse` | Before a tool is executed |
| `PostToolUse` | After a tool completes successfully |
| `PostToolUseFailure` | After a tool execution fails |

### Permission Events

| Event | When it fires |
|---|---|
| `PermissionRequest` | Tool is requesting permission from the user |
| `PermissionDenied` | User or policy denied a permission request |

### Agent Events

| Event | When it fires |
|---|---|
| `SubagentStart` | A sub-agent is starting |
| `SubagentStop` | A sub-agent has stopped |

### User Interaction Events

| Event | When it fires |
|---|---|
| `UserPromptSubmit` | User submits a prompt |
| `Notification` | A notification is being sent |
| `Elicitation` | An elicitation (question to user) is being presented |
| `ElicitationResult` | User responded to an elicitation |

### Task Events

| Event | When it fires |
|---|---|
| `TaskCreated` | A new task was created |
| `TaskCompleted` | A task has completed |
| `TeammateIdle` | A teammate agent is idle |

### Workspace Events

| Event | When it fires |
|---|---|
| `WorktreeCreate` | A git worktree is being created |
| `WorktreeRemove` | A git worktree is being removed |
| `FileChanged` | A file changed on disk |

---

## Hook Types

### Persistable Types (user-configurable in settings.json)

| Type | Description | Execution model |
|---|---|---|
| `command` | Runs a shell command (bash or powershell). Input is piped via stdin; output is read from stdout as JSON. | Subprocess |
| `prompt` | Sends a prompt to a lightweight LLM. The `$ARGUMENTS` placeholder in the prompt string is replaced with the JSON-serialized hook input. | LLM call |
| `http` | Sends an HTTP POST with JSON body to a URL. Expects a JSON response conforming to the output schema. | Network request |
| `agent` | Runs a full sub-agent with access to tools. The `$ARGUMENTS` placeholder in the prompt string is replaced with hook input. Most powerful but slowest. | Sub-agent |

### Internal-Only Types (not user-configurable)

| Type | Description |
|---|---|
| `callback` | Programmatic JavaScript callback, used internally by the SDK and plugin system. |
| `function` | Session-scoped function, attached by agents or skills during a session. |

---

## Hook Schema Fields

### Common Fields (all hook types)

| Field | Type | Required | Description |
|---|---|---|---|
| `type` | `"command"` \| `"prompt"` \| `"http"` \| `"agent"` | Yes | Hook type |
| `if` | `string` | No | Pattern filter using permission rule syntax (e.g., `"Bash(git *)"`) |
| `timeout` | `number` | No | Maximum execution time in seconds |
| `statusMessage` | `string` | No | Custom text shown in the spinner while the hook runs |
| `once` | `boolean` | No | If `true`, the hook is removed after its first execution |
| `async` | `boolean` | No | If `true`, the hook runs in the background and does not block |
| `asyncRewake` | `boolean` | No | Background hook that wakes the model when it exits with code 2 |

### Command-Specific Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `command` | `string` | Yes | The shell command to execute |
| `shell` | `"bash"` \| `"powershell"` | No | Shell to use (defaults to platform default) |

### Prompt-Specific Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `prompt` | `string` | Yes | Prompt text; use `$ARGUMENTS` as placeholder for hook input |
| `model` | `string` | No | Model to use for the LLM call |

### HTTP-Specific Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `url` | `string` | Yes | URL to POST to |
| `headers` | `object` | No | Additional HTTP headers |
| `allowedEnvVars` | `string[]` | No | Environment variables whose values may be included in the request |

### Agent-Specific Fields

| Field | Type | Required | Description |
|---|---|---|---|
| `prompt` | `string` | Yes | Agent prompt; use `$ARGUMENTS` as placeholder for hook input |
| `model` | `string` | No | Model to use for the agent |

---

## Hook Input Data

Hook input is delivered as JSON via stdin (for `command` hooks), HTTP POST body
(for `http` hooks), or substituted into the `$ARGUMENTS` placeholder (for `prompt`
and `agent` hooks).

### Base Fields (present on all events)

| Field | Type | Description |
|---|---|---|
| `session_id` | `string` | Current session identifier |
| `transcript_path` | `string` | Path to the conversation transcript file |
| `cwd` | `string` | Current working directory |
| `permission_mode` | `string` | Current permission mode |
| `agent_id` | `string` | Identifier of the agent that triggered the event |
| `agent_type` | `string` | Type of agent (e.g., `"main"`, `"subagent"`) |

### Event-Specific Fields

#### PreToolUse

| Field | Type | Description |
|---|---|---|
| `tool_name` | `string` | Name of the tool about to be used |
| `tool_input` | `object` | Parameters being passed to the tool |
| `tool_use_id` | `string` | Unique identifier for this tool invocation |

#### PostToolUse

| Field | Type | Description |
|---|---|---|
| `tool_name` | `string` | Name of the tool that was used |
| `tool_input` | `object` | Parameters that were passed to the tool |
| `tool_response` | `any` | The tool's return value |
| `tool_use_id` | `string` | Unique identifier for this tool invocation |

#### PostToolUseFailure

| Field | Type | Description |
|---|---|---|
| `tool_name` | `string` | Name of the tool that failed |
| `tool_input` | `object` | Parameters that were passed to the tool |
| `tool_use_id` | `string` | Unique identifier for this tool invocation |
| `error` | `string` | Error message |
| `is_interrupt` | `boolean` | Whether the failure was due to an interrupt |

#### Stop

| Field | Type | Description |
|---|---|---|
| `stop_hook_active` | `boolean` | Whether a stop hook is currently active |
| `last_assistant_message` | `string` | The last message the model produced |

#### SessionStart

| Field | Type | Description |
|---|---|---|
| `source` | `string` | One of `"startup"`, `"resume"`, `"clear"`, `"compact"` |
| `agent_type` | `string` | Type of agent starting |
| `model` | `string` | Model being used |

#### Setup

| Field | Type | Description |
|---|---|---|
| `trigger` | `string` | One of `"init"` or `"maintenance"` |

#### SubagentStop

| Field | Type | Description |
|---|---|---|
| `stop_hook_active` | `boolean` | Whether a stop hook is currently active |
| `agent_id` | `string` | Identifier of the sub-agent that stopped |
| `agent_transcript_path` | `string` | Path to the sub-agent's transcript |
| `agent_type` | `string` | Type of the sub-agent |
| `last_assistant_message` | `string` | The last message the sub-agent produced |

#### PermissionRequest

| Field | Type | Description |
|---|---|---|
| `tool_name` | `string` | Tool requesting permission |
| `tool_input` | `object` | Parameters the tool wants to use |
| `permission_suggestions` | `array` | Suggested permission rules |

#### PermissionDenied

| Field | Type | Description |
|---|---|---|
| `tool_name` | `string` | Tool whose permission was denied |
| `tool_input` | `object` | Parameters the tool wanted to use |
| `tool_use_id` | `string` | Unique identifier for the tool invocation |
| `reason` | `string` | Reason the permission was denied |

---

## Hook Output Schema

Hook output is expected as JSON on stdout (for `command` hooks) or in the HTTP
response body (for `http` hooks). For `prompt` and `agent` hooks, the system parses
the model's response into the same schema.

### Top-Level Output Fields

| Field | Type | Description |
|---|---|---|
| `continue` | `boolean` | Whether to continue normal execution |
| `suppressOutput` | `boolean` | Whether to suppress output from this hook |
| `stopReason` | `string` | If set, stop execution with this reason |
| `decision` | `"approve"` \| `"block"` | High-level approval decision |
| `reason` | `string` | Human-readable explanation of the decision |
| `systemMessage` | `string` | Message to inject into the system context |
| `hookSpecificOutput` | `object` | Event-specific output fields (see below) |

### Async Output

When a hook needs to signal that it is running asynchronously:

```json
{
  "async": true,
  "asyncTimeout": 300
}
```

### Exit Code 2

For `command` hooks, exit code **2** signals a blocking error. The hook is treated
as having produced an error that should halt the current operation. When used with
`asyncRewake`, exit code 2 wakes the model to process the hook's output.

---

## Hook-Specific Output Variants

The `hookSpecificOutput` field varies by event type.

### PreToolUse

| Field | Type | Description |
|---|---|---|
| `permissionDecision` | `"allow"` \| `"deny"` \| `"ask"` | Permission verdict for this tool call |
| `permissionDecisionReason` | `string` | Explanation for the decision |
| `updatedInput` | `object` | Modified tool parameters (replaces original input) |
| `additionalContext` | `string` | Extra context injected into the conversation |

### PostToolUse

| Field | Type | Description |
|---|---|---|
| `additionalContext` | `string` | Extra context injected after tool execution |
| `updatedMCPToolOutput` | `any` | Replacement output for MCP tool results |

### SessionStart

| Field | Type | Description |
|---|---|---|
| `additionalContext` | `string` | Context injected at session start |
| `initialUserMessage` | `string` | Override the initial user message |
| `watchPaths` | `string[]` | File paths to watch for changes |

### PermissionRequest

The hook can respond with either an approval (with optional modifications) or a denial:

**Approval:**

| Field | Type | Description |
|---|---|---|
| `decision` | `"approve"` | Approve the permission request |
| `updatedInput` | `object` | Modified tool parameters |
| `updatedPermissions` | `array` | Modified permission rules |

**Denial:**

| Field | Type | Description |
|---|---|---|
| `decision` | `"deny"` | Deny the permission request |
| `reason` | `string` | Reason for denial |

### PermissionDenied

| Field | Type | Description |
|---|---|---|
| `retry` | `boolean` | If `true`, retry the permission request |

### Elicitation / ElicitationResult

| Field | Type | Description |
|---|---|---|
| `action` | `"accept"` \| `"decline"` \| `"cancel"` | How to handle the elicitation |
| `content` | `any` | Content for the elicitation response |

---

## Hook Execution Flow

1. **Collection** -- Hooks are gathered from three sources:
   - **Settings snapshot** -- hooks defined in `settings.json`
   - **Registered hooks** -- hooks registered via the SDK or plugin system
   - **Session hooks** -- hooks attached at runtime by agents or skills

2. **Event matching** -- Each hook is matched against the current event name.
   Pattern matching supports:
   - Exact match (e.g., `"PreToolUse"`)
   - Pipe-separated alternatives (e.g., `"PreToolUse|PostToolUse"`)
   - Regular expressions

3. **Condition filtering** -- If a hook has an `if` field, the condition is
   evaluated using permission rule syntax. The hook only runs if the condition
   matches. For example, `"if": "Bash(git *)"` restricts a `PreToolUse` hook to
   only fire when the `Bash` tool is invoked with a command starting with `git`.

4. **Parallel execution** -- All matched hooks run **in parallel**, each with its
   own timeout. There is no guaranteed ordering between hooks for the same event.

5. **Result merging** -- Results from all hooks are collected. Blocking decisions
   (`deny`) take priority. If any hook denies, the operation is denied regardless
   of other hooks' decisions.

6. **Security gate** -- In interactive mode, all hooks require **workspace trust**.
   Hooks from untrusted workspaces will not execute. This prevents malicious
   repositories from injecting hooks via checked-in configuration.

---

## Hook Permission Resolution

When a `PreToolUse` hook returns a `permissionDecision`, the following rules apply:

| Hook decision | Effect |
|---|---|
| `"allow"` | Grants permission for this specific invocation, but does **NOT** override `deny` or `ask` rules in `settings.json`. If `settings.json` denies the tool, the denial wins. |
| `"deny"` | **Final.** The tool call is blocked. No other hook or setting can override a deny. |
| `"ask"` | Forces the interactive permission dialog, even if `settings.json` would otherwise allow it. |

Key rules:

- A hook `allow` is a "soft allow" -- it can be overruled by settings-level deny/ask rules.
- A hook `deny` is a "hard deny" -- it is final and cannot be overruled.
- A hook `ask` escalates to the user -- it forces manual confirmation.
- When a hook provides `updatedInput`, the modified parameters replace the original
  tool input for the remainder of the execution (including subsequent hooks and the
  actual tool call).

---

## Configuration Examples

### Basic Command Hook (settings.json)

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "type": "command",
        "command": "/path/to/validator.sh",
        "if": "Bash(rm *)",
        "timeout": 10,
        "statusMessage": "Checking destructive command..."
      }
    ]
  }
}
```

### Prompt Hook for Code Review

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "type": "prompt",
        "prompt": "Review the following tool call for safety. Input: $ARGUMENTS. Respond with JSON containing a 'decision' field of 'approve' or 'block' and a 'reason' field.",
        "if": "Edit(*)",
        "timeout": 30
      }
    ]
  }
}
```

### HTTP Webhook on Session Events

```json
{
  "hooks": {
    "SessionStart": [
      {
        "type": "http",
        "url": "https://hooks.example.com/claude/session-start",
        "headers": {
          "Authorization": "Bearer ${API_TOKEN}"
        },
        "allowedEnvVars": ["API_TOKEN"],
        "timeout": 15
      }
    ]
  }
}
```

### Async Background Hook with Rewake

```json
{
  "hooks": {
    "Stop": [
      {
        "type": "command",
        "command": "/path/to/auto-continue.sh",
        "async": true,
        "asyncRewake": true,
        "timeout": 300,
        "statusMessage": "Checking if more work is needed..."
      }
    ]
  }
}
```

### Agent Hook for Complex Validation

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "type": "agent",
        "prompt": "You are a security reviewer. Analyze this tool call and decide if it should proceed: $ARGUMENTS",
        "if": "Bash(*)",
        "timeout": 60
      }
    ]
  }
}
```

### Multiple Hooks on the Same Event

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "type": "command",
        "command": "/path/to/audit-logger.sh",
        "async": true
      },
      {
        "type": "command",
        "command": "/path/to/policy-check.sh",
        "if": "Bash(curl *)|Bash(wget *)",
        "timeout": 5
      }
    ]
  }
}
```

### One-Time Setup Hook

```json
{
  "hooks": {
    "SessionStart": [
      {
        "type": "command",
        "command": "/path/to/first-run-setup.sh",
        "once": true,
        "statusMessage": "Running first-time setup..."
      }
    ]
  }
}
```
