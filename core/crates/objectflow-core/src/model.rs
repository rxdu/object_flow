//! A flow module as the core holds it, read from its description
//! (`flow-format.md` §2 to §4) by the strict loader.
//!
//! The model keeps everything the first slice runs, each expression parsed
//! once, and names everything it does not (ADR-0153 decision 3): a type that
//! binds a machine or extends a base, a transition with an effect. A request that reaches one is answered
//! as not run, naming it, and never guessed. Whether a module is well formed
//! is the flow checker's to say, in this slice (ADR-0153 decision 5); the
//! loader refuses only what it cannot read.

use std::collections::{BTreeMap, BTreeSet};
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

/// An identifier minted from a sequence (`flow-format.md` §4.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identifier {
    pub sequence: String,
    /// The reference or attribute whose every value the sequence numbers
    /// separately.
    pub scope: Option<String>,
    /// The text minted: `{n}` the number, `{n:6}` zero-padded, `{a.b}` a
    /// value of the object, `[ … ]` a segment left out whole when a value in
    /// it is absent.
    pub format: String,
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
    /// The identifier minted for it from a sequence, when its object is
    /// created (`flow-format.md` §4.12).
    pub identifier: Option<Identifier>,
    /// A stored relationship end the other side owns or derives.
    pub opposite: Option<String>,
    /// `composite`: its objects are parts of this one, for no longer.
    pub composite: bool,
    /// An external identifier: the source whose value it holds (§4.15).
    pub external: Option<String>,
    /// Whether a query may filter on it, which an index on its column
    /// serves (`flow-format.md` §4.3, `storage-schema.md` §10, D593).
    pub indexed: bool,
    /// On a composite end, `survives: true`: its parts outlive any final state
    /// of the whole, a migration's included (`publish-and-import.md` §3).
    pub survives_always: bool,
    /// On a composite end, the transitions its parts take when this object
    /// takes one of those listed.
    pub cascade: Vec<CascadeClause>,
}

/// The transition each part takes when its whole takes one of `on`.
#[derive(Clone, Debug, PartialEq)]
pub struct CascadeClause {
    pub on: Vec<String>,
    pub transition: String,
    pub inputs: Vec<(String, Expr)>,
    pub limit: Option<i64>,
}

/// One write of an effect (`flow.schema.json`, `step`).
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Assign {
        location: String,
        expr: Expr,
    },
    Clear(Vec<String>),
    Add {
        location: String,
        expr: Expr,
    },
    Remove {
        location: String,
        expr: Expr,
    },
    Create {
        type_: String,
        transition: String,
        inputs: Vec<(String, Expr)>,
        result: Option<String>,
    },
    Call {
        target: Expr,
        transition: String,
        inputs: Vec<(String, Expr)>,
    },
    Foreach {
        item: String,
        array: Option<Expr>,
        range: Option<Expr>,
        filter: Option<Expr>,
        limit: i64,
        steps: Vec<Step>,
    },
    Supersede(Expr),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub declared: Declared,
    pub many: bool,
    pub optional: bool,
    pub default: Option<Expr>,
    /// A personal value, which the attempt log names and withholds
    /// (ADR-0105).
    pub personal: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub category: String,
    pub is_final: bool,
    pub required_attributes: Vec<String>,
    /// An object in it is superseded: another stands in its place (ADR-0028).
    pub superseding: bool,
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
    /// What it does, in the author's words, which a tool's description
    /// leads with (`renderers.md` §3).
    pub description: Option<String>,
    pub from: Option<From>,
    pub to: Vec<String>,
    pub only_via: Vec<String>,
    pub proposable: bool,
    pub required_inputs: Vec<String>,
    pub optional_inputs: Vec<String>,
    pub inputs: Vec<(String, Input)>,
    pub guards: Vec<(String, Enforce)>,
    /// The writes it makes, in order (`flow-format.md` §4.8).
    pub effect: Vec<Step>,
    pub backdating_limit: Option<String>,
    /// On an assertion, the invariants a request may admit breaking (§4.13).
    pub may_admit: Vec<String>,
    /// On a correction, the attributes it corrects, whose events carry the
    /// `corrected` provenance (§4.13).
    pub corrects: Vec<String>,
    /// The kinds of actor that may take it, from which the load generates
    /// the record of who took it and the invariant over it (ADR-0176).
    pub taken_by: Vec<String>,
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
    /// What this type's structure holds that the first slice's model does
    /// not: a request on it is not run, naming the first (ADR-0153 decision
    /// 3).
    pub beyond: Vec<String>,
    /// What only an applying request reaches that the first slice does not
    /// run: its refusals are still decided, and an outcome is not run.
    pub at_outcome: Vec<String>,
    /// The observation kinds recorded on it: each collection's name and the
    /// kind it holds, in the order declared (`flow-format.md` §4.4).
    pub observations: Vec<(String, String)>,
    /// For an observation kind, the type of its subject (`declaration-syntax.md`
    /// §6.8); none for any other type.
    pub observation_on: Option<String>,
    /// The shared machine it binds, whose states, conditions and
    /// transitions the library merges into it (`flow-format.md` §4.10).
    pub state_machine: Option<String>,
    /// The base it extends, whose attributes, derived attributes and
    /// invariants it inherits (`declaration-syntax.md` §2).
    pub extends: Option<String>,
    /// Another system owns it, and only the import writes it (§4.15).
    pub mirror: bool,
    /// A base with no objects of its own.
    pub abstract_: bool,
    /// Dropped by a later version: its objects and history stay readable,
    /// and nothing creates or changes one (`storage-schema.md` §10).
    pub retired: bool,
    /// Each inherited attribute, and the base that declares it.
    pub inherited: BTreeMap<String, String>,
    /// The type as its description writes it, which `declaration` returns.
    pub source: serde_json::Value,
}

impl Library {
    /// The members whose every value is timed: the state, each enumeration
    /// attribute, and each singular reference the object stores (PRD D11,
    /// `DESIGN.md` §7).
    pub fn tracked(&self, t: &Type) -> Vec<String> {
        let mut out = vec!["state".to_string()];
        for (n, a) in &t.attributes {
            if a.many {
                continue;
            }
            let tracked = match &a.declared {
                Declared::Scalar(s) => self.enumerations.contains_key(s.as_str()),
                Declared::Reference(target) => a.opposite.as_ref().is_none_or(|o| {
                    self.type_(target)
                        .and_then(|tt| tt.attribute(o))
                        .is_some_and(|back| back.many)
                }),
            };
            if tracked {
                out.push(n.clone());
            }
        }
        out
    }
}

impl Attribute {
    /// Whether the object stores it as a set: a set of values, or of
    /// references with no opposite end to compute it from (§3.3).
    pub fn stored_set(&self) -> bool {
        self.many && !(matches!(self.declared, Declared::Reference(_)) && self.opposite.is_some())
    }

    /// Whether it has no column and no table: a set of references computed
    /// from the opposite end that stores them (§3.1, the derived end).
    pub fn derived_end(&self) -> bool {
        self.many && matches!(self.declared, Declared::Reference(_)) && self.opposite.is_some()
    }
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
    /// Its `migration`: what a publish of it maps (`flow-format.md` §4.16).
    pub migration: Migration,
    pub enumerations: Vec<(String, Vec<String>)>,
    pub types: Vec<Type>,
    /// The observation kinds its types declare, by name.
    pub observation_kinds: Vec<String>,
    /// The shared state machines it declares.
    pub machines: Vec<Type>,
    /// Its evaluators' functions, as `<evaluator>.<function>`.
    pub evaluators: Vec<String>,
    /// Its metrics, its types' and its own, which share one namespace: each
    /// by name, with the type declaring it, none for a combined one, and its
    /// declaration (`flow-format.md` §4.9).
    pub metrics: Vec<(String, Option<String>, serde_json::Value)>,
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
            external: node
                .get("external")
                .and_then(Node::text)
                .map(str::to_string),
            indexed: self.flag(node, "indexed")?,
            survives_always: node.get("survives").and_then(Node::boolean) == Some(true),
            actor_kind: node
                .get("actor_kind")
                .map(|k| self.text(k, "a kind of actor"))
                .transpose()?,
            identifier: match node.get("identifier") {
                None => None,
                Some(i) => Some(Identifier {
                    sequence: match i.get("sequence").and_then(Node::text) {
                        Some(s) => s.to_string(),
                        None => return self.error(i, "an identifier names its sequence"),
                    },
                    scope: i.get("scope").and_then(Node::text).map(str::to_string),
                    format: i
                        .get("format")
                        .and_then(Node::text)
                        .unwrap_or("{n}")
                        .to_string(),
                }),
            },
            opposite: node
                .get("opposite")
                .map(|o| self.text(o, "an opposite end"))
                .transpose()?,
            composite: node.get("aggregation").and_then(Node::text) == Some("composite"),
            cascade: {
                let mut clauses = Vec::new();
                for c in node.get("cascade").map(Node::items).unwrap_or_default() {
                    clauses.push(CascadeClause {
                        on: self.names(c.get("on"))?,
                        transition: match c.get("transition").and_then(Node::text) {
                            Some(t) => t.to_string(),
                            None => {
                                return self
                                    .error(c, "a cascade names the transition its parts take");
                            }
                        },
                        inputs: self.bindings(c.get("inputs"))?,
                        limit: c.get("limit").and_then(Node::integer),
                    });
                }
                clauses
            },
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
            personal: self.flag(node, "personal")?,
        })
    }

    /// Named expressions: a call's, a creation's or a cascade's inputs.
    fn bindings(&self, node: Option<&Node>) -> Loaded<Vec<(String, Expr)>> {
        let mut out = Vec::new();
        for (k, e) in node.map(Node::entries).unwrap_or_default() {
            out.push((k.clone(), self.expression(e)?));
        }
        Ok(out)
    }

    fn located(&self, node: &Node) -> Loaded<(String, Expr)> {
        match (node.get("location").and_then(Node::text), node.get("expr")) {
            (Some(l), Some(e)) => Ok((l.to_string(), self.expression(e)?)),
            _ => self.error(node, "a write names its location and its expr"),
        }
    }

    fn steps(&self, node: Option<&Node>) -> Loaded<Vec<Step>> {
        let mut out = Vec::new();
        for st in node.map(Node::items).unwrap_or_default() {
            let Some((kind, spec)) = st.entries().first() else {
                return self.error(
                    st,
                    "a step is one of assign, clear, add, remove, create, call, foreach, supersede",
                );
            };
            out.push(match kind.as_str() {
                "assign" => {
                    let (location, expr) = self.located(spec)?;
                    Step::Assign { location, expr }
                }
                "add" => {
                    let (location, expr) = self.located(spec)?;
                    Step::Add { location, expr }
                }
                "remove" => {
                    let (location, expr) = self.located(spec)?;
                    Step::Remove { location, expr }
                }
                "clear" => Step::Clear(self.names(Some(spec))?),
                "create" => Step::Create {
                    type_: match spec.get("type").and_then(Node::text) {
                        Some(t) => t.to_string(),
                        None => return self.error(spec, "a creation names its type"),
                    },
                    transition: spec
                        .get("transition")
                        .and_then(Node::text)
                        .unwrap_or_default()
                        .to_string(),
                    inputs: self.bindings(spec.get("inputs"))?,
                    result: spec.get("result").and_then(Node::text).map(str::to_string),
                },
                "call" => Step::Call {
                    target: match spec.get("target") {
                        Some(t) => self.expression(t)?,
                        None => return self.error(spec, "a call names its target"),
                    },
                    transition: spec
                        .get("transition")
                        .and_then(Node::text)
                        .unwrap_or_default()
                        .to_string(),
                    inputs: self.bindings(spec.get("inputs"))?,
                },
                "foreach" => Step::Foreach {
                    item: spec
                        .get("item")
                        .and_then(Node::text)
                        .unwrap_or_default()
                        .to_string(),
                    array: spec.get("array").map(|e| self.expression(e)).transpose()?,
                    range: spec.get("range").map(|e| self.expression(e)).transpose()?,
                    filter: spec.get("where").map(|e| self.expression(e)).transpose()?,
                    limit: spec.get("limit").and_then(Node::integer).unwrap_or(0),
                    steps: self.steps(spec.get("steps"))?,
                },
                "supersede" => Step::Supersede(self.expression(spec)?),
                other => return self.error(st, format!("the effect step `{other}`")),
            });
        }
        Ok(out)
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
            description: node
                .get("description")
                .and_then(Node::text)
                .map(str::to_string),
            from,
            to: self.names(node.get("to"))?,
            only_via: self.names(node.get("only_via"))?,
            proposable: self.flag(node, "proposable")?,
            required_inputs: self.names(node.get("required_inputs"))?,
            optional_inputs: self.names(node.get("optional_inputs"))?,
            inputs,
            guards,
            effect: self.steps(node.get("effect"))?,
            backdating_limit: node
                .get("backdating_limit")
                .map(|b| self.text(b, "a duration"))
                .transpose()?,
            may_admit: self.names(node.get("may_admit"))?,
            corrects: self.names(node.get("corrects"))?,
            taken_by: self.names(node.get("taken_by"))?,
        })
    }

    fn type_(&self, module: &str, name: &str, node: &Node) -> Loaded<Type> {
        let mut beyond = Vec::new();
        let at_outcome = Vec::new();
        if node
            .get("inherited_parts")
            .is_some_and(|v| v.boolean() != Some(false))
        {
            beyond.push(format!("parts a family inherits ({name}.inherited_parts)"));
        }
        let mut attributes = Vec::new();
        for (n, spec) in node
            .get("attributes")
            .map(Node::entries)
            .unwrap_or_default()
        {
            let a = self.attribute(spec)?;
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
                    superseding: self.flag(spec, "superseding")?,
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
            at_outcome,
            observations: node
                .get("observations")
                .map(Node::entries)
                .unwrap_or_default()
                .iter()
                .filter_map(|(c, ob)| {
                    ob.get("kind")
                        .and_then(Node::text)
                        .map(|k| (c.clone(), k.to_string()))
                })
                .collect(),
            observation_on: None,
            state_machine: node
                .get("state_machine")
                .and_then(Node::text)
                .map(str::to_string),
            extends: node.get("extends").and_then(Node::text).map(str::to_string),
            mirror: self.flag(node, "mirror")?,
            abstract_: self.flag(node, "abstract")?,
            retired: false,
            inherited: BTreeMap::new(),
            source: node.to_json(),
        })
    }

    /// An observation kind as the type it is sugar for (`declaration-syntax.md`
    /// §6.8): its fields, `subject`, an optional `corrects`, and the times it
    /// was recorded and occurred; one state, `RECORDED`, closed and final; a
    /// creation, `record`, taking the subject and the fields; and, for a kind
    /// with a personal field, an erasure, `forget`.
    fn kind_type(&self, module: &str, subject: &str, kind: &str, ob: &Node) -> Loaded<Type> {
        let plain = |declared: Declared, optional: bool| Attribute {
            declared,
            many: false,
            optional,
            default: None,
            unique: Unique::No,
            assignee: false,
            personal: false,
            actor_kind: None,
            identifier: None,
            opposite: None,
            composite: false,
            cascade: Vec::new(),
            external: None,
            indexed: false,
            survives_always: false,
        };
        let mut attributes = Vec::new();
        let mut required = vec!["subject".to_string()];
        let mut optional = vec!["corrects".to_string()];
        let mut personal = false;
        for (n, spec) in ob.get("attributes").map(Node::entries).unwrap_or_default() {
            let a = self.attribute(spec)?;
            if a.optional {
                &mut optional
            } else {
                &mut required
            }
            .push(n.clone());
            personal |= a.personal;
            attributes.push((n.clone(), a));
        }
        attributes.push((
            "subject".into(),
            plain(Declared::Reference(subject.into()), false),
        ));
        attributes.push((
            "corrects".into(),
            plain(Declared::Reference(kind.into()), true),
        ));
        for at in ["recorded_at", "occurred_at"] {
            attributes.push((
                at.into(),
                plain(Declared::Scalar("timestamp".into()), false),
            ));
        }
        let mut invariants = Vec::new();
        for (n, spec) in ob.get("invariants").map(Node::entries).unwrap_or_default() {
            let Some(e) = spec.get("expression") else {
                return self.error(spec, "an invariant has an expression");
            };
            invariants.push((n.clone(), self.expression(e)?));
        }
        let limit = ob
            .get("max_recording_delay")
            .map(|b| self.text(b, "a duration"))
            .transpose()?;
        let mut transitions = vec![(
            "record".to_string(),
            Transition {
                description: None,
                kind: Kind::Initial,
                from: None,
                to: vec!["RECORDED".into()],
                only_via: Vec::new(),
                proposable: false,
                required_inputs: required.clone(),
                optional_inputs: optional.clone(),
                inputs: Vec::new(),
                guards: Vec::new(),
                effect: Vec::new(),
                backdating_limit: limit.clone(),
                may_admit: Vec::new(),
                corrects: Vec::new(),
                taken_by: Vec::new(),
            },
        )];
        let mut source_transitions = serde_json::json!({
            "record": {
                "kind": "initial", "to": "RECORDED",
                "required_inputs": required, "optional_inputs": optional,
            }
        });
        if let Some(l) = &limit {
            source_transitions["record"]["backdating_limit"] = serde_json::json!(l);
        }
        if personal {
            let reason = Input {
                declared: Declared::Scalar("string".into()),
                many: false,
                optional: false,
                default: None,
                personal: false,
            };
            transitions.push((
                "forget".into(),
                Transition {
                    description: None,
                    kind: Kind::Erasure,
                    from: None,
                    to: Vec::new(),
                    only_via: Vec::new(),
                    proposable: false,
                    required_inputs: Vec::new(),
                    optional_inputs: Vec::new(),
                    inputs: vec![("reason".into(), reason)],
                    guards: Vec::new(),
                    effect: Vec::new(),
                    backdating_limit: None,
                    may_admit: Vec::new(),
                    corrects: Vec::new(),
                    taken_by: Vec::new(),
                },
            ));
            source_transitions["forget"] = serde_json::json!({ "kind": "erasure", "inputs": { "reason": { "type": "string" } } });
        }
        // the declaration as the reference runner serves it: the kind's
        // expansion, in the flow format's shape
        let mut source_attributes = ob
            .get("attributes")
            .map_or(serde_json::json!({}), Node::to_json);
        source_attributes["subject"] = serde_json::json!({ "reference": subject });
        source_attributes["corrects"] = serde_json::json!({ "reference": kind, "optional": true });
        source_attributes["recorded_at"] = serde_json::json!({ "type": "timestamp" });
        source_attributes["occurred_at"] = serde_json::json!({ "type": "timestamp" });
        let mut source = serde_json::json!({
            "attributes": source_attributes,
            "states": { "RECORDED": { "category": "closed", "final": true } },
            "invariants": ob.get("invariants").map_or(serde_json::json!({}), Node::to_json),
            "transitions": source_transitions,
        });
        if let Some(d) = ob.get("description").map(Node::to_json) {
            source["description"] = d;
        }
        Ok(Type {
            name: kind.to_string(),
            module: module.to_string(),
            attributes,
            derived: Vec::new(),
            states: vec![(
                "RECORDED".into(),
                State {
                    category: "closed".into(),
                    is_final: true,
                    required_attributes: Vec::new(),
                    superseding: false,
                },
            )],
            conditions: Vec::new(),
            invariants,
            transitions,
            beyond: Vec::new(),
            at_outcome: Vec::new(),
            observations: Vec::new(),
            observation_on: Some(subject.to_string()),
            state_machine: None,
            extends: None,
            mirror: false,
            abstract_: false,
            retired: false,
            inherited: BTreeMap::new(),
            source,
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
        let mut observation_kinds = Vec::new();
        let mut kinds = Vec::new();
        let mut metrics = Vec::new();
        for (n, spec) in doc.get("types").map(Node::entries).unwrap_or_default() {
            types.push(self.type_(name, n, spec)?);
            for (m, declared) in spec.get("metrics").map(Node::entries).unwrap_or_default() {
                metrics.push((m.clone(), Some(n.clone()), declared.to_json()));
            }
            for (_, ob) in spec
                .get("observations")
                .map(Node::entries)
                .unwrap_or_default()
            {
                let Some(kind) = ob.get("kind").and_then(Node::text) else {
                    return self.error(ob, "an observation names its kind");
                };
                observation_kinds.push(kind.to_string());
                kinds.push(self.kind_type(name, n, kind, ob)?);
            }
        }
        types.extend(kinds);
        // a shared machine reads as a type whose attributes are those it
        // requires, which its binders declare (flow-format.md §4.10)
        let mut machines = Vec::new();
        for (n, spec) in doc.get("machines").map(Node::entries).unwrap_or_default() {
            machines.push(self.type_(name, n, spec)?);
        }
        let mut evaluators = Vec::new();
        for (e, spec) in doc.get("evaluators").map(Node::entries).unwrap_or_default() {
            for (f, _) in spec.get("functions").map(Node::entries).unwrap_or_default() {
                evaluators.push(format!("{e}.{f}"));
            }
        }
        for (m, declared) in doc.get("metrics").map(Node::entries).unwrap_or_default() {
            metrics.push((m.clone(), None, declared.to_json()));
        }
        Ok(Module {
            name: name.to_string(),
            migration: self.migration(doc.get("migration"))?,
            enumerations,
            types,
            observation_kinds,
            machines,
            evaluators,
            metrics,
        })
    }
}

/// A module's `migration`: the mappings a publish of it applies, each naming
/// its type or enumeration (`publish-and-import.md` §3, `flow-format.md`
/// §4.16).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Migration {
    /// A type, a state it removes, and the state its objects move to.
    pub removed_states: Vec<(String, String, String)>,
    /// An enumeration, a member it removes, and the member it maps to.
    pub removed_members: Vec<(String, String, String)>,
    /// A type, an attribute's old name, and its new one.
    pub renamed_attributes: Vec<(String, String, String)>,
    /// A type, an attribute, and the value written to it, in the order
    /// written.
    pub backfill: Vec<(String, String, Expr)>,
    /// An invariant, `Type.name`, a violation of which is admitted, and the
    /// reason, an enumeration member written `Enum.MEMBER`.
    pub admit: Vec<(String, String)>,
}

impl Migration {
    fn extend(&mut self, other: Migration) {
        self.removed_states.extend(other.removed_states);
        self.removed_members.extend(other.removed_members);
        self.renamed_attributes.extend(other.renamed_attributes);
        self.backfill.extend(other.backfill);
        self.admit.extend(other.admit);
    }

    pub fn is_empty(&self) -> bool {
        *self == Migration::default()
    }
}

impl Reader<'_> {
    fn migration(&self, node: Option<&Node>) -> Loaded<Migration> {
        let mut m = Migration::default();
        let Some(node) = node else {
            return Ok(m);
        };
        let pairs = |key: &str| -> Vec<(String, String, String)> {
            node.get(key)
                .map(Node::entries)
                .unwrap_or_default()
                .iter()
                .flat_map(|(owner, map)| {
                    map.entries()
                        .iter()
                        .filter_map(|(old, new)| {
                            new.text()
                                .map(|n| (owner.clone(), old.clone(), n.to_string()))
                        })
                        .collect::<Vec<_>>()
                })
                .collect()
        };
        m.removed_states = pairs("removed_states");
        m.removed_members = pairs("removed_members");
        m.renamed_attributes = pairs("renamed_attributes");
        for (tn, map) in node.get("backfill").map(Node::entries).unwrap_or_default() {
            for (a, e) in map.entries() {
                m.backfill
                    .push((tn.clone(), a.clone(), self.expression(e)?));
            }
        }
        for item in node.get("admit").map(Node::items).unwrap_or_default() {
            let (Some(inv), Some(reason)) = (
                item.get("invariant").and_then(Node::text),
                item.get("reason").and_then(Node::text),
            ) else {
                return self.error(item, "an admit names its invariant and its reason");
            };
            m.admit.push((inv.to_string(), reason.to_string()));
        }
        Ok(m)
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
    /// The observation kinds the types declare, each also a type of its own.
    pub observation_kinds: BTreeSet<String>,
    /// The evaluators' functions the modules declare, in the order loaded.
    pub evaluators: Vec<String>,
    /// The modules loaded, in import order.
    pub modules: Vec<String>,
    /// The types the modules declare, in the order read: neither the
    /// built-ins every store holds nor the observation kinds, which a
    /// recording creates. What a sandbox's load reports it installed.
    pub declared: Vec<String>,
    /// The declaration version it is installed as (`storage-schema.md` §5).
    pub version: i64,
    /// Its source: each module's description by its name, and each examples
    /// file by `<module>.examples`, in the order given (ADR-0135, ADR-0136).
    pub source: Vec<(String, String)>,
    /// The migrations its modules carry, which a publish of it applies.
    pub migration: Migration,
    /// Each module's metrics, its types' and its own, by name: the type
    /// declaring one, none for a combined one, and its declaration
    /// (`flow-format.md` §4.9).
    pub metrics: BTreeMap<String, BTreeMap<String, (Option<String>, serde_json::Value)>>,
}

/// The line a name is declared on in a module's text, a key indented once
/// under its section; the first line where none is found.
fn declared_line(text: &str, name: &str) -> usize {
    let key = format!("  {name}:");
    text.lines()
        .position(|l| l.starts_with(&key) && !l.starts_with("   "))
        .map_or(1, |i| i + 1)
}

/// The attribute and the invariant a transition's `taken_by` generates.
pub fn taken_by_names(transition: &str) -> (String, String) {
    (
        format!("{transition}_by_kind"),
        format!("{transition}_taken_by"),
    )
}

/// A type with what its transitions' `taken_by` generate (ADR-0176 decision
/// 1), as `check-flows.py`'s `with_taken_by`: for each, an optional
/// attribute of type `actor_kind` that its effect first assigns `actor.kind`,
/// and an invariant holding it absent or one of the kinds named, each after
/// the type's own. A name the type declares itself refuses the load.
fn taken_by(t: &mut Type) -> Result<(), String> {
    let marked: Vec<(String, Vec<String>)> = t
        .transitions
        .iter()
        .filter(|(_, x)| !x.taken_by.is_empty())
        .map(|(n, x)| (n.clone(), x.taken_by.clone()))
        .collect();
    for (xn, kinds) in marked {
        let (a, i) = taken_by_names(&xn);
        if t.attribute(&a).is_some() || t.invariants.iter().any(|(n, _)| *n == i) {
            return Err(format!(
                "{}.{xn}'s taken_by generates the attribute {a} and the invariant {i}, which {} declares itself (ADR-0176)",
                t.name, t.name
            ));
        }
        let text = std::iter::once(format!("{a} is null"))
            .chain(kinds.iter().map(|k| format!("{a} == {k}")))
            .collect::<Vec<_>>()
            .join(" or ");
        let (Ok(holds), Ok(kind)) = (expr::parse(&text), expr::parse("actor.kind")) else {
            return Err(format!(
                "{}.{xn}'s taken_by names a kind of actor that is not human, agent or service",
                t.name
            ));
        };
        t.attributes.push((
            a.clone(),
            Attribute {
                declared: Declared::Scalar("actor_kind".into()),
                many: false,
                optional: true,
                default: None,
                unique: Unique::No,
                assignee: false,
                personal: false,
                actor_kind: None,
                identifier: None,
                opposite: None,
                composite: false,
                cascade: Vec::new(),
                external: None,
                indexed: false,
                survives_always: false,
            },
        ));
        t.invariants.push((i, holds));
        if let Some((_, x)) = t.transitions.iter_mut().find(|(n, _)| *n == xn) {
            x.effect.insert(
                0,
                Step::Assign {
                    location: a,
                    expr: kind,
                },
            );
        }
    }
    Ok(())
}

/// The bases a type extends, nearest first; the walk stops at an unknown
/// name or where it would come back to a type it has passed.
fn bases(name: &str, types: &BTreeMap<String, Type>) -> Vec<String> {
    let mut chain = Vec::new();
    let mut seen = BTreeSet::from([name.to_string()]);
    let mut base = types.get(name).and_then(|t| t.extends.clone());
    while let Some(b) = base {
        if !seen.insert(b.clone()) || !types.contains_key(&b) {
            break;
        }
        base = types.get(&b).and_then(|t| t.extends.clone());
        chain.push(b);
    }
    chain
}

/// A subtype with what its bases give it: their attributes, derived
/// attributes and invariants, the farthest base's first and its own last,
/// its own replacing one of the same name in place; its own invariants are
/// checked before the inherited ones (`declaration-syntax.md` §2,
/// `flow-format.md` §4.8).
fn inherit(t: &mut Type, chain: &[String], types: &BTreeMap<String, Type>) {
    fn upsert<V: Clone>(into: &mut Vec<(String, V)>, n: &str, v: &V) {
        match into.iter_mut().find(|(k, _)| k == n) {
            Some(slot) => slot.1 = v.clone(),
            None => into.push((n.to_string(), v.clone())),
        }
    }
    let ancestors: Vec<&Type> = chain.iter().rev().filter_map(|b| types.get(b)).collect();
    let mut attributes = Vec::new();
    let mut derived = Vec::new();
    let mut invariants = Vec::new();
    for b in &ancestors {
        for (n, a) in &b.attributes {
            upsert(&mut attributes, n, a);
            t.inherited.insert(n.clone(), b.name.clone());
        }
        for (n, e) in &b.derived {
            upsert(&mut derived, n, e);
        }
        for (n, e) in &b.invariants {
            upsert(&mut invariants, n, e);
        }
    }
    for (n, a) in &t.attributes {
        upsert(&mut attributes, n, a);
        t.inherited.remove(n);
    }
    for (n, e) in &t.derived {
        upsert(&mut derived, n, e);
    }
    let own: Vec<String> = t.invariants.iter().map(|(n, _)| n.clone()).collect();
    for (n, e) in &t.invariants {
        upsert(&mut invariants, n, e);
    }
    let (mine, theirs): (Vec<_>, Vec<_>) =
        invariants.into_iter().partition(|(n, _)| own.contains(n));
    t.attributes = attributes;
    t.derived = derived;
    t.invariants = mine.into_iter().chain(theirs).collect();
    // the declaration as the reference runner serves a subtype's
    for key in ["attributes", "derived_attributes", "invariants"] {
        let mut merged = serde_json::Map::new();
        for b in &ancestors {
            if let Some(m) = b.source.get(key).and_then(|v| v.as_object()) {
                merged.extend(m.clone());
            }
        }
        if let Some(m) = t.source.get(key).and_then(|v| v.as_object()) {
            merged.extend(m.clone());
        }
        if let Some(src) = t.source.as_object_mut()
            && !merged.is_empty()
        {
            src.insert(key.into(), serde_json::Value::Object(merged));
        }
    }
}

/// A binder with its machine merged in: the machine's states; its
/// conditions, beside the binder's own; and its transitions, of which the
/// initial ones give way to a binder's own initial transition, the binder's
/// own after them (`flow-format.md` §4.10, `declaration-syntax.md` §2.1).
fn bind(t: &mut Type, m: &Type) {
    let own_initial = t.transitions.iter().any(|(_, x)| x.kind == Kind::Initial);
    let own = std::mem::take(&mut t.transitions);
    t.transitions = m
        .transitions
        .iter()
        .filter(|(_, x)| !(own_initial && x.kind == Kind::Initial))
        .cloned()
        .collect();
    for (n, x) in own {
        match t.transitions.iter_mut().find(|(k, _)| *k == n) {
            Some(slot) => slot.1 = x,
            None => t.transitions.push((n, x)),
        }
    }
    let own_conditions = std::mem::take(&mut t.conditions);
    t.conditions = m.conditions.clone();
    for (n, c) in own_conditions {
        match t.conditions.iter_mut().find(|(k, _)| *k == n) {
            Some(slot) => slot.1 = c,
            None => t.conditions.push((n, c)),
        }
    }
    t.states = m.states.clone();
    // the declaration as the reference runner serves a binder's: its own,
    // with the machine's states, conditions and transitions merged in
    let section = |key: &str| {
        m.source
            .get(key)
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}))
    };
    let mut transitions = section("transitions");
    if let Some(map) = transitions.as_object_mut() {
        if own_initial {
            map.retain(|_, x| x.get("kind").and_then(|k| k.as_str()) != Some("initial"));
        }
        if let Some(mine) = t.source.get("transitions").and_then(|x| x.as_object()) {
            for (k, v) in mine {
                map.insert(k.clone(), v.clone());
            }
        }
    }
    let mut conditions = section("conditions");
    if let (Some(map), Some(mine)) = (
        conditions.as_object_mut(),
        t.source.get("conditions").and_then(|x| x.as_object()),
    ) {
        for (k, v) in mine {
            map.insert(k.clone(), v.clone());
        }
    }
    if let Some(src) = t.source.as_object_mut() {
        src.insert("states".into(), section("states"));
        src.insert("transitions".into(), transitions);
        src.insert("conditions".into(), conditions);
    }
}

/// The built-in type of the store's operator, its first actor (ADR-0126):
/// one object, `operator`, a human, active from the store's creation.
pub const OPERATOR: &str = "operator";

/// The built-in type of the store's operator.
pub const OPERATOR_TYPE: &str = "Operator";

/// One of the operator's transitions, taking no input.
fn operator_transition(kind: Kind, from: Option<From>, to: &str) -> Transition {
    Transition {
        description: None,
        kind,
        from,
        to: vec![to.to_string()],
        only_via: Vec::new(),
        proposable: false,
        required_inputs: Vec::new(),
        optional_inputs: Vec::new(),
        inputs: Vec::new(),
        guards: Vec::new(),
        effect: Vec::new(),
        backdating_limit: None,
        may_admit: Vec::new(),
        corrects: Vec::new(),
        taken_by: Vec::new(),
    }
}

fn operator_type() -> Type {
    Type {
        name: "Operator".to_string(),
        module: String::new(),
        attributes: vec![(
            "identity".to_string(),
            Attribute {
                declared: Declared::Scalar("identity".to_string()),
                many: false,
                optional: false,
                default: None,
                // one operator per identity (ADR-0126)
                unique: Unique::Yes,
                assignee: false,
                personal: false,
                actor_kind: Some("human".to_string()),
                identifier: None,
                opposite: None,
                composite: false,
                cascade: Vec::new(),
                external: None,
                indexed: false,
                survives_always: false,
            },
        )],
        derived: Vec::new(),
        states: vec![
            (
                "ACTIVE".to_string(),
                State {
                    category: "live".to_string(),
                    is_final: false,
                    required_attributes: Vec::new(),
                    superseding: false,
                },
            ),
            (
                "RETIRED".to_string(),
                State {
                    category: "closed".to_string(),
                    is_final: true,
                    required_attributes: Vec::new(),
                    superseding: false,
                },
            ),
        ],
        conditions: Vec::new(),
        invariants: Vec::new(),
        // its creation, taken only by creating the store, and its
        // retirement, requested like any transition (ADR-0126)
        transitions: vec![
            (
                "install".into(),
                operator_transition(Kind::Initial, None, "ACTIVE"),
            ),
            (
                "retire".into(),
                operator_transition(
                    Kind::External,
                    Some(From::States(vec!["ACTIVE".into()])),
                    "RETIRED",
                ),
            ),
        ],
        beyond: Vec::new(),
        at_outcome: Vec::new(),
        observations: Vec::new(),
        observation_on: None,
        state_machine: None,
        extends: None,
        mirror: false,
        abstract_: false,
        retired: false,
        inherited: BTreeMap::new(),
        source: serde_json::json!({
            "description": "The store's operator, its first actor (ADR-0126).",
            "attributes": { "identity": { "type": "identity", "actor_kind": "human", "unique": true } },
            "states": { "ACTIVE": { "category": "live" }, "RETIRED": { "category": "closed", "final": true } },
            "transitions": {
                "install": { "kind": "initial", "to": "ACTIVE" },
                "retire": { "kind": "external", "from": ["ACTIVE"], "to": "RETIRED" }
            }
        }),
    }
}

/// The built-in type of a request filed for approval (`DESIGN.md` §9):
/// what it would request, who filed it, and its lifecycle, `PENDING` until it
/// is approved and executed, rejected, withdrawn or invalidated. Its
/// transitions are the store's own, not declared here.
pub const PROPOSAL: &str = "Proposal";

fn proposal_type() -> Type {
    let attribute = |t: &str, optional: bool| Attribute {
        declared: Declared::Scalar(t.to_string()),
        many: false,
        optional,
        default: None,
        unique: Unique::No,
        assignee: false,
        personal: false,
        actor_kind: None,
        identifier: None,
        opposite: None,
        composite: false,
        cascade: Vec::new(),
        external: None,
        indexed: false,
        survives_always: false,
    };
    let state = |category: &str, is_final: bool| State {
        category: category.to_string(),
        is_final,
        required_attributes: Vec::new(),
        superseding: false,
    };
    Type {
        name: PROPOSAL.to_string(),
        module: String::new(),
        attributes: vec![
            // none for a proposed creation
            ("target".into(), attribute("string", true)),
            ("target_type".into(), attribute("string", false)),
            ("transition".into(), attribute("string", false)),
            // the inputs as the wire carried them, re-read under the
            // declaration in force when it is approved
            ("inputs".into(), attribute("string", false)),
            ("proposer".into(), attribute("string", false)),
            // why the proposer filed it, which its execution records
            ("context".into(), attribute("string", true)),
            ("occurred".into(), attribute("timestamp", true)),
            ("filed_at".into(), attribute("timestamp", false)),
            ("expires_at".into(), attribute("timestamp", true)),
        ],
        derived: Vec::new(),
        states: vec![
            ("PENDING".into(), state("live", false)),
            ("EXECUTED".into(), state("closed", true)),
            ("REJECTED".into(), state("closed", true)),
            ("WITHDRAWN".into(), state("closed", true)),
            ("INVALIDATED".into(), state("closed", true)),
        ],
        conditions: Vec::new(),
        invariants: Vec::new(),
        transitions: Vec::new(),
        beyond: Vec::new(),
        at_outcome: Vec::new(),
        observations: Vec::new(),
        observation_on: None,
        state_machine: None,
        extends: None,
        mirror: false,
        abstract_: false,
        retired: false,
        inherited: BTreeMap::new(),
        source: serde_json::json!({
            "description": "A request filed for approval instead of executed (DESIGN.md §9).",
            "attributes": {
                "target": { "type": "string", "optional": true },
                "target_type": { "type": "string" },
                "transition": { "type": "string" },
                "inputs": { "type": "string" },
                "proposer": { "type": "string" },
                "context": { "type": "string", "optional": true },
                "occurred": { "type": "timestamp", "optional": true },
                "filed_at": { "type": "timestamp" },
                "expires_at": { "type": "timestamp", "optional": true }
            },
            "states": {
                "PENDING": { "category": "live" },
                "EXECUTED": { "category": "closed", "final": true },
                "REJECTED": { "category": "closed", "final": true },
                "WITHDRAWN": { "category": "closed", "final": true },
                "INVALIDATED": { "category": "closed", "final": true }
            },
            "transitions": {
                "approve": { "kind": "external", "from": ["PENDING"], "to": "EXECUTED" },
                "reject": { "kind": "external", "from": ["PENDING"], "to": "REJECTED" },
                "withdraw": { "kind": "external", "from": ["PENDING"], "to": "WITHDRAWN" },
                "invalidate": { "kind": "external", "from": ["PENDING"], "to": "INVALIDATED" }
            }
        }),
    }
}

/// The built-in type of a change to the declaration (`DESIGN.md` §9): the
/// source it proposes, where it came from and its digest, the version it was
/// drafted against, and the dry run's report. Its transitions are the
/// store's own.
pub const DECLARATION_CHANGE: &str = "DeclarationChange";

fn declaration_change_type() -> Type {
    let mut t = proposal_type();
    t.name = DECLARATION_CHANGE.to_string();
    let attribute = |ty: &str, optional: bool| Attribute {
        declared: Declared::Scalar(ty.to_string()),
        optional,
        ..t.attributes[0].1.clone()
    };
    t.attributes = vec![
        // the source as a JSON object, each module's name to its text
        ("source".into(), attribute("string", false)),
        ("source_ref".into(), attribute("string", false)),
        ("source_digest".into(), attribute("string", false)),
        ("base_version".into(), attribute("int", false)),
        // the dry run's report, as JSON, when it is submitted
        ("report".into(), attribute("string", true)),
        // the references it cites, and each read as it answered at
        // submission, as JSON (ADR-0097, ADR-0159)
        ("evidence".into(), attribute("string", true)),
        ("drafted_by".into(), attribute("string", false)),
        ("drafted_by_kind".into(), attribute("string", false)),
        ("approved_by".into(), attribute("string", true)),
        ("approved_by_kind".into(), attribute("string", true)),
    ];
    let state = |category: &str, is_final: bool| State {
        category: category.to_string(),
        is_final,
        required_attributes: Vec::new(),
        superseding: false,
    };
    t.states = vec![
        ("DRAFTED".into(), state("live", false)),
        ("SUBMITTED".into(), state("live", false)),
        ("PUBLISHED".into(), state("closed", true)),
        ("REJECTED".into(), state("closed", true)),
        ("WITHDRAWN".into(), state("closed", true)),
        ("SUPERSEDED".into(), state("closed", true)),
    ];
    t.source = serde_json::json!({
        "description": "A change to the declaration, drafted, submitted and published (DESIGN.md §9).",
        "attributes": {
            "source": { "type": "string" }, "source_ref": { "type": "string" },
            "source_digest": { "type": "string" }, "base_version": { "type": "int" },
            "report": { "type": "string", "optional": true },
            "evidence": { "type": "string", "optional": true },
            "drafted_by": { "type": "string" }, "drafted_by_kind": { "type": "string" },
            "approved_by": { "type": "string", "optional": true },
            "approved_by_kind": { "type": "string", "optional": true }
        },
        "states": {
            "DRAFTED": { "category": "live" }, "SUBMITTED": { "category": "live" },
            "PUBLISHED": { "category": "closed", "final": true },
            "REJECTED": { "category": "closed", "final": true },
            "WITHDRAWN": { "category": "closed", "final": true },
            "SUPERSEDED": { "category": "closed", "final": true }
        },
        "transitions": {
            "draft": { "kind": "initial", "to": "DRAFTED" },
            "revise": { "kind": "internal", "from": ["DRAFTED"] },
            "reopen": { "kind": "external", "from": ["SUBMITTED"], "to": "DRAFTED" },
            "submit": { "kind": "external", "from": ["DRAFTED"], "to": "SUBMITTED" },
            "refresh": { "kind": "internal", "from": ["SUBMITTED"] },
            "publish": { "kind": "external", "from": ["SUBMITTED"], "to": "PUBLISHED" },
            "reject": { "kind": "external", "from": ["SUBMITTED"], "to": "REJECTED" },
            "withdraw": { "kind": "external", "from": ["DRAFTED", "SUBMITTED"], "to": "WITHDRAWN" },
            "supersede": { "kind": "external", "from": ["DRAFTED", "SUBMITTED"], "to": "SUPERSEDED" }
        }
    });
    t
}

/// The built-in observation kind every type carries (`declaration-syntax.md`
/// §6.8), and the type it is sugar for.
pub const LABEL: &str = "label";

/// The type a label's `subject` names: any, since every type carries labels.
pub const ANY_TYPE: &str = "*";

/// A label as the kind it is: a name, normalised to lower case and trimmed;
/// an optional note, personal, which its `forget` redacts; and the subject's
/// state when it was applied, which the recording writes, so it is no input.
/// Its occurred time is its recorded time, so it has no recording bound.
fn label_type() -> Type {
    let attribute = |declared: Declared, optional: bool, personal: bool| Attribute {
        declared,
        many: false,
        optional,
        default: None,
        unique: Unique::No,
        assignee: false,
        personal,
        actor_kind: None,
        identifier: None,
        opposite: None,
        composite: false,
        cascade: Vec::new(),
        external: None,
        indexed: false,
        survives_always: false,
    };
    let text = || Declared::Scalar("string".into());
    let time = || Declared::Scalar("timestamp".into());
    let transition = |kind: Kind,
                      to: Vec<String>,
                      required: &[&str],
                      optional: &[&str],
                      inputs: Vec<(String, Input)>| {
        Transition {
            description: None,
            kind,
            from: None,
            to,
            only_via: Vec::new(),
            proposable: false,
            required_inputs: required.iter().map(|s| s.to_string()).collect(),
            optional_inputs: optional.iter().map(|s| s.to_string()).collect(),
            inputs,
            guards: Vec::new(),
            effect: Vec::new(),
            backdating_limit: None,
            may_admit: Vec::new(),
            corrects: Vec::new(),
            taken_by: Vec::new(),
        }
    };
    let reason = Input {
        declared: text(),
        many: false,
        optional: false,
        default: None,
        personal: false,
    };
    Type {
        name: LABEL.to_string(),
        module: String::new(),
        attributes: vec![
            ("name".into(), attribute(text(), false, false)),
            ("note".into(), attribute(text(), true, true)),
            ("subject_state".into(), attribute(text(), false, false)),
            (
                "subject".into(),
                attribute(Declared::Reference(ANY_TYPE.into()), false, false),
            ),
            (
                "corrects".into(),
                attribute(Declared::Reference(LABEL.into()), true, false),
            ),
            ("recorded_at".into(), attribute(time(), false, false)),
            ("occurred_at".into(), attribute(time(), false, false)),
        ],
        derived: Vec::new(),
        states: vec![(
            "RECORDED".into(),
            State {
                category: "closed".into(),
                is_final: true,
                required_attributes: Vec::new(),
                superseding: false,
            },
        )],
        conditions: Vec::new(),
        invariants: Vec::new(),
        transitions: vec![
            (
                "record".into(),
                transition(
                    Kind::Initial,
                    vec!["RECORDED".into()],
                    &["subject", "name"],
                    &["corrects", "note"],
                    Vec::new(),
                ),
            ),
            (
                "forget".into(),
                transition(
                    Kind::Erasure,
                    Vec::new(),
                    &[],
                    &[],
                    vec![("reason".into(), reason)],
                ),
            ),
        ],
        beyond: Vec::new(),
        at_outcome: Vec::new(),
        observations: Vec::new(),
        observation_on: Some(ANY_TYPE.to_string()),
        state_machine: None,
        extends: None,
        mirror: false,
        abstract_: false,
        retired: false,
        inherited: BTreeMap::new(),
        source: serde_json::json!({
            "description": "A label: what nobody has modelled yet, on an object of any type (DESIGN.md §5.11).",
            "attributes": {
                "name": { "type": "string" },
                "note": { "type": "string", "optional": true, "personal": true },
                "subject_state": { "type": "string" },
                "subject": { "reference": ANY_TYPE },
                "corrects": { "reference": LABEL, "optional": true },
                "recorded_at": { "type": "timestamp" },
                "occurred_at": { "type": "timestamp" }
            },
            "states": { "RECORDED": { "category": "closed", "final": true } },
            "transitions": {
                "record": {
                    "kind": "initial", "to": "RECORDED",
                    "required_inputs": ["subject", "name"], "optional_inputs": ["corrects", "note"]
                },
                "forget": { "kind": "erasure", "inputs": { "reason": { "type": "string" } } }
            }
        }),
    }
}

/// The built-in type of a subscription (`DESIGN.md` §7): its filter, over a
/// type or its family, transition names, every one where it names none, and
/// whether only a change of state; its lag thresholds; and its lifecycle,
/// `subscribe` creating one `ACTIVE` and `revoke` ending it.
pub const SUBSCRIPTION: &str = "Subscription";

fn subscription_type() -> Type {
    let attribute = |t: &str, many: bool, optional: bool, default: Option<Expr>| Attribute {
        declared: Declared::Scalar(t.to_string()),
        many,
        optional,
        default,
        unique: Unique::No,
        assignee: false,
        personal: false,
        actor_kind: None,
        identifier: None,
        opposite: None,
        composite: false,
        cascade: Vec::new(),
        external: None,
        indexed: false,
        survives_always: false,
    };
    let state = |category: &str, is_final: bool| State {
        category: category.to_string(),
        is_final,
        required_attributes: Vec::new(),
        superseding: false,
    };
    let transition =
        |kind: Kind, from: Option<From>, to: &str, required: &[&str], optional: &[&str]| {
            Transition {
                description: None,
                kind,
                from,
                to: vec![to.to_string()],
                only_via: Vec::new(),
                proposable: false,
                required_inputs: required.iter().map(|s| s.to_string()).collect(),
                optional_inputs: optional.iter().map(|s| s.to_string()).collect(),
                inputs: Vec::new(),
                guards: Vec::new(),
                effect: Vec::new(),
                backdating_limit: None,
                may_admit: Vec::new(),
                corrects: Vec::new(),
                taken_by: Vec::new(),
            }
        };
    Type {
        name: SUBSCRIPTION.to_string(),
        module: String::new(),
        attributes: vec![
            (
                "target_type".into(),
                attribute("string", false, false, None),
            ),
            (
                "target_family".into(),
                attribute("bool", false, false, Some(Expr::Bool(false))),
            ),
            ("transitions".into(), attribute("string", true, true, None)),
            (
                "changes_state_only".into(),
                attribute("bool", false, false, Some(Expr::Bool(false))),
            ),
            (
                "lagging_after".into(),
                attribute("duration", false, true, None),
            ),
            (
                "dead_after".into(),
                attribute("duration", false, true, None),
            ),
        ],
        derived: Vec::new(),
        states: vec![
            ("ACTIVE".into(), state("live", false)),
            ("REVOKED".into(), state("closed", true)),
        ],
        conditions: Vec::new(),
        invariants: Vec::new(),
        transitions: vec![
            (
                "subscribe".into(),
                transition(
                    Kind::Initial,
                    None,
                    "ACTIVE",
                    &["target_type"],
                    &[
                        "target_family",
                        "transitions",
                        "changes_state_only",
                        "lagging_after",
                        "dead_after",
                    ],
                ),
            ),
            (
                "revoke".into(),
                transition(
                    Kind::External,
                    Some(From::States(vec!["ACTIVE".into()])),
                    "REVOKED",
                    &[],
                    &[],
                ),
            ),
        ],
        beyond: Vec::new(),
        at_outcome: Vec::new(),
        observations: Vec::new(),
        observation_on: None,
        state_machine: None,
        extends: None,
        mirror: false,
        abstract_: false,
        retired: false,
        inherited: BTreeMap::new(),
        source: serde_json::json!({
            "description": "A subscription: a filter over the log that a reader pulls and acknowledges (DESIGN.md §7).",
            "attributes": {
                "target_type": { "type": "string" },
                "target_family": { "type": "bool", "default": false },
                "transitions": { "type": "string[]" },
                "changes_state_only": { "type": "bool", "default": false },
                "lagging_after": { "type": "duration", "optional": true },
                "dead_after": { "type": "duration", "optional": true }
            },
            "states": {
                "ACTIVE": { "category": "live" },
                "REVOKED": { "category": "closed", "final": true }
            },
            "transitions": {
                "subscribe": {
                    "kind": "initial", "to": "ACTIVE", "required_inputs": ["target_type"],
                    "optional_inputs": ["target_family", "transitions", "changes_state_only", "lagging_after", "dead_after"]
                },
                "revoke": { "kind": "external", "from": ["ACTIVE"], "to": "REVOKED" }
            }
        }),
    }
}

impl Library {
    /// The library of modules given as (file, text), in import order, with
    /// the built-in Operator, Proposal, DeclarationChange and Subscription
    /// types and the label kind.
    pub fn load(files: &[(String, String)]) -> Result<Library, LoadError> {
        let mut lib = Library::default();
        let mut machines: BTreeMap<String, Type> = BTreeMap::new();
        lib.types.insert("Operator".to_string(), operator_type());
        lib.types.insert(PROPOSAL.to_string(), proposal_type());
        lib.types
            .insert(DECLARATION_CHANGE.to_string(), declaration_change_type());
        lib.types.insert(LABEL.to_string(), label_type());
        lib.types
            .insert(SUBSCRIPTION.to_string(), subscription_type());
        lib.version = 1;
        // which module declared each type, enumeration and machine: a store
        // holds one of a name (ADR-0163 decision 5)
        let mut owner: BTreeMap<String, String> = BTreeMap::new();
        for (file, text) in files {
            // an examples file is part of the source, and no module of it
            if let Some(name) = crate::source::name_of(text)
                && crate::source::is_examples(&name)
            {
                lib.source.push((name, text.clone()));
                continue;
            }
            let m = load_module(file, text)?;
            let names = m
                .types
                .iter()
                .map(|t| ("type", t.name.as_str()))
                .chain(
                    m.enumerations
                        .iter()
                        .map(|(n, _)| ("enumeration", n.as_str())),
                )
                .chain(m.machines.iter().map(|t| ("machine", t.name.as_str())));
            for (kind, n) in names {
                if let Some(other) = owner.insert(n.to_string(), m.name.clone())
                    && other != m.name
                {
                    return Err(LoadError {
                        file: file.clone(),
                        line: declared_line(text, n),
                        message: format!(
                            "the {kind} {n} is also declared by {other}; a store holds one type, enumeration or machine of a name (ADR-0163)"
                        ),
                    });
                }
            }
            lib.source.push((m.name.clone(), text.clone()));
            lib.migration.extend(m.migration);
            for (n, members) in m.enumerations {
                lib.enumerations.insert(n, members);
            }
            for t in m.types {
                if !m.observation_kinds.contains(&t.name) {
                    lib.declared.push(t.name.clone());
                }
                lib.types.insert(t.name.clone(), t);
            }
            lib.observation_kinds.extend(m.observation_kinds);
            for machine in m.machines {
                machines.insert(machine.name.clone(), machine);
            }
            lib.evaluators.extend(m.evaluators);
            let declared = lib.metrics.entry(m.name.clone()).or_default();
            for (n, owner, spec) in m.metrics {
                declared.insert(n, (owner, spec));
            }
            lib.modules.push(m.name);
        }
        // each binder with its machine merged in, once every module, and so
        // every machine it may import, is read
        for t in lib.types.values_mut() {
            if let Some(name) = t.state_machine.clone() {
                match machines.get(&name) {
                    Some(m) => bind(t, m),
                    None => t
                        .beyond
                        .push(format!("the machine {name}, which no module declares")),
                }
            }
        }
        // what each transition's taken_by generates, once each binder holds
        // its machine's transitions and before a subtype inherits it
        // (ADR-0176 decision 1); a transition declaring none takes its
        // type's, a machine's bound by the type among them, which a subtype
        // does not inherit, as it inherits no transition (ADR-0182 decision 5)
        for t in lib.types.values_mut() {
            let own: Vec<String> = t
                .source
                .get("taken_by")
                .and_then(serde_json::Value::as_array)
                .map(|kinds| {
                    kinds
                        .iter()
                        .filter_map(|k| k.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();
            if !own.is_empty() {
                for (_, x) in &mut t.transitions {
                    if x.taken_by.is_empty() {
                        x.taken_by = own.clone();
                    }
                }
            }
            if let Err(message) = taken_by(t) {
                let (file, line) = files
                    .iter()
                    .find(|(_, text)| {
                        crate::source::name_of(text).as_deref() == Some(t.module.as_str())
                    })
                    .map_or((String::new(), 1), |(f, text)| {
                        (f.clone(), declared_line(text, &t.name))
                    });
                return Err(LoadError {
                    file,
                    line,
                    message,
                });
            }
        }
        // each subtype with what its bases give it, each base as declared
        let declared = lib.types.clone();
        for t in lib.types.values_mut() {
            let chain = bases(&t.name, &declared);
            if !chain.is_empty() {
                inherit(t, &chain, &declared);
            }
        }
        // no attribute is the column a single reference is kept in, which
        // the store could not create, refused as the checker refuses it
        // (`storage-schema.md` §3.2, D578)
        for t in lib.types.values() {
            for (a, _) in &t.attributes {
                let clash = a
                    .strip_suffix("_id")
                    .and_then(|base| t.attribute(base))
                    .is_some_and(|r| matches!(r.declared, Declared::Reference(_)) && !r.many);
                if clash {
                    let (file, line) = files
                        .iter()
                        .find(|(_, text)| {
                            crate::source::name_of(text).as_deref() == Some(t.module.as_str())
                        })
                        .map_or((String::new(), 1), |(f, text)| {
                            (f.clone(), declared_line(text, &t.name))
                        });
                    return Err(LoadError {
                        file,
                        line,
                        message: format!(
                            "{}.{a} is the column the store keeps the reference {} in; name the attribute otherwise (storage-schema.md §3.2, D578)",
                            t.name,
                            a.trim_end_matches("_id")
                        ),
                    });
                }
            }
        }
        Ok(lib)
    }

    /// The type whose family a uniqueness of an attribute spans: the one
    /// declaring it, so a base's spans every subtype, siblings included
    /// (`flow-format.md` §4.14, D519).
    pub fn unique_scope<'t>(&'t self, t: &'t Type, attribute: &str) -> &'t str {
        t.inherited
            .get(attribute)
            .map_or(t.name.as_str(), String::as_str)
    }

    /// Whether a type is the one wanted, or extends it.
    pub fn is_a(&self, type_: &str, want: &str) -> bool {
        type_ == want || bases(type_, &self.types).iter().any(|b| b == want)
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
            "    description: A job.\n    inherited_parts: { lines: { survives: true } }\n",
        );
        let m = load_module("jobs.yaml", &text).unwrap();
        assert_eq!(m.types[0].beyond.len(), 1);
        assert!(m.types[0].beyond[0].contains("Job.inherited_parts"));
    }

    #[test]
    fn a_bad_expression_is_refused_with_its_line() {
        let text = JOBS.replace("title is not null", "title is not");
        let err = load_module("jobs.yaml", &text).unwrap_err();
        assert_eq!(err.line, 17);
        assert!(err.message.contains("not an expression"));
    }

    #[test]
    fn a_binder_takes_its_machine_its_own_initial_replacing_the_machines() {
        let text = r#"
module: approvals
categories: [pending, closed]
machines:
  Approval:
    description: An approval.
    requires:
      attributes:
        amount: { type: int }
    states:
      DRAFT: { category: pending }
      DONE: { category: closed, final: true }
    conditions:
      small: { expression: amount < 10, remedy: delegable }
    transitions:
      draft: { kind: initial, to: DRAFT, required_inputs: [amount] }
      approve: { kind: external, from: [DRAFT], to: DONE, guards: { small: deny } }
types:
  Purchase:
    state_machine: Approval
    attributes:
      amount: { type: int }
      vendor: { type: string }
    conditions:
      named: { expression: vendor is not null, remedy: self_serviceable }
    transitions:
      raise: { kind: initial, to: DRAFT, required_inputs: [amount, vendor] }
      rename: { kind: internal, from: [DRAFT], required_inputs: [vendor], guards: { named: deny } }
"#;
        let lib = Library::load(&[("approvals.yaml".into(), text.into())]).unwrap();
        let t = lib.type_("Purchase").unwrap();
        assert!(t.beyond.is_empty(), "{:?}", t.beyond);
        let names: Vec<&str> = t.transitions.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["approve", "raise", "rename"]);
        let states: Vec<&str> = t.states.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(states, vec!["DRAFT", "DONE"]);
        assert!(t.condition("small").is_some() && t.condition("named").is_some());
        let src = &t.source;
        assert!(src["transitions"].get("draft").is_none());
        assert!(src["transitions"].get("approve").is_some());
        assert_eq!(src["states"]["DONE"]["final"], serde_json::json!(true));
    }

    /// A store holds one type, enumeration or machine of a name (ADR-0163):
    /// a load refuses a name a second module declares, naming both and the
    /// line, as two modules each declaring `Service` once did (D529).
    #[test]
    fn taken_by_generates_the_record_and_the_invariant_over_it() {
        let desk = "module: desk\ncategories: [live, closed]\ntypes:\n  Ticket:\n    attributes:\n      title: { type: string }\n    states: { OPEN: { category: live }, DONE: { category: closed, final: true } }\n    transitions:\n      open: { kind: initial, to: OPEN, required_inputs: [title] }\n      close: { kind: external, from: OPEN, to: DONE, taken_by: [human, service], effect: [ { clear: [title] } ] }\n";
        let lib = Library::load(&[("desk.yaml".into(), desk.into())]).unwrap();
        let t = lib.type_("Ticket").unwrap();
        let a = t.attribute("close_by_kind").unwrap();
        assert!(a.optional && a.declared == Declared::Scalar("actor_kind".into()));
        let (name, _) = t.invariants.last().unwrap();
        assert_eq!(name, "close_taken_by");
        let close = &t.transitions.iter().find(|(n, _)| n == "close").unwrap().1;
        assert!(
            matches!(&close.effect[..], [Step::Assign { location, .. }, Step::Clear(_)] if location == "close_by_kind")
        );
        // the opening, unmarked, is untouched, and the declaration served is
        // the one written
        let open = &t.transitions.iter().find(|(n, _)| n == "open").unwrap().1;
        assert!(open.effect.is_empty());
        assert!(t.source["attributes"].get("close_by_kind").is_none());
        // a name it generates that the type declares refuses the load
        let mine = desk.replace(
            "      title: { type: string }\n",
            "      title: { type: string }\n      close_by_kind: { type: string, optional: true }\n",
        );
        let err = Library::load(&[("desk.yaml".into(), mine)]).unwrap_err();
        assert!(
            err.message
                .contains("Ticket.close's taken_by generates the attribute close_by_kind"),
            "{}",
            err.message
        );
        assert_eq!((err.file.as_str(), err.line), ("desk.yaml", 4));
    }

    #[test]
    fn a_load_refuses_a_name_two_modules_declare() {
        let people = "module: people\ncategories: [live]\ntypes:\n  Service:\n    attributes:\n      name: { type: identity, actor_kind: service }\n    states: { ACTIVE: { category: live } }\n";
        let desk = "module: desk\ncategories: [live]\nenumerations:\n  Tier: [A, B]\ntypes:\n  Service:\n    states: { ON: { category: live } }\n";
        let err = Library::load(&[
            ("people.yaml".into(), people.into()),
            ("desk.yaml".into(), desk.into()),
        ])
        .unwrap_err();
        assert_eq!((err.file.as_str(), err.line), ("desk.yaml", 6));
        assert!(
            err.message
                .contains("the type Service is also declared by people"),
            "{}",
            err.message
        );
        // an enumeration clashes as a type does
        let tiers = "module: tiers\ncategories: [live]\nenumerations:\n  Tier: [X]\n";
        let err = Library::load(&[
            ("desk.yaml".into(), desk.replace("  Service:", "  Desk:")),
            ("tiers.yaml".into(), tiers.into()),
        ])
        .unwrap_err();
        assert!(
            err.message
                .contains("the enumeration Tier is also declared by desk"),
            "{}",
            err.message
        );
        assert!(
            Library::load(&[
                ("people.yaml".into(), people.into()),
                ("tiers.yaml".into(), tiers.into())
            ])
            .is_ok()
        );
    }
}
