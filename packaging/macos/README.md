# Manis macOS privileged helper

Manis starts Mihomo as the logged-in user for normal HTTP/SOCKS proxy mode. macOS TUN requires
route and interface privileges, so the packaged app includes a fixed-purpose LaunchDaemon helper.

The Rust app invokes `Contents/MacOS/manis-helperctl` with this stable contract:

```text
manis-helperctl status
  stdout: running <pid> v3 | stopped v3 <last-exit-reason>

manis-helperctl start --data-dir PATH --config PATH --controller PATH
  stdout: started <pid>

manis-helperctl stop
  stdout: stopped

manis-helperctl register
  stdout: registered

manis-helperctl reinstall
  stdout: registered
  used automatically when Manis detects an outdated registered helper
```

The helper never accepts a Mihomo binary path or arbitrary arguments from the UI. It derives Mihomo
from `Contents/Resources/mihomo/mihomo` in the same app bundle, and the embedded LaunchDaemon plist
fixes the required code-signing requirement for `manis-helperctl`. Runtime paths are accepted only
for the Manis user data boundary:

```text
/Users/<user>/Library/Application Support/Manis/mihomo
```

The app keeps its redacted, correlated operation log at
`~/Library/Application Support/Manis/logs/manis-events.log`. Privileged Mihomo startup and stderr
output is capped and written to
`~/Library/Application Support/Manis/mihomo/manis-privileged-core.log`; subscription URLs and
tokens are never added by Manis's event logger.

Build a local bundle:

```bash
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
packaging/macos/build-app.sh
```

For a helper that can be approved and registered on production macOS, provide a Developer ID or
Apple Development signing identity:

```bash
MANIS_CODESIGN_IDENTITY="Developer ID Application: Example (TEAMID)" \
MANIS_CLIENT_REQUIREMENT='identifier "dev.manis.app.helperctl" and anchor apple generic and certificate leaf[subject.OU] = "TEAMID"' \
MANIS_PARENT_REQUIREMENT='identifier "dev.manis.app" and anchor apple generic and certificate leaf[subject.OU] = "TEAMID"' \
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
packaging/macos/build-app.sh
```

Without a real signing identity, the script still compiles the app and helper for verification, but
`manis-helperctl register` is expected to fail when macOS enforces privileged helper approval.
The identifier-only development requirements are also rejected at runtime unless an isolated local
build explicitly sets `MANIS_ALLOW_INSECURE_LOCAL_HELPER=1`; never distribute such a build.

An explicitly insecure local build uses a separate development-only path instead of pretending its
ad-hoc signature can register through `SMAppService`. On the first TUN request, macOS asks for
administrator approval and the fixed-purpose `manis-local-helper-install` executable installs a
root-owned LaunchDaemon, helper, and bundled Mihomo at fixed `/Library` paths. The installer does
not accept program paths or proxy arguments. Production-signed builds never enter this fallback.

Build the local TUN-test bundle with:

```bash
MANIS_ALLOW_INSECURE_LOCAL_HELPER=1 \
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
packaging/macos/build-app.sh
```

Because this mode deliberately permits an identifier-only local XPC client requirement, it is only
for an isolated development machine and must never be published or distributed.
