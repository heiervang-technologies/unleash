# Audit Scratchpad

## Phase 1: Deep Contextualization & Auditing
*Initial setup and exploration phase.*

### Immediate Findings & Observations
- Project is a Rust-based CLI/TUI tool named "unleash", wrapping Anthropic's Claude Code.
- It provides a `tmux` based headless mode, auto-mode, plugin support, and version management.
- The `unleashtx` command (headless mode) has command injection vulnerabilities in `src/tmux.rs` when joining shell arguments.
- The launcher auto-appends `--dangerously-skip-permissions` without explicit user confirmation, which is central to "unleashed" but potentially dangerous.

### Hypotheses & Architectural Observations
- **Architecture**: The wrapper manages execution by injecting environment variables and launching Claude Code. The hooks modify `~/.claude/settings.json` to attach scripts to Claude's event lifecycle.
- **Tmux Integration**: `unleashtx` relies heavily on parsing and waiting for output files to stabilize rather than programmatic IPC.

### Visual Documentation (Mermaid) Needs
- Converted ASCII tree in `README.md` to a Mermaid diagram. (Pending)
- Add architecture diagram for Wrapper vs Extension Layer. (Pending)

## Phase 2 Categorization & Effort Estimation

- **P0: Critical Security & Integrity**
  - None yet.

- **P1: High-Impact Bugs & Failures**
  - **Command Injection in `unleashtx start` and `unleashtx send` (src/tmux.rs):** User input `args` are joined by spaces without proper shell escaping, meaning an argument like `hello; rm -rf /` gets injected into the tmux shell. (Effort: S) - Fix by properly escaping or using array-based execution in tmux.

- **P2: Bad Practices & Documentation Debt**
  - **Always-on `--dangerously-skip-permissions` (src/launcher.rs):** Auto-appended without warning. Should document this prominently or provide an opt-out. (Effort: XS)
  - **Unsafe Tmux send-keys (src/tmux.rs):** If message starts with `-`, `tmux send-keys` tries to interpret it as a flag. Needs `-l` or `--`. (Effort: XS)

- **P3: Test Coverage & Minor Security**
  - **Insecure download execution:** `install-remote.sh` encourages `curl | bash` without checksums for the bash script or the GitHub release binaries. (Effort: S)

- **P4: Technical Debt & Refactoring**
  - **Fragile JSON parsing in installer (scripts/install-remote.sh):** The `sed` fallback for updating `~/.claude.json` uses simple regex that could corrupt JSON if the format changes slightly. (Effort: S)
  - **Busy waiting in headless mode (src/tmux.rs):** `cmd_wait` uses `thread::sleep(Duration::from_secs(1))` in a loop checking file sizes. (Effort: M)

- **P4.5: Performance Bottlenecks**
  - None major observed yet; it's a lightweight CLI wrapper.

- **P5: Future Directions (Extrapolation)**
  - TBD

- **P6: Novel & Creative Expansions**
  - TBD
