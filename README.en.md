# Manis

[![CI](https://github.com/kaigedong/Manis/actions/workflows/ci.yml/badge.svg)](https://github.com/kaigedong/Manis/actions/workflows/ci.yml)
[![Security](https://github.com/kaigedong/Manis/actions/workflows/security.yml/badge.svg)](https://github.com/kaigedong/Manis/actions/workflows/security.yml)
[![Package](https://github.com/kaigedong/Manis/actions/workflows/package.yml/badge.svg)](https://github.com/kaigedong/Manis/actions/workflows/package.yml)
[![License](https://img.shields.io/badge/source-Apache--2.0-blue.svg)](LICENSE)

[简体中文](README.md) · **English**

Manis is a small, experimental desktop client built with Rust and GPUI. Its current focus is narrow:
showing how an ordered rule leads to a policy group and then to a proxy node, while providing a UI
for a limited set of common Mihomo and sing-box tasks.

It is a personal open-source experiment, not an attempt to redefine proxy clients or replace mature
tools. Existing clients already serve many users well; Manis mainly explores a workflow that its
maintainers find easier to follow.

> [!WARNING]
> Manis is alpha software. macOS is the current development and runtime-validation platform.
> Windows and Linux builds are maintained as targets but still need broader device testing. Do not
> rely on Manis as the only way to restore network access on a production machine.

## Project scope

Manis concentrates on presenting one routing chain in a direct way:

```text
ordered rule -> policy group -> selected node
```

- Rules decide which policy group handles a connection.
- Manual groups use the node selected by the user.
- Automatic groups benchmark candidates and select according to their configured strategy.
- Global mode uses the active node selected in the Nodes workspace.
- Network activity distinguishes observed core data from local routing predictions.

This model is intentionally limited and will not fit every proxy setup. Manis takes inspiration from
the understandable policy workflow of Quantumult X, but does not copy its interface or configuration
format.

## Implemented so far

- Native adaptive GPUI interface with English, Simplified Chinese, light, and dark modes.
- HTTP/HTTPS subscription and VLESS import with private local persistence.
- Manual, latency-based, fallback, and load-balancing policy group models, gated by core capability.
- Ordered routing rules, QX rule-list import, compound domain/port matching, and explicit fallback.
- Per-source and per-policy latency tests with incremental results.
- Direct, global, and rule routing modes.
- System HTTP/SOCKS proxy control and macOS TUN integration.
- Live connections, route evidence, core logs, and redacted application diagnostics.
- Mihomo as the primary managed core and a capability-gated sing-box adapter.

Manis does not contain or download a proxy-core executable. It discovers an executable supplied by
the user and only manages child processes it starts.

## Platform status

| Platform | Build target | Runtime status |
| --- | --- | --- |
| macOS 13+ | Maintained | Primary development platform; system proxy and TUN tested manually |
| Windows | Maintained | Experimental; managed controller transport is not implemented yet |
| Linux | Maintained | Experimental; native packages and desktop environments need broader testing |

The CI configuration checks all three platforms. A green compile check is not a claim that every
network integration has been validated on that operating system.

The Package workflow builds unsigned macOS bundles for Apple Silicon and Intel, plus an experimental
Arch Linux `x86_64` package with native Wayland support. These workflow artifacts contain no proxy
core and are not notarized production releases. See the [macOS](packaging/macos/README.md) and
[Arch Linux](packaging/archlinux/README.md) packaging notes before installing them.

## Build from source

The repository pins its Rust toolchain in `rust-toolchain.toml` and its GPUI revision in
`crates/manis-ui/Cargo.toml`.

On macOS, install Xcode Command Line Tools. On Linux, install the Wayland/X11, GTK 3, fontconfig, and
ALSA development packages required by GPUI. Then run:

```bash
git clone https://github.com/kaigedong/Manis.git
cd Manis
cargo run -p manis-ui
```

Manis only starts Mihomo processes it owns and only runs configuration generated from data managed
through the application. It does not attach to another application's controller or run a supplied
Mihomo YAML file. Release packages include a SHA-256-verified stable upstream seed. On first launch
it is installed into Manis's private data directory; subsequent downloads, version validation,
atomic replacement, and rollback are handled by the in-app updater.

Before the first node is added, Manis prepares a direct-only bootstrap configuration. After a
subscription or individual node is added, Manis validates and writes the generated configuration to
its private runtime directory. The controller endpoint is also assigned by Manis, not configured by
the user.

## Development checks

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo check -p manis-ui --example snapshot --features snapshot-fixtures --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Real-core tests are ignored by default. They must be enabled explicitly and use
synthetic fixtures; a private subscription must never be committed or used in public test output.
See [CONTRIBUTING.md](CONTRIBUTING.md) for the full workflow.

## Security and privacy

Subscription URLs, tokens, node credentials, controller secrets, and generated core configurations
are private data. Manis stores them outside the repository in the platform user-data directory and
redacts them from its own diagnostics. Plain HTTP subscriptions remain inherently observable on the
network; HTTPS should be used whenever possible.

The macOS TUN path uses a fixed-purpose privileged helper. Development-only insecure helper builds
must never be distributed. Review [SECURITY.md](SECURITY.md) before testing privileged behavior and
report vulnerabilities through GitHub private vulnerability reporting.

## Repository layout

| Path | Responsibility |
| --- | --- |
| `crates/manis-core` | Kernel-neutral policies, routing evidence, and application state |
| `crates/manis-engine` | Core discovery, validation, process ownership, and lifecycle |
| `crates/manis-profile` | Typed profiles and Mihomo/sing-box configuration compilation |
| `crates/manis-mihomo` | Restricted Mihomo controller transport and domain mapping |
| `crates/manis-ui` | GPUI application, persistence boundaries, and platform integration |
| `packaging/macos` | macOS bundle and fixed-purpose privileged-helper tooling |
| `packaging/archlinux` | Experimental Arch Linux package with Wayland support |
| `docs` | Architecture, design, and maintainer documentation |

Additional reading:

- [Product principles](PRODUCT.md)
- [Design system](DESIGN.md)
- [GPUI implementation notes](GPUI_IMPLEMENTATION.md)
- [Development guide](docs/development.md)
- [Direct and compound routing rules](docs/architecture/direct-rules.md)
- [macOS packaging](packaging/macos/README.md)
- [Release checklist](docs/maintainers/release-checklist.md)

## License

Manis source is licensed under [Apache License 2.0](LICENSE). Linked Rust dependencies and optional
proxy-core executables have their own licenses, including GPL components. No prebuilt core is stored
in this repository. Read [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) before distributing a
binary build.
