# sweph-rs

Rust bindings for the [Swiss Ephemeris](https://www.astro.com/swisseph/), the
high-precision astronomical library by Astrodienst used by most professional
astrology software. Same JPL-derived data NASA uses; sub-arcsecond precision
over roughly 13,200 BCE to 17,191 CE (the JPL DE431 span).

Two crates, following the standard Rust FFI split:

| Crate | What it is |
|-------|------------|
| [`sweph`](sweph/) | Safe, thread-safe Rust API — planets, houses, eclipses, sidereal zodiac. Start here. |
| [`sweph-sys`](sweph-sys/) | Raw FFI bindings; vendors and compiles the C sources (v2.10.03). No runtime dependencies. |

The name follows the convention of the other language bindings —
[pyswisseph](https://github.com/astrorigin/pyswisseph) (Python),
[sweph](https://github.com/timotejroiko/sweph) (Node.js),
[swego](https://github.com/astrotools/swego) (Go),
[SwissEphNet](https://github.com/ygrenier/SwissEphNet) (.NET) — and upstream's
own naming (`sweph.c`, "the SWEPH package").

## Quick start

```toml
[dependencies]
sweph = "0.2"
```

```rust
use sweph::{Body, HouseSystem};

let jd = sweph::julian_day(1990, 6, 21, 12.0);

let sun = sweph::calc(jd, Body::Sun)?;
println!("Sun at {:.2}° {}", sun.sign_degree(), sun.sign()); // Sun at 29.86° Gemini

let houses = sweph::houses(jd, 40.7128, -74.0060, HouseSystem::Placidus)?;
println!("Ascendant {:.2}°", houses.ascendant);

let eclipse = sweph::next_solar_eclipse(jd)?;
println!("next solar eclipse: {:?} at JD {}", eclipse.kind, eclipse.maximum_jd);
```

Works out of the box with **no data files**: when the Swiss Ephemeris `*.se1`
files are absent, the library transparently falls back to the built-in Moshier
analytical ephemeris (~0.1″ precision for the planets — more than enough for
astrology); `Position::ephemeris` tells you which source was actually used.
For full precision, Chiron, or asteroids, download the
[data files](https://github.com/aloistr/swisseph/tree/master/ephe) and call
`sweph::set_ephe_path("/path/to/ephe")`.

## Features

- **Bodies**: Sun through Pluto, lunar nodes (mean/true), lunar apogee
  (Lilith, mean/osculating), Earth, Chiron, Pholus, Ceres, Pallas, Juno, Vesta
- **Houses**: Placidus, Koch, Porphyry, Regiomontanus, Campanus, Equal,
  Whole Sign, Alcabitius, Morinus, Topocentric, Vehlow — with graceful errors
  for quadrant systems at polar latitudes
- **Eclipses**: global solar and lunar eclipse search with type classification
  (total / annular / hybrid / partial / penumbral)
- **Sidereal**: Lahiri, Fagan-Bradley, Raman, Krishnamurti, De Luce ayanamshas
- **Time**: Julian day conversions both ways, UTC → JD with leap-second
  handling, delta-T
- **Coordinate options**: equatorial, heliocentric, barycentric, topocentric,
  J2000, true/astrometric positions via `Flags`

## Thread safety

The Swiss Ephemeris C library is **not** thread-safe — it keeps global state
(ephemeris path, file handles, caches). `sweph-sys` compiles it with
thread-local storage disabled (`TLSOFF`) so there is a single process-wide
state, and the `sweph` crate serializes every FFI call behind a mutex. The
safe API can therefore be called freely from any thread; configuration calls
(`set_ephe_path`, `set_sidereal_mode`, `set_topocentric`) affect the whole
process. If you use `sweph-sys` directly, you own that serialization.

## Versioning

The vendored C sources are upstream release **2.10.03**
(tag `v2.10.3final`, commit pinned in `sweph-sys/vendor/UPSTREAM_COMMIT`).

## License

**AGPL-3.0-only.** The Swiss Ephemeris is © Astrodienst AG and dual-licensed:
AGPL-3.0, or the paid
[Swiss Ephemeris Professional License](https://www.astro.com/swisseph/) for
closed-source use. Because these crates vendor and compile the Swiss Ephemeris
C sources, they are distributed under the AGPL-3.0 — software that uses them
must itself be released under the AGPL-3.0 or a compatible license, unless you
hold a Professional License from Astrodienst covering your use of the
underlying library.

This project is not affiliated with or endorsed by Astrodienst AG. Authors of
the Swiss Ephemeris: Dieter Koch and Alois Treindl.
