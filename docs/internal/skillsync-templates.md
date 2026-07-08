# SkillSync Degradation Templates

This document defines the exact templates used by the Unleash `skillsync` engine when degrading a native hub skill (`SKILL.md` with YAML frontmatter) into other agent-specific formats.

---

## 1. The Source Hub Skill (`SKILL.md`)

This is the canonical source representation stored at `~/.local/share/unleash/skills/<name>/SKILL.md`.

```markdown
---
name: "deploy-agent"
description: "Deploys the current project codebase to production using configured environments."
---
# Deploy Agent Skill

This skill allows you to safely deploy changes to the production server.

## Instructions
1. First, check that all tests are passing by running `cargo test` (or the equivalent test runner).
2. Check the git status to verify there are no uncommitted changes.
3. Run the deployment script `./scripts/deploy.sh`.
4. Verify the deployment status by hitting the healthcheck endpoint.
```

---

## 2. Codex Custom Prompt Template (`~/.codex/prompts/<name>.md`)

For Codex, the skill is converted into a custom system prompt file. The header explains the origin of the prompt to the model, and the body contains the instructions.

```markdown
# Codex Custom Prompt: deploy-agent

> [!NOTE]
> This instruction set was automatically generated and synchronized from the Unleash skill **"deploy-agent"**.
> **Description:** Deploys the current project codebase to production using configured environments.

---

## Instructions for the Model
You are acting with the "deploy-agent" skill active. Adhere strictly to the guidelines and workflows specified below whenever the user requests actions related to this skill's scope:

# Deploy Agent Skill

This skill allows you to safely deploy changes to the production server.

## Instructions
1. First, check that all tests are passing by running `cargo test` (or the equivalent test runner).
2. Check the git status to verify there are no uncommitted changes.
3. Run the deployment script `./scripts/deploy.sh`.
4. Verify the deployment status by hitting the healthcheck endpoint.
```

---

## 3. Gemini Custom Command TOML (`~/.gemini/commands/<name>.toml`)

For Gemini and Agy, the skill is compiled into a custom slash command. The TOML structure holds the description and prompt template. It includes support for argument injection.

```toml
# Synced via Unleash skillsync
description = "Deploys the current project codebase to production using configured environments. (Synced Skill)"

prompt = """
# Custom Command: /deploy-agent

You are executing a custom command synchronized from the Unleash skill 'deploy-agent'.

**Scope/Description**: Deploys the current project codebase to production using configured environments.
**User Arguments**: {{args}}

Please execute this task by following these skill instructions:

# Deploy Agent Skill

This skill allows you to safely deploy changes to the production server.

## Instructions
1. First, check that all tests are passing by running `cargo test` (or the equivalent test runner).
2. Check the git status to verify there are no uncommitted changes.
3. Run the deployment script `./scripts/deploy.sh`.
4. Verify the deployment status by hitting the healthcheck endpoint.

---
Use the provided user arguments (if any) to parameterize this run (e.g. specifying target environments, branches, or tags).
"""
```

---

## 4. Plain Context-File Reference (Pi/Hermes `AGENTS.md` Append)

For Pi and Hermes, which lack native custom commands or prompts, the skill is appended to the global context file (`AGENTS.md`) as a reference. It provides the metadata, the path to the original skill directory, and a summary.

```markdown
<!-- unleash-skillsync-start: deploy-agent -->
### 🛠️ Synced Skill: deploy-agent

*   **Description:** Deploys the current project codebase to production using configured environments.
*   **Source Skill Path:** [skills/deploy-agent](file:///home/me/.local/share/unleash/skills/deploy-agent/SKILL.md)

When asked to deploy changes or work on production pipelines, you should locate and read the full skill instructions in the file linked above. If you cannot access the link, use these summarized guidelines:
1. Ensure tests pass (`cargo test`).
2. Verify git status is clean.
3. Run `./scripts/deploy.sh`.
4. Check the production healthcheck.
<!-- unleash-skillsync-end: deploy-agent -->
```
