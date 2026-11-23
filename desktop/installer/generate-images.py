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

# Each bitmap is drawn this many times as large and each block averaged in linear light: edges
# get even, gamma-correct anti-aliasing, and no filter overshoot (Lanczos rings dark around the
# mark).
SUPERSAMPLE = 4

# The icon's sizes: the app icon's own frames, and the ones Windows asks for at 125% and 250%,
# drawn from the app icon.
ICON_SIZES = (16, 20, 24, 32, 40, 48, 64, 256)

WASH_TOP = "#f8fafc"
WASH_BOTTOM = "#e3edfc"
WAVE_BACK = "#d3e2fb"
WAVE_FRONT = "#bfd4f7"

SVG_NS = "http://www.w3.org/2000/svg"


def favicon() -> str:
    """The app's mark (the dashboard's favicon) as an element to nest."""
    return re.sub(r"<\?xml[^>]*\?>", "", FAVICON.read_text()).strip()


MARK = favicon()


def app_icon(x: float, y: float, size: float) -> str:
    return MARK.replace("<svg ", f'<svg x="{x}" y="{y}" width="{size}" height="{size}" ', 1)


def sidebar_svg(scale: int) -> tuple[str, int, int]:
    """A wash, the app's mark large and on whole pixels, and two soft rising waves below it.

    No hairlines, small dots or blurred shadows: at 100% the sidebar is 164 px wide, where such
    detail stair-steps, and nothing in it needs a sharp pixel to read.
    """
    width, height, _, _ = CONTROLS[scale]
    mark = round(width * 0.56)
    x = (width - mark) // 2
    y = round(height * 0.19)

    def wave(left: float, right: float, dip: float) -> str:
        # A smooth rise from left to right, closed along the bottom edge.
        y0, y1 = height * left, height * right
        return (
            f"M0 {y0:.2f} C {width * 0.35:.2f} {y0 + height * dip:.2f} "
            f"{width * 0.6:.2f} {y1 + height * dip * 0.4:.2f} {width} {y1:.2f} "
            f"L{width} {height} L0 {height} Z"
        )

    svg = f"""<svg xmlns="{SVG_NS}" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<defs>
  <linearGradient id="wash" x1="0" y1="0" x2="0" y2="1">
    <stop offset="0" stop-color="{WASH_TOP}"/><stop offset="1" stop-color="{WASH_BOTTOM}"/>
  </linearGradient>
</defs>
<rect width="{width}" height="{height}" fill="url(#wash)"/>
<path d="{wave(0.80, 0.64, 0.04)}" fill="{WAVE_BACK}"/>
<path d="{wave(0.90, 0.76, 0.05)}" fill="{WAVE_FRONT}"/>
{app_icon(x, y, mark)}
</svg>"""
    return svg, width, height


def header_svg(scale: int) -> tuple[str, int, int]:
    """White, as the header band is, with the app's mark at the right on whole pixels."""
    _, _, width, height = CONTROLS[scale]
    mark = round(height * 0.7)
    x = width - round(12 * scale / 100) - mark
    y = (height - mark) // 2
    svg = f"""<svg xmlns="{SVG_NS}" width="{width}" height="{height}" viewBox="0 0 {width} {height}">
<rect width="{width}" height="{height}" fill="#fff"/>
{app_icon(x, y, mark)}
</svg>"""
    return svg, width, height


def render(svg: str, width: int, height: int, out: pathlib.Path, work: pathlib.Path,
           supersample: int = SUPERSAMPLE) -> None:
    """Draws the SVG `supersample` times as large and averages each block in linear light,
    then writes a plain 24-bit BMP (BITMAPINFOHEADER, no alpha): what LoadImage and MUI2 take."""
    source = work / f"{out.stem}.svg"
    png = work / f"{out.stem}.png"
    source.write_text(svg)
    subprocess.run(
        ["rsvg-convert", "-w", str(width * supersample), "-h", str(height * supersample),
         str(source), "-o", str(png)],
        check=True,
    )
    subprocess.run(
        ["magick", str(png), "-background", "white", "-alpha", "remove", "-alpha", "off",
         "-colorspace", "RGB", "-filter", "Box", "-resize", f"{width}x{height}!",
         "-colorspace", "sRGB", "-type", "TrueColor", f"BMP3:{out}"],
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
