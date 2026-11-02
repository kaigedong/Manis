# Interface materials and hierarchy

Manis renders with NSH. Its interface uses opaque, subtly tinted surfaces, compact native
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
Manis's dense tables so that even small text meets a 4.5:1 contrast target. NSH owns native
rendering; the shared TSX pages own buttons, inputs, dialogs, popovers, focus, and dismissal.

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

`android/nsh-ui` is the source of truth for page tokens and shared components. Do not add
per-page palettes or reduce text alpha to create secondary labels. [DESIGN.md](../DESIGN.md)
captures the extracted palette, type, spacing and component rules.
Page titles use 24 px / 32 px semibold; section titles use 16 px / 22 px semibold; body uses
13 px / 20 px.

## Native windows

Each NSH shell provides the window and OS integration: GTK 4 on Linux, AppKit on macOS and Win32
on Windows. Page content, navigation, apply bars and dialogs live in the shared TSX pages, so a
layout change reaches every platform at once. The shell supplies only the window, the tray and
safe-area insets. The NSH root keeps one bundle alive while tabs switch, so opening a page does not
reload the frontend or lose its scroll position.

## Verification

Unit tests enforce contrast, distinct interaction surfaces and command capability grants:

```bash
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

The NSH pages have their own render tests in `android/nsh-ui`; CI rebuilds the bundle and fails when
the committed bytecode differs from a fresh build. Use only synthetic sources in shareable
screenshots. Never attach actual subscription addresses, node credentials, or private traffic logs
to a PR.

## Reviewed examples

| Artifact | What it shows |
|---|---|
| [Light Nodes workspace](assets/interface-light.png) | Main Nodes layout with sources above policy groups. |
| [Dark Nodes workspace](assets/interface-dark.png) | The same opaque material hierarchy in dark appearance. |
| [Nested source popup](assets/interface-popover.png) | The legacy inline editor fixture with an anchored popup; this is retained fixture coverage. |
| [Minimum-width proxy source editor](assets/interface-minimum-dialog.png) | Production native utility content at 480 px width in dark appearance, including stable footer and initial input focus. The filename is retained for existing links. |
| [Settings utility content](assets/interface-settings.png) | Production Settings panel with directory, scrolling sections and feedback footer in light appearance. |
| [Held rule priority drag](assets/interface-rule-drag.png) | Historical GPUI drag with floating group label and teal insertion line. |

These are historical GPUI renders of synthetic data, not screenshots of a user's live proxy session or the
operating system's window decoration.
