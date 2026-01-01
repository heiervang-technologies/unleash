# Snail Core Template

Template repository for setting up a Snail AI agent on your GitHub repository.

## Quick Start

1. **Create a new repository from this template**
   - Click the green "Use this template" button above
   - Choose a name for your repository
   - Click "Create repository"

2. **Configure your credentials**
   - After creation, a GitHub issue will automatically be created with setup instructions
   - Follow the instructions in that issue to configure the required secrets

3. **Customize your agent**
   - Edit `.github/workflows/mention-trigger.yml` to change the agent username from `@marksverdhai` to your agent's username
   - Update the `agent_name` parameter to match

4. **Test your agent**
   - Create an issue and mention your agent (e.g., `@your-agent help me with...`)
   - The agent should respond within a few minutes

## Required Secrets

These are provided as organization secrets in heiervang-technologies:

| Secret | Description |
|--------|-------------|
| `HAI_GH_PAT` | GitHub Personal Access Token with `repo` and `workflow` scopes |
| `HEI_DOCKER_PAT` | Docker Hub PAT for pulling snail images (read-only access) |
| `CLAUDE_CREDENTIALS_JSON` | Claude API credentials in JSON format |

## Included Workflows

### Mention Trigger (`mention-trigger.yml`)

Triggers the snail agent when mentioned in:
- Issue bodies
- Issue comments
- Pull request review comments

### Setup Check (`setup-check.yml`)

Automatically runs on first push to verify:
- Required secrets are configured
- PAT has sufficient repository permissions
- Claude credentials are valid

Creates an issue with detailed setup instructions if anything is missing.

## How It Works

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Your Repository                              │
├─────────────────────────────────────────────────────────────────────┤
│                                                                      │
│   @agent help me fix this bug                                        │
│         │                                                            │
│         ▼                                                            │
│   ┌─────────────────────┐                                            │
│   │ mention-trigger.yml │                                            │
│   └──────────┬──────────┘                                            │
│              │                                                       │
│              │ Uses reusable workflow                                │
│              ▼                                                       │
│   ┌─────────────────────────────────────────────────────────────┐   │
│   │           heiervang-technologies/core                        │   │
│   │               spawn-agent.yml                                │   │
│   │                                                              │   │
│   │   ┌─────────────┐     ┌─────────────┐     ┌────────────┐    │   │
│   │   │ Pull snail  │────▶│ Run Claude  │────▶│ Post       │    │   │
│   │   │ container   │     │ in container│     │ results    │    │   │
│   │   └─────────────┘     └─────────────┘     └────────────┘    │   │
│   └─────────────────────────────────────────────────────────────┘   │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

## Adding Assignment Trigger

To also trigger the agent when issues/PRs are assigned, copy the assignment trigger workflow from the core repository:

```bash
# From the heiervang-technologies/core repo, copy:
# .github/workflows/assignment-trigger.yml
```

Then update the assignee filter to match your agent's username.

## Troubleshooting

### "Setup Required" issue keeps appearing

- Ensure all three secrets are available (org secrets or repo secrets)
- Verify the `HAI_GH_PAT` has `repo` and `workflow` scopes
- Check that the PAT belongs to an account with write access to the repo

### Agent doesn't respond to mentions

1. Check the Actions tab for workflow runs
2. Look for errors in the workflow logs
3. Verify the agent username matches what's in the workflow file
4. Ensure the PAT hasn't expired

### Authentication errors

If you see "authentication error" in the workflow logs:
- Your Claude credentials may have expired
- Refresh the `CLAUDE_CREDENTIALS_JSON` secret with new credentials

## License

MIT
