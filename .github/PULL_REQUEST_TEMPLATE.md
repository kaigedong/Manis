## Summary

Describe the user-visible behavior and why the change is needed.

## Validation

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo check --workspace --all-targets --locked`
- [ ] `cargo test --workspace --all-targets --locked`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] Documentation and screenshots are updated when behavior or UI changed
- [ ] No real subscription URL, token, node credential, or private log is included

## Security and compatibility

Describe changes to persisted data, network access, subprocesses, system proxy/TUN behavior,
privileged code, or third-party dependencies. Write “None” if no boundary changed.
