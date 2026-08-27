# Security policy

## Supported versions

Manis is pre-release software. Security fixes are applied to the latest commit on `main`; older
commits and locally modified builds are not supported.

## Report a vulnerability

Please use GitHub's private vulnerability reporting for this repository:

<https://github.com/kaigedong/Manis/security/advisories/new>

Do not open a public issue for a suspected vulnerability. Do not include real subscription URLs,
tokens, proxy credentials, private node addresses, controller secrets, or unredacted logs in any
public discussion.

Include the affected platform and commit, impact, reproduction steps using synthetic data, and any
suggested mitigation. You should receive an acknowledgement within seven days. A disclosure
timeline will be coordinated after the issue is reproduced and scoped.

## Security boundaries

- Subscription and node credentials are stored in platform user-data directories, not the Git
  repository. On macOS and Linux, Manis requires restrictive directory and file permissions.
- Controller TCP connections are restricted to loopback addresses. Unix socket connections are
  checked for type and symbolic links before use.
- Manis only manages child processes it starts. It must not stop or rewrite another proxy client's
  process or configuration.
- The macOS privileged helper accepts a fixed operation set and validates paths inside the Manis
  user-data boundary. Development-only insecure helper builds must never be distributed.
- Plain HTTP subscription URLs expose credentials and content to the network. Prefer HTTPS; use HTTP
  only on a network you explicitly trust.

These controls reduce risk but do not make the current pre-release build suitable for unattended or
high-assurance environments.

## Known dependency exceptions

The Linux UI and tray stack currently requires the unmaintained GTK 3 Rust bindings and `glib`
0.18. RustSec reports `RUSTSEC-2024-0429` for an unsound iterator implementation in that `glib`
release. The affected dependency cannot be replaced independently of the GTK/tray stack, so the
exception is recorded in `deny.toml`, Linux remains experimental, and the dependency must be
re-evaluated before a Linux binary release. A local `cargo audit` still reports the advisory; the
required CI job ignores only the same two documented IDs so that unrelated advisories remain fatal.
