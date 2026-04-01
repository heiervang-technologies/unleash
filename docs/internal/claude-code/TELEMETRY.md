# Claude Code Telemetry Systems

Reference documentation for all telemetry, analytics, and non-inference network traffic in Claude Code.

Last verified: 2026-03-31, verified against source.

---

## Table of Contents

- [Telemetry Systems](#telemetry-systems)
  - [Datadog Event Logging](#1-datadog-event-logging)
  - [First-Party Event Logging (1P)](#2-first-party-event-logging-1p)
  - [GrowthBook Feature Flags](#3-growthbook-feature-flags)
  - [BigQuery Metrics (OTEL)](#4-bigquery-metrics-otel)
  - [Customer OTEL Export](#5-customer-otel-export)
  - [Transcript Sharing](#6-transcript-sharing)
- [Non-Inference Endpoints](#non-inference-endpoints)
- [Environment Variables](#environment-variables)
- [Privacy Controls](#privacy-controls)
- [Failed Event Retry Queue](#failed-event-retry-queue)
- [Unleash Telemetry Blocking](#unleash-telemetry-blocking)

---

## Telemetry Systems

### 1. Datadog Event Logging

**Endpoint:** `https://http-intake.logs.us5.datadoghq.com/api/v2/logs`

Datadog browser-style logging using a public client token (standard practice for Datadog browser SDK). Events are sent from an explicit allowlist of approximately 35-40 event names.

**Key characteristics:**

- Events are batched and flushed every 15 seconds
- Metadata is restricted to boolean and number values only -- the type system explicitly forbids string values to prevent accidental logging of code, filepaths, or PII
- MCP tool names are normalized to the generic string `"mcp"` (individual tool names are never sent)
- Automatically disabled when:
  - `NODE_ENV=test`
  - Using a third-party provider (Bedrock, Vertex, Foundry)
  - Telemetry is explicitly disabled via environment variable

### 2. First-Party Event Logging (1P)

**Endpoint:** `https://api.anthropic.com/api/event_logging/batch`

Uses OpenTelemetry's `BatchLogRecordProcessor` to send `ClaudeCodeInternalEvent` protobuf messages to Anthropic's own backend.

**Key characteristics:**

- Batched with a 10-second flush interval and 200-event batch size
- Failed events are persisted to `~/.claude/telemetry/` as JSON files for retry on the next session launch
- This retry queue can grow to hundreds of MB if events consistently fail to send (see [Failed Event Retry Queue](#failed-event-retry-queue))
- Uses authenticated requests tied to the user's session

### 3. GrowthBook Feature Flags

**Endpoint:** `https://api.anthropic.com/` (proxied)

GrowthBook SDK with remote evaluation -- user attributes are sent to the server, which evaluates feature flags and returns results.

**Key characteristics:**

- Three SDK client keys are configured: external, Anthropic production, and Anthropic development
- Results are cached to disk for offline use
- Used for A/B experiments, feature gating, dynamic configuration, and killswitches
- One such killswitch controls the analytics sink itself

### 4. BigQuery Metrics (OTEL)

**Endpoint:** `https://api.anthropic.com/api/claude_code/metrics`

OpenTelemetry metric data points exported to Anthropic's metrics pipeline (backed by BigQuery).

**Key characteristics:**

- Gated by an organization-level opt-out setting
- Standard OTEL metric format

### 5. Customer OTEL Export

**Endpoint:** User-configured via `OTEL_EXPORTER_OTLP_ENDPOINT`

Enterprise customers can configure Claude Code to export OpenTelemetry data to their own collectors. This is the intended mechanism for organizations that want visibility into Claude Code usage.

### 6. Transcript Sharing

**Endpoint:** `https://api.anthropic.com/api/claude_code_shared_session_transcripts`

Conversation transcripts can be sent to Anthropic under specific conditions.

**Key characteristics:**

- Triggered by feedback surveys (thumbs up/down) and frustration detection heuristics
- A redaction pass is applied before transmission
- Uses authenticated requests

---

## Non-Inference Endpoints

All network traffic Claude Code makes beyond model inference, categorized by function.

### Authentication and OAuth

| Endpoint | Purpose |
|---|---|
| `platform.claude.com` | OAuth flow for account authentication |
| `claude.com` | OAuth flow for account authentication |

### Auto-Update

| Endpoint | Purpose |
|---|---|
| GCS bucket (`storage.googleapis.com`) | Native binary update checks and downloads |
| npm registry (`registry.npmjs.org`) | npm-based update checks and downloads |

### Organization APIs

| Endpoint | Purpose |
|---|---|
| `api.anthropic.com/.../settings` | Organization settings and preferences |
| `api.anthropic.com/.../team_memory` | Shared team memory / context |
| `api.anthropic.com/.../policy_limits` | Usage policy and rate limits |
| `api.anthropic.com/.../grove` | Key-value storage API |

### MCP Infrastructure

| Endpoint | Purpose |
|---|---|
| `mcp-proxy.anthropic.com` | MCP proxy for remote tool servers |
| MCP Registry endpoints | Discovery and registration of MCP servers |

### Miscellaneous

| Endpoint | Purpose |
|---|---|
| GitHub raw content (`raw.githubusercontent.com`) | Release notes fetching |

---

## Environment Variables

| Variable | Effect |
|---|---|
| `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` | Nuclear option -- blocks ALL non-inference network traffic (telemetry, updates, feature flags, org APIs, everything) |
| `DISABLE_TELEMETRY` | Blocks analytics subsystems only (Datadog, 1P event logging, feedback/transcripts) |
| `CLAUDE_CODE_USE_BEDROCK` | Third-party provider mode -- auto-disables all analytics |
| `CLAUDE_CODE_USE_VERTEX` | Third-party provider mode -- auto-disables all analytics |
| `CLAUDE_CODE_USE_FOUNDRY` | Third-party provider mode -- auto-disables all analytics |
| `OTEL_METRICS_EXPORTER=none` | Disable OpenTelemetry metrics export |
| `OTEL_LOGS_EXPORTER=none` | Disable OpenTelemetry logs export |
| `OTEL_TRACES_EXPORTER=none` | Disable OpenTelemetry traces export |
| `NODE_ENV=test` | Test environment -- disables all analytics subsystems |

### Recommended Combinations

**Disable all telemetry but keep updates and feature flags:**
```
DISABLE_TELEMETRY=1
```

**Disable all non-inference traffic (full isolation):**
```
CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1
```

**Disable all OTEL exports:**
```
OTEL_METRICS_EXPORTER=none
OTEL_LOGS_EXPORTER=none
OTEL_TRACES_EXPORTER=none
```

---

## Privacy Controls

Claude Code implements several layers of privacy protection in its telemetry systems:

- **Type-level PII prevention:** The `AnalyticsMetadata` type only permits boolean and number values. String values are rejected at compile time, making it structurally impossible to log code snippets, file paths, or free-text user data in analytics events.
- **MCP tool name normalization:** Individual MCP tool names are replaced with the generic string `"mcp"` before being included in any analytics event.
- **Proto field stripping:** Certain protobuf fields are stripped before events reach general-access backends, limiting exposure of sensitive metadata.
- **Organization-level opt-out:** Organizations can disable metrics collection via the API.
- **Sink killswitch:** Analytics sinks can be remotely disabled via GrowthBook feature flags.

---

## Failed Event Retry Queue

**Location:** `~/.claude/telemetry/`

When first-party (1P) event logging fails to deliver events (network errors, server errors, etc.), the failed events are serialized as individual JSON files in the telemetry directory. On the next session launch, Claude Code attempts to re-send these queued events.

**Warning:** This directory can grow to hundreds of megabytes if events consistently fail to deliver, such as when telemetry endpoints are blocked at the network level but `DISABLE_TELEMETRY` is not set. In that scenario, events are generated, fail to send, get persisted to disk, and accumulate indefinitely.

---

## Unleash Telemetry Blocking

Unleash automatically configures telemetry blocking for all agents it launches:

1. **Sets `DISABLE_TELEMETRY=1`** -- disables Datadog, 1P event logging, and feedback/transcript sharing
2. **Sets `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`** -- blocks all remaining non-inference traffic (updates, feature flags, org APIs)
3. **Purges `~/.claude/telemetry/`** on each launch -- prevents the failed event retry queue from accumulating disk space

No additional user configuration is required when running Claude Code through Unleash.
