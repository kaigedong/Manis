# Changelog

All notable changes to Manis will be documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases will use semantic versioning
once the public API and configuration formats stabilize.

## [Unreleased]

### Added

- Rust and GPUI desktop application for policy-based proxy routing.
- Mihomo management and a capability-gated sing-box adapter.
- Subscription and VLESS import, policy groups, ordered routing rules, latency testing, network
  activity, logs, system proxy control, and macOS TUN integration.
- English and Simplified Chinese interface with system-language detection.

### Security

- Restricted controller endpoints, private credential storage, redacted diagnostics, owned-process
  lifecycle management, and fixed-purpose macOS privileged-helper boundaries.

### Fixed

- Route Linux `systemd-resolved` queries through the managed TUN interface, flush stale poisoned
  answers, and restore the original resolver route when TUN stops so fake-IP and domain rules work
  even when DHCP supplies a private gateway DNS server.

[Unreleased]: https://github.com/kaigedong/Manis/commits/main
