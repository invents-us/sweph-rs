# Changelog

## 0.2.0 — 2026-07-04

Fixes from a multi-agent review of the initial release, verified against the
vendored C source. Breaking changes are marked **[breaking]**.

### Correctness

- `Position` gains an `ephemeris: Ephemeris` field reporting the source
  actually used — the C library silently substitutes Moshier when Swiss data
  files are missing, and that substitution is now visible. **[breaking]**
  (struct literal / exhaustive-match construction)
- Speeds are now always computed: `calc_with` forces `SEFLG_SPEED`, so
  `Position::retrograde()` can no longer silently return `false` because a
  hand-built flag set omitted `Flags::SPEED`.
- `Flags::SWISS | Flags::MOSHIER` is rejected with an error instead of the C
  library silently preferring Moshier.
- `next_solar_eclipse` / `next_lunar_eclipse` return `Result<Eclipse>` instead
  of `Result<Option<Eclipse>>` — the `None` case was unreachable (the C search
  never reports "no eclipse"; verified against `swecl.c`) and the old doc
  promise could lure callers into infinite loops. **[breaking]**
- `julian_day` takes `month: i32, day: i32` and documents the C library's
  arithmetic date extension; the old `u32` parameters wrapped silently at the
  FFI boundary. **[breaking]**
- `ayanamsha(jd, mode)` now takes the mode explicitly and applies it under a
  single lock acquisition, so the returned value is always for the requested
  ayanamsha. **[breaking]**
- Documented that the houses path honors only `SIDEREAL`/`NO_NUTATION`
  (everything else, e.g. `EQUATORIAL`, is silently ignored by the C library),
  that `Flags::SIDEREAL` without `set_sidereal_mode` defaults to
  Fagan/Bradley, and that configure-then-compute sequences are not atomic
  across threads.
- README: corrected the ephemeris coverage to ~13,200 BCE – 17,191 CE (was
  understated as 10,800 BCE – 16,800 CE).

### Added

- `Ephemeris` enum, `Flags::NONE`, `next_solar_eclipse_with` /
  `next_lunar_eclipse_with` (explicit ephemeris source),
  `SEFLG_EPHMASK` in `sweph-sys`.

### Internal

- Factored the serr-buffer/lock/error-check FFI orchestration into one helper.
- Houses go through a single `swe_houses_ex` path (equivalent to `swe_houses`
  for this crate; the delta-T caveat is documented at the call site).
- Guard comments + regression test for the `rem_euclid(360.0)` boundary
  rounding that makes the sign-math modulo chain load-bearing.
- `build.rs` no longer forces `-O3` in debug builds; CI installs `cargo-audit`
  as a prebuilt binary.

## 0.1.0 — 2026-07-04

Initial release: `sweph-sys` (vendored Swiss Ephemeris v2.10.03, raw FFI) and
`sweph` (safe, mutex-serialized API: planets/nodes/apogees/asteroids, 11 house
systems, global eclipse search, sidereal modes, UTC/JD time handling).
