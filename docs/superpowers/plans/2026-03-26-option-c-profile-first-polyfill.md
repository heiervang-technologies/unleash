# Option C: Profile-First Polyfill Restructure

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make all first arguments profile lookups with unified polyfill flags, eliminating hardcoded agent subcommands.

**Architecture:** Remove Claude/Codex/Gemini/OpenCode enum variants from clap. Parse first arg as profile name, look it up, determine AgentType from the profile, then apply data-driven polyfill resolution using AgentPolyfillConfig metadata from agents.rs.

**Tech Stack:** Rust, clap 4.5, serde/toml for profiles

---

### Task 1: Restructure cli.rs — Remove agent subcommands, profile-first parsing

**Files:**
- Modify: `src/cli.rs`

- [ ] **Step 1: Remove Claude/Codex/Gemini/OpenCode variants from Commands enum**
- [ ] **Step 2: Add a single `Run` variant that takes profile name + PolyfillArgs + trailing args**
- [ ] **Step 3: Hide --yolo flag (it's the default), make --safe the visible inverse**
- [ ] **Step 4: Update PolyfillArgs to_polyfill_flags() conversion**
- [ ] **Step 5: Update tests**
- [ ] **Step 6: Commit**

### Task 2: Rewrite lib.rs dispatch — Single profile path

**Files:**
- Modify: `src/lib.rs`

- [ ] **Step 1: Remove per-agent match arms, route all to run_agent_with_polyfill**
- [ ] **Step 2: Merge run_profile() and run_agent_with_polyfill() into single function**
- [ ] **Step 3: Profile lookup determines AgentType, polyfill resolution, then launcher**
- [ ] **Step 4: Remove UNLEASH_POLYFILL_ACTIVE env var hack**
- [ ] **Step 5: Update tests**
- [ ] **Step 6: Commit**

### Task 3: Make polyfill.rs data-driven

**Files:**
- Modify: `src/polyfill.rs`
- Depends on: Gemini's AgentPolyfillConfig expansion in `src/agents.rs`

- [ ] **Step 1: Change resolve() to accept AgentPolyfillConfig instead of AgentType**
- [ ] **Step 2: Use config metadata for yolo flag, headless strategy, session strategy, fork strategy**
- [ ] **Step 3: Add deduplication logic (skip flags already in profile agent_args)**
- [ ] **Step 4: Update tests to use AgentPolyfillConfig**
- [ ] **Step 5: Commit**

### Task 4: Unify launcher.rs

**Files:**
- Modify: `src/launcher.rs`

- [ ] **Step 1: Remove dual code path (UNLEASH_POLYFILL_ACTIVE check)**
- [ ] **Step 2: launcher::run always uses polyfill-resolved args (no auto yolo injection)**
- [ ] **Step 3: Update tests**
- [ ] **Step 4: Commit**

### Task 5: Update documentation

**Files:**
- Modify: `README.md`
- Modify: `CLAUDE.md`

- [ ] **Step 1: Update README CLI usage to show profile-first syntax**
- [ ] **Step 2: Update CLAUDE.md if needed**
- [ ] **Step 3: Commit**

### Task 6: Final validation

- [ ] **Step 1: cargo test — all tests pass**
- [ ] **Step 2: cargo clippy — no warnings**
- [ ] **Step 3: cargo install --path . — installs cleanly**
- [ ] **Step 4: Push and update PR**
