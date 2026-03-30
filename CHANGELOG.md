# Changelog

## [0.1.1](https://github.com/heiervang-technologies/unleash/compare/v0.1.0...v0.1.1) (2026-03-30)


### Bug Fixes

* installer downloads prebuilt binaries, no cargo required ([c1791d9](https://github.com/heiervang-technologies/unleash/commit/c1791d90e3c5537906a5b66829d0843be6f44538))

## 0.1.0 (2026-03-30)


### Features

* multi-platform release binaries + interactive installer ([6baf1d3](https://github.com/heiervang-technologies/unleash/commit/6baf1d3504209a82b915d187d69193edcc2663a8))
* multi-platform splash binaries + installer downloads both ([58032b6](https://github.com/heiervang-technologies/unleash/commit/58032b6ae4ac5a5a8cb5b27236bb8575bb7f2244))

## 1.0.0

Initial open-source release.

### Features

- **Unified CLI wrapper** for Claude Code, Codex, Gemini CLI, and OpenCode
- **Polyfill flag layer** — common flags (`-m`, `-p`, `-c`, `-r`, `-e`, `-a`, `--safe`) work across all agents
- **TUI** for profile management, agent version control, and session browsing
- **Profile system** — named TOML configurations with per-profile model, effort, and safe-mode defaults
- **Agent lifecycle management** — `unleash install`, `unleash update`, `unleash uninstall`
- **Crossload** — portable conversation histories between CLI formats (Claude, Codex, Gemini, OpenCode)
- **Interactive installer** with ANSI mascot art and agent-specific theme recoloring
- **Plugin system** with bundled plugins:
  - **auto-mode** — autonomous operation between prompts via Stop hook
  - **process-restart** — self-restart with session preservation (`unleash-refresh`)
  - **mcp-refresh** — detect and reload MCP config changes
  - **hyprland-focus** — window transparency on Hyprland
- **Docker support** — sandboxed containers and multi-agent mesh
- **Diagonal gradient theming** — per-agent mascot art recoloring (e.g., Gemini blue-to-pink gradient)
- **Multi-platform binaries** — Linux x86_64/aarch64, macOS x86_64/aarch64
- **Yolo mode** by default — permission prompts bypassed, `--safe` to restore
