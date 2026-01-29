# V5 Planning Document

## Goal
Transition from a Claude-specific wrapper to a multi-agent management wrapper that provides:
1.  **Multi-Agent Support** (Select and launch different code agents)
2.  **Resurrection** (Restart with context)
3.  **Process Management** (Clean termination, signal handling)
4.  **Autonomous Capabilities** (Auto-mode, loops, hooks)
5.  **Unified Config** (Profiles, Env Vars, TUI)

## 1. The Multi-Agent Wrapper Architecture

The Cloud Unleashed wrapper acts as a unified UI and lifecycle manager for multiple code agents.

### Core Concepts

*   **Agent Definition**: Each agent is defined with its name, binary path, and description
*   **Agent Selection**: Users select which agent to launch from the TUI
*   **Agent Configuration**: Each agent has its own profiles, patches, and settings
*   **Lifecycle Management**: The wrapper handles launch, resurrection, and termination for all agents
*   **Feature Uniformity**: Core features (resurrection, auto-mode, hooks) work consistently across agents

## 2. Configuration Schema (`config.toml`)

Support for defining multiple available agents with their specific configurations.

```toml
active_agent = "claude"

[agents.claude]
name = "Claude Code"
binary = "claude"
description = "Anthropic's Claude Code CLI"

[agents.aider]
name = "Aider"
binary = "aider"
description = "AI pair programming tool"

[agents.codex]
name = "Codex"
binary = "codex"
description = "OpenAI Codex CLI with OpenRouter"
```

### Per-Agent Configuration

Each agent can have agent-specific settings:
- **Patches**: Version-specific patches to apply
- **Environment Variables**: Agent-specific env vars
- **Profiles**: Different configuration profiles
- **Auto-Mode Settings**: Agent-specific auto-mode behavior

## 3. The "Auto-Mode" Strategy

*   **Native Support**: Prefer using the agent's built-in auto mode if available
*   **Wrapper Support**: Explore if the wrapper needs to manage the loop for agents that don't have native auto-mode

## 4. Agent Lifecycle Management

The wrapper manages the complete lifecycle of each agent:

### Launch
- Start the selected agent with appropriate arguments
- Apply agent-specific patches if needed
- Set up environment variables and configuration

### Resurrection
- Restart the agent process
- Preserve session state and conversation context
- Reload configurations, plugins, and environment variables

### Termination
- Clean shutdown of the agent process
- Signal handling and graceful exit
- Cleanup of temporary resources

## 5. Work Breakdown

1.  **Update Configuration Schema**: Support multiple agent definitions and per-agent settings
2.  **Agent Selection UI**: Add agent selection capability to the TUI
3.  **Multi-Agent Launch Logic**: Implement launching logic for different agent types
4.  **Unified Resurrection**: Apply resurrection features consistently across agents
5.  **Feature Uniformity**: Ensure auto-mode, hooks, and other features work for all agents
6.  **Testing**: Validate with multiple agents (Claude, Codex, Aider, etc.)

## 6. Backward Compatibility

- Keep existing `claude-unleashed` command and behavior
- Default agent remains Claude Code for existing users
- Gradual rollout of multi-agent features
