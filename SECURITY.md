# Security Policy

## Supported versions

Only the latest published 0.x release of `sweph` and `sweph-sys` receives
security fixes.

## Reporting a vulnerability

Please report vulnerabilities privately via GitHub:
**Security → Report a vulnerability** on this repository
(<https://github.com/invents-us/sweph-rs/security/advisories/new>).
Do not open a public issue for anything you believe is exploitable.

You should receive an acknowledgement within a few days. Please include a
minimal reproduction if you can.

If the issue is in the Swiss Ephemeris C library itself rather than in the
Rust bindings, we will coordinate with upstream (Astrodienst,
<https://github.com/aloistr/swisseph>) and may ask you to report it there as
well.

## Threat model and scope

Things callers should know when embedding these crates:

- **Ephemeris data files (`*.se1`) are trusted input.** The C library's file
  parser is not hardened against adversarial files. Only load data files
  obtained from Astrodienst (or the official GitHub mirror), and do not point
  `set_ephe_path` at directories writable by untrusted parties. The C library
  also honors the `SE_EPHE_PATH` environment variable, which takes priority
  over the path set through the API — relevant if untrusted code can influence
  your process environment.
- **Numeric inputs are memory-safe but not validated.** Out-of-range dates,
  coordinates, or Julian day numbers produce astronomically meaningless
  results or `Err` returns, not memory unsafety.
- **Thread safety is provided by the `sweph` crate only.** The C library is
  compiled with a single shared global state (`TLSOFF`); `sweph` serializes
  every call behind a process-wide mutex. If you call `sweph-sys` directly you
  are responsible for equivalent serialization — concurrent unsynchronized
  calls are a data race (undefined behavior).
- **Vendored C provenance.** `sweph-sys/vendor/` is byte-identical to the
  pinned upstream release recorded in `sweph-sys/vendor/UPSTREAM_COMMIT`.
  CI-independent verification: `git clone` upstream at that commit and `cmp`
  the files.
