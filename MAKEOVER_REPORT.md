# Makeover Report: Agent Unleashed

## Executive Summary
Agent Unleashed is a powerful, lightweight wrapper around Anthropic's Claude Code, enabling headless execution via tmux, autonomous operation, and plugin-based extensions. The repository is generally well-structured and functional. 

However, the audit revealed critical operational and security weaknesses primarily in how shell commands are formulated within the headless (`autx`) mode, which leaves it vulnerable to local command injection. Furthermore, the installation scripts bypass robust artifact verification, and the application silently overrides security boundaries (`--dangerously-skip-permissions`) without explicitly alerting the user.

A phased remediation plan is recommended to harden the application against injection, improve installation safety, and elevate the reliability of the system.

---

## Findings & Categorization

### P1: High-Impact Bugs & Failures
* **Command Injection in `autx start` and `autx send`** (Effort: S)
  * **File:** `src/tmux.rs`
  * **Description:** User-provided arguments for launching the background agent (`claude_args`) are joined by a space and passed verbatim into `tmux send-keys`. If an argument contains shell metacharacters (e.g., `autx start "hello; rm -rf /"`), it is executed by the shell inside the tmux session. Furthermore, `autx send` joins arguments and passes them without the `-l` (literal) flag, which can cause `tmux` to incorrectly interpret them as flags if they begin with a hyphen.
  * **Remediation:** Implement proper shell-escaping for all arguments passed to the shell via `tmux send-keys`, and use `tmux send-keys -l -- <message>` for literal message sending.

### P2: Bad Practices & Documentation Debt
* **Silent Override of Permissions** (Effort: XS)
  * **File:** `src/launcher.rs`
  * **Description:** The wrapper automatically appends `--dangerously-skip-permissions` to the Claude Code invocation. While this is necessary for "autonomous" mode and extensions to function smoothly, doing so silently without any configuration toggle or prominent user warning violates the principle of least astonishment.
  * **Remediation:** Document this clearly, or print a prominent warning to the terminal on startup.
* **Missing Architecture Diagrams** (Effort: XS - **Fixed during audit**)
  * **File:** `README.md`
  * **Description:** The architecture documentation previously relied on ASCII art which is less maintainable and visually appealing than standard Mermaid diagrams. 

### P3: Test Coverage & Minor Security
* **Insecure Remote Installation Execution** (Effort: S)
  * **File:** `scripts/install-remote.sh`
  * **Description:** The remote installer fetches binary artifacts and runs them. While it verifies GCS artifacts, the GitHub release fetching lacks cryptographic signature or SHA256 checksum validation for the downloaded binaries, trusting the network directly.
  * **Remediation:** Implement checksum validation for the GitHub release downloads.

### P4: Technical Debt & Refactoring
* **Fragile JSON Parsing in Onboarding Bypass** (Effort: S)
  * **File:** `scripts/install-remote.sh`
  * **Description:** The fallback `sed` commands used to update `~/.claude.json` rely on simplistic regex replacement. If the JSON structure deviates (e.g., unexpected line breaks or missing keys), it can silently fail or corrupt the JSON.
  * **Remediation:** Always rely on proper JSON processing tools, or fail gracefully rather than blindly replacing string values.
* **Busy Waiting in Headless Mode** (Effort: M)
  * **File:** `src/tmux.rs`
  * **Description:** `cmd_wait` uses a busy-wait loop (`thread::sleep`) checking file size every second to determine if Claude is done outputting. 
  * **Remediation:** Refactor to use inotify (via a crate like `notify`) to wait on file changes instead of polling.

### P4.5: Performance Bottlenecks
* **Redundant API Rate Limits and Output Stalls** (Effort: L)
  * **Description:** Not explicitly observed in the codebase, but the `autx` tmux file output tracking may struggle with very large contexts due to reading the entire file on each poll to check its length (`fs::read(config.output_file()).map(|b| b.len())`).
  * **Remediation:** Use `fs::metadata` to check file length instead of loading the entire file into memory during the polling loop.

### P5: Future Directions (Extrapolation)
* **Native Headless Mode (No tmux):** The reliance on `tmux` limits cross-platform compatibility (especially on Windows) and adds a dependency. Future iterations could use pseudoterminal (PTY) spawning via Rust natively instead of shelling out to `tmux`.
* **Plugin Sandboxing:** Currently plugins run with the same permissions as the wrapper. Leveraging WebAssembly (Wasm) for plugins could provide strict, capable sandboxing.

### P6: Novel & Creative Expansions
* **Multi-Agent Orchestration:** Extend the `autx` background worker model to allow multiple specialized Claude instances to communicate via shared MCP buffers or designated memory workspaces.
* **TUI Visualization of Agent Thinking:** Render real-time Mermaid graphs of the agent's plan or structural changes directly within the `aui` interface using ratatui canvas components.

---

## Architectural Critique
The design choice to treat Claude Code as a black box and manipulate its behavior via `.claude/settings.json` hooks and `tmux` multiplexing is pragmatic and achieves "zero upstream conflicts." However, it fundamentally suffers from *brittleness*—any subtle change to Anthropic's CLI output, hook processing, or terminal expectations can break the entire wrapper. 

Relying on `tmux` scraping (via `pipe-pane` and byte-length polling) to determine response completion is the most fragile part of the architecture. Moving towards a native PTY implementation in Rust would drastically improve robustness and reduce latency.