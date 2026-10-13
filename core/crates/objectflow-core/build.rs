//! A build without SQLite's shared page cache turned off stops here, where
//! one built from outside `core/` served four heavy reads at once each in
//! what all four took (fifteenth production review, ADR-0202 decision 2).
//! The repository's `.cargo/config.toml` sets the flag for every build run
//! inside it.

fn main() {
    println!("cargo:rerun-if-env-changed=LIBSQLITE3_FLAGS");
    let flags = std::env::var("LIBSQLITE3_FLAGS").unwrap_or_default();
    if !flags
        .split_whitespace()
        .any(|f| f == "-USQLITE_ENABLE_MEMORY_MANAGEMENT")
    {
        eprintln!(
            "objectflow-core is built with LIBSQLITE3_FLAGS holding -USQLITE_ENABLE_MEMORY_MANAGEMENT, \
             which the repository's .cargo/config.toml sets for a build run inside it; got {flags:?}. \
             Build inside the repository, or set the variable: without it every connection's page \
             cache shares one lock, and heavy reads at once each take as long as all of them."
        );
        std::process::exit(1);
    }
}
