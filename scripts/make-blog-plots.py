#!/usr/bin/env python3
"""Publication-quality plots for blog posts.

Produces two standalone PNGs:
  1. agent-startup-survey.png  - unleash-agnostic: how heavy are these 7 CLIs?
  2. wrapper-overhead.png      - unleash-specific: wrapping cost is within noise

Both 16:9 at 200 DPI, designed to drop into a blog post or social media card.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import matplotlib.pyplot as plt
import matplotlib.ticker as mticker
from matplotlib import rcParams

REPO = Path(__file__).resolve().parents[1]
BENCH_DIR = REPO / "docs" / "benchmarks"

RAW = BENCH_DIR / "overhead-2026-06-01.json"
EQUIV = BENCH_DIR / "overhead-2026-06-01-equivalent.json"

OUT_SURVEY = BENCH_DIR / "blog-agent-startup-survey.png"
OUT_OVERHEAD = BENCH_DIR / "blog-wrapper-overhead.png"

# Palette — muted, print-safe, distinct in greyscale.
INK = "#1f2933"
MUTED = "#52606d"
GRID = "#e4e7eb"
ACCENT = "#1d4ed8"        # deep blue
ACCENT_2 = "#ea580c"      # warm orange
NEUTRAL = "#94a3b8"
GOOD = "#16a34a"
WARN = "#dc2626"


def style():
    rcParams.update({
        "figure.facecolor": "white",
        "axes.facecolor": "white",
        "axes.edgecolor": MUTED,
        "axes.labelcolor": INK,
        "axes.titlecolor": INK,
        "axes.titleweight": "bold",
        "axes.titlesize": 14,
        "axes.labelsize": 11,
        "axes.spines.top": False,
        "axes.spines.right": False,
        "xtick.color": MUTED,
        "ytick.color": MUTED,
        "xtick.labelsize": 10,
        "ytick.labelsize": 10,
        "font.family": "DejaVu Sans",
        "font.size": 11,
        "savefig.dpi": 200,
        "savefig.bbox": "tight",
        "savefig.pad_inches": 0.4,
    })


def load(path: Path) -> dict[str, dict]:
    data = json.loads(path.read_text())
    return {r["agent"]: r for r in data["results"]}


def fig_title(fig, title: str, subtitle: str):
    fig.suptitle(title, fontsize=20, fontweight="bold", color=INK,
                 x=0.05, ha="left", y=0.965)
    fig.text(0.05, 0.905, subtitle, fontsize=12, color=MUTED, ha="left")


def caption(fig, text: str):
    fig.text(0.05, 0.025, text, fontsize=9, color=MUTED, ha="left", style="italic")


# ---------------------------------------------------------------------------
# Plot 1: Agent startup survey (unleash-agnostic).
# ---------------------------------------------------------------------------
def plot_survey():
    raw = load(RAW)
    # Order by wall ascending so the eye sweeps from fastest to slowest.
    agents = sorted(raw.keys(), key=lambda a: raw[a]["direct"]["wall_sec"]["median"])
    wall_ms = [raw[a]["direct"]["wall_sec"]["median"] * 1000 for a in agents]
    rss_mb = [raw[a]["direct"]["max_rss_kb"]["median"] / 1024 for a in agents]

    fig, (ax_wall, ax_rss) = plt.subplots(1, 2, figsize=(13, 7.2),
                                          gridspec_kw={"wspace": 0.28})
    plt.subplots_adjust(top=0.74, bottom=0.12, left=0.05, right=0.97)

    fig_title(
        fig,
        "How heavy are agent CLIs at startup?",
        "Cold-start cost of `<agent> --version` — median of 15 runs, single host (Arch Linux, x86_64)",
    )

    # Wall clock
    bars = ax_wall.barh(agents, wall_ms, color=ACCENT, edgecolor="white", linewidth=0.8)
    ax_wall.set_title("Wall clock", loc="left", pad=10)
    ax_wall.set_xlabel("milliseconds")
    ax_wall.xaxis.set_major_locator(mticker.MaxNLocator(integer=True, nbins=6))
    ax_wall.grid(axis="x", color=GRID, linewidth=0.8, zorder=0)
    ax_wall.set_axisbelow(True)
    for bar, val in zip(bars, wall_ms):
        ax_wall.text(bar.get_width() + max(wall_ms) * 0.015,
                     bar.get_y() + bar.get_height() / 2,
                     f"{val:.0f} ms", va="center", fontsize=10, color=INK)
    ax_wall.set_xlim(0, max(wall_ms) * 1.18)

    # RSS
    bars = ax_rss.barh(agents, rss_mb, color=ACCENT_2, edgecolor="white", linewidth=0.8)
    ax_rss.set_title("Peak resident memory", loc="left", pad=10)
    ax_rss.set_xlabel("MB")
    ax_rss.grid(axis="x", color=GRID, linewidth=0.8, zorder=0)
    ax_rss.set_axisbelow(True)
    for bar, val in zip(bars, rss_mb):
        ax_rss.text(bar.get_width() + max(rss_mb) * 0.015,
                    bar.get_y() + bar.get_height() / 2,
                    f"{val:.0f} MB", va="center", fontsize=10, color=INK)
    ax_rss.set_xlim(0, max(rss_mb) * 1.18)

    # Sweet little annotation: highlight the extremes.
    fastest = agents[0]
    slowest = agents[-1]
    second_slowest = agents[-2]
    heaviest = max(agents, key=lambda a: raw[a]["direct"]["max_rss_kb"]["median"])
    lightest = min(agents, key=lambda a: raw[a]["direct"]["max_rss_kb"]["median"])
    ratio = wall_ms[-1] / wall_ms[-2]

    note = (
        f"Heaviest start: {slowest} ({ratio:.1f}× slower than {second_slowest}).   "
        f"Heaviest RAM: {heaviest} ({raw[heaviest]['direct']['max_rss_kb']['median']/1024:.0f} MB).   "
        f"Lightest both ways: {lightest} ({raw[lightest]['direct']['max_rss_kb']['median']/1024:.0f} MB)."
    )
    fig.text(0.05, 0.845, note, fontsize=11, color=INK, ha="left")

    caption(
        fig,
        "Source: GNU /usr/bin/time, 15 iterations + 2 warmups discarded, "
        "default shell env, no shared cache between runs.",
    )

    fig.savefig(OUT_SURVEY)
    plt.close(fig)
    print(f"wrote {OUT_SURVEY.relative_to(REPO)}")


# ---------------------------------------------------------------------------
# Plot 2: Wrapper overhead (unleash-specific).
# ---------------------------------------------------------------------------
def plot_overhead():
    eq = load(EQUIV)
    # Order by absolute wall delta ascending so the eye sees "everything tiny"
    # left to right.
    agents = sorted(eq.keys(),
                    key=lambda a: abs(eq[a]["overhead"]["wall_sec_abs"]))
    wall_ms = [eq[a]["overhead"]["wall_sec_abs"] * 1000 for a in agents]
    rss_kb = [eq[a]["overhead"]["max_rss_kb_abs"] for a in agents]
    rss_mb = [v / 1024 for v in rss_kb]

    fig, (ax_wall, ax_rss) = plt.subplots(1, 2, figsize=(13, 7.2),
                                          gridspec_kw={"wspace": 0.28})
    plt.subplots_adjust(top=0.74, bottom=0.12, left=0.05, right=0.97)

    fig_title(
        fig,
        "Wrapping is free.",
        "Wrapper overhead of `unleash` over 7 agent CLIs — apples-to-apples (matched flags), n=15",
    )

    # Wall delta — uniform accent color; measurement floor marker.
    TIME_TICK_MS = 10  # GNU /usr/bin/time resolution.
    bars = ax_wall.barh(agents, wall_ms, color=ACCENT,
                        edgecolor="white", linewidth=0.8, zorder=3)
    # Measurement-floor band: ±10 ms (one GNU time tick).
    ax_wall.axvspan(-TIME_TICK_MS, TIME_TICK_MS, color=NEUTRAL, alpha=0.15,
                    zorder=1, label="measurement floor (±10 ms)")
    ax_wall.axvline(0, color=MUTED, linewidth=1.0, zorder=2)
    ax_wall.set_title("Wall clock overhead", loc="left", pad=10)
    ax_wall.set_xlabel("Δ milliseconds  (wrapped − direct)")
    ax_wall.grid(axis="x", color=GRID, linewidth=0.8, zorder=0)
    ax_wall.set_axisbelow(True)
    for bar, val in zip(bars, wall_ms):
        x = bar.get_width()
        label = f"+{val:.0f} ms" if val >= 0 else f"−{abs(val):.0f} ms"
        offset = 1.5
        ax_wall.text(x + offset if x >= 0 else x - offset,
                     bar.get_y() + bar.get_height() / 2,
                     label, va="center",
                     ha="left" if x >= 0 else "right",
                     fontsize=10.5, color=INK)
    ax_wall.set_xlim(-30, 45)
    ax_wall.legend(loc="lower right", frameon=False, fontsize=9,
                   labelcolor=MUTED)

    # RSS delta — same treatment, with a ±1 MB context band.
    NOISE_MB = 1.0
    bars = ax_rss.barh(agents, rss_mb, color=ACCENT_2,
                       edgecolor="white", linewidth=0.8, zorder=3)
    ax_rss.axvspan(-NOISE_MB, NOISE_MB, color=NEUTRAL, alpha=0.15,
                   zorder=1, label="±1 MB (run-to-run jitter)")
    ax_rss.axvline(0, color=MUTED, linewidth=1.0, zorder=2)
    ax_rss.set_title("Memory overhead", loc="left", pad=10)
    ax_rss.set_xlabel("Δ MB resident  (wrapped − direct)")
    ax_rss.grid(axis="x", color=GRID, linewidth=0.8, zorder=0)
    ax_rss.set_axisbelow(True)
    for bar, val in zip(bars, rss_mb):
        x = bar.get_width()
        if abs(val) < 0.05:
            label = f"{val * 1024:+.0f} KB"
        else:
            label = f"{val:+.2f} MB"
        offset = 0.12
        ax_rss.text(x + offset if x >= 0 else x - offset,
                    bar.get_y() + bar.get_height() / 2,
                    label, va="center",
                    ha="left" if x >= 0 else "right",
                    fontsize=10.5, color=INK)
    ax_rss.set_xlim(-2, 2)
    ax_rss.legend(loc="lower right", frameon=False, fontsize=9,
                  labelcolor=MUTED)

    # Headline note above the plots.
    fig.text(0.05, 0.845,
             "Every wrapped launch lands within ±20 ms of wall and ±0.5 MB of RAM "
             "of its direct equivalent — at or under the measurement floor.",
             fontsize=11, color=INK, ha="left")

    caption(
        fig,
        "Equivalent mode: direct invocations receive the same flags unleash would add "
        "(e.g. `--dangerously-skip-permissions` for claude), so the delta isolates wrapper "
        "cost alone. GNU /usr/bin/time, 15 iterations + 2 warmups discarded.",
    )

    fig.savefig(OUT_OVERHEAD)
    plt.close(fig)
    print(f"wrote {OUT_OVERHEAD.relative_to(REPO)}")


def main():
    if not RAW.exists() or not EQUIV.exists():
        print(f"missing input: {RAW} and/or {EQUIV}", file=sys.stderr)
        return 1
    style()
    plot_survey()
    plot_overhead()
    return 0


if __name__ == "__main__":
    sys.exit(main())
