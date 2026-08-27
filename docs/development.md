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
common Homebrew locations. A development build on macOS can also discover the Mihomo installed by
Clash Verge Rev, but Manis does not stop or modify that application's process.

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

## External controller mode

Use an already running local Mihomo controller:

```bash
MANIS_MIHOMO_CONTROLLER=http://127.0.0.1:9090 \
MANIS_MIHOMO_SECRET='synthetic-controller-secret' \
cargo run -p manis-ui
```

Plain HTTP is accepted only for `localhost` or an IP loopback address. On macOS and Linux, Unix
sockets are supported:

```bash
MANIS_MIHOMO_CONTROLLER=unix:///path/to/mihomo.sock cargo run -p manis-ui
```

External controllers are configuration-read-only: Manis can observe controller state and run an
explicitly requested latency test, but must not rewrite another application's configuration.

## Managed configuration modes

When saved subscriptions or VLESS nodes exist and no external controller is configured, Manis
builds a private configuration, validates it with the selected core, starts a child process, and
stops only that owned child on exit.

An existing Mihomo YAML file can be used for isolated development:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
MANIS_MIHOMO_CONFIG=/absolute/path/to/config.yaml \
MANIS_MIHOMO_DATA_DIR=/absolute/path/to/manis-runtime \
cargo run -p manis-ui
```

For the legacy file-based subscription development mode, create a one-line HTTPS URL file outside
the repository with mode `0600`, then set `MANIS_MIHOMO_SUBSCRIPTION_FILE`. Prefer importing through
the UI for normal development.

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
cargo run -p manis-ui --example snapshot
```

## macOS bundle and TUN

See `packaging/macos/README.md` for the fixed-purpose privileged helper, signing requirements, and
local development fallback. A development helper build is not a release artifact. Before producing
any distributable bundle, complete `docs/maintainers/release-checklist.md`.
