# Headless Mode (unleashtx)

`unleashtx` (Unleash tmux eXecutor) provides a headless mode for running Claude Code in the background using tmux as the session manager. This approach offers significant advantages over native headless modes for automation, scripting, and CI/CD integration.

## Overview

### What is unleashtx?

`unleashtx` is a wrapper script that runs Claude Code inside a tmux session, enabling:

- **Headless operation**: Run Claude without an interactive terminal
- **Session persistence**: Sessions survive disconnects and can be reattached
- **Programmatic interaction**: Send messages and read responses via commands
- **Background processing**: Let Claude work while you do other things

### Why unleashtx Over Native Headless?

| Feature | unleashtx | Native Headless |
|---------|------|-----------------|
| Session persistence | Yes (tmux-based) | No |
| Attach/detach mid-session | Yes | No |
| Output logging | Automatic | Manual |
| Multiple parallel sessions | Yes (via session names) | Limited |
| Works with unleash | Yes | Partial |
| Response detection | File-size heuristic | Varies |
| Debugging | Attach and inspect | Difficult |

### Key Benefits

1. **Attach Anytime**: Start a headless session, then attach to see what Claude is doing
2. **Persistent Logs**: All output is captured to `~/.cache/unleash/unleashtx/`
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

`unleashtx` is installed automatically when you run the main installation script:

```bash
./scripts/install.sh
```

This creates a symlink at `~/.local/bin/unleashtx` pointing to `scripts/unleashtx`.

### Manual Installation

If you need to install manually:

```bash
# From the unleash directory
ln -sf "$(pwd)/scripts/unleashtx" ~/.local/bin/unleashtx

# Ensure ~/.local/bin is in your PATH
echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

### Verify Installation

```bash
unleashtx help
```

## Commands Reference

### `unleashtx start [args]`

Start a new Claude session in a detached tmux session.

**Usage:**
```bash
unleashtx start [--auto|-a] [-d|--daemon] [claude-args...]
```

**Options:**
- `--auto`, `-a`: Enable auto mode (sets `CLAUDE_AUTO_MODE=1`)
- `-d`, `--daemon`: Kill the tmux session when Claude exits
- All other arguments are passed through to Claude

**Examples:**
```bash
# Start a basic session
unleashtx start

# Start with auto mode enabled
unleashtx start --auto

# Start in daemon mode (session closes when Claude exits)
unleashtx start -d

# Start with a specific project directory
unleashtx start /path/to/project

# Continue a previous session
unleashtx start --continue

# Combine options
unleashtx start --auto -d --continue
```

**Notes:**
- If a session already exists, the command will fail (use `unleashtx stop` first)
- The session name defaults to `unleash` (configurable via environment)
- Output is automatically logged to `~/.cache/unleash/unleashtx/`

---

### `unleashtx send "message"`

Send a message to the running Claude session.

**Usage:**
```bash
unleashtx send "your message here"
```

**Examples:**
```bash
# Send a simple message
unleashtx send "Hello Claude"

# Send a multi-line message
unleashtx send "Review this code:
def hello():
    print('world')
"

# Send content from a file
unleashtx send "Analyze this: $(cat file.py)"

# Send command output
unleashtx send "Explain this error: $(npm test 2>&1)"
```

**Notes:**
- The command records a marker to track new output since the message was sent
- Use `unleashtx wait` after sending to wait for the response
- Special characters are passed through to tmux

---

### `unleashtx read`

Read the output from Claude.

**Usage:**
```bash
unleashtx read
```

**Behavior:**
- If a message was recently sent (marker exists), shows only output since that message
- If no marker exists, shows all output from the session
- Output includes ANSI escape codes (pipe through `cat -v` to see them)

**Examples:**
```bash
# Read current output
unleashtx read

# Read and save to file
unleashtx read > claude-response.txt

# Read and strip ANSI codes
unleashtx read | sed 's/\x1b\[[0-9;]*m//g'

# Read and extract specific content
unleashtx read | grep -A 10 "Summary:"
```

---

### `unleashtx wait [timeout]`

Wait for Claude to finish responding.

**Usage:**
```bash
unleashtx wait [timeout_seconds]
```

**Arguments:**
- `timeout_seconds`: Maximum time to wait (default: 300 seconds / 5 minutes)

**Detection Method:**
The command considers a response complete when the output file size remains stable for 3 consecutive seconds (the "stable threshold").

**Examples:**
```bash
# Wait with default timeout (300s)
unleashtx wait

# Wait with custom timeout
unleashtx wait 60

# Wait for a long operation
unleashtx wait 600
```

**Exit Codes:**
- `0`: Response completed
- `1`: Timeout reached

---

### `unleashtx attach [--here]`

Attach to the running Claude tmux session.

**Usage:**
```bash
unleashtx attach [--here|-h]
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
unleashtx attach

# Join Claude pane into current window (when already in tmux)
unleashtx attach --here
```

---

### `unleashtx stop`

Stop the Claude session and clean up.

**Usage:**
```bash
unleashtx stop
```

**Actions:**
1. Kills the tmux session
2. Removes the output file
3. Removes the marker file

**Examples:**
```bash
# Stop the session
unleashtx stop

# Force stop and restart
unleashtx stop && unleashtx start
```

---

### `unleashtx status`

Check if a session is running and display information.

**Usage:**
```bash
unleashtx status
```

**Output includes:**
- Session running status (green/red indicator)
- Session details (windows, creation time)
- Last 10 lines of output (if available)

**Example Output:**
```
[unleashtx] Session 'unleash' is running
unleash: 1 windows, created Sat Jan 10 14:30:00 2025

Recent output (last 10 lines):
─────────────────────────────
Claude: I've finished analyzing the code. Here are my findings...
```

---

### `unleashtx "message"` (Shorthand)

Send a message and wait for the response in one command.

**Usage:**
```bash
unleashtx "your message here"
```

**Behavior:**
1. Starts a new session if none exists (waits 3 seconds for initialization)
2. Sends the message
3. Waits for the response
4. Prints the response

**Examples:**
```bash
# Quick query
unleashtx "What is the capital of France?"

# Analyze a file
unleashtx "Review this code for bugs: $(cat main.py)"

# Get a summary
unleashtx "Summarize the key points in README.md"
```

**Notes:**
- This is equivalent to: `unleashtx start; unleashtx send "msg"; unleashtx wait; unleashtx read`
- Convenient for one-off queries
- Session remains running after the command (use `unleashtx stop` to clean up)

---

### `unleashtx help`

Display the help message with all commands and options.

**Usage:**
```bash
unleashtx help
# or
unleashtx --help
unleashtx -h
```

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `AUTX_SESSION_NAME` | `unleash` | tmux session name. Change this to run multiple parallel sessions. |
| `AUTX_WAIT_TIMEOUT` | `300` | Default timeout in seconds for `unleashtx wait` command. |

### Configuration Examples

```bash
# Run multiple parallel sessions
AUTX_SESSION_NAME=project-a unleashtx start /path/to/project-a
AUTX_SESSION_NAME=project-b unleashtx start /path/to/project-b

# Set longer default timeout for complex operations
export AUTX_WAIT_TIMEOUT=600
unleashtx "Refactor this entire codebase..."

# Use in scripts with custom session
export AUTX_SESSION_NAME="ci-claude-${BUILD_ID}"
unleashtx start -d
unleashtx send "Review PR #${PR_NUMBER}"
unleashtx wait 120
unleashtx read
# Session auto-closes due to -d flag
```

### Internal Configuration

These values are set in the script and control internal behavior:

| Setting | Value | Description |
|---------|-------|-------------|
| `CACHE_DIR` | `~/.cache/unleash/unleashtx` | Directory for output and marker files |
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

      - name: Install Unleash
        run: |
          git clone https://github.com/heiervang-technologies/unleash.git
          cd unleash
          ./scripts/install.sh

      - name: Review PR
        env:
          ANTHROPIC_API_KEY: ${{ secrets.ANTHROPIC_API_KEY }}
        run: |
          # Start Claude in daemon mode
          unleashtx start -d

          # Send review request
          unleashtx send "Review this PR for security issues, bugs, and code quality:

          $(git diff origin/main...HEAD)

          Focus on:
          1. Security vulnerabilities
          2. Logic errors
          3. Performance issues
          4. Code style"

          # Wait for response
          unleashtx wait 180

          # Save review
          unleashtx read > review.txt

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
    - unleashtx start -d
    - unleashtx send "Review the changes in this MR: $(git diff origin/main)"
    - unleashtx wait 120
    - unleashtx read > review.txt
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
    unleashtx start -d
    unleashtx send "Analyze these recent commits for potential issues:

$COMMITS

$(git diff HEAD~10..HEAD)

Provide a summary of:
1. Any concerning changes
2. Suggested improvements
3. Documentation that might need updates"

    unleashtx wait 300
    unleashtx read > "$OUTPUT_DIR/review-$DATE.txt"
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

unleashtx start -d
unleashtx send "Audit the project dependencies:

package.json:
$(cat package.json)

package-lock.json (summary):
$(jq '.packages | keys | length' package-lock.json) packages

Check for:
1. Known vulnerabilities
2. Outdated packages
3. Unnecessary dependencies"

unleashtx wait 180
AUDIT=$(unleashtx read)

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
unleashtx status >/dev/null 2>&1 || unleashtx start

# Generate report
unleashtx send "Generate a comprehensive summary of the following data:

$(cat "$INPUT_FILE")

Format the output as:
1. Executive Summary (2-3 sentences)
2. Key Findings (bullet points)
3. Recommendations (numbered list)"

unleashtx wait 120
unleashtx read > "$OUTPUT_FILE"

echo "Report saved to $OUTPUT_FILE"
```

#### Batch Processing

```bash
#!/bin/bash
# process-files.sh

OUTPUT_DIR="./summaries"
mkdir -p "$OUTPUT_DIR"

unleashtx start

for file in ./documents/*.txt; do
    filename=$(basename "$file" .txt)

    unleashtx send "Summarize this document in 3 bullet points:

$(cat "$file")"

    unleashtx wait 60
    unleashtx read > "$OUTPUT_DIR/${filename}-summary.txt"

    echo "Processed: $file"
done

unleashtx stop
echo "All files processed"
```

#### Interactive Script with User Input

```bash
#!/bin/bash
# interactive-claude.sh

echo "Starting Claude session..."
unleashtx start

while true; do
    echo ""
    read -p "You: " message

    if [ "$message" = "quit" ] || [ "$message" = "exit" ]; then
        break
    fi

    unleashtx send "$message"
    unleashtx wait
    echo ""
    echo "Claude:"
    unleashtx read
done

unleashtx stop
echo "Session ended"
```

### Parallel Sessions

Run multiple Claude instances for different tasks:

```bash
#!/bin/bash
# parallel-review.sh

# Start sessions for different aspects
AUTX_SESSION_NAME=security-review unleashtx start -d
AUTX_SESSION_NAME=performance-review unleashtx start -d
AUTX_SESSION_NAME=style-review unleashtx start -d

CODE=$(cat src/main.py)

# Send requests in parallel
AUTX_SESSION_NAME=security-review unleashtx send "Review for security: $CODE"
AUTX_SESSION_NAME=performance-review unleashtx send "Review for performance: $CODE"
AUTX_SESSION_NAME=style-review unleashtx send "Review for code style: $CODE"

# Wait for all
AUTX_SESSION_NAME=security-review unleashtx wait &
AUTX_SESSION_NAME=performance-review unleashtx wait &
AUTX_SESSION_NAME=style-review unleashtx wait &
wait

# Collect results
echo "=== Security Review ===" > full-review.txt
AUTX_SESSION_NAME=security-review unleashtx read >> full-review.txt

echo "=== Performance Review ===" >> full-review.txt
AUTX_SESSION_NAME=performance-review unleashtx read >> full-review.txt

echo "=== Style Review ===" >> full-review.txt
AUTX_SESSION_NAME=style-review unleashtx read >> full-review.txt

# Sessions auto-close due to -d flag
```

## Limitations

### Response Detection is Heuristic-Based

The `unleashtx wait` command detects response completion by monitoring output file size stability. This approach has limitations:

- **False positives**: If Claude pauses while thinking, it might be detected as "done"
- **Long operations**: Extended tool use or file operations may need longer timeouts
- **No semantic understanding**: The detection doesn't know if Claude is mid-sentence

**Mitigations:**
- Increase `AUTX_WAIT_TIMEOUT` for complex operations
- Increase the stable threshold by modifying the script
- Use `unleashtx attach` to visually verify completion

### Single Session Per Name

Only one session can run per session name at a time.

**Workaround:**
```bash
# Use different session names for parallel work
AUTX_SESSION_NAME=project-a unleashtx start
AUTX_SESSION_NAME=project-b unleashtx start
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
unleashtx read | sed 's/\x1b\[[0-9;]*m//g'
```

### No Built-in JSON Output

For programmatic use, you may need to parse Claude's text output.

**Suggestion:** Ask Claude to format output as JSON:
```bash
unleashtx send "List the files. Output as JSON array only, no other text."
```

## Troubleshooting

### Session Won't Start

**Symptoms:**
- "Session already exists" error
- Command hangs

**Solutions:**
```bash
# Check for existing session
unleashtx status

# Force stop and restart
unleashtx stop
unleashtx start

# Check tmux directly
tmux list-sessions

# Kill orphaned session
tmux kill-session -t unleash
```

### No Output from `unleashtx read`

**Symptoms:**
- Command returns empty
- Response seems missing

**Solutions:**
```bash
# Check if output file exists
ls -la ~/.cache/unleash/unleashtx/

# View raw output file
cat ~/.cache/unleash/unleashtx/unleash.output

# Check if Claude is still running
unleashtx status

# Attach to see what's happening
unleashtx attach
```

### Response Appears Incomplete

**Symptoms:**
- Output cuts off mid-sentence
- `wait` returns before Claude finishes

**Solutions:**
```bash
# Increase wait timeout
unleashtx wait 600

# Or set environment variable
export AUTX_WAIT_TIMEOUT=600
unleashtx wait

# Check if Claude is still processing
unleashtx attach
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
unleashtx attach

# Send Ctrl+C to interrupt
tmux send-keys -t unleash C-c

# Check Claude process
ps aux | grep claude

# Restart session
unleashtx stop
unleashtx start
```

### Output File Growing Too Large

**Symptoms:**
- Disk space warnings
- Slow `unleashtx read`

**Solutions:**
```bash
# Check file size
du -h ~/.cache/unleash/unleashtx/

# Clear and restart
unleashtx stop
rm ~/.cache/unleash/unleashtx/*
unleashtx start
```

## Architecture

### How unleashtx Works

```
┌─────────────────────────────────────────────────────────────────┐
│                         unleashtx Command                             │
├─────────────────────────────────────────────────────────────────┤
│  unleashtx start     │  Creates tmux session, starts Claude          │
│  unleashtx send      │  Sends keystrokes to tmux session             │
│  unleashtx read      │  Reads from output file                       │
│  unleashtx wait      │  Polls output file size                       │
│  unleashtx attach    │  Connects terminal to tmux session            │
│  unleashtx stop      │  Kills tmux session                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                       tmux Session                               │
│  Session: unleash (configurable)                       │
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
- `unleashtx read` uses marker to show only new output
- Stored in `~/.cache/unleash/unleashtx/SESSION.marker`

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
| Script | `scripts/unleashtx` | Main unleashtx executable |
| Output | `~/.cache/unleash/unleashtx/{session}.output` | Captured terminal output |
| Marker | `~/.cache/unleash/unleashtx/{session}.marker` | Byte position marker |

### Security Considerations

- Output files may contain sensitive information from Claude sessions
- Cache directory is user-readable only (`~/.cache/`)
- No credentials are stored by unleashtx itself
- Consider clearing output files after sensitive operations:
  ```bash
  unleashtx stop
  rm ~/.cache/unleash/unleashtx/*
  ```

## Related Documentation

- [Restart and Refresh](./restart-refresh.md) - Auto mode and session management
- [Plugin Development](./plugin-development.md) - Extending Unleash
