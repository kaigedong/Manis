# Interface materials and hierarchy

Manis keeps GPUI and gpui-component. Its interface uses opaque, subtly tinted surfaces, compact native
typography, and restrained semantic accents. This replaces the previous whole-window blur and
stacked translucent green surfaces; readability must not depend on the user's wallpaper.

## Reference and scope

The reference is OpenAI's public [UI guidelines](https://developers.openai.com/plugins/concepts/ui-guidelines)
and [Apps SDK UI design tokens](https://github.com/openai/apps-sdk-ui/tree/0f00143c7a639906f1621fe58e1b6be7b5bea46d/src/styles).
They document neutral system colors, limited accents, a small typography scale, consistent spacing,
and clear content/action hierarchy. They are a public design reference, **not** evidence of the private
ChatGPT desktop implementation. Manis does not embed that React library, reproduce ChatGPT branding,
or change its proxy-management information architecture.

The spacing and hierarchy follow that reference; Manis uses its own restrained teal action accent. Metadata and semantic text colors are adjusted for
Manis's dense tables so that even small text meets a 4.5:1 contrast target. GPUI still owns native
rendering; gpui-component still owns buttons, inputs, dialogs, popovers, focus, and dismissal.

## Rules

- The Nodes page contains source/node lists followed by policy groups, separated by a subtle rule.
  Use one page scroll container; expanded groups retain their natural height and push later items
  down. Do not restore a separate policy workspace or availability counters/filter controls.
- Native windows use `WindowBackgroundAppearance::Opaque`. The integrated macOS title bar remains
  transparent *to the application's opaque chrome*, not to the desktop. Do not restore backdrop blur.
- Window, page, navigation, card, input, table, dialog, and popup surfaces are fully opaque in both
  themes. Tint and luminance differences establish hierarchy. Transparent layout-only containers may
  inherit an opaque parent; modal scrims and shadows may blend over the application.
- Main content is `#fafbfc` / `#15191e`, chrome is `#e9eef2` / `#1c2229`, and popup/editor
  surfaces are white / `#242c35`. Popups conceal the text beneath them.
- Primary commit actions use teal `#176c62` / `#79d7c6` with a contrasting label and semibold
  text. Secondary actions use a subdued tinted fill and medium text. Structural actions such as
  adding an AND condition stay secondary. All buttons use `action_button` / `style_action_button`;
  hover, press, loading and disabled states come from the shared component palette.
  Teal also identifies selections; green, amber, red and warm route accents retain status semantics.
- Log level badges use blue for INFO, purple for DEBUG, cyan for TRACE, amber for WARN, and red
  for ERROR/FATAL/PANIC, with a subtle matching fill. Keep log messages neutral for readability;
  level labels remain visible so severity never depends on color alone.
- Destructive buttons have an independent red-fill/white-label contrast pair; never derive their
  labels from the dark-mode primary button. Input caret, selection, tabs, scrollbars, and chrome are
  projected into the same component theme. Synchronize both legacy colors and render tokens.
- Page/section headings use semibold, labels/data medium, and supporting text normal weight.
  Preserve the 4/8/12/16/24 px spacing scale and 8/12 px corner radii. Text buttons share 14 px labels,
  centered in 34 px compact or 38 px standard controls. Icon-only buttons use a 32 px square target.
  Buttons and navigation use a pointer; rule reorder handles use the native open-hand cursor.
  In narrow split panes, actions wrap before descriptions become cramped columns.
- Decorative separators are subtle. Interactive input borders and keyboard focus indicators meet
  3:1 against their intended surfaces. Navigation hover, selected, and keyboard-focus states differ.
  Badges do not need an extra outline around an already distinct surface.

`crates/manis-ui/src/theme.rs` is the source of truth. Do not add per-page palettes or reduce text
alpha to create secondary labels. `components.rs` contains the shared heading and dialog treatment.
[DESIGN.md](../DESIGN.md) captures the extracted palette, type, spacing and component rules.
Page titles use 24 px / 32 px semibold; section titles use 16 px / 22 px semibold; body uses
13 px / 20 px. Editors retain their existing compact title treatment.

## Native windows and rule priority

Settings opens one native window per application model from the navigation or Cmd/Ctrl+, and leaves
the main workspace usable. Proxy source, rule source, manual rule and policy group editors each have their
own native window, with one draft per editor kind. Reopening activates the existing draft; cancellation clears it.
Editors own their focus, scrolling, resize and Cmd/Ctrl+W/Escape dismissal; their first input receives
initial focus. Background imports and policy saves refuse cancellation while busy. Settings refuses
native close during an active configuration transfer. Short confirmations and the configuration replacement
preview remain modal to their launching window. Utility windows never replace the main resize
subscription or become the tray's main-window target.

Default settings size is 980×760; editor size is 720×660. Both can resize down to 480×480 logical
pixels. At compact widths, the settings directory is a three-column, two-row grid. Stable editor
headers and footers frame scrollable form content so commit actions remain visible. The settings footer shares current application feedback.

Routing priority uses a 32 px six-dot `GripVertical` handle, a floating group label and a teal insertion
line before or after the drop target. Dragging at the viewport edges scrolls the list. Dropping saves and applies
through the existing transaction; failed applies restore the previous order. Escape cancels a drag;
Alt+Up/Down on a focused handle preserves keyboard reordering. No up/down arrow buttons remain.
The handle reserves a transparent 2 px border, replaced by a blue focus ring when reached with the
keyboard; tests check at least 3:1 contrast on its supported surfaces. A manual rule draft blocks
main-list deletion to keep its index stable until saved or cancelled; configuration replacement waits
for native editors to close.

The native host and close guards are defined in [app/windows.rs](../crates/manis-ui/src/app/windows.rs),
compact Settings navigation in [settings/navigation.rs](../crates/manis-ui/src/app/configuration/settings/navigation.rs),
and drag feedback and edge scrolling in [list/reorder.rs](../crates/manis-ui/src/app/configuration/manual_rules/list/reorder.rs).

## Verification

Unit tests enforce opacity, distinct surfaces, distinguishable interaction fills, 4.5:1 normal-text
contrast, 3:1 control/focus contrast, destructive button contrast, native window opacity, and
component token synchronization through light → dark → light changes.

On macOS, capture the synthetic appearance matrix:

```bash
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --appearance
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --buttons
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --utility-windows --routing-rules-review
```

The appearance matrix retains five legacy fixture workspaces at 1420×900, 1060×800, 720×720,
and the minimum main-window 640×560 size in light and dark modes, plus source dialogs and nested
popovers. Fixture mode intentionally keeps inline Settings and legacy editor dialogs for that matrix;
it does not describe the production window-opening path. `--utility-windows` renders the actual
production utility panel builders in representative offscreen captures at default widths of 980 px
(Settings) and 720 px (editors), plus 480 px minimum-width examples. The set includes light and dark
appearance examples; it does not capture every editor kind in both themes or the absolute 480×480
minimum window size. `--routing-rules-review` captures rule priority layouts, a held mouse drag and
visible keyboard focus. `--buttons` separately captures shared action variants.

The capture harness asserts screenshot alpha and samples an unobstructed chrome pixel to confirm
the actual theme. All outputs go to the ignored `target/manis-snapshots/` directory. Review text
wrapping, clipping, popup occlusion and focus visibility; pixel opacity alone cannot prove good
layout. The refreshed review set includes 21 captures. The initial review identified a missing visible
focus treatment on the reorder handle; the reserved blue border resolves that finding in both themes.

The final verification passed 476 workspace tests (7 ignored) and clippy for all targets including
snapshot fixtures. Native view tests exercise retained drafts, close guards, focus ownership and
reorder transaction rollback. Captures are synthetic **GPUI content renders on macOS**, without real
OS title bars or desktop compositor output. They establish content layout and appearance; they do
not establish OS compositing, platform window behavior on Windows/Linux, or a live kernel session.
Cross-platform compilation is a separate check from native runtime and visual validation.

Use only synthetic sources in shareable screenshots. Never attach actual subscription addresses,
node credentials, or private traffic logs to a PR.

## Reviewed examples

| Artifact | What it shows |
|---|---|
| [Light Nodes workspace](assets/interface-light.png) | Main Nodes layout with sources above policy groups. |
| [Dark Nodes workspace](assets/interface-dark.png) | The same opaque material hierarchy in dark appearance. |
| [Nested source popup](assets/interface-popover.png) | The legacy inline editor fixture with an anchored popup; this is retained fixture coverage. |
| [Minimum-width proxy source editor](assets/interface-minimum-dialog.png) | Production native utility content at 480 px width in dark appearance, including stable footer and initial input focus. The filename is retained for existing links. |
| [Settings utility content](assets/interface-settings.png) | Production Settings panel with directory, scrolling sections and feedback footer in light appearance. |
| [Held rule priority drag](assets/interface-rule-drag.png) | Actual GPUI drag with floating group label and teal insertion line. |

These are GPUI renders of synthetic data, not screenshots of a user's live proxy session or the
operating system's window decoration.
