# Changelog

All notable changes to Manis will be documented in this file. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and releases will use semantic versioning
once the public API and configuration formats stabilize.

## [Unreleased]

### Added

- `manis-presentation::ConnectionPageView`: Android and desktop now build the connection page
  through the same Rust model (active/node/mode/summary/error/warnings/hasSources).
- `manis-presentation::SubscriptionsPageView`: subscription/manual-node/group rows, default
  names, indices and URL masking are shared. URL userinfo is now stripped from the displayed
  source address on both platforms.
- `manis-presentation::RulesPageView`: rule rows, policy labels, policy deduplication, rule kinds
  and warning limits are shared; Android and desktop both build the rules page through it.
- `manis-presentation::SettingsPageView`: status/mode/proxy-DNS/kernel/counts/data-dir/version/
  autostart/update fields and blank-value defaults are shared by Android and desktop.
- `manis-presentation::COMMANDS`: the desktop dispatcher and `nsh.json` host capabilities are
  checked against one shared command catalog, so a missing grant fails a unit test.
- macOS packaging now bundles the NSH host as `Manis.app/Contents/MacOS/Manis` and keeps the GPUI
  binary beside it as the headless engine.
- Windows: add `packaging/windows/build-release.ps1` for a portable `Manis.exe` +
  `manis-ui.exe` zip; the managed controller transport remains unavailable there.
- `manis-nsh-host`: the shared NSH bridge (root payload, command handlers and desktop engine
  discovery) used by the desktop shells, so every platform answers the same TSX through one
  backend module.
- macOS (AppKit) and Windows (Win32) NSH shells: both run the same bundle and shared host bridge;
  Linux keeps the GTK shell. The shared bridge resolves the controller path through the same brand
  data directory as the engine, so the macOS shell probes the real socket. Windows cross-checks
  with the GNU target.
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
- Android: Settings → 应用更新 checks the rolling GitHub release, downloads the arm64 APK with
  progress, verifies its `.sha256`, and turns the action into 安装. The first install asks for the
  system's install-unknown-apps permission and then opens the standard package installer.
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

- Android: the Nodes page now renders through NSH (`android/nsh-ui`) inside the existing Compose
  shell, backed by the shared Rust `manis-presentation` model. The Subscriptions page follows:
  subscription/manual-node/group lists, enable/disable, delete, add, refresh and apply are host
  channel commands, while Kotlin keeps downloads and persistence. The Rules page is also NSH now:
  structured rule rows, enable/disable, add/edit/delete and apply go through the same host channel.
  Settings follows with connection/config summaries, DNS mode and custom resolvers, backup and
  restore, kernel logs, update actions and the license note. Connection is the last content tab:
  VPN start/stop, exit node, routing modes, errors, warnings and the jump to the Nodes page are NSH
  host commands. The shell is NSH too: one root owns the bottom navigation, ApplyBar and all
  five pages, so switching tabs no longer recreates an NshView. Compose is now only the invisible
  AndroidView host for polling, VPN/installer permissions and clipboard/platform effects.
  Remove the now-unused Compose page implementations and per-page NSH hosts, keeping only the
  shared platform helpers and the single root host.
- Desktop: add `manis-nsh-desktop`, the first NSH embedding host. It loads the Android TSX bundle
  in NSH's GTK 4 `AppView` and renders the same single-root UI. Its read-only bridge reuses the
  desktop storage loader for saved subscriptions, manual nodes, policy groups and routing mode;
  live connection state from the desktop controller socket is now included
  (connection status, exit node, proxy/rule counts and routing mode). The Nodes page now shows
  live subscription/manual sources and policy groups, and selecting a node or routing mode goes
  through the desktop controller. The Rules page now shows the live runtime rules (with the FINAL
  row), and the Settings page can read the desktop kernel log. Subscription and manual-node
  enable/disable and deletion now write through the desktop store (verified disable → restore).
  New subscription URLs and manual share links can also be added and removed (verified add →
  delete). Apply/refresh now syncs single-node provider files, recompiles the saved profile and
  reloads the live controller through `/configs`, restoring the previous YAML on failure.
  Manual rules can now be added, edited, toggled and deleted from the Rules page; each write is
  followed by the same profile apply. Runtime rules remain read-only. Connection stop and the final
  `manis-ui` replacement are the next step. The Connection page can now toggle the live TUN
  interface (verified off → on), and connection.reload now reapplies the saved profile. CI installs
  GTK 4 and links the NSH desktop host on Linux.
- Build: add `android/build-nsh-ui.sh` and `android/test-nsh-ui.sh`, pin the NSH CLI to 0.3.0 in
  Android CI, and fail when the committed NSH assets differ from a fresh build. All five NSH UI
  tests run in CI.
- Desktop launch: `manis-ui --headless` runs the managed engine without a window. The NSH host
  starts it when no controller is running, waits for the Unix controller, and stops the owned
  engine when its window closes. With an existing controller it attaches instead.
- Packaging: the Arch/AUR `manis` entry now installs the NSH host as `manis` and the GPUI binary
  as `manis-ui` (the headless engine), with the `gtk4` dependency.
- Desktop shell: add a StatusNotifierItem tray with open/quit, and hide the window on close while
  the tray is available. The tray registers as `dev.manis.app`.
- Settings: expose the desktop login-startup preference and toggle it from the NSH Settings page;
  verified enable/disable writes/removes the XDG autostart entry. Desktop update actions open the
  GitHub Release page; install points at the manual release package (matching the GPUI flow). Subscription/manual source grouping,
  the internal `__MANIS_GLOBAL__` label, current selection and delays are built once instead of
  being reconstructed by the Kotlin UI. The NSH Android runtime is pinned to `v0.3.0`, fetched by
  `android/fetch-nsh.sh`, and raises `minSdk` to 28.
- Settings opens inside the main window again: the sidebar item and the settings shortcut no
  longer create a separate window.
- Android: refresh subscriptions without connecting the VPN. The app downloads each enabled
  subscription into the core's provider cache, so a first-time user can validate a subscription and
  see its node count before connecting; a running core still refreshes live.
- Android: replace the raw QX rule text box with structured rule rows matching the desktop editor
  (type, value, target, enable/disable, edit/delete). The rule/global/direct mode selector now lives
  only on the connection page.

- Upgrade every direct dependency to its newest release, including GPUI `0.3.10`, GPUI Component
  `0.8.1`, `zip` 9.1.3, `zbus` 6.2, and `ureq` 3.5. The core archive reader accepts `zip` 9's
  fallible entry names, and the Linux tray integration test drives `zbus` 6 from its process-wide
  multithreaded runtime now that the blocking API is gone. The Android bridge moves to `jni` 0.22's
  `EnvUnowned` native-method entry point and reports every failure as `IllegalStateException`
  through one error policy. Every GitHub Actions pin moves to its current release (`checkout` 7,
  `cache` 6, `setup-java` 6, `setup-go` 7, `setup-android` 4, artifact upload/download 7/8,
  `install-action`, `cargo-deny-action`, and `audit-check`), so Dependabot no longer has open
  update pull requests.

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

- Android: label the internal `__MANIS_GLOBAL__` selector as “全局” and split the connected node
  list back into subscription and manual source cards. Provider nodes are read from
  `/providers/proxies` for their last delay, and hidden kernel groups are no longer listed.
- Android: the Nodes page now lists saved subscriptions, cached proxy names, and manual nodes while
  disconnected, with a clear hint that connecting enables switching and latency tests. It previously
  showed only a short “尚未连接” card, which made a configured account look empty.
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
