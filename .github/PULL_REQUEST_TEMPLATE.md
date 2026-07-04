## What

## Checklist

- [ ] `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo test --workspace` pass locally
- [ ] New `unsafe` blocks have `// SAFETY:` comments; new FFI bindings were checked against `swephexp.h` (see CONTRIBUTING.md)
- [ ] `sweph-sys/vendor/` untouched — or this PR is an upstream version bump following the CONTRIBUTING.md procedure
- [ ] I agree to the [contribution terms](../CONTRIBUTING.md#license-of-contributions) (AGPL-3.0 inbound = outbound, plus the maintainer license grant)
