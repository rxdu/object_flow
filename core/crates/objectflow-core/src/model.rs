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
    /// A value of several fields, by the name of the value a module
    /// declares (ADR-0254). The reader gives it as a `Scalar`, and the
    /// library names it a value once every module is read.
    Value(String),
}

/// A value of several fields a module declares once with its rules, which
/// an attribute or a declared input names as its type: equal by its fields,
/// with no id, state or events of its own (ADR-0254).
#[derive(Clone, Debug, PartialEq)]
pub struct ValueType {
    pub name: String,
    pub module: String,
    pub description: String,
    /// Its fields in the order declared, each a built-in type or an
    /// enumeration.
    pub fields: Vec<(String, Attribute)>,
    /// Its invariants, each by its name and its expression as written over
    /// its own fields, which each attribute holding it holds as
    /// `<attribute>_<invariant>` (ADR-0255).
    pub invariants: Vec<(String, String)>,
    /// Its declaration as written, as a type's `source` is.
    pub source: serde_json::Value,
}

impl ValueType {
    pub fn field(&self, name: &str) -> Option<&Attribute> {
        self.fields.iter().find(|(n, _)| n == name).map(|(_, a)| a)
    }
}

/// Whether a list of media types, as a file attribute or input declares
/// it, takes a file of `media_type`: any where the list is empty, else one
/// it lists, or one of a type it lists as `type/*` (ADR-0239). A file's
/// media type is held in lower case, as the list is written.
pub fn takes_media(types: &[String], media_type: &str) -> bool {
    types.is_empty()
        || types.iter().any(|t| match t.strip_suffix("/*") {
            Some(kind) => media_type.split_once('/').is_some_and(|(k, _)| k == kind),
            None => t == media_type,
        })
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
    /// `not_blank: true`, on a string: a text empty once trimmed is never
    /// written, by its input or by the invariant `<attribute>_not_blank`
    /// the library generates (ADR-0257).
    pub not_blank: bool,
    /// On a file or a set of files, the media types it takes, each
    /// `type/subtype` or `type/*`; empty where it takes any (ADR-0239).
    pub media_types: Vec<String>,
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
        /// `rest: inputs`, until the load expands it into the inputs it
        /// passes, after which it is false (ADR-0271).
        rest: bool,
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
    /// `not_blank: true`, on a string: a text empty once trimmed is refused
    /// `malformed` (ADR-0257).
    pub not_blank: bool,
    /// On a file or a set of files, the media types it takes; empty where
    /// it takes any (ADR-0239).
    pub media_types: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub category: String,
    pub is_final: bool,
    pub required_attributes: Vec<String>,
    /// An object in it is superseded: another stands in its place (ADR-0028).
    pub superseding: bool,
    /// Whether an actor whose object is in it acts: false in a final state
    /// and in one declaring `acts: false`, either refusing the actor
    /// `actor_active` (ADR-0182 decision 6, ADR-0241).
    pub acts: bool,
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
    /// On an initial transition, the inputs, each a required reference to
    /// one object, whose object's offer holds this creation (ADR-0240).
    pub offered_on: Vec<String>,
    pub proposable: bool,
    pub required_inputs: Vec<String>,
    pub optional_inputs: Vec<String>,
    /// The optional inputs that clear their attribute where a request gives
    /// them as null, as `clear` does; one left out keeps the value
    /// (ADR-0262).
    pub clearable_inputs: Vec<String>,
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
    /// For each part it inherits from an abstract base, the cascade and
    /// whether the parts survive every final transition, which the library
    /// gives its copy of that end (`flow-format.md` §4.14).
    pub inherited_parts: Vec<(String, Vec<CascadeClause>, bool)>,
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

/// Members by type, as `(type, member)`.
pub type Held = Vec<(String, String)>;

/// Where a library holds values of type `identity`, each by type and name.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct IdentityHeld {
    pub attrs: Held,
    pub sets: Held,
    pub inputs: Held,
}

impl Library {
    /// Where a value of type `identity` is held, by type: a single
    /// attribute, a set, and an input a transition takes, its own or an
    /// attribute's, which an erasure of an actor whose identity is personal
    /// replaces wherever it is (ADR-0203 decision 1, ADR-0205 decision 6).
    pub fn identity_held(&self) -> IdentityHeld {
        let identity = |d: &Declared| matches!(d, Declared::Scalar(s) if s == "identity");
        let mut held = IdentityHeld::default();
        let mut inputs = BTreeSet::new();
        for t in self.types.values().filter(|t| !t.abstract_) {
            for (n, a) in &t.attributes {
                if identity(&a.declared) {
                    let at = if a.many {
                        &mut held.sets
                    } else {
                        &mut held.attrs
                    };
                    at.push((t.name.clone(), n.clone()));
                }
            }
            for (_, x) in &t.transitions {
                for (n, i) in &x.inputs {
                    if identity(&i.declared) {
                        inputs.insert((t.name.clone(), n.clone()));
                    }
                }
                for n in x.required_inputs.iter().chain(&x.optional_inputs) {
                    if t.attribute(n).is_some_and(|a| identity(&a.declared)) {
                        inputs.insert((t.name.clone(), n.clone()));
                    }
                }
            }
        }
        held.inputs = inputs.into_iter().collect();
        held
    }

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
                // a value's fields are not timed in this slice (ADR-0254)
                Declared::Value(_) => false,
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

/// The expression of the invariant `<attribute>_not_blank` that an
/// attribute's `not_blank` generates (ADR-0257 decision 3).
fn not_blank_expression(attribute: &str) -> String {
    format!("{attribute} is null or trim({attribute}) != \"\"")
}

impl Type {
    pub fn attribute(&self, name: &str) -> Option<&Attribute> {
        self.attributes
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a)
    }

    /// The attribute an invariant holds never blank, where it is the one a
    /// `not_blank` generates: named `<attribute>_not_blank`, its expression
    /// the one the marking generates, so an invariant of that name a subtype
    /// declares otherwise is not taken for it (ADR-0257 decision 3,
    /// ADR-0259).
    pub fn not_blank_of<'i>(&self, invariant: &'i str, expression: &Expr) -> Option<&'i str> {
        let a = invariant.strip_suffix("_not_blank")?;
        let generated = expr::parse(&not_blank_expression(a)).ok()?;
        (generated == *expression).then_some(a)
    }

    /// The attribute an invariant `<attribute> is not null` holds present,
    /// where the attribute is one stored scalar of the type, which no input
    /// writes absent and no other type's transition writes (ADR-0261).
    pub fn presence_of<'e>(&self, expression: &'e Expr) -> Option<&'e str> {
        let Expr::IsNull(e, true) = expression else {
            return None;
        };
        let Expr::Name(a) = e.as_ref() else {
            return None;
        };
        self.attribute(a)
            .is_some_and(|s| matches!(s.declared, Declared::Scalar(_)) && !s.many)
            .then_some(a.as_str())
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
    /// The kinds of actor that approve a version, where it declares them
    /// (`publishing.taken_by`, ADR-0187 decision 1).
    pub publishing: Option<Vec<String>>,
    /// The sequences it declares, by name (`flow-format.md` §4.12).
    pub sequences: Vec<String>,
    /// Its `migration`: what a publish of it maps (`flow-format.md` §4.16).
    pub migration: Migration,
    pub enumerations: Vec<(String, Vec<String>)>,
    /// The values it declares (ADR-0254).
    pub values: Vec<ValueType>,
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
            not_blank: self.flag(node, "not_blank")?,
            media_types: self.names(node.get("media_types"))?,
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
            cascade: self.cascade(node.get("cascade"))?,
        })
    }

    /// A composite end's cascade, or an inherited part's.
    fn cascade(&self, node: Option<&Node>) -> Loaded<Vec<CascadeClause>> {
        let mut clauses = Vec::new();
        for c in node.map(Node::items).unwrap_or_default() {
            clauses.push(CascadeClause {
                on: self.names(c.get("on"))?,
                transition: match c.get("transition").and_then(Node::text) {
                    Some(t) => t.to_string(),
                    None => {
                        return self.error(c, "a cascade names the transition its parts take");
                    }
                },
                inputs: self.bindings(c.get("inputs"))?,
                limit: c.get("limit").and_then(Node::integer),
            });
        }
        Ok(clauses)
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
            not_blank: self.flag(node, "not_blank")?,
            media_types: self.names(node.get("media_types"))?,
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
                    rest: spec.get("rest").and_then(Node::text) == Some("inputs"),
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
            offered_on: self.names(node.get("offered_on"))?,
            proposable: self.flag(node, "proposable")?,
            required_inputs: self.names(node.get("required_inputs"))?,
            optional_inputs: self.names(node.get("optional_inputs"))?,
            clearable_inputs: self.names(node.get("clearable_inputs"))?,
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
        let beyond = Vec::new();
        let at_outcome = Vec::new();
        // each part it inherits, with its cascade and survives, where every
        // request of the type was not run (the values audit's K-11)
        let mut inherited_parts = Vec::new();
        for (part, entry) in node
            .get("inherited_parts")
            .map(Node::entries)
            .unwrap_or_default()
        {
            inherited_parts.push((
                part.clone(),
                self.cascade(entry.get("cascade"))?,
                entry.get("survives").and_then(Node::boolean) == Some(true),
            ));
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
            let is_final = self.flag(spec, "final")?;
            // a state acts unless it is final or says it does not, which
            // only a state that is not final may say (ADR-0241)
            let acts = !is_final && (spec.get("acts").is_none() || self.flag(spec, "acts")?);
            states.push((
                n.clone(),
                State {
                    category: spec
                        .get("category")
                        .and_then(Node::text)
                        .unwrap_or("")
                        .to_string(),
                    is_final,
                    required_attributes: self.names(spec.get("required_attributes"))?,
                    superseding: self.flag(spec, "superseding")?,
                    acts,
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
            inherited_parts,
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
            not_blank: false,
            media_types: Vec::new(),
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
                offered_on: Vec::new(),
                proposable: false,
                required_inputs: required.clone(),
                optional_inputs: optional.clone(),
                clearable_inputs: Vec::new(),
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
                not_blank: false,
                media_types: Vec::new(),
            };
            transitions.push((
                "forget".into(),
                Transition {
                    description: None,
                    kind: Kind::Erasure,
                    from: None,
                    to: Vec::new(),
                    only_via: Vec::new(),
                    offered_on: Vec::new(),
                    proposable: false,
                    required_inputs: Vec::new(),
                    optional_inputs: Vec::new(),
                    clearable_inputs: Vec::new(),
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
                    acts: false,
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
            inherited_parts: Vec::new(),
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
        let mut values = Vec::new();
        for (n, spec) in doc.get("values").map(Node::entries).unwrap_or_default() {
            let mut fields = Vec::new();
            for (f, field) in spec
                .get("attributes")
                .map(Node::entries)
                .unwrap_or_default()
            {
                fields.push((f.clone(), self.attribute(field)?));
            }
            let mut invariants = Vec::new();
            for (i, inv) in spec
                .get("invariants")
                .map(Node::entries)
                .unwrap_or_default()
            {
                let Some(e) = inv.get("expression") else {
                    return self.error(inv, "an invariant has an expression");
                };
                let text = match (e.boolean(), e.integer()) {
                    (Some(b), _) => b.to_string(),
                    (_, Some(n)) => n.to_string(),
                    _ => self.text(e, "an expression")?,
                };
                invariants.push((i.clone(), text));
            }
            values.push(ValueType {
                name: n.clone(),
                module: name.to_string(),
                description: spec
                    .get("description")
                    .and_then(Node::text)
                    .unwrap_or_default()
                    .to_string(),
                fields,
                invariants,
                source: spec.to_json(),
            });
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
        let publishing = doc
            .get("publishing")
            .and_then(|p| p.get("taken_by"))
            .map(|kinds| {
                let mut kinds: Vec<String> = kinds
                    .items()
                    .iter()
                    .filter_map(|k| k.text().map(str::to_string))
                    .collect();
                kinds.sort();
                kinds
            });
        let sequences = doc
            .get("sequences")
            .map(Node::entries)
            .unwrap_or_default()
            .iter()
            .map(|(n, _)| n.clone())
            .collect();
        Ok(Module {
            name: name.to_string(),
            publishing,
            sequences,
            migration: self.migration(doc.get("migration"))?,
            enumerations,
            values,
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
    /// An enumeration the version renames, its old name and its new, every
    /// attribute typed by it carried with its values, each member kept by
    /// its name (ADR-0247 decision 1).
    pub renamed_enumerations: Vec<(String, String)>,
    /// A type, an attribute's old name, and its new one.
    pub renamed_attributes: Vec<(String, String, String)>,
    /// A type the version removes, and the entries that move its objects
    /// into types of the version, in the order written (ADR-0229).
    pub moved_types: Vec<(String, Vec<MovedTo>)>,
    /// A type the version removes and retires on purpose, and why
    /// (ADR-0229 decision 1).
    pub retired_types: Vec<(String, String)>,
    /// A type, an attribute, and the value written to it, in the order
    /// written.
    pub backfill: Vec<(String, String, Expr)>,
    /// A type, an attribute, and the condition on which an object's value
    /// of it is cleared, in the order written, after the backfills
    /// (ADR-0268).
    pub clear: Vec<(String, String, Expr)>,
    /// An invariant, `Type.name`, a violation of which is admitted, and the
    /// reason, an enumeration member written `Enum.MEMBER`.
    pub admit: Vec<(String, String)>,
}

impl Migration {
    fn extend(&mut self, other: Migration) {
        self.removed_states.extend(other.removed_states);
        self.removed_members.extend(other.removed_members);
        self.renamed_enumerations.extend(other.renamed_enumerations);
        self.renamed_attributes.extend(other.renamed_attributes);
        self.moved_types.extend(other.moved_types);
        self.retired_types.extend(other.retired_types);
        self.backfill.extend(other.backfill);
        self.clear.extend(other.clear);
        self.admit.extend(other.admit);
    }

    pub fn is_empty(&self) -> bool {
        *self == Migration::default()
    }

    /// The name an enumeration of the version before takes in this one: its
    /// new name where the version renames it (ADR-0247 decision 1).
    pub fn enumeration_now<'m>(&'m self, was: &'m str) -> &'m str {
        self.renamed_enumerations
            .iter()
            .find(|(old, _)| old == was)
            .map_or(was, |(_, new)| new.as_str())
    }

    /// The name this version's enumeration had in the version before: its
    /// old name where the version renames it.
    pub fn enumeration_was<'m>(&'m self, now: &'m str) -> &'m str {
        self.renamed_enumerations
            .iter()
            .find(|(_, new)| new == now)
            .map_or(now, |(old, _)| old.as_str())
    }

    /// The entries that move a type's objects, where the version moves it.
    pub fn moved(&self, type_: &str) -> Option<&[MovedTo]> {
        self.moved_types
            .iter()
            .find(|(t, _)| t == type_)
            .map(|(_, entries)| entries.as_slice())
    }
}

/// One entry of a removed type's `moved_types`: the type its objects move
/// into, which of them, and what they hold there (ADR-0229 decision 2).
#[derive(Clone, Debug, PartialEq)]
pub struct MovedTo {
    pub to: String,
    /// A condition on the old object; none takes every object.
    pub where_: Option<Expr>,
    /// Each attribute of the target and the expression giving it.
    pub attributes: Vec<(String, Expr)>,
    /// The members each attribute's expression reads, as the checker reads
    /// them, so a publish over an installed version lacking one gives the
    /// attribute nothing (ADR-0256 decision 1).
    pub reads: BTreeMap<String, BTreeSet<String>>,
    /// Each old state and the target's state its objects enter.
    pub states: Vec<(String, String)>,
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
        m.renamed_enumerations = node
            .get("renamed_enumerations")
            .map(Node::entries)
            .unwrap_or_default()
            .iter()
            .filter_map(|(old, new)| new.text().map(|n| (old.clone(), n.to_string())))
            .collect();
        m.renamed_attributes = pairs("renamed_attributes");
        for (tn, entries) in node
            .get("moved_types")
            .map(Node::entries)
            .unwrap_or_default()
        {
            let mut out = Vec::new();
            for e in entries.items() {
                let Some(to) = e.get("to").and_then(Node::text) else {
                    return self.error(e, "a moved type's entry names the type it moves to");
                };
                let (mut attributes, mut reads) = (Vec::new(), BTreeMap::new());
                for (a, x) in e.get("attributes").map(Node::entries).unwrap_or_default() {
                    attributes.push((a.clone(), self.expression(x)?));
                    reads.insert(
                        a.clone(),
                        x.text().map(crate::check::member_reads).unwrap_or_default(),
                    );
                }
                out.push(MovedTo {
                    to: to.to_string(),
                    where_: e.get("where").map(|w| self.expression(w)).transpose()?,
                    attributes,
                    reads,
                    states: e
                        .get("states")
                        .map(Node::entries)
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|(old, new)| new.text().map(|n| (old.clone(), n.to_string())))
                        .collect(),
                });
            }
            m.moved_types.push((tn.clone(), out));
        }
        m.retired_types = node
            .get("retired_types")
            .map(Node::entries)
            .unwrap_or_default()
            .iter()
            .filter_map(|(tn, why)| why.text().map(|w| (tn.clone(), w.to_string())))
            .collect();
        for (tn, map) in node.get("backfill").map(Node::entries).unwrap_or_default() {
            for (a, e) in map.entries() {
                m.backfill
                    .push((tn.clone(), a.clone(), self.expression(e)?));
            }
        }
        for (tn, map) in node.get("clear").map(Node::entries).unwrap_or_default() {
            for (a, e) in map.entries() {
                m.clear.push((tn.clone(), a.clone(), self.expression(e)?));
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
    /// The values the modules declare, by name (ADR-0254).
    pub values: BTreeMap<String, ValueType>,
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
    /// The kinds of actor that approve the store's next version, as its
    /// modules declare them, sorted; none where no module does, when the
    /// gateway alone decides (ADR-0187 decision 1).
    pub publishing: Option<Vec<String>>,
    /// The sequences its modules declare, which an import may raise
    /// (`flow-format.md` §4.12, fourth production review A-15).
    pub sequences: BTreeSet<String>,
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
            .chain(kinds.iter().map(|k| format!("{a} == ActorKind.{k}")))
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
                not_blank: false,
                media_types: Vec::new(),
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
    // a part a base declares goes with the subtype's final transitions as
    // its `inherited_parts` says (`flow-format.md` §4.14)
    for (part, cascade, survives) in &t.inherited_parts {
        if let Some((_, a)) = attributes.iter_mut().find(|(n, _)| n == part) {
            a.cascade = cascade.clone();
            a.survives_always = *survives;
        }
    }
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

/// The keys of a target's input or attribute a derived input keeps
/// (ADR-0271 decision 2), as the checks' view keeps them.
const INPUT_KEYS: [&str; 8] = [
    "type",
    "reference",
    "optional",
    "personal",
    "not_blank",
    "media_types",
    "default",
    "description",
];

/// A type with each `rest: inputs` of its creates expanded (ADR-0271): each
/// input of the created transition the step does not name becomes an input
/// of the transition making it, declared as the target declares it,
/// optional where the target does not require it or gives it a default,
/// and the step passes it as `inputs.<name>`. An observation kind's
/// `subject` and `corrects` are never derived. A name the transition takes
/// itself, or one two creates derive unlike, is left underived, which step
/// 3 refuses (`names`). The declaration served gives each derived input
/// beside the act's own, with `derived_from` naming its target.
fn with_rest(t: &mut Type, types: &BTreeMap<String, Type>) {
    let mut served = Vec::new();
    for (xn, x) in &mut t.transitions {
        let own: BTreeSet<String> = x
            .required_inputs
            .iter()
            .chain(&x.optional_inputs)
            .chain(x.inputs.iter().map(|(n, _)| n))
            .cloned()
            .collect();
        let mut derived = Vec::new();
        expand_rest(&mut x.effect, &own, types, &mut derived);
        for (j, input, spec) in derived {
            x.inputs.push((j.clone(), input));
            served.push((xn.clone(), j, spec));
        }
    }
    for (xn, j, spec) in served {
        if let Some(x) = t
            .source
            .get_mut("transitions")
            .and_then(|xs| xs.get_mut(&xn))
            .and_then(serde_json::Value::as_object_mut)
        {
            let inputs = x.entry("inputs").or_insert_with(|| serde_json::json!({}));
            if let Some(m) = inputs.as_object_mut() {
                m.insert(j, spec);
            }
        }
    }
}

/// Whether a step, a loop's included, is a create taking `rest: inputs`.
fn rests(steps: &[Step]) -> bool {
    steps.iter().any(|st| match st {
        Step::Create { rest, .. } => *rest,
        Step::Foreach { steps, .. } => rests(steps),
        _ => false,
    })
}

/// `with_rest` over a list of steps, a loop's included (ADR-0271 decision
/// 5): each derived input with its model and its served declaration.
fn expand_rest(
    steps: &mut [Step],
    own: &BTreeSet<String>,
    types: &BTreeMap<String, Type>,
    derived: &mut Vec<(String, Input, serde_json::Value)>,
) {
    for st in steps {
        match st {
            Step::Foreach { steps, .. } => expand_rest(steps, own, types, derived),
            Step::Create {
                type_,
                transition,
                inputs,
                rest,
                ..
            } if *rest => {
                *rest = false;
                let Some(k) = types.get(type_.as_str()) else {
                    continue;
                };
                let Some(y) = k.transition(transition) else {
                    continue;
                };
                let given: BTreeSet<String> = inputs.iter().map(|(n, _)| n.clone()).collect();
                let names: BTreeSet<&String> = y
                    .required_inputs
                    .iter()
                    .chain(&y.optional_inputs)
                    .chain(y.inputs.iter().map(|(n, _)| n))
                    .collect();
                let mark = format!("{type_}.{transition}");
                for j in names {
                    if given.contains(j)
                        || (k.observation_on.is_some() && (j == "subject" || j == "corrects"))
                    {
                        continue;
                    }
                    let Some((input, spec)) = derived_input(k, y, transition, j) else {
                        continue;
                    };
                    if own.contains(j) {
                        continue;
                    }
                    match derived.iter().find(|(n, _, _)| n == j) {
                        Some((_, _, earlier)) => {
                            let mut alike = earlier.clone();
                            if let Some(m) = alike.as_object_mut() {
                                m.remove("derived_from");
                            }
                            if alike != spec {
                                continue;
                            }
                        }
                        None => {
                            let mut marked = spec;
                            marked["derived_from"] = serde_json::json!(mark);
                            derived.push((j.clone(), input, marked));
                        }
                    }
                    inputs.push((
                        j.clone(),
                        Expr::Path(Box::new(Expr::Name("inputs".into())), j.clone()),
                    ));
                }
            }
            _ => {}
        }
    }
}

/// The input of `k`'s transition `y` named `j`, as an input of the act
/// deriving it: the target's declared input, or the attribute it writes,
/// required only where the target requires it and gives no default; with
/// its served declaration, the target's keys `INPUT_KEYS` holds.
fn derived_input(
    k: &Type,
    y: &Transition,
    transition: &str,
    j: &str,
) -> Option<(Input, serde_json::Value)> {
    let (input, written) = match y.inputs.iter().find(|(n, _)| n == j) {
        Some((_, i)) => (
            Input {
                optional: i.optional || i.default.is_some(),
                default: None,
                ..i.clone()
            },
            k.source
                .get("transitions")
                .and_then(|xs| xs.get(transition))
                .and_then(|x| x.get("inputs"))
                .and_then(|i| i.get(j)),
        ),
        None => {
            let a = k.attribute(j)?;
            (
                Input {
                    declared: a.declared.clone(),
                    many: a.many,
                    optional: !y.required_inputs.iter().any(|n| n == j),
                    default: None,
                    personal: a.personal,
                    not_blank: a.not_blank,
                    media_types: a.media_types.clone(),
                },
                k.source.get("attributes").and_then(|a| a.get(j)),
            )
        }
    };
    let mut spec = serde_json::Map::new();
    if let Some(w) = written.and_then(serde_json::Value::as_object) {
        for (key, v) in w {
            if INPUT_KEYS.contains(&key.as_str()) && key != "optional" && key != "default" {
                spec.insert(key.clone(), v.clone());
            }
        }
    }
    if input.optional {
        spec.insert("optional".into(), serde_json::json!(true));
    }
    Some((input, serde_json::Value::Object(spec)))
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
        offered_on: Vec::new(),
        proposable: false,
        required_inputs: Vec::new(),
        optional_inputs: Vec::new(),
        clearable_inputs: Vec::new(),
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
                not_blank: false,
                media_types: Vec::new(),
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
                    acts: true,
                },
            ),
            (
                "RETIRED".to_string(),
                State {
                    category: "closed".to_string(),
                    is_final: true,
                    required_attributes: Vec::new(),
                    superseding: false,
                    acts: false,
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
        inherited_parts: Vec::new(),
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
        not_blank: false,
        media_types: Vec::new(),
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
        acts: !is_final,
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
        inherited_parts: Vec::new(),
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
        acts: !is_final,
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

/// The widest an identifier's number is padded, `{n:<width>}`: an `i64`
/// has at most 19 digits (tenth production review).
pub const IDENTIFIER_WIDTH: usize = 20;

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
        not_blank: false,
        media_types: Vec::new(),
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
            offered_on: Vec::new(),
            proposable: false,
            required_inputs: required.iter().map(|s| s.to_string()).collect(),
            optional_inputs: optional.iter().map(|s| s.to_string()).collect(),
            clearable_inputs: Vec::new(),
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
        not_blank: false,
        media_types: Vec::new(),
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
                acts: false,
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
        inherited_parts: Vec::new(),
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
        not_blank: false,
        media_types: Vec::new(),
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
        acts: !is_final,
    };
    let transition =
        |kind: Kind, from: Option<From>, to: &str, required: &[&str], optional: &[&str]| {
            Transition {
                description: None,
                kind,
                from,
                to: vec![to.to_string()],
                only_via: Vec::new(),
                offered_on: Vec::new(),
                proposable: false,
                required_inputs: required.iter().map(|s| s.to_string()).collect(),
                optional_inputs: optional.iter().map(|s| s.to_string()).collect(),
                clearable_inputs: Vec::new(),
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
        inherited_parts: Vec::new(),
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
                .chain(m.values.iter().map(|v| ("value", v.name.as_str())))
                .chain(m.machines.iter().map(|t| ("machine", t.name.as_str())));
            for (kind, n) in names {
                if let Some(other) = owner.insert(n.to_string(), m.name.clone())
                    && other != m.name
                {
                    return Err(LoadError {
                        file: file.clone(),
                        line: declared_line(text, n),
                        message: format!(
                            "the {kind} {n} is also declared by {other}; a store holds one type, enumeration, value or machine of a name (ADR-0163)"
                        ),
                    });
                }
            }
            // who approves a version, declared alike by every module that
            // declares it (ADR-0187 decision 1)
            if let Some(kinds) = &m.publishing {
                match &lib.publishing {
                    Some(theirs) if theirs != kinds => {
                        return Err(LoadError {
                            file: file.clone(),
                            line: text
                                .lines()
                                .position(|l| l.starts_with("publishing:"))
                                .map_or(1, |i| i + 1),
                            message: format!(
                                "publishing.taken_by is [{}], where another module declares [{}]; a version's approvers are declared alike by every module that declares them (ADR-0187)",
                                kinds.join(", "),
                                theirs.join(", ")
                            ),
                        });
                    }
                    _ => lib.publishing = Some(kinds.clone()),
                }
            }
            lib.sequences.extend(m.sequences);
            lib.source.push((m.name.clone(), text.clone()));
            lib.migration.extend(m.migration);
            for (n, members) in m.enumerations {
                lib.enumerations.insert(n, members);
            }
            for v in m.values {
                lib.values.insert(v.name.clone(), v);
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
        // an attribute or an input typed by a value names it, once every
        // module, and so every value one may import, is read, and before a
        // subtype inherits it (ADR-0254)
        let values: BTreeSet<String> = lib.values.keys().cloned().collect();
        let as_value = |d: &mut Declared| {
            if let Declared::Scalar(n) = d
                && values.contains(n.as_str())
            {
                let n = n.clone();
                *d = Declared::Value(n);
            }
        };
        for t in lib.types.values_mut() {
            for (_, a) in &mut t.attributes {
                as_value(&mut a.declared);
            }
            for (_, x) in &mut t.transitions {
                for (_, i) in &mut x.inputs {
                    as_value(&mut i.declared);
                }
            }
        }
        // a type's own invariant named as one a base's attribute generates,
        // `<attribute>_not_blank` or a value's `<attribute>_<invariant>`, is
        // refused as one named after its own attribute's is, where it took
        // the generated one's place in the subtype and a text marked never
        // blank was written blank there; an attribute the type declares
        // again is its own (ADR-0257 decision 3, ADR-0255)
        for t in lib.types.values() {
            for b in bases(&t.name, &lib.types) {
                let Some(bt) = lib.types.get(&b) else {
                    continue;
                };
                for (a, attr) in &bt.attributes {
                    if t.attributes.iter().any(|(n, _)| n == a) {
                        continue;
                    }
                    // each name, and what generates it, as the checkers say
                    let mut generated = Vec::new();
                    if attr.not_blank {
                        generated.push((
                            format!("{a}_not_blank"),
                            format!("{b}.{a}'s not_blank generates"),
                            "ADR-0257",
                        ));
                    }
                    if let Declared::Value(vn) = &attr.declared
                        && let Some(vt) = lib.values.get(vn)
                    {
                        for (i, _) in &vt.invariants {
                            generated.push((
                                format!("{a}_{i}"),
                                format!("{b}.{a} holds of its value {vn}'s {i}"),
                                "ADR-0255",
                            ));
                        }
                    }
                    for (name, from, adr) in generated {
                        if t.invariants.iter().any(|(n, _)| *n == name) {
                            let (file, line) = files
                                .iter()
                                .find(|(_, text)| {
                                    crate::source::name_of(text).as_deref()
                                        == Some(t.module.as_str())
                                })
                                .map_or((String::new(), 1), |(f, text)| {
                                    (f.clone(), declared_line(text, &t.name))
                                });
                            return Err(LoadError {
                                file,
                                line,
                                message: format!(
                                    "{}.{name} is the invariant {from}, which {} inherits; name the type's invariant otherwise ({adr})",
                                    t.name, t.name
                                ),
                            });
                        }
                    }
                }
            }
        }
        // each value's invariant, held by each attribute holding the value
        // as its type's invariant `<attribute>_<invariant>`, after the type's
        // own and before a subtype inherits it (ADR-0255)
        let declared_values = lib.values.clone();
        for t in lib.types.values_mut() {
            let mut held = Vec::new();
            for (a, attr) in &t.attributes {
                let Declared::Value(vn) = &attr.declared else {
                    continue;
                };
                let Some(vt) = declared_values.get(vn) else {
                    continue;
                };
                let fields: BTreeSet<String> = vt.fields.iter().map(|(f, _)| f.clone()).collect();
                for (i, text) in &vt.invariants {
                    let name = format!("{a}_{i}");
                    let at = |message: String| {
                        let (file, line) = files
                            .iter()
                            .find(|(_, text)| {
                                crate::source::name_of(text).as_deref() == Some(t.module.as_str())
                            })
                            .map_or((String::new(), 1), |(f, text)| {
                                (f.clone(), declared_line(text, &t.name))
                            });
                        LoadError {
                            file,
                            line,
                            message,
                        }
                    };
                    if t.invariants.iter().any(|(n, _)| *n == name) {
                        return Err(at(format!(
                            "{}.{name} is the invariant {a} holds of its value {vn}'s {i}; name the type's invariant otherwise (ADR-0255)",
                            t.name
                        )));
                    }
                    let expr = expr::held_through(text, &fields, a)
                        .and_then(|e| expr::parse(&e))
                        .map_err(|e| {
                            at(format!(
                                "{vn}.{i}: {e}: not an expression of declaration-syntax.md §8"
                            ))
                        })?;
                    held.push((name, expr));
                }
            }
            t.invariants.extend(held);
        }
        // each text marked `not_blank`, held by the invariant
        // `<attribute>_not_blank`, after the type's own and before a subtype
        // inherits it, so every write and a publish over the values held are
        // checked as any invariant is (ADR-0257 decision 3)
        for t in lib.types.values_mut() {
            let mut held = Vec::new();
            for (a, _) in t.attributes.iter().filter(|(_, x)| x.not_blank) {
                let name = format!("{a}_not_blank");
                let at = |message: String| {
                    let (file, line) = files
                        .iter()
                        .find(|(_, text)| {
                            crate::source::name_of(text).as_deref() == Some(t.module.as_str())
                        })
                        .map_or((String::new(), 1), |(f, text)| {
                            (f.clone(), declared_line(text, &t.name))
                        });
                    LoadError {
                        file,
                        line,
                        message,
                    }
                };
                if t.invariants.iter().any(|(n, _)| *n == name) {
                    return Err(at(format!(
                        "{}.{name} is the invariant {a}'s not_blank generates; name the type's invariant otherwise (ADR-0257)",
                        t.name
                    )));
                }
                let expr = expr::parse(&not_blank_expression(a))
                    .map_err(|e| at(format!("{}.{name}: {e}", t.name)))?;
                held.push((name, expr));
            }
            t.invariants.extend(held);
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
        // each create's `rest: inputs` expanded into inputs of the act making
        // it, once each target holds what it inherits (ADR-0271 decision 8)
        if lib
            .types
            .values()
            .any(|t| t.transitions.iter().any(|(_, x)| rests(&x.effect)))
        {
            let inherited = lib.types.clone();
            for t in lib.types.values_mut() {
                with_rest(t, &inherited);
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
                // nor the column a value's field is kept in (ADR-0254)
                let field_clash = a.split_once("__").is_some_and(|(base, f)| {
                    t.attribute(base).is_some_and(|v| match &v.declared {
                        Declared::Value(vn) => {
                            lib.values.get(vn).is_some_and(|vt| vt.field(f).is_some())
                        }
                        _ => false,
                    })
                });
                if field_clash {
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
                            "{}.{a} is the column the store keeps a field of a value in; name the attribute otherwise (ADR-0254, as D578)",
                            t.name
                        ),
                    });
                }
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

    /// A part an abstract base declares takes, on each subtype, the cascade
    /// and survives its `inherited_parts` entry gives (`flow-format.md`
    /// §4.14), where such a subtype was named beyond the slice and every
    /// request of it not run (the values audit's K-11).
    #[test]
    fn an_inherited_part_goes_with_each_subtype_as_it_says() {
        const NOTES: &str = r#"
module: notes
categories: [live, closed]
types:
  Carrier:
    abstract: true
    tracking: record
    attributes:
      notes: { reference: "Note[]", aggregation: composite, opposite: subject }
  Ticket:
    extends: Carrier
    inherited_parts:
      notes: { cascade: [ { on: [close], transition: file, limit: 100 } ] }
    states: { OPEN: { category: live }, CLOSED: { category: closed, final: true } }
    transitions:
      raise: { kind: initial, to: OPEN }
      close: { kind: external, from: OPEN, to: CLOSED }
  Folder:
    extends: Carrier
    inherited_parts:
      notes: { survives: true }
    states: { OPEN: { category: live }, GONE: { category: closed, final: true } }
    transitions:
      make: { kind: initial, to: OPEN }
      drop: { kind: external, from: OPEN, to: GONE }
  Note:
    tracking: record
    attributes:
      subject: { reference: Carrier, opposite: notes }
    states: { ACTIVE: { category: live }, FILED: { category: closed, final: true } }
    transitions:
      add: { kind: initial, to: ACTIVE, required_inputs: [subject] }
      file: { kind: external, from: ACTIVE, to: FILED }
"#;
        let lib = Library::load(&[("notes.yaml".into(), NOTES.into())]).unwrap();
        let ticket = lib.type_("Ticket").unwrap();
        assert!(ticket.beyond.is_empty());
        let notes = ticket.attribute("notes").unwrap();
        assert_eq!(notes.cascade.len(), 1);
        assert_eq!(
            (
                notes.cascade[0].on.as_slice(),
                notes.cascade[0].transition.as_str()
            ),
            (["close".to_string()].as_slice(), "file")
        );
        assert!(!notes.survives_always);
        let folder = lib.type_("Folder").unwrap().attribute("notes").unwrap();
        assert!(folder.survives_always && folder.cascade.is_empty());
        // the base's own end is as declared
        let base = lib.type_("Carrier").unwrap().attribute("notes").unwrap();
        assert!(base.cascade.is_empty() && !base.survives_always);
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

    /// A create's `rest: inputs` is expanded at load into inputs of the act
    /// (ADR-0271): each is declared as its target declares it, required
    /// only where the target requires it with no default, personal and of
    /// its media types as there; the step passes each as `inputs.<name>`, a
    /// loop's step too; `subject` is never derived; a name the act takes
    /// itself, or one a second create derives unlike, is left underived;
    /// and the declaration served marks each with its target.
    #[test]
    fn rest_inputs_derives_the_targets_inputs() {
        let text = r#"
module: moves
categories: [live, closed]
types:
  Thing:
    attributes:
      label: { type: string }
    observations:
      custody:
        kind: Custody
        attributes:
          to_whom: { type: string, description: Who takes it. }
          note: { type: string, optional: true, personal: true }
          scan: { type: file, optional: true, media_types: [application/pdf] }
    states: { ON: { category: live } }
    transitions:
      add:
        kind: initial
        to: ON
        required_inputs: [label]
        inputs:
          size: { type: int, default: 1 }
          note: { type: int, optional: true }
  Mover:
    attributes:
      thing: { reference: Thing }
    states: { ON: { category: live } }
    transitions:
      add: { kind: initial, to: ON, required_inputs: [thing] }
      move:
        kind: internal
        from: ON
        effect:
          - create: { type: Custody, transition: record, inputs: { subject: thing }, rest: inputs }
      unnamed:
        kind: internal
        from: ON
        effect:
          - create: { type: Custody, transition: record, rest: inputs }
      spawn:
        kind: internal
        from: ON
        inputs:
          label: { type: string }
        effect:
          - foreach: { item: i, range: "2", limit: 5, steps: [ { create: { type: Thing, transition: add, rest: inputs } } ] }
      both:
        kind: internal
        from: ON
        effect:
          - create: { type: Custody, transition: record, inputs: { subject: thing }, rest: inputs }
          - create: { type: Thing, transition: add, rest: inputs }
"#;
        let lib = Library::load(&[("moves.yaml".into(), text.into())]).unwrap();
        let t = lib.type_("Mover").unwrap();
        let x = |n: &str| t.transition(n).unwrap();
        let names =
            |n: &str| -> Vec<String> { x(n).inputs.iter().map(|(k, _)| k.clone()).collect() };
        let input = |n: &str, k: &str| x(n).inputs.iter().find(|(j, _)| j == k).unwrap().1.clone();
        let passed = |st: &Step| -> (Vec<String>, bool) {
            match st {
                Step::Create { inputs, rest, .. } => {
                    (inputs.iter().map(|(k, _)| k.clone()).collect(), *rest)
                }
                Step::Foreach { steps, .. } => match &steps[..] {
                    [Step::Create { inputs, rest, .. }] => {
                        (inputs.iter().map(|(k, _)| k.clone()).collect(), *rest)
                    }
                    other => panic!("{other:?}"),
                },
                other => panic!("{other:?}"),
            }
        };
        // each input of the record but subject and corrects, as declared there
        assert_eq!(names("move"), ["note", "scan", "to_whom"]);
        let to_whom = input("move", "to_whom");
        assert!(!to_whom.optional && to_whom.declared == Declared::Scalar("string".into()));
        let note = input("move", "note");
        assert!(note.optional && note.personal);
        assert_eq!(input("move", "scan").media_types, ["application/pdf"]);
        assert_eq!(
            passed(&x("move").effect[0]),
            (
                vec![
                    "subject".into(),
                    "note".into(),
                    "scan".into(),
                    "to_whom".into()
                ],
                false
            )
        );
        assert!(matches!(&x("move").effect[0],
            Step::Create { inputs, .. } if matches!(&inputs[3].1,
                Expr::Path(b, n) if n == "to_whom" && matches!(b.as_ref(), Expr::Name(i) if i == "inputs"))));
        // subject is never derived, where the step names none
        assert_eq!(names("unnamed"), ["note", "scan", "to_whom"]);
        // within a loop; a name the act takes itself is left underived, and a
        // defaulted input is optional, its default the target's alone
        assert_eq!(names("spawn"), ["label", "note", "size"]);
        let size = input("spawn", "size");
        assert!(size.optional && size.default.is_none());
        assert_eq!(
            passed(&x("spawn").effect[0]),
            (vec!["note".into(), "size".into()], false)
        );
        // a name a second create derives unlike is left to the first
        assert_eq!(names("both"), ["note", "scan", "to_whom", "label", "size"]);
        assert!(input("both", "note").personal);
        assert_eq!(
            passed(&x("both").effect[1]),
            (vec!["label".into(), "size".into()], false)
        );
        // the declaration served marks each derived input with its target,
        // and keeps the create as written
        let served = &t.source["transitions"]["move"];
        assert_eq!(
            served["inputs"]["to_whom"],
            serde_json::json!({"type": "string", "description": "Who takes it.", "derived_from": "Custody.record"})
        );
        assert_eq!(
            served["inputs"]["note"],
            serde_json::json!({"type": "string", "personal": true, "optional": true, "derived_from": "Custody.record"})
        );
        assert_eq!(served["effect"][0]["create"]["rest"], "inputs");
        assert_eq!(
            t.source["transitions"]["spawn"]["inputs"]["size"],
            serde_json::json!({"type": "int", "optional": true, "derived_from": "Thing.add"})
        );
        assert_eq!(
            t.source["transitions"]["spawn"]["inputs"]["label"],
            serde_json::json!({"type": "string"})
        );
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
