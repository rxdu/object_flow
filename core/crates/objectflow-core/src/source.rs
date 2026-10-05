//! A version's source: its modules, and their examples files, by the names
//! a change's digest gives them, and the digest itself (ADR-0135, ADR-0136).

use sha2::{Digest, Sha256};

use crate::yaml;

/// A file's name in a source: a description by its `module`, an examples
/// file by the module it is for followed by `.examples`, which no module's
/// name can be (ADR-0136). None where the text names neither.
pub fn name_of(text: &str) -> Option<String> {
    let doc = yaml::load(text).ok()?;
    if let Some(m) = doc.get("module").and_then(|n| n.text()) {
        return Some(m.to_string());
    }
    doc.get("examples_for")
        .and_then(|n| n.text())
        .map(|m| format!("{m}.examples"))
}

/// Whether a source's name is an examples file's.
pub fn is_examples(name: &str) -> bool {
    name.ends_with(".examples")
}

/// The digest a change records beside its source's reference: SHA-256 over
/// the files in ascending order of their names' UTF-8 bytes, each as its
/// name, a line feed, its length in bytes in decimal, a line feed and its
/// bytes as submitted, written `sha256:<hex>` (ADR-0135). None where two
/// files share a name, since a source holds one file per module.
pub fn digest(files: &[(String, String)]) -> Option<String> {
    let mut sorted: Vec<&(String, String)> = files.iter().collect();
    sorted.sort_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    if sorted.windows(2).any(|w| w[0].0 == w[1].0) {
        return None;
    }
    let mut h = Sha256::new();
    for (name, body) in sorted {
        h.update(name.as_bytes());
        h.update(b"\n");
        h.update(body.len().to_string().as_bytes());
        h.update(b"\n");
        h.update(body.as_bytes());
    }
    let hex: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    Some(format!("sha256:{hex}"))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_digest_sees_names_and_bytes_and_not_the_order_given() {
        let a = ("inventory".to_string(), "module: inventory\n".to_string());
        let b = ("people".to_string(), "module: people\n".to_string());
        assert_eq!(
            digest(&[a.clone(), b.clone()]),
            digest(&[b.clone(), a.clone()])
        );
        let changed = ("people".to_string(), "module: people \n".to_string());
        assert_ne!(
            digest(&[a.clone(), b.clone()]),
            digest(&[a.clone(), changed])
        );
        let moved = digest(&[("ab".into(), "c".into())]);
        assert_ne!(moved, digest(&[("a".into(), "bc".into())]));
        assert_eq!(digest(&[a.clone(), a]), None);
        assert_eq!(
            name_of("module: inventory\ntypes: {}\n").as_deref(),
            Some("inventory")
        );
        assert_eq!(
            name_of("examples_for: inventory\n").as_deref(),
            Some("inventory.examples")
        );
    }
}
