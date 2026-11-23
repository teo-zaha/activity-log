#!/usr/bin/env python3
"""Draws the Windows installer's icon and its header and sidebar bitmaps, one pair per scale.

MUI2 sizes its image controls in dialog units and resizes a bitmap to them with LoadImage, whose
resampling is coarse. So each bitmap here is drawn at exactly the pixel size its control has at
one display scale, and `hooks.nsh` hands MUI the largest that fits the control it measures, which
LoadImage then copies 1:1. Run it after changing the art, the sizes or the app icon, from the
repository root, and commit what it writes:

    nix shell --inputs-from . nixpkgs#python3 nixpkgs#librsvg nixpkgs#imagemagick \
      -c python3 desktop/installer/generate-images.py

It writes `nsis-header*.bmp`, `nsis-sidebar*.bmp` (the 100% pair keeps the names
`tauri.windows.conf.json` gives Tauri), `nsis-scales.nsh` (which pair fits which control) and
`installer.ico`.
"""

import pathlib
import re
import struct
import subprocess
import tempfile
import xml.etree.ElementTree as ET

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
FAVICON = ROOT / "dashboard/src/favicon.svg"
APP_ICON_ICO = ROOT / "desktop/icons/icon.ico"
APP_ICON_PNG = ROOT / "desktop/icons/icon.png"

# The controls' pixel sizes at Windows' scale presets. MUI2 lays them out in dialog units (sidebar
# 109x193, header 100x35, in `modern_headerbmp.exe`), which Windows turns into pixels by the
# dialog font's base units: Tahoma 8 pt (MS Shell Dlg 2), measured at each DPI from its `hdmx`
# widths and its ascent and descent, as GetDialogBaseUnits does. At 100% that gives MUI's
# documented 164x314 and 150x57. The sizes do not grow in proportion to the scale.
#   scale: (sidebar width, sidebar height, header width, header height)
CONTROLS = {
    100: (164, 314, 150, 57),
    125: (191, 386, 175, 70),
    150: (245, 458, 225, 83),
    175: (273, 555, 250, 101),
    200: (327, 603, 300, 109),
    225: (354, 700, 325, 127),
    250: (409, 796, 375, 144),
    300: (491, 941, 450, 171),
    350: (545, 1086, 500, 197),
}

# The icon's sizes: the app icon's own frames, and the ones Windows asks for at 125% and 250%,
# drawn from the app icon.
ICON_SIZES = (16, 20, 24, 32, 40, 48, 64, 256)

WASH_TOP = "#f8fafc"
WASH_BOTTOM = "#dce9fc"
CHART_LINE = "#a9c4f5"
CHART_GRID = "#dbe6f8"

SVG_NS = "http://www.w3.org/2000/svg"


def favicon() -> tuple[str, str]:
    """The app's mark (the dashboard's favicon) as an element to nest, and its blue."""
    text = FAVICON.read_text()
    brand = ET.fromstring(text).find(f"{{{SVG_NS}}}rect").get("fill")
    body = re.sub(r"<\?xml[^>]*\?>", "", text).strip()
    return body, brand


MARK, BRAND = favicon()


def app_icon(x: float, y: float, size: float) -> str:
    return MARK.replace("<svg ", f'<svg x="{x}" y="{y}" width="{size}" height="{size}" ', 1)


def sidebar_svg(scale: int) -> tuple[str, int, int]:
    width, height, _, _ = CONTROLS[scale]
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
        f'<rect x="{round(step * i)}" y="{round(height * 0.62)}" width="{grid}" height="{height}" fill="{CHART_GRID}"/>'
        for i in range(1, 5)
    )
    points = [(0, 0.89), (0.183, 0.833), (0.40, 0.871), (0.652, 0.753), (0.838, 0.782), (1.02, 0.70)]
    line = " ".join(f"{width * px:.2f},{height * py:.2f}" for px, py in points)
    dots = "".join(
        f'<circle cx="{width * px:.2f}" cy="{height * py:.2f}" r="{4.5 * k:.2f}" fill="#fff" '
        f'stroke="{CHART_LINE}" stroke-width="{2 * k:.2f}"/>'
        for px, py in points[1:-1]
    )
    svg = f"""<svg xmlns="{SVG_NS}" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<defs>
  <linearGradient id="wash" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="{WASH_TOP}"/><stop offset="1" stop-color="{WASH_BOTTOM}"/>
  </linearGradient>
  <filter id="shadow" x="-30%" y="-30%" width="160%" height="160%">
    <feDropShadow dx="0" dy="{2 * k:.2f}" stdDeviation="{5 * k:.2f}" flood-color="{BRAND}" flood-opacity="0.14"/>
  </filter>
</defs>
<rect width="{width}" height="{height}" fill="url(#wash)"/>
{rows}{cols}
<polyline points="{line}" fill="none" stroke="{CHART_LINE}" stroke-width="{2 * k:.2f}" stroke-linejoin="round"/>
{dots}
<rect x="{tile_x}" y="{tile_y}" width="{tile}" height="{tile}" rx="{22 * k:.2f}" fill="#fff" filter="url(#shadow)"/>
{app_icon(tile_x + (tile - icon) / 2, tile_y + (tile - icon) / 2, icon)}
</svg>"""
    return svg, width, height


def header_svg(scale: int) -> tuple[str, int, int]:
    _, _, width, height = CONTROLS[scale]
    k = scale / 100
    icon = round(height * 0.6)
    x = width - round(13 * k) - icon
    y = round((height - icon) / 2)
    svg = f"""<svg xmlns="{SVG_NS}" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
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


def bitmap(kind: str, scale: int) -> str:
    return f"nsis-{kind}{'' if scale == 100 else f'-{scale}'}.bmp"


def scales_nsh() -> str:
    """Two macros that put the largest bitmap fitting a control of WIDTH x HEIGHT pixels at TARGET.

    They leave RESULT 1 when one fits, and 0 when none does (a control smaller than the 100%
    bitmap), so MUI's own image stays. The 100% pair is MUI's own, so it is never extracted again.
    """
    out = [
        "; Generated by generate-images.py from its CONTROLS table: do not edit, run the script.",
        "; ACTIVITY_LOG_IMAGES is the folder of these bitmaps (hooks.nsh defines it).",
        "",
    ]
    for kind, (w_at, h_at) in (("header", (2, 3)), ("sidebar", (0, 1))):
        out.append(f"!macro ACTIVITY_LOG_{kind.upper()}_FOR WIDTH HEIGHT TARGET RESULT")
        out.append("  StrCpy ${RESULT} 0")
        first = True
        for scale in sorted(CONTROLS, reverse=True):
            if scale == 100:
                continue
            size = CONTROLS[scale]
            name = bitmap(kind, scale)
            out.append(f"  ${{{'If' if first else 'ElseIf'}}} ${{WIDTH}} >= {size[w_at]}")
            out.append(f"  ${{AndIf}} ${{HEIGHT}} >= {size[h_at]}")
            out.append(f'    !pragma verifyloadimage "${{ACTIVITY_LOG_IMAGES}}/{name}"')
            out.append(f'    File "/oname=${{TARGET}}" "${{ACTIVITY_LOG_IMAGES}}/{name}"')
            out.append("    StrCpy ${RESULT} 1")
            first = False
        out.append("  ${EndIf}")
        out.append("!macroend")
        out.append("")
    return "\n".join(out)


def installer_ico(work: pathlib.Path) -> bytes:
    """The app icon's frames as they are, and the missing sizes drawn from icons/icon.png."""
    data = APP_ICON_ICO.read_bytes()
    frames = {}
    for i in range(struct.unpack_from("<H", data, 4)[0]):
        w, _, _, _, planes, bpp, size, offset = struct.unpack_from("<BBBBHHII", data, 6 + 16 * i)
        frames[w or 256] = (planes, bpp, data[offset:offset + size])
    for size in ICON_SIZES:
        if size not in frames:
            png = work / f"icon-{size}.png"
            subprocess.run(
                ["magick", str(APP_ICON_PNG), "-filter", "Lanczos", "-resize", f"{size}x{size}",
                 "-strip", f"PNG32:{png}"],
                check=True,
            )
            frames[size] = (1, 32, png.read_bytes())
    head = struct.pack("<HHH", 0, 1, len(ICON_SIZES))
    body = b""
    offset = 6 + 16 * len(ICON_SIZES)
    for size in ICON_SIZES:
        planes, bpp, image = frames[size]
        head += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, planes, bpp, len(image),
                            offset + len(body))
        body += image
    return head + body


def main() -> None:
    with tempfile.TemporaryDirectory() as tmp:
        work = pathlib.Path(tmp)
        for scale in CONTROLS:
            render(*sidebar_svg(scale), HERE / bitmap("sidebar", scale), work)
            render(*header_svg(scale), HERE / bitmap("header", scale), work)
        (HERE / "nsis-scales.nsh").write_text(scales_nsh())
        (HERE / "installer.ico").write_bytes(installer_ico(work))


if __name__ == "__main__":
    main()
