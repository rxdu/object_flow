//! A flow module as the core holds it, read from its description
//! (`flow-format.md` §2 to §4) by the strict loader.
//!
//! The model keeps everything the first slice runs, each expression parsed
//! once, and names everything it does not (ADR-0153 decision 3): a type that
//! binds a machine or extends a base, a transition with an effect, an
//! identifier minted from a sequence. A request that reaches one is answered
//! as not run, naming it, and never guessed. Whether a module is well formed
//! is the flow checker's to say, in this slice (ADR-0153 decision 5); the
//! loader refuses only what it cannot read.

use std::collections::BTreeMap;
use std::fmt;

use crate::expr::{self, Expr, Number};
use crate::yaml::{self, Node};

/// What a caller can do about a refusal (`DESIGN.md` §5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Remedy {
    SelfServiceable,
    Delegable,
    Temporal,
    Dependent,
    UnreachableFromHere,
}

impl Remedy {
    pub fn parse(text: &str) -> Option<Remedy> {
        Some(match text {
            "self_serviceable" => Remedy::SelfServiceable,
            "delegable" => Remedy::Delegable,
            "temporal" => Remedy::Temporal,
            "dependent" => Remedy::Dependent,
            "unreachable_from_here" => Remedy::UnreachableFromHere,
            _ => return None,
        })
    }

    pub fn text(self) -> &'static str {
        match self {
            Remedy::SelfServiceable => "self_serviceable",
            Remedy::Delegable => "delegable",
            Remedy::Temporal => "temporal",
            Remedy::Dependent => "dependent",
            Remedy::UnreachableFromHere => "unreachable_from_here",
        }
    }
}

/// How a guard is enforced (`flow-format.md` §4.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Enforce {
    /// It refuses the request.
    Deny,
    /// Its failure is recorded, and refuses nothing.
    Audit,
    /// Its failure is told to the caller, and refuses nothing.
    Warn,
}

/// An attribute's or input's declared type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declared {
    /// A built-in type or an enumeration, by name: `string`, `decimal(10,2)`,
    /// `Priority`.
    Scalar(String),
    /// A reference to an object of a type.
    Reference(String),
}

/// Whether an attribute's values are unique, in the four forms the schema
/// gives (`flow.schema.json`, `attribute.unique`).
#[derive(Clone, Debug, PartialEq)]
pub enum Unique {
    No,
    /// Across every object of the type, absent values aside.
    Yes,
    /// Within the scope its identifier names.
    InScope,
    /// Together with the attributes listed.
    With(Vec<String>),
    /// Among the objects the expression holds for.
    Where(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Attribute {
    pub declared: Declared,
    /// `[]`: a set of values, or of references.
    pub many: bool,
    pub optional: bool,
    pub default: Option<Expr>,
    pub unique: Unique,
    pub assignee: bool,
    pub personal: bool,
    /// The kind of actor whose identity it holds, for an actor's type.
    pub actor_kind: Option<String>,
    /// An identifier minted from a sequence, which the first slice does not
    /// run.
    pub minted: bool,
    /// A stored relationship end the other side owns or derives.
    pub opposite: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub declared: Declared,
    pub many: bool,
    pub optional: bool,
    pub default: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub category: String,
    pub is_final: bool,
    pub required_attributes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Condition {
    pub expression: Expr,
    pub remedy: Option<Remedy>,
}

/// A transition's kind (`flow-format.md` §4.8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Initial,
    External,
    Internal,
    Assertion,
    Erasure,
}

/// The states a transition may be taken from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum From {
    /// `any`: every state not final, and for an external transition every
    /// one but its target.
    Any,
    States(Vec<String>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub kind: Kind,
    pub from: Option<From>,
    pub to: Vec<String>,
    pub only_via: Vec<String>,
    pub proposable: bool,
    pub required_inputs: Vec<String>,
    pub optional_inputs: Vec<String>,
    pub inputs: Vec<(String, Input)>,
    pub guards: Vec<(String, Enforce)>,
    /// It declares an effect, which the first slice does not run.
    pub has_effect: bool,
    pub backdating_limit: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Type {
    pub name: String,
    pub module: String,
    pub attributes: Vec<(String, Attribute)>,
    pub derived: Vec<(String, Expr)>,
    pub states: Vec<(String, State)>,
    pub conditions: Vec<(String, Condition)>,
    pub invariants: Vec<(String, Expr)>,
    pub transitions: Vec<(String, Transition)>,
    /// What this type declares that the first slice does not run: a request
    /// on it is not run, naming the first (ADR-0153 decision 3).
    pub beyond: Vec<String>,
}

impl Type {
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a)
    }

    pub fn state(&self, name: &str) -> Option<&State> {
        self.states.iter().find(|(n, _)| n == name).map(|(_, s)| s)
    }

    pub fn transition(&self, name: &str) -> Option<&Transition> {
        self.transitions
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, t)| t)
    }

    pub fn condition(&self, name: &str) -> Option<&Condition> {
        self.conditions
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c)
    }

    /// The attribute that holds an actor's identity, if this type's objects
    /// are actors, and the kind of actor they are (`DESIGN.md` §6 step 1).
    pub fn actor_identity(&self) -> Option<(&str, &str)> {
        self.attributes
            .iter()
            .find_map(|(n, a)| a.actor_kind.as_deref().map(|k| (n.as_str(), k)))
    }

    /// Whether a state is closed or final, so the object is finished in it.
    pub fn finished(&self, state: &str) -> bool {
        self.state(state)
            .is_some_and(|s| s.is_final || s.category == "closed")
    }
}

/// A module as loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub name: String,
    pub enumerations: Vec<(String, Vec<String>)>,
    pub types: Vec<Type>,
}

/// Why a module does not load.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadError {
    pub file: String,
    pub line: usize,
    pub message: String,
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{} {}", self.file, self.line, self.message)
    }
}

impl std::error::Error for LoadError {}

struct Reader<'a> {
    file: &'a str,
}

type Loaded<T> = Result<T, LoadError>;

impl Reader<'_> {
    fn error<T>(&self, node: &Node, message: impl Into<String>) -> Loaded<T> {
        Err(LoadError {
            file: self.file.to_string(),
            line: node.line(),
            message: message.into(),
        })
    }

    fn text(&self, node: &Node, what: &str) -> Loaded<String> {
        match node.text() {
            Some(t) => Ok(t.to_string()),
            None => self.error(node, format!("{what} is a scalar")),
        }
    }

    fn flag(&self, map: &Node, key: &str) -> Loaded<bool> {
        match map.get(key) {
            None => Ok(false),
            Some(n) => match n.boolean() {
                Some(b) => Ok(b),
                None => self.error(n, format!("{key} is true or false")),
            },
        }
    }

    fn names(&self, node: Option<&Node>) -> Loaded<Vec<String>> {
        match node {
            None => Ok(Vec::new()),
            Some(n @ Node::Scalar { .. }) if !n.is_null() => Ok(vec![self.text(n, "a name")?]),
            Some(Node::Seq { items, .. }) => items.iter().map(|i| self.text(i, "a name")).collect(),
            Some(n) if n.is_null() => Ok(Vec::new()),
            Some(n) => self.error(n, "a list of names"),
        }
    }

    /// An expression where the schema puts one: a YAML boolean or number
    /// stands for its literal (`flow-format.md` §5), anything else is text.
    fn expression(&self, node: &Node) -> Loaded<Expr> {
        if let Some(b) = node.boolean() {
            return Ok(Expr::Bool(b));
        }
        if let Some(n) = node.integer() {
            return Ok(Expr::Num(Number::Int(n)));
        }
        let text = self.text(node, "an expression")?;
        match expr::parse(&text) {
            Ok(e) => Ok(e),
            Err(e) => self.error(
                node,
                format!("{e}: not an expression of declaration-syntax.md §8"),
            ),
        }
    }

    fn declared(&self, node: &Node) -> Loaded<(Declared, bool)> {
        let (text, reference) = match (node.get("type"), node.get("reference")) {
            (Some(t), None) => (self.text(t, "a type")?, false),
            (None, Some(r)) => (self.text(r, "a reference")?, true),
            _ => return self.error(node, "exactly one of type and reference"),
        };
        let many = text.ends_with("[]");
        let base = text.trim_end_matches("[]").to_string();
        Ok((
            if reference {
                Declared::Reference(base)
            } else {
                Declared::Scalar(base)
            },
            many,
        ))
    }

    fn attribute(&self, node: &Node) -> Loaded<Attribute> {
        let (declared, many) = self.declared(node)?;
        Ok(Attribute {
            declared,
            many,
            optional: self.flag(node, "optional")?,
            default: node
                .get("default")
                .map(|d| self.expression(d))
                .transpose()?,
            unique: match node.get("unique") {
                None => Unique::No,
                Some(u) if u.text() == Some("in_scope") => Unique::InScope,
                Some(u @ Node::Map { .. }) => match (u.get("with"), u.get("where")) {
                    (Some(w), None) => Unique::With(self.names(Some(w))?),
                    (None, Some(e)) => Unique::Where(self.expression(e)?),
                    _ => return self.error(u, "unique is with a list, or where a condition"),
                },
                Some(u) => match u.boolean() {
                    Some(true) => Unique::Yes,
                    Some(false) => Unique::No,
                    None => return self.error(u, "unique is true, false, in_scope, with or where"),
                },
            },
            assignee: self.flag(node, "assignee")?,
            personal: self.flag(node, "personal")?,
            actor_kind: node
                .get("actor_kind")
                .map(|k| self.text(k, "a kind of actor"))
                .transpose()?,
            minted: node.get("identifier").is_some(),
            opposite: node
                .get("opposite")
                .map(|o| self.text(o, "an opposite end"))
                .transpose()?,
        })
    }

    fn input(&self, node: &Node) -> Loaded<Input> {
        let (declared, many) = self.declared(node)?;
        Ok(Input {
            declared,
            many,
            optional: self.flag(node, "optional")?,
            default: node
                .get("default")
                .map(|d| self.expression(d))
                .transpose()?,
        })
    }

    fn transition(&self, node: &Node) -> Loaded<Transition> {
        let kind = match node.get("kind").and_then(Node::text) {
            Some("initial") => Kind::Initial,
            Some("external") => Kind::External,
            Some("internal") => Kind::Internal,
            Some("assertion") => Kind::Assertion,
            Some("erasure") => Kind::Erasure,
            _ => {
                return self.error(
                    node,
                    "a transition's kind is initial, external, internal, assertion or erasure",
                );
            }
        };
        let from = match node.get("from") {
            None => None,
            Some(f) if f.text() == Some("any") => Some(From::Any),
            Some(f) => Some(From::States(self.names(Some(f))?)),
        };
        let mut guards = Vec::new();
        for (name, mode) in node.get("guards").map(Node::entries).unwrap_or_default() {
            let enforce = match mode.text() {
                Some("deny") => Enforce::Deny,
                Some("audit") => Enforce::Audit,
                Some("warn") => Enforce::Warn,
                _ => return self.error(mode, "a guard is enforced as deny, audit or warn"),
            };
            guards.push((name.clone(), enforce));
        }
        let mut inputs = Vec::new();
        for (name, spec) in node.get("inputs").map(Node::entries).unwrap_or_default() {
            inputs.push((name.clone(), self.input(spec)?));
        }
        Ok(Transition {
            kind,
            from,
            to: self.names(node.get("to"))?,
            only_via: self.names(node.get("only_via"))?,
            proposable: self.flag(node, "proposable")?,
            required_inputs: self.names(node.get("required_inputs"))?,
            optional_inputs: self.names(node.get("optional_inputs"))?,
            inputs,
            guards,
            has_effect: node.get("effect").is_some_and(|e| !e.items().is_empty()),
            backdating_limit: node
                .get("backdating_limit")
                .map(|b| self.text(b, "a duration"))
                .transpose()?,
        })
    }

    fn type_(&self, module: &str, name: &str, node: &Node) -> Loaded<Type> {
        let mut beyond = Vec::new();
        for (key, what) in [
            ("state_machine", "a machine the type binds"),
            ("extends", "a family, the type extending a base"),
            ("abstract", "an abstract type"),
            ("mirror", "a mirror type, owned elsewhere"),
            ("inherited_parts", "parts a family inherits"),
        ] {
            if node.get(key).is_some_and(|v| v.boolean() != Some(false)) {
                beyond.push(format!("{what} ({name}.{key})"));
            }
        }
        let mut attributes = Vec::new();
        for (n, spec) in node
            .get("attributes")
            .map(Node::entries)
            .unwrap_or_default()
        {
            let a = self.attribute(spec)?;
            if spec.get("cascade").is_some() {
                beyond.push(format!("a cascade to the parts ({name}.{n})"));
            }
            if a.minted {
                beyond.push(format!("an identifier minted from a sequence ({name}.{n})"));
            }
            attributes.push((n.clone(), a));
        }
        let mut derived = Vec::new();
        for (n, spec) in node
            .get("derived_attributes")
            .map(Node::entries)
            .unwrap_or_default()
        {
            match spec.get("expression") {
                Some(e) => derived.push((n.clone(), self.expression(e)?)),
                None => return self.error(spec, "a derived attribute has an expression"),
            }
        }
        let mut states = Vec::new();
        for (n, spec) in node.get("states").map(Node::entries).unwrap_or_default() {
            if spec.get("superseding").is_some() {
                beyond.push(format!("supersession ({name}.{n})"));
            }
            states.push((
                n.clone(),
                State {
                    category: spec
                        .get("category")
                        .and_then(Node::text)
                        .unwrap_or("")
                        .to_string(),
                    is_final: self.flag(spec, "final")?,
                    required_attributes: self.names(spec.get("required_attributes"))?,
                },
            ));
        }
        let mut conditions = Vec::new();
        for (n, spec) in node
            .get("conditions")
            .map(Node::entries)
            .unwrap_or_default()
        {
            let Some(e) = spec.get("expression") else {
                return self.error(spec, "a condition has an expression");
            };
            conditions.push((
                n.clone(),
                Condition {
                    expression: self.expression(e)?,
                    remedy: spec
                        .get("remedy")
                        .and_then(Node::text)
                        .and_then(Remedy::parse),
                },
            ));
        }
        let mut invariants = Vec::new();
        for (n, spec) in node
            .get("invariants")
            .map(Node::entries)
            .unwrap_or_default()
        {
            let Some(e) = spec.get("expression") else {
                return self.error(spec, "an invariant has an expression");
            };
            invariants.push((n.clone(), self.expression(e)?));
        }
        let mut transitions = Vec::new();
        for (n, spec) in node
            .get("transitions")
            .map(Node::entries)
            .unwrap_or_default()
        {
            transitions.push((n.clone(), self.transition(spec)?));
        }
        Ok(Type {
            name: name.to_string(),
            module: module.to_string(),
            attributes,
            derived,
            states,
            conditions,
            invariants,
            transitions,
            beyond,
        })
    }

    fn module(&self, doc: &Node) -> Loaded<Module> {
        let Some(name) = doc.get("module").and_then(Node::text) else {
            return self.error(doc, "a description names its module");
        };
        let mut enumerations = Vec::new();
        for (n, members) in doc
            .get("enumerations")
            .map(Node::entries)
            .unwrap_or_default()
        {
            enumerations.push((n.clone(), self.names(Some(members))?));
        }
        let mut types = Vec::new();
        for (n, spec) in doc.get("types").map(Node::entries).unwrap_or_default() {
            types.push(self.type_(name, n, spec)?);
        }
        Ok(Module {
            name: name.to_string(),
            enumerations,
            types,
        })
    }
}

/// A module read from its description.
pub fn load_module(file: &str, text: &str) -> Result<Module, LoadError> {
    let doc = yaml::load(text).map_err(|e| LoadError {
        file: file.to_string(),
        line: e.line,
        message: e.message,
    })?;
    Reader { file }.module(&doc)
}

/// The modules a store runs, read in import order: every type and every
/// enumeration they declare, by name, a name declared once across them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Library {
    pub types: BTreeMap<String, Type>,
    pub enumerations: BTreeMap<String, Vec<String>>,
}

impl Library {
    /// The library of modules given as (file, text), in import order.
    pub fn load(files: &[(String, String)]) -> Result<Library, LoadError> {
        let mut lib = Library::default();
        for (file, text) in files {
            let m = load_module(file, text)?;
            for (n, members) in m.enumerations {
                lib.enumerations.insert(n, members);
            }
            for t in m.types {
                lib.types.insert(t.name.clone(), t);
            }
        }
        Ok(lib)
    }

    pub fn type_(&self, name: &str) -> Option<&Type> {
        self.types.get(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const JOBS: &str = r#"
module: jobs
categories: [inbound, live, closed]
enumerations:
  Priority: [LOW, HIGH]
types:
  Job:
    description: A job.
    attributes:
      title: { type: string }
      priority: { type: Priority, optional: true }
      engineer: { reference: User, optional: true, assignee: true }
    states:
      OPEN: { category: inbound, required_attributes: [title] }
      DONE: { category: closed, final: true }
    conditions:
      titled: { expression: title is not null, remedy: self_serviceable }
    invariants:
      short: { expression: "length(title) <= 80" }
    transitions:
      open: { kind: initial, to: OPEN, required_inputs: [title], optional_inputs: [priority] }
      close: { kind: external, from: [OPEN], to: DONE, guards: { titled: deny } }
      retitle: { kind: internal, from: any, required_inputs: [title] }
"#;

    #[test]
    fn a_module_loads_with_its_rules_parsed() {
        let m = load_module("jobs.yaml", JOBS).unwrap();
        assert_eq!(m.name, "jobs");
        let job = &m.types[0];
        assert_eq!(job.name, "Job");
        assert!(job.beyond.is_empty());
        assert_eq!(
            job.attribute("engineer").unwrap().declared,
            Declared::Reference("User".into())
        );
        assert!(job.attribute("engineer").unwrap().assignee);
        assert_eq!(job.state("OPEN").unwrap().required_attributes, ["title"]);
        assert!(job.finished("DONE") && !job.finished("OPEN"));
        let close = job.transition("close").unwrap();
        assert_eq!(close.kind, Kind::External);
        assert_eq!(close.from, Some(From::States(vec!["OPEN".into()])));
        assert_eq!(close.guards, [("titled".to_string(), Enforce::Deny)]);
        assert_eq!(job.transition("retitle").unwrap().from, Some(From::Any));
        assert_eq!(
            job.condition("titled").unwrap().remedy,
            Some(Remedy::SelfServiceable)
        );
    }

    #[test]
    fn what_the_slice_does_not_run_is_named_not_dropped() {
        let text = JOBS.replace(
            "    description: A job.\n",
            "    description: A job.\n    extends: Base\n",
        );
        let m = load_module("jobs.yaml", &text).unwrap();
        assert_eq!(m.types[0].beyond.len(), 1);
        assert!(m.types[0].beyond[0].contains("Job.extends"));
    }

    #[test]
    fn a_bad_expression_is_refused_with_its_line() {
        let text = JOBS.replace("title is not null", "title is not");
        let err = load_module("jobs.yaml", &text).unwrap_err();
        assert_eq!(err.line, 17);
        assert!(err.message.contains("not an expression"));
    }
}
