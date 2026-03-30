# Documentation Map

Quick visual guide to finding the right documentation.

## Documentation Tree

```
docs/
├── README.md                          # START HERE - Overview and navigation
│
├── auth-check-command.md              # CLI: Authentication verification command
├── JSON_OUTPUT.md                     # CLI: JSON output specification
│
├── docker/                            # Docker Container Guides
│   ├── README.md                      # Quick start, single agent, multi-agent (tiers 1-2)
│   └── NETWORKING.md                  # Advanced networking, sidecars (tier 3)
│
├── extensions/                        # Extension Development Guides
│   ├── plugin-development.md          # PRIMARY: Creating plugins (1195 lines)
│   ├── configuration.md               # Configuration options (CLI, TUI, settings)
│   ├── restart-refresh.md             # Process restart and MCP refresh
│   ├── snail-integration.md           # GitHub Actions workflows (1332 lines)
│   └── testing-guide.md               # Testing strategies (1372 lines)
│
├── internal/                          # Internal Developer Documentation
│   ├── README.md                      # Index + CLI format comparison matrix
│   ├── claude-code/
│   │   ├── CLI_FORMAT.md              # Claude Code JSONL transcript format
│   │   └── XML_MESSAGE_SIGNATURES.md  # Native XML tags for rendering & agent messages
│   ├── codex/
│   │   └── CLI_FORMAT.md              # Codex JSONL + SQLite format
│   ├── gemini/
│   │   └── CLI_FORMAT.md              # Gemini CLI JSON session format
│   └── opencode/
│       └── CLI_FORMAT.md              # OpenCode SQLite + Drizzle format
│
└── (root)
    └── tests/README.md                # Test suite documentation
```

## Decision Tree: Which Doc Do I Need?

```
┌─ I want to add functionality
│  └─> START: plugin-development.md
│      ├─ Commands, agents, skills? → plugin-development.md (Component Types)
│      └─ Testing my plugin? → testing-guide.md (Local Plugin Testing)
│
┌─ I want to use CLI tools
│  └─> START: auth-check-command.md
│      ├─ Check authentication? → auth-check-command.md
│      ├─ JSON output? → JSON_OUTPUT.md
│      └─ Run tests? → tests/README.md
│
┌─ I want to run agents in Docker
│  └─> START: docker/README.md
│      ├─ Single agent? → docker/README.md (Quick Start)
│      ├─ Multi-agent team? → docker/README.md (Multi-Agent Teams)
│      ├─ Sidecars/advanced? → docker/NETWORKING.md
│      └─ Networking issues? → docker/NETWORKING.md (Troubleshooting)
│
┌─ I want to configure settings
│  └─> START: configuration.md
│      ├─ CLI flags? → configuration.md (CLI Configuration)
│      ├─ TUI settings? → configuration.md (TUI Settings)
│      ├─ Stop prompt? → configuration.md (Stop Prompt Configuration)
│      └─ Config files? → configuration.md (Configuration Files)
│
┌─ I want to integrate with GitHub
│  └─> START: snail-integration.md
│      ├─ Workflow triggers? → snail-integration.md (Workflow Integration)
│      ├─ Agent automation? → snail-integration.md (Example Commands and Agents)
│      └─ MCP servers? → snail-integration.md (Available MCP Servers)
│
┌─ I want to test my changes
│  └─> START: testing-guide.md
│      ├─ Local testing? → testing-guide.md (Local Plugin Testing)
│      ├─ Workflow testing? → testing-guide.md (GitHub Workflow Testing)
│      └─ Debugging? → testing-guide.md (Debugging Tips)
│
└─ I'm new here
   └─> START: docs/README.md
       └─ Then: plugin-development.md (Step-by-Step Plugin Creation)
```

## By Task

| Task | Primary Doc | Related Docs |
|------|-------------|--------------|
| Check authentication | auth-check-command.md | JSON_OUTPUT.md |
| Use JSON output | JSON_OUTPUT.md | auth-check-command.md |
| Run tests | tests/README.md | - |
| Create new command | plugin-development.md § Commands | testing-guide.md § Testing Commands |
| Create agent | plugin-development.md § Agents | snail-integration.md § Example Agents |
| Create skill | plugin-development.md § Skills | - |
| Create hook | plugin-development.md § Hooks | testing-guide.md § Testing Hooks |
| Add MCP server | plugin-development.md § MCP Servers | snail-integration.md § Available MCP Servers |
| Configure CLI | configuration.md § CLI Configuration | - |
| Configure TUI | configuration.md § TUI Settings | - |
| Customize stop prompt | configuration.md § Stop Prompt Configuration | plugins/bundled/auto-mode/README.md |
| Run agent in Docker | docker/README.md | docker/NETWORKING.md |
| Multi-agent networking | docker/NETWORKING.md | docker/README.md |
| Test locally | testing-guide.md § Local Plugin Testing | - |
| Test workflows | testing-guide.md § GitHub Workflow Testing | snail-integration.md |
| Debug issues | testing-guide.md § Debugging Tips | - |
| Setup GitHub Actions | snail-integration.md § Workflow Integration | - |
| Configure secrets | snail-integration.md § Configuration and Secrets | - |

## By Expertise Level

### Beginner (New to unleash)

1. **docs/README.md** - Start here for overview
2. **plugin-development.md** - Learn plugin basics
3. **testing-guide.md** - Test your first plugin
4. **snail-integration.md** - Understand workflow integration

### Intermediate (Creating plugins)

1. **plugin-development.md** - Deep dive into components
2. **testing-guide.md** - Advanced testing strategies
3. **snail-integration.md** - Create workflow-integrated plugins
### Advanced (Core contributor)

1. **plugin-development.md** - Complex plugin patterns
2. **testing-guide.md** - Test automation

## By Component Type

### Commands

- **Primary**: plugin-development.md § Creating a Command
- **Testing**: testing-guide.md § Testing Commands
- **Examples**: snail-integration.md § Example Commands

### Agents

- **Primary**: plugin-development.md § Creating an Agent
- **Testing**: testing-guide.md § Testing Agents
- **Examples**: snail-integration.md § Example Agents
- **Workflows**: snail-integration.md § How Plugins Enhance Workflows

### Skills

- **Primary**: plugin-development.md § Creating a Skill
- **Testing**: testing-guide.md § Testing Skills

### Hooks

- **Primary**: plugin-development.md § Creating Hooks
- **Testing**: testing-guide.md § Testing Hooks
- **Examples**: Existing hookify plugin

### MCP Servers

- **Primary**: plugin-development.md § MCP Servers
- **Integration**: snail-integration.md § Available MCP Servers
- **Testing**: testing-guide.md § Testing MCP Servers

## By Scenario

### Scenario: "I want to create an agent that triages GitHub issues"

1. **plugin-development.md** § Creating an Agent (understand agent structure)
2. **snail-integration.md** § Example 1: Issue Triage Command (see complete example)
3. **testing-guide.md** § Testing Agents (test your implementation)
4. **snail-integration.md** § GitHub Actions Workflow Integration (deploy to workflow)

### Scenario: "My plugin isn't loading"

1. **testing-guide.md** § Issue: Plugin Not Loading (troubleshoot)
2. **plugin-development.md** § Directory Structure (verify structure)
3. **testing-guide.md** § Debug Mode Testing (detailed debugging)

## File Sizes (Comprehensiveness)

| File | Lines | Focus Area |
|------|-------|------------|
| testing-guide.md | 1372 | Most comprehensive testing guide |
| snail-integration.md | 1332 | Complete GitHub Actions integration |
| plugin-development.md | 1195 | Comprehensive plugin creation guide |
| README.md | 610 | Overview and navigation |
| **Total** | **4509** | **Complete documentation suite** |

## Search Keywords

### Quick Search Index

- **Authentication**: auth-check-command.md
- **auth-check**: auth-check-command.md
- **JSON output**: JSON_OUTPUT.md
- **Test suite**: tests/README.md
- **Plugin creation**: plugin-development.md
- **Command**: plugin-development.md § Commands
- **Agent**: plugin-development.md § Agents
- **Skill**: plugin-development.md § Skills
- **Hook**: plugin-development.md § Hooks
- **MCP**: plugin-development.md § MCP Servers
- **Configuration**: configuration.md
- **CLI flags**: configuration.md § CLI Configuration
- **TUI settings**: configuration.md § TUI Settings
- **Stop prompt**: configuration.md § Stop Prompt Configuration
- **Testing**: testing-guide.md
- **Debug**: testing-guide.md § Debugging Tips
- **Workflow**: snail-integration.md
- **GitHub Actions**: snail-integration.md
- **Secrets**: snail-integration.md § Configuration and Secrets
## Navigation Tips

1. **Start with docs/README.md** for overview
2. **Use the decision tree above** to find relevant documentation
3. **Search for keywords** using the index above
4. **Follow cross-references** between documents
5. **Check examples** in snail-integration.md for practical patterns

## Getting Help

If you can't find what you need:

1. Check the **decision tree** above
2. Search **all docs** for keywords
3. Review **examples** in snail-integration.md
4. Create a **GitHub issue** with your question

---

**Documentation Version**: 1.2.0
**Last Updated**: 2026-03-01
**Total Coverage**: 4800+ lines across 5 comprehensive guides
