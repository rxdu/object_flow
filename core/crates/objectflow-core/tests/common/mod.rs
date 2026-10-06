//! What the tests holding the core to the Python reference share.

use std::path::PathBuf;

/// The Python the repository's scripts are pinned to
/// (`scripts/requirements-dev.txt`), which `scripts/setup/python-env`
/// builds; a test fails, not skips, without it, since the system's Python
/// may hold other versions of what the scripts read YAML and schemas with.
pub fn interpreter() -> PathBuf {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../scripts/.venv/bin/python");
    assert!(
        p.exists(),
        "no {}: build it with scripts/setup/python-env",
        p.display()
    );
    p
}
