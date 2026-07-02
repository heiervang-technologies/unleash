# Cross-CLI Agent Skill Synchronization

Synchronize agent skills, custom instructions, and workflows across all your terminal AI harnesses. Define a skill once, and use it seamlessly in Claude Code, Codex, Gemini, OpenCode, Pi, and Hermes.

## How It Works

Skill synchronization uses a unified **hub-and-spoke** architecture to sync custom instructions and skills across different formats without requiring custom converters for every pair of CLIs.

1. **Discovery** -- `unleash skills sync` scans the active directories of your chosen source agent (e.g. Claude Code's `~/.claude/skills/`).
2. **Hub Conversion** -- Discovered skills are imported into the Unleash canonical skill store (`~/.local/share/unleash/skills/`) using the **Agent Skills format** (a directory containing `SKILL.md` with YAML frontmatter).
3. **Availability Tracking** -- The sync engine updates `~/.local/share/unleash/skills/skillsync.toml` to track which skills are enabled for which harnesses.
4. **Target Translation and Injection** -- The engine exports the skills from the canonical store to the target harnesses, degrading or referencing them where native skills are not supported.

### Hub-and-Spoke Architecture

```
Claude Skills (Native)   <-->   Canonical Store (~/.local/share/unleash/skills/)   <-->   OpenCode Agents (Native)
                                                 |
                                                 |----> Codex Prompts (~/.codex/prompts/)       [Degraded]
                                                 |----> Gemini Commands (~/.gemini/commands/)   [Degraded]
                                                 |----> Pi Context (~/.pi/AGENTS.md append)      [Reference]
                                                 |----> Hermes Context (~/.hermes/AGENTS.md)     [Reference]
```

## CLI Surface

Unleash provides four CLI subcommands to inspect and trigger skill synchronization:

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

## Synchronization Fidelity

Different harnesses support different concepts of "skills". Unleash maps them into three tiers:

*   **Native** (`🟢`): The target supports full multi-file skills with auto-activation triggers. The skill folder is copied intact.
*   **Degraded** (`🟡`): The target does not support skills but supports custom prompts/commands. The skill is compiled into a single text prompt or command template containing the instructions.
*   **Reference** (`⚪`): The target lacks prompts or commands. The skill is linked or appended textually inside the harness's global context file (similar to `AGENTS.md`).

| Target Harness | Target Representation | Sync Fidelity | Notes |
|---|---|---|---|
| **Claude** | `~/.claude/skills/<name>/SKILL.md` | **Native** | Native format, supports support files and triggers. |
| **OpenCode** | `~/.config/opencode/agent/<name>.md` | **Native** | Native markdown layout, supports directory-based lookup. |
| **Codex** | `~/.codex/prompts/<name>.md` | **Degraded** | Converted to custom prompt template. |
| **Gemini** | `~/.gemini/commands/<name>.toml` | **Degraded** | Converted to custom `/` slash command. |
| **Agy** | `~/.gemini/commands/<name>.toml` | **Degraded** | Inherits Gemini command layout. |
| **Pi** | `~/.pi/AGENTS.md` | **Reference** | Appended as instruction reference block. |
| **Hermes** | `~/.hermes/AGENTS.md` | **Reference** | Appended as instruction reference block. |

---

## TUI Status Matrix UX

The `unleash skills status` command prints a tabular matrix showing the current sync state and fidelity of all skills:

```
SKILL                  CLAUDE       CODEX        GEMINI       OPENCODE     PI           HERMES       AGY
-------------------------------------------------------------------------------------------------------------
git-expert             🟢 Native    🟡 Prompt    🟡 Command   🟢 Native    ⚪ Reference ⚪ Reference  🟡 Command
ui-builder             🟢 Native    🟡 Prompt    🟡 Command   🟢 Native    ⚪ Reference ⚪ Reference  🟡 Command
db-debugger            🟢 Native    🟡 Prompt    🟡 Command   🟢 Native    ⚪ Reference ⚪ Reference  🟡 Command
```

*   `🟢 Native`: Lossless synchronization of the full skill directory.
*   `🟡 Prompt` / `Command`: Degraded representation. Original instructions preserved, auto-activation lost.
*   `⚪ Reference`: Textual reference appended to the agent's context.

---

## Known Limitations

*   **Auto-activation triggers**: Claude and OpenCode support auto-activating skills based on file matches or queries. Converted prompts (Codex) and commands (Gemini) must be triggered manually by name (e.g. `/git-expert` or `/refactor`).
*   **Support Files**: Non-native targets (Codex, Gemini, Pi, Hermes) discard helper scripts and resources (e.g. `scripts/` or `templates/` folders) because they only accept flat text instructions.
*   **Deletions**: If `delete_orphans` is disabled, deleting a skill from the source harness will leave the degraded files intact on the targets.

## Detailed Matrix

See [skillsync-matrix.md](skillsync-matrix.md) for full pair-wise status, verification fixtures, and active issues.
