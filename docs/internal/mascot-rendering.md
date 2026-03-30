# Mascot Rendering Pipeline

How ANSI art goes from a `.ans` file to colored pixels in the TUI.

## Source

Single file: `src/assets/mascot.claude.ans` — 106 columns wide, ~30 lines. Contains raw ANSI escape sequences (`\x1b[38;2;R;G;Bm` for foreground, `\x1b[48;2;R;G;Bm` for background).

Embedded at compile time:

```rust
const FULL_ART: &str = include_str!("assets/mascot.claude.ans");
```

## Runtime Slicing

The TUI shows either the left half, right half, or full image depending on layout. Halves are derived at runtime — no separate files.

```rust
// split_ansi_art tracks escape code state across the split boundary
// so the right half starts with the correct active color
let (left, right) = split_ansi_art(FULL_ART, 53); // 53 = half of 106
```

`split_ansi_line` walks each line character by character, routing visible chars to left/right based on column count, while forwarding escape sequences to the active side. The left half gets a `\x1b[0m` reset appended; the right half gets the last active escape sequence prepended.

## ANSI to Ratatui

The TUI uses ratatui, which needs `Line<'static>` (vec of styled `Span`s), not raw ANSI strings. The parser in `pixel_art.rs` converts:

```
"\x1b[38;2;255;128;0mHello\x1b[0m world"
```

into:

```rust
Line::from(vec![
    Span::styled("Hello", Style::default().fg(Color::Rgb(255, 128, 0))),
    Span::styled(" world", Style::default()),
])
```

Core function: `parse_ansi_line_to_spans_themed(line, shift)` — walks chars, accumulates text until hitting `\x1b[`, parses the SGR sequence, updates the ratatui `Style`, flushes the span.

## Recoloring

Three modes, all operating on the same source art:

### Theme shift (profile color)

Each profile has a `theme` color (e.g. "orange", "blue", "#ff00ff"). At render time this becomes a `ThemeShift` containing a hue rotation angle:

```rust
let shift = self.theme_color.theme_shift(); // e.g. 120 degrees for blue
let lines = mascots::unleashed_claude_ratatui_themed(max_lines, shift);
```

Inside the parser, every RGB color in the art gets its hue rotated:

```rust
// In parse_ansi_sequence_themed:
let (nr, ng, nb) = transform_theme_color(r, g, b, shift);
style = style.fg(RatatuiColor::Rgb(nr, ng, nb));
```

`transform_theme_color` converts RGB -> HSL, adds the hue offset, converts back. The original art is orange; a 120-degree shift makes it blue.

### Lava mode (easter egg)

Triggered by Konami code. Each line gets a different hue offset based on `animation_frame + line_index`, creating a cycling rainbow effect:

```rust
let hue_offset = ((animation_frame + line_idx) * 7) % 360;
```

### No shift (identity)

Default when profile theme is orange (the art's native color). `ThemeShift::identity()` passes colors through unchanged.

## TUI Layout

`render_ui()` in `app.rs` decides placement:

```
┌──────────────────────────────────────────┐
│ [right-half art] │ [menu content]        │  <- art_position = Left
│                  │                       │
├──────────────────────────────────────────┤
│ status bar                               │
└──────────────────────────────────────────┘
```

The art side flips when the user clicks the mascot (toggles `art_position`). During the flip animation, the full 106-column art is rendered and scrolled horizontally using `Paragraph::scroll`.

Key constant: `ART_WIDTH = 53` (half the full art width).

## Adding a New Mascot

1. Create a single `.ans` file at full width (even number of columns)
2. Place it in `src/assets/mascot.<name>.ans`
3. The slicing and recoloring pipeline works on any ANSI art — no code changes needed for the color system
