# Changelog

## [9.15.0](https://github.com/heiervang-technologies/unleash/compare/v9.14.2...v9.15.0) (2026-03-30)


### Features

* multi-platform release binaries + interactive installer ([6baf1d3](https://github.com/heiervang-technologies/unleash/commit/6baf1d3504209a82b915d187d69193edcc2663a8))

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
- **Yolo mode** by default — permission prompts bypassed, `--safe` to restore
