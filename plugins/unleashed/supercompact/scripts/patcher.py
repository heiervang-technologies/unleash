#!/usr/bin/env python3
"""Patch Claude Code cli.js to replace LLM compaction with EITF.

Replaces the LLM API call in MW1 (main compaction function) with an
async IIFE that runs supercompact's EITF algorithm. Falls back to original
LLM compaction on any error.

Uses regex to adapt to minified variable name changes across versions.
The structural pattern and keyword argument names are stable across versions.

Usage:
    python3 patcher.py <cli.js path> <supercompact dir> [--dry-run]
"""

import re
import sys
from pathlib import Path


# Structural regex for MW1's compaction LLM call.
# Captures the minified variable/function names so we can reuse them.
#
# Pattern (readable):
#   <promptVar>=<promptFn>(<promptArg>),
#   <msgVar>=<msgFn>({content:<promptVar>}),
#   <respVar>=await <llmFn>({
#     messages:<msgsVar>,summaryRequest:<msgVar>,appState:<appVar>,
#     context:<ctxVar>,preCompactTokenCount:<tokVar>,cacheSafeParams:<cacheVar>
#   })
#
# The keyword arg names (messages, summaryRequest, etc.) and the string
# "Your task is to create a detailed summary of the conversation so far"
# (in the prompt function) are stable across versions.
MW1_PATTERN = re.compile(
    r'(?P<prompt_var>\w+)=(?P<prompt_fn>\w+)\((?P<prompt_arg>\w+)\),'
    r'(?P<msg_var>\w+)=(?P<msg_fn>\w+)\(\{content:(?P=prompt_var)\}\),'
    r'(?P<resp_var>\w+)=await (?P<llm_fn>[^\(]+)\(\{'
    r'messages:(?P<msgs_var>\w+),'
    r'summaryRequest:(?P=msg_var),'
    r'appState:(?P<app_var>[^,]+),'
    r'context:(?P<ctx_var>\w+),'
    r'preCompactTokenCount:(?P<tok_var>\w+),'
    r'cacheSafeParams:(?P<cache_var>\w+)'
    r'\}\)'
)

# The prompt function for MW1 contains this unique string (vs TE7/YR7 for partial compact)
MW1_PROMPT_MARKER = "Your task is to create a detailed summary of the conversation so far"


def find_mw1_match(content: str) -> re.Match | None:
    """Find the MW1 compaction LLM call, distinguishing it from mZ6."""
    for m in MW1_PATTERN.finditer(content):
        # Verify this is MW1's prompt function (not mZ6's)
        prompt_fn = m.group("prompt_fn")
        # Find the function definition and check it contains the MW1 marker
        fn_def_pattern = f"function {re.escape(prompt_fn)}("
        fn_idx = content.find(fn_def_pattern)
        if fn_idx >= 0:
            # Check next ~500 chars for the marker
            fn_snippet = content[fn_idx:fn_idx + 500]
            if MW1_PROMPT_MARKER in fn_snippet:
                return m
    return None


def build_replacement(m: re.Match, supercompact_dir: str) -> str:
    """Build the EITF replacement using captured variable names."""
    sc = supercompact_dir.replace('\\', '\\\\').replace('"', '\\"')

    prompt_var = m.group("prompt_var")
    prompt_fn = m.group("prompt_fn")
    prompt_arg = m.group("prompt_arg")
    msg_var = m.group("msg_var")
    msg_fn = m.group("msg_fn")
    resp_var = m.group("resp_var")
    llm_fn = m.group("llm_fn")
    msgs_var = m.group("msgs_var")
    app_var = m.group("app_var")
    ctx_var = m.group("ctx_var")
    tok_var = m.group("tok_var")
    cache_var = m.group("cache_var")

    # Build the original call args for fallback
    orig_args = (
        f"messages:{msgs_var},"
        f"summaryRequest:{msg_var},"
        f"appState:{app_var},"
        f"context:{ctx_var},"
        f"preCompactTokenCount:{tok_var},"
        f"cacheSafeParams:{cache_var}"
    )

    return (
        # Keep the prompt/message creation (harmless, may be used by telemetry)
        f'{prompt_var}={prompt_fn}({prompt_arg}),'
        f'{msg_var}={msg_fn}({{content:{prompt_var}}}),'
        # Replace LLM call with EITF IIFE
        f'{resp_var}=await(async()=>{{'
        f'try{{'
        f'const _cp=require("child_process"),'
        f'_path=require("path"),'
        f'_fs=require("fs");'
        f'const _home=process.env.HOME||"/root";'
        f'const _projDir=_path.join(_home,".claude","projects",'
        f'process.cwd().replace(/\\//g,"-"));'
        f'const _files=_fs.readdirSync(_projDir)'
        f'.filter(f=>f.endsWith(".jsonl")&&!f.includes(".pre-supercompact")&&!f.includes(".backup"))'
        f'.map(f=>({{f,t:_fs.statSync(_path.join(_projDir,f)).mtimeMs}}))'
        f'.sort((a,b)=>b.t-a.t);'
        f'if(!_files.length)throw new Error("SUPERCOMPACT_EITF:no_jsonl");'
        f'const _jsonl=_path.join(_projDir,_files[0].f);'
        f'const _budget={tok_var}||80000;'
        f'_cp.execSync('
        f'"cd \\"{sc}\\" && uv run python compact.py \\""+_jsonl+"\\" '
        f'--method eitf --budget "+_budget+" --format summary '
        f'--output /tmp/supercompact-eitf.txt",'
        f'{{timeout:30000,encoding:"utf8",stdio:["pipe","pipe","pipe"]}});'
        f'const _summary=_fs.readFileSync("/tmp/supercompact-eitf.txt","utf8");'
        f'if(!_summary)throw new Error("SUPERCOMPACT_EITF:empty");'
        f'return{{type:"assistant",message:{{role:"assistant",'
        f'content:[{{type:"text",text:_summary}}]}}}}'
        # Fallback to original LLM call on any error
        f'}}catch(_e){{'
        f'return {llm_fn}({{{orig_args}}})'
        f'}}}})()'
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

    if "SUPERCOMPACT_EITF" in content:
        print("Already patched (EITF compaction)")
        sys.exit(0)

    m = find_mw1_match(content)
    if m is None:
        print("Error: MW1 compaction pattern not found in cli.js", file=sys.stderr)
        print("The structural pattern may have changed.", file=sys.stderr)
        sys.exit(1)

    old = m.group(0)
    new = build_replacement(m, supercompact_dir)

    print(f"Found MW1 at offset {m.start()}")
    print(f"  LLM fn: {m.group('llm_fn')}, msg fn: {m.group('msg_fn')}, "
          f"prompt fn: {m.group('prompt_fn')}")

    content = content[:m.start()] + new + content[m.end():]

    # Verify
    if "SUPERCOMPACT_EITF" not in content:
        print("Error: verification failed - EITF marker not in output", file=sys.stderr)
        sys.exit(1)
    if old in content:
        print("Error: verification failed - original pattern still present", file=sys.stderr)
        sys.exit(1)

    if dry_run:
        print("Dry run - patch verified OK, not writing")
        idx = content.index("SUPERCOMPACT_EITF")
        print(f"  ...{content[idx-50:idx+70]}...")
        sys.exit(0)

    cli_path.write_text(content)
    print("Patch applied successfully")
    print(f"  EITF replaces LLM summarization (~0.2s vs ~30s)")
    print(f"  Falls back to LLM on any error")


if __name__ == "__main__":
    main()
