#!/usr/bin/env python3
"""Patch Claude Code cli.js to replace LLM compaction with EITF.

Replaces the $s4 LLM API call in MW1 (main compaction function) with an
async IIFE that runs supercompact's EITF algorithm. Falls back to original
LLM compaction on any error.

Usage:
    python3 patcher.py <cli.js path> <supercompact dir> [--dry-run]
"""

import sys
from pathlib import Path


def build_old_pattern():
    """The exact string to find in cli.js (MW1's LLM call)."""
    return (
        'j=UOA(z),M=c6({content:j}),'
        'P=await $s4({messages:A,summaryRequest:M,appState:_,'
        'context:q,preCompactTokenCount:H,cacheSafeParams:K})'
    )


def build_new_pattern(supercompact_dir: str):
    """The EITF replacement IIFE.

    Flow:
    1. Find the session JSONL in ~/.claude/projects/<project>/
    2. Run compact.py --method eitf --format summary
    3. Return fake API response matching o51's expected format
    4. On any error, fall back to original $s4 LLM call
    """
    sc = supercompact_dir.replace('\\', '\\\\').replace('"', '\\"')

    return (
        'j=UOA(z),M=c6({content:j}),'
        'P=await(async()=>{'
        'try{'
        'const _cp=require("child_process"),'
        '_path=require("path"),'
        '_fs=require("fs");'
        'const _home=process.env.HOME||"/root";'
        'const _projDir=_path.join(_home,".claude","projects",'
        'process.cwd().replace(/\\//g,"-"));'
        'const _files=_fs.readdirSync(_projDir)'
        '.filter(f=>f.endsWith(".jsonl")&&!f.includes(".pre-supercompact")&&!f.includes(".backup"))'
        '.map(f=>({f,t:_fs.statSync(_path.join(_projDir,f)).mtimeMs}))'
        '.sort((a,b)=>b.t-a.t);'
        'if(!_files.length)throw new Error("SUPERCOMPACT_EITF:no_jsonl");'
        'const _jsonl=_path.join(_projDir,_files[0].f);'
        'const _budget=H||80000;'
        '_cp.execSync('
        '"cd \\"' + sc + '\\" && uv run python compact.py \\""+_jsonl+"\\" '
        '--method eitf --budget "+_budget+" --format summary '
        '--output /tmp/supercompact-eitf.txt",'
        '{timeout:30000,encoding:"utf8",stdio:["pipe","pipe","pipe"]});'
        'const _summary=_fs.readFileSync("/tmp/supercompact-eitf.txt","utf8");'
        'if(!_summary)throw new Error("SUPERCOMPACT_EITF:empty");'
        'return{type:"assistant",message:{role:"assistant",'
        'content:[{type:"text",text:_summary}]}}'
        '}catch(_e){'
        'return $s4({messages:A,summaryRequest:M,appState:_,'
        'context:q,preCompactTokenCount:H,cacheSafeParams:K})'
        '}})()'
    )


def main():
    if len(sys.argv) < 3:
        print(f"Usage: {sys.argv[0]} <cli.js> <supercompact_dir> [--dry-run]")
        sys.exit(1)

    cli_path = Path(sys.argv[1])
    supercompact_dir = sys.argv[2]
    dry_run = "--dry-run" in sys.argv

    if not cli_path.exists():
        print(f"Error: {cli_path} not found", file=sys.stderr)
        sys.exit(1)

    content = cli_path.read_text()

    # Check already patched
    if "SUPERCOMPACT_EITF" in content:
        print("Already patched (EITF compaction)")
        sys.exit(0)

    old = build_old_pattern()
    new = build_new_pattern(supercompact_dir)

    count = content.count(old)
    if count == 0:
        print("Error: anchor pattern not found in cli.js", file=sys.stderr)
        print("Claude Code version may have changed.", file=sys.stderr)
        sys.exit(1)
    if count != 1:
        print(f"Error: expected 1 match, found {count}", file=sys.stderr)
        sys.exit(1)

    content = content.replace(old, new, 1)

    # Verify
    if "SUPERCOMPACT_EITF" not in content:
        print("Error: verification failed - EITF marker not in output", file=sys.stderr)
        sys.exit(1)
    if old in content:
        print("Error: verification failed - original pattern still present", file=sys.stderr)
        sys.exit(1)

    if dry_run:
        print("Dry run - patch verified OK, not writing")
        # Show a snippet of the patched area
        idx = content.index("SUPERCOMPACT_EITF")
        print(f"  ...{content[idx-40:idx+60]}...")
        sys.exit(0)

    cli_path.write_text(content)
    print("Patch applied successfully")
    print(f"  EITF replaces LLM summarization (~0.2s vs ~30s)")
    print(f"  Falls back to LLM on any error")


if __name__ == "__main__":
    main()
