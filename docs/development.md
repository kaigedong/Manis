# Development guide

This guide documents local runtime modes and tests that are intentionally too detailed for the
project overview. All examples use synthetic values. Never place a private subscription URL or node
credential in a shell history, issue, screenshot, fixture, or commit.

## Toolchain and native dependencies

Rust is pinned in `rust-toolchain.toml`. On macOS, install Xcode Command Line Tools. On Linux, install
the Wayland/X11, fontconfig, and ALSA development packages listed in `.github/workflows/ci.yml`.
The Linux tray uses `ksni` over the session D-Bus, with no GTK or libappindicator dependency. A
StatusNotifierItem host is required (for example, Plasma or GNOME with an AppIndicator extension).
Without a compatible host, the app reports the tray as unavailable and retains normal window-close
behavior instead of hiding into an inaccessible tray.

Linux tray tests (the second command supplies its own mock host on a private session bus):

```bash
cargo test -p manis-ui --lib linux_tray --locked
dbus-run-session -- cargo test -p manis-ui --lib linux_tray_dbus --locked -- --ignored
```

```bash
cargo run -p manis-ui
```

The generated runtime, subscription state, rules, logs, and credentials are stored in the platform
user-data directory, never in the repository.

## Core selection

Mihomo is the default core. Production builds only use the executable in Manis's private data
directory. A packaged seed is installed there on first launch; if it is absent, Manis can download
the current stable release from the official GitHub release, verify the release asset digest, check
the reported version, and publish it atomically. It never searches `PATH`, Homebrew, or another
application's files.

Debug builds may override the managed executable for local development:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo cargo run -p manis-ui
```

## Managed Mihomo runtime

Mihomo has one production runtime path: Manis builds a private configuration from its saved sources,
validates that configuration, starts the child process, owns the controller endpoint, and stops the
same child on exit. The binary override below is development-only; release builds ignore external
Mihomo paths:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
MANIS_MIHOMO_DATA_DIR=/absolute/path/to/manis-runtime \
cargo run -p manis-ui
```

`MANIS_MIHOMO_CONTROLLER`, `MANIS_MIHOMO_SECRET`, `MANIS_MIHOMO_CONFIG`, and
`MANIS_MIHOMO_SUBSCRIPTION_FILE` are not supported runtime inputs. Import subscriptions and nodes
through the UI so the product model remains the configuration source of truth.

## Diagnostics

Controller connection problems appear in the shared status bar rather than replacing the network
activity or log workspace. Connection snapshots use finite polling; a quiet log stream does not
become a timeout merely because it has no new records.

Enable redacted lifecycle events with:

```bash
MANIS_UI_TRACE=debug cargo run -p manis-ui
```

Manis diagnostics contain fixed event names and redacted metadata. Raw core logs can still contain
data produced by the core; inspect them before sharing.

Application lifecycle records in `logs/manis-events.log` use JSON Lines. Existing tab-separated
history is still readable. Both the UI and file retain operation IDs for correlating mode changes,
source imports, and other operations. See [runtime foundations](architecture/runtime-foundations.md)
for the HTTP, configuration, and diagnostics implementation choices and upgrade constraints.

## Portable configuration and policy selection

Under Configuration → General → Backup and migration, export a `.json` file and import it on
another Manis installation. Import accepts a file or clipboard text, validates it, previews item
counts, and requires confirmation before replacing the destination configuration and restarting.
This is a versioned Manis backup, not Mihomo YAML for other clients.

The backup includes subscription URLs, individually saved nodes, policy groups, rule sources and
cached rules, manual rules, routing and node selections, language, and core choice. It excludes core
binaries, system permissions, the active proxy mode, logs, and latency results. Subscription nodes
reload from their saved URLs on the destination. The file contains plaintext credentials: transfer
it privately and never commit it or attach it to an issue.

Before replacement, Manis disables proxy mode, stops its managed core, and backs up the old store
under `configuration-backups` in the user-data directory. It does not merge configurations or
automatically enable system proxy/TUN after restart. Validation failures do not change the store;
write failures attempt rollback. The UI exposes the backup location for recovery.

In a policy group's Node scope, choose “Select nodes or groups” and select **Proxy** to follow the
manual selection on the Nodes page. This references the existing internal selector, not a copy of
the currently chosen node. Switching the home-page exit redirects new connections without editing
the policy; established connections are not forcibly closed. Both manual and automatic policies
can include this candidate. Automatic policies expose the core's native switch tolerance in
milliseconds, persisted as an optional `tolerance-ms` field; old policy files remain readable.
Manis ships the official core without a custom tolerance patch.

The sidebar reuses bundled SVG icons: wide windows retain labels, while narrow windows show icons
with full-name tooltips. Synthetic UI examples: [configuration import preview](assets/configuration-import.png)
and [Proxy candidate and sidebar icons](assets/proxy-candidate.png).

## Tests

The default verification path is offline and deterministic:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo check -p manis-ui --example snapshot --features snapshot-fixtures --locked
```

Clippy type-checks all workspace targets as well as linting them; a separate workspace-wide
`cargo check` is useful for a quick local check but redundant in the full verification sequence.

CI caches Cargo's registry, Git dependencies, and compiled artifacts separately for each OS and
architecture. Cache keys include the pinned toolchain, CI configuration, lockfile, and manifests;
dependency changes can reuse compatible artifacts, and only successful jobs save a new cache.
The first run still builds dependencies; later runs reuse them without skipping tests or linting.
CI disables incremental compilation and uses `line-tables-only` debug information for both dev and
test profiles to reduce build/cache size while keeping file and line information in backtraces.
These environment overrides do not change local development or release profiles.

Optional dependency policy checks:

```bash
cargo audit
cargo deny check
```

Tests against real cores are ignored by default. Read the individual test before enabling it and
use only a synthetic local fixture. The visual snapshot example writes generated files under
`target/manis-snapshots/`, which is ignored by Git:

```bash
cargo run -p manis-ui --example snapshot --features snapshot-fixtures
```

For the light/dark appearance matrix, including minimum-size windows and overlays, use:

```bash
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --appearance
```

Focused synthetic captures for configuration migration (including the prefilled editor in wide and
compact windows, light and dark), sidebar icons, and the Proxy picker:

```bash
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --configuration-transfer
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --app-updates
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --source-cards
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --navigation-icons
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --proxy-candidate
```

See [Interface materials and hierarchy](interface-design.md) for surface, contrast, and visual
review requirements. Snapshot captures reject non-opaque application pixels.

## List layout checks

These macOS captures use synthetic data and do not require a real subscription:

```bash
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --merged-nodes
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --routing-rules
cargo run -p manis-ui --example snapshot --features snapshot-fixtures --locked -- --log-colors
```

The policy fixture contains four groups with 50 nodes each. It asserts that expanding the second group
leaves the first card unchanged and that scrolling changes the visible content. Captures cover the
first and second expanded groups, the final node and following cards, wide/compact windows, and both
themes. The rules capture covers disclosure, edit/add dialogs, and the single page header; the log
capture includes single-line and wrapped messages with centered metadata and severity badges. Re-run
the commands above to reproduce these captures locally.

## macOS bundle and TUN

See `packaging/macos/README.md` for the fixed-purpose privileged helper, signing requirements, and
local development fallback. A development helper build is not a release artifact. Before producing
any distributable bundle, complete `docs/maintainers/release-checklist.md`.
