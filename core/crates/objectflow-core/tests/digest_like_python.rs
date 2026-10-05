//! A source's digest, held to the flow checker's `--digest`, which is the
//! reference a repository's own checks use (ADR-0135, ADR-0155 decision 2):
//! every module of the design with its examples file, byte for byte.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::process::Command;

use objectflow_core::source::{digest, name_of};

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

#[test]
fn every_design_module_digests_as_the_flow_checker_digests_it() {
    let dir = repo().join("docs/design/flow-format/examples");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        if !name.ends_with(".yaml") || name.ends_with(".examples.yaml") {
            continue;
        }
        let mut paths = vec![path.clone()];
        let examples = dir.join(name.replace(".yaml", ".examples.yaml"));
        if examples.exists() {
            paths.push(examples);
        }
        let files: Vec<(String, String)> = paths
            .iter()
            .map(|p| {
                let text = std::fs::read_to_string(p).unwrap();
                (name_of(&text).unwrap(), text)
            })
            .collect();
        let ours = digest(&files).unwrap();
        let out = Command::new("python3")
            .arg(repo().join("scripts/check-flows.py"))
            .arg("--digest")
            .args(&paths)
            .output()
            .expect("python3 runs the flow checker");
        let theirs = String::from_utf8(out.stdout).unwrap().trim().to_string();
        assert_eq!(ours, theirs, "{name}");
        checked += 1;
    }
    assert!(checked >= 10, "{checked} modules");
}
