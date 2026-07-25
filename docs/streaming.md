# Live stream normalization

`unleash stream` converts harness-specific JSONL into one canonical live-event
interface. Consumers switch on `type`; they never need to switch on Claude
versus Codex frame names.

## Usage

```bash
claude -p "fix the tests" --output-format stream-json --verbose \
  | unleash stream --harness claude-code

codex exec --json "fix the tests" \
  | unleash stream --harness codex --headless

# A saved capture works too.
unleash stream --harness claude-code capture.jsonl
```

Input defaults to stdin (`-`). Output is canonical JSONL.

## Event contract

Every output record has:

- `session_id`: the latest session identity announced by the harness;
- `type`: the canonical discriminator;
- `data`: the event payload.

The discriminators are:

| `type` | `data` |
|---|---|
| `session_start` | UCF `SessionHeader` |
| `message` | completed UCF `HubMessage` |
| `delta` | `{kind, text, cumulative}` |
| `event` | UCF `HubEvent` lifecycle/status data |
| `interaction_request` | canonical human-input request |
| `passthrough` | an unrecognized native frame preserved verbatim |

For example:

```json
{"session_id":"abc","type":"interaction_request","data":{"id":"call-1:0","session_id":"abc","tool_call_id":"call-1","method":"select","title":"Runtime","message":"Which runtime?","options":["Tokio","smol"],"custom":true,"multiple":false,"raw":{"questions":[]}}}
```

Unknown frames and non-JSON lines become `passthrough`; they are never dropped.
Claude `message_delta` and `content_block_start` frames are ordinary `event`
records because they uniquely carry per-turn usage/stop data and early tool
identity. Non-init Claude system subtypes use namespaced event types such as
`system:hook_started`.

Pure framing delimiters (`message_start`, `message_stop`,
`content_block_stop`, and `ping`) may emit nothing because their completed
frames repeat all information.

## Human-input routing

Claude `AskUserQuestion` and `ExitPlanMode` tool calls produce two outputs:

1. the ordinary UCF tool-use `message`, preserving the transcript;
2. a canonical `interaction_request` for an attended UI.

The runtime records the tool-call id to session-id relationship before deciding
whether to forward the UI request.

- Default attended mode forwards both records.
- `--headless` suppresses only `interaction_request`.

The completed tool message, unknown native fields, and attribution bookkeeping
are identical in both modes.

## Codex upstream limitation

The Codex adapter normalizes `codex exec --json`. That execution surface
currently rejects the built-in `request_user_input` operation rather than
emitting a JSON event for it. Unleash cannot manufacture a question that the
source process never exposes.

Supported behavior is therefore explicit:

- all events Codex does emit remain normalized or passed through verbatim;
- no Codex human-input capability is advertised for `exec --json`;
- Claude question/plan calls use the canonical interaction channel;
- reopen Codex interaction work when `codex exec --json` exposes a
  request-and-response event contract.

This is an upstream ownership boundary, not a silent partial implementation.

## Rust API

Rust consumers can use `stream::parser_for` directly. Consumers that need
session envelopes, attended/headless routing, and tool attribution should pass
adapter output through `StreamRuntime`. `normalize_reader` is the embeddable
equivalent of the CLI filter.
