# Unleash CLI Reference

Unified CLI manager for AI code agents (Claude, Codex, Antigravity, Gemini, OpenCode, Pi, Hermes).

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

| unleash | Claude | Codex | Antigravity (`agy`) | Gemini | OpenCode | Pi | Hermes |
|---------|--------|-------|---------------------|--------|----------|----|--------|
| `-p <prompt>` | `-p <prompt>` | `exec <prompt>` | `-p <prompt>` | `-p <prompt>` | `run <prompt>` | `-p <prompt>` | `-z <prompt>` |
| `-c` | `--continue` | `resume --last` | `--continue` | `--resume latest` | `--continue` | `--continue` | `--continue` |
| `-r [id]` | `--resume [id]` | `resume [id]` | `--conversation [id]` | `--resume [id]` | `-s <id>` | `--session <id>` | `--resume [id]` |
| `--fork` | `--fork-session` | `fork` subcommand | *(unsupported)* | *(unsupported)* | `--fork` | `--fork` | `--worktree` |
| *(default)* | `--dangerously-skip-permissions` | `--dangerously-bypass-approvals-and-sandbox` | `--dangerously-skip-permissions` | `--yolo` | *(no-op)* | *(no-op)* | `--yolo` |
| `--safe` | *(omits above)* | *(omits above)* | *(omits above)* | *(omits above)* | *(no-op)* | *(no-op)* | *(omits above)* |

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

### `unleash agents`

Manage agent CLIs (built-in or custom).

```bash
unleash agents status                # All agent versions + update status (default)
unleash agents list                  # Available agents (built-in + registered custom)
unleash agents check [agent]         # Query latest releases (all, or one)
unleash agents update <agent>        # Install the latest release of one agent
unleash agents info <agent>          # Detailed info for one agent
unleash agents add <name> --binary <path> --headless-flag=<flag>  # Register a custom agent
```

`unleash agents add` writes both a `[[custom_agents]]` entry to
`~/.config/unleash/config.toml` and a matching profile file at
`~/.config/unleash/profiles/<name>.toml`, so `unleash <name>` works
immediately. See [docs/custom-agents.md](custom-agents.md) for full
field reference and examples; `--dry-run` previews the TOML without
touching disk.

### `unleash sessions`

List sessions across all installed CLIs.

```bash
unleash sessions                              # List all
unleash sessions --cli claude                 # Filter by CLI
unleash sessions --find claude:abc1234        # Lookup one
unleash sessions reindex                      # Rebuild the search index
unleash sessions name claude:abc "My Title"   # Override the display title
unleash sessions name claude:abc              # Regenerate via chat model
```

`sessions reindex` re-embeds every session and refreshes generated titles using
the configured OpenAI-compatible endpoint (see `unleash search`).

`sessions name <cli>:<source_id> [TITLE]` updates the `generated_title` column
in the search index. With an explicit `TITLE` it's a direct write; without one
the configured chat model is asked for a new 3–6 word title. Useful when an
auto-generated title is wrong or missing.

### `unleash search`

Semantic + BM25 hybrid search across all indexed sessions.

```bash
unleash search                          # Open the interactive TUI
unleash search "fix install summary"    # Pre-fill the TUI with a query
unleash search --json --top 10 "..."    # Non-interactive ranked JSON output
unleash search --reindex                # Rebuild before searching
```

Backed by a Turso DB at `~/.local/share/unleash/search-index.db`. Uses an
OpenAI-compatible local server for embeddings (e.g.
`llama-server --embeddings -m embed.gguf`). See `unleash search --help` for
environment variables (`OAI_BASE`, `OAI_EMBED_MODEL`, `OAI_CHAT_MODEL`,
`ALPHA`).

### `unleash convert`

Convert between CLI session formats. Use `--from` to specify the source format and
`--to` for the target (defaults to `hub`). Output goes to stdout unless `-o` is given.

```bash
unleash convert --from claude session.jsonl                         # Convert to hub format (stdout)
unleash convert --from claude --to codex session.jsonl              # Convert Claude → Codex
unleash convert --from claude --to codex session.jsonl -o out.json  # Write to file
unleash convert --from codex session.jsonl --verify                 # round-trip lossless check
unleash convert --from claude --to passthrough session.jsonl        # markdown transcript for prompt-paste
```

Required:
- `--from <format>`: source format. One of: `claude` (alias `claude-code`),
  `codex`, `gemini` (aliases `gemini-cli`, `antigravity`, `antigravity-cli`,
  `agy` — same JSON schema), `opencode`, `pi` (alias `pi-coding-agent`),
  `hermes` (alias `hermes-agent`), or `hub` / `ucf`.
- `<input>`: path to the input file (positional)

Optional:
- `--to <format>`: target format. Same set as `--from`, plus the special
  text-output format `passthrough` (aliases `transcript`, `prompt`) — a
  markdown-formatted chat transcript intended for piping into a target
  CLI's `--prompt` flag when session-level injection isn't supported
  (e.g. `agy -i "$(unleash convert --from claude --to passthrough …)"`;
  see issue #313). Tool calls, tool results, thinking blocks, and
  images are summarised rather than reproduced verbatim. Default: `hub`.
- `--output <path>` / `-o <path>`: output file (default: stdout)
- `--verify`: verify lossless round-trip instead of converting

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
