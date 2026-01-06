# Process Restart Plugin - Handoff Document

**Date**: 2026-01-06
**Session**: Investigating self-restart capability
**Issue**: https://github.com/heiervang-technologies/claude-unleashed/issues/7

## Current State

The process-restart plugin is **partially working**:
- Session state preservation works (when manually restarted)
- Session ID discovery from project files works
- The restart trigger script executes successfully
- **But**: Self-restart (Claude killing itself and spawning replacement) does not work

## What Was Attempted

### 1. Original Hook-Based Design
The plugin was designed to use Claude Code's Stop hook:
```
trigger-restart.sh creates trigger file
  → User runs /exit (or Claude exits)
  → Stop hook fires
  → restart-handler.sh detects trigger, spawns new process
```

**Finding**: Stop hooks don't fire when process is killed by SIGTERM. They only fire during graceful exit via `/exit` command.

### 2. Self-Contained Trigger Script
Rewrote `trigger-restart.sh` to handle everything:
1. Find session ID from `~/.claude/projects/-{path}/{session}.jsonl`
2. Save state to `~/.cache/claude-unleashed/process-restart/restart-state.json`
3. Spawn new Claude with `nohup claude --resume {session-id} &`
4. Kill current process with `kill -TERM`

**Finding**: New process doesn't survive. Possibly killed when parent dies despite nohup.

### 3. setsid for Full Detachment
Changed spawn command to use `setsid` for new session:
```bash
setsid "${CLAUDE_CMD}" "${CMD_ARGS[@]}" < /dev/null > /dev/null 2>&1 &
```

**Finding**: Still doesn't work. Process appears to spawn but doesn't take over.

### 4. tmux Approach (Blocked)
Idea: Run Claude inside tmux, use `tmux send-keys` to send `/exit`:
```bash
tmux send-keys -t claude "/exit" Enter
# Stop hook fires properly, restart-handler spawns new process
```

**Blocker**: tmux not installed, can't install because sudo is broken.

## Sandbox Issue

The Claude Code sandbox has modified system files:
```
/usr/bin/sudo     owned by nobody:nogroup (should be root:root with setuid)
/etc/sudo.conf    owned by nobody:nogroup (should be root:root)
```

This prevents using sudo even with `dangerouslyDisableSandbox: true`.

**To fix** (run outside Claude Code):
```bash
sudo chown root:root /usr/bin/sudo /etc/sudo.conf
sudo chmod 4755 /usr/bin/sudo
```

## Key Code Locations

| File | Purpose |
|------|---------|
| `scripts/trigger-restart.sh` | Main restart script - finds session, saves state, spawns new process, kills current |
| `hooks-handlers/restart-handler.sh` | Stop hook handler (works with graceful exit only) |
| `hooks-handlers/session-restore.sh` | SessionStart hook - restores state after restart |
| `commands/restart.md` | Skill documentation (symlinked to `~/.claude/skills/restarting/SKILL.md`) |

## Session ID Discovery

Successfully implemented fallback mechanism to find session ID:
```bash
# Convert /home/me/claude-unleashed to -home-me-claude-unleashed
PROJECT_PATH=$(echo "${WORKING_DIR}" | sed 's|^/||; s|/|-|g')
PROJECT_DIR="${HOME}/.claude/projects/-${PROJECT_PATH}"

# Find most recent session file (not agent files)
SESSION_FILE=$(find "${PROJECT_DIR}" -maxdepth 1 -name "*.jsonl" \
  ! -name "agent-*.jsonl" -type f -printf '%T@ %p\n' \
  | sort -rn | head -1 | cut -d' ' -f2-)

SESSION_ID=$(basename "${SESSION_FILE}" .jsonl)
```

## What Works

1. **Skill renamed**: `/restart` → `/restarting` (directory and frontmatter)
2. **Session discovery**: Can find current session ID from project files
3. **State file creation**: Saves proper JSON state to cache directory
4. **Manual restart preservation**: If user manually restarts with `--resume`, session continues

## What Doesn't Work

1. **Self-restart**: Claude cannot restart itself
2. **Hook firing on SIGTERM**: Hooks bypass when killed by signal
3. **Process detachment**: nohup/setsid don't properly detach new process
4. **sudo**: Broken by sandbox, blocking tmux installation

## Recommended Next Steps

1. **Fix sudo** (manual step outside Claude Code)
2. **Install tmux**: `sudo apt-get install tmux`
3. **Test tmux approach**:
   ```bash
   # Start Claude in tmux
   tmux new-session -s claude "claude"

   # From inside, restart should work via:
   tmux send-keys -t claude "/exit" Enter
   ```
4. **If tmux works**, update trigger-restart.sh to detect tmux and use send-keys
5. **Consider alternatives**:
   - systemd user service for restart management
   - Double-fork daemon pattern
   - WebSocket/IPC for internal restart command

## Password Test

During this session, a password test was conducted:
- Password: `purple-elephant-42`
- Purpose: Test if memory persists across restart
- Result: Session history preserved (password visible in conversation), but self-restart mechanism failed

## Files Modified (Uncommitted)

```
modified:   plugins/unleashed/process-restart/commands/restart.md
modified:   plugins/unleashed/process-restart/hooks-handlers/restart-handler.sh
modified:   plugins/unleashed/process-restart/scripts/trigger-restart.sh
```
