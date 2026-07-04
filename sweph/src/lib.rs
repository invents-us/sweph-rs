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
//! # Thread safety
//!
//! The Swiss Ephemeris C library keeps global state (the ephemeris path, open
//! file handles, nutation caches). This crate compiles it with thread-local
//! storage disabled and serializes every FFI call behind a process-wide mutex,
//! so the API here is safe to call from any thread. The trade-off is that
//! calls are serialized — the ephemeris is CPU-cheap, so this is rarely a
//! bottleneck, but heavy parallel workloads should batch per thread.
//!
//! # Ephemeris data files
//!
//! Positions are computed from the Swiss Ephemeris data files (`*.se1`) found
//! in the directory given to [`set_ephe_path`] (or the `SE_EPHE_PATH`
//! environment variable the C library honors). When no files are present the
//! library silently falls back to the Moshier analytical ephemeris, which
//! needs no files and is accurate to ~0.1″ for the planets (Chiron and the
//! asteroids do require data files). Data files are distributed by
//! Astrodienst at <https://github.com/aloistr/swisseph/tree/master/ephe>.

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
pub fn set_topocentric(longitude: f64, latitude: f64, altitude: f64) {
    let _guard = ffi_lock();
    unsafe { sys::swe_set_topo(longitude, latitude, altitude) };
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// Julian day number (UT) for a Gregorian calendar date.
/// `hour` is a decimal hour, e.g. `18.5` for 18:30 UT.
pub fn julian_day(year: i32, month: u32, day: u32, hour: f64) -> f64 {
    let _guard = ffi_lock();
    unsafe { sys::swe_julday(year, month as i32, day as i32, hour, sys::SE_GREG_CAL) }
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
/// seconds and delta-T.
pub fn utc_to_julian_day(
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: f64,
) -> Result<JulianDay> {
    let mut dret = [0.0f64; 2];
    let mut serr = [0 as c_char; sys::SE_MAX_STNAME];
    let _guard = ffi_lock();
    let ret = unsafe {
        sys::swe_utc_to_jd(
            year,
            month as i32,
            day as i32,
            hour as i32,
            minute as i32,
            second,
            sys::SE_GREG_CAL,
            dret.as_mut_ptr(),
            serr.as_mut_ptr(),
        )
    };
    drop(_guard);
    if ret < 0 {
        return Err(Error::new(read_serr(&serr)));
    }
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
/// Combine with `|`: `Flags::SWISS | Flags::SPEED | Flags::SIDEREAL`.
/// [`Flags::default`] is `SWISS | SPEED`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Flags(i32);

impl Flags {
    /// Use the Swiss Ephemeris data files (falls back to Moshier if absent).
    pub const SWISS: Flags = Flags(sys::SEFLG_SWIEPH);
    /// Use the built-in Moshier analytical ephemeris (no data files).
    pub const MOSHIER: Flags = Flags(sys::SEFLG_MOSEPH);
    /// Also compute speeds (`speed_*` fields of [`Position`]).
    pub const SPEED: Flags = Flags(sys::SEFLG_SPEED);
    /// Heliocentric positions.
    pub const HELIOCENTRIC: Flags = Flags(sys::SEFLG_HELCTR);
    /// Barycentric positions.
    pub const BARYCENTRIC: Flags = Flags(sys::SEFLG_BARYCTR);
    /// Equatorial coordinates (right ascension / declination) instead of
    /// ecliptic longitude / latitude.
    pub const EQUATORIAL: Flags = Flags(sys::SEFLG_EQUATORIAL);
    /// Sidereal zodiac; configure the ayanamsha with [`set_sidereal_mode`].
    pub const SIDEREAL: Flags = Flags(sys::SEFLG_SIDEREAL);
    /// Topocentric positions; set the observer with [`set_topocentric`].
    pub const TOPOCENTRIC: Flags = Flags(sys::SEFLG_TOPOCTR);
    /// Reference the J2000 equinox instead of the equinox of date.
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
}

impl Default for Flags {
    fn default() -> Self {
        Flags::SWISS | Flags::SPEED
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

// ---------------------------------------------------------------------------
// Bodies and signs
// ---------------------------------------------------------------------------

/// A celestial body known to the Swiss Ephemeris.
///
/// Chiron, Pholus, and the asteroids require the corresponding data files;
/// the planets, nodes, and apogees work with the Moshier fallback too.
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
    /// The ten classical planets plus the true node — a common natal set.
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
    /// Degrees per day; `0.0` unless [`Flags::SPEED`] was set.
    pub speed_longitude: f64,
    pub speed_latitude: f64,
    pub speed_distance: f64,
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
        self.longitude.rem_euclid(360.0) % 30.0
    }
}

/// Compute a body's position at a Julian day (UT) with the default flags
/// (Swiss Ephemeris files with Moshier fallback, speeds included).
pub fn calc(jd_ut: f64, body: Body) -> Result<Position> {
    calc_with(jd_ut, body, Flags::default())
}

/// Compute a body's position with explicit [`Flags`].
pub fn calc_with(jd_ut: f64, body: Body, flags: Flags) -> Result<Position> {
    let mut xx = [0.0f64; 6];
    let mut serr = [0 as c_char; sys::SE_MAX_STNAME];
    let _guard = ffi_lock();
    let ret = unsafe {
        sys::swe_calc_ut(
            jd_ut,
            body.to_swe(),
            flags.bits(),
            xx.as_mut_ptr(),
            serr.as_mut_ptr(),
        )
    };
    drop(_guard);
    if ret < 0 {
        return Err(Error::new(read_serr(&serr)));
    }
    Ok(Position {
        body,
        longitude: xx[0],
        latitude: xx[1],
        distance: xx[2],
        speed_longitude: xx[3],
        speed_latitude: xx[4],
        speed_distance: xx[5],
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
    houses_raw(jd_ut, None, latitude, longitude, system)
}

/// [`houses`] with explicit flags — pass [`Flags::SIDEREAL`] for sidereal
/// cusps (after [`set_sidereal_mode`]).
pub fn houses_with(
    jd_ut: f64,
    latitude: f64,
    longitude: f64,
    system: HouseSystem,
    flags: Flags,
) -> Result<Houses> {
    houses_raw(jd_ut, Some(flags), latitude, longitude, system)
}

fn houses_raw(
    jd_ut: f64,
    flags: Option<Flags>,
    latitude: f64,
    longitude: f64,
    system: HouseSystem,
) -> Result<Houses> {
    let mut cusps = [0.0f64; 13];
    let mut ascmc = [0.0f64; 10];
    let _guard = ffi_lock();
    let ret = unsafe {
        match flags {
            None => sys::swe_houses(
                jd_ut,
                latitude,
                longitude,
                system.to_swe(),
                cusps.as_mut_ptr(),
                ascmc.as_mut_ptr(),
            ),
            Some(f) => sys::swe_houses_ex(
                jd_ut,
                f.bits(),
                latitude,
                longitude,
                system.to_swe(),
                cusps.as_mut_ptr(),
                ascmc.as_mut_ptr(),
            ),
        }
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

/// Select the ayanamsha used by [`Flags::SIDEREAL`] computations and
/// [`ayanamsha`]. Affects the whole process.
pub fn set_sidereal_mode(mode: Ayanamsha) {
    let _guard = ffi_lock();
    unsafe { sys::swe_set_sid_mode(mode.to_swe(), 0.0, 0.0) };
}

/// The ayanamsha value (tropical minus sidereal longitude, in degrees) at a
/// Julian day (UT), for the mode chosen with [`set_sidereal_mode`].
pub fn ayanamsha(jd_ut: f64) -> Result<f64> {
    let mut daya = 0.0f64;
    let mut serr = [0 as c_char; sys::SE_MAX_STNAME];
    let _guard = ffi_lock();
    let ret = unsafe {
        sys::swe_get_ayanamsa_ex_ut(jd_ut, sys::SEFLG_SWIEPH, &mut daya, serr.as_mut_ptr())
    };
    drop(_guard);
    if ret < 0 {
        return Err(Error::new(read_serr(&serr)));
    }
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
/// Returns `Ok(None)` if the search finds no further eclipse.
pub fn next_solar_eclipse(jd_start: f64) -> Result<Option<Eclipse>> {
    eclipse_search(jd_start, true)
}

/// Find the next lunar eclipse at or after `jd_start` (UT).
pub fn next_lunar_eclipse(jd_start: f64) -> Result<Option<Eclipse>> {
    eclipse_search(jd_start, false)
}

fn eclipse_search(jd_start: f64, solar: bool) -> Result<Option<Eclipse>> {
    let mut tret = [0.0f64; 10];
    let mut serr = [0 as c_char; sys::SE_MAX_STNAME];
    // The eclipse search functions reject SEFLG_SPEED; pass the bare
    // ephemeris flag.
    let ifl = sys::SEFLG_SWIEPH;
    let _guard = ffi_lock();
    let ret = unsafe {
        if solar {
            sys::swe_sol_eclipse_when_glob(
                jd_start,
                ifl,
                0,
                tret.as_mut_ptr(),
                0,
                serr.as_mut_ptr(),
            )
        } else {
            sys::swe_lun_eclipse_when(jd_start, ifl, 0, tret.as_mut_ptr(), 0, serr.as_mut_ptr())
        }
    };
    drop(_guard);
    if ret < 0 {
        return Err(Error::new(read_serr(&serr)));
    }
    if ret == 0 {
        return Ok(None);
    }
    Ok(Some(Eclipse {
        maximum_jd: tret[0],
        kind: EclipseKind::from_retflag(ret),
    }))
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
        let e = next_solar_eclipse(start)
            .unwrap()
            .expect("eclipse expected");
        assert_eq!(e.kind, EclipseKind::Total);
        let d = date_from_julian_day(e.maximum_jd);
        assert_eq!((d.year, d.month, d.day), (2024, 4, 8));
    }

    #[test]
    fn lahiri_ayanamsha_near_24_degrees_at_j2000() {
        set_sidereal_mode(Ayanamsha::Lahiri);
        let aya = ayanamsha(2451545.0).unwrap();
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
