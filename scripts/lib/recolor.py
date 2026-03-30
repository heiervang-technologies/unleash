#!/usr/bin/env python3
"""Recolor ANSI art by hue-rotating orange-tone pixels.

Mirrors the Rust logic in src/theme.rs: converts each 24-bit RGB color in
ANSI escape sequences to HSL, detects orange-family pixels (hue 0-50 or
>=350, saturation > 0.10), shifts hue and scales saturation, converts back.

Usage:
    recolor.py <file> <hue_shift> <sat_scale>
    recolor.py mascot.claude.ans 200.0 1.0   # blue theme
    recolor.py mascot.claude.ans 0.0 1.0      # orange (identity)
"""

import re
import sys

BASE_ORANGE_HUE = 14.769230769230768

def rgb_to_hsl(r, g, b):
    r, g, b = r / 255.0, g / 255.0, b / 255.0
    mx, mn = max(r, g, b), min(r, g, b)
    l = (mx + mn) / 2.0
    if abs(mx - mn) < 1e-10:
        return (0.0, 0.0, l)
    d = mx - mn
    s = d / (2.0 - mx - mn) if l > 0.5 else d / (mx + mn)
    if abs(mx - r) < 1e-10:
        h = (g - b) / d + (6.0 if g < b else 0.0)
    elif abs(mx - g) < 1e-10:
        h = (b - r) / d + 2.0
    else:
        h = (r - g) / d + 4.0
    return (h * 60.0, s, l)

def hue_to_rgb(p, q, t):
    if t < 0: t += 1
    if t > 1: t -= 1
    if t < 1/6: return p + (q - p) * 6 * t
    if t < 1/2: return q
    if t < 2/3: return p + (q - p) * (2/3 - t) * 6
    return p

def hsl_to_rgb(h, s, l):
    if abs(s) < 1e-10:
        v = round(l * 255)
        return (v, v, v)
    q = l * (1 + s) if l < 0.5 else l + s - l * s
    p = 2 * l - q
    hn = h / 360.0
    r = hue_to_rgb(p, q, hn + 1/3)
    g = hue_to_rgb(p, q, hn)
    b = hue_to_rgb(p, q, hn - 1/3)
    return (round(r * 255), round(g * 255), round(b * 255))

def is_orange_tone(h, s):
    return (h <= 50.0 or h >= 350.0) and s > 0.10

def transform(r, g, b, hue_shift, sat_scale):
    h, s, l = rgb_to_hsl(r, g, b)
    if is_orange_tone(h, s):
        new_h = (h + hue_shift) % 360.0
        new_s = max(0.0, min(1.0, s * sat_scale))
        return hsl_to_rgb(new_h, new_s, l)
    return (r, g, b)

def main():
    if len(sys.argv) != 4:
        print(f"Usage: {sys.argv[0]} <file> <hue_shift> <sat_scale>", file=sys.stderr)
        sys.exit(1)

    path, hue_shift, sat_scale = sys.argv[1], float(sys.argv[2]), float(sys.argv[3])

    with open(path, 'rb') as f:
        data = f.read()

    # Build a cache of (r,g,b) -> (nr,ng,nb) to avoid repeated conversions
    cache = {}

    def replace_color(m):
        prefix = m.group(1)  # b"38;2;" or b"48;2;"
        r, g, b = int(m.group(2)), int(m.group(3)), int(m.group(4))
        key = (r, g, b)
        if key not in cache:
            cache[key] = transform(r, g, b, hue_shift, sat_scale)
        nr, ng, nb = cache[key]
        return b'\x1b[' + prefix + f'{nr};{ng};{nb}m'.encode()

    pattern = rb'\x1b\[(38;2;|48;2;)(\d+);(\d+);(\d+)m'
    result = re.sub(pattern, replace_color, data)
    sys.stdout.buffer.write(result)

if __name__ == '__main__':
    main()
