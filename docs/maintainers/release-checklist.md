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
- [ ] Every bundled Mihomo or sing-box binary has an exact version, upstream source location,
  license text, required notices, and corresponding-source fulfillment plan.
- [ ] `THIRD_PARTY_NOTICES.md` matches the binaries actually shipped.

If any license item is unresolved, publish source only and do not attach application binaries.

## macOS package

- [ ] The app, helper controller, privileged helper, and local installer use the intended bundle
  identifiers and production signing identity.
- [ ] The XPC client and parent requirements pin the Apple anchor and Team ID.
- [ ] `MANIS_ALLOW_INSECURE_LOCAL_HELPER` is absent from the release environment and bundle.
- [ ] The bundled Mihomo checksum and provenance are recorded before packaging.
- [ ] The app is signed, notarized, stapled, and tested on a clean macOS account.
- [ ] System proxy and TUN are each enabled, disabled, and restored after an abnormal exit.

## Runtime verification

- [ ] Rule, global, and direct modes are tested with synthetic configuration.
- [ ] Manual and automatic policy groups select the expected node.
- [ ] DNS and IPv4/IPv6 routing are checked with the competing proxy applications stopped.
- [ ] Credential-bearing values are absent from the app log, privileged-core log, crash output, and
  screenshots.
- [ ] Upgrade and uninstall preserve or restore the user's network state.

## Publication

- [ ] Release notes state platform support and known limitations without claiming unverified
  behavior.
- [ ] Checksums are published for every artifact.
- [ ] GitHub security reporting is enabled.
- [ ] The tag and artifacts are immutable after publication; corrections use a new release.
