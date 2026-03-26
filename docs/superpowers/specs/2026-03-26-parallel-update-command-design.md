# Parallel Update Command Design

**Issue:** #220

## Goal

Add `unleash update` command that updates all agent CLIs in parallel with real-time progress bars showing download and build progress. Also wire up the existing cosmetic `AutoUpdateConfig` to actually perform updates.

## Command Interface

```
unleash update              # Update all installed agents in parallel
unleash update claude codex # Update specific agents only
unleash update --check      # Check for updates without installing
unleash update --self       # Also update unleash itself
unleash update --json       # JSON output for scripting
```

## Visualization

uv-style parallel progress bars using ANSI escape codes. Each agent gets its own line that updates in-place.

### Phases

**Phase 1: Check** (parallel version lookups)
```
Checking agents...
  Claude Code    2.1.77 → 2.1.81 (update available)
  Codex          0.115.0 (up to date)
  Gemini CLI     0.35.0 → 0.36.0 (update available)
  OpenCode       1.2.27 (up to date)
```

**Phase 2: Update** (parallel downloads/builds with progress bars)
```
Updating 2 agents...
  Claude Code    ████████████████░░░░░░░░  67% downloading 2.1.81
  Gemini CLI     ██████████████████████░░  92% installing 0.36.0
```

**Phase 3: Summary**
```
  ✓ Claude Code    2.1.77 → 2.1.81 (3.2s)
  ✓ Gemini CLI     0.35.0 → 0.36.0 (5.1s)
  · Codex          0.115.0 (up to date)
  · OpenCode       1.2.27 (up to date)

2 updated, 2 up to date
```

### Progress Tracking Per Agent

Each agent update method has different progress granularity:

| Agent | Method | Progress Source |
|-------|--------|----------------|
| Claude | `claude install <version>` or npm | Parse output lines for download/install phases |
| Codex | git clone + cargo build | Clone progress from git, build progress from cargo (crate count) |
| Gemini | npm install | Parse npm output for progress |
| OpenCode | `opencode upgrade` | Parse output for phases |

For agents where granular progress is hard to get, show a spinner with phase text instead of a percentage bar.

## Architecture

### New Module: `src/updater.rs`

Orchestrates parallel updates with progress reporting.

```rust
pub struct UpdateOrchestrator {
    agents: Vec<AgentType>,
    check_only: bool,
    include_self: bool,
}

pub enum UpdateProgress {
    Checking(AgentType),
    CheckComplete(AgentType, CheckResult),
    Downloading(AgentType, f32),      // 0.0-1.0
    Installing(AgentType, f32),       // 0.0-1.0
    Building(AgentType, String),      // phase description
    Complete(AgentType, UpdateResult),
    Error(AgentType, String),
}

pub struct CheckResult {
    pub installed: Option<String>,
    pub latest: Option<String>,
    pub update_available: bool,
}

pub struct UpdateResult {
    pub from_version: Option<String>,
    pub to_version: String,
    pub duration: Duration,
}
```

### Progress Renderer: `src/progress.rs`

Terminal progress bar renderer using ANSI escape codes. No ratatui dependency.

```rust
pub struct ProgressRenderer {
    lines: Vec<ProgressLine>,
    terminal_width: u16,
}

pub struct ProgressLine {
    agent_name: String,
    state: LineState,
}

pub enum LineState {
    Checking,
    UpToDate(String),
    UpdateAvailable { from: String, to: String },
    Downloading(f32),
    Installing(f32),
    Building(String),
    Complete { from: String, to: String, duration: Duration },
    Error(String),
}
```

Rendering uses:
- `\x1b[{n}A` — cursor up n lines
- `\x1b[2K` — clear line
- `\r` — carriage return
- Unicode block chars for progress bar: `█` and `░`

### Threading Model

- Main thread: renders progress, receives updates via `mpsc::channel`
- One worker thread per agent: runs the update, sends `UpdateProgress` messages
- Render loop: poll channel, redraw changed lines, ~60ms tick

## Auto-Update Integration

### On Launch (`src/lib.rs` / `src/launcher.rs`)

When `unleash <profile>` launches an agent:

1. Read `AutoUpdateConfig` from `config.toml`
2. If auto-update enabled for this agent type:
   a. Check version cache age (skip if checked within last 24h)
   b. If stale, spawn background version check (non-blocking)
   c. If update available, print one-line notice to stderr:
      `[Unleash] Update available: Claude Code 2.1.77 → 2.1.81 (run 'unleash update' to install)`
3. Never block agent launch for updates

### Config Format (already exists)

```toml
[auto_update]
unleash = false

[auto_update.agents]
claude = true
codex = false
gemini = true
opencode = false
```

## CLI Integration

Add `Update` variant to `Commands` enum in `src/cli.rs`:

```rust
/// Update agent CLIs to latest versions
Update {
    /// Specific agents to update (omit for all)
    agents: Vec<String>,

    /// Only check for updates, don't install
    #[arg(long)]
    check: bool,

    /// Also update unleash itself
    #[arg(long, name = "self")]
    update_self: bool,
}
```

## Dependencies

- No new crate dependencies needed
- ANSI progress rendering is simple enough to hand-roll
- Threading uses `std::thread` + `std::sync::mpsc`
- Terminal width from `crossterm::terminal::size()` (already a dependency)

## Edge Cases

- **Non-TTY output** (piped): Skip progress bars, print simple line-by-line status
- **Agent not installed**: Skip with "not installed" message
- **Network failure**: Show error per-agent, don't fail others
- **Concurrent updates**: File lock on update cache to prevent parallel `unleash update` races
- **Codex build (slow)**: Show cargo build output as phase text since it can take minutes
