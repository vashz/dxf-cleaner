"""Builds assets/icon.ico (+ icon-256.png, icon-small-256.png) from generated SVG artwork.

icon-256.png and icon-small-256.png are the two designs the app scales down for its
toolbar logo: the small one (no label, bolder strokes) for 40 pixels and under.

Requires Inkscape (for SVG -> PNG). No Python packages needed.
Run:  py -I assets/build_icon.py
"""

import math
import os
import struct
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
INKSCAPE = os.environ.get("INKSCAPE", r"C:\Program Files\Inkscape\bin\inkscape.exe")

C = (128.0, 132.0)  # center of the outline
R = 70.0            # radius of the outline


def pt(deg, r=R):
    a = math.radians(deg)
    return C[0] + r * math.cos(a), C[1] + r * math.sin(a)  # SVG: y down, clockwise


def svg(small: bool) -> str:
    # Clean part: a true arc from the top (-90deg) clockwise round to the left (180deg).
    x0, y0 = pt(-90)
    x1, y1 = pt(180)
    stroke = 26 if small else 17
    clean = (
        f'<path d="M {x0:.2f} {y0:.2f} A {R} {R} 0 1 1 {x1:.2f} {y1:.2f}" fill="none" '
        f'stroke="#ffffff" stroke-width="{stroke}" stroke-linecap="round"/>'
    )
    # Polluted part: jagged point-to-point segments over the upper-left quarter.
    n = 3 if small else 5
    pts = []
    for i in range(n + 1):
        deg = 180 + 90 * i / n
        wobble = 0 if i in (0, n) else (9 if i % 2 else -9)
        pts.append(pt(deg, R + (0 if small else wobble)))
    poly = " ".join(f"{x:.2f},{y:.2f}" for x, y in pts)
    seg_w = 10 if small else 6
    polluted = (
        f'<polyline points="{poly}" fill="none" stroke="#9fd0ff" stroke-width="{seg_w}" '
        f'stroke-linejoin="miter" stroke-linecap="square" opacity="0.95"/>'
    )
    nodes = ""
    if not small:
        for x, y in pts[1:-1]:
            nodes += f'<rect x="{x - 7:.2f}" y="{y - 7:.2f}" width="14" height="14" rx="2" fill="#9fd0ff"/>'
    # Cutting head where the clean arc starts.
    hx, hy = pt(180)
    head_r = 21 if small else 15
    head = (
        f'<circle cx="{hx:.2f}" cy="{hy:.2f}" r="{head_r + 5}" fill="#ffb02e" opacity="0.35"/>'
        f'<circle cx="{hx:.2f}" cy="{hy:.2f}" r="{head_r}" fill="#ffb02e"/>'
    )
    label = ""
    if not small:
        label = (
            '<text x="128" y="143" text-anchor="middle" font-family="Segoe UI, Arial, sans-serif" '
            'font-weight="700" font-size="44" fill="#ffffff" letter-spacing="1">DXF</text>'
        )
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
  <defs>
    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#2f86f0"/>
      <stop offset="1" stop-color="#0f3a86"/>
    </linearGradient>
  </defs>
  <rect x="6" y="6" width="244" height="244" rx="54" fill="url(#bg)"/>
  <rect x="6.5" y="6.5" width="243" height="243" rx="53.5" fill="none" stroke="#ffffff" stroke-opacity="0.18"/>
  {clean}
  {polluted}
  {nodes}
  {head}
  {label}
</svg>
"""


def render(svg_text: str, size: int, tmp: str) -> bytes:
    src = os.path.join(tmp, f"icon_{size}.svg")
    out = os.path.join(tmp, f"icon_{size}.png")
    with open(src, "w", encoding="utf-8") as f:
        f.write(svg_text)
    subprocess.run(
        [INKSCAPE, src, "--export-type=png", f"--export-filename={out}", f"--export-width={size}", f"--export-height={size}"],
        check=True,
        capture_output=True,
    )
    with open(out, "rb") as f:
        return f.read()


def write_ico(path: str, images: list[tuple[int, bytes]]) -> None:
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    data = b""
    for size, png in images:
        dim = 0 if size >= 256 else size
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(png), offset + len(data))
        data += png
    with open(path, "wb") as f:
        f.write(header + entries + data)


def main() -> None:
    if not os.path.exists(INKSCAPE):
        sys.exit(f"Inkscape not found at {INKSCAPE} (set INKSCAPE env var)")
    with open(os.path.join(HERE, "icon.svg"), "w", encoding="utf-8") as f:
        f.write(svg(False))
    with open(os.path.join(HERE, "icon-small.svg"), "w", encoding="utf-8") as f:
        f.write(svg(True))
    with tempfile.TemporaryDirectory() as tmp:
        images = []
        for size in (16, 20, 24, 32, 40, 48, 64, 96, 128, 256):
            images.append((size, render(svg(size <= 40), size, tmp)))
        write_ico(os.path.join(HERE, "icon.ico"), images)
        with open(os.path.join(HERE, "icon-256.png"), "wb") as f:
            f.write(dict(images)[256])
        with open(os.path.join(HERE, "icon-small-256.png"), "wb") as f:
            f.write(render(svg(True), 256, tmp))
    print("wrote icon.ico, icon-256.png, icon-small-256.png, icon.svg, icon-small.svg")


if __name__ == "__main__":
    main()
