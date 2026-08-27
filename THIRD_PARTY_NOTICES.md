# Third-party notices

Manis source code is offered under the Apache License 2.0. A built application also contains or
interacts with third-party components governed by their own licenses. This file highlights the
components that require explicit release attention; it is not a substitute for the license report
generated from the exact dependency lockfile of a release.

## GPUI and Zed crates

Manis pins `gpui` and `gpui_platform` to Zed commit
`5631830c564afa89b3aba679f45d9c3345f9460f`.

- Upstream: <https://github.com/zed-industries/zed>
- GPUI license: Apache-2.0
- Important transitive licenses: the pinned dependency graph includes `zlog`, `ztracing`, and
  `ztracing_macro`, which declare GPL-3.0-or-later.

Maintainers must review the complete linked dependency graph before distributing Manis binaries.
The Apache-2.0 license of Manis source does not override the licenses of linked dependencies.

## Mihomo

Manis can supervise a separately built Mihomo executable. The repository does not contain a Mihomo
binary.

- Upstream: <https://github.com/MetaCubeX/mihomo>
- License: GPL-3.0

The macOS packaging script can copy a maintainer-supplied Mihomo executable into an app bundle.
Anyone distributing such a bundle is responsible for including Mihomo's license, corresponding
source offer or source location, copyright notices, and any other material required by the exact
Mihomo build.

## sing-box

Manis can supervise a separately installed sing-box executable. The repository does not contain a
sing-box binary.

- Upstream: <https://github.com/SagerNet/sing-box>
- License: GPL-3.0-or-later, including the additional term in its upstream license notice

Manis currently discovers sing-box from the user's system; release packaging must not begin
bundling it without a separate compliance review.

## Release policy

Before publishing any binary release:

1. run `cargo deny check` and `cargo audit` against the committed `Cargo.lock`;
2. generate a complete machine-readable license inventory for all Rust dependencies;
3. review the GPUI/Zed GPL dependency path and choose release terms compatible with the final
   linked artifact;
4. include the exact license texts and notices for every bundled executable; and
5. record the reviewed component versions in the release notes.

No statement in this file is legal advice.
