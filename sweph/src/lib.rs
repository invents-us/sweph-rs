//! Safe, thread-safe Rust API for the [Swiss Ephemeris](https://www.astro.com/swisseph/).
//!
//! # Quick start
//!
//! ```
//! use sweph::{Body, HouseSystem};
//!
//! // Optional: point at a directory of Swiss Ephemeris data files (*.se1).
//! // Without data files the library transparently falls back to the built-in
//! // Moshier analytical ephemeris (no files needed, ~0.1 arcsec precision).
//! // sweph::set_ephe_path("/usr/share/sweph/ephe");
//!
//! let jd = sweph::julian_day(1990, 6, 21, 12.0);
//! let sun = sweph::calc(jd, Body::Sun).unwrap();
//! println!("Sun at {:.2}° {}", sun.sign_degree(), sun.sign());
//!
//! let houses = sweph::houses(jd, 40.7128, -74.0060, HouseSystem::Placidus).unwrap();
//! println!("Ascendant {:.2}°", houses.ascendant);
//! ```
//!
//! # Thread safety and global configuration
//!
//! The Swiss Ephemeris C library keeps global state (the ephemeris path, open
//! file handles, nutation caches). This crate compiles it with thread-local
//! storage disabled and serializes every FFI call behind a process-wide mutex,
//! so the API here is safe to call from any thread. The trade-off is that
//! calls are serialized — the ephemeris is CPU-cheap, so this is rarely a
//! bottleneck.
//!
//! The configuration functions ([`set_ephe_path`], [`set_sidereal_mode`],
//! [`set_topocentric`]) mutate **process-wide** state that later computations
//! read. Each individual call is thread-safe, but a configure-then-compute
//! sequence is not atomic: if another thread reconfigures between your two
//! calls, your computation uses its settings. Configure once at startup, or
//! externally synchronize threads that need different settings.
//!
//! # Ephemeris data files
//!
//! Positions are computed from the Swiss Ephemeris data files (`*.se1`) found
//! in the directory given to [`set_ephe_path`] (or the `SE_EPHE_PATH`
//! environment variable the C library honors). When no files are present the
//! library silently falls back to the Moshier analytical ephemeris, which
//! needs no files and is accurate to ~0.1″ for the planets (Chiron and the
//! asteroids do require data files). The [`Position::ephemeris`] field
//! reports which source was actually used, so the fallback is detectable.
//! Data files are distributed by Astrodienst at
//! <https://github.com/aloistr/swisseph/tree/master/ephe>.

use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::c_char;
use std::sync::Mutex;

use sweph_sys as sys;

// ---------------------------------------------------------------------------
// FFI serialization
// ---------------------------------------------------------------------------

/// Serializes every entry into the (non-thread-safe) C library.
static FFI_LOCK: Mutex<()> = Mutex::new(());

/// Acquire the FFI lock, tolerating poisoning: the guarded value is `()`, so
/// there is no Rust-side state a panicking holder could have left
/// inconsistent, and refusing the lock forever after one panic would turn a
/// single failure into a permanent one for long-lived processes.
fn ffi_lock() -> std::sync::MutexGuard<'static, ()> {
    FFI_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn read_serr(buf: &[c_char]) -> String {
    // SAFETY: the C API guarantees serr is a NUL-terminated string of at most
    // 256 bytes when an error is reported; the buffer is zero-initialized so
    // this holds even if the library wrote nothing.
    unsafe { CStr::from_ptr(buf.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

/// Run one C call that reports errors through a `serr` buffer: allocate the
/// buffer, hold the process-wide lock for the duration of `f`, and convert a
/// negative return code into `Err` carrying the C error message.
fn locked_serr_call(f: impl FnOnce(*mut c_char) -> i32) -> Result<i32> {
    let mut serr = [0 as c_char; sys::SE_MAX_STNAME];
    let _guard = ffi_lock();
    let ret = f(serr.as_mut_ptr());
    drop(_guard);
    if ret < 0 {
        return Err(Error::new(read_serr(&serr)));
    }
    Ok(ret)
}

// ---------------------------------------------------------------------------
// Error type
// ---------------------------------------------------------------------------

/// An error reported by the Swiss Ephemeris.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    message: String,
}

impl Error {
    fn new(message: impl Into<String>) -> Self {
        Error {
            message: message.into(),
        }
    }

    /// The error message reported by the C library (or by this crate for
    /// input validation).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "swiss ephemeris: {}", self.message)
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

// ---------------------------------------------------------------------------
// Global configuration
// ---------------------------------------------------------------------------

/// Set the directory the Swiss Ephemeris searches for `*.se1` data files.
///
/// May be called again to switch directories; affects the whole process.
/// Without a call, the C library falls back to its compiled-in default path
/// and, when no files are found there, to the Moshier analytical ephemeris.
///
/// # Panics
/// Panics if `path` contains an interior NUL byte.
pub fn set_ephe_path(path: &str) {
    let c_path = CString::new(path).expect("ephemeris path must not contain NUL");
    let _guard = ffi_lock();
    unsafe { sys::swe_set_ephe_path(c_path.as_ptr()) };
}

/// Release all resources held by the C library (open files, caches).
///
/// Subsequent calls reinitialize automatically; calling this is optional.
pub fn close() {
    let _guard = ffi_lock();
    unsafe { sys::swe_close() };
}

/// The version of the vendored Swiss Ephemeris C library (e.g. `"2.10.03"`).
pub fn version() -> String {
    let mut buf = [0 as c_char; sys::SE_MAX_STNAME];
    let _guard = ffi_lock();
    unsafe { sys::swe_version(buf.as_mut_ptr()) };
    drop(_guard);
    read_serr(&buf)
}

/// Set the observer position for topocentric calculations
/// ([`Flags::TOPOCENTRIC`]). `altitude` is meters above sea level.
///
/// Process-wide state — see the crate docs on global configuration.
pub fn set_topocentric(longitude: f64, latitude: f64, altitude: f64) {
    let _guard = ffi_lock();
    unsafe { sys::swe_set_topo(longitude, latitude, altitude) };
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// Julian day number (UT) for a Gregorian calendar date.
/// `hour` is a decimal hour, e.g. `18.5` for 18:30 UT.
///
/// Values are not validated: out-of-range `month`/`day` are extended
/// arithmetically by the underlying algorithm (e.g. day 0 is the last day of
/// the previous month), matching the C library. Use [`utc_to_julian_day`]
/// for a validating conversion.
pub fn julian_day(year: i32, month: i32, day: i32, hour: f64) -> f64 {
    let _guard = ffi_lock();
    unsafe { sys::swe_julday(year, month, day, hour, sys::SE_GREG_CAL) }
}

/// A Gregorian calendar date with decimal hour (UT), as returned by
/// [`date_from_julian_day`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
    /// Decimal hour, 0.0 ≤ hour < 24.0.
    pub hour: f64,
}

/// Convert a Julian day number (UT) back to a Gregorian calendar date.
pub fn date_from_julian_day(jd: f64) -> Date {
    let (mut y, mut m, mut d, mut h) = (0i32, 0i32, 0i32, 0f64);
    let _guard = ffi_lock();
    unsafe { sys::swe_revjul(jd, sys::SE_GREG_CAL, &mut y, &mut m, &mut d, &mut h) };
    Date {
        year: y,
        month: m as u32,
        day: d as u32,
        hour: h,
    }
}

/// Julian day numbers on both timescales, from [`utc_to_julian_day`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JulianDay {
    /// Ephemeris/Terrestrial Time — for `swe_calc`-style ET functions.
    pub et: f64,
    /// Universal Time (UT1) — what [`calc`], [`houses`], etc. expect.
    pub ut: f64,
}

/// Convert a UTC civil timestamp to Julian day, correctly handling leap
/// seconds and delta-T. Unlike [`julian_day`], the date is validated.
pub fn utc_to_julian_day(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: f64,
) -> Result<JulianDay> {
    let mut dret = [0.0f64; 2];
    locked_serr_call(|serr| unsafe {
        sys::swe_utc_to_jd(
            year,
            month as i32,
            day as i32,
            hour as i32,
            minute as i32,
            second,
            sys::SE_GREG_CAL,
            dret.as_mut_ptr(),
            serr,
        )
    })?;
    Ok(JulianDay {
        et: dret[0],
        ut: dret[1],
    })
}

/// Delta T (TT − UT1) in days at the given Julian day (UT).
pub fn delta_t(jd_ut: f64) -> f64 {
    let _guard = ffi_lock();
    unsafe { sys::swe_deltat(jd_ut) }
}

// ---------------------------------------------------------------------------
// Flags
// ---------------------------------------------------------------------------

/// Computation flags for [`calc_with`] / [`houses_with`].
///
/// Combine with `|`: `Flags::SWISS | Flags::SIDEREAL`.
/// [`Flags::default`] is [`Flags::SWISS`].
///
/// [`Flags::SWISS`] and [`Flags::MOSHIER`] are mutually exclusive ephemeris
/// *sources* — combining them is rejected with an error rather than letting
/// the C library silently prefer Moshier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags(i32);

impl Flags {
    /// No flags. With no ephemeris-source bit set, the C library defaults to
    /// the Swiss Ephemeris.
    pub const NONE: Flags = Flags(0);
    /// Use the Swiss Ephemeris data files. When the files are absent, bodies
    /// Moshier can compute fall back to Moshier (see [`Position::ephemeris`]
    /// to detect the substitution); file-only bodies (Chiron, Pholus, the
    /// asteroids) return an error instead.
    pub const SWISS: Flags = Flags(sys::SEFLG_SWIEPH);
    /// Use the built-in Moshier analytical ephemeris (no data files).
    ///
    /// Moshier has no model for Chiron, Pholus, or the asteroids — those
    /// always come from Swiss data files, so [`calc_with`] rejects this flag
    /// for them rather than let the C library silently substitute the file
    /// (while still labeling the result Moshier).
    pub const MOSHIER: Flags = Flags(sys::SEFLG_MOSEPH);
    /// Speeds are always requested by [`calc`] / [`calc_with`]; this constant
    /// is retained for interop with `sweph-sys`.
    pub const SPEED: Flags = Flags(sys::SEFLG_SPEED);
    /// Heliocentric positions.
    pub const HELIOCENTRIC: Flags = Flags(sys::SEFLG_HELCTR);
    /// Barycentric positions.
    pub const BARYCENTRIC: Flags = Flags(sys::SEFLG_BARYCTR);
    /// Equatorial coordinates (right ascension / declination) instead of
    /// ecliptic longitude / latitude. Honored by [`calc_with`] only — the
    /// houses path ignores it (see [`houses_with`]).
    pub const EQUATORIAL: Flags = Flags(sys::SEFLG_EQUATORIAL);
    /// Sidereal zodiac. **Call [`set_sidereal_mode`] first** — without it the
    /// C library silently defaults to the Fagan/Bradley ayanamsha.
    pub const SIDEREAL: Flags = Flags(sys::SEFLG_SIDEREAL);
    /// Topocentric positions; set the observer with [`set_topocentric`].
    pub const TOPOCENTRIC: Flags = Flags(sys::SEFLG_TOPOCTR);
    /// Reference the J2000 equinox instead of the equinox of date.
    /// Honored by [`calc_with`] only — the houses path ignores it.
    pub const J2000: Flags = Flags(sys::SEFLG_J2000);
    /// No nutation.
    pub const NO_NUTATION: Flags = Flags(sys::SEFLG_NONUT);
    /// True geometric positions (no light-time correction).
    pub const TRUE_POSITIONS: Flags = Flags(sys::SEFLG_TRUEPOS);
    /// Astrometric positions (no aberration or gravitational deflection).
    pub const ASTROMETRIC: Flags = Flags(sys::SEFLG_ASTROMETRIC);

    /// The raw `iflag` bits, for use with `sweph-sys` directly.
    pub fn bits(self) -> i32 {
        self.0
    }

    pub fn contains(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    /// Reject the contradictory SWISS|MOSHIER combination, which the C
    /// library would otherwise resolve by silently preferring Moshier.
    fn validate_source(self) -> Result<()> {
        if self.contains(Flags::SWISS) && self.contains(Flags::MOSHIER) {
            return Err(Error::new(
                "Flags::SWISS and Flags::MOSHIER are mutually exclusive; \
                 pick one ephemeris source",
            ));
        }
        Ok(())
    }
}

impl Default for Flags {
    fn default() -> Self {
        Flags::SWISS
    }
}

impl std::ops::BitOr for Flags {
    type Output = Flags;
    fn bitor(self, rhs: Flags) -> Flags {
        Flags(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for Flags {
    fn bitor_assign(&mut self, rhs: Flags) {
        self.0 |= rhs.0;
    }
}

/// The ephemeris source actually used for a computation — see
/// [`Position::ephemeris`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ephemeris {
    /// Swiss Ephemeris data files.
    Swiss,
    /// Built-in Moshier analytical ephemeris (used when requested, or as the
    /// silent fallback when data files are missing).
    Moshier,
    /// JPL ephemeris file (not currently requestable through this crate's
    /// [`Flags`]; present for completeness).
    Jpl,
}

impl Ephemeris {
    /// Decode the source bits from the iflag value `swe_calc_ut` returns,
    /// which reflects the ephemeris it actually used.
    fn from_iflag(iflag: i32) -> Ephemeris {
        if iflag & sys::SEFLG_MOSEPH != 0 {
            Ephemeris::Moshier
        } else if iflag & sys::SEFLG_JPLEPH != 0 {
            Ephemeris::Jpl
        } else {
            Ephemeris::Swiss
        }
    }
}

// ---------------------------------------------------------------------------
// Bodies and signs
// ---------------------------------------------------------------------------

/// A celestial body known to the Swiss Ephemeris.
///
/// Chiron, Pholus, and the asteroids require the corresponding data files;
/// the planets, nodes, and apogees work with the Moshier fallback too.
/// Requesting [`Flags::MOSHIER`] explicitly for a file-only body is rejected
/// by [`calc_with`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Body {
    Sun,
    Moon,
    Mercury,
    Venus,
    Mars,
    Jupiter,
    Saturn,
    Uranus,
    Neptune,
    Pluto,
    MeanNode,
    TrueNode,
    /// Mean lunar apogee ("Lilith" / "Black Moon").
    MeanApogee,
    /// Osculating lunar apogee.
    OsculatingApogee,
    /// Only meaningful with [`Flags::HELIOCENTRIC`] or [`Flags::BARYCENTRIC`].
    Earth,
    Chiron,
    Pholus,
    Ceres,
    Pallas,
    Juno,
    Vesta,
}

impl Body {
    /// The ten classical planets — a common natal set.
    pub const PLANETS: &'static [Body] = &[
        Body::Sun,
        Body::Moon,
        Body::Mercury,
        Body::Venus,
        Body::Mars,
        Body::Jupiter,
        Body::Saturn,
        Body::Uranus,
        Body::Neptune,
        Body::Pluto,
    ];

    // Bodies the Moshier analytical ephemeris has no model for — the C
    // library computes them from Swiss data files regardless of the
    // requested ephemeris source.
    fn requires_data_files(self) -> bool {
        matches!(
            self,
            Body::Chiron | Body::Pholus | Body::Ceres | Body::Pallas | Body::Juno | Body::Vesta
        )
    }

    fn to_swe(self) -> i32 {
        match self {
            Body::Sun => sys::SE_SUN,
            Body::Moon => sys::SE_MOON,
            Body::Mercury => sys::SE_MERCURY,
            Body::Venus => sys::SE_VENUS,
            Body::Mars => sys::SE_MARS,
            Body::Jupiter => sys::SE_JUPITER,
            Body::Saturn => sys::SE_SATURN,
            Body::Uranus => sys::SE_URANUS,
            Body::Neptune => sys::SE_NEPTUNE,
            Body::Pluto => sys::SE_PLUTO,
            Body::MeanNode => sys::SE_MEAN_NODE,
            Body::TrueNode => sys::SE_TRUE_NODE,
            Body::MeanApogee => sys::SE_MEAN_APOG,
            Body::OsculatingApogee => sys::SE_OSCU_APOG,
            Body::Earth => sys::SE_EARTH,
            Body::Chiron => sys::SE_CHIRON,
            Body::Pholus => sys::SE_PHOLUS,
            Body::Ceres => sys::SE_CERES,
            Body::Pallas => sys::SE_PALLAS,
            Body::Juno => sys::SE_JUNO,
            Body::Vesta => sys::SE_VESTA,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Body::Sun => "Sun",
            Body::Moon => "Moon",
            Body::Mercury => "Mercury",
            Body::Venus => "Venus",
            Body::Mars => "Mars",
            Body::Jupiter => "Jupiter",
            Body::Saturn => "Saturn",
            Body::Uranus => "Uranus",
            Body::Neptune => "Neptune",
            Body::Pluto => "Pluto",
            Body::MeanNode => "Mean Node",
            Body::TrueNode => "True Node",
            Body::MeanApogee => "Mean Apogee",
            Body::OsculatingApogee => "Osculating Apogee",
            Body::Earth => "Earth",
            Body::Chiron => "Chiron",
            Body::Pholus => "Pholus",
            Body::Ceres => "Ceres",
            Body::Pallas => "Pallas",
            Body::Juno => "Juno",
            Body::Vesta => "Vesta",
        }
    }
}

impl fmt::Display for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A sign of the tropical (or sidereal, per flags) zodiac.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Sign {
    Aries,
    Taurus,
    Gemini,
    Cancer,
    Leo,
    Virgo,
    Libra,
    Scorpio,
    Sagittarius,
    Capricorn,
    Aquarius,
    Pisces,
}

impl Sign {
    pub const ALL: [Sign; 12] = [
        Sign::Aries,
        Sign::Taurus,
        Sign::Gemini,
        Sign::Cancer,
        Sign::Leo,
        Sign::Virgo,
        Sign::Libra,
        Sign::Scorpio,
        Sign::Sagittarius,
        Sign::Capricorn,
        Sign::Aquarius,
        Sign::Pisces,
    ];

    /// The sign containing an ecliptic longitude in degrees.
    pub fn from_longitude(longitude: f64) -> Sign {
        // The trailing `% 12` looks redundant but is load-bearing: for tiny
        // negative inputs, `rem_euclid(360.0)` can round to exactly 360.0,
        // making the index 12 — without the modulo that is an out-of-bounds
        // panic. Do not "simplify" this expression.
        Sign::ALL[(longitude.rem_euclid(360.0) / 30.0) as usize % 12]
    }

    pub fn name(self) -> &'static str {
        match self {
            Sign::Aries => "Aries",
            Sign::Taurus => "Taurus",
            Sign::Gemini => "Gemini",
            Sign::Cancer => "Cancer",
            Sign::Leo => "Leo",
            Sign::Virgo => "Virgo",
            Sign::Libra => "Libra",
            Sign::Scorpio => "Scorpio",
            Sign::Sagittarius => "Sagittarius",
            Sign::Capricorn => "Capricorn",
            Sign::Aquarius => "Aquarius",
            Sign::Pisces => "Pisces",
        }
    }
}

impl fmt::Display for Sign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

// ---------------------------------------------------------------------------
// Body positions
// ---------------------------------------------------------------------------

/// The computed position of a body at a moment in time.
///
/// With the default (ecliptic) coordinates, `longitude`/`latitude` are
/// ecliptic degrees and `distance` is in AU. With [`Flags::EQUATORIAL`] the
/// first two fields hold right ascension and declination instead.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Position {
    pub body: Body,
    pub longitude: f64,
    pub latitude: f64,
    pub distance: f64,
    /// Degrees per day (speeds are always computed).
    pub speed_longitude: f64,
    pub speed_latitude: f64,
    pub speed_distance: f64,
    /// The ephemeris source actually used. When [`Flags::SWISS`] was
    /// requested but the data files are missing, the C library silently
    /// substitutes Moshier — this field reports the substitution.
    pub ephemeris: Ephemeris,
}

impl Position {
    /// Whether the body appears to move backward through the zodiac.
    pub fn retrograde(&self) -> bool {
        self.speed_longitude < 0.0
    }

    pub fn sign(&self) -> Sign {
        Sign::from_longitude(self.longitude)
    }

    /// Degrees into the current sign (0.0 ≤ x < 30.0).
    pub fn sign_degree(&self) -> f64 {
        // The double modulo is deliberate: `rem_euclid(30.0)` alone can round
        // to exactly 30.0 for tiny negative inputs, violating the documented
        // half-open range. `rem_euclid(360.0)` first, then `% 30.0` on the
        // now-nonnegative value, keeps the result in [0, 30).
        self.longitude.rem_euclid(360.0) % 30.0
    }
}

/// Compute a body's position at a Julian day (UT) with the default flags
/// (Swiss Ephemeris files with Moshier fallback).
pub fn calc(jd_ut: f64, body: Body) -> Result<Position> {
    calc_with(jd_ut, body, Flags::default())
}

/// Compute a body's position with explicit [`Flags`].
///
/// Speeds are always computed. Returns an error if both [`Flags::SWISS`] and
/// [`Flags::MOSHIER`] are set, or if [`Flags::MOSHIER`] is requested for a
/// body Moshier cannot compute (Chiron, Pholus, Ceres, Pallas, Juno, Vesta —
/// the C library would read the Swiss data file anyway while still labeling
/// the result Moshier).
pub fn calc_with(jd_ut: f64, body: Body, flags: Flags) -> Result<Position> {
    flags.validate_source()?;
    if flags.contains(Flags::MOSHIER) && body.requires_data_files() {
        return Err(Error::new(format!(
            "the Moshier ephemeris has no model for {body}; it is computed \
             from Swiss data files — use Flags::SWISS and set_ephe_path",
        )));
    }
    let mut xx = [0.0f64; 6];
    let iflag = flags.bits() | sys::SEFLG_SPEED;
    let ret = locked_serr_call(|serr| unsafe {
        sys::swe_calc_ut(jd_ut, body.to_swe(), iflag, xx.as_mut_ptr(), serr)
    })?;
    Ok(Position {
        body,
        longitude: xx[0],
        latitude: xx[1],
        distance: xx[2],
        speed_longitude: xx[3],
        speed_latitude: xx[4],
        speed_distance: xx[5],
        // swe_calc_ut returns the flags it actually used, which differ from
        // the request when the library falls back (e.g. missing .se1 files).
        ephemeris: Ephemeris::from_iflag(ret),
    })
}

// ---------------------------------------------------------------------------
// Houses
// ---------------------------------------------------------------------------

/// A house system for [`houses`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HouseSystem {
    Placidus,
    Koch,
    Porphyry,
    Regiomontanus,
    Campanus,
    /// Equal houses from the Ascendant.
    Equal,
    WholeSign,
    Alcabitius,
    Morinus,
    Topocentric,
    Vehlow,
}

impl HouseSystem {
    // SAFETY-RELEVANT: houses_raw passes a 13-slot cusp buffer, which is only
    // large enough for 12-house systems. The Gauquelin system ('G') makes the
    // C library write 37 cusps — adding it here without enlarging the buffer
    // in houses_raw would be a stack buffer overflow. The same applies to the
    // Sunshine systems ('I'/'i') only insofar as they must stay 12-house.
    fn to_swe(self) -> i32 {
        (match self {
            HouseSystem::Placidus => b'P',
            HouseSystem::Koch => b'K',
            HouseSystem::Porphyry => b'O',
            HouseSystem::Regiomontanus => b'R',
            HouseSystem::Campanus => b'C',
            HouseSystem::Equal => b'A',
            HouseSystem::WholeSign => b'W',
            HouseSystem::Alcabitius => b'B',
            HouseSystem::Morinus => b'M',
            HouseSystem::Topocentric => b'T',
            HouseSystem::Vehlow => b'V',
        }) as i32
    }

    pub fn name(self) -> &'static str {
        match self {
            HouseSystem::Placidus => "Placidus",
            HouseSystem::Koch => "Koch",
            HouseSystem::Porphyry => "Porphyry",
            HouseSystem::Regiomontanus => "Regiomontanus",
            HouseSystem::Campanus => "Campanus",
            HouseSystem::Equal => "Equal",
            HouseSystem::WholeSign => "Whole Sign",
            HouseSystem::Alcabitius => "Alcabitius",
            HouseSystem::Morinus => "Morinus",
            HouseSystem::Topocentric => "Topocentric",
            HouseSystem::Vehlow => "Vehlow",
        }
    }
}

impl fmt::Display for HouseSystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// House cusps and chart angles, all in ecliptic degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Houses {
    /// Cusps of houses 1–12 (`cusps[0]` is the 1st-house cusp).
    pub cusps: [f64; 12],
    pub ascendant: f64,
    pub midheaven: f64,
    /// Sidereal time expressed as ARMC (right ascension of the MC).
    pub armc: f64,
    pub vertex: f64,
}

/// Compute house cusps and angles for a Julian day (UT) and geographic
/// position (degrees; north and east positive).
///
/// Quadrant systems such as Placidus and Koch are undefined inside the polar
/// circles; the C library then falls back to Porphyry cusps and reports an
/// error, which is surfaced here as `Err`.
pub fn houses(jd_ut: f64, latitude: f64, longitude: f64, system: HouseSystem) -> Result<Houses> {
    houses_with(jd_ut, latitude, longitude, system, Flags::NONE)
}

/// [`houses`] with explicit flags.
///
/// The C houses path honors only [`Flags::SIDEREAL`] (call
/// [`set_sidereal_mode`] first) and [`Flags::NO_NUTATION`]; the ephemeris
/// source bits affect only the internal delta-T model. All other flags —
/// [`Flags::EQUATORIAL`], [`Flags::J2000`], etc. — are **silently ignored**
/// by the C library: cusps are always ecliptic longitudes.
pub fn houses_with(
    jd_ut: f64,
    latitude: f64,
    longitude: f64,
    system: HouseSystem,
    flags: Flags,
) -> Result<Houses> {
    flags.validate_source()?;
    let mut cusps = [0.0f64; 13];
    let mut ascmc = [0.0f64; 10];
    let _guard = ffi_lock();
    // swe_houses(x..) is equivalent to swe_houses_ex(x.., iflag=0) for this
    // crate: the two differ only in delta-T tidal-acceleration selection when
    // a non-DE431 JPL file has been loaded via swe_set_jplfile, which this
    // crate never binds. Single code path, one C entry point.
    let ret = unsafe {
        sys::swe_houses_ex(
            jd_ut,
            flags.bits(),
            latitude,
            longitude,
            system.to_swe(),
            cusps.as_mut_ptr(),
            ascmc.as_mut_ptr(),
        )
    };
    drop(_guard);
    if ret < 0 {
        return Err(Error::new(format!(
            "{} houses could not be computed at latitude {latitude} (polar region?)",
            system.name(),
        )));
    }
    let mut c = [0.0f64; 12];
    c.copy_from_slice(&cusps[1..13]);
    Ok(Houses {
        cusps: c,
        ascendant: ascmc[0],
        midheaven: ascmc[1],
        armc: ascmc[2],
        vertex: ascmc[3],
    })
}

// ---------------------------------------------------------------------------
// Sidereal zodiac
// ---------------------------------------------------------------------------

/// An ayanamsha (sidereal zodiac reference) for [`set_sidereal_mode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ayanamsha {
    FaganBradley,
    Lahiri,
    DeLuce,
    Raman,
    Krishnamurti,
}

impl Ayanamsha {
    fn to_swe(self) -> i32 {
        match self {
            Ayanamsha::FaganBradley => sys::SE_SIDM_FAGAN_BRADLEY,
            Ayanamsha::Lahiri => sys::SE_SIDM_LAHIRI,
            Ayanamsha::DeLuce => sys::SE_SIDM_DELUCE,
            Ayanamsha::Raman => sys::SE_SIDM_RAMAN,
            Ayanamsha::Krishnamurti => sys::SE_SIDM_KRISHNAMURTI,
        }
    }
}

/// Select the ayanamsha used by [`Flags::SIDEREAL`] computations.
///
/// **Call this before any sidereal computation** — if no mode has been set,
/// the C library silently defaults to Fagan/Bradley. Process-wide state; see
/// the crate docs on global configuration.
pub fn set_sidereal_mode(mode: Ayanamsha) {
    let _guard = ffi_lock();
    unsafe { sys::swe_set_sid_mode(mode.to_swe(), 0.0, 0.0) };
}

/// The ayanamsha value (tropical minus sidereal longitude, in degrees) at a
/// Julian day (UT) for the given mode.
///
/// Also selects `mode` as the process-wide sidereal mode (the two operations
/// happen under one lock acquisition, so the value returned is always for
/// the mode passed here).
pub fn ayanamsha(jd_ut: f64, mode: Ayanamsha) -> Result<f64> {
    let mut daya = 0.0f64;
    locked_serr_call(|serr| unsafe {
        sys::swe_set_sid_mode(mode.to_swe(), 0.0, 0.0);
        sys::swe_get_ayanamsa_ex_ut(jd_ut, sys::SEFLG_SWIEPH, &mut daya, serr)
    })?;
    Ok(daya)
}

// ---------------------------------------------------------------------------
// Eclipses
// ---------------------------------------------------------------------------

/// The type of a solar or lunar eclipse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EclipseKind {
    Total,
    Annular,
    /// Hybrid (annular-total) solar eclipse.
    Hybrid,
    Partial,
    /// Lunar only.
    Penumbral,
    Unknown,
}

impl EclipseKind {
    fn from_retflag(retflag: i32) -> Self {
        if retflag & sys::SE_ECL_TOTAL != 0 {
            EclipseKind::Total
        } else if retflag & sys::SE_ECL_ANNULAR_TOTAL != 0 {
            EclipseKind::Hybrid
        } else if retflag & sys::SE_ECL_ANNULAR != 0 {
            EclipseKind::Annular
        } else if retflag & sys::SE_ECL_PARTIAL != 0 {
            EclipseKind::Partial
        } else if retflag & sys::SE_ECL_PENUMBRAL != 0 {
            EclipseKind::Penumbral
        } else {
            EclipseKind::Unknown
        }
    }
}

/// A solar or lunar eclipse found by [`next_solar_eclipse`] /
/// [`next_lunar_eclipse`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Eclipse {
    /// Julian day (UT) of maximum eclipse.
    pub maximum_jd: f64,
    pub kind: EclipseKind,
}

/// Find the next solar eclipse anywhere on Earth at or after `jd_start` (UT).
///
/// The search always finds one — solar eclipses recur at least twice a year —
/// so there is no "none remaining" case; errors indicate an ephemeris
/// failure.
pub fn next_solar_eclipse(jd_start: f64) -> Result<Eclipse> {
    eclipse_search(jd_start, true, Flags::default())
}

/// [`next_solar_eclipse`] with an explicit ephemeris source (e.g.
/// [`Flags::MOSHIER`] for data-file-free operation). Only the source bits
/// are used; all other flags are stripped.
pub fn next_solar_eclipse_with(jd_start: f64, flags: Flags) -> Result<Eclipse> {
    eclipse_search(jd_start, true, flags)
}

/// Find the next lunar eclipse at or after `jd_start` (UT).
///
/// The search always finds one; errors indicate an ephemeris failure.
pub fn next_lunar_eclipse(jd_start: f64) -> Result<Eclipse> {
    eclipse_search(jd_start, false, Flags::default())
}

/// [`next_lunar_eclipse`] with an explicit ephemeris source.
pub fn next_lunar_eclipse_with(jd_start: f64, flags: Flags) -> Result<Eclipse> {
    eclipse_search(jd_start, false, flags)
}

fn eclipse_search(jd_start: f64, solar: bool, flags: Flags) -> Result<Eclipse> {
    flags.validate_source()?;
    let mut tret = [0.0f64; 10];
    // The eclipse functions accept only the ephemeris-source bits and reject
    // others (SEFLG_SPEED outright); strip everything else.
    let ifl = flags.bits() & sys::SEFLG_EPHMASK;
    let ret = locked_serr_call(|serr| unsafe {
        if solar {
            sys::swe_sol_eclipse_when_glob(jd_start, ifl, 0, tret.as_mut_ptr(), 0, serr)
        } else {
            sys::swe_lun_eclipse_when(jd_start, ifl, 0, tret.as_mut_ptr(), 0, serr)
        }
    })?;
    // The C search loops until it finds an eclipse or fails with ERR; it
    // never returns 0 (verified against swecl.c). Keep a defensive error
    // rather than modeling an unreachable "none found" state.
    if ret == 0 {
        return Err(Error::new("eclipse search returned no result (unexpected)"));
    }
    Ok(Eclipse {
        maximum_jd: tret[0],
        kind: EclipseKind::from_retflag(ret),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // All tests run against the Moshier fallback — no data files needed.

    #[test]
    fn version_matches_vendored_release() {
        assert_eq!(version(), "2.10.03");
    }

    #[test]
    fn julian_day_j2000_and_roundtrip() {
        let jd = julian_day(2000, 1, 1, 12.0);
        assert!((jd - 2451545.0).abs() < 1e-6);
        let d = date_from_julian_day(jd);
        assert_eq!((d.year, d.month, d.day), (2000, 1, 1));
        assert!((d.hour - 12.0).abs() < 1e-6);
    }

    #[test]
    fn sun_in_capricorn_at_j2000() {
        let sun = calc(2451545.0, Body::Sun).unwrap();
        assert_eq!(sun.sign(), Sign::Capricorn);
        assert!(
            (sun.longitude - 280.37).abs() < 0.1,
            "Sun longitude was {}",
            sun.longitude
        );
        assert!(!sun.retrograde());
        assert!(sun.speed_longitude > 0.9 && sun.speed_longitude < 1.1);
    }

    #[test]
    fn speeds_are_computed_even_without_speed_flag() {
        // Regression: speed used to be silently 0.0 (retrograde() always
        // false) when the caller built flags without SPEED.
        let sun = calc_with(2451545.0, Body::Sun, Flags::SWISS).unwrap();
        assert!(
            sun.speed_longitude > 0.9 && sun.speed_longitude < 1.1,
            "speed was {}",
            sun.speed_longitude
        );
    }

    #[test]
    fn conflicting_ephemeris_sources_are_rejected() {
        let err = calc_with(2451545.0, Body::Sun, Flags::SWISS | Flags::MOSHIER);
        assert!(err.is_err(), "SWISS|MOSHIER must be rejected");
    }

    #[test]
    fn moshier_is_rejected_for_file_only_bodies() {
        // Moshier has no asteroid model; the C library would silently read
        // the Swiss data file and still return SEFLG_MOSEPH in the iflag,
        // making Position::ephemeris lie (issue #4). Reject up front —
        // deterministically, whether or not data files are installed.
        let jd = julian_day(1990, 6, 21, 12.0);
        for body in [
            Body::Chiron,
            Body::Pholus,
            Body::Ceres,
            Body::Pallas,
            Body::Juno,
            Body::Vesta,
        ] {
            let err = calc_with(jd, body, Flags::MOSHIER).unwrap_err();
            assert!(
                err.message().contains("Moshier"),
                "{body}: unexpected message {:?}",
                err.message()
            );
        }
        // The nodes and apogees have Moshier models and must keep working.
        for body in [
            Body::MeanNode,
            Body::TrueNode,
            Body::MeanApogee,
            Body::OsculatingApogee,
        ] {
            calc_with(jd, body, Flags::MOSHIER).unwrap();
        }
    }

    #[test]
    fn ephemeris_downgrade_is_reported() {
        // Explicit Moshier is always reported as Moshier.
        let moshier = calc_with(2451545.0, Body::Sun, Flags::MOSHIER).unwrap();
        assert_eq!(moshier.ephemeris, Ephemeris::Moshier);
        // With SWISS requested, the field reports what was actually used:
        // Moshier in this test environment (no .se1 files), Swiss if data
        // files happen to be installed.
        let requested_swiss = calc(2451545.0, Body::Sun).unwrap();
        assert!(
            matches!(
                requested_swiss.ephemeris,
                Ephemeris::Swiss | Ephemeris::Moshier
            ),
            "unexpected source: {:?}",
            requested_swiss.ephemeris
        );
    }

    #[test]
    fn all_planets_compute_without_data_files() {
        let jd = julian_day(1990, 6, 21, 12.0);
        for &body in Body::PLANETS {
            let pos = calc(jd, body).unwrap();
            assert!(
                (0.0..360.0).contains(&pos.longitude),
                "{body}: {}",
                pos.longitude
            );
        }
    }

    #[test]
    fn sign_math_survives_rem_euclid_boundary_rounding() {
        // rem_euclid(360.0) rounds to exactly 360.0 for tiny negative inputs;
        // the sign index and degree math must stay in range regardless.
        for lon in [-1e-18, -0.0, 0.0, 360.0, -360.0] {
            let sign = Sign::from_longitude(lon); // must not panic
            assert_eq!(sign, Sign::Aries, "lon {lon:e}");
        }
        let p = Position {
            body: Body::Sun,
            longitude: -1e-18,
            latitude: 0.0,
            distance: 1.0,
            speed_longitude: 1.0,
            speed_latitude: 0.0,
            speed_distance: 0.0,
            ephemeris: Ephemeris::Moshier,
        };
        assert!(
            (0.0..30.0).contains(&p.sign_degree()),
            "sign_degree out of range: {}",
            p.sign_degree()
        );
    }

    #[test]
    fn placidus_houses_new_york() {
        let jd = julian_day(1990, 6, 21, 12.0);
        let h = houses(jd, 40.7128, -74.0060, HouseSystem::Placidus).unwrap();
        assert!((0.0..360.0).contains(&h.ascendant));
        assert!((0.0..360.0).contains(&h.midheaven));
        assert!((h.cusps[0] - h.ascendant).abs() < 1e-9);
        assert!((h.cusps[9] - h.midheaven).abs() < 1e-9);
    }

    #[test]
    fn whole_sign_cusps_on_sign_boundaries() {
        let jd = julian_day(1990, 6, 21, 12.0);
        let h = houses(jd, 40.7128, -74.0060, HouseSystem::WholeSign).unwrap();
        for cusp in h.cusps {
            assert!(
                (cusp % 30.0).abs() < 1e-9,
                "whole-sign cusp not on a boundary: {cusp}"
            );
        }
    }

    #[test]
    fn placidus_fails_gracefully_at_the_pole() {
        let jd = julian_day(1990, 6, 21, 12.0);
        let err = houses(jd, 89.9, 0.0, HouseSystem::Placidus);
        assert!(err.is_err(), "Placidus at 89.9°N should error");
        // Whole-sign still works there.
        houses(jd, 89.9, 0.0, HouseSystem::WholeSign).unwrap();
    }

    #[test]
    fn total_solar_eclipse_of_april_2024() {
        let start = julian_day(2024, 3, 1, 0.0);
        let e = next_solar_eclipse(start).unwrap();
        assert_eq!(e.kind, EclipseKind::Total);
        let d = date_from_julian_day(e.maximum_jd);
        assert_eq!((d.year, d.month, d.day), (2024, 4, 8));
    }

    #[test]
    fn eclipse_search_with_explicit_moshier() {
        let start = julian_day(2024, 3, 1, 0.0);
        let e = next_lunar_eclipse_with(start, Flags::MOSHIER).unwrap();
        let d = date_from_julian_day(e.maximum_jd);
        assert_eq!((d.year, d.month, d.day), (2024, 3, 25));
    }

    #[test]
    fn lahiri_ayanamsha_near_24_degrees_at_j2000() {
        let aya = ayanamsha(2451545.0, Ayanamsha::Lahiri).unwrap();
        assert!((23.0..25.0).contains(&aya), "ayanamsha was {aya}");
    }

    #[test]
    fn utc_to_julian_day_handles_timescales() {
        let jd = utc_to_julian_day(2000, 1, 1, 12, 0, 0.0).unwrap();
        // ET runs ahead of UT by delta-T (~64s in 2000).
        assert!(jd.et > jd.ut);
        assert!((jd.ut - 2451545.0).abs() < 0.01);
    }

    // A panic while holding FFI_LOCK must not poison the API for the rest of
    // the process: ffi_lock() recovers the guard instead of cascading panics.
    #[test]
    fn ffi_lock_recovers_from_poison() {
        let poisoned = std::thread::spawn(|| {
            let _g = ffi_lock();
            panic!("intentional panic while holding FFI_LOCK (test)");
        })
        .join();
        assert!(poisoned.is_err(), "helper thread should have panicked");

        let sun =
            calc(2451545.0, Body::Sun).expect("calc must succeed even after the lock was poisoned");
        assert_eq!(sun.body, Body::Sun);
    }
}
