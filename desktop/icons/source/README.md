# App icon sources

`app-icon-macos.svg` is the macOS app icon: the app's mark on an 824 px body in a 1024 px canvas,
following Apple's icon grid, so it sits at the same size as other Dock icons. It produces
`../icon.icns` (also used for the Dock icon in `tauri dev`):

```sh
mkdir -p AppIcon.iconset
for s in 16 32 128 256 512; do
  nix shell nixpkgs#librsvg -c rsvg-convert -w $s -h $s app-icon-macos.svg -o AppIcon.iconset/icon_${s}x${s}.png
  nix shell nixpkgs#librsvg -c rsvg-convert -w $((s*2)) -h $((s*2)) app-icon-macos.svg -o AppIcon.iconset/icon_${s}x${s}@2x.png
done
iconutil -c icns AppIcon.iconset -o ../icon.icns
rm -r AppIcon.iconset
```

The Windows and Linux icons (`../icon.ico`, `../*.png`) stay full-bleed, as those platforms expect.
