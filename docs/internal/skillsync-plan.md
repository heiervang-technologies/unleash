# Skillsync Plugin — Implementation Plan

**Status:** planning brief for implementation agents (2026-07-02)
**Owner:** unleash-agent session (coordinator)

## Goal

A bundled, toggleable unleash plugin called **skillsync** that synchronizes
skills — and the *availability* of skills (enabled/disabled per harness) —
from one harness to all other installed harnesses. Test coverage and status
reporting must follow the same pattern as the ucf crossload system:
per-pair matrix doc, synthetic fixtures, round-trip tests in `src/`.

## Architecture: hub-and-spoke, mirroring `src/interchange/`

Like session crossload, skill sync uses O(N) adapters against a hub format
instead of O(N²) pairs.

- **Hub format:** the Agent Skills format — a directory with `SKILL.md`
  (YAML frontmatter: `name`, `description`) plus optional support files.
  This is Claude Code's native format and the emerging cross-tool standard
  (OpenCode already reads it), so the hub is lossless for the richest case.
- **Canonical store:** `~/.local/share/unleash/skills/<name>/` — sync is
  source → hub → targets.
- **Availability manifest:** `~/.local/share/unleash/skills/skillsync.toml`
  tracking, per skill, which harnesses it is enabled for. Disabling a skill
  for a harness uninstalls it there on next sync.

### Per-harness adapters (new Rust module `src/skillsync/`, sibling of `interchange/`)

Each adapter implements: `discover() -> Vec<Skill>`, `install(&Skill)`,
`uninstall(name)`, and reports a **fidelity level**:

| Harness | Expected target representation | Expected fidelity |
|---|---|---|
| claude | `~/.claude/skills/<name>/SKILL.md` | Native (lossless) |
| opencode | OpenCode skills location (verify on this machine) | Native or near-native |
| codex | `~/.codex/prompts/<name>.md` custom prompt | Degraded (skill → prompt) |
| gemini | `~/.gemini/commands/<name>.toml` custom command | Degraded |
| agy | shares Gemini path (see `normalize_target_cli` precedent in `inject.rs`) | Inherits gemini |
| pi | verify — command/prompt mechanism or context-file append | Degraded or Reference |
| hermes | verify — likely context-file append | Reference |

Fidelity legend (parallel to crossload's Lossless/Partial):
**Native** = full skill dir installed, auto-activation preserved.
**Degraded** = converted to that harness's prompt/command primitive; body
preserved, activation semantics lost. **Reference** = only listed/linked in
the harness's context file (AGENTS.md-style).

All seven CLIs (claude, codex, gemini, opencode, pi, hermes, agy) are
installed on this machine — verify real formats against the actual tools,
not from memory.

## CLI surface (Rust, `src/`)

```
unleash skills list            # all skills across harnesses, with per-harness availability
unleash skills sync [--from H] # source → hub → all targets (default source: claude)
unleash skills status          # matrix view, like `unleash sessions` table style
unleash skills diff            # what would change, without writing
```

## Plugin layer (`plugins/bundled/skillsync/`)

Toggleability is free: anything in `plugins/bundled/` with a valid
`.claude-plugin/plugin.json` shows up in the unleash TUI plugin list
(`discover_plugins()` in `src/config.rs`, `enabled_plugins` filter in
`src/launcher.rs`).

- `.claude-plugin/plugin.json` — manifest with `settings` (schema precedent:
  supercompact's manifest):
  - `source` (choice: claude/codex/gemini/opencode/hub; default claude)
  - `sync_on_launch` (choice: on/off; default on)
  - `delete_orphans` (choice: on/off; default off) — remove skills from
    targets when removed from source
- `hooks/hooks.json` — SessionStart hook → `unleash skills sync`, guarded by
  the `check-enabled.sh` pattern (copy from supercompact/scripts/).
- `commands/skillsync.md` — `/skillsync` slash command: on-demand sync + status.
- `README.md`.

## Tests + status matrix (the "ucf pattern")

1. **Synthetic fixtures:** `src/skillsync/tests/fixtures/synthetic/` — 3+
   skills with known content: minimal (frontmatter+body), one with support
   files (`scripts/`), one with unicode/edge-case content.
2. **Round-trip unit tests:** `src/skillsync/cross_harness_tests.rs` —
   install into a temp-HOME target, discover back, semantic-compare portable
   fields (name, description, body). Parallel to
   `src/interchange/cross_cli_tests.rs`.
3. **Matrix doc:** `docs/skillsync-matrix.md` — same structure and legend
   style as `docs/crossload-matrix.md` (:green_circle: verified end-to-end,
   :yellow_circle: works with limitations, :red_circle: not working,
   :white_circle: untested), one row per source→target pair, "Last updated"
   stamp, fixtures section, known limitations.
4. **Plugin smoke test:** `tests/test_skillsync.sh` following the
   `tests/test-plugins.sh` pattern (manifest parses, hooks executable,
   hooks survive empty JSON payload).

## Division of labor

- **Pane %6 (coding agent):** Rust module + adapters + CLI subcommand +
  plugin scaffolding + fixtures + tests. Verify each harness's real skill /
  prompt / command format against the installed CLIs.
- **Pane %4 (gemini — creative/stylistic):** documentation set
  (`docs/skillsync.md`, `docs/skillsync-matrix.md` skeleton, plugin README,
  main README section), degradation templates (how a skill should *read*
  when rendered as a Codex prompt / Gemini TOML command / context-file
  reference), and the `unleash skills status` table/matrix UX copy.
  Coordinate so %6 implements against %4's templates and fills in real test
  status in the matrix.

## Conventions

- Conventional commits (`feat(skillsync): …`), focused and atomic.
- Work on a feature branch `feat/skillsync`, not main.
- Don't modify Claude Code itself; don't hardcode org-specific values.
