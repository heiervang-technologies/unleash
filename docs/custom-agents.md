# Custom Agent CLIs

unleash supports any agent CLI as a first-class profile — alongside the seven built-in agents (`claude`, `codex`, `agy`, `gemini`, `opencode`, `pi`, `hermes`). Custom agents get:

- Launchable as `unleash <name>`
- Polyfilled unified flags (`-c`, `-r`, `-p`, `-m`, `--auto`, …)
- TUI integration (agent picker, status)
- Optional GitHub or npm version metadata

What's deferred: automatic version install/update for custom agents. Use the binary you already have on `$PATH`.

## Quick start

Two steps: declare the agent's capabilities, then create a profile that launches it.

**1.** Add a `[[custom_agents]]` block to `~/.config/unleash/config.toml`:

```toml
[[custom_agents]]
name = "aider"
binary = "aider"
description = "AI pair programming in your terminal"
github_repo = "paul-gauthier/aider"

[custom_agents.polyfill]
headless = { flag = "--message" }
session = { continue_strategy = { flag = "--restore-chat-history" }, resume_strategy = { flag = "--restore-chat-history" } }
fork = "unsupported"
model_flag = "--model"
yolo_flag = "--yes"
```

**2.** Create `~/.config/unleash/profiles/aider.toml`:

```toml
name = "aider"
description = "Aider"
agent_cli_path = "aider"
agent_cli_args = []
theme = "orange"

[defaults]
```

The profile's `name` must match the `[[custom_agents]].name` exactly — that's how the launcher resolves which polyfill to apply. `agent_cli_path` is what gets exec'd; usually identical to `binary` (or an absolute path if the binary isn't on `$PATH`).

Then:

```bash
unleash aider              # launch
unleash aider -p "fix it"  # headless — translated to `aider --message "fix it"`
unleash aider -m gpt-4 -c  # model + continue
```

> The TUI's "Add custom agent" wizard does step 1 automatically and points the *currently-editing* profile at the new binary. To get a dedicated `unleash <name>` subcommand, you still need a profile file with the matching name — copy any built-in profile and adjust.

## Required fields

| Field | Type | Notes |
|-------|------|-------|
| `name` | string | Subcommand identifier — `unleash <name>` launches it |
| `binary` | string | Executable name on `$PATH`, or absolute path |
| `polyfill` | section | See below — `headless`, `session`, `fork`, `model_flag` are required |

Everything else (`description`, `github_repo`, `npm_package`, `enabled`) is optional.

## Polyfill strategies

The polyfill section maps unleash's unified flags onto each agent's native syntax.

### `headless` — how to pass a prompt non-interactively

```toml
headless = { flag = "--message" }       # passes `--message <prompt>`
headless = { subcommand = "exec" }      # invokes `<binary> exec <prompt>`
```

### `session` — how to continue or resume a session

Each side (`continue_strategy`, `resume_strategy`) is a `ResumeStrategy`:

```toml
[custom_agents.polyfill.session]
continue_strategy = { flag = "--continue" }      # most common
resume_strategy = { flag = "--resume" }          # `--resume <id>` if id given
# Or:
continue_strategy = { subcommand = "resume --last" }
resume_strategy = { subcommand = "resume" }
```

### `fork` — how to fork (branch) the session

```toml
fork = { flag = "--fork-session" }     # flag-based
fork = { subcommand = "fork" }         # subcommand-based
fork = "unsupported"                   # most agents
```

### `sandbox` — how to enable sandbox mode

```toml
sandbox = { boolflag = "--sandbox" }                    # bare flag
sandbox = { valueflag = ["--sandbox", "workspace-write"] }  # flag with fixed value
sandbox = "unsupported"                                 # default
```

## Flag fields

The polyfill section also takes a set of single-flag fields. These map unleash's unified surface to each agent's native flag:

| Field | unleash flag | Required? |
|-------|--------------|-----------|
| `model_flag` | `-m`, `--model` | **yes** |
| `yolo_flag` | (auto-injected — permission bypass) | optional |
| `effort_flag` | `-e`, `--effort` | optional |
| `auto_flag` | `--auto`, `-a` | optional |
| `verbose_flag` | (passthrough) | optional |
| `output_format_flag` | (passthrough) | optional |
| `system_prompt_flag` | (passthrough) | optional |
| `allowed_tools_flag` | (passthrough) | optional |
| `name_flag` | (passthrough) | optional |
| `add_dir_flag` | (passthrough) | optional |
| `approval_mode_flag` | (passthrough) | optional |
| `worktree_flag` | (passthrough) | optional |

Each takes a single string — the flag name as the agent accepts it. Omitted optional fields are treated as "this agent does not support that capability"; the corresponding unleash flag becomes a no-op for that agent.

## Optional metadata

| Field | Type | Purpose |
|-------|------|---------|
| `description` | string | Shown in TUI agent picker |
| `github_repo` | string `"owner/repo"` | Future: version fetch |
| `npm_package` | string | Future: npm install |
| `enabled` | bool | Default `true`. Set `false` to hide without removing from config |

## Multiple agents

You can declare any number of `[[custom_agents]]` blocks:

```toml
[[custom_agents]]
name = "aider"
binary = "aider"
[custom_agents.polyfill]
headless = { flag = "--message" }
session = { continue_strategy = { flag = "--restore-chat-history" }, resume_strategy = { flag = "--restore-chat-history" } }
fork = "unsupported"
model_flag = "--model"

[[custom_agents]]
name = "cursor"
binary = "cursor-cli"
enabled = false                # hidden until re-enabled
[custom_agents.polyfill]
headless = { flag = "-p" }
session = { continue_strategy = { flag = "--continue" }, resume_strategy = { flag = "--resume" } }
fork = "unsupported"
model_flag = "--model"
```

## Verifying the config parses

The fastest sanity check is `--dry-run` — it resolves the polyfilled command without launching:

```bash
unleash aider --dry-run -m gpt-4 -p "test"
```

Expected output for the example above:

```
Would execute: aider --yes --model gpt-4 --message test
```

If your TOML is malformed, you'll see a deserialization error pointing at the offending field. If you get `Profile 'aider' not found`, you skipped step 2 (profile file).

## Canonical examples

The integration test suite (`tests/custom_agents.rs`) is the authoritative reference — every example there is exercised against the real `from_custom_config` + `polyfill::resolve` pipeline on every CI run. When the schema changes, those examples track it.

## Reporting issues

If a flag you need isn't on the list above, or your agent has a quirk the polyfill can't express, open an issue referencing [#69](https://github.com/heiervang-technologies/unleash/issues/69) — the custom-agents surface is intentionally extensible.
