fn main() {
    println!("cargo:rerun-if-changed=vendor");
    cc::Build::new()
        .files([
            "vendor/swecl.c",
            "vendor/swedate.c",
            "vendor/swehel.c",
            "vendor/swehouse.c",
            "vendor/swejpl.c",
            "vendor/swemmoon.c",
            "vendor/swemplan.c",
            "vendor/sweph.c",
            "vendor/swephlib.c",
        ])
        .include("vendor")
        // Suppress swe_get_library_path (dladdr-based, GNU-only, unused here).
        .define("NO_SWE_GLP", None)
        // Disable thread-local storage for the library's global `swed` state.
        // With TLS enabled each OS thread gets its own copy, so configuration
        // calls like swe_set_ephe_path() made on one thread would not apply on
        // another (e.g. a worker/blocking pool). The safe `sweph` crate instead
        // serializes every FFI entry point behind a single Mutex, for which one
        // shared `swed` is the correct shape. If you call these raw bindings
        // directly, you are responsible for that serialization.
        .define("TLSOFF", None)
        .warnings(false)
        .compile("sweph");
}
