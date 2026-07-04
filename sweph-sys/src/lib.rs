//! Raw FFI bindings to the Swiss Ephemeris C library.
//!
//! The C sources (v2.10.03, pinned in `vendor/UPSTREAM_COMMIT`) are vendored
//! and compiled by `build.rs` with `TLSOFF`, so the library keeps **one
//! process-wide state** (ephemeris path, open file handles, caches) shared by
//! all threads. The Swiss Ephemeris is not thread-safe: callers must
//! serialize every call into this crate. The companion `sweph` crate does
//! that behind a mutex and is the recommended entry point; use these raw
//! bindings directly only if you provide equivalent locking.
//!
//! Swiss Ephemeris is © Astrodienst AG, dual-licensed AGPL-3.0 or the Swiss
//! Ephemeris Professional License. This crate is distributed under AGPL-3.0.

use std::os::raw::{c_char, c_double, c_int};

// ---------------------------------------------------------------------------
// Body numbers (ipl)
// ---------------------------------------------------------------------------
pub const SE_SUN: c_int = 0;
pub const SE_MOON: c_int = 1;
pub const SE_MERCURY: c_int = 2;
pub const SE_VENUS: c_int = 3;
pub const SE_MARS: c_int = 4;
pub const SE_JUPITER: c_int = 5;
pub const SE_SATURN: c_int = 6;
pub const SE_URANUS: c_int = 7;
pub const SE_NEPTUNE: c_int = 8;
pub const SE_PLUTO: c_int = 9;
pub const SE_MEAN_NODE: c_int = 10;
pub const SE_TRUE_NODE: c_int = 11;
pub const SE_MEAN_APOG: c_int = 12;
pub const SE_OSCU_APOG: c_int = 13;
pub const SE_EARTH: c_int = 14;
pub const SE_CHIRON: c_int = 15;
pub const SE_PHOLUS: c_int = 16;
pub const SE_CERES: c_int = 17;
pub const SE_PALLAS: c_int = 18;
pub const SE_JUNO: c_int = 19;
pub const SE_VESTA: c_int = 20;
/// Add a minor-planet catalogue number to this offset to address any
/// numbered asteroid (requires the corresponding `se*.se1` asteroid file).
pub const SE_AST_OFFSET: c_int = 10000;

// ---------------------------------------------------------------------------
// Calendar flags (gregflag)
// ---------------------------------------------------------------------------
pub const SE_JUL_CAL: c_int = 0;
pub const SE_GREG_CAL: c_int = 1;

// ---------------------------------------------------------------------------
// Ephemeris / computation flags (iflag)
// ---------------------------------------------------------------------------
pub const SEFLG_JPLEPH: c_int = 1;
pub const SEFLG_SWIEPH: c_int = 2;
pub const SEFLG_MOSEPH: c_int = 4;
pub const SEFLG_HELCTR: c_int = 8;
pub const SEFLG_TRUEPOS: c_int = 16;
pub const SEFLG_J2000: c_int = 32;
pub const SEFLG_NONUT: c_int = 64;
pub const SEFLG_SPEED3: c_int = 128;
pub const SEFLG_SPEED: c_int = 256;
pub const SEFLG_NOGDEFL: c_int = 512;
pub const SEFLG_NOABERR: c_int = 1024;
pub const SEFLG_ASTROMETRIC: c_int = SEFLG_NOABERR | SEFLG_NOGDEFL;
pub const SEFLG_EQUATORIAL: c_int = 2048;
pub const SEFLG_XYZ: c_int = 4096;
pub const SEFLG_RADIANS: c_int = 8192;
pub const SEFLG_BARYCTR: c_int = 16384;
pub const SEFLG_TOPOCTR: c_int = 32768;
pub const SEFLG_SIDEREAL: c_int = 65536;

// ---------------------------------------------------------------------------
// Sidereal modes (swe_set_sid_mode)
// ---------------------------------------------------------------------------
pub const SE_SIDM_FAGAN_BRADLEY: c_int = 0;
pub const SE_SIDM_LAHIRI: c_int = 1;
pub const SE_SIDM_DELUCE: c_int = 2;
pub const SE_SIDM_RAMAN: c_int = 3;
pub const SE_SIDM_KRISHNAMURTI: c_int = 5;

// ---------------------------------------------------------------------------
// Eclipse type bitmask (ifltype filter + retflag from when_glob/when)
// ---------------------------------------------------------------------------
pub const SE_ECL_CENTRAL: c_int = 1;
pub const SE_ECL_NONCENTRAL: c_int = 2;
pub const SE_ECL_TOTAL: c_int = 4;
pub const SE_ECL_ANNULAR: c_int = 8;
pub const SE_ECL_PARTIAL: c_int = 16;
pub const SE_ECL_ANNULAR_TOTAL: c_int = 32;
pub const SE_ECL_PENUMBRAL: c_int = 64;

/// Size the C API requires for `serr` error-message buffers (AS_MAXCH).
pub const SE_MAX_STNAME: usize = 256;

extern "C" {
    pub fn swe_set_ephe_path(path: *const c_char);
    pub fn swe_close();
    pub fn swe_version(svers: *mut c_char) -> *mut c_char;

    pub fn swe_calc_ut(
        tjd_ut: c_double,
        ipl: c_int,
        iflag: c_int,
        xx: *mut c_double,
        serr: *mut c_char,
    ) -> c_int;

    pub fn swe_calc(
        tjd_et: c_double,
        ipl: c_int,
        iflag: c_int,
        xx: *mut c_double,
        serr: *mut c_char,
    ) -> c_int;

    pub fn swe_get_planet_name(ipl: c_int, name: *mut c_char) -> *mut c_char;

    pub fn swe_julday(
        year: c_int,
        month: c_int,
        day: c_int,
        hour: c_double,
        gregflag: c_int,
    ) -> c_double;

    pub fn swe_revjul(
        jd: c_double,
        gregflag: c_int,
        jyear: *mut c_int,
        jmon: *mut c_int,
        jday: *mut c_int,
        jut: *mut c_double,
    );

    pub fn swe_utc_to_jd(
        iyear: c_int,
        imonth: c_int,
        iday: c_int,
        ihour: c_int,
        imin: c_int,
        dsec: c_double,
        gregflag: c_int,
        dret: *mut c_double,
        serr: *mut c_char,
    ) -> c_int;

    pub fn swe_deltat(tjd: c_double) -> c_double;

    pub fn swe_houses(
        tjd_ut: c_double,
        geolat: c_double,
        geolon: c_double,
        hsys: c_int,
        cusps: *mut c_double,
        ascmc: *mut c_double,
    ) -> c_int;

    pub fn swe_houses_ex(
        tjd_ut: c_double,
        iflag: c_int,
        geolat: c_double,
        geolon: c_double,
        hsys: c_int,
        cusps: *mut c_double,
        ascmc: *mut c_double,
    ) -> c_int;

    pub fn swe_house_name(hsys: c_int) -> *const c_char;

    pub fn swe_set_topo(geolon: c_double, geolat: c_double, geoalt: c_double);

    pub fn swe_set_sid_mode(sid_mode: c_int, t0: c_double, ayan_t0: c_double);

    pub fn swe_get_ayanamsa_ex_ut(
        tjd_ut: c_double,
        iflag: c_int,
        daya: *mut c_double,
        serr: *mut c_char,
    ) -> c_int;

    pub fn swe_sol_eclipse_when_glob(
        tjd_start: c_double,
        ifl: c_int,
        ifltype: c_int,
        tret: *mut c_double,
        backward: c_int,
        serr: *mut c_char,
    ) -> c_int;

    pub fn swe_lun_eclipse_when(
        tjd_start: c_double,
        ifl: c_int,
        ifltype: c_int,
        tret: *mut c_double,
        backward: c_int,
        serr: *mut c_char,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    // The C library is not thread-safe and `cargo test` runs tests on
    // multiple threads, so every test serializes on this lock — the same
    // discipline the `sweph` crate applies for real callers.
    static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn julday_j2000() {
        let _guard = TEST_LOCK.lock().unwrap();
        let jd = unsafe { swe_julday(2000, 1, 1, 12.0, SE_GREG_CAL) };
        assert!((jd - 2451545.0).abs() < 0.0001, "JD was {jd}");
    }

    #[test]
    fn calc_sun_position() {
        let _guard = TEST_LOCK.lock().unwrap();
        unsafe {
            let jd = swe_julday(2000, 1, 1, 12.0, SE_GREG_CAL);
            let mut xx = [0.0_f64; 6];
            let mut serr = [0i8; SE_MAX_STNAME];
            let ret = swe_calc_ut(
                jd,
                SE_SUN,
                SEFLG_SPEED | SEFLG_SWIEPH,
                xx.as_mut_ptr(),
                serr.as_mut_ptr().cast(),
            );
            assert!(ret >= 0, "swe_calc_ut failed: {ret}");
            let sun_lon = xx[0];
            assert!(
                sun_lon > 270.0 && sun_lon < 290.0,
                "Sun longitude was {sun_lon}"
            );
        }
    }

    #[test]
    fn version_string() {
        let _guard = TEST_LOCK.lock().unwrap();
        let mut buf = [0i8; SE_MAX_STNAME];
        let s = unsafe {
            swe_version(buf.as_mut_ptr().cast());
            std::ffi::CStr::from_ptr(buf.as_ptr().cast())
                .to_string_lossy()
                .into_owned()
        };
        assert!(s.starts_with("2.10"), "unexpected version: {s}");
    }
}
