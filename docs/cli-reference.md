# Unleash CLI Reference

Unified CLI manager for AI code agents (Claude, Codex, Gemini, OpenCode).

## Usage

```
unleash                              # Launch TUI
unleash <profile> [flags] [-- ...]   # Run agent with unified flags
unleash <subcommand>                 # Management commands
```

## Profiles

A profile maps to an installed agent CLI. Run `unleash agents status` to see available profiles.

```bash
unleash claude              # Launch Claude Code
unleash codex -a            # Launch Codex in auto-mode
unleash gemini -p "fix it"  # Headless Gemini prompt
```

## Unified Flags

| Flag | Short | Description |
|------|-------|-------------|
| `--prompt <text>` | `-p` | Non-interactive headless mode with given prompt |
| `--model <model>` | `-m` | Model to use |
| `--continue` | `-c` | Continue most recent session |
| `--resume [id]` | `-r` | Resume by ID, or open picker if no ID given |
| `--fork` | | Fork session (use with `--continue` or `--resume`) |
| `--auto` | `-a` | Enable auto-mode (autonomous operation) |
| `--effort <level>` | `-e` | Reasoning effort level (e.g. `high`, `low`) |
| `--crossload [source]` | `-x` | Load conversation from another CLI (e.g. `codex:rust-eng`) or open picker |
| `--safe` | | Restore approval prompts (default is permissions-bypassed) |
| `--dry-run` | | Show resolved command without executing |

## Flag Translation

How unified flags map to each agent's native CLI:

| unleash | Claude | Codex | Gemini | OpenCode |
|---------|--------|-------|--------|----------|
| `-p <prompt>` | `-p <prompt>` | `exec <prompt>` | `-p <prompt>` | `run <prompt>` |
| `-c` | `--continue` | `resume --last` | `--resume latest` | `--continue` |
| `-r [id]` | `--resume [id]` | `resume [id]` | `--resume [id]` | `--session <id>` |
| `--fork` | `--fork-session` | `fork` subcommand | *(unsupported)* | `--fork` |
| *(default)* | `--dangerously-skip-permissions` | `--dangerously-bypass-approvals-and-sandbox` | `--yolo` | *(no-op)* |
| `--safe` | *(omits above)* | *(omits above)* | *(omits above)* | *(no-op)* |

## Passthrough

Anything after `--` is forwarded to the agent unchanged:

```bash
unleash claude -m opus -- --effort max --verbose
unleash codex -- --notify
```

## Subcommands

### `unleash` (no arguments)

Launch the interactive TUI for profile and version management.

### `unleash update`

Update unleash and/or agent CLIs.

```bash
unleash update              # Update unleash itself
unleash update -c           # Update all installed agent CLIs
unleash update -a           # Update unleash + all agent CLIs
unleash update claude       # Update only Claude
unleash update claude codex # Update Claude and Codex
unleash update --check      # Dry run, show available updates
```

### `unleash version`

Manage Claude Code versions (install, list, switch).

```bash
unleash version                   # Show installed Claude Code version
unleash version --list            # List all available Claude Code versions
unleash version --install 2.1.87  # Install a specific Claude Code version
```

> For all agents' versions at a glance, use `unleash agents status`.

### `unleash auth`

Check authentication status for configured agents.

```bash
unleash auth                # Human-readable status
unleash auth --json         # Machine-readable JSON output
```

### `unleash agents status`

Show all agent versions and update status in a single table.

### `unleash sessions`

List sessions across all installed CLIs.

### `unleash convert --from <format> <input>`

Convert between CLI session formats. Use `--from` to specify the source format and
`--to` for the target (defaults to `hub`). Output goes to stdout unless `-o` is given.

```bash
unleash convert --from claude session.jsonl                     # Convert to hub format (stdout)
unleash convert --from claude --to codex session.jsonl          # Convert Claude → Codex
unleash convert --from claude --to codex session.jsonl -o out.json  # Write to file
unleash convert --from claude --to codex session.jsonl --verify # Verify round-trip fidelity
```

## Examples

```bash
# Start Claude in auto-mode, continue last session
unleash claude -c -a

# Headless Codex run with a prompt
unleash codex -p "add error handling to src/main.rs"

# Resume a specific Gemini session
unleash gemini -r abc123

# Crossload a Codex session into Claude
unleash claude -x codex:rust-eng

# Fork and continue a session with a specific model
unleash claude -c --fork -m sonnet

# See what command would be executed
unleash codex -p "hello" --dry-run
```
