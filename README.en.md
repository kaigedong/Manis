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
- Nodes and policy groups share the Nodes page in one scrolling document; sources are managed in Configuration.
- Direct, global, and rule routing modes.
- System HTTP/SOCKS proxy control and macOS TUN integration.
- Live connections, route evidence, core logs, and redacted application diagnostics.
- Mihomo as the primary managed core and a capability-gated sing-box adapter.

QX rule-list import accepts `HOST` / `DOMAIN`, `HOST-SUFFIX` / `DOMAIN-SUFFIX`, and
`HOST-KEYWORD` / `DOMAIN-KEYWORD`, regardless of case. Other rule types are skipped and
counted in the import diagnostics; they are not silently converted to domain rules.

The Manis source repository does not commit a prebuilt proxy core. Release builds download the
stable asset for their architecture from the official Mihomo release, verify its upstream SHA-256,
and include it as a first-launch seed. The application then uses and updates only its managed core.

## Platform status

| Platform | Build target | Runtime status |
| --- | --- | --- |
| macOS 13+ | Maintained | Primary development platform; system proxy verified, TUN supported for testing through administrator approval |
| Windows | Maintained | Experimental; managed controller transport is not implemented yet |
| Linux | Maintained | Experimental; native packages and desktop environments need broader testing |

The CI configuration checks all three platforms. A green compile check is not a claim that every
network integration has been validated on that operating system.

The Package workflow builds unnotarized macOS bundles for Apple Silicon and Intel, plus an
experimental Arch Linux `x86_64` package with native Wayland support, and generates checksums for
each package. Every commit merged into `main` replaces the public rolling `latest` pre-release,
while manual workflow artifacts are retained for 14 days. Pushing any version tag also creates a
separate draft GitHub Release until a maintainer completes the release checklist and decides
whether to publish it. See the [macOS](packaging/macos/README.md) and
[Arch Linux](packaging/archlinux/README.md) packaging notes before installing them.

## Download a test build

The most recent successful `main` build is available from the `Latest Manis development build`
pre-release on the [Releases page](https://github.com/kaigedong/Manis/releases), or download a single run from the
[Package workflow page](https://github.com/kaigedong/Manis/actions/workflows/package.yml). Choose
`arm64` for an Apple Silicon Mac, `x86_64` for an Intel Mac, or the `.pkg.tar.zst` package for
CachyOS and Arch Linux. Version tags continue to produce maintainer-only draft releases.

The macOS archives are ad-hoc signed but do not have a Developer ID signature or Apple
notarization, so they are test builds only. macOS Gatekeeper still warns on first open; use only a
trusted official release and verify the archive with its accompanying `.sha256` file before opening
it. GitHub downloads support TUN by default, but the first TUN enablement and every changed Manis
app version require administrator authorization. That path installs a root-owned LaunchDaemon that
pins the approved app, `manis-helperctl`, privileged-helper fingerprints, and the current user ID,
and does not require a paid Apple Developer Program account. The Developer ID/SMAppService signed
path remains available as an optional maintainer release route.

Manis checks for a new version at startup and every hour. It only reads version metadata and never
downloads or installs application updates. View the result under Configuration → App updates at the
bottom of the settings page, then use “View on GitHub” to download and install a release yourself.
The link remains available if the check fails. The section also shows the current version and project
address. Choose “About Manis” from the tray menu to view the same information in a dialog. Manual
Mihomo core updates are unchanged.

## Build from source

The repository pins its Rust toolchain in `rust-toolchain.toml` and its GPUI revision in
`crates/manis-ui/Cargo.toml`.

On macOS, install Xcode Command Line Tools. On Linux, install the Wayland/X11, fontconfig, and
ALSA development packages required by GPUI. The Linux tray uses StatusNotifierItem over the session
D-Bus without GTK 3; GNOME needs an AppIndicator extension to display it. Then run:

```bash
git clone https://github.com/kaigedong/Manis.git
cd Manis
cargo run -p manis-ui
```

Manis only starts Mihomo processes it owns and only runs configuration generated from data managed
through the application. It does not attach to another application's controller or run a supplied
Mihomo YAML file. Release packages include a SHA-256-verified stable upstream seed. On first launch
it is installed into Manis's private data directory; subsequent downloads, version validation,
atomic replacement, and rollback are handled by the in-app core updater.

Before the first node is added, Manis prepares a direct-only bootstrap configuration. After a
subscription or individual node is added, Manis validates and writes the generated configuration to
its private runtime directory. The controller endpoint is also assigned by Manis, not configured by
the user.

## Configuration backup and editing

Under Settings → General → Backup and migration, export a complete `.manis.json` file and import it
on another device. Export and file import show only the system file picker; import opens a preview
after the selected file passes validation. Progress and results appear in the bottom status bar
without resizing the settings card. **Edit configuration** opens the current complete
configuration in a text editor. Edit it directly or paste an exported file, then validate and preview.
Even a configuration with stale policy references can be opened for repair, but invalid drafts cannot
be applied. You can return to editing or cancel without changing the stored configuration.

Only **Replace and restart** applies the preview: Manis stops its proxy and core, backs up the old
configuration, replaces it and restarts. This is Manis's backup format, not Mihomo YAML. It contains
plaintext credentials; keep it private. Core binaries, TUN permissions, logs and latency results
are excluded, and importing does not enable a proxy.

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

Proxy and rule sources in Settings highlight the full card on hover, with enable checkboxes centered
vertically against the complete card content.

Automatic and manual policy groups use the same neutral selection fill and Current outlet label,
without leading radio circles or checkmarks. Policy groups expand downward and scroll when needed without
compressing adjacent cards. The routing
rules page places its description, rule counts, and Add button in one header above the groups; group
headers show a pointer cursor on hover. Log references, levels, messages, and timestamps are vertically
centered within each row. See [List layout checks](docs/development.md#list-layout-checks) for the
reproduction commands.

## Security and privacy

Subscription URLs, tokens, node credentials, controller secrets, and generated core configurations
are private data. Manis stores them outside the repository in the platform user-data directory and
redacts them from its own diagnostics. Plain HTTP subscriptions remain inherently observable on the
network; HTTPS should be used whenever possible.

The macOS TUN path uses a fixed-purpose privileged helper. GitHub ad-hoc packages pin this app
version's code fingerprints through administrator approval; the old
`MANIS_ALLOW_INSECURE_LOCAL_HELPER` local development bypass is obsolete and must not be used for
distribution. Review [SECURITY.md](SECURITY.md) before testing privileged behavior and report
vulnerabilities through GitHub private vulnerability reporting.

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
