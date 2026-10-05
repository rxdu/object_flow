//! A description read as the reference checker reads its dictionaries, and
//! the scans of expression text its regular expressions make.
//!
//! `check-flows.py` reads a module as nested Python dictionaries, every
//! absent member taken as empty (`x.get(k) or {}`), and many of its checks
//! scan expression text with patterns that look behind and ahead, which the
//! `regex` crate does not offer; those scans are written out here, each
//! beside the pattern it stands for.

use std::collections::BTreeSet;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value};

use crate::yaml::Seg;

pub type Obj = Map<String, Value>;
pub type Path = Vec<Seg>;
/// A finding of a step: where, its code and what it says.
pub type Found = Vec<(Path, String, String)>;

static EMPTY: LazyLock<Obj> = LazyLock::new(Map::new);

/// A path built of keys and indexes: `segs!["types", tn, "attributes", a]`.
macro_rules! segs {
    ($($p:expr),* $(,)?) => { vec![$(crate::yaml::Seg::from($p)),*] };
}
pub(crate) use segs;

impl From<&str> for Seg {
    fn from(s: &str) -> Seg {
        Seg::Key(s.to_string())
    }
}

impl From<String> for Seg {
    fn from(s: String) -> Seg {
        Seg::Key(s)
    }
}

impl From<&String> for Seg {
    fn from(s: &String) -> Seg {
        Seg::Key(s.clone())
    }
}

impl From<usize> for Seg {
    fn from(i: usize) -> Seg {
        Seg::Index(i)
    }
}

/// A path extended: `ext(&base, segs!["guards", g])`.
pub fn ext(base: impl AsRef<[Seg]>, more: Path) -> Path {
    let mut p = base.as_ref().to_vec();
    p.extend(more);
    p
}

/// A finding.
pub fn finding(path: Path, code: &str, message: String) -> (Path, String, String) {
    (path, code.to_string(), message)
}

/// A value as a mapping, or an empty one.
pub fn as_obj(v: &Value) -> &Obj {
    match v {
        Value::Object(o) => o,
        _ => &EMPTY,
    }
}

/// A member as a mapping, or an empty one: Python's `x.get(k) or {}`.
pub fn obj<'a>(v: &'a Value, k: &str) -> &'a Obj {
    v.get(k).map_or(&EMPTY, as_obj)
}

/// A member's text, where it is a string.
pub fn text<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// A member as a list, or an empty one.
pub fn list<'a>(v: &'a Value, k: &str) -> &'a [Value] {
    match v.get(k) {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}

/// The strings of a member that is a list of them.
pub fn strings(v: &Value, k: &str) -> Vec<String> {
    list(v, k)
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_string)
        .collect()
}

/// The keys of a member that is a mapping.
pub fn keys(v: &Value, k: &str) -> Vec<String> {
    obj(v, k).keys().cloned().collect()
}

/// Python's truthiness.
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|f| f != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// Whether a member is truthy.
pub fn flag(v: &Value, k: &str) -> bool {
    truthy(v.get(k))
}

pub fn has(v: &Value, k: &str) -> bool {
    v.get(k).is_some()
}

/// Python's `str` of a value, as a message or a comparison writes it.
pub fn py_str(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        other => crate::check::schema::repr(other),
    }
}

/// Python's `str(spec.get(k, default))` where the default is text.
pub fn str_or(v: &Value, k: &str, default: &str) -> String {
    v.get(k).map_or_else(|| default.to_string(), py_str)
}

/// `spec.get("type", spec.get("reference", ""))`, as text.
pub fn kind_text(spec: &Value) -> String {
    match spec.get("type") {
        Some(t) => py_str(t),
        None => str_or(spec, "reference", ""),
    }
}

pub fn optional(spec: &Value) -> bool {
    flag(spec, "optional")
}

/// `" ".join(str(text).split())`.
pub fn squash(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A compiled pattern, which every caller writes correctly.
pub fn re(pattern: &str) -> Regex {
    #[allow(clippy::expect_used)]
    Regex::new(pattern).expect("a pattern of the checker")
}

/// Python's `\w`: a letter, a digit or an underscore, of any script.
pub fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn lower_name_len(chars: &[char], at: usize) -> usize {
    if !chars.get(at).is_some_and(char::is_ascii_lowercase) {
        return 0;
    }
    let mut n = 1;
    while chars
        .get(at + n)
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
    {
        n += 1;
    }
    n
}

/// Each `(?<![\w.])(this\.)?([a-z][a-z0-9_]*)\b(?!\s*\()` in a text: a
/// lower-case name beginning a path and not called, with whether `this.`
/// came before it, and its start and end, by char.
pub fn path_heads(text: &str) -> Vec<(bool, String, usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let at_name = |start: usize| -> Option<usize> {
        let n = lower_name_len(&chars, start);
        if n == 0 {
            return None;
        }
        let end = start + n;
        if chars.get(end).is_some_and(|c| is_word(*c)) {
            return None; // \b fails, and a shorter name ends inside a word
        }
        let mut j = end;
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        if chars.get(j) == Some(&'(') {
            return None;
        }
        Some(end)
    };
    while i < chars.len() {
        let before_ok = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.');
        if before_ok {
            let this: String = chars[i..chars.len().min(i + 5)].iter().collect();
            if this == "this."
                && let Some(end) = at_name(i + 5)
            {
                out.push((true, chars[i + 5..end].iter().collect(), i, end));
                i = end;
                continue;
            }
            if let Some(end) = at_name(i) {
                out.push((false, chars[i..end].iter().collect(), i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Each `(?<![.\w])[A-Z][A-Z0-9_]*(?![\w.])` in a text: an upper-case word
/// standing alone, with its start and end, by char.
pub fn bare_upper(text: &str) -> Vec<(String, usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let before_ok = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.');
        if before_ok && chars[i].is_ascii_uppercase() {
            let mut end = i + 1;
            while chars
                .get(end)
                .is_some_and(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
            {
                end += 1;
            }
            if !chars.get(end).is_some_and(|c| is_word(*c) || *c == '.') {
                out.push((chars[i..end].iter().collect(), i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Each `(?<![\w.])([a-z][a-z0-9_]*)\.([a-z][a-z0-9_]*)\(` in a text, with
/// the char index after its `(`.
pub fn dotted_calls(text: &str) -> Vec<(String, String, usize)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let before_ok = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.');
        let a = if before_ok {
            lower_name_len(&chars, i)
        } else {
            0
        };
        if a > 0 && chars.get(i + a) == Some(&'.') {
            let b = lower_name_len(&chars, i + a + 1);
            let open = i + a + 1 + b;
            if b > 0 && chars.get(open) == Some(&'(') {
                out.push((
                    chars[i..i + a].iter().collect(),
                    chars[i + a + 1..open].iter().collect(),
                    open + 1,
                ));
                i = open + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Each member read as `(?<![\w.])<name>\.([a-z][a-z0-9_]*)` in a text.
pub fn members_of(text: &str, name: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let want: Vec<char> = name.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + want.len() < chars.len() {
        let before_ok = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.');
        if before_ok && chars[i..i + want.len()] == want[..] && chars[i + want.len()] == '.' {
            let n = lower_name_len(&chars, i + want.len() + 1);
            if n > 0 {
                let start = i + want.len() + 1;
                out.push(chars[start..start + n].iter().collect());
                i = start + n;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// Whether a text has `(?<![\w.])<name>\s*\.`: the name read through.
pub fn read_through(text: &str, name: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let want: Vec<char> = name.chars().collect();
    (0..chars.len()).any(|i| {
        let before_ok = i == 0 || !(is_word(chars[i - 1]) || chars[i - 1] == '.');
        if !before_ok || i + want.len() > chars.len() || chars[i..i + want.len()] != want[..] {
            return false;
        }
        let mut j = i + want.len();
        while chars.get(j).is_some_and(|c| c.is_whitespace()) {
            j += 1;
        }
        chars.get(j) == Some(&'.')
    })
}

pub const KEYWORDS: &[&str] = &[
    "and", "or", "not", "implies", "is", "null", "in", "where", "if", "then", "else", "true",
    "false", "distinct", "over", "last", "fresh",
];
pub const AGGREGATES: &[&str] = &["count", "sum", "min", "max", "all", "any", "none"];
pub const DURATION_UNITS: &[&str] = &["s", "min", "h", "day", "days", "week", "weeks"];
pub const FLOW_DATA: &[&str] = &["intervals", "transitions", "labels"];
pub const ACTOR_KINDS: &[&str] = &["human", "agent", "service"];

static BOUND: LazyLock<Regex> = LazyLock::new(|| {
    re(&format!(
        r"\b(?:{})\(\s*(?:distinct\s+)?([a-z][a-z0-9_]*)\s+in\b",
        AGGREGATES.join("|")
    ))
});
static STRING: LazyLock<Regex> = LazyLock::new(|| re(r#""[^"]*""#));

/// The member names an expression reads: each lower-case name that begins a
/// path, `this.<name>` included, less keywords, actor kinds, the names an
/// aggregate binds, a binding's own name, a metric's name, a duration's
/// unit and `this.`'s flow data (`check-flows.py`'s `reads`).
pub fn reads(expr: &str) -> BTreeSet<String> {
    let text = STRING.replace_all(&squash(expr), "\"\"").into_owned();
    let bound: BTreeSet<String> = BOUND
        .captures_iter(&text)
        .map(|c| c[1].to_string())
        .collect();
    let chars: Vec<char> = text.chars().collect();
    let mut out = BTreeSet::new();
    for (this, name, start, end) in path_heads(&text) {
        let before: String = chars[..start].iter().collect();
        let after: String = chars[end..].iter().collect();
        if KEYWORDS.contains(&name.as_str())
            || ACTOR_KINDS.contains(&name.as_str())
            || bound.contains(&name)
            || after.trim_start().starts_with(":=")
            || before.trim_end().strip_suffix('(').is_some_and(|b| {
                b.strip_suffix("metric")
                    .is_some_and(|m| !m.chars().last().is_some_and(is_word))
            })
        {
            continue;
        }
        if DURATION_UNITS.contains(&name.as_str())
            && before
                .trim_end()
                .chars()
                .last()
                .is_some_and(|c| c.is_numeric())
        {
            continue;
        }
        if this && FLOW_DATA.contains(&name.as_str()) {
            continue;
        }
        out.insert(name);
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn reads_takes_what_a_path_begins_with_and_nothing_else() {
        assert_eq!(
            reads("engineer.login == actor.id"),
            set(&["actor", "engineer"])
        );
        assert_eq!(reads("count(r in checks where r.ok) > 2"), set(&["checks"]));
        assert_eq!(reads("eta < now + 2 days"), set(&["eta", "now"]));
        // as check-flows.py's reads answers: `this` read before a call
        assert_eq!(
            reads("this.intervals(state) and this.photo"),
            set(&["photo", "state", "this"])
        );
        assert_eq!(reads(r#"note == "a b c" and human"#), set(&["note"]));
        assert_eq!(
            reads("metric(throughput, site := site_id)"),
            set(&["site_id"])
        );
        assert_eq!(reads("this.Foo"), set(&["this"]));
    }

    #[test]
    fn the_scans_find_what_their_patterns_find() {
        assert_eq!(
            bare_upper("state == OPEN and USD 5 and Enum.X"),
            vec![("OPEN".into(), 9, 13), ("USD".into(), 18, 21)]
        );
        assert_eq!(
            dotted_calls("not risk.check(a, b)"),
            vec![("risk".into(), "check".into(), 15)]
        );
        assert_eq!(
            members_of("r.outcome != x and xr.no", "r"),
            vec!["outcome".to_string()]
        );
        assert!(read_through("owner .name", "owner"));
        assert!(!read_through("co_owner.name", "owner"));
    }
}
