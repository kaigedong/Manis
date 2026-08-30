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

- Route Linux DNS through Mihomo's TUN hijacker so fake-IP routing and domain-aware proxying are
  not bypassed by `systemd-resolved`, and report the actual platform DNS strategy in diagnostics.

[Unreleased]: https://github.com/kaigedong/Manis/commits/main
