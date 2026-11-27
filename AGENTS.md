# Manis Agent Guide (`AGENTS.md`)

Concise architectural rules, invariants, and verification commands for AI coding agents working in this repository.

## 1. Repository Map

| Crate / Path | Responsibility |
|---|---|
| `crates/manis-core` | Pure domain model (`PolicyCatalog`, `PolicyGroup`, `PolicyNode`, `RoutingMode`, `ProxyMode`, `PolicyWorkspaceState`). No UI or I/O. |
| `crates/manis-profile` | Pure Mihomo YAML compiler, VLESS/subscription parsers, rule normalization, and private-file helpers (`write_private_atomic` for **loose runtime/cache files**, NOT `config.toml`). |
| `crates/manis-mihomo` | Read/write HTTP & Unix-socket Mihomo controller client (`/version`, `/proxies`, `/configs`, `/connections`, delay probes). |
| `crates/manis-engine` | Managed Mihomo process lifecycle, readiness polling, and safe shutdown. |
| `crates/manis-linux-helper` | Privileged Linux TUN/DNS helper binary (strict authorization boundaries). |
| `crates/manis-android` | Android JNI / UniFFI bridge for the Kotlin app under `android/`. |
| `crates/manis-ui` | GPUI desktop app (`ManisApp`), settings/topology views, `config_toml` / `config_jsonc` persistence, `core_update` / `app_update`, local Control API (`control_api`), Agent CLI (`cli`), and built-in stdio MCP server (`mcp`). |

Reference docs:
- Product contract: `docs/PRODUCT.md`
- UI & design system: `docs/DESIGN.md`, `design-system/manis/MASTER.md`
- Developer workflow: `docs/development.md`
- Agent CLI & MCP integration: `docs/agent-integration.md`

## 2. Hard Invariants (Do Not Violate)

1. **`unsafe_code = "forbid"`** workspace-wide, and **`clippy::pedantic` with `-D warnings`** in CI. Always run `cargo clippy --workspace --all-targets -- -D warnings`.
2. **Persistence rule — `config.toml` vs loose files**:
   - Every entry listed in `crates/manis-ui/src/config_toml.rs::is_config_entry_name` MUST be written through `crate::config_toml::write_entry` (or the explicitly named `write_config_entry_bytes` helper) and read via `crate::config_toml::read_entry`.
   - NEVER call `manis_profile::write_private_atomic` for a `config.toml` entry (that writes an orphaned loose file on disk that `config_toml::read_entry` ignores once `config.toml` exists). Reserve `manis_profile::write_private_atomic` strictly for runtime/cache/generated YAML files (`mihomo/`, `cache/`, backups).
3. **Authoritative policy selection**:
   - `PolicyGroup.target` on `self.catalog` is the authoritative runtime/synced target for UI rendering (`selected_node_for_policy`). Always keep `self.workspace` in sync via `PolicyWorkspaceState::set_selection_for`, and re-apply `self.managed_policies.node_selections` during refreshed snapshots (`apply_refreshed_policy_snapshot`).
4. **Security & redaction**:
   - Keep the GUI unprivileged; never bypass helper signatures, symlink checks, or `0o700` / `0o600` private-directory permissions.
   - All agent-facing surfaces (CLI `config show`, local Control API, MCP tools) MUST redact subscription tokens/credentials, node passwords/UUIDs, and controller secrets by default.
5. **Arch Linux packaging**:
   - `packaging/archlinux/PKGBUILD` MUST keep `options=('!lto' '!debug')` so C static archives compiled by `cc-rs` (`ring`, `wayland-backend`, `tree-sitter`, `tree-sitter-json`) remain linkable by Rust.
6. **Live user environment**:
   - Never modify the user's running proxy mode, system proxy, TUN routes, installed `Manis.app`, or live `~/Library/Application Support/Manis` data directory during development or testing.

## 3. Agent Surface (`manis` CLI & MCP)

The `manis-ui` / `manis` binary launches the GPUI desktop app with no arguments and acts as a headless Agent CLI when passed a subcommand:
- `manis status --json` — runtime, version, proxy/routing mode, connection counts
- `manis policies --json` — policy groups, strategies, current exits, candidates, latencies
- `manis connections [--host ...] [--process ...] [--limit N] --json` — live connections
- `manis rules --json` — ordered manual rules, local groups, remote rule sources, fallback
- `manis explain <target> [--process ...] --json` — deterministic `rule → policy group → exit` trace
- `manis select <group> <node> --json` — validated manual policy switch (persisted + applied live)
- `manis routing-mode <rule|global|direct> --json` — switch routing mode
- `manis config show --json` — redacted structured configuration view
- `manis config validate <path|-> --json` — dry-run JSONC validation + change preview
- `manis mcp` — stdio JSON-RPC 2.0 MCP server backed by the same local Control API

## 4. Verification Commands

Run these before finishing any Rust change:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p manis-ui --example snapshot --features snapshot-fixtures
cargo test --workspace --quiet
```
