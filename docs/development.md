# Development guide

This guide documents local runtime modes and tests that are intentionally too detailed for the
project overview. All examples use synthetic values. Never place a private subscription URL or node
credential in a shell history, issue, screenshot, fixture, or commit.

## Architecture in one paragraph

The desktop product is two processes: `manis` (the NSH host, `crates/manis-nsh-desktop`) and
`manis-engine` (the headless engine, `crates/manis-service`). The host renders the shared TSX bundle
through GTK 4, AppKit or Win32 and calls the Rust bridge in `crates/manis-service/src/nsh.rs`. The
engine owns the Mihomo process and reapplies the saved profile when remote sources come due. Neither
process links a UI toolkit other than the native shell.

## Toolchain and native dependencies

Rust is pinned in `rust-toolchain.toml`. On macOS, install Xcode Command Line Tools. On Linux,
install GTK 4 with its Wayland/X11 development packages (see `.github/workflows/ci.yml`).
The Linux tray uses `ksni` over the session D-Bus, with no GTK or libappindicator dependency. A
StatusNotifierItem host is required (for example, Plasma or GNOME with an AppIndicator extension).

```bash
cargo run -p manis-nsh-desktop
```

The generated runtime, subscription state, rules, logs, and credentials are stored in the platform
user-data directory, never in the repository.

## Engine discovery

The NSH host looks for the engine next to its own executable (`manis-engine`), then on `PATH`; set
`MANIS_ENGINE_BIN` to point at an explicit build during development. If a controller socket is
already answering, the host attaches to it instead of starting another engine.

Releases install `/usr/bin/manis` and `/usr/bin/manis-engine`; the macOS app ships
`Contents/MacOS/Manis` and `Contents/MacOS/manis-engine`; the Windows zip ships `Manis.exe` and
`manis-engine.exe`.

## Core selection

Mihomo is the default core. Production builds only use the executable in Manis's private data
directory. A packaged seed is installed there on first launch. Manis never searches `PATH`,
Homebrew, or another application's files.

Debug builds may override the managed executable for local development:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo cargo run -p manis-engine
```

Mihomo has one production runtime path: Manis builds a private configuration from its saved sources,
validates that configuration, starts the child process, owns the controller endpoint, and stops the
same child on exit.

`MANIS_MIHOMO_CONTROLLER`, `MANIS_MIHOMO_SECRET`, `MANIS_MIHOMO_CONFIG`, and
`MANIS_MIHOMO_SUBSCRIPTION_FILE` are not supported runtime inputs. Import subscriptions and nodes
through the UI so the product model remains the configuration source of truth.

## Diagnostics

Enable redacted lifecycle events with:

```bash
MANIS_ENGINE_TRACE=debug cargo run -p manis-engine
```

Manis diagnostics contain fixed event names and redacted metadata. Raw core logs can still contain
data produced by the core; inspect them before sharing. Application lifecycle records in
`logs/manis-events.log` use JSON Lines. See
[runtime foundations](architecture/runtime-foundations.md) for the HTTP, configuration, and
diagnostics implementation choices and upgrade constraints.

## Portable configuration

Under Settings → Backup and migration, export a `.json` file and import it on another Manis
installation. This is a versioned Manis backup, not Mihomo YAML for other clients. It contains
plaintext credentials: transfer it privately and never commit it or attach it to an issue. Before
replacement, Manis disables proxy mode, stops its managed core, and backs up the old store under
`configuration-backups` in the user-data directory.

## Tests

The default verification path is offline and deterministic:

```bash
cargo fmt --all -- --check
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Clippy type-checks all workspace targets as well as linting them; a separate workspace-wide
`cargo check` is useful for a quick local check but redundant in the full verification sequence.

CI caches Cargo's registry and compiled artifacts separately for each OS and architecture. Cache keys
include the pinned toolchain, CI configuration, lockfile, and manifests; dependency changes can reuse
compatible artifacts, and only successful jobs save a new cache. The first run still builds
dependencies; later runs reuse them without skipping tests or linting. CI disables incremental
compilation and uses `line-tables-only` debug information for both dev and test profiles to reduce
build/cache size while keeping file and line information in backtraces.

Optional dependency policy checks:

```bash
cargo audit
cargo deny check
```

Tests against real cores are ignored by default. Read the individual test before enabling it and use
only a synthetic local fixture.

## NSH pages

The shared TSX pages live in `android/nsh-ui` and are compiled to
`android/app/src/main/assets/nsh/bundle.nshc`. Rebuild them with:

```bash
MANIS_BUILD_NSH=1 android/build-apk.sh
```

The desktop shells load the same compiled bundle. Host commands and their capability grants are
declared in `manis-presentation::COMMANDS` and checked by a unit test, so adding a page command
requires updating both the TSX call and the capability list.

## macOS bundle and TUN

See `packaging/macos/README.md` for the fixed-purpose privileged helper, signing requirements, and
local development fallback. A development helper build is not a release artifact. Before producing
any distributable bundle, complete `docs/maintainers/release-checklist.md`.
