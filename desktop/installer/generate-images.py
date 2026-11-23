#!/usr/bin/env python3
"""Draws the Windows installer's header and sidebar bitmaps, one pair per display scale.

NSIS draws these 24-bit BMPs 1:1 (`hooks.nsh` sets `NoStretchNoCrop` and picks the pair for
the window's DPI), so each is drawn at the pixel size its control has at that scale. Run it
from anywhere after changing the art or the sizes; it rewrites every `nsis-*.bmp` beside it:

    nix shell nixpkgs#librsvg nixpkgs#imagemagick -c python3 desktop/installer/generate-images.py

The 100% pair keeps the names `tauri.windows.conf.json` gives Tauri (`nsis-header.bmp`,
`nsis-sidebar.bmp`); the others are `nsis-header-<scale>.bmp` and `nsis-sidebar-<scale>.bmp`.
"""

import pathlib
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent

# The controls' pixel sizes per scale. MUI2 lays them out in dialog units (sidebar 109x193,
# header 100x35, in `modern_headerbmp.exe`), which Windows turns into pixels by the dialog
# font's base units: Tahoma 8 pt (MS Shell Dlg 2), measured at each DPI from its `hdmx` widths
# and its ascent and descent, as GetDialogBaseUnits does. At 100% that gives MUI's documented
# 164x314 and 150x57. The sizes do not grow in proportion to the scale, which is why one
# bitmap stretched to every scale looked blurred and distorted.
#   scale: (sidebar width, sidebar height, header width, header height)
CONTROLS = {
    100: (164, 314, 150, 57),
    125: (191, 386, 175, 70),
    150: (245, 458, 225, 83),
    175: (273, 555, 250, 101),
    200: (327, 603, 300, 109),
    250: (409, 796, 375, 144),
    300: (491, 941, 450, 171),
}

# hooks.nsh uses the largest scale at or below the window's, so a sidebar is drawn as tall as
# the next scale's control (225% included under 200%): the page clips what lies below its
# bottom, and a custom scale in between shows no strip under the image.
SIDEBAR_TAIL = {100: 386, 125: 458, 150: 555, 175: 603, 200: 796, 250: 941, 300: 1101}

BRAND = "#1d4ed8"
WASH_TOP = "#f8fafc"
WASH_BOTTOM = "#dce9fc"
CHART_LINE = "#a9c4f5"
CHART_GRID = "#dbe6f8"

# The app's mark (dashboard favicon.svg): white on a #1d4ed8 rounded square, on a 32 grid.
MARK = (
    "M23 8c0 1.1-.9 2-2 2-.18 0-.35-.02-.51-.07l-3.56 3.55c.05.16.07.34.07.52 0 1.1-.9 2-2 2s-2"
    "-.9-2-2c0-.18.02-.36.07-.52l-2.55-2.55c-.16.05-.34.07-.52.07s-.36-.02-.52-.07l-4.55 4.56c"
    ".05.16.07.33.07.51 0 1.1-.9 2-2 2s-2-.9-2-2 .9-2 2-2c.18 0 .35.02.51.07l4.56-4.55C8.02 9.36"
    " 8 9.18 8 9c0-1.1.9-2 2-2s2 .9 2 2c0 .18-.02.36-.07.52l2.55 2.55c.16-.05.34-.07.52-.07s.36"
    ".02.52.07l3.55-3.56C19.02 8.35 19 8.18 19 8c0-1.1.9-2 2-2s2 .9 2 2z"
)


def app_icon(x: float, y: float, size: float) -> str:
    k = size / 32
    return (
        f'<rect x="{x}" y="{y}" width="{size}" height="{size}" rx="{8 * k}" fill="{BRAND}"/>'
        f'<path fill="#fff" transform="translate({x + 4 * k} {y + 4 * k}) scale({k})" d="{MARK}"/>'
    )


def sidebar_svg(scale: int) -> tuple[str, int, int]:
    width, height, _, _ = CONTROLS[scale]
    tail = SIDEBAR_TAIL[scale]
    k = scale / 100
    tile = round(100 * k)
    tile_x = (width - tile) / 2
    tile_y = round(height * 0.172)
    icon = round(72 * k)
    grid = max(1, round(k))
    # Grid lines on whole pixels, so they stay one crisp line at every scale.
    rows = "".join(
        f'<rect x="0" y="{round(height * f)}" width="{width}" height="{grid}" fill="{CHART_GRID}"/>'
        for f in (0.65, 0.79, 0.93)
    )
    step = width / 5
    cols = "".join(
        f'<rect x="{round(step * i)}" y="{round(height * 0.62)}" width="{grid}" height="{tail}" fill="{CHART_GRID}"/>'
        for i in range(1, 5)
    )
    points = [(0, 0.89), (0.183, 0.833), (0.40, 0.871), (0.652, 0.753), (0.838, 0.782), (1.02, 0.70)]
    line = " ".join(f"{width * px:.2f},{height * py:.2f}" for px, py in points)
    dots = "".join(
        f'<circle cx="{width * px:.2f}" cy="{height * py:.2f}" r="{4.5 * k:.2f}" fill="#fff" '
        f'stroke="{CHART_LINE}" stroke-width="{2 * k:.2f}"/>'
        for px, py in points[1:-1]
    )
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{tail}" viewBox="0 0 {width} {tail}">
<defs>
  <linearGradient id="wash" gradientUnits="userSpaceOnUse" x1="0" y1="0" x2="0" y2="{height}">
    <stop offset="0" stop-color="{WASH_TOP}"/><stop offset="1" stop-color="{WASH_BOTTOM}"/>
  </linearGradient>
  <filter id="shadow" x="-30%" y="-30%" width="160%" height="160%">
    <feDropShadow dx="0" dy="{2 * k:.2f}" stdDeviation="{5 * k:.2f}" flood-color="{BRAND}" flood-opacity="0.14"/>
  </filter>
</defs>
<rect width="{width}" height="{tail}" fill="{WASH_BOTTOM}"/>
<rect width="{width}" height="{height}" fill="url(#wash)"/>
{rows}{cols}
<polyline points="{line}" fill="none" stroke="{CHART_LINE}" stroke-width="{2 * k:.2f}" stroke-linejoin="round"/>
{dots}
<rect x="{tile_x}" y="{tile_y}" width="{tile}" height="{tile}" rx="{22 * k:.2f}" fill="#fff" filter="url(#shadow)"/>
{app_icon(tile_x + (tile - icon) / 2, tile_y + (tile - icon) / 2, icon)}
</svg>"""
    return svg, width, tail


def header_svg(scale: int) -> tuple[str, int, int]:
    _, _, width, height = CONTROLS[scale]
    k = scale / 100
    icon = round(height * 0.6)
    x = width - round(13 * k) - icon
    y = round((height - icon) / 2)
    svg = f"""<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="{width}" height="{height}" fill="#fff"/>
{app_icon(x, y, icon)}
</svg>"""
    return svg, width, height


def render(svg: str, width: int, height: int, out: pathlib.Path, work: pathlib.Path) -> None:
    source = work / f"{out.stem}.svg"
    png = work / f"{out.stem}.png"
    source.write_text(svg)
    subprocess.run(
        ["rsvg-convert", "-w", str(width), "-h", str(height), str(source), "-o", str(png)],
        check=True,
    )
    # A plain 24-bit BMP (BITMAPINFOHEADER, no alpha): what LoadImage and MUI2 take.
    subprocess.run(
        ["magick", str(png), "-background", "white", "-alpha", "remove", "-alpha", "off",
         "-type", "TrueColor", f"BMP3:{out}"],
        check=True,
    )


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        work = pathlib.Path(tmp)
        for scale in CONTROLS:
            suffix = "" if scale == 100 else f"-{scale}"
            render(*sidebar_svg(scale), HERE / f"nsis-sidebar{suffix}.bmp", work)
            render(*header_svg(scale), HERE / f"nsis-header{suffix}.bmp", work)


if __name__ == "__main__":
    main()
