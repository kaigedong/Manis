# Manis brand assets

- `manis-mark.svg` is the compact product mark. Use it for application icons, the in-app brand
  lockup, repository avatars, non-template system trays (Linux/Windows), and other small or square
  placements.
- `manis-tray-template.svg` is the monochrome transparent menu bar template derived from the core
  layered silhouette of `manis-mark.svg` without the dark backing tile. Use it for the macOS status
  bar template icon so AppKit tints it cleanly for both light and dark menu bars.
- `manis-shanshui.svg` is the detailed companion artwork. Use it at large sizes in documentation
  or release material; its fine texture is not intended for interface icons.

Both source files use a `1254 × 1254` square view box and contain only embedded vector paths.
Keep the SVG sources as the canonical assets and generate platform-specific raster formats during
packaging where possible.

The compact mark uses a filled rounded rectangle with transparent outer padding. Keep the
background as a rounded rectangle rather than clipping a square: macOS `sips` does not preserve
the rounded `clipPath` when generating the Dock icon. Do not add a white backing tile.
