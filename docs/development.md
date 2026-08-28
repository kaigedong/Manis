# Development guide

This guide documents local runtime modes and tests that are intentionally too detailed for the
project overview. All examples use synthetic values. Never place a private subscription URL or node
credential in a shell history, issue, screenshot, fixture, or commit.

## Toolchain and native dependencies

Rust is pinned in `rust-toolchain.toml`. On macOS, install Xcode Command Line Tools. On Linux, install
the Wayland/X11, GTK 3, fontconfig, and ALSA development packages listed in `.github/workflows/ci.yml`.

```bash
cargo run -p manis-ui
```

The generated runtime, subscription state, rules, logs, and credentials are stored in the platform
user-data directory, never in the repository.

## Core selection

Mihomo is the default core. Manis looks for an executable beside the application, in `PATH`, and in
common Homebrew locations. It does not discover or reuse a core installed by another application.

Override discovery explicitly when needed:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo cargo run -p manis-ui
```

The sing-box adapter discovers `sing-box` from `MANIS_SING_BOX_BINARY`, `PATH`, and common Homebrew
locations. The UI only permits switching when the saved configuration can be translated without
silently changing behavior.

```bash
MANIS_SING_BOX_BINARY=/absolute/path/to/sing-box cargo run -p manis-ui
```

Neither executable is downloaded or stored by this repository.

## Managed Mihomo runtime

Mihomo has one production runtime path: Manis builds a private configuration from its saved sources,
validates that configuration, starts the child process, owns the controller endpoint, and stops the
same child on exit. A local official Mihomo executable can be selected explicitly:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
MANIS_MIHOMO_DATA_DIR=/absolute/path/to/manis-runtime \
cargo run -p manis-ui
```

`MANIS_MIHOMO_CONTROLLER`, `MANIS_MIHOMO_SECRET`, `MANIS_MIHOMO_CONFIG`, and
`MANIS_MIHOMO_SUBSCRIPTION_FILE` are not supported runtime inputs. Import subscriptions and nodes
through the UI so the product model remains the configuration source of truth.

## Diagnostics

Enable redacted lifecycle events with:

```bash
MANIS_UI_TRACE=debug cargo run -p manis-ui
```

Manis diagnostics contain fixed event names and redacted metadata. Raw core logs can still contain
data produced by the core; inspect them before sharing.

## Tests

The default verification path is offline and deterministic:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check -p manis-ui --example snapshot --features snapshot-fixtures --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

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

## macOS bundle and TUN

See `packaging/macos/README.md` for the fixed-purpose privileged helper, signing requirements, and
local development fallback. A development helper build is not a release artifact. Before producing
any distributable bundle, complete `docs/maintainers/release-checklist.md`.
