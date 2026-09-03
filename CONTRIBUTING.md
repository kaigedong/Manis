# Contributing to Manis

Thank you for helping improve Manis. The project is in an early, experimental stage, so small,
well-tested changes are easier to review than broad rewrites.

## Before opening a change

- Search existing issues and pull requests.
- Do not include subscription URLs, tokens, node credentials, private controller addresses, logs
  containing personal data, or screenshots containing real node names.
- Open an issue before introducing a new dependency, changing a persisted file format, altering the
  privileged-helper boundary, or changing project licensing.
- Keep the relationship `rule -> policy group -> node` explicit in product and code changes.

## Development setup

Install the Rust toolchain declared in `rust-toolchain.toml`. macOS development requires Xcode
Command Line Tools. Linux additionally requires the native Wayland/X11, GTK 3, fontconfig, and ALSA
development packages used by GPUI.

```bash
cargo run -p manis-ui
```

Mihomo is an external program and is not included in this repository. Most tests use local fixtures
and do not require a running core.

## Required checks

Run these commands before opening a pull request:

```bash
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

If installed, also run:

```bash
cargo audit
cargo deny check
```

Tests that use a real network, controller, or proxy core must remain ignored by default and require
an explicit environment variable. Never use a private subscription as a test fixture.

## Pull requests

- Describe the user-visible behavior and the trust boundary affected by the change.
- Add regression tests for behavior changes.
- Update English and Chinese documentation together when user-facing behavior changes.
- Keep commits focused and use a single-line Conventional Commit message, for example
  `fix(routing): preserve manual rule order`.
- Expect review of security-sensitive code involving credentials, file permissions, subprocesses,
  system proxy settings, TUN, XPC, or privileged helpers.

By submitting a contribution, you agree that it may be distributed under the repository's license.
See `CODE_OF_CONDUCT.md` for community expectations and `SECURITY.md` for private vulnerability
reporting.
