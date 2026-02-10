---
description: EITF entity-preservation compaction (~400x faster than /compact, 2x better entity retention)
argument-hint: [budget]
allowed-tools: Bash(cd *), Bash(uv *), Bash(PROJECT_DIR*), Bash(JSONL_FILE*), Bash(ls *), Bash(wc *), Bash(cp *), Bash(mv *)
---

# Supercompact — EITF Entity-Preservation Compaction

**CRITICAL: Do NOT use the built-in /compact command. You must follow the exact steps below using Bash tool calls.**

You are running the supercompact EITF algorithm. This is completely separate from Claude Code's built-in /compact. You must execute the bash commands below, not delegate to any built-in compaction.

## Step 1: Find the conversation JSONL

```bash
PROJECT_DIR=$(echo "$PWD" | sed 's|/|-|g; s|^|/home/me/.claude/projects/|')
JSONL_FILE=$(ls -t "$PROJECT_DIR"/*.jsonl 2>/dev/null | head -1)
echo "JSONL: $JSONL_FILE"
wc -l "$JSONL_FILE"
```

## Step 2: Run EITF compaction

Budget: use $ARGUMENTS if provided, otherwise 80000.

```bash
cd /home/me/ht/supercompact && uv run python compact.py "$JSONL_FILE" --method eitf --budget ${BUDGET:-80000} --output /tmp/supercompact-output.jsonl --verbose
```

## Step 3: Replace the session JSONL

```bash
cp "$JSONL_FILE" "${JSONL_FILE}.pre-supercompact"
mv /tmp/supercompact-output.jsonl "$JSONL_FILE"
echo "Replaced session JSONL (backup: ${JSONL_FILE}.pre-supercompact)"
```

## Step 4: Report

Report: turns kept vs dropped, compression ratio, wall clock time. Tell the user the session JSONL has been compacted in-place and the backup is saved as `.pre-supercompact`.
