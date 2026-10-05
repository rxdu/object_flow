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
//! - Inside `{ … }` or `[ … ]`, a plain scalar holding `?` is refused, as
//!   PyYAML's scanner ends a plain scalar there and refuses what follows,
//!   where YAML 1.2 reads `file?` as one scalar.

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
    /// A mapping, its entries in the order written, every key a scalar, and
    /// the line each key is on.
    Map {
        entries: Vec<(String, Node)>,
        key_lines: Vec<usize>,
        line: usize,
    },
}

/// A step of a path from a document's root: a mapping's key or a sequence's
/// index, as the flow checker's `line_index` keys them.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Seg {
    Key(String),
    Index(usize),
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

    /// The line a finding at a path is reported on: the line of the path's
    /// last key or item, or of the deepest one of its prefixes the document
    /// holds, and the first line where it holds none, as the flow checker's
    /// `line_of` reports it.
    pub fn line_at(&self, path: &[Seg]) -> usize {
        let (mut node, mut line) = (self, 1);
        for seg in path {
            match (node, seg) {
                (
                    Node::Map {
                        entries, key_lines, ..
                    },
                    Seg::Key(k),
                ) => match entries.iter().position(|(n, _)| n == k) {
                    Some(i) => {
                        line = key_lines.get(i).copied().unwrap_or(line);
                        node = &entries[i].1;
                    }
                    None => break,
                },
                (Node::Seq { items, .. }, Seg::Index(i)) if *i < items.len() => {
                    line = items[*i].line();
                    node = &items[*i];
                }
                _ => break,
            }
        }
        line
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

    /// The node as JSON, as the flow checker's strict loader reads it: a
    /// plain `true` or `false` a boolean, a plain whole number a number, a
    /// plain null a null, every other scalar a string, a mapping in the order
    /// written.
    pub fn to_json(&self) -> serde_json::Value {
        match self {
            Node::Scalar { .. } if self.is_null() => serde_json::Value::Null,
            Node::Scalar { text, .. } => {
                if let Some(b) = self.boolean() {
                    serde_json::Value::Bool(b)
                } else if let Some(n) = self.integer() {
                    serde_json::Value::from(n)
                } else {
                    serde_json::Value::String(text.clone())
                }
            }
            Node::Seq { items, .. } => {
                serde_json::Value::Array(items.iter().map(Node::to_json).collect())
            }
            Node::Map { entries, .. } => serde_json::Value::Object(
                entries
                    .iter()
                    .map(|(k, v)| (k.clone(), v.to_json()))
                    .collect(),
            ),
        }
    }

    /// A null scalar: empty and plain, or `null` or `~` written plain.
    pub fn is_null(&self) -> bool {
        matches!(self, Node::Scalar { text, plain: true, .. } if text.is_empty() || text == "null" || text == "~")
    }
}

enum Open {
    Seq(Vec<Node>, usize),
    Map(
        Vec<(String, Node)>,
        Option<(String, usize)>,
        usize,
        Vec<usize>,
    ),
}

fn line_of(span: &Span) -> usize {
    span.start.line()
}

/// The document a text holds, as a tree; an empty text is a null scalar.
pub fn load(text: &str) -> Result<Node, YamlError> {
    let mut parser = Parser::new_from_str(text);
    let chars: Vec<char> = text.chars().collect();
    // for each open collection written in flow style, the character that
    // closes it
    let mut flows: Vec<Option<char>> = Vec::new();
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
            Some(Open::Map(entries, key, _, key_lines)) => match key.take() {
                None => match node {
                    Node::Scalar { text, line, .. } => {
                        if entries.iter().any(|(k, _)| *k == text) {
                            return Err(YamlError {
                                line,
                                message: format!("the key '{text}' appears twice in one mapping"),
                            });
                        }
                        *key = Some((text, line));
                        Ok(())
                    }
                    other => Err(YamlError {
                        line: other.line(),
                        message: "a mapping's key is a scalar".to_string(),
                    }),
                },
                Some((k, line)) => {
                    entries.push((k, node));
                    key_lines.push(line);
                    Ok(())
                }
            },
        }
    }

    while let Some(next) = parser.next_event() {
        let (event, span) = next.map_err(|e| YamlError {
            line: e.marker().line(),
            // in PyYAML's words, which the flow checker's messages follow
            message: match e.info() {
                "mapping values are not allowed in this context" => {
                    "mapping values are not allowed here".to_string()
                }
                other => other.to_string(),
            },
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
                if let Some(Some(close)) = flows.last()
                    && style == ScalarStyle::Plain
                    && value.contains('?')
                {
                    return Err(YamlError {
                        line,
                        message: format!("expected ',' or '{close}', but got '?'"),
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
            Event::SequenceStart(..) => {
                flows.push((chars.get(span.start.index()) == Some(&'[')).then_some(']'));
                stack.push(Open::Seq(Vec::new(), line));
            }
            Event::MappingStart(..) => {
                flows.push((chars.get(span.start.index()) == Some(&'{')).then_some('}'));
                stack.push(Open::Map(Vec::new(), None, line, Vec::new()));
            }
            Event::SequenceEnd | Event::MappingEnd => {
                flows.pop();
                let node = match stack.pop() {
                    Some(Open::Seq(items, line)) => Node::Seq { items, line },
                    Some(Open::Map(entries, _, line, key_lines)) => Node::Map {
                        entries,
                        key_lines,
                        line,
                    },
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
    fn a_path_is_reported_at_its_key_or_its_deepest_prefix() {
        let doc = load("module: m\ntypes:\n  Job:\n    states:\n      - A\n      - B\n").unwrap();
        let at = |p: &[&str]| {
            let path: Vec<Seg> = p
                .iter()
                .map(|s| {
                    s.parse()
                        .map_or_else(|_| Seg::Key(s.to_string()), Seg::Index)
                })
                .collect();
            doc.line_at(&path)
        };
        assert_eq!(at(&["types", "Job"]), 3);
        assert_eq!(at(&["types", "Job", "states", "1"]), 6);
        assert_eq!(at(&["types", "Job", "nothing", "here"]), 3);
        assert_eq!(at(&["absent"]), 1);
    }

    #[test]
    fn a_question_mark_inside_a_flow_collection_is_refused() {
        let err = load("a: 1\nphoto: { type: file? }\n").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(load("photo: [ok, why?]\n").is_err());
        // in block style, and quoted, it is text
        assert_eq!(
            load("q: why?\n").unwrap().get("q").and_then(Node::text),
            Some("why?")
        );
        assert!(load("p: { q: 'why?' }\n").is_ok());
        // a multi-byte character before the collection leaves it found
        assert!(load("é: { type: file? }\n").is_err());
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
