#!/usr/bin/env python3
"""Draws the Windows installer's icon, `installer.ico`, with a frame for every size it is shown at.

The installer's window (`one-click.nsh`) loads its logo from this icon, the exe's own, at the
size the display scale asks for: 56 px at 100%, 70 at 125%, 84 at 150%, 98 at 175% and 112 at
200%; larger scales scale the 256 px frame down. The title bar and Explorer take the small
frames. Run it after changing the app icon, from the repository root, and commit what it writes:

    nix shell --inputs-from . nixpkgs#python3 nixpkgs#imagemagick \
      -c python3 desktop/installer/generate-images.py
"""

import pathlib
import struct
import subprocess
import tempfile

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parent.parent
APP_ICON_ICO = ROOT / "desktop/icons/icon.ico"
APP_ICON_PNG = ROOT / "desktop/icons/icon.png"

# The app icon's own frames are taken as they are; the others are drawn from icons/icon.png:
# 20 and 40 (small icons at 125% and 250%) and the logo's sizes at 100% to 200%.
ICON_SIZES = (16, 20, 24, 32, 40, 48, 56, 64, 70, 84, 98, 112, 256)


def installer_ico(work: pathlib.Path) -> bytes:
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
        (HERE / "installer.ico").write_bytes(installer_ico(pathlib.Path(tmp)))


if __name__ == "__main__":
    main()
