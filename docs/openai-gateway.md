# OpenAI-compatible agent gateway

`unleash serve` exposes one stateful, Unleash-managed agent instance through
the OpenAI Chat Completions protocol.

The default path is headful: Unleash launches the selected profile in a
pseudo-terminal, keeps its terminal UI visible, submits API turns to that same
process, and incrementally parses the harness's persisted native history. The
terminal and the API are two interfaces to one conversation.

## Start a headful instance

```bash
unleash serve claude \
  --name work-auth \
  --model claude-opus-4-6 \
  --port 8787
```

Unleash binds to `127.0.0.1` by default and prints the instance's model ID:

```text
unleash/work-auth/claude-opus-4-6/claude-code
```

Use that exact ID with an OpenAI client:

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -H 'Idempotency-Key: turn-001' \
  -d '{
    "model": "unleash/work-auth/claude-opus-4-6/claude-code",
    "messages": [{"role": "user", "content": "Inspect the failing tests"}],
    "stream": true
  }'
```

Or discover the current ID first:

```bash
curl http://127.0.0.1:8787/v1/models
```

`/v1/models` lists live, serveable Unleash instances rather than installed
binaries. Each ID combines the instance/session name, underlying model slug,
and harness:

```text
unleash/<instance-name>/<model-slug>/<harness>
```

The model object also includes `x-unleash` metadata with the native session ID,
unslugged component values, harness, and execution mode. A new session whose
underlying model is not configured starts with `default` and updates after the
first native history record identifies the actual model.

To resume an existing native conversation:

```bash
unleash serve codex \
  --name release-fix \
  --session codex:019abc123 \
  --model gpt-5.6
```

## Conversation semantics

The native harness history is the source of truth. An OpenAI client commonly
sends the full transcript in every request, but replaying that transcript into
an already-stateful CLI would duplicate it. The gateway therefore requires the
last `messages` entry to be a new `user` turn, submits only that text, and
treats earlier entries as client-side context.

Every Chat Completions request must include a non-empty `Idempotency-Key`
header. Retrying the same model and user turn with the same key attaches to the
in-flight native turn or replays its completed response; it never submits the
prompt again. Reusing a key with different input returns HTTP `409` with code
`idempotency_key_conflict`. Keep keys unique per intended native turn. The
gateway retains completed keys and responses for the lifetime of the server.

One instance accepts one API turn at a time. A concurrent request receives
HTTP `409` with code `instance_busy`. While an API turn owns a headful
instance, local terminal input remains buffered and is forwarded after the API
turn reaches a native completion boundary.

Disconnecting an HTTP client does not cancel the native turn: the agent keeps
running to a native completion boundary and caches the outcome so a retry with
the same `Idempotency-Key` is replay-safe. There is no cancellation endpoint in
v1. If `--turn-timeout-secs` expires, Unleash kills the per-turn headless
process or the attached headful instance instead of accepting another prompt
into an indeterminate conversation.

When the terminal submits a local turn, API injection is rejected with HTTP
`409` and code `native_instance_busy` until the persisted native history proves
that local turn crossed a completion boundary. This prevents a local answer
from being attributed to an overlapping API request.

Agent-owned tools remain agent-owned. Their activity is parsed for lifecycle
and completion tracking, but the gateway does not emit a client-owned
`tool_calls` stop or accept `role: "tool"` results. Requests with a non-empty
`tools` array fail explicitly.

Current content support is text-only. Image/audio content parts, client-owned
function calls, `n` values other than `1`, and arbitrary tool-result injection
are rejected rather than silently misrepresented.

## Streaming and non-streaming responses

`stream: true` returns `text/event-stream` data using
`chat.completion.chunk` objects and a final `[DONE]`. When
`stream_options.include_usage` is true, Unleash emits the OpenAI-style final
usage chunk with an empty `choices` array.

The history projection emits completed native assistant text blocks as they
are appended. It does not fabricate token deltas when a headful harness only
persists message-sized blocks.

Without `stream`, Unleash buffers the same projected turn into one
`chat.completion` response.

## Secondary headless mode

```bash
unleash serve codex \
  --headless \
  --name ci-agent \
  --model gpt-5.6
```

Headless mode starts one process per API turn and resumes the native session ID
on subsequent turns. It shares model discovery, history conversion, request
validation, response schemas, authentication, and concurrency behavior with
the headful mode. Because the process must exit before its final history can be
collected, this fallback does not provide live message-sized streaming.

## Permissions and network exposure

Gateway launches use Unleash safe mode by default. The visible headful terminal
can still present native approval requests. `--unsafe` restores Unleash's
normal permission bypass and should be used only in an externally isolated
workspace:

```bash
unleash serve claude --unsafe
```

Loopback access can also require a bearer token:

```bash
UNLEASH_API_KEY='replace-me' unleash serve claude
curl http://127.0.0.1:8787/v1/models \
  -H 'Authorization: Bearer replace-me'
```

A non-loopback bind is rejected unless both `--allow-remote` and an API key are
present:

```bash
UNLEASH_API_KEY='replace-me' \
  unleash serve claude --host 0.0.0.0 --allow-remote
```

The built-in server is HTTP, not TLS. Put a TLS-authenticating reverse proxy in
front of it before crossing an untrusted network. Anyone who can call this API
can submit instructions to a code-executing agent in the served workspace.

## Harness compatibility

The native history projection uses the existing UCF converters:

| Harness | Headful history | Explicit completion boundary |
|---------|-----------------|------------------------------|
| Claude Code | JSONL | assistant `stop_reason` |
| Codex / Clanker | rollout JSONL | `event_msg/task_complete` |
| Gemini / Antigravity | session JSON | stable-history fallback |
| OpenCode | SQLite | stable-history fallback |
| Pi | JSONL | stable-history fallback |
| Hermes | SQLite | stable-history fallback |

Claude Code and Codex/Clanker have the strongest v1 completion guarantees.
Other built-in harnesses use the same lossless history parsers but fall back to
a quiet period after assistant text because their persisted formats do not
expose one uniform terminal event.

Codex compatibility covers both persisted rollout events and the live
`codex exec --json` contract. The live adapter accepts the current
`item.type: "agent_message"` shape and the earlier
`item.item_type: "assistant_message"` shape, maps current MCP result content,
command decline/failure status, todo/collaboration events, and preserves
unknown future events as passthrough data.
