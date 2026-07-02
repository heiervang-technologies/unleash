# Cross-CLI Skill Synchronization Matrix

Status of agent skill synchronization across all supported agent CLIs.

**Last updated:** 2026-07-02

> Coverage below outlines the fidelity of synchronizing skills from any source harness to any target harness. Statuses represent current validation via synthetic skill fixtures.

## Usage

```bash
# List all skills across all harnesses with their local availability
unleash skills list

# Synchronize skills: source -> canonical store -> targets
unleash skills sync

# Sync from a specific source harness (defaults to claude)
unleash skills sync --from codex

# Display a status matrix of all skills and their target fidelity
unleash skills status

# Dry-run showing what files and settings would change
unleash skills diff
```

## Matrix

| Source → Target | Status | Notes |
|----------------|--------|-------|
| Claude → Claude | :green_circle: Lossless | Verified synthetic fixture round-trip via native Agent Skills directory. |
| Claude → Codex | :yellow_circle: Partial | Verified degraded render to `~/.codex/prompts/<name>.md`; support files referenced only through text. |
| Claude → Gemini | :yellow_circle: Partial | Verified degraded render to `~/.gemini/commands/<name>.toml`; manual slash-command invocation. |
| Claude → OpenCode | :yellow_circle: Partial | Verified reference block in `~/.config/opencode/AGENTS.md`; no native installed format found locally. |
| Claude → Agy | :yellow_circle: Partial | Verified degraded Gemini command TOML path shared by Agy. |
| Claude → Pi | :yellow_circle: Partial | Verified reference block in `~/.pi/AGENTS.md`. |
| Claude → Hermes | :yellow_circle: Partial | Verified reference block in `~/.hermes/AGENTS.md`. |
| Codex → Claude | :green_circle: Lossless | Verified synthetic fixture round-trip from Codex prompt representation through hub into native skill dir. |
| Codex → Codex | :yellow_circle: Partial | Verified degraded prompt representation preserves portable fields. |
| Codex → Gemini | :yellow_circle: Partial | Verified degraded prompt source to Gemini command target. |
| Codex → OpenCode | :yellow_circle: Partial | Verified reference block target; no native installed OpenCode skill format found locally. |
| Codex → Agy | :yellow_circle: Partial | Verified degraded prompt source to Agy/Gemini command target. |
| Codex → Pi | :yellow_circle: Partial | Verified reference block target. |
| Codex → Hermes | :yellow_circle: Partial | Verified reference block target. |
| Gemini → Claude | :green_circle: Lossless | Verified command TOML source preserves portable fields into native skill dir. |
| Gemini → Codex | :yellow_circle: Partial | Verified command TOML source to Codex prompt target. |
| Gemini → Gemini | :yellow_circle: Partial | Verified degraded command representation preserves portable fields. |
| Gemini → OpenCode | :yellow_circle: Partial | Verified reference block target; no native installed OpenCode skill format found locally. |
| Gemini → Agy | :yellow_circle: Partial | Verified Gemini command source to Agy shared command path. |
| Gemini → Pi | :yellow_circle: Partial | Verified reference block target. |
| Gemini → Hermes | :yellow_circle: Partial | Verified reference block target. |
| OpenCode → Claude | :green_circle: Lossless | Verified reference source portable fields into native skill dir. |
| OpenCode → Codex | :yellow_circle: Partial | Verified reference source to Codex prompt target. |
| OpenCode → Gemini | :yellow_circle: Partial | Verified reference source to Gemini command target. |
| OpenCode → OpenCode | :yellow_circle: Partial | Verified reference block representation only; no native installed format found locally. |
| OpenCode → Agy | :yellow_circle: Partial | Verified reference source to Agy/Gemini command target. |
| OpenCode → Pi | :yellow_circle: Partial | Verified reference block target. |
| OpenCode → Hermes | :yellow_circle: Partial | Verified reference block target. |
| Agy → Claude | :green_circle: Lossless | Verified Agy/Gemini command source portable fields into native skill dir. |
| Agy → Codex | :yellow_circle: Partial | Verified Agy/Gemini command source to Codex prompt target. |
| Agy → Gemini | :yellow_circle: Partial | Verified shared command path target. |
| Agy → OpenCode | :yellow_circle: Partial | Verified reference block target; no native installed OpenCode skill format found locally. |
| Agy → Agy | :yellow_circle: Partial | Verified degraded command representation preserves portable fields. |
| Agy → Pi | :yellow_circle: Partial | Verified reference block target. |
| Agy → Hermes | :yellow_circle: Partial | Verified reference block target. |
| Pi → Claude | :green_circle: Lossless | Verified reference source portable fields into native skill dir. |
| Pi → Codex | :yellow_circle: Partial | Verified reference source to Codex prompt target. |
| Pi → Gemini | :yellow_circle: Partial | Verified reference source to Gemini command target. |
| Pi → OpenCode | :yellow_circle: Partial | Verified reference block target; no native installed OpenCode skill format found locally. |
| Pi → Agy | :yellow_circle: Partial | Verified reference source to Agy/Gemini command target. |
| Pi → Pi | :yellow_circle: Partial | Verified reference block representation preserves portable fields. |
| Pi → Hermes | :yellow_circle: Partial | Verified reference block target. |
| Hermes → Claude | :green_circle: Lossless | Verified reference source portable fields into native skill dir. |
| Hermes → Codex | :yellow_circle: Partial | Verified reference source to Codex prompt target. |
| Hermes → Gemini | :yellow_circle: Partial | Verified reference source to Gemini command target. |
| Hermes → OpenCode | :yellow_circle: Partial | Verified reference block target; no native installed OpenCode skill format found locally. |
| Hermes → Agy | :yellow_circle: Partial | Verified reference source to Agy/Gemini command target. |
| Hermes → Pi | :yellow_circle: Partial | Verified reference block target. |
| Hermes → Hermes | :yellow_circle: Partial | Verified reference block representation preserves portable fields. |

**Legend:** :green_circle: Lossless (verified native directory sync) · :yellow_circle: Partial (degraded to prompt/command template or context reference) · :red_circle: Not working · :white_circle: Untested

---

## Known Limitations

*   **Trigger Lossiness**: Native triggers (e.g. `glob` matches for automatic activation) only execute under Claude in the verified implementation. Targets using custom prompts/commands (Codex, Gemini, Agy) or context references (OpenCode, Pi, Hermes) require manual invocation.
*   **Orphan Cleanups**: When removing a skill from the source harness, target cleanup is disabled unless `delete_orphans` is configured to `"on"`.
*   **Asset Stripping**: Non-native adapters (Codex, Gemini, Pi, Hermes) discard binary assets and sub-folder helper scripts. Only the primary instructions in `SKILL.md` are synchronized.
*   **Agy Cascading**: Like session crossload, Antigravity (`agy`) inherits Gemini's command paths but may enforce server-side validation checks on execution context.

## Planned Improvements

*   **Automated Verification**: Wire round-trip tests to verify exact string equality after double-conversion (e.g. `Claude -> Hub -> Gemini -> Hub -> Claude`).
*   **Interactive Command Generator**: Provide TUI-based editing of custom triggers for degraded targets.

## Test Fixtures

Verification is run against the following synthetic skill fixtures located in `src/skillsync/tests/fixtures/synthetic/`:

*   `minimal-skill/`: A basic skill containing only YAML frontmatter and simple text instructions.
*   `helper-script-skill/`: A skill containing a `scripts/` directory with a bash script to test asset handling/filtering.
*   `unicode-edgecase-skill/`: A skill containing special characters, emojis, and multiline markdown tables to test encoding preservation.
