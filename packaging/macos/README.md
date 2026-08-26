# Relay macOS privileged helper

Relay starts Mihomo as the logged-in user for normal HTTP/SOCKS proxy mode. macOS TUN requires
route and interface privileges, so the packaged app includes a fixed-purpose LaunchDaemon helper.

The Rust app invokes `Contents/MacOS/relay-helperctl` with this stable contract:

```text
relay-helperctl status
  stdout: running <pid> v2 | stopped v2

relay-helperctl start --data-dir PATH --config PATH --controller PATH
  stdout: started <pid>

relay-helperctl stop
  stdout: stopped

relay-helperctl register
  stdout: registered

relay-helperctl reinstall
  stdout: registered
  used automatically when Relay detects an outdated registered helper
```

The helper never accepts a Mihomo binary path or arbitrary arguments from the UI. It derives Mihomo
from `Contents/Resources/mihomo/mihomo` in the same app bundle, and the embedded LaunchDaemon plist
fixes the required code-signing requirement for `relay-helperctl`. Runtime paths are accepted only
for the Relay user data boundary:

```text
/Users/<user>/Library/Application Support/Relay/mihomo
```

Build a local bundle:

```bash
RELAY_MIHOMO_BINARY=/absolute/path/to/mihomo \
packaging/macos/build-app.sh
```

For a helper that can be approved and registered on production macOS, provide a Developer ID or
Apple Development signing identity:

```bash
RELAY_CODESIGN_IDENTITY="Developer ID Application: Example (TEAMID)" \
RELAY_CLIENT_REQUIREMENT='identifier "dev.relay.prototype.helperctl" and anchor apple generic and certificate leaf[subject.OU] = "TEAMID"' \
RELAY_PARENT_REQUIREMENT='identifier "dev.relay.prototype" and anchor apple generic and certificate leaf[subject.OU] = "TEAMID"' \
RELAY_MIHOMO_BINARY=/absolute/path/to/mihomo \
packaging/macos/build-app.sh
```

Without a real signing identity, the script still compiles the app and helper for verification, but
`relay-helperctl register` is expected to fail when macOS enforces privileged helper approval.
The identifier-only development requirements are also rejected at runtime unless an isolated local
build explicitly sets `RELAY_ALLOW_INSECURE_LOCAL_HELPER=1`; never distribute such a build.
