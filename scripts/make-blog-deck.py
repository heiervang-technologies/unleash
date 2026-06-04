#!/usr/bin/env python3
"""Build a single-file reveal.js deck for the unleash overhead benchmark.

Output is one HTML file with reveal.js pulled from CDN and both plot PNGs
base64-embedded. Drop on any static host (or paste into a Reddit/LinkedIn
share-link preview) without worrying about asset paths.
"""

from __future__ import annotations

import base64
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
BENCH = REPO / "docs" / "benchmarks"

PLOT_SURVEY = BENCH / "blog-agent-startup-survey.png"
PLOT_OVERHEAD = BENCH / "blog-wrapper-overhead.png"
OUT = BENCH / "blog-deck.html"


def b64(path: Path) -> str:
    return base64.b64encode(path.read_bytes()).decode("ascii")


SURVEY_DATA = f"data:image/png;base64,{b64(PLOT_SURVEY)}"
OVERHEAD_DATA = f"data:image/png;base64,{b64(PLOT_OVERHEAD)}"


HTML = r"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>I benchmarked 7 AI coding CLIs. The wrapper isn't the problem.</title>
<meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=1">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/reveal.js@5/dist/reveal.css">
<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/reveal.js@5/dist/theme/black.css">
<style>
  :root {
    --accent: #38bdf8;
    --accent-warm: #fb923c;
    --good: #22c55e;
    --bad: #ef4444;
    --muted: #94a3b8;
    --ink: #f8fafc;
  }
  .reveal { font-family: "Inter", "DejaVu Sans", sans-serif; }
  .reveal h1, .reveal h2, .reveal h3 { font-weight: 800; letter-spacing: -0.02em; text-transform: none; color: var(--ink); }
  .reveal h1 { font-size: 2.6em; line-height: 1.1; }
  .reveal h2 { font-size: 2.0em; line-height: 1.15; }
  .reveal h3 { font-size: 1.4em; color: var(--accent); }
  .reveal p, .reveal li { line-height: 1.45; }
  .reveal em { color: var(--accent); font-style: normal; font-weight: 700; }
  .reveal .lede { color: var(--muted); font-size: 0.85em; margin-top: -0.4em; }
  .reveal .big { font-size: 3.5em; font-weight: 900; line-height: 1; letter-spacing: -0.03em; }
  .reveal .huge { font-size: 5em; font-weight: 900; line-height: 1; letter-spacing: -0.04em; color: var(--accent); }
  .reveal .kicker { text-transform: uppercase; letter-spacing: 0.18em; font-size: 0.75em; color: var(--accent); font-weight: 700; margin-bottom: 0.5em; }
  .reveal .pill { display: inline-block; padding: 4px 14px; border: 1px solid var(--muted); border-radius: 999px; color: var(--muted); font-size: 0.7em; letter-spacing: 0.06em; }
  .reveal .footer { position: absolute; left: 24px; bottom: 14px; color: var(--muted); font-size: 0.55em; }
  .reveal section img.plot { width: 92%; max-height: 78vh; object-fit: contain; margin: 0 auto; box-shadow: 0 6px 24px rgba(0,0,0,0.35); border-radius: 8px; }
  .reveal table { border-collapse: collapse; margin: 0.4em auto; font-size: 0.7em; }
  .reveal table th, .reveal table td { padding: 8px 14px; border: none; }
  .reveal table thead th { color: var(--accent); border-bottom: 1px solid #334155; text-align: left; }
  .reveal table tbody tr { border-bottom: 1px solid #1e293b; }
  .reveal table td.num { font-variant-numeric: tabular-nums; text-align: right; }
  .reveal .grid7 { display: grid; grid-template-columns: repeat(7, 1fr); gap: 14px; margin: 0.5em 0 1em; }
  .reveal .grid7 .agent { background: #0f172a; border: 1px solid #1e293b; border-radius: 8px; padding: 14px 6px; text-align: center; font-weight: 700; color: var(--ink); }
  .reveal .grid7 .agent small { display: block; color: var(--muted); font-weight: 400; font-size: 0.7em; margin-top: 4px; }
  .reveal .twoup { display: grid; grid-template-columns: 1fr 1fr; gap: 32px; align-items: center; }
  .reveal .stat { background: #0f172a; border: 1px solid #1e293b; border-radius: 12px; padding: 26px; }
  .reveal .stat .n { font-size: 2.6em; font-weight: 900; line-height: 1; }
  .reveal .stat .n.bad { color: var(--bad); }
  .reveal .stat .n.good { color: var(--good); }
  .reveal .stat .label { color: var(--muted); font-size: 0.75em; margin-top: 8px; letter-spacing: 0.04em; }
  .reveal code, .reveal pre { font-family: "JetBrains Mono", "DejaVu Sans Mono", monospace; }
  .reveal code { background: #0f172a; border: 1px solid #1e293b; padding: 2px 8px; border-radius: 4px; color: var(--accent); }
  .reveal .pull { font-size: 1.3em; line-height: 1.3; color: var(--muted); border-left: 4px solid var(--accent); padding-left: 18px; text-align: left; max-width: 80%; margin: 0.5em auto; }
  .reveal .pull strong { color: var(--ink); font-weight: 700; }
  .reveal a { color: var(--accent); }
</style>
</head>
<body>
<div class="reveal">
<div class="slides">

<!-- 1. HOOK -->
<section data-background-color="#0b1220">
  <div class="kicker">A benchmark</div>
  <h1>I measured how much it costs<br>to wrap 7 AI coding CLIs<br>in a single launcher.</h1>
  <p class="lede" style="margin-top: 30px;">The number that surprised me was not the wrapper.</p>
  <div class="footer">scroll &darr;</div>
</section>

<!-- 2. CAST -->
<section data-background-color="#0b1220">
  <div class="kicker">The cast</div>
  <h2>Seven coding agents.<br>One Rust harness.</h2>
  <div class="grid7">
    <div class="agent">claude<small>Anthropic</small></div>
    <div class="agent">codex<small>OpenAI</small></div>
    <div class="agent">agy<small>Google Antigravity</small></div>
    <div class="agent">gemini<small>Google</small></div>
    <div class="agent">opencode<small>SST</small></div>
    <div class="agent">pi<small>Inception</small></div>
    <div class="agent">hermes<small>Nous Research</small></div>
  </div>
  <p style="color: var(--muted); font-size: 0.7em;">
    Wrapped by <code>unleash</code> &mdash; one binary that adds auto-mode,
    plugins, version pinning, and a TUI.
  </p>
</section>

<!-- 3. SURVEY -->
<section data-background-color="#0b1220">
  <div class="kicker">Question 1</div>
  <h2>How heavy are these CLIs before any wrapper?</h2>
  <img class="plot" src="__SURVEY__" alt="Agent startup survey">
</section>

<!-- 4. SURVEY TAKEAWAYS -->
<section data-background-color="#0b1220">
  <div class="kicker">Cold-start ecosystem</div>
  <div class="twoup">
    <div class="stat">
      <div class="n">960 ms</div>
      <div class="label">gemini &mdash; 2.2&times; the next slowest</div>
    </div>
    <div class="stat">
      <div class="n">215 MB</div>
      <div class="label">heaviest RAM: gemini</div>
    </div>
    <div class="stat">
      <div class="n">19 MB</div>
      <div class="label">lightest both ways: codex</div>
    </div>
    <div class="stat">
      <div class="n">70 ms</div>
      <div class="label">claude &mdash; <em>fast</em> at startup&hellip;</div>
    </div>
  </div>
  <p class="lede" style="font-size: 0.7em; margin-top: 12px;">Hold onto that last one.</p>
</section>

<!-- 5. WRAPPING IT -->
<section data-background-color="#0b1220">
  <div class="kicker">Question 2</div>
  <h2>Now wrap them.<br>How much does the launcher cost?</h2>
  <p class="pull" style="margin-top: 40px;">
    Naive test: time <code>unleash claude --version</code><br>
    against <code>claude --version</code>.
  </p>
</section>

<!-- 6. THE BAD NUMBER -->
<section data-background-color="#0b1220">
  <div class="kicker">First result</div>
  <h2>Six of seven launches are free.<br>One isn't.</h2>
  <div class="twoup" style="margin-top: 30px;">
    <div class="stat">
      <div class="n bad">+130 ms</div>
      <div class="label">claude wall-clock delta</div>
    </div>
    <div class="stat">
      <div class="n bad">+53 MB</div>
      <div class="label">claude resident memory delta</div>
    </div>
  </div>
  <p class="lede" style="margin-top: 24px;">
    Every other agent: &lt; 20 ms wall, &lt; &plusmn;1.5 MB RAM. So the wrapper
    is fine&hellip; <em>except for claude?</em>
  </p>
</section>

<!-- 7. PLOT TWIST -->
<section data-background-color="#0b1220">
  <div class="kicker">Plot twist</div>
  <h2 style="color: var(--accent-warm);">It isn't the wrapper.</h2>
  <p style="font-size: 1.2em; margin-top: 30px; color: var(--muted);">
    To run plugins, the wrapper passes <code>--dangerously-skip-permissions</code>
    and <code>--plugin-dir</code> to claude.<br>
    What happens if you pass either flag <em>directly?</em>
  </p>
</section>

<!-- 8. 2x2 FACTORIAL -->
<section data-background-color="#0b1220">
  <div class="kicker">2&times;2 factorial &mdash; <code>claude --version</code></div>
  <table>
    <thead>
      <tr><th>invocation</th><th>wall</th><th>RSS</th></tr>
    </thead>
    <tbody>
      <tr><td>bare</td><td class="num">70 ms</td><td class="num">212 MB</td></tr>
      <tr><td>+ <code>--dangerously-skip-permissions</code></td><td class="num">170 ms</td><td class="num">266 MB</td></tr>
      <tr><td>+ <code>--plugin-dir DIR</code></td><td class="num">170 ms</td><td class="num">266 MB</td></tr>
      <tr><td>+ both</td><td class="num">170 ms</td><td class="num">266 MB</td></tr>
    </tbody>
  </table>
  <p class="pull" style="margin-top: 24px;">
    <strong>Either flag alone</strong> sends claude down its plugin /
    permissions init path.<br>
    Both flags together cost the same as one. <em>Plugin count is irrelevant too.</em>
  </p>
</section>

<!-- 9. THE FIX FOR THE MEASUREMENT -->
<section data-background-color="#0b1220">
  <div class="kicker">Apples to apples</div>
  <h2>Re-measure with matched flags.</h2>
  <p style="color: var(--muted); margin-top: 28px;">
    Give the direct invocation the same flags the wrapper would add &mdash;
    isolating the launcher's <em>own</em> cost from claude's startup path.
  </p>
</section>

<!-- 10. OVERHEAD PLOT -->
<section data-background-color="#0b1220">
  <h2 style="color: var(--good); margin-bottom: 10px;">Wrapping is free.</h2>
  <img class="plot" src="__OVERHEAD__" alt="Wrapper overhead, apples-to-apples">
</section>

<!-- 11. STATEMENTS -->
<section data-background-color="#0b1220">
  <div class="kicker">Bottom line</div>
  <div class="twoup">
    <div class="stat">
      <div class="n good">&le; 20 ms</div>
      <div class="label">wrapper-only wall-clock cost &mdash; <em>all 7 agents</em></div>
    </div>
    <div class="stat">
      <div class="n good">&plusmn; 0.5 MB</div>
      <div class="label">wrapper-only RAM cost &mdash; <em>all 7 agents</em></div>
    </div>
  </div>
  <p class="pull" style="margin-top: 28px;">
    Every wrapped launch lands inside the grey band: at or under the
    measurement floor.
  </p>
</section>

<!-- 12. POSTSCRIPT -->
<section data-background-color="#0b1220">
  <div class="kicker">Postscript</div>
  <h2 style="color: var(--good);">And the +130 ms?<br>Gone too.</h2>
  <p style="font-size: 1.1em; margin-top: 28px; color: var(--muted); max-width: 80%; margin-left: auto; margin-right: auto;">
    Because claude's plugin-init tax fires the moment you pass
    <code>--dangerously-skip-permissions</code> or <code>--plugin-dir</code>,
    the wrapper now <em>short-circuits</em> meta commands &mdash;
    <code>--version</code>, <code>--help</code>, <code>doctor</code> &mdash;
    and execs the agent bare.
  </p>
  <div class="twoup" style="margin-top: 24px;">
    <div class="stat">
      <div class="n bad">+130 ms</div>
      <div class="label">before</div>
    </div>
    <div class="stat">
      <div class="n good">+0 ms</div>
      <div class="label">after &mdash; <code>unleash claude --version</code></div>
    </div>
  </div>
</section>

<!-- 13. TL;DR -->
<section data-background-color="#0b1220">
  <div class="kicker">TL;DR</div>
  <h2>Three things I learned.</h2>
  <ol style="font-size: 0.95em; max-width: 80%; margin: 1em auto; line-height: 1.7;">
    <li>A naive wrapper benchmark <em>blames the wrapper</em> for costs the agent itself pays.</li>
    <li>Matching flags is the only honest baseline. Always run the 2&times;2.</li>
    <li>Wrapping 7 production AI CLIs in a Rust harness <em>is essentially free</em> &mdash; the launch tax lives inside the agent, not the launcher.</li>
  </ol>
  <p class="lede" style="margin-top: 28px;">
    Code, raw data, and full report &mdash;
    <a href="https://github.com/heiervang-technologies/unleash">github.com/heiervang-technologies/unleash</a>
  </p>
  <div class="footer">scripts/bench-overhead.sh &middot; scripts/make-overhead-report.py</div>
</section>

</div>
</div>

<script src="https://cdn.jsdelivr.net/npm/reveal.js@5/dist/reveal.js"></script>
<script>
  Reveal.initialize({
    hash: true,
    transition: "fade",
    backgroundTransition: "fade",
    controls: true,
    progress: true,
    center: true,
    slideNumber: "c/t",
  });
</script>
</body>
</html>
"""


def main():
    html = HTML.replace("__SURVEY__", SURVEY_DATA).replace("__OVERHEAD__", OVERHEAD_DATA)
    OUT.write_text(html)
    size_kb = OUT.stat().st_size / 1024
    print(f"wrote {OUT.relative_to(REPO)} ({size_kb:.0f} KB)")


if __name__ == "__main__":
    main()
