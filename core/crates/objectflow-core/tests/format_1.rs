//! Format 1's corpus on the core (ADR-0180 decision 7): every bundle of the
//! design's sample modules as they stood when format 1 was promised, frozen
//! in `docs/design/flow-format/format-1/`, checks with no fatal finding, a
//! failed example among them, and every example is run, as the core ran all
//! 175 then. `scripts/check-compat.py` holds the digests and the reference
//! runner to the same corpus.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::BTreeMap;
use std::path::PathBuf;

use objectflow_core::check::{Severity, checked_reading};
use serde_json::Value;

fn corpus() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../docs/design/flow-format/format-1")
}

#[test]
fn the_core_holds_format_1() {
    let dir = corpus();
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.join("format-1.json")).expect("format-1.json"),
    )
    .expect("the corpus's manifest");
    let text = |name: &str| std::fs::read_to_string(dir.join(name)).expect("a frozen file");
    let read = |name: &str| {
        let path = dir.join(name);
        (std::path::Path::new(name).file_name() == Some(std::ffi::OsStr::new(name))
            && path.is_file())
        .then(|| std::fs::read_to_string(path).ok())
        .flatten()
    };
    let mut breaks = Vec::new();
    let bundles = manifest["bundles"].as_array().expect("its bundles");
    for bundle in bundles {
        let module = bundle["module"].as_str().expect("a module");
        let files: Vec<(String, String)> = bundle["files"]
            .as_array()
            .expect("its files")
            .iter()
            .map(|f| {
                let name = f.as_str().expect("a file name");
                (name.to_string(), text(name))
            })
            .collect();
        let previous: BTreeMap<String, String> = bundle
            .get("previous")
            .and_then(Value::as_object)
            .map(|m| {
                m.iter()
                    .map(|(n, p)| (n.clone(), text(p.as_str().expect("a file name"))))
                    .collect()
            })
            .unwrap_or_default();
        for f in checked_reading(&files, &previous, &read).findings {
            if f.severity == Severity::Fatal || f.code == "notrun" {
                breaks.push(format!(
                    "{module}: {}:{} {}: {}",
                    f.file, f.line, f.code, f.message
                ));
            }
        }
    }
    assert!(
        breaks.is_empty(),
        "format 1 is broken on the core:\n{}",
        breaks.join("\n")
    );
    assert_eq!(bundles.len(), 11, "the corpus's bundles");
}
