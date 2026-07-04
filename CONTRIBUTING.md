# Contributing to sweph-rs

Thanks for your interest! Bug reports, chart-accuracy discrepancies, and PRs
are all welcome.

## Development setup

```bash
git clone https://github.com/invents-us/sweph-rs
cd sweph-rs
cargo test          # no ephemeris data files needed (Moshier fallback)
```

Before opening a PR, make sure the same gates CI runs are green locally:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Ground rules

### Vendored C sources are never hand-edited

`sweph-sys/vendor/` must stay **byte-identical** to the upstream Swiss
Ephemeris release pinned in `sweph-sys/vendor/UPSTREAM_COMMIT`. Behavior
changes belong in the Rust layer (or upstream at
[aloistr/swisseph](https://github.com/aloistr/swisseph)), never as patches to
`vendor/`. A PR that updates the vendored version must:

1. Copy the files from a fresh checkout of the new upstream tag,
2. update `UPSTREAM_COMMIT` to the new commit SHA,
3. state the upstream tag in the PR description, and
4. verify byte-identity (`cmp` each file against the upstream checkout).

### Unsafe code policy

- Raw FFI lives in `sweph-sys` only. The `sweph` crate contains the minimal
  `unsafe` blocks needed to call it, each behind the process-wide mutex.
- Every `unsafe` block needs a `// SAFETY:` comment explaining why it is sound.
- A new `extern "C"` binding must be checked against the prototype in
  `sweph-sys/vendor/swephexp.h`, including output buffer sizes (`serr` is 256
  bytes; house cusp buffers are 13 doubles for 12-house systems but **37 for
  Gauquelin** — see the comment on `HouseSystem::to_swe`).
- The C library is not thread-safe: any code path (including tests) that calls
  into it must hold the serialization lock.

### Tests

- Tests run against the real Swiss Ephemeris, not mocks, and must pass without
  ephemeris data files (the Moshier fallback covers the planets).
- When asserting positions, use values verifiable against an independent
  source (e.g. astro.com) and comment the expectation.

## License of contributions

This project is licensed **AGPL-3.0-only** (see [README](README.md#license)
for the upstream Swiss Ephemeris dual-licensing context). By submitting a
contribution you agree that it is your own work and that you license it under
the same terms (inbound = outbound).

## Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
