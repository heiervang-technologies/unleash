# QA Test Plan: Chat Log Portability

**Feature:** Convert conversation logs between Claude Code, Codex, Gemini CLI, and OpenCode via Hub interchange format
**Date:** 2026-03-29
**QA Owner:** unleash-qa

---

## Scope

Test the `unleash convert` command and underlying interchange library for:
1. Lossless round-trip conversion (CLI -> Hub -> same CLI)
2. Cross-CLI conversion (CLI A -> Hub -> CLI B)
3. Error handling and edge cases
4. CLI command interface

## Test Fixtures

Pre-collected sanitized conversation samples in `src/interchange/tests/fixtures/`:

| Fixture | Source | Content |
|---------|--------|---------|
| `claude-sample.jsonl` | Claude Code | 13 messages: user, assistant+thinking+tool_use, tool_result |
| `codex-sample.jsonl` | Codex CLI | 45 events: session_meta, turn_context, user, assistant+tool, token_count |
| `gemini-sample.json` | Gemini CLI | 6 messages: user, gemini+toolCalls+thoughts, info |
| `opencode-messages.json` | OpenCode | 10 messages: user, assistant with cost/tokens |
| `opencode-parts.json` | OpenCode | 32 parts: tool, reasoning, text, step-start, step-finish |

---

## Test Suite 1: Round-Trip Lossless (Critical)

For each CLI, convert to Hub format and back. Assert semantic equality.

### RT-1: Claude Code Round-Trip

| ID | Test Case | Input | Expected |
|----|-----------|-------|----------|
| RT-1.1 | Basic round-trip | claude-sample.jsonl | Semantic equality after Hub -> Claude |
| RT-1.2 | Thinking blocks preserved | Message with `thinking` + `signature` | signature, text, type all intact |
| RT-1.3 | Tool use/result correlation | `tool_use.id` <-> `tool_result.tool_use_id` | IDs match after round-trip |
| RT-1.4 | API message ID preserved | `id: "msg_01ABC..."` on assistant | `api_message_id` survives round-trip |
| RT-1.5 | Cache token split preserved | `cache_creation_input_tokens` + `cache_read_input_tokens` | Both values intact, not collapsed |
| RT-1.6 | Compound tool result | tool_result with content array (text + image) | Array structure preserved |
| RT-1.7 | Sidechain flag | `isSidechain: true` message | Flag survives via extensions |
| RT-1.8 | Universal fields | uuid, parentUuid, timestamp, sessionId, version, cwd, gitBranch | All preserved |
| RT-1.9 | File history snapshot | `file-history-snapshot` event | Content preserved as Hub event |
| RT-1.10 | Metadata types | pr-link, custom-title, agent-name, agent-color | Preserved as Hub events |

### RT-2: Codex Round-Trip

| ID | Test Case | Input | Expected |
|----|-----------|-------|----------|
| RT-2.1 | Basic round-trip | codex-sample.jsonl | Semantic equality after Hub -> Codex |
| RT-2.2 | Session meta preserved | `session_meta` event | All fields reconstructed |
| RT-2.3 | Turn context preserved | `turn_context` events | approval_policy, sandbox_policy, effort all intact |
| RT-2.4 | Cumulative token handling | Multiple `token_count` events | Hub stores deltas, reconverts to cumulative totals |
| RT-2.5 | Function call correlation | `function_call` <-> `function_call_output` | call IDs match |
| RT-2.6 | Task lifecycle | `task_started` + `task_complete` | Both events preserved |
| RT-2.7 | Reasoning content | `event_msg` with type `reasoning` | Text content preserved |
| RT-2.8 | Item IDs preserved | `response_item.payload.id` | Survives via extensions |

### RT-3: Gemini CLI Round-Trip

| ID | Test Case | Input | Expected |
|----|-----------|-------|----------|
| RT-3.1 | Basic round-trip | gemini-sample.json | Semantic equality after Hub -> Gemini JSON |
| RT-3.2 | Thoughts preserved | `thoughts` array with subject + description | Both fields + per-thought timestamps intact |
| RT-3.3 | Tool calls with metadata | `toolCalls` with displayName, description, status | All fields preserved |
| RT-3.4 | Token breakdown | tokens.input, .output, .cached, .thoughts, .tool | All 5 fields preserved |
| RT-3.5 | Project hash preserved | `projectHash` (SHA-256) | Survives via session extensions |
| RT-3.6 | Multimodal content | `inlineData` with base64 image | Image data + mimeType preserved |
| RT-3.7 | Info messages | type: "info" messages | Mapped and reconstructed correctly |
| RT-3.8 | Session metadata | sessionId, startTime, lastUpdated | All timestamps preserved |

### RT-4: OpenCode Round-Trip

| ID | Test Case | Input | Expected |
|----|-----------|-------|----------|
| RT-4.1 | Basic round-trip | opencode-messages.json + opencode-parts.json | Semantic equality after Hub -> OpenCode |
| RT-4.2 | Dual timestamps | `time.created` + `time.completed` | Both preserved (timestamp + completed_at) |
| RT-4.3 | Step boundaries | `step-start` + `step-finish` parts | Preserved as step_boundary content blocks |
| RT-4.4 | Patch parts | `patch` with `hash.before` + `hash.after` | Both hashes preserved per file |
| RT-4.5 | Reasoning encryption | `reasoning` part with encrypted data | encrypted_data + encryption_format preserved |
| RT-4.6 | Cost tracking | `cost` field per message | Preserved in metadata |
| RT-4.7 | Session hierarchy | `parent_id` on session | Preserved as parent_session_id |
| RT-4.8 | Session slug | Two-word slug (e.g. "hidden-wolf") | Preserved in session header |
| RT-4.9 | Tool metadata | state.title, state.metadata.truncated | Both fields preserved |
| RT-4.10 | Part types | All 6 part types (text, step-start, step-finish, reasoning, tool, patch) | All survive round-trip |

---

## Test Suite 2: Cross-CLI Conversion

Convert between all 12 CLI pairs. Verify portable fields are preserved and CLI-specific fields are handled gracefully.

### Portable Fields (must survive all cross-CLI conversions)

| Field | Verification |
|-------|-------------|
| User/assistant roles | role field matches |
| Text content | Exact string match |
| Tool call name + arguments | name and input object match |
| Tool result output | Output text matches |
| Timestamps | ISO 8601 values match |
| Session ID | UUID preserved |
| Token usage (input + output) | Values match |
| Model name | String match |

### Cross-CLI Matrix

| ID | Source | Target | Key Verification |
|----|--------|--------|-----------------|
| CC-1 | Claude | Codex | tool_use maps to function_call, thinking maps to reasoning |
| CC-2 | Claude | Gemini | thinking maps to thoughts, tool_result maps to toolCalls[].result |
| CC-3 | Claude | OpenCode | thinking maps to reasoning part, tool maps to tool part |
| CC-4 | Codex | Claude | function_call maps to tool_use, reasoning maps to thinking |
| CC-5 | Codex | Gemini | function_call maps to toolCalls |
| CC-6 | Codex | OpenCode | function_call maps to tool part |
| CC-7 | Gemini | Claude | thoughts maps to thinking, toolCalls maps to tool_use + tool_result |
| CC-8 | Gemini | Codex | thoughts maps to reasoning, toolCalls maps to function_call |
| CC-9 | Gemini | OpenCode | thoughts maps to reasoning part, toolCalls maps to tool part |
| CC-10 | OpenCode | Claude | reasoning part maps to thinking, tool part maps to tool_use |
| CC-11 | OpenCode | Codex | reasoning part maps to reasoning, tool part maps to function_call |
| CC-12 | OpenCode | Gemini | reasoning part maps to thoughts, tool part maps to toolCalls |

### Conversion metadata

For each cross-CLI test, verify:
- `_conversion` extension is present with source_cli, converted_at, approximated_fields
- CLI-specific fields NOT in target format are listed in approximated_fields
- Target CLI required fields have sensible defaults

---

## Test Suite 3: Edge Cases

| ID | Test Case | Input | Expected |
|----|-----------|-------|----------|
| EC-1 | Empty session | Session with 0 messages | Valid Hub file with header only; round-trips correctly |
| EC-2 | Single message | One user message, no response | Valid conversion |
| EC-3 | Very large session | 100K+ lines JSONL | Completes without OOM; streaming processing |
| EC-4 | Interrupted session | JSONL with truncated last line | Graceful skip of bad line, rest converts |
| EC-5 | Base64 images | Assistant message with multiple screenshot images | Images preserved, no corruption |
| EC-6 | Encrypted reasoning | OpenCode reasoning with encrypted_data | Data preserved as opaque blob |
| EC-7 | Sidechain messages | Claude isSidechain=true messages | Preserved in extensions; cross-CLI drops gracefully |
| EC-8 | Unicode content | Messages with emoji, CJK, RTL text | No encoding corruption |
| EC-9 | Null vs missing | Fields that are null vs absent | Treated as equivalent per semantic equality |
| EC-10 | Deep nesting | Tool input with deeply nested JSON objects | Preserved exactly |
| EC-11 | Empty arrays | Messages with empty thoughts[], toolCalls[], content | Arrays preserved as empty |
| EC-12 | Multiple tool calls | Assistant message with 5+ concurrent tool calls | All calls + results preserved and correlated |

---

## Test Suite 4: Negative Tests

| ID | Test Case | Input | Expected |
|----|-----------|-------|----------|
| NEG-1 | Missing extensions | Hub file with no extensions object | Convert succeeds with defaults |
| NEG-2 | Unknown content block | Hub file with type: "custom_block" | Preserved as-is or warned, not crashed |
| NEG-3 | Unknown event type | Hub file with event_type: "future_event" | Preserved or warned |
| NEG-4 | Corrupt JSON line | JSONL with invalid JSON on line 5 | Lines 1-4 and 6+ convert; line 5 skipped with warning |
| NEG-5 | Wrong CLI format | `--from claude` with Codex JSONL | Clear error message, not crash |
| NEG-6 | Future UCF version | Hub file with ucf_version: "2.0.0" | Refuse with version mismatch error |
| NEG-7 | Missing session header | Hub file starting with message record | Error: missing session header |
| NEG-8 | Empty file | 0 bytes | Error: empty input |
| NEG-9 | Binary file | Random binary data | Error: not valid JSON |
| NEG-10 | Permission denied | Read-only output path | Error with clear message |

---

## Test Suite 5: CLI Interface

| ID | Test Case | Command | Expected |
|----|-----------|---------|----------|
| CLI-1 | Basic convert | `unleash convert --from claude input.jsonl -o output.ucf.jsonl` | Creates valid Hub file |
| CLI-2 | Cross-CLI shorthand | `unleash convert --from claude input.jsonl --to codex -o output.jsonl` | Creates valid Codex file |
| CLI-3 | Verify command | `unleash convert --verify input.jsonl --format claude` | Exit 0 if lossless, exit 1 if not |
| CLI-4 | Stdout output | `unleash convert --from claude input.jsonl` (no -o) | Output to stdout |
| CLI-5 | Missing --from | `unleash convert input.jsonl` | Error: --from required |
| CLI-6 | Invalid format | `unleash convert --from invalid input.jsonl` | Error: unknown format |
| CLI-7 | Help text | `unleash convert --help` | Shows usage with all options |

---

## Acceptance Criteria

1. All RT-* tests pass (4 CLI round-trips, 36+ field-specific checks)
2. All CC-* tests pass (12 cross-CLI pairs, portable fields verified)
3. All EC-* tests pass (12 edge cases)
4. All NEG-* tests pass (10 negative tests)
5. All CLI-* tests pass (7 interface tests)
6. No test takes longer than 30 seconds (except EC-3 large session)
7. Test coverage > 90% on `src/interchange/` modules

## Test Execution

```bash
# Run all interchange tests
cargo test --test round_trip --test cross_cli --test edge_cases --test negative

# Run verify on each fixture
for fmt in claude codex gemini opencode; do
  unleash convert --verify "src/interchange/tests/fixtures/${fmt}-sample.*" --format $fmt
done
```
