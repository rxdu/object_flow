//! The strict YAML loader (ADR-0143 decision 3): `saphyr-parser`'s events
//! built into a tree with the line of every node, reading plain scalars as
//! the flow checker's strict loader does (`scripts/check-flows.py`, `load`).
//!
//! - A key repeated in one mapping is refused, not silently overwritten.
//! - A plain scalar is a boolean only when it is exactly `true` or `false`,
//!   so `NO`, `TRUE` and `yes` stay names (YAML 1.2's core schema, and the
//!   flow format's own rule, `flow-format.md` §2).
//! - A timestamp stays text, as the strict loader leaves it.
//! - An alias or anchor is refused: a flow description writes each thing once.

use std::fmt;

use saphyr_parser::{Event, Parser, ScalarStyle, Span};

/// A node of a YAML document, with the 1-indexed line it begins on.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A scalar: its text, and whether it was written plain, unquoted.
    Scalar {
        text: String,
        plain: bool,
        line: usize,
    },
    Seq {
        items: Vec<Node>,
        line: usize,
    },
    /// A mapping, its entries in the order written, every key a scalar.
    Map {
        entries: Vec<(String, Node)>,
        line: usize,
    },
}

/// Why a document does not load, and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct YamlError {
    pub line: usize,
    pub message: String,
}

impl fmt::Display for YamlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for YamlError {}

impl Node {
    pub fn line(&self) -> usize {
        match self {
            Node::Scalar { line, .. } | Node::Seq { line, .. } | Node::Map { line, .. } => *line,
        }
    }

    /// The text of a scalar.
    pub fn text(&self) -> Option<&str> {
        match self {
            Node::Scalar { text, .. } => Some(text),
            _ => None,
        }
    }

    /// A plain `true` or `false`, the only booleans the strict loader reads.
    pub fn boolean(&self) -> Option<bool> {
        match self {
            Node::Scalar {
                text, plain: true, ..
            } if text == "true" => Some(true),
            Node::Scalar {
                text, plain: true, ..
            } if text == "false" => Some(false),
            _ => None,
        }
    }

    /// A plain scalar that is a whole number, as YAML reads `50` or `-3`.
    pub fn integer(&self) -> Option<i64> {
        match self {
            Node::Scalar {
                text, plain: true, ..
            } => text.replace('_', "").parse().ok(),
            _ => None,
        }
    }

    /// A mapping's value for a key.
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map { entries, .. } => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// A mapping's entries, or none for any other node.
    pub fn entries(&self) -> &[(String, Node)] {
        match self {
            Node::Map { entries, .. } => entries,
            _ => &[],
        }
    }

    /// A sequence's items, or none for any other node.
    pub fn items(&self) -> &[Node] {
        match self {
            Node::Seq { items, .. } => items,
            _ => &[],
        }
    }

    /// A null scalar: empty and plain, or `null` or `~` written plain.
    pub fn is_null(&self) -> bool {
        matches!(self, Node::Scalar { text, plain: true, .. } if text.is_empty() || text == "null" || text == "~")
    }
}

enum Open {
    Seq(Vec<Node>, usize),
    Map(Vec<(String, Node)>, Option<String>, usize),
}

fn line_of(span: &Span) -> usize {
    span.start.line()
}

/// The document a text holds, as a tree; an empty text is a null scalar.
pub fn load(text: &str) -> Result<Node, YamlError> {
    let mut parser = Parser::new_from_str(text);
    let mut stack: Vec<Open> = Vec::new();
    let mut root: Option<Node> = None;
    let mut documents = 0;

    fn place(stack: &mut [Open], root: &mut Option<Node>, node: Node) -> Result<(), YamlError> {
        match stack.last_mut() {
            None => {
                *root = Some(node);
                Ok(())
            }
            Some(Open::Seq(items, _)) => {
                items.push(node);
                Ok(())
            }
            Some(Open::Map(entries, key, _)) => match key.take() {
                None => match node {
                    Node::Scalar { text, line, .. } => {
                        if entries.iter().any(|(k, _)| *k == text) {
                            return Err(YamlError {
                                line,
                                message: format!("the key '{text}' appears twice in one mapping"),
                            });
                        }
                        *key = Some(text);
                        Ok(())
                    }
                    other => Err(YamlError {
                        line: other.line(),
                        message: "a mapping's key is a scalar".to_string(),
                    }),
                },
                Some(k) => {
                    entries.push((k, node));
                    Ok(())
                }
            },
        }
    }

    while let Some(next) = parser.next_event() {
        let (event, span) = next.map_err(|e| YamlError {
            line: e.marker().line(),
            message: e.info().to_string(),
        })?;
        let line = line_of(&span);
        match event {
            Event::StreamStart | Event::StreamEnd | Event::DocumentEnd | Event::Nothing => {}
            Event::DocumentStart(_) => {
                documents += 1;
                if documents > 1 {
                    return Err(YamlError {
                        line,
                        message: "a description is one document".to_string(),
                    });
                }
            }
            Event::Alias(_) => {
                return Err(YamlError {
                    line,
                    message: "an alias is refused: a flow description writes each thing once"
                        .to_string(),
                });
            }
            Event::Scalar(value, style, anchor, _tag) => {
                if anchor != 0 {
                    return Err(YamlError {
                        line,
                        message: "an anchor is refused".to_string(),
                    });
                }
                let node = Node::Scalar {
                    text: value.into_owned(),
                    plain: style == ScalarStyle::Plain,
                    line,
                };
                place(&mut stack, &mut root, node)?;
            }
            Event::SequenceStart(anchor, _) | Event::MappingStart(anchor, _) if anchor != 0 => {
                return Err(YamlError {
                    line,
                    message: "an anchor is refused".to_string(),
                });
            }
            Event::SequenceStart(..) => stack.push(Open::Seq(Vec::new(), line)),
            Event::MappingStart(..) => stack.push(Open::Map(Vec::new(), None, line)),
            Event::SequenceEnd | Event::MappingEnd => {
                let node = match stack.pop() {
                    Some(Open::Seq(items, line)) => Node::Seq { items, line },
                    Some(Open::Map(entries, _, line)) => Node::Map { entries, line },
                    None => {
                        return Err(YamlError {
                            line,
                            message: "a collection closes that never opened".to_string(),
                        });
                    }
                };
                place(&mut stack, &mut root, node)?;
            }
        }
    }
    Ok(root.unwrap_or(Node::Scalar {
        text: String::new(),
        plain: true,
        line: 1,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mapping_keeps_its_order_and_lines() {
        let doc =
            load("module: service\ntypes:\n  Job:\n    states: { OPEN: {}, DONE: {} }\n").unwrap();
        assert_eq!(doc.get("module").and_then(Node::text), Some("service"));
        let job = doc.get("types").and_then(|t| t.get("Job")).unwrap();
        assert_eq!(job.line(), 4);
        let states: Vec<&str> = job
            .get("states")
            .unwrap()
            .entries()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect();
        assert_eq!(states, ["OPEN", "DONE"]);
    }

    #[test]
    fn only_a_plain_true_or_false_is_a_boolean() {
        let doc = load("a: true\nb: NO\nc: 'true'\nd: TRUE\ne: yes\n").unwrap();
        assert_eq!(doc.get("a").unwrap().boolean(), Some(true));
        for k in ["b", "c", "d", "e"] {
            assert_eq!(doc.get(k).unwrap().boolean(), None, "{k}");
        }
        assert_eq!(doc.get("b").and_then(Node::text), Some("NO"));
    }

    #[test]
    fn a_timestamp_stays_text() {
        let doc = load("at: 2026-01-01T00:00:00Z\n").unwrap();
        assert_eq!(
            doc.get("at").and_then(Node::text),
            Some("2026-01-01T00:00:00Z")
        );
    }

    #[test]
    fn a_repeated_key_is_refused_with_its_line() {
        let err = load("guards:\n  a: deny\n  a: audit\n").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(err.message.contains("appears twice"));
    }

    #[test]
    fn anchors_aliases_and_bad_yaml_are_refused() {
        assert!(load("a: &x 1\nb: *x\n").is_err());
        assert!(load("a: [1, 2\n").is_err());
        assert!(load("a: 1\n---\nb: 2\n").is_err());
    }
}
