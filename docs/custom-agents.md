# Custom Agent CLIs

unleash supports any agent CLI as a first-class profile — alongside the seven built-in agents (`claude`, `codex`, `agy`, `gemini`, `opencode`, `pi`, `hermes`). Custom agents get:

- Launchable as `unleash <name>`
- Polyfilled unified flags (`-c`, `-r`, `-p`, `-m`, `--auto`, …)
- TUI integration (agent picker, status)
- Optional GitHub or npm version metadata

What's deferred: automatic version install/update for custom agents. Use the binary you already have on `$PATH`.

## Quick start

The fastest path is `unleash agents add`, which writes both the
`[[custom_agents]]` entry and the matching profile file in one shot:

```bash
unleash agents add aider \
  --binary aider \
  --headless-flag=--message \
  --model-flag=--model \
  --yolo-flag=--yes \
  --github-repo paul-gauthier/aider
```

Then launch:

```bash
unleash aider              # launch interactive
unleash aider -p "fix it"  # headless — translated to `aider --message "fix it"`
unleash aider -m gpt-4 -c  # model + continue
```

Use `--dry-run` to preview the TOML that would be written without
touching disk. Pass values starting with `--` either via `=` syntax
(`--headless-flag=--message`) or quoted (`--headless-flag '--message'`) —
clap won't accept an unquoted `--message` as a value otherwise.

### Re-running `agents add` is a merge, not a replace

If an agent with the same `name` is already registered, `agents add` updates
the existing entry in place. The CLI invocation owns `binary` and the
`headless` strategy — those always take the values you pass. Optional fields
(`description`, `continue_flag`, `resume_flag`, `model_flag`, `yolo_flag`,
`github_repo`, `npm_package`) are only overwritten when the corresponding flag
is provided. Everything else — `enabled`, `effort_flag`, `sandbox`, `fork`,
`verbose_flag`, and every other polyfill knob without a dedicated CLI flag —
is preserved verbatim from the existing config. The matching profile file
follows the same rule (theme, env, agent_cli_args, stop_prompt all preserved).

This means you can hand-edit `config.toml` to set fields the CLI doesn't expose
yet, and a later `unleash agents add <name> --binary <new> --headless-flag=…`
won't wipe them. `--dry-run` previews the merged result with a "merging with
existing entry" hint so you can confirm before committing.

### Or do it by hand

If you'd rather edit the config files directly (e.g. to script bulk
registration or check the result into a dotfiles repo):

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

> The TUI's "Add custom agent" wizard writes the same files. `unleash agents add` is the headless/scriptable equivalent.

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
