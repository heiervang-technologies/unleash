#!/usr/bin/env python3
"""Render ANSI art files to PNG images for visual inspection.

Usage: python3 scripts/render-ansi.py <input.ans> [output.png] [--scale N]
"""
import sys
from PIL import Image, ImageDraw, ImageFont

def parse_color(codes, idx):
    if idx+1 < len(codes) and codes[idx+1] == 5 and idx+2 < len(codes):
        c = codes[idx+2]
        if c < 16:
            basic = [(0,0,0),(170,0,0),(0,170,0),(170,85,0),(0,0,170),(170,0,170),(0,170,170),(170,170,170),
                     (85,85,85),(255,85,85),(85,255,85),(255,255,85),(85,85,255),(255,85,255),(85,255,255),(255,255,255)]
            return basic[c], idx+3
        elif c < 232:
            c -= 16
            return ((c//36)*51, ((c%36)//6)*51, (c%6)*51), idx+3
        else:
            v = 8+(c-232)*10; return (v,v,v), idx+3
    elif idx+1 < len(codes) and codes[idx+1] == 2 and idx+4 < len(codes):
        return (codes[idx+2], codes[idx+3], codes[idx+4]), idx+5
    return None, idx+1

def render(input_path, output_path, scale=2):
    with open(input_path, 'r') as f:
        content = f.read()

    lines = content.split('\n')
    CELL_W, CELL_H = 10, 18
    W = 106 * CELL_W
    H = len(lines) * CELL_H
    img = Image.new('RGB', (W, H), (0, 0, 0))
    draw = ImageDraw.Draw(img)

    try:
        font = ImageFont.truetype('/usr/share/fonts/TTF/JetBrainsMonoNerdFont-Regular.ttf', 16)
    except OSError:
        font = ImageFont.load_default()

    fg = (255,255,255); bg = (0,0,0)
    for row, line in enumerate(lines):
        col = 0; i = 0
        while i < len(line):
            if line[i] == '\x1b' and i+1 < len(line) and line[i+1] == '[':
                end = line.find('m', i)
                if end < 0: i += 1; continue
                codes = [int(x) if x else 0 for x in line[i+2:end].split(';')]
                j = 0
                while j < len(codes):
                    c = codes[j]
                    if c == 0: fg=(255,255,255); bg=(0,0,0); j+=1
                    elif c == 38: r,j = parse_color(codes,j+1); fg = r if r else fg
                    elif c == 48: r,j = parse_color(codes,j+1); bg = r if r else bg
                    else: j+=1
                i = end+1
            else:
                x=col*CELL_W; y=row*CELL_H
                if bg!=(0,0,0): draw.rectangle([x,y,x+CELL_W-1,y+CELL_H-1], fill=bg)
                if line[i]!=' ': draw.text((x,y), line[i], fill=fg, font=font)
                col+=1; i+=1

    if scale != 1:
        img = img.resize((W*scale, H*scale), Image.NEAREST)
    img.save(output_path)
    print(f"Saved {output_path} ({W*scale}x{H*scale})")

if __name__ == '__main__':
    if len(sys.argv) < 2:
        print(__doc__); sys.exit(1)
    input_path = sys.argv[1]
    output_path = sys.argv[2] if len(sys.argv) > 2 else '/tmp/ansi_render.png'
    scale = int(sys.argv[sys.argv.index('--scale')+1]) if '--scale' in sys.argv else 2
    render(input_path, output_path, scale)
