//! Step 4 of the flow checker: the sixty-four checks of
//! `declaration-syntax.md` §10, run on the core's reading of the modules,
//! never on the internal text form (ADR-0156 decision 1).
//!
//! The reference runs them in `scripts/check-syntax-doc.py`, over the text
//! `check-flows.py`'s `to_text` converts each module to, and maps each
//! finding back to the YAML by the line it is reported at. Each check here
//! is the same rule read from the YAML's meaning, reported at the YAML path
//! its text line stands for, with the text checker's own words, since it
//! drops a finding repeating an earlier one's check and words.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::doc::{Path, flag, list, obj, py_str, segs, squash, str_or, strings, text};
use super::steps::{kind, sources, targets, walk};
use super::view::types;

/// A finding of step 4: its check's number, what it says, the module it is
/// in and the YAML path its text line stands for.
pub struct Found4 {
    pub check: u32,
    pub message: String,
    pub module: usize,
    pub path: Path,
}

/// What a declaration is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum DeclKind {
    Type,
    Machine,
}

/// A state, as `state <S> category <c> [terminal] [superseding]` says it.
#[derive(Clone)]
pub struct StateD {
    pub name: String,
    pub category: String,
    pub terminal: bool,
    pub superseding: bool,
    pub module: usize,
    pub path: Path,
}

/// A transition's kind, as the text's head word says it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Head {
    Create,
    Do,
    Act,
    Assert,
    Erase,
}

/// A transition, the YAML of it, and where it is.
#[derive(Clone)]
pub struct TransD {
    pub head: Head,
    pub name: String,
    pub x: Value,
    pub module: usize,
    pub path: Path,
}

/// A type or a machine, as the text checker's `Decl` holds it.
#[derive(Clone)]
pub struct Decl {
    pub kind: DeclKind,
    pub name: String,
    pub module: usize,
    pub path: Path,
    pub abstract_: bool,
    pub mirror: bool,
    pub base: Option<String>,
    pub machine: Option<String>,
    pub tracking: Option<String>,
    pub states: Vec<StateD>,
    pub trans: Vec<TransD>,
    /// The attributes that are neither references nor counters.
    pub attrs: Vec<(String, Value)>,
    pub counters: BTreeSet<String>,
    /// The references, each a part, an owner or a ref.
    pub rels: Vec<RelD>,
    /// The YAML of the declaration, for what the checks read beyond the above.
    pub yaml: Value,
}

/// A reference's end, as the conversion writes it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RelKind {
    Part,
    Owner,
    Ref,
}

/// A reference attribute: its kind, its spec and where it is.
#[derive(Clone)]
pub struct RelD {
    pub name: String,
    pub kind: RelKind,
    pub spec: Value,
    pub module: usize,
    pub path: Path,
}

impl RelD {
    /// The type it names, without `[]`.
    pub fn target(&self) -> String {
        str_or(&self.spec, "reference", "")
            .trim_end_matches(['[', ']'])
            .to_string()
    }

    pub fn is_set(&self) -> bool {
        str_or(&self.spec, "reference", "").ends_with("[]")
    }

    pub fn optional(&self) -> bool {
        flag(&self.spec, "optional")
    }

    pub fn inverse(&self) -> Option<String> {
        self.spec
            .get("opposite")
            .filter(|o| super::doc::truthy(Some(o)))
            .map(py_str)
    }
}

fn head_of(x: &Value) -> Head {
    match kind(x) {
        "initial" => Head::Create,
        "external" => Head::Do,
        "internal" => Head::Act,
        "assertion" => Head::Assert,
        _ => Head::Erase,
    }
}

fn states_of(section: &Value, module: usize, at: &Path) -> Vec<StateD> {
    obj(section, "states")
        .iter()
        .map(|(s, v)| StateD {
            name: s.clone(),
            category: str_or(v, "category", ""),
            terminal: flag(v, "final"),
            superseding: flag(v, "superseding"),
            module,
            path: super::doc::ext(at, segs!["states", s]),
        })
        .collect()
}

fn trans_of(section: &Value, module: usize, at: &Path) -> Vec<TransD> {
    obj(section, "transitions")
        .iter()
        .map(|(n, x)| TransD {
            head: head_of(x),
            name: n.clone(),
            x: x.clone(),
            module,
            path: super::doc::ext(at, segs!["transitions", n]),
        })
        .collect()
}

/// Every type and machine of the modules, by name, as the text checker's
/// `world` holds them across every module converted.
pub fn decls(docs: &[Value]) -> Vec<Decl> {
    let mut out = Vec::new();
    for (module, doc) in docs.iter().enumerate() {
        for (mn, m) in obj(doc, "machines") {
            let at = segs!["machines", mn];
            out.push(Decl {
                kind: DeclKind::Machine,
                name: mn.clone(),
                module,
                path: at.clone(),
                abstract_: false,
                mirror: false,
                base: None,
                machine: None,
                tracking: None,
                states: states_of(m, module, &at),
                trans: trans_of(m, module, &at),
                attrs: Vec::new(),
                counters: BTreeSet::new(),
                rels: Vec::new(),
                yaml: m.clone(),
            });
        }
        for (tn, t) in types(doc) {
            let at = segs!["types", &tn];
            let mut attrs = Vec::new();
            let mut counters = BTreeSet::new();
            let mut rels = Vec::new();
            for (a, spec) in obj(t, "attributes") {
                if let Some(r) = spec.get("reference") {
                    // an end whose far end, in this module, is composite is an owner
                    let far = obj(doc, "types")
                        .get(py_str(r).trim_end_matches(['[', ']']))
                        .cloned()
                        .unwrap_or(Value::Null);
                    let opposite = spec
                        .get("opposite")
                        .filter(|o| super::doc::truthy(Some(o)))
                        .map(py_str);
                    let owner = opposite
                        .as_ref()
                        .and_then(|o| obj(&far, "attributes").get(o))
                        .is_some_and(|e| text(e, "aggregation") == Some("composite"));
                    let kind = if text(spec, "aggregation") == Some("composite") {
                        RelKind::Part
                    } else if owner {
                        RelKind::Owner
                    } else {
                        RelKind::Ref
                    };
                    rels.push(RelD {
                        name: a.clone(),
                        kind,
                        spec: spec.clone(),
                        module,
                        path: super::doc::ext(&at, segs!["attributes", a]),
                    });
                    continue;
                }
                if text(spec, "type") == Some("counter") {
                    counters.insert(a.clone());
                } else {
                    attrs.push((a.clone(), spec.clone()));
                }
            }
            out.push(Decl {
                kind: DeclKind::Type,
                name: tn.clone(),
                module,
                path: at.clone(),
                abstract_: flag(t, "abstract"),
                mirror: flag(t, "mirror"),
                base: t
                    .get("extends")
                    .filter(|b| super::doc::truthy(Some(b)))
                    .map(py_str),
                machine: t
                    .get("state_machine")
                    .filter(|m| super::doc::truthy(Some(m)))
                    .map(py_str),
                tracking: t
                    .get("tracking")
                    .filter(|m| super::doc::truthy(Some(m)))
                    .map(py_str),
                states: states_of(t, module, &at),
                trans: trans_of(t, module, &at),
                attrs,
                counters,
                rels,
                yaml: t.clone(),
            });
        }
    }
    out
}

/// The checks, over every module's declarations at once.
pub fn checks(docs: &[Value]) -> Vec<Found4> {
    let all = decls(docs);
    let world: BTreeMap<&str, &Decl> = all.iter().map(|d| (d.name.as_str(), d)).collect();
    let mut out = Vec::new();
    for d in &all {
        state_checks(d, &world, &mut out);
        body_checks(d, &world, &all, &mut out);
        only_via_checks(d, &world, &mut out);
        only_via_reached(d, &world, &mut out);
        reference_checks(d, &world, &mut out);
        mirror_checks(d, &world, &mut out);
    }
    super::data::checks(docs, &all, &world, &mut out);
    super::lines::checks(docs, &all, &world, &mut out);
    out
}

pub(super) fn add(out: &mut Vec<Found4>, check: u32, message: String, module: usize, path: &Path) {
    out.push(Found4 {
        check,
        message,
        module,
        path: path.clone(),
    });
}

/// A binder's transitions: its own and its machine's, a machine's creations
/// giving way where the binder declares one of its own (§2.1); and the
/// states the replaced creations reached.
fn binder_trans<'a>(
    d: &'a Decl,
    world: &BTreeMap<&str, &'a Decl>,
) -> (Vec<&'a TransD>, BTreeSet<String>) {
    let machine = d.machine.as_deref().and_then(|m| world.get(m)).copied();
    let mut trans: Vec<&TransD> = d.trans.iter().collect();
    let mut orphaned = BTreeSet::new();
    if let Some(m) = machine {
        let own_create = d.trans.iter().any(|t| t.head == Head::Create);
        for t in &m.trans {
            if own_create && t.head == Head::Create {
                orphaned.extend(targets(&t.x));
            } else {
                trans.push(t);
            }
        }
    }
    (trans, orphaned)
}

/// Checks 15, 16, 19 (a state), 31, 34, 40, 42 and 43: a binder's machine,
/// a type's creation and tracking, and the graph of its states.
fn state_checks(d: &Decl, world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    let machine = d.machine.as_deref().and_then(|m| world.get(m)).copied();
    let (trans, orphaned) = binder_trans(d, world);
    let states: &Vec<StateD> = if d.states.is_empty() {
        machine.map_or(&d.states, |m| &m.states)
    } else {
        &d.states
    };
    // what the type has, its bases' included, as `attrs` and `rels` gather them
    let mut members: BTreeSet<String> = obj(&d.yaml, "attributes").keys().cloned().collect();
    let mut invariants: BTreeSet<String> = obj(&d.yaml, "invariants").keys().cloned().collect();
    let mut seen = BTreeSet::new();
    let mut anc = d.base.as_deref().and_then(|b| world.get(b)).copied();
    while let Some(a) = anc {
        if !seen.insert(a.name.clone()) {
            break;
        }
        members.extend(obj(&a.yaml, "attributes").keys().cloned());
        invariants.extend(obj(&a.yaml, "invariants").keys().cloned());
        for (s, v) in obj(&a.yaml, "states") {
            if flag(v, "required_attributes") {
                invariants.insert(format!("{}_invariant", s.to_lowercase()));
            }
        }
        anc = a.base.as_deref().and_then(|b| world.get(b)).copied();
    }
    for (s, v) in obj(&d.yaml, "states") {
        if flag(v, "required_attributes") {
            invariants.insert(format!("{}_invariant", s.to_lowercase()));
        }
    }
    if let Some(m) = machine {
        let req = m.yaml.get("requires").cloned().unwrap_or(Value::Null);
        let required: Vec<String> = obj(&req, "attributes")
            .keys()
            .cloned()
            .chain(super::doc::strings(&req, "invariants"))
            .collect();
        for rn in required {
            if !members.contains(&rn) && !invariants.contains(&rn) && !d.counters.contains(&rn) {
                add(
                    out,
                    16,
                    format!(
                        "{} binds {} requiring {rn}, not declared on the binder",
                        d.name, m.name
                    ),
                    d.module,
                    &d.path,
                );
            }
        }
    }
    if d.kind == DeclKind::Type && !d.abstract_ {
        if d.machine.is_none() && d.states.is_empty() {
            add(
                out,
                16,
                format!("{} neither binds a machine nor declares states", d.name),
                d.module,
                &d.path,
            );
        }
        if d.machine.is_some() && !d.states.is_empty() {
            add(
                out,
                16,
                format!("{} both binds a machine and declares states", d.name),
                d.module,
                &d.path,
            );
        }
        if !d.mirror && !trans.iter().any(|t| t.head == Head::Create) {
            add(
                out,
                34,
                format!("{} has no creation transition", d.name),
                d.module,
                &d.path,
            );
        }
        if d.tracking.as_deref() == Some("quantity") && d.counters.is_empty() {
            add(
                out,
                31,
                format!("{} is tracking quantity with no counter", d.name),
                d.module,
                &d.path,
            );
        }
        if let Some(tr @ ("serial" | "record")) = d.tracking.as_deref()
            && !d.counters.is_empty()
        {
            add(
                out,
                31,
                format!("{} is tracking {tr} with a counter", d.name),
                d.module,
                &d.path,
            );
        }
    }
    if !states.is_empty() && d.kind == DeclKind::Type {
        let terminal = |s: &str| states.iter().any(|x| x.name == s && x.terminal);
        let declared = |s: &str| states.iter().any(|x| x.name == s);
        let non_terminal: BTreeSet<String> = states
            .iter()
            .filter(|s| !s.terminal)
            .map(|s| s.name.clone())
            .collect();
        let (mut out_s, mut in_s) = (BTreeSet::new(), BTreeSet::new());
        for t in &trans {
            let src = sources(&t.x);
            let (mut froms, mut tos, mut ats) = (BTreeSet::new(), BTreeSet::new(), BTreeSet::new());
            match t.head {
                Head::Do => {
                    if src.iter().any(|s| s == "any") {
                        froms.extend(non_terminal.iter().cloned());
                    }
                    froms.extend(src.iter().filter(|s| *s != "any").cloned());
                    tos.extend(targets(&t.x));
                }
                Head::Create | Head::Assert => tos.extend(targets(&t.x)),
                Head::Act => {
                    if src.iter().any(|s| s == "any") {
                        ats.extend(non_terminal.iter().cloned());
                    }
                    ats.extend(src.iter().filter(|s| *s != "any").cloned());
                }
                Head::Erase => {}
            }
            // the text's patterns read only names in capitals
            let upper = |s: &&String| s.chars().next().is_some_and(|c| c.is_ascii_uppercase());
            for x in froms
                .iter()
                .chain(tos.iter())
                .chain(ats.iter())
                .filter(upper)
            {
                if !declared(x) && x.to_lowercase() != "any" {
                    add(
                        out,
                        19,
                        format!("{}.{} names undeclared state {x}", d.name, t.name),
                        t.module,
                        &t.path,
                    );
                }
            }
            if t.head == Head::Do {
                out_s.extend(froms.iter().filter(|x| declared(x)).cloned());
            }
            if matches!(t.head, Head::Do | Head::Create) {
                in_s.extend(tos.iter().filter(|x| declared(x)).cloned());
            }
            for a in &ats {
                if declared(a) && terminal(a) {
                    add(
                        out,
                        40,
                        format!("{}.{} is an act at terminal state {a}", d.name, t.name),
                        t.module,
                        &t.path,
                    );
                }
            }
            for f in &froms {
                if declared(f) && terminal(f) && t.head == Head::Do {
                    add(
                        out,
                        15,
                        format!("{}.{} is a do leaving terminal state {f}", d.name, t.name),
                        t.module,
                        &t.path,
                    );
                }
            }
        }
        for s in states {
            if s.category.is_empty() {
                add(
                    out,
                    15,
                    format!("{}.{} has no category", d.name, s.name),
                    s.module,
                    &s.path,
                );
            }
            if d.mirror {
                continue;
            }
            // a closed state is finished work, and may end a lifecycle (ADR-0103 §8)
            let closed = s.category.starts_with("closed");
            if !s.terminal && !closed && !out_s.contains(&s.name) {
                add(
                    out,
                    15,
                    format!(
                        "{}.{} is non-terminal with no outgoing transition",
                        d.name, s.name
                    ),
                    s.module,
                    &s.path,
                );
            }
            if !in_s.contains(&s.name) && !orphaned.contains(&s.name) {
                add(
                    out,
                    15,
                    format!("{}.{} is reachable by nothing", d.name, s.name),
                    s.module,
                    &s.path,
                );
            }
        }
        if !d.mirror && !states.iter().any(|s| s.terminal) {
            add(
                out,
                15,
                format!("{} has no terminal state", d.name),
                d.module,
                &d.path,
            );
        }
    }
    if d.kind == DeclKind::Type && !d.abstract_ {
        let mut tr = d.tracking.clone();
        let mut anc = Some(d);
        let mut seen = BTreeSet::new();
        while tr.is_none() {
            let Some(base) = anc.and_then(|a| a.base.clone()) else {
                break;
            };
            if !seen.insert(base.clone()) {
                break;
            }
            anc = world.get(base.as_str()).copied();
            tr = anc.and_then(|a| a.tracking.clone());
        }
        if !matches!(tr.as_deref(), Some("serial" | "quantity" | "record")) {
            let shown = tr.map_or_else(|| "None".to_string(), |t| format!("'{t}'"));
            add(
                out,
                42,
                format!(
                    "{} has tracking {shown}, declared or inherited, which is not serial, quantity or record",
                    d.name
                ),
                d.module,
                &d.path,
            );
        }
    }
    if let Some(base) = &d.base {
        let mut seen = BTreeSet::new();
        let mut cur = Some(d);
        while let Some(c) = cur {
            let Some(b) = &c.base else { break };
            if !seen.insert(b.clone()) {
                add(
                    out,
                    43,
                    format!("{} has a cyclic extends chain through {b}", d.name),
                    d.module,
                    &d.path,
                );
                break;
            }
            cur = world.get(b.as_str()).copied();
        }
        if world.get(base.as_str()).is_some_and(|a| !a.abstract_) {
            add(
                out,
                43,
                format!("{} extends {base}, which is not abstract", d.name),
                d.module,
                &d.path,
            );
        }
    }
}

/// The scalar types of §3.1: a reason of one of these is no enumeration.
const SCALARS: &[&str] = &[
    "string",
    "bool",
    "int",
    "decimal",
    "money",
    "timestamp",
    "duration",
    "identity",
    "file",
    "event",
    "date",
    "actor_kind",
];

/// What a type has with its bases': its attributes, references, derived
/// attributes, invariants and counters (`attrs`, `rels`, `derives` and
/// `invariants` as the text checker merges them).
struct Merged {
    attrs: Vec<(String, Value)>,
    rels: Vec<RelD>,
    derives: BTreeSet<String>,
}

fn merged(d: &Decl, world: &BTreeMap<&str, &Decl>) -> Merged {
    let mut attrs = d.attrs.clone();
    let mut rels = d.rels.clone();
    let mut derives: BTreeSet<String> =
        obj(&d.yaml, "derived_attributes").keys().cloned().collect();
    let mut seen = BTreeSet::new();
    let mut anc = d.base.as_deref().and_then(|b| world.get(b)).copied();
    while let Some(a) = anc {
        if !seen.insert(a.name.clone()) {
            break;
        }
        for (n, spec) in &a.attrs {
            if !attrs.iter().any(|(m, _)| m == n) {
                attrs.push((n.clone(), spec.clone()));
            }
        }
        for r in &a.rels {
            if !rels.iter().any(|x| x.name == r.name) {
                rels.push(r.clone());
            }
        }
        derives.extend(obj(&a.yaml, "derived_attributes").keys().cloned());
        anc = a.base.as_deref().and_then(|b| world.get(b)).copied();
    }
    Merged {
        attrs,
        rels,
        derives,
    }
}

/// What the text's `accepts` lists: a transition's attribute inputs.
fn accepts(x: &Value) -> Vec<String> {
    strings(x, "required_inputs")
        .into_iter()
        .chain(strings(x, "optional_inputs"))
        .collect()
}

/// Whether a single reference end stores its value, as `stores` decides it.
pub(super) fn stores(r: &RelD, world: &BTreeMap<&str, &Decl>) -> bool {
    if r.is_set() {
        return false;
    }
    let Some(inverse) = r.inverse() else {
        return true;
    };
    if flag(&r.spec, "stored") {
        return true;
    }
    world
        .get(r.target().as_str())
        .and_then(|far| far.rels.iter().find(|x| x.name == inverse))
        .is_some_and(RelD::is_set)
}

/// The inputs a transition declares that a call must give: those not
/// optional, as the text's `input <n> : <T>` lines without `?` say.
fn needed(x: &Value) -> BTreeSet<String> {
    obj(x, "inputs")
        .iter()
        .filter(|(_, sp)| !flag(sp, "optional"))
        .map(|(i, _)| i.clone())
        .collect()
}

/// Checks 2, 8, 11, 17, 18, 19 (a write, a clear, a call), 20, 26, 29, 35
/// and 38: what each transition's body writes, clears, creates and calls.
fn body_checks(d: &Decl, world: &BTreeMap<&str, &Decl>, all: &[Decl], out: &mut Vec<Found4>) {
    let machine = d.machine.as_deref().and_then(|m| world.get(m)).copied();
    let trans: Vec<&TransD> = if d.kind == DeclKind::Type {
        binder_trans(d, world).0
    } else {
        d.trans.iter().collect()
    };
    let states: &Vec<StateD> = if d.states.is_empty() {
        machine.map_or(&d.states, |m| &m.states)
    } else {
        &d.states
    };
    let m = merged(d, world);
    let attr = |n: &str| m.attrs.iter().find(|(a, _)| a == n).map(|(_, s)| s);
    let rel = |n: &str| m.rels.iter().find(|r| r.name == n);
    // a machine's required members, by kind, as its `requires` lines say them
    let requires: BTreeMap<String, Value> = if d.kind == DeclKind::Machine {
        obj(d.yaml.get("requires").unwrap_or(&Value::Null), "attributes")
            .iter()
            .map(|(n, s)| (n.clone(), s.clone()))
            .collect()
    } else {
        BTreeMap::new()
    };
    let required_names: BTreeSet<String> = requires
        .keys()
        .cloned()
        .chain(strings(
            d.yaml.get("requires").unwrap_or(&Value::Null),
            "invariants",
        ))
        .collect();
    let owner = d.rels.iter().find(|r| r.kind == RelKind::Owner);
    for t in &trans {
        let x = &t.x;
        let (dn, tn) = (&d.name, &t.name);
        let steps = walk(x.get("effect"));
        let mut written: BTreeSet<String> = steps
            .iter()
            .filter(|s| s.kind == "assign")
            .map(|s| str_or(s.body(), "location", ""))
            .collect();
        written.extend(accepts(x));
        if t.head == Head::Create {
            if let Some(o) = owner {
                let via = strings(x, "only_via");
                if via.is_empty() {
                    add(
                        out,
                        11,
                        format!("{dn}.{tn} creates a part but is not 'only via' its whole"),
                        t.module,
                        &t.path,
                    );
                } else {
                    let whole = o.target();
                    let mut wholes = BTreeSet::from([whole.clone()]);
                    if world.get(whole.as_str()).is_some_and(|w| w.abstract_) {
                        wholes.extend(
                            all.iter()
                                .filter(|x| x.base.as_deref() == Some(whole.as_str()))
                                .map(|x| x.name.clone()),
                        );
                    }
                    let named: BTreeSet<String> = via
                        .iter()
                        .filter_map(|v| v.split_once('.').map(|(ty, _)| ty.to_string()))
                        .collect();
                    if !named.is_empty() && named.is_disjoint(&wholes) {
                        let shown: Vec<String> = named.iter().map(|n| format!("'{n}'")).collect();
                        add(
                            out,
                            11,
                            format!(
                                "{dn}.{tn} is only via [{}], not its whole {whole}",
                                shown.join(", ")
                            ),
                            t.module,
                            &t.path,
                        );
                    }
                }
                if !written.contains(&o.name) {
                    add(
                        out,
                        18,
                        format!(
                            "{dn}.{tn} creates a part without writing owner '{}'",
                            o.name
                        ),
                        t.module,
                        &t.path,
                    );
                }
            }
            for p in m.rels.iter().filter(|r| r.kind == RelKind::Part) {
                if p.optional() || p.is_set() {
                    continue;
                }
                let filled = steps
                    .iter()
                    .any(|s| s.kind == "create" && str_or(s.body(), "type", "") == p.target());
                if !filled {
                    add(
                        out,
                        8,
                        format!("{dn}.{tn} never fills required part '{}'", p.name),
                        t.module,
                        &t.path,
                    );
                }
            }
            for (an, spec) in &m.attrs {
                let set = str_or(spec, "type", "").contains("[]");
                if flag(spec, "optional")
                    || set
                    || spec.get("default").is_some()
                    || flag(spec, "identifier")
                {
                    continue;
                }
                if !written.contains(an) {
                    add(
                        out,
                        8,
                        format!("{dn}.{tn} never writes required attribute '{an}'"),
                        t.module,
                        &t.path,
                    );
                }
            }
        }
        let inputs = obj(x, "inputs");
        if t.head == Head::Assert {
            match inputs.get("reason") {
                None => add(
                    out,
                    29,
                    format!("{dn}.{tn} asserts with no reason input"),
                    t.module,
                    &t.path,
                ),
                Some(r) => {
                    let first = r
                        .get("type")
                        .or_else(|| r.get("reference"))
                        .map(py_str)
                        .unwrap_or_default();
                    let word: String = first
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if SCALARS.contains(&word.as_str()) {
                        add(
                            out,
                            29,
                            format!("{dn}.{tn} asserts with a {word} reason, not a declared enum"),
                            t.module,
                            &t.path,
                        );
                    }
                }
            }
        }
        if t.head == Head::Erase && !inputs.contains_key("reason") {
            add(
                out,
                29,
                format!("{dn}.{tn} erases with no reason input"),
                t.module,
                &t.path,
            );
        }
        // 17: a declared decimal holds 1 to 18 digits, its scale within them
        for sp in inputs.values() {
            let k = sp
                .get("type")
                .or_else(|| sp.get("reference"))
                .map(py_str)
                .unwrap_or_default();
            if let Some((p, sc)) = decimal_digits(&k)
                && !((1..=18).contains(&p) && sc <= p)
            {
                add(
                    out,
                    17,
                    format!(
                        "{dn}.{tn} declares decimal({p},{sc}); a decimal holds 1 to 18 digits, its scale within them"
                    ),
                    t.module,
                    &t.path,
                );
            }
        }
        for s in &steps {
            if !matches!(s.kind, "assign" | "add" | "remove") {
                continue;
            }
            let tgt = str_or(s.body(), "location", "");
            if tgt.contains('.') {
                add(
                    out,
                    17,
                    format!("{dn}.{tn} writes through a path '{tgt}'"),
                    t.module,
                    &t.path,
                );
            } else if attr(&tgt).is_none()
                && rel(&tgt).is_none()
                && !d.counters.contains(&tgt)
                && !m.derives.contains(&tgt)
                && !required_names.contains(&tgt)
            {
                add(
                    out,
                    19,
                    format!("{dn}.{tn} writes undeclared name '{tgt}'"),
                    t.module,
                    &t.path,
                );
            }
        }
        for s in steps.iter().filter(|s| s.kind == "clear") {
            for tgt in list(s.st, "clear").iter().map(py_str) {
                if tgt.contains('.') {
                    add(
                        out,
                        17,
                        format!("{dn}.{tn} clears through a path '{tgt}'"),
                        t.module,
                        &t.path,
                    );
                } else if d.counters.contains(&tgt) {
                    add(
                        out,
                        17,
                        format!("{dn}.{tn} clears counter '{tgt}'"),
                        t.module,
                        &t.path,
                    );
                } else if let Some(r) = rel(&tgt) {
                    if !(r.kind == RelKind::Ref && r.optional() && stores(r, world)) {
                        add(
                            out,
                            17,
                            format!(
                                "{dn}.{tn} clears relationship end '{tgt}', which is not an optional, singular, stored ref"
                            ),
                            t.module,
                            &t.path,
                        );
                    }
                } else if let Some(spec) = requires.get(&tgt) {
                    let counter = text(spec, "type") == Some("counter");
                    let composite = text(spec, "aggregation") == Some("composite");
                    let kind = str_or(spec, "reference", "");
                    let single_optional = flag(spec, "optional")
                        && !kind.ends_with("[]")
                        && !str_or(spec, "type", "").ends_with("[]");
                    if counter || composite || !single_optional {
                        add(
                            out,
                            17,
                            format!(
                                "{dn}.{tn} clears required '{tgt}', which is not an optional, singular ref or attribute"
                            ),
                            t.module,
                            &t.path,
                        );
                    }
                } else if let Some(spec) = attr(&tgt) {
                    if !flag(spec, "optional") {
                        add(
                            out,
                            17,
                            format!("{dn}.{tn} clears required attribute '{tgt}'"),
                            t.module,
                            &t.path,
                        );
                    }
                } else {
                    add(
                        out,
                        19,
                        format!("{dn}.{tn} clears undeclared name '{tgt}'"),
                        t.module,
                        &t.path,
                    );
                }
            }
        }
        for s in steps.iter().filter(|s| matches!(s.kind, "add" | "remove")) {
            let tgt = str_or(s.body(), "location", "");
            if let Some(spec) = attr(&tgt)
                && !str_or(spec, "type", "").contains("[]")
            {
                add(
                    out,
                    17,
                    format!("{dn}.{tn} adds to non-set '{tgt}'"),
                    t.module,
                    &t.path,
                );
            }
        }
        // 20: an optional input with a default
        for (i, sp) in inputs {
            if flag(sp, "optional") && sp.get("default").is_some() {
                add(
                    out,
                    20,
                    format!("{dn}.{tn} gives a default to optional input '{i}'"),
                    t.module,
                    &t.path,
                );
            }
        }
        supersession(d, t, states, &steps, out);
        calls(d, t, world, &steps, out);
        if d.kind == DeclKind::Machine {
            for n in &written {
                if !n.is_empty() && !required_names.contains(n) && n != "state" {
                    add(
                        out,
                        35,
                        format!("{dn}.{tn} names '{n}', which the machine does not require"),
                        t.module,
                        &t.path,
                    );
                }
            }
        }
    }
}

/// `decimal(<p>,<s>)`, its digits and its scale.
fn decimal_digits(k: &str) -> Option<(u64, u64)> {
    let inner = k.strip_prefix("decimal(")?;
    let (p, rest) = inner.split_once(',')?;
    let s = rest.split(')').next()?;
    Some((p.trim().parse().ok()?, s.trim().parse().ok()?))
}

/// Check 26, as the text checker reads a transition's supersede steps.
fn supersession(
    d: &Decl,
    t: &TransD,
    states: &[StateD],
    steps: &[super::steps::Step<'_>],
    out: &mut Vec<Found4>,
) {
    let (dn, tn) = (&d.name, &t.name);
    // the head's single targets, `-> S`, not a set of them
    let to_states: Vec<String> = match t.x.get("to") {
        Some(Value::String(s)) => vec![s.clone()],
        Some(Value::Array(a)) if a.len() == 1 => a.iter().map(py_str).collect(),
        _ => Vec::new(),
    };
    let superseding = |s: &str| states.iter().any(|x| x.name == s && x.superseding);
    let sups: Vec<&super::steps::Step<'_>> =
        steps.iter().filter(|s| s.kind == "supersede").collect();
    if let Some(first) = sups.first() {
        if !to_states.iter().any(|s| superseding(s)) {
            add(
                out,
                26,
                format!("{dn}.{tn} supersedes into a non-superseding state"),
                t.module,
                &t.path,
            );
        }
        if squash(&py_str(first.body())) == "this" {
            add(
                out,
                26,
                format!("{dn}.{tn} supersedes itself"),
                t.module,
                &t.path,
            );
        }
    }
    if sups.len() > 1 {
        add(
            out,
            26,
            format!("{dn}.{tn} names more than one successor"),
            t.module,
            &t.path,
        );
    }
    for s in &sups {
        if s.depth > 0 {
            add(
                out,
                26,
                format!(
                    "{dn}.{tn} supersedes inside a loop, which may name no successor or several"
                ),
                t.module,
                &t.path,
            );
        }
        let who = squash(&py_str(s.body()));
        if let Some(i) = who
            .strip_prefix("inputs.")
            .filter(|i| i.chars().all(|c| c.is_alphanumeric() || c == '_'))
            && obj(&t.x, "inputs")
                .get(i)
                .is_some_and(|sp| flag(sp, "optional"))
        {
            add(
                out,
                26,
                format!(
                    "{dn}.{tn} supersedes with optional input '{i}', which a request may leave out"
                ),
                t.module,
                &t.path,
            );
        }
    }
    if sups.is_empty() {
        for s in &to_states {
            if superseding(s) {
                add(
                    out,
                    26,
                    format!("{dn}.{tn} enters superseding state {s} without a supersede"),
                    t.module,
                    &t.path,
                );
            }
        }
    }
}

/// Whether a call's target is the text's `[\w.$]+`, a name or a path.
fn simple_path(e: &str) -> bool {
    !e.is_empty()
        && e.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$')
}

/// Check 2, and 19 for a transition no callee has: each call or create the
/// text's pattern reads, a create bound to a name not among them.
fn calls(
    d: &Decl,
    t: &TransD,
    world: &BTreeMap<&str, &Decl>,
    steps: &[super::steps::Step<'_>],
    out: &mut Vec<Found4>,
) {
    let (dn, tn) = (&d.name, &t.name);
    for s in steps {
        let body = s.body();
        let (target, given) = match s.kind {
            "call" => (
                squash(&str_or(body, "target", "")),
                obj(body, "inputs").keys().cloned().collect::<BTreeSet<_>>(),
            ),
            "create"
                if body
                    .get("result")
                    .filter(|r| super::doc::truthy(Some(r)))
                    .is_none() =>
            {
                (
                    str_or(body, "type", ""),
                    obj(body, "inputs").keys().cloned().collect(),
                )
            }
            _ => continue,
        };
        if !simple_path(&target) {
            continue;
        }
        let tn2 = str_or(body, "transition", "");
        let last = target.rsplit('.').next().unwrap_or(&target).to_string();
        let Some(callee) = world
            .get(target.as_str())
            .or_else(|| world.get(last.as_str()))
        else {
            continue;
        };
        let callee_trans: Vec<&TransD> = callee.trans.iter().collect();
        match callee_trans.iter().find(|c| c.name == tn2) {
            Some(ct) => {
                for miss in needed(&ct.x).difference(&given) {
                    add(
                        out,
                        2,
                        format!("{dn}.{tn} calls {target}.{tn2} without required '{miss}'"),
                        t.module,
                        &t.path,
                    );
                }
            }
            None => add(
                out,
                19,
                format!("{dn}.{tn} calls undeclared transition {target}.{tn2}"),
                t.module,
                &t.path,
            ),
        }
    }
}

/// Check 13: an `only via`'s parents exist and reach the transition.
fn only_via_checks(d: &Decl, world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    for t in &d.trans {
        for parent in strings(&t.x, "only_via") {
            let Some((ty, tr)) = parent.split_once('.') else {
                continue;
            };
            let Some(pd) = world.get(ty) else {
                add(
                    out,
                    13,
                    format!("{}.{} names unknown parent type {ty}", d.name, t.name),
                    t.module,
                    &t.path,
                );
                continue;
            };
            let mut pt = pd.trans.iter().find(|x| x.name == tr);
            if pt.is_none()
                && let Some(pm) = pd.machine.as_deref().and_then(|m| world.get(m))
            {
                pt = pm.trans.iter().find(|x| x.name == tr);
            }
            // a cascade on the parent naming this transition: the parent's
            // parts, and its inherited ones, each clause on `tr` read with the
            // clauses after it in the same end, as the text's pattern reads them
            let mut ends: Vec<(String, Vec<Value>)> = obj(&pd.yaml, "attributes")
                .values()
                .filter(|sp| text(sp, "aggregation") == Some("composite"))
                .map(|sp| {
                    (
                        str_or(sp, "reference", "")
                            .trim_end_matches(['[', ']'])
                            .to_string(),
                        list(sp, "cascade").to_vec(),
                    )
                })
                .collect();
            for (an, ip) in obj(&pd.yaml, "inherited_parts") {
                let target = inherited_part_target(pd, an, world);
                ends.push((target, list(ip, "cascade").to_vec()));
            }
            let cascades = ends.iter().any(|(target, clauses)| {
                target == &d.name
                    && clauses.iter().enumerate().any(|(i, c)| {
                        strings(c, "on").iter().any(|o| o == tr)
                            && clauses[i..]
                                .iter()
                                .any(|later| str_or(later, "transition", "") == t.name)
                    })
            });
            let Some(pt) = pt else {
                if !cascades {
                    add(
                        out,
                        13,
                        format!(
                            "{}.{} names parent {parent}, which does not exist",
                            d.name, t.name
                        ),
                        t.module,
                        &t.path,
                    );
                }
                continue;
            };
            let reaches = cascades
                || walk(pt.x.get("effect")).iter().any(|s| {
                    let body = s.body();
                    str_or(body, "transition", "") == t.name
                        && match s.kind {
                            "call" => simple_path(&squash(&str_or(body, "target", ""))),
                            // a creation naming its result reaches the
                            // transition as a bare one does (D551)
                            "create" => true,
                            _ => false,
                        }
                });
            if !reaches {
                add(
                    out,
                    13,
                    format!(
                        "{}.{} names parent {parent}, which never reaches it",
                        d.name, t.name
                    ),
                    t.module,
                    &t.path,
                );
            }
        }
    }
}

/// Check 13's converse: a create or a call into an `only via` transition
/// from one it does not name, the caller's type or a base of it, or for a
/// machine each type binding it (§4.2; the audit's write path R-05, refused
/// at run time since). A cascade is the run time's alone.
fn only_via_reached(d: &Decl, world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    let callers: Vec<&Decl> = match d.kind {
        DeclKind::Type => vec![d],
        DeclKind::Machine => world
            .values()
            .copied()
            .filter(|b| b.kind == DeclKind::Type && b.machine.as_deref() == Some(d.name.as_str()))
            .collect(),
    };
    let lineage = |x: &Decl| -> BTreeSet<String> {
        let mut seen = BTreeSet::new();
        let mut at = Some(x);
        while let Some(c) = at {
            if !seen.insert(c.name.clone()) {
                break;
            }
            at = c.base.as_deref().and_then(|b| world.get(b)).copied();
        }
        seen
    };
    for t in &d.trans {
        for s in walk(t.x.get("effect")) {
            let body = s.body();
            let target = match s.kind {
                "call" => squash(&str_or(body, "target", "")),
                "create" => str_or(body, "type", ""),
                _ => continue,
            };
            if !simple_path(&target) {
                continue;
            }
            let tn2 = str_or(body, "transition", "");
            let last = target.rsplit('.').next().unwrap_or(&target);
            let Some(callee) = world.get(target.as_str()).or_else(|| world.get(last)) else {
                continue;
            };
            let Some(ct) = callee.trans.iter().find(|c| c.name == tn2).or_else(|| {
                callee
                    .machine
                    .as_deref()
                    .and_then(|m| world.get(m))
                    .and_then(|pm| pm.trans.iter().find(|c| c.name == tn2))
            }) else {
                continue;
            };
            let parents = strings(&ct.x, "only_via");
            if parents.is_empty() {
                continue;
            }
            for caller in &callers {
                let mine = lineage(caller);
                let named = parents.iter().any(|p| {
                    p.split_once('.')
                        .is_some_and(|(ty, tr)| tr == t.name && mine.contains(ty))
                });
                if !named {
                    add(
                        out,
                        13,
                        format!(
                            "{}.{} reaches {}.{tn2}, whose only via names {} and not it",
                            caller.name,
                            t.name,
                            callee.name,
                            parents.join(", ")
                        ),
                        t.module,
                        &t.path,
                    );
                }
            }
        }
    }
}

/// The part type an inherited part names: the reference of the base end.
pub(super) fn inherited_part_target(d: &Decl, an: &str, world: &BTreeMap<&str, &Decl>) -> String {
    let mut seen = BTreeSet::new();
    let mut anc = d.base.as_deref().and_then(|b| world.get(b)).copied();
    while let Some(a) = anc {
        if !seen.insert(a.name.clone()) {
            break;
        }
        if let Some(sp) = obj(&a.yaml, "attributes").get(an) {
            return str_or(sp, "reference", "")
                .trim_end_matches(['[', ']'])
                .to_string();
        }
        anc = a.base.as_deref().and_then(|b| world.get(b)).copied();
    }
    String::new()
}

/// Checks 7, 41 and 47: a reference end's markings and pair, and a
/// cascade's bound.
fn reference_checks(d: &Decl, world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    for r in &d.rels {
        // the text writes markings on a ref's line alone, an owner's and a part's having none
        if r.kind == RelKind::Ref && flag(&r.spec, "indexed") && stores(r, world) {
            add(
                out,
                7,
                format!(
                    "{}.{} is a stored relationship end and is marked indexed, which it already is",
                    d.name, r.name
                ),
                r.module,
                &r.path,
            );
        }
        if r.kind == RelKind::Part {
            for c in list(&r.spec, "cascade") {
                if c.get("limit").is_none() {
                    add(
                        out,
                        47,
                        format!("{}.{} has a cascade clause with no limit", d.name, r.name),
                        r.module,
                        &r.path,
                    );
                }
            }
        }
        if r.kind != RelKind::Ref {
            continue;
        }
        let Some(inverse) = r.inverse() else { continue };
        let fname = r.target();
        let Some(far) = world.get(fname.as_str()) else {
            continue;
        };
        let Some(fe) = far.rels.iter().find(|x| x.name == inverse) else {
            add(
                out,
                41,
                format!(
                    "{}.{} names inverse {fname}.{inverse}, which {fname} does not declare",
                    d.name, r.name
                ),
                r.module,
                &r.path,
            );
            continue;
        };
        if fe.kind != RelKind::Ref {
            continue;
        }
        if fe.inverse().is_some_and(|fm| fm != r.name) {
            add(
                out,
                41,
                format!(
                    "{}.{} and {fname}.{inverse} do not name each other",
                    d.name, r.name
                ),
                r.module,
                &r.path,
            );
            continue;
        }
        if d.name > fname {
            continue; // the pair is reported once
        }
        match (r.is_set(), fe.is_set()) {
            (true, true) => add(
                out,
                41,
                format!(
                    "{}.{} and {fname}.{inverse} are both set-valued, so neither end can store the pair",
                    d.name, r.name
                ),
                r.module,
                &r.path,
            ),
            (false, false) => {
                let n =
                    usize::from(flag(&r.spec, "stored")) + usize::from(flag(&fe.spec, "stored"));
                if n != 1 {
                    add(
                        out,
                        41,
                        format!(
                            "{}.{} and {fname}.{inverse} are both singular; exactly one must be marked 'stored' ({n} are)",
                            d.name, r.name
                        ),
                        r.module,
                        &r.path,
                    );
                }
            }
            _ => {}
        }
    }
}

/// Check 53: nothing writes, or composes with, a type another system owns.
fn mirror_checks(d: &Decl, world: &BTreeMap<&str, &Decl>, out: &mut Vec<Found4>) {
    let mirror_of = |n: &str| world.get(n).is_some_and(|x| x.mirror);
    let owned_type = |n: &str| {
        world
            .get(n)
            .is_some_and(|x| x.kind == DeclKind::Type && !x.mirror)
    };
    if d.mirror {
        if let Some(m) = &d.machine {
            add(
                out,
                53,
                format!(
                    "{} is a mirror and binds machine {m}, whose transitions would make it writable",
                    d.name
                ),
                d.module,
                &d.path,
            );
        }
        let writes = d.trans.iter().filter(|t| t.head != Head::Erase).count();
        if writes > 0 {
            add(
                out,
                53,
                format!(
                    "{} is a mirror and declares {writes} transition(s) other than erase",
                    d.name
                ),
                d.module,
                &d.path,
            );
        }
        if let Some(b) = &d.base
            && world.get(b.as_str()).is_some_and(|x| !x.mirror)
        {
            add(
                out,
                53,
                format!(
                    "{} is a mirror and extends {b}, which this store owns",
                    d.name
                ),
                d.module,
                &d.path,
            );
        }
        for r in &d.rels {
            let far = r.target();
            if matches!(r.kind, RelKind::Part | RelKind::Owner) && owned_type(&far) {
                add(
                    out,
                    53,
                    format!(
                        "{}.{} is a mirror composing with {far}, which this store owns",
                        d.name, r.name
                    ),
                    r.module,
                    &r.path,
                );
            }
            if r.kind == RelKind::Ref
                && (stores(r, world) || (r.is_set() && r.inverse().is_none()))
                && owned_type(&far)
            {
                add(
                    out,
                    53,
                    format!(
                        "{}.{} is a mirror referencing {far}, which this store owns, so a referent was cut over before its referrer",
                        d.name, r.name
                    ),
                    r.module,
                    &r.path,
                );
            }
        }
    }
    if !d.mirror
        && let Some(b) = &d.base
        && mirror_of(b)
    {
        add(
            out,
            53,
            format!(
                "{} extends mirror {b}, so a type this store owns would inherit the shape of one it does not",
                d.name
            ),
            d.module,
            &d.path,
        );
    }
    if d.mirror {
        return;
    }
    let trans: Vec<&TransD> = if d.kind == DeclKind::Type {
        binder_trans(d, world).0
    } else {
        d.trans.iter().collect()
    };
    let rels = merged(d, world).rels;
    for t in trans {
        // an input or a relationship end resolves to the type it names
        let mut local: BTreeMap<String, String> = obj(&t.x, "inputs")
            .iter()
            .map(|(n, sp)| {
                let k = sp
                    .get("type")
                    .filter(|k| super::doc::truthy(Some(k)))
                    .or_else(|| sp.get("reference"))
                    .map(py_str)
                    .unwrap_or_default();
                (n.clone(), k.trim_end_matches(['?', '[', ']']).to_string())
            })
            .collect();
        for r in &rels {
            local.insert(r.name.clone(), r.target());
        }
        for s in walk(t.x.get("effect")) {
            let body = s.body();
            let path = match s.kind {
                "create" => str_or(body, "type", ""),
                "call" => squash(&str_or(body, "target", "")),
                _ => continue,
            };
            if path.is_empty()
                || !path
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '_' || c == '.')
            {
                continue;
            }
            let root = path.rsplit('.').next().unwrap_or(&path).to_string();
            let named = local.get(&root).cloned().unwrap_or(root);
            let tgt = world
                .get(named.as_str())
                .or_else(|| world.get(path.as_str()));
            if let Some(tgt) = tgt
                && tgt.mirror
            {
                add(
                    out,
                    53,
                    format!(
                        "{}.{} writes {}, which is a mirror owned by another system",
                        d.name, t.name, tgt.name
                    ),
                    t.module,
                    &t.path,
                );
            }
        }
    }
    for r in &d.rels {
        let far = r.target();
        if matches!(r.kind, RelKind::Part | RelKind::Owner) && mirror_of(&far) {
            add(
                out,
                53,
                format!(
                    "{}.{} composes with mirror {far}, whose lifetime this store does not own",
                    d.name, r.name
                ),
                r.module,
                &r.path,
            );
        }
    }
}
