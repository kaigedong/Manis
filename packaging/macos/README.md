# Manis macOS privileged helper

Manis starts Mihomo as the logged-in user for normal HTTP/SOCKS proxy mode. macOS TUN requires
route and interface privileges, so the packaged app includes a fixed-purpose LaunchDaemon helper.

The Rust app invokes `Contents/MacOS/manis-helperctl` with this stable contract:

```text
manis-helperctl status
  stdout: running <pid> v7 | stopped v7 <last-exit-reason>

manis-helperctl stage-core
  stdout: staged <sha256>
  copies the fixed Manis-managed user core through authenticated XPC into root-owned storage

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

The helper never accepts a Mihomo binary path or arbitrary arguments from the UI. The signed control
tool derives the core from Manis's fixed private data path, sends its bytes and digest through
authenticated XPC, and the helper publishes a root-owned copy at a fixed path before TUN starts.
For administrator-installed builds, the root helper independently checks the bytes against the
seed SHA-256 recorded during approval, the existing root-owned core, or the verified official latest
GitHub release. It never trusts the digest supplied by the client as proof of provenance. Developer
ID builds retain controller-side provenance checks and the helper's bundle-seal check.
For Developer ID or Apple Development builds, the embedded LaunchDaemon plist fixes the required
Team ID code-signing requirement for `manis-helperctl`. For the default GitHub ad-hoc builds, the
first TUN request asks for administrator approval and installs a root-owned LaunchDaemon that pins
the exact cdhash requirements for `Manis.app`, `manis-helperctl`, and the privileged helper, plus
the invoking user ID. When any bundled app version changes, those fingerprints change too, so the
next TUN start must be approved again. Runtime paths are accepted only for the Manis user data
boundary:

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
packaging/macos/build-app.sh
```

The packaging script generates `Contents/Resources/Manis.icns` from
`assets/brand/manis-mark.svg`; the SVG remains the canonical icon source.

`MANIS_BUNDLE_VERSION` may override the three-component app version and `MANIS_BUNDLE_BUILD` may
set its positive integer build number. The Package workflow uses these inputs to create separate
Apple Silicon and Intel verification artifacts. Each successful `main` build replaces the public
`latest` pre-release, manual runs keep short-lived Actions artifacts, and pushing any version tag
creates a separate draft release. Versioned drafts are never published automatically. Rolling
builds use `0.1.<Package run number>` as the monotonic application version. The release also
contains a digest-protected update manifest; packaged applications poll it in the background,
download and validate the matching bundle, and expose **Restart and update** only after staging has
completed. Source builds and renamed/non-bundle installations do not self-update. The artifacts are
ad-hoc signed and not notarized. macOS Gatekeeper still warns on first open, and users should only
install archives downloaded from the trusted official release together with the matching checksum.
The first TUN enablement for each app version needs administrator authorization; no paid Apple
Developer Program account is required for this ad-hoc GitHub package path. The build downloads the
official stable Mihomo asset selected for the target architecture, checks the SHA-256 digest
published by GitHub Releases, validates `mihomo -v`, and stores it as the first-launch seed.
`MANIS_MIHOMO_BINARY` remains a local packaging override. A
maintainer distributing the bundle must satisfy the exact Mihomo build's GPL obligations and
include its license, notices, and corresponding-source information; see `THIRD_PARTY_NOTICES.md`
and `docs/maintainers/release-checklist.md`.

The optional Developer ID or Apple Development path remains supported for maintainers who want
`SMAppService` registration and Team ID requirements instead of administrator-pinned ad-hoc
fingerprints:

```bash
MANIS_CODESIGN_IDENTITY="Developer ID Application: Example (TEAMID)" \
MANIS_CLIENT_REQUIREMENT='identifier "dev.manis.app.helperctl" and anchor apple generic and certificate leaf[subject.OU] = "TEAMID"' \
MANIS_PARENT_REQUIREMENT='identifier "dev.manis.app" and anchor apple generic and certificate leaf[subject.OU] = "TEAMID"' \
MANIS_MIHOMO_BINARY=/absolute/path/to/mihomo \
packaging/macos/build-app.sh
```

Without `MANIS_CODESIGN_IDENTITY`, the default build does not attempt `SMAppService` registration.
Instead, the fixed-purpose `manis-local-helper-install` executable requests administrator approval,
copies the approved app snapshot into root-controlled staging, and installs the LaunchDaemon,
helper, and bundled Mihomo at fixed `/Library` paths. The installer does not accept program paths
or proxy arguments from the UI. The old `MANIS_ALLOW_INSECURE_LOCAL_HELPER` development bypass is
obsolete and rejected by the packaging script; it is not needed for GitHub builds and must not be
used for distribution.
