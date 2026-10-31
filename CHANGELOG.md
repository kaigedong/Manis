# Changelog

All notable changes to Manis will be documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases will use semantic versioning
once the public API and configuration formats stabilize.

## [Unreleased]

### Added

- Rust and GPUI desktop application for policy-based proxy routing.
- Android client (`android/`) with a `VpnService` TUN, a bundled patched Mihomo core, and a
  workspace model compiled by the same Rust profile code as the desktop: multiple subscriptions,
  VLESS imports, user policy groups, QX rules, rule/global/direct modes, node switching with
  latency tests, and kernel logs.
- Android: subscription sources can name their own resolvers. `proxy-server-nameserver` entries
  found in a subscription document are read automatically, and Settings → 节点域名解析 → 自定义 DoH
  accepts a manual list. Airports that rotate node entry addresses only answer correctly through
  their own DoH, so public resolvers make every node look unreachable.
- Android: the subscription screen can refresh a provider on demand (`PUT /providers/proxies`)
  without restarting the core, and the kernel log dialog can copy its contents.
- The configuration editor shows a flat JSONC document instead of the per-entry readable text,
  and the importer accepts both. Hand-written comments in `config.toml` are no longer preserved
  through an edit, because JSONC is a projection of the stored entries.
- Mihomo management with a private, managed runtime.
- Subscription and VLESS import, policy groups, ordered routing rules, latency testing, network
  activity, logs, system proxy control, and macOS TUN integration.
- English and Simplified Chinese interface with system-language detection.
- Optional login auto-start, enabled by default and configurable under Configuration → General.
- Single-instance behavior: launching Manis again brings the existing window forward instead of
  starting a second copy.
- Clicking a connection in Network activity opens the routing-rule editor with its domain (or
  destination address) and current policy filled in, so one row can be routed elsewhere quickly.
- Opening Manis starts the managed kernel automatically (Configuration → Startup → "Start
  connected", on by default). A missing core is installed first; one failed start is retried once
  before the error stays on screen with the manual retry button.
- The Package workflow publishes the Android `arm64-v8a` debug APK as
  `Manis-<version>-android-arm64-debug.apk` with a `.sha256` file on the rolling `latest`
  pre-release and on `build-<version>` releases; its version name and code follow the release.

### Changed

- Settings opens inside the main window again: the sidebar item and the settings shortcut no
  longer create a separate window.

- Upgrade every direct dependency to its newest release, including GPUI 0.3.9, GPUI Component 0.8,
  `zip` 9, `zbus` 6, and `ureq` 3.5. The core archive reader accepts `zip` 9's fallible entry
  names, and the Linux tray integration test drives `zbus` 6 from its process-wide multithreaded
  runtime now that the blocking API is gone.

### Removed

- Android: remove the proxy-server "address override" (hosts pinning) from the workspace and the
  settings dialog. A pinned address silently outlives an airport's address rotation and then breaks
  every subscription node while manual nodes keep working; the encrypted DNS default resolves the
  current address instead. Stale overrides in an existing workspace are ignored on load.

### Security

- Remove vulnerable `glib` (GHSA-wrw7-89jp-8q8g / RUSTSEC-2024-0429) and the unmaintained GTK 3 Rust
  bindings by using a GTK-free StatusNotifierItem tray on Linux. Remove both audit exceptions;
  macOS and Windows retain their native tray backend with GTK features disabled.
- Restricted controller endpoints, private credential storage, redacted diagnostics, owned-process
  lifecycle management, and fixed-purpose macOS privileged-helper boundaries.
- Install Linux TUN capabilities with the package and restrict passwordless DNS repair to a
  root-owned, fixed-purpose PolicyKit helper, avoiding repeated broad administrator prompts.

### Fixed

- Android: ship the Manis launcher icon, round icon, and explicit launcher label. The APK had no
  icon resource at all, so phones showed the generic Android marker and some launchers did not list
  the sideloaded app in search. The launcher activity now repeats the icon and label instead of
  relying on the application defaults.
- Linux: keep the status item alive when Manis starts at login before the desktop shell exports
  `org.kde.StatusNotifierWatcher`. The icon now appears as soon as the host is ready, and close
  hides instead of quitting; previously the first launch installed no tray, so closing the window
  exited Manis until the next manual start. A shell restart also re-registers the item.
- Route Linux `systemd-resolved` queries through the managed TUN interface, flush stale poisoned
  answers, and restore the original resolver route when TUN stops so fake-IP and domain rules work
  even when DHCP supplies a private gateway DNS server.
- Reapply Linux link-scoped DNS routing whenever a managed configuration restart recreates the TUN
  interface, instead of silently falling back to the physical interface DNS.
- Android: resolve proxy server hostnames with the same encrypted DNS as the desktop by default.
  The active network's plaintext resolvers, previously always passed to the core, can answer an
  airport's node domain with a blocked address and leave every node timing out; they are now an
  explicit "本机 DNS" option under Settings → 节点域名解析.

[Unreleased]: https://github.com/kaigedong/Manis/commits/main
