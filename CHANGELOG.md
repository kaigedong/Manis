# Changelog

All notable changes to Manis will be documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases will use semantic versioning
once the public API and configuration formats stabilize.

## [Unreleased]

### Added

- Rust and GPUI desktop application for policy-based proxy routing.
- Mihomo management with a private, managed runtime.
- Subscription and VLESS import, policy groups, ordered routing rules, latency testing, network
  activity, logs, system proxy control, and macOS TUN integration.
- English and Simplified Chinese interface with system-language detection.

### Security

- Remove vulnerable `glib` (GHSA-wrw7-89jp-8q8g / RUSTSEC-2024-0429) and the unmaintained GTK 3 Rust
  bindings by using a GTK-free StatusNotifierItem tray on Linux. Remove both audit exceptions;
  macOS and Windows retain their native tray backend with GTK features disabled.
- Restricted controller endpoints, private credential storage, redacted diagnostics, owned-process
  lifecycle management, and fixed-purpose macOS privileged-helper boundaries.
- Install Linux TUN capabilities with the package and restrict passwordless DNS repair to a
  root-owned, fixed-purpose PolicyKit helper, avoiding repeated broad administrator prompts.

### Fixed

- Route Linux `systemd-resolved` queries through the managed TUN interface, flush stale poisoned
  answers, and restore the original resolver route when TUN stops so fake-IP and domain rules work
  even when DHCP supplies a private gateway DNS server.
- Reapply Linux link-scoped DNS routing whenever a managed configuration restart recreates the TUN
  interface, instead of silently falling back to the physical interface DNS.

[Unreleased]: https://github.com/kaigedong/Manis/commits/main
