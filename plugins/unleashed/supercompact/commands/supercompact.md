---
description: EITF entity-preservation compaction (~400x faster than /compact, 2x better entity retention)
argument-hint: "[budget] [--method eitf|setcover|dedup]"
allowed-tools: Bash(cd *), Bash(uv *), Bash(PROJECT_DIR*), Bash(JSONL_FILE*), Bash(ls *), Bash(wc *), Bash(cp *), Bash(mv *), Bash(restart-claude*), Bash(echo *)
---

# Supercompact — Entity-Preservation Compaction

**CRITICAL: Do NOT use the built-in /compact command. You must follow the exact steps below using Bash tool calls.**

You are running the supercompact algorithm. This is completely separate from Claude Code's built-in /compact. You must execute the bash commands below, not delegate to any built-in compaction.

## Configuration

Settings come from environment variables (set via plugin config):
- `PLUGIN_SETTING_METHOD` — scoring method (default: `eitf`). Options: `eitf`, `setcover`, `dedup`
- `PLUGIN_SETTING_BUDGET` — token budget (default: `80000`)

The user can override these via arguments: `/supercompact 120000 --method setcover`

## Step 1: Find the conversation JSONL

```bash
PROJECT_DIR=$(echo "$PWD" | sed 's|/|-|g; s|^|/home/me/.claude/projects/|')
JSONL_FILE=$(ls -t "$PROJECT_DIR"/*.jsonl 2>/dev/null | head -1)
echo "JSONL: $JSONL_FILE"
wc -l "$JSONL_FILE"
```

## Step 2: Run compaction

Parse $ARGUMENTS for an optional numeric budget and `--method <name>`. Fall back to env vars, then defaults.

```bash
METHOD="${PLUGIN_SETTING_METHOD:-eitf}"
BUDGET="${PLUGIN_SETTING_BUDGET:-80000}"
# Override from arguments if provided (e.g. "/supercompact 120000 --method setcover")
for arg in $ARGUMENTS; do
  if [[ "$arg" =~ ^[0-9]+$ ]]; then BUDGET="$arg"; fi
  if [[ "$prev" == "--method" ]]; then METHOD="$arg"; fi
  prev="$arg"
done
echo "Method: $METHOD, Budget: $BUDGET"
cd /home/me/ht/supercompact && uv run python compact.py "$JSONL_FILE" --method "$METHOD" --budget "$BUDGET" --output /tmp/supercompact-output.jsonl --verbose
```

## Step 3: Replace the session JSONL

```bash
cp "$JSONL_FILE" "${JSONL_FILE}.pre-supercompact"
mv /tmp/supercompact-output.jsonl "$JSONL_FILE"
echo "Replaced session JSONL (backup: ${JSONL_FILE}.pre-supercompact)"
```

## Step 4: Report results briefly

Report: method used, turns kept vs dropped, compression ratio, wall clock time.

## Step 5: Restart to reload compacted context

The JSONL on disk is now compacted, but the live session still has old context in memory. Restart to load the compacted version:

```bash
restart-claude "Session compacted with supercompact ($METHOD). Restarting to load compacted context."
```

If `restart-claude` is not available (not running under agent-unleashed), tell the user: "Run `/quit` then `claude --resume` to load the compacted context."
