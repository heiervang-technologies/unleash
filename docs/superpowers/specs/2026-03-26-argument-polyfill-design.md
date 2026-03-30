# Design Spec: Unified Argument Polyfill Layer (Issue 210)
**Date:** 2026-03-26  
**Status:** Approved (Markus) - **PIVOT: Profile-first (Option C)**  
**Authors:** Gemini CLI, Claude Opus (Repo Manager)

## Objective
Normalize common flags and session management across all four supported code agents (Claude, Codex, Gemini, OpenCode). This layer acts as a "polyfill" to bridge the semantic gaps between different agent CLIs before the double-dash (`--`) passthrough boundary.

## 1. Profile-First Architecture (Option C)
To ensure consistency, **all command launches** (including built-in agent names and custom profiles) go through the same polyfill logic.

### Invocation Flow
1.  **Profile Lookup:** unleash resolves the profile name (e.g., `unleash claude` or `unleash work`).
2.  **Agent Detection:** The profile's `agent_cli_path` determines the `AgentType`.
3.  **Polyfill Execution:** Unified flags are parsed and resolved into agent-specific flags based on the `AgentDefinition`.
4.  **Deduplication:** The polyfill ensures flags present in both the profile's `agent_args` and the CLI are not duplicated.
5.  **Execution:** The final `ResolvedInvocation` is passed to the launcher.

### Argument Layers
Arguments BEFORE `--` are unified flags handled by unleash.  
Arguments AFTER `--` are passed directly to the agent CLI (escape hatch).

## 2. Unified Session Semantics
Standard set of session-related flags to `unleash` that map to specific agent commands.

| Unified Flag | Meaning |
|---|---|
| `--continue` / `-c` | Resumes the most recent conversation in the current directory. |
| `--resume <ID>` / `-r <ID>` | Resumes a specific conversation by ID (or index, depending on agent). |

### Agent Mappings (Data-Driven)
Mappings are stored in `AgentPolyfillConfig` in `src/agents.rs`.
| Agent | Continue (`--continue` / `-c`) | Resume (`--resume <ID>` / `-r <ID>`) |
|---|---|---|
| **Claude** | `--continue` | `--resume <ID>` |
| **Codex** | `resume --last` | `resume <ID>` |
| **Gemini** | `--resume latest` | `--resume <ID>` |
| **OpenCode** | `--continue` | `--session <ID>` |

## 3. Headless Mode
| Unified Flag | Meaning |
|---|---|
| `--headless` / `-p` | Runs the agent in headless/prompt mode. |

### Agent Mappings
| Agent | Mapping Type | Command / Flag |
|---|---|---|
| **Claude** | Flag | `-p` |
| **Gemini** | Flag | `-p` |
| **Codex** | Subcommand | `exec` |
| **OpenCode** | Subcommand | `run` |

## 4. Session Forking
| Unified Flag | Meaning |
|---|---|
| `--fork` | Forks the current or specified session. |

### Agent Mappings
| Agent | Mapping |
|---|---|
| **Claude** | `--fork-session` |
| **Codex** | `fork` (subcommand) |
| **OpenCode** | `--fork` |
| **Gemini** | *Not Supported* (Warning only) |

## 5. Safety Mode & YOLO
unleash defaults to "YOLO mode" (e.g., injecting permission bypasses).

| Unified Flag | Meaning |
|---|---|
| `--yolo` | Hidden flag, no-op (legacy support). |
| `--safe` | **Primary Opt-out.** Disables YOLO-mode injections. |

### Agent YOLO Mappings
| Agent | YOLO Flag |
|---|---|
| **Claude** | `--dangerously-skip-permissions` |
| **Codex** | `--dangerously-bypass-approvals-and-sandbox` |
| **Gemini** | `--yolo` |
| **OpenCode** | (None) |

## 6. Profile Overrides & Precedence
Profiles support `[agent.TYPE]` blocks for per-agent overrides in TOML.

### Argument Precedence
1. **CLI Explicit Flags** (Highest)
2. **Profile Agent Overrides** (`[agent.codex]` in TOML)
3. **Profile General Args** (`agent_args` in TOML)
4. **unleash Defaults** (YOLO on) - Lowest

## 7. Architecture: `polyfill.rs`
Provides a data-driven engine that consumes `AgentPolyfillConfig`.

### `ResolvedInvocation` Struct
```rust
pub struct ResolvedInvocation {
    pub binary: PathBuf,      // May include subcommand prefix (e.g., 'codex exec')
    pub args: Vec<String>,    // Normalized/mapped arguments
    pub env: HashMap<String, String>,
    pub auto_mode: bool,      // Injected from --auto flag
}
```
