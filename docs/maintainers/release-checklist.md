# Release checklist

Manis is not ready for binary distribution until every blocking item below is complete. This
checklist applies to release maintainers; normal contributors do not need signing credentials or
private core binaries.

## Source readiness

- [ ] The release commit is on `main`, tagged, and built from a clean checkout.
- [ ] CI passes on macOS, Windows, and Linux.
- [ ] `cargo fmt`, `cargo check`, `cargo test`, and strict Clippy pass with `--locked`.
- [ ] `cargo audit` and `cargo deny check` pass or have a documented, reviewed exception.
- [ ] Every exception in `deny.toml` is still necessary, scoped, and reflected in `SECURITY.md`.
- [ ] The changelog describes user-visible changes and migration concerns.
- [ ] English and Chinese documentation describe the same supported behavior.
- [ ] A repository and Git-history credential scan contains no real secrets or personal paths.
- [ ] Raw commit authors contain no local-only identity or email; `.mailmap` changes presentation but
  does not erase commit metadata.

## License gate

- [ ] The exact Rust dependency license report is archived with the release.
- [ ] The GPUI/Zed `zlog`/`ztracing`/`ztracing_macro` GPL dependency path has received an explicit
  compatibility decision for the final linked artifact.
- [ ] The bundled Mihomo binary has an exact version, upstream source location,
  license text, required notices, and corresponding-source fulfillment plan.
- [ ] `THIRD_PARTY_NOTICES.md` matches the binaries actually shipped.

If any license item is unresolved, publish source only and do not attach application binaries.

The Package workflow produces short-lived, unsigned verification artifacts. Its successful output
does not satisfy this license gate and must not be promoted to a public release without completing
the remaining checklist.

## macOS package

- [ ] The app, helper controller, privileged helper, and local installer use the intended bundle
  identifiers and signing mode.
- [ ] GitHub ad-hoc builds install protocol v8 through administrator approval, pin exact
  `Manis.app`, `manis-helperctl`, and privileged-helper cdhash requirements plus the invoking UID,
  and require reapproval after an app version change.
- [ ] Developer ID or Apple Development builds, when used, register through `SMAppService` and pin
  the Apple anchor and Team ID for the XPC client and parent requirements.
- [ ] `MANIS_ALLOW_INSECURE_LOCAL_HELPER` is absent from the release environment and bundle.
- [ ] The bundled Mihomo checksum and provenance are recorded before packaging.
- [ ] Developer ID release candidates, when used, are signed, notarized, stapled, and tested on a
  clean macOS account.
- [ ] Ad-hoc GitHub release notes state that the archive is not notarized, Gatekeeper still warns
  on first open, and users should install only the trusted official release after checksum
  verification.
- [ ] System proxy and TUN are each enabled, disabled, and restored after an abnormal exit.

## Arch Linux package

- [ ] The package is built by `makepkg` in the pinned Arch Linux container.
- [ ] `pacman -Qip` and `pacman -Qlp` show the intended metadata and file layout.
- [ ] The application opens in a native Wayland session and its tray appears in a compatible shell.
- [ ] The managed Mihomo seed and first-run install path are tested; external Mihomo locations are
      ignored.
- [ ] System proxy and TUN limitations are stated in the release notes.

## Runtime verification

- [ ] Rule, global, and direct modes are tested with synthetic configuration.
- [ ] Manual and automatic policy groups select the expected node.
- [ ] DNS and IPv4/IPv6 routing are checked with the competing proxy applications stopped.
- [ ] Credential-bearing values are absent from the app log, privileged-core log, crash output, and
  screenshots.
- [ ] Upgrade and uninstall preserve or restore the user's network state.

## Publication

- [ ] The tag workflow's draft release contains both macOS architectures, the Arch Linux package,
      and a valid checksum for each asset.
- [ ] Release notes state platform support and known limitations without claiming unverified
  behavior.
- [ ] Checksums are published for every artifact.
- [ ] GitHub security reporting is enabled.
- [ ] The tag and artifacts are immutable after publication; corrections use a new release.
