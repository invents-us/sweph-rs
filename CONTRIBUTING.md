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

### All changes land via pull request

`main` is protected with `enforce_admins` — nobody pushes to it directly,
maintainers included. Branch, open a PR, and merge when CI is green and
conversations are resolved. (There is no required-review count: this is a
solo-maintained project and GitHub does not let authors approve their own
PRs; CI is the merge gate.)

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
contribution you certify and agree that:

1. **You have the right to submit it** — the contribution is your own
   original work, or you otherwise have sufficient rights to submit it under
   these terms.
2. **Inbound = outbound** — your contribution is licensed to the project and
   to everyone else under AGPL-3.0-only.
3. **Additional grant to the maintainer** — you grant invents.us (the project
   maintainer) a perpetual, worldwide, non-exclusive, royalty-free,
   irrevocable license to use, reproduce, modify, distribute, and sublicense
   your contribution under license terms of its choosing, including in
   proprietary products.

Point 3 exists for transparency's sake: the maintainer also uses these crates
in its own commercial services, where the underlying C library is covered by
the paid Swiss Ephemeris Professional License rather than the AGPL. Without
the grant, external contributions would be AGPL-only and that use would have
to stop. Everyone else receives every line of this project under the AGPL,
exactly as before — the grant gives the maintainer no rights over *your*
other code, only over what you contribute here.

If you're not comfortable with the grant, please open an issue describing
the change instead of a PR — maintainer-authored implementations of publicly
suggested ideas carry no grant question.

Maintainers: **do not merge external PRs from authors who have not agreed to
these terms** (agreement is the checked box in the PR template).

## Conduct

This project follows the [Contributor Covenant](CODE_OF_CONDUCT.md).
