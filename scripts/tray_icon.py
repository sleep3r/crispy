#!/usr/bin/env python3
"""Convert tray.svg to tray.png with black pixels made transparent. Standalone, no Cargo deps."""

import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SVG = ROOT / "src-tauri" / "icons" / "tray.svg"
OUT = ROOT / "src-tauri" / "resources" / "tray.png"
SIZE = 128  # higher res → sharper when macOS scales for menubar
BLACK_THRESHOLD = 40  # pixels with r,g,b below this become transparent


def svg_to_png_bytes():
    """Render SVG to PNG bytes. Prefer rsvg-convert, then cairosvg."""
    # 1) rsvg-convert (librsvg) — brew install librsvg
    rsvg = subprocess.run(
        ["rsvg-convert", "-w", str(SIZE), "-h", str(SIZE), str(SVG)],
        capture_output=True,
    )
    if rsvg.returncode == 0:
        return rsvg.stdout
    # 2) cairosvg — pip install cairosvg pillow
    try:
        import cairosvg
        import io
        buf = io.BytesIO()
        cairosvg.svg2png(
            url=str(SVG),
            write_to=buf,
            output_width=SIZE,
            output_height=SIZE,
        )
        return buf.getvalue()
    except ImportError:
        pass
    print(
        "Need one of: rsvg-convert (brew install librsvg) or cairosvg (pip install cairosvg pillow)",
        file=sys.stderr,
    )
    sys.exit(1)


def main():
    OUT.parent.mkdir(parents=True, exist_ok=True)
    if not SVG.exists():
        print(f"Missing {SVG}", file=sys.stderr)
        sys.exit(1)

    png_bytes = svg_to_png_bytes()

    try:
        from PIL import Image
        import io
    except ImportError:
        print("Install: pip install pillow", file=sys.stderr)
        sys.exit(1)

    img = Image.open(io.BytesIO(png_bytes)).convert("RGBA")
    data = img.load()
    w, h = img.size
    for y in range(h):
        for x in range(w):
            r, g, b, a = data[x, y]
            if r <= BLACK_THRESHOLD and g <= BLACK_THRESHOLD and b <= BLACK_THRESHOLD:
                data[x, y] = (0, 0, 0, 0)
    img.save(OUT, "PNG")
    print(f"Wrote {OUT}")


if __name__ == "__main__":
    main()
