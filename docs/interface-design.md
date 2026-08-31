# Interface materials and hierarchy

Manis keeps GPUI and gpui-component. Its interface uses opaque neutral surfaces, compact native
typography, and restrained semantic accents. This replaces the previous whole-window blur and
stacked translucent green surfaces; readability must not depend on the user's wallpaper.

## Reference and scope

The reference is OpenAI's public [UI guidelines](https://developers.openai.com/plugins/concepts/ui-guidelines)
and [Apps SDK UI design tokens](https://github.com/openai/apps-sdk-ui/tree/0f00143c7a639906f1621fe58e1b6be7b5bea46d/src/styles).
They document neutral system colors, limited accents, a small typography scale, consistent spacing,
and clear content/action hierarchy. They are a public design reference, **not** evidence of the private
ChatGPT desktop implementation. Manis does not embed that React library, reproduce ChatGPT branding,
or change its proxy-management information architecture.

The neutral color scale follows that reference; metadata and semantic text colors are adjusted for
Manis's dense tables so that even small text meets a 4.5:1 contrast target. GPUI still owns native
rendering; gpui-component still owns buttons, inputs, dialogs, popovers, focus, and dismissal.

## Rules

- Native windows use `WindowBackgroundAppearance::Opaque`. The integrated macOS title bar remains
  transparent *to the application's opaque chrome*, not to the desktop. Do not restore backdrop blur.
- Window, page, navigation, card, input, table, dialog, and popup surfaces are fully opaque in both
  themes. Gray luminance differences establish hierarchy. Transparent layout-only containers may
  inherit an opaque parent; modal scrims and shadows may blend over the application.
- Main content is white in light mode and `#212121` in dark mode. Chrome is `#f9f9f9` / `#181818`;
  elevated content is white / `#303030`. Popups must conceal the text beneath them.
- Primary actions are charcoal / near-white with inverse labels. Hover and pressed fills differ.
  Navigation and selected rows use neutral gray, not a brand-colored wash. Green, amber, red, and
  warm route accents communicate status, not general decoration.
- Destructive buttons have an independent red-fill/white-label contrast pair; never derive their
  labels from the dark-mode primary button. Input caret, selection, tabs, scrollbars, and chrome are
  projected into the same component theme. Synchronize both legacy colors and render tokens.
- Page/section headings use semibold, labels/data medium, and supporting text normal weight.
  Preserve the 4/8/12/16/24 px spacing scale, 8/12 px corner radii, and existing pointer target sizes.
  In narrow split panes, actions wrap before descriptions become cramped columns.
- Decorative separators are subtle. Interactive input borders and keyboard focus indicators meet
  3:1 against their intended surfaces. Navigation hover, selected, and keyboard-focus states differ.
  Badges do not need an extra outline around an already distinct surface.

`crates/manis-ui/src/theme.rs` is the source of truth. Do not add per-page palettes or reduce text
alpha to create secondary labels. `components.rs` contains the shared heading and dialog treatment.

## Verification

Unit tests enforce opacity, neutral surfaces, distinguishable interaction fills, 4.5:1 normal-text
contrast, 3:1 control/focus contrast, destructive button contrast, native window opacity, and
component token synchronization through light → dark → light changes.

On macOS, capture the synthetic appearance matrix:

```bash
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --appearance
```

This covers all six workspaces at 1420×900, 1060×800, 720×720, and the minimum 640×560 size in light
and dark modes, plus source dialogs and nested popovers. It asserts screenshot alpha and samples
an unobstructed chrome pixel to confirm the actual theme rather than trusting coordinate clicks.
All outputs go to the ignored `target/manis-snapshots/` directory. Review text wrapping, clipping,
popup occlusion, and focus visibility; pixel opacity alone cannot prove good layout or native
compositor behavior. Linux/Windows CI validates compilation; native visual checks are separate.

Use only synthetic sources in shareable screenshots. Never attach actual subscription addresses,
node credentials, or private traffic logs to a PR.

Reviewed fixture examples: [light workspace](assets/interface-light.png),
[dark workspace](assets/interface-dark.png), [nested popup](assets/interface-popover.png), and
[minimum-size dialog](assets/interface-minimum-dialog.png). These are GPUI renders of synthetic
data, not screenshots of a user's live proxy session.
