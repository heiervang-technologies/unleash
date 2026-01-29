# Headless Mode (autx)

`autx` (Agent Unleashed tmux eXecutor) provides a headless mode for running Claude Code in the background. Note: The legacy command `cutx` still works using tmux as the session manager. This approach offers significant advantages over native headless modes for automation, scripting, and CI/CD integration.

## Overview

### What is autx?

`autx` is a wrapper script that runs Claude Code inside a tmux session, enabling:

- **Headless operation**: Run Claude without an interactive terminal
- **Session persistence**: Sessions survive disconnects and can be reattached
- **Programmatic interaction**: Send messages and read responses via commands
- **Background processing**: Let Claude work while you do other things

### Why autx Over Native Headless?

| Feature | autx | Native Headless |
|---------|------|-----------------|
| Session persistence | Yes (tmux-based) | No |
| Attach/detach mid-session | Yes | No |
| Output logging | Automatic | Manual |
| Multiple parallel sessions | Yes (via session names) | Limited |
| Works with agent-unleashed | Yes | Partial |
| Response detection | File-size heuristic | Varies |
| Debugging | Attach and inspect | Difficult |

### Key Benefits

1. **Attach Anytime**: Start a headless session, then attach to see what Claude is doing
2. **Persistent Logs**: All output is captured to `~/.cache/agent-unleashed/autx/`
3. **Session Recovery**: If your SSH connection drops, the session keeps running
4. **Simple Scripting**: Send commands and read responses with basic shell commands
5. **Multiple Sessions**: Run multiple Claude instances with different session names

## Prerequisites

### Required Software

- **tmux**: Terminal multiplexer (version 3.0+)
- **Claude Code**: Installed and authenticated
- **Bash**: Version 4.0+ (standard on most Linux systems)

### Installation Check

```bash
# Verify tmux is installed
tmux -V

# Verify Claude is authenticated
claude --version
```

### Installing tmux

```bash
# Ubuntu/Debian
sudo apt install tmux

# macOS
brew install tmux

# Fedora
sudo dnf install tmux
```

## Installation

### Automatic Installation

`autx` is installed automatically when you run the main installation script:

```bash
./scripts/install.sh
```

This creates a symlink at `~/.local/bin/autx` pointing to `scripts/autx`.

### Manual Installation

If you need to install manually:

```bash
# From the agent-unleashed directory
ln -sf "$(pwd)/scripts/autx" ~/.local/bin/autx

# Ensure ~/.local/bin is in your PATH
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Verify Installation

```bash
autx help
```

## Commands Reference

### `autx start [args]`

Start a new Claude session in a detached tmux session.

**Usage:**
```bash
autx start [--auto|-a] [-d|--daemon] [claude-args...]
```

**Options:**
- `--auto`, `-a`: Enable auto mode (sets `CLAUDE_AUTO_MODE=1`)
- `-d`, `--daemon`: Kill the tmux session when Claude exits
- All other arguments are passed through to Claude

**Examples:**
```bash
# Start a basic session
autx start

# Start with auto mode enabled
autx start --auto

# Start in daemon mode (session closes when Claude exits)
autx start -d

# Start with a specific project directory
autx start /path/to/project

# Continue a previous session
autx start --continue

# Combine options
autx start --auto -d --continue
```

**Notes:**
- If a session already exists, the command will fail (use `autx stop` first)
- The session name defaults to `agent-unleashed` (configurable via environment)
- Output is automatically logged to `~/.cache/agent-unleashed/autx/`

---

### `autx send "message"`

Send a message to the running Claude session.

**Usage:**
```bash
autx send "your message here"
```

**Examples:**
```bash
# Send a simple message
autx send "Hello Claude"

# Send a multi-line message
autx send "Review this code:
def hello():
    print('world')
"

# Send content from a file
autx send "Analyze this: $(cat file.py)"

# Send command output
autx send "Explain this error: $(npm test 2>&1)"
```

**Notes:**
- The command records a marker to track new output since the message was sent
- Use `autx wait` after sending to wait for the response
- Special characters are passed through to tmux

---

### `autx read`

Read the output from Claude.

**Usage:**
```bash
autx read
```

**Behavior:**
- If a message was recently sent (marker exists), shows only output since that message
- If no marker exists, shows all output from the session
- Output includes ANSI escape codes (pipe through `cat -v` to see them)

**Examples:**
```bash
# Read current output
autx read

# Read and save to file
autx read > claude-response.txt

# Read and strip ANSI codes
autx read | sed 's/\x1b\[[0-9;]*m//g'

# Read and extract specific content
autx read | grep -A 10 "Summary:"
```

---

### `autx wait [timeout]`

Wait for Claude to finish responding.

**Usage:**
```bash
autx wait [timeout_seconds]
```

**Arguments:**
- `timeout_seconds`: Maximum time to wait (default: 300 seconds / 5 minutes)

**Detection Method:**
The command considers a response complete when the output file size remains stable for 3 consecutive seconds (the "stable threshold").

**Examples:**
```bash
# Wait with default timeout (300s)
autx wait

# Wait with custom timeout
autx wait 60

# Wait for a long operation
autx wait 600
```

**Exit Codes:**
- `0`: Response completed
- `1`: Timeout reached

---

### `autx attach [--here]`

Attach to the running Claude tmux session.

**Usage:**
```bash
autx attach [--here|-h]
```

**Options:**
- `--here`, `-h`: Join the Claude pane into your current tmux window (side-by-side)

**Behavior:**
- If already in tmux: switches to the Claude session (use `prefix + (` or `)` to switch back)
- If not in tmux: attaches to the session directly
- Detach with `Ctrl+B, D`

**Examples:**
```bash
# Attach to session
autx attach

# Join Claude pane into current window (when already in tmux)
autx attach --here
```

---

### `autx stop`

Stop the Claude session and clean up.

**Usage:**
```bash
autx stop
```

**Actions:**
1. Kills the tmux session
2. Removes the output file
3. Removes the marker file

**Examples:**
```bash
# Stop the session
autx stop

# Force stop and restart
autx stop && autx start
```

---

### `autx status`

Check if a session is running and display information.

**Usage:**
```bash
autx status
```

**Output includes:**
- Session running status (green/red indicator)
- Session details (windows, creation time)
- Last 10 lines of output (if available)

**Example Output:**
```
[autx] Session 'agent-unleashed' is running
agent-unleashed: 1 windows, created Sat Jan 10 14:30:00 2025

Recent output (last 10 lines):
─────────────────────────────
Claude: I've finished analyzing the code. Here are my findings...
```

---

### `autx "message"` (Shorthand)

Send a message and wait for the response in one command.

**Usage:**
```bash
autx "your message here"
```

**Behavior:**
1. Starts a new session if none exists (waits 3 seconds for initialization)
2. Sends the message
3. Waits for the response
4. Prints the response

**Examples:**
```bash
# Quick query
autx "What is the capital of France?"

# Analyze a file
autx "Review this code for bugs: $(cat main.py)"

# Get a summary
autx "Summarize the key points in README.md"
```

**Notes:**
- This is equivalent to: `autx start; autx send "msg"; autx wait; autx read`
- Convenient for one-off queries
- Session remains running after the command (use `autx stop` to clean up)

---

### `autx help`

Display the help message with all commands and options.

**Usage:**
```bash
autx help
# or
autx --help
autx -h
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AUTX_SESSION_NAME` | `agent-unleashed` | tmux session name. Change this to run multiple parallel sessions. |
| `AUTX_WAIT_TIMEOUT` | `300` | Default timeout in seconds for `autx wait` command. |

### Configuration Examples

```bash
# Run multiple parallel sessions
AUTX_SESSION_NAME=project-a autx start /path/to/project-a
AUTX_SESSION_NAME=project-b autx start /path/to/project-b

# Set longer default timeout for complex operations
export AUTX_WAIT_TIMEOUT=600
autx "Refactor this entire codebase..."

# Use in scripts with custom session
export AUTX_SESSION_NAME="ci-claude-${BUILD_ID}"
autx start -d
autx send "Review PR #${PR_NUMBER}"
autx wait 120
autx read
# Session auto-closes due to -d flag
```

### Internal Configuration

These values are set in the script and control internal behavior:

| Setting | Value | Description |
|---------|-------|-------------|
| `CACHE_DIR` | `~/.cache/agent-unleashed/autx` | Directory for output and marker files |
| `stable_threshold` | `3` | Seconds of stable output before considering response complete |
| `interval` | `1` | Polling interval in seconds for wait command |

## Use Cases

### CI/CD Integration

#### GitHub Actions Example

```yaml
name: Claude Code Review

on:
  pull_request:
    types: [opened, synchronize]

jobs:
  review:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0

      - name: Install Agent Unleashed
        run: |
          git clone https://github.com/heiervang-technologies/agent-unleashed.git
          cd agent-unleashed
          ./scripts/install.sh

      - name: Review PR
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          # Start Claude in daemon mode
          autx start -d

          # Send review request
          autx send "Review this PR for security issues, bugs, and code quality:

          $(git diff origin/main...HEAD)

          Focus on:
          1. Security vulnerabilities
          2. Logic errors
          3. Performance issues
          4. Code style"

          # Wait for response
          autx wait 180

          # Save review
          autx read > review.txt

      - name: Post Review Comment
        uses: actions/github-script@v7
        with:
          script: |
            const fs = require('fs');
            const review = fs.readFileSync('review.txt', 'utf8');
            github.rest.issues.createComment({
              issue_number: context.issue.number,
              owner: context.repo.owner,
              repo: context.repo.repo,
              body: '## Claude Code Review\n\n' + review
            });
```

#### GitLab CI Example

```yaml
claude-review:
  stage: review
  script:
    - autx start -d
    - autx send "Review the changes in this MR: $(git diff origin/main)"
    - autx wait 120
    - autx read > review.txt
  artifacts:
    paths:
      - review.txt
```

### Cron Jobs

#### Daily Code Analysis

```bash
#!/bin/bash
# /etc/cron.daily/claude-code-review

PROJECT_DIR="/home/user/myproject"
OUTPUT_DIR="/var/log/claude-reviews"
DATE=$(date +%Y-%m-%d)

cd "$PROJECT_DIR"

# Get recent commits
COMMITS=$(git log --oneline --since="24 hours ago")

if [ -n "$COMMITS" ]; then
    autx start -d
    autx send "Analyze these recent commits for potential issues:

$COMMITS

$(git diff HEAD~10..HEAD)

Provide a summary of:
1. Any concerning changes
2. Suggested improvements
3. Documentation that might need updates"

    autx wait 300
    autx read > "$OUTPUT_DIR/review-$DATE.txt"
fi
```

Add to crontab:
```bash
# Run daily at 9 AM
0 9 * * * /etc/cron.daily/claude-code-review
```

#### Weekly Dependency Audit

```bash
#!/bin/bash
# weekly-audit.sh

cd /path/to/project

autx start -d
autx send "Audit the project dependencies:

package.json:
$(cat package.json)

package-lock.json (summary):
$(jq '.packages | keys | length' package-lock.json) packages

Check for:
1. Known vulnerabilities
2. Outdated packages
3. Unnecessary dependencies"

autx wait 180
AUDIT=$(autx read)

echo "$AUDIT" | mail -s "Weekly Dependency Audit" team@example.com
```

### Scripting

#### Automated Report Generation

```bash
#!/bin/bash
# generate-report.sh

INPUT_FILE="$1"
OUTPUT_FILE="${2:-report.txt}"

if [ ! -f "$INPUT_FILE" ]; then
    echo "Usage: $0 <input-file> [output-file]"
    exit 1
fi

# Start session if not running
autx status >/dev/null 2>&1 || autx start

# Generate report
autx send "Generate a comprehensive summary of the following data:

$(cat "$INPUT_FILE")

Format the output as:
1. Executive Summary (2-3 sentences)
2. Key Findings (bullet points)
3. Recommendations (numbered list)"

autx wait 120
autx read > "$OUTPUT_FILE"

echo "Report saved to $OUTPUT_FILE"
```

#### Batch Processing

```bash
#!/bin/bash
# process-files.sh

OUTPUT_DIR="./summaries"
mkdir -p "$OUTPUT_DIR"

autx start

for file in ./documents/*.txt; do
    filename=$(basename "$file" .txt)

    autx send "Summarize this document in 3 bullet points:

$(cat "$file")"

    autx wait 60
    autx read > "$OUTPUT_DIR/${filename}-summary.txt"

    echo "Processed: $file"
done

autx stop
echo "All files processed"
```

#### Interactive Script with User Input

```bash
#!/bin/bash
# interactive-claude.sh

echo "Starting Claude session..."
autx start

while true; do
    echo ""
    read -p "You: " message

    if [ "$message" = "quit" ] || [ "$message" = "exit" ]; then
        break
    fi

    autx send "$message"
    autx wait
    echo ""
    echo "Claude:"
    autx read
done

autx stop
echo "Session ended"
```

### Parallel Sessions

Run multiple Claude instances for different tasks:

```bash
#!/bin/bash
# parallel-review.sh

# Start sessions for different aspects
AUTX_SESSION_NAME=security-review autx start -d
AUTX_SESSION_NAME=performance-review autx start -d
AUTX_SESSION_NAME=style-review autx start -d

CODE=$(cat src/main.py)

# Send requests in parallel
AUTX_SESSION_NAME=security-review autx send "Review for security: $CODE"
AUTX_SESSION_NAME=performance-review autx send "Review for performance: $CODE"
AUTX_SESSION_NAME=style-review autx send "Review for code style: $CODE"

# Wait for all
AUTX_SESSION_NAME=security-review autx wait &
AUTX_SESSION_NAME=performance-review autx wait &
AUTX_SESSION_NAME=style-review autx wait &
wait

# Collect results
echo "=== Security Review ===" > full-review.txt
AUTX_SESSION_NAME=security-review autx read >> full-review.txt

echo "=== Performance Review ===" >> full-review.txt
AUTX_SESSION_NAME=performance-review autx read >> full-review.txt

echo "=== Style Review ===" >> full-review.txt
AUTX_SESSION_NAME=style-review autx read >> full-review.txt

# Sessions auto-close due to -d flag
```

## Limitations

### Response Detection is Heuristic-Based

The `autx wait` command detects response completion by monitoring output file size stability. This approach has limitations:

- **False positives**: If Claude pauses while thinking, it might be detected as "done"
- **Long operations**: Extended tool use or file operations may need longer timeouts
- **No semantic understanding**: The detection doesn't know if Claude is mid-sentence

**Mitigations:**
- Increase `AUTX_WAIT_TIMEOUT` for complex operations
- Increase the stable threshold by modifying the script
- Use `autx attach` to visually verify completion

### Single Session Per Name

Only one session can run per session name at a time.

**Workaround:**
```bash
# Use different session names for parallel work
AUTX_SESSION_NAME=project-a autx start
AUTX_SESSION_NAME=project-b autx start
```

### Requires tmux

The tool will not work without tmux installed.

**Check:**
```bash
command -v tmux || echo "tmux not installed"
```

### Output Contains ANSI Codes

Claude's output includes terminal formatting codes.

**Strip them:**
```bash
autx read | sed 's/\x1b\[[0-9;]*m//g'
```

### No Built-in JSON Output

For programmatic use, you may need to parse Claude's text output.

**Suggestion:** Ask Claude to format output as JSON:
```bash
autx send "List the files. Output as JSON array only, no other text."
```

## Troubleshooting

### Session Won't Start

**Symptoms:**
- "Session already exists" error
- Command hangs

**Solutions:**
```bash
# Check for existing session
autx status

# Force stop and restart
autx stop
autx start

# Check tmux directly
tmux list-sessions

# Kill orphaned session
tmux kill-session -t agent-unleashed
```

### No Output from `autx read`

**Symptoms:**
- Command returns empty
- Response seems missing

**Solutions:**
```bash
# Check if output file exists
ls -la ~/.cache/agent-unleashed/autx/

# View raw output file
cat ~/.cache/agent-unleashed/autx/agent-unleashed.output

# Check if Claude is still running
autx status

# Attach to see what's happening
autx attach
```

### Response Appears Incomplete

**Symptoms:**
- Output cuts off mid-sentence
- `wait` returns before Claude finishes

**Solutions:**
```bash
# Increase wait timeout
autx wait 600

# Or set environment variable
export AUTX_WAIT_TIMEOUT=600
autx wait

# Check if Claude is still processing
autx attach
```

### tmux Permission Issues

**Symptoms:**
- "server exited unexpectedly" error
- Cannot create session

**Solutions:**
```bash
# Check tmux socket directory
ls -la /tmp/tmux-$(id -u)/

# Try with explicit socket
tmux -S /tmp/my-tmux.sock new-session -d

# Check for zombie processes
pgrep -f tmux
```

### Claude Not Responding

**Symptoms:**
- Messages sent but no output
- Session appears frozen

**Solutions:**
```bash
# Attach and check visually
autx attach

# Send Ctrl+C to interrupt
tmux send-keys -t agent-unleashed C-c

# Check Claude process
ps aux | grep claude

# Restart session
autx stop
autx start
```

### Output File Growing Too Large

**Symptoms:**
- Disk space warnings
- Slow `autx read`

**Solutions:**
```bash
# Check file size
du -h ~/.cache/agent-unleashed/autx/

# Clear and restart
autx stop
rm ~/.cache/agent-unleashed/autx/*
autx start
```

## Architecture

### How autx Works

```
┌─────────────────────────────────────────────────────────────────┐
│                         autx Command                             │
├─────────────────────────────────────────────────────────────────┤
│  autx start     │  Creates tmux session, starts Claude          │
│  autx send      │  Sends keystrokes to tmux session             │
│  autx read      │  Reads from output file                       │
│  autx wait      │  Polls output file size                       │
│  autx attach    │  Connects terminal to tmux session            │
│  autx stop      │  Kills tmux session                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       tmux Session                               │
│  Session: agent-unleashed (configurable)                       │
│  Size: 200x50 characters                                        │
│  pipe-pane: Captures all output                                 │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌──────────────────┐ ┌──────────────┐ ┌──────────────────┐
│   Claude Code    │ │  Output File │ │   Marker File    │
│   (running)      │ │  (logging)   │ │  (tracking)      │
└──────────────────┘ └──────────────┘ └──────────────────┘
```

### Key Components

#### 1. tmux Session

- Created with `tmux new-session -d -s SESSION_NAME -x 200 -y 50`
- Runs detached (`-d`) so it doesn't require a terminal
- Fixed size (`-x 200 -y 50`) for consistent output formatting
- Named session allows multiple users/scripts to interact

#### 2. Output Capture (pipe-pane)

- `tmux pipe-pane -t SESSION -o "cat >> OUTPUT_FILE"`
- Continuously captures all terminal output
- Appends to file (preserves history)
- Includes ANSI escape codes

#### 3. Marker System

- Records byte position when message is sent
- `autx read` uses marker to show only new output
- Stored in `~/.cache/agent-unleashed/autx/SESSION.marker`

#### 4. Wait Detection

```
┌─────────────────────────────────────────────────────┐
│                  Wait Algorithm                      │
├─────────────────────────────────────────────────────┤
│  1. Record current file size                        │
│  2. Sleep for 1 second                              │
│  3. Compare new file size with previous             │
│  4. If same: increment stable counter               │
│     If different: reset stable counter              │
│  5. If stable counter >= 3: response complete       │
│  6. If elapsed time >= timeout: give up             │
│  7. Repeat from step 2                              │
└─────────────────────────────────────────────────────┘
```

### File Locations

| File | Path | Purpose |
|------|------|---------|
| Script | `scripts/autx` | Main autx executable |
| Output | `~/.cache/agent-unleashed/autx/{session}.output` | Captured terminal output |
| Marker | `~/.cache/agent-unleashed/autx/{session}.marker` | Byte position marker |

### Security Considerations

- Output files may contain sensitive information from Claude sessions
- Cache directory is user-readable only (`~/.cache/`)
- No credentials are stored by autx itself
- Consider clearing output files after sensitive operations:
  ```bash
  autx stop
  rm ~/.cache/agent-unleashed/autx/*
  ```

## Related Documentation

- [Restart and Refresh](./restart-refresh.md) - Auto mode and session management
- [Core Patches](./core-patches.md) - Claude Code modifications
- [Plugin Development](./plugin-development.md) - Extending Agent Unleashed
