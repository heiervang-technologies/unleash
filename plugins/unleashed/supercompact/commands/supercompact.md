---
name: supercompact
description: Compact conversation using EITF entity-preservation scoring (faster & better than /compact)
argument-hint: "[budget]"
allowed-tools: Bash(uv:*, python:*, ls:*, find:*, cat:*, wc:*)
---

# Supercompact — EITF Entity-Preservation Compaction

You are performing conversation compaction using the supercompact EITF algorithm instead of the built-in /compact. This method is ~400x faster (0.2s vs 80s) and preserves 2x more entities.

## Step 1: Find the current conversation JSONL

The conversation JSONL files live at `~/.claude/projects/`. Find the correct project directory by converting the current working directory to a path key (replace `/` with `-`), then find the most recently modified `.jsonl` file in that directory.

```bash
PROJECT_DIR=$(echo "$PWD" | sed 's|/|-|g; s|^|/home/me/.claude/projects/|')
JSONL_FILE=$(ls -t "$PROJECT_DIR"/*.jsonl 2>/dev/null | head -1)
echo "JSONL: $JSONL_FILE"
wc -l "$JSONL_FILE"
```

## Step 2: Run EITF compaction

Set the token budget. If the user provided an argument, use that: $ARGUMENTS. Otherwise default to 80000.

```bash
cd /home/me/ht/supercompact && uv run python compact.py "$JSONL_FILE" --method eitf --budget ${BUDGET:-80000} --output /tmp/supercompact-output.jsonl --verbose
```

## Step 3: Replace the session JSONL

After compaction completes successfully, replace the original session file with the compacted version:

```bash
cp "$JSONL_FILE" "${JSONL_FILE}.pre-supercompact"
mv /tmp/supercompact-output.jsonl "$JSONL_FILE"
```

## Step 4: Report results

After compaction completes, report:
- How many turns were kept vs dropped
- Token compression ratio (percentage kept/reduced)
- Which high-scoring turns were preserved
- Wall clock time

Tell the user the session JSONL has been compacted in-place (backup saved as `.pre-supercompact`).
