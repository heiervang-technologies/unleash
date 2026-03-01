# Strategic Vision & Future Directions

## P5: Future Directions (Extrapolation)

### 1. Native Headless Mode (No `tmux` dependency)
**Concept:** Currently, the `unleashtx` command relies heavily on `tmux` for background execution and capturing output. While practical, it introduces a hard dependency on a third-party tool, limits cross-platform compatibility (e.g., Windows), and forces the system to rely on file-polling to determine when the agent has finished responding.
**Proposal:** Move to a pure-Rust PTY (pseudoterminal) implementation using crates like `portable-pty`. The agent can be spawned natively in the background, and standard IPC or WebSockets can be used to stream output, handle completion signals robustly (without file scraping), and interact dynamically.

### 2. Strict Plugin Sandboxing via WebAssembly
**Concept:** The extension layer currently loads plugins that run with the exact same permissions as the wrapper and Claude itself.
**Proposal:** By integrating a Wasm runtime (like Wasmtime or Wasmer), plugins could be compiled to WebAssembly. This would provide strict sandboxing—plugins would only have access to explicit capabilities (e.g., a specific subset of files or a constrained network API) minimizing the blast radius if a third-party plugin acts maliciously.

## P6: Novel & Creative Expansions

### 1. Multi-Agent Orchestration & Shared Memory
**Concept:** Expand the headless mode to manage a "swarm" of agents rather than just one.
**Proposal:** 
* Implement a central dispatcher in the wrapper.
* Spin up multiple specialized Claude instances (e.g., "Reviewer", "Coder", "Architect") in parallel.
* Use a shared memory bus (or MCP - Model Context Protocol servers) where agents can pass messages or coordinate on complex repositories without human intervention.

### 2. Visual Agent Thinking (Ratatui TUI)
**Concept:** Claude generates code and plans sequentially, which can be hard to track for complex changes.
**Proposal:** Utilize `ratatui` (the TUI framework already used in this project) to dynamically parse Claude's thinking and structural intent, rendering real-time UI components like tree graphs, dependency maps, or a progress pipeline on the side of the terminal while it works. This provides the user with an "X-ray" into the autonomous execution state.

### 3. Integrated Rollback / Snapshot Engine
**Concept:** Unattended auto-mode can occasionally corrupt a project.
**Proposal:** The wrapper could automatically hook into `git` to perform an ephemeral commit (or use a file system snapshot mechanism) before starting an auto-mode session. If Claude's session goes awry, the user can invoke a built-in `unleash revert` command to instantly undo all operations from the specific session without untangling complex git histories manually.
