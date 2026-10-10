//! What an examples file names, checked against its module and the modules
//! it imports (`check-flows.py`'s `example_errors` and `uncovered`;
//! `flow-format.md` §11, ADR-0136): each setup and example starts from a
//! named setup, each step requests a transition the flow declares, of an
//! object an earlier step made, with inputs the transition takes, as an
//! actor the store holds, and each expectation names a verdict the request
//! can give, with the clause and remedy the rules declare.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Map, Value, json};

use super::doc::{
    Found, Obj, Path, ext, finding, flag, keys, list, obj, optional, py_str, re, segs, squash,
    str_or, strings,
};
use super::steps::{kind, required_input_names, sources, taken, targets};
use super::view::bases;

/// Each transition an examples file's examples request, as its type and
/// name, with the examples that request it, in the order written.
pub type Requested = BTreeMap<(String, String), Vec<String>>;

/// Each transition a setup's or an example's step takes, or an
/// expectation's `cascaded` names, which the coverage notice tells apart
/// from those requested under test (ADR-0173 decision 6).
pub type Exercised = BTreeSet<(String, String)>;

const EXAMPLES_ORDER: &[&str] = &["examples_for", "setups", "examples"];
const GENERATED_GUARDS: &[&str] = &[
    "actor_known",
    "occurred_within",
    "whole_open",
    "not_erased",
    "subject_open",
    "corrects_current",
    "subject_owned",
    "successor_not_superseded",
];
const BUILT_IN_MEMBERS: &[&str] = &[
    "id",
    "open",
    "created_at",
    "created_by_kind",
    "recorded_from",
    "declaration_version",
];
const ACTOR_KINDS: &[&str] = &["human", "agent", "service"];

static DATE: LazyLock<Regex> =
    LazyLock::new(|| re(r"^\d{4}-(0[1-9]|1[0-2])-(0[1-9]|[12]\d|3[01])$"));
static TIMESTAMP: LazyLock<Regex> = LazyLock::new(|| {
    re(r"^(now( [+-] [0-9]+ (s|min|h|days?|weeks?))?|\d{4}-\d{2}-\d{2}T\d{2}:\d{2}(:\d{2})?Z)$")
});

fn is_a(tn: &str, want: &str, kinds: &Obj) -> bool {
    tn == want || bases(tn, kinds).iter().any(|b| b == want)
}

/// A type whose objects are actors: one of its attributes marks an actor kind.
fn marks_actor(tn: &str, kinds: &Obj) -> bool {
    kinds.get(tn).is_some_and(|t| {
        obj(t, "attributes")
            .values()
            .any(|s| s.get("actor_kind").is_some())
    })
}

/// What a request resolves to: the type and the transition, and the inputs
/// it leaves out, refused as invalid input or by a generated guard.
struct Resolved {
    tn: String,
    x: Value,
    missing: Vec<String>,
    unprovided: Vec<String>,
}

struct Checker {
    kinds: Obj,
    enums: BTreeMap<String, BTreeSet<String>>,
    functions: BTreeSet<String>,
    machines: BTreeSet<String>,
    sequences: BTreeSet<String>,
    exercised: RefCell<Exercised>,
    out: Found,
}

impl Checker {
    fn value_errors(
        &self,
        v: &Value,
        spec: Option<&Value>,
        env: &BTreeMap<String, String>,
        at: &Path,
        name: &str,
    ) -> Found {
        let spec = spec.cloned().unwrap_or(Value::Null);
        let reference = spec
            .get("reference")
            .filter(|r| super::doc::truthy(Some(r)))
            .map(py_str);
        let typ = spec.get("type").map(py_str);
        if let Some(r) = reference {
            let (want, many) = (
                r.trim_end_matches(['[', ']']).to_string(),
                r.ends_with("[]"),
            );
            if many && !v.is_array() {
                return vec![finding(
                    at.clone(),
                    "names",
                    format!("'{name}' is a set of {want}, given as a list of aliases"),
                )];
            }
            let got: Vec<&Value> = if many {
                v.as_array().map(|a| a.iter().collect()).unwrap_or_default()
            } else {
                vec![v]
            };
            return got
                .into_iter()
                .filter(|a| !a.as_str().is_some_and(|a| env.get(a).is_some_and(|t| is_a(t, &want, &self.kinds))))
                .map(|a| {
                    finding(
                        at.clone(),
                        "names",
                        format!("'{name}' is a reference to {want}, and '{}' is not an alias an earlier step gave one", py_str(a)),
                    )
                })
                .collect();
        }
        let Some(typ) = typ else { return Found::new() };
        if let Some(members) = self.enums.get(&typ) {
            let ok = v.as_str().is_some_and(|s| {
                s.split_once('.')
                    .is_some_and(|(e, m)| e == typ && members.contains(m))
            });
            if !ok {
                return vec![finding(
                    at.clone(),
                    "names",
                    format!(
                        "'{name}' is a {typ}, written qualified as {typ}.<MEMBER> with a member it declares"
                    ),
                )];
            }
            return Found::new();
        }
        let wrong = match typ.as_str() {
            "bool" => (!v.is_boolean()).then(|| format!("'{name}' is a bool")),
            "int" => (!(v.is_i64() || v.is_u64())).then(|| format!("'{name}' is an int")),
            "timestamp" => (!v.as_str().is_some_and(|s| TIMESTAMP.is_match(s))).then(|| {
                format!("'{name}' is a timestamp, written in UTC as 2026-01-01T09:00Z, or as now, now + <duration> or now - <duration>")
            }),
            "date" => (!v.as_str().is_some_and(|s| DATE.is_match(s))).then(|| format!("'{name}' is a date, written as 2026-03-01")),
            "actor_kind" => (!v.as_str().is_some_and(|s| ACTOR_KINDS.contains(&s)))
                .then(|| format!("'{name}' is an actor_kind, written bare as human, agent or service")),
            _ => None,
        };
        wrong
            .map(|w| vec![finding(at.clone(), "names", w)])
            .unwrap_or_default()
    }

    fn actor_errors(&self, r: &Value, env: &BTreeMap<String, String>, at: &Path) -> Found {
        let actor = str_or(r, "actor", "operator");
        if actor != "operator" && !env.get(&actor).is_some_and(|t| marks_actor(t, &self.kinds)) {
            return vec![finding(
                ext(at, segs!["actor"]),
                "names",
                format!(
                    "names the actor '{actor}', which is neither `operator` nor an alias of an object whose type marks an actor identity"
                ),
            )];
        }
        Found::new()
    }

    /// A request's names; `applies` for a setup's step, which must apply.
    fn request_errors(
        &self,
        r: &Value,
        env: &BTreeMap<String, String>,
        at: &Path,
        applies: bool,
    ) -> (Found, Option<Resolved>) {
        let mut errs = Found::new();
        let tn = if let Some(t) = r.get("type") {
            let tn = py_str(t);
            if !self.kinds.contains_key(&tn) || self.machines.contains(&tn) {
                return (
                    vec![finding(
                        ext(at, segs!["type"]),
                        "names",
                        format!("creates a {tn}, which no module checked declares as a type"),
                    )],
                    None,
                );
            }
            if flag(&self.kinds[&tn], "abstract") {
                return (
                    vec![finding(
                        ext(at, segs!["type"]),
                        "names",
                        format!("creates a {tn}, which is abstract and has no objects"),
                    )],
                    None,
                );
            }
            tn
        } else {
            let o = str_or(r, "object", "");
            match env.get(&o) {
                Some(t) => t.clone(),
                None => {
                    return (
                        vec![finding(
                            ext(at, segs!["object"]),
                            "names",
                            format!("acts on '{o}', which no earlier step gave as an alias"),
                        )],
                        None,
                    );
                }
            }
        };
        let xn = str_or(r, "transition", "");
        let Some(x) = self
            .kinds
            .get(&tn)
            .and_then(|t| obj(t, "transitions").get(&xn))
            .filter(|x| !x.is_null())
            .cloned()
        else {
            return (
                vec![finding(
                    ext(at, segs!["transition"]),
                    "names",
                    format!("requests {tn}.{xn}, which {tn} does not declare"),
                )],
                None,
            );
        };
        let initial = kind(&x) == "initial";
        if r.get("type").is_some() != initial {
            let how = if initial {
                "a creation, requested with the type"
            } else {
                "requested on an object, named by its alias"
            };
            errs.push(finding(
                ext(at, segs!["transition"]),
                "names",
                format!("{tn}.{xn} is {how}"),
            ));
        }
        if applies && flag(&x, "only_via") {
            errs.push(finding(
                ext(at, segs!["transition"]),
                "names",
                format!(
                    "a setup's step must apply, and {tn}.{xn} is only via {}, so a direct request is not requestable",
                    strings(&x, "only_via").join(", ")
                ),
            ));
        }
        if let Some(o) = r.get("occurred_at") {
            errs.extend(self.value_errors(
                o,
                Some(&json!({"type": "timestamp"})),
                env,
                &ext(at, segs!["occurred_at"]),
                "occurred_at",
            ));
        }
        errs.extend(self.actor_errors(r, env, at));
        let attrs = obj(&self.kinds[&tn], "attributes").clone();
        let own = obj(&x, "inputs").clone();
        let builtin: BTreeSet<&str> = if kind(&x) == "assertion" {
            ["to", "admits"].into()
        } else {
            BTreeSet::new()
        };
        let given = obj(r, "inputs").clone();
        let takes = taken(&x);
        for (k, v) in &given {
            let here = ext(at, segs!["inputs", k]);
            if !takes.contains(k) && !builtin.contains(k.as_str()) {
                errs.push(finding(
                    here,
                    "names",
                    format!("{tn}.{xn} takes no input '{k}'"),
                ));
            } else if k == "to" {
                if !targets(&x).contains(&py_str(v)) {
                    errs.push(finding(
                        here,
                        "names",
                        format!(
                            "'{}' is not a state {tn}.{xn} may put an object in",
                            py_str(v)
                        ),
                    ));
                }
            } else if k != "admits" {
                let spec = if own.contains_key(k) {
                    own.get(k)
                } else {
                    attrs.get(k)
                };
                errs.extend(self.value_errors(v, spec, env, &here, k));
            }
        }
        let given_names: BTreeSet<String> = given.keys().cloned().collect();
        let left: BTreeSet<String> = required_input_names(&x)
            .difference(&given_names)
            .cloned()
            .collect();
        let unprovided: Vec<String> = strings(&x, "required_inputs")
            .into_iter()
            .filter(|i| {
                left.contains(i)
                    && !own.contains_key(i)
                    && attrs
                        .get(i)
                        .is_some_and(|a| optional(a) || a.get("default").is_some())
            })
            .collect();
        let mut missing: Vec<String> = left
            .iter()
            .filter(|i| {
                !unprovided.contains(i) && !own.get(*i).is_some_and(|s| s.get("default").is_some())
            })
            .cloned()
            .collect();
        if kind(&x) == "assertion" && !given.contains_key("to") {
            missing.push("to".into());
        }
        (
            errs,
            Some(Resolved {
                tn,
                x,
                missing,
                unprovided,
            }),
        )
    }

    /// The types a transition's effect creates at any depth: through its
    /// loops, the creations it makes and the transitions it calls, each of
    /// which may create in turn (`flow-format.md` §11.3, ADR-0173 decision
    /// 6).
    fn created_types(
        &self,
        tn: &str,
        x: &Value,
        seen: &mut BTreeSet<(String, String)>,
    ) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let t = self.kinds.get(tn).cloned().unwrap_or(Value::Null);
        for step in super::steps::walk(x.get("effect")) {
            let body = step.body();
            let next = match step.kind {
                "create" => {
                    let ct = str_or(body, "type", "");
                    out.insert(ct.clone());
                    (ct, str_or(body, "transition", ""))
                }
                "call" => {
                    let env = super::names::binders(&self.kinds, tn, x, &step.path);
                    let Some(k) = super::names::call_target(&self.kinds, &t, tn, x, body, &env)
                    else {
                        continue;
                    };
                    (k, str_or(body, "transition", ""))
                }
                _ => continue,
            };
            if seen.insert(next.clone()) {
                let called = self
                    .kinds
                    .get(&next.0)
                    .and_then(|t| obj(t, "transitions").get(&next.1))
                    .cloned()
                    .unwrap_or(Value::Null);
                out.extend(self.created_types(&next.0, &called, seen));
            }
        }
        out
    }

    /// A sequence an import raises is one the modules declare, given the
    /// scope it was minted in where it is scoped, and none where it is not
    /// (`flow-format.md` §11.3, `publish-and-import.md` §4).
    fn raise_errors(&self, rs: &Value, env: &BTreeMap<String, String>, at: &Path) -> Found {
        let seq = str_or(rs, "sequence", "");
        if !self.sequences.contains(&seq) {
            return vec![finding(
                ext(at, segs!["sequence"]),
                "names",
                format!("raises the sequence {seq}, which no module checked declares"),
            )];
        }
        let mut scoped: Vec<(String, String, String)> = Vec::new();
        for (tn, t) in &self.kinds {
            for (a, spec) in obj(t, "attributes") {
                let ident = spec.get("identifier").unwrap_or(&Value::Null);
                let scope = str_or(ident, "scope", "");
                if str_or(ident, "sequence", "") == seq && !scope.is_empty() {
                    scoped.push((tn.clone(), a.clone(), scope));
                }
            }
        }
        scoped.sort();
        match (rs.get("scope"), scoped.first()) {
            (Some(_), None) => vec![finding(
                ext(at, segs!["scope"]),
                "names",
                format!("raises {seq} in a scope, and {seq} numbers one series, with no scope"),
            )],
            (None, Some((tn, a, scope))) => vec![finding(
                at.clone(),
                "names",
                format!(
                    "raises {seq} with no scope, and {tn}.{a} numbers it in each value of {scope}"
                ),
            )],
            (Some(v), Some((tn, _, scope))) => {
                let spec = self
                    .kinds
                    .get(tn)
                    .and_then(|t| obj(t, "attributes").get(scope));
                self.value_errors(v, spec, env, &ext(at, segs!["scope"]), "scope")
            }
            (None, None) => Vec::new(),
        }
    }

    /// A decision names a proposal an earlier step filed, as an actor the
    /// store holds (`DESIGN.md` §9).
    fn decision_errors(&self, d: &Value, env: &BTreeMap<String, String>, at: &Path) -> Found {
        let mut errs = Found::new();
        let p = str_or(d, "proposal", "");
        if !env.get(&p).is_some_and(|t| t.starts_with("Proposal:")) {
            errs.push(finding(
                ext(at, segs!["proposal"]),
                "names",
                format!("decides '{p}', which no earlier step filed as a proposal"),
            ));
        }
        errs.extend(self.actor_errors(d, env, at));
        errs
    }

    #[allow(clippy::too_many_lines)]
    fn steps_errors(
        &self,
        steps: &[Value],
        env: &mut BTreeMap<String, String>,
        at: &Path,
    ) -> Found {
        let mut errs = Found::new();
        for (i, st) in steps.iter().enumerate() {
            let here = ext(at, segs![i]);
            // under the version a publish installed, which this check does not
            // read (flow-format.md §11.3)
            if env.contains_key(PUBLISHED) {
                continue;
            }
            if let Some(pb) = st.get("publish") {
                errs.extend(self.actor_errors(pb, env, &ext(&here, segs!["publish"])));
                env.insert(PUBLISHED.into(), String::new());
                continue;
            }
            if let Some(r) = st.get("propose") {
                let (e, found) = self.request_errors(r, env, &ext(&here, segs!["propose"]), false);
                errs.extend(e);
                if let Some(f) = &found
                    && !flag(&f.x, "proposable")
                {
                    errs.push(finding(
                        ext(&here, segs!["propose", "transition"]),
                        "names",
                        format!(
                            "{}.{} is not proposable, so no proposal of it can be filed",
                            f.tn,
                            str_or(r, "transition", "")
                        ),
                    ));
                }
                let alias = str_or(r, "proposal", "");
                if env.contains_key(&alias) || alias == "operator" {
                    errs.push(finding(
                        ext(&here, segs!["propose", "proposal"]),
                        "names",
                        format!("'{alias}' is already an alias"),
                    ));
                }
                if let Some(f) = found {
                    env.insert(
                        alias,
                        format!("Proposal:{}.{}", f.tn, str_or(r, "transition", "")),
                    );
                }
                continue;
            }
            if let Some(verb) = ["approve", "reject", "withdraw"]
                .into_iter()
                .find(|v| st.get(*v).is_some())
            {
                errs.extend(self.decision_errors(&st[verb], env, &ext(&here, segs![verb])));
                let filed = env
                    .get(&str_or(&st[verb], "proposal", ""))
                    .cloned()
                    .unwrap_or_default();
                if verb == "approve"
                    && let Some((tn, xn)) = filed
                        .strip_prefix("Proposal:")
                        .and_then(|l| l.split_once('.'))
                {
                    self.exercised
                        .borrow_mut()
                        .insert((tn.to_string(), xn.to_string()));
                }
                continue;
            }
            if let Some(r) = st.get("request") {
                let at_r = ext(&here, segs!["request"]);
                let (e, found) = self.request_errors(r, env, &at_r, true);
                errs.extend(e);
                let Some(f) = found else { continue };
                self.exercised
                    .borrow_mut()
                    .insert((f.tn.clone(), str_or(r, "transition", "")));
                let left: Vec<String> = f.missing.iter().chain(&f.unprovided).cloned().collect();
                if !left.is_empty() {
                    let plural = if left.len() > 1 { "s" } else { "" };
                    errs.push(finding(
                        at_r.clone(),
                        "names",
                        format!("a setup's step must apply, and it leaves out the required input{plural} {}", left.join(", ")),
                    ));
                }
                if let Some(alias) = r.get("as").map(py_str) {
                    if env.contains_key(&alias) || alias == "operator" {
                        errs.push(finding(
                            ext(&at_r, segs!["as"]),
                            "names",
                            format!("'{alias}' is already an alias"),
                        ));
                    }
                    env.insert(alias, f.tn.clone());
                }
                if let Some(Value::Object(named)) = r.get("creates") {
                    // a mapping from each type to the aliases of its objects
                    // (flow-format.md §11.3, ADR-0173 decision 6)
                    let made = self.created_types(&f.tn, &f.x, &mut BTreeSet::new());
                    for (ct, aliases) in named {
                        if !made.contains(ct) {
                            let what = if made.is_empty() {
                                "none".to_string()
                            } else {
                                format!(
                                    "only {}",
                                    made.iter().cloned().collect::<Vec<_>>().join(", ")
                                )
                            };
                            errs.push(finding(
                                ext(&at_r, segs!["creates", ct]),
                                "names",
                                format!(
                                    "names objects of {ct} {}.{} creates, and its effect creates {what}",
                                    f.tn,
                                    str_or(r, "transition", "")
                                ),
                            ));
                        }
                        for alias in aliases.as_array().into_iter().flatten().map(py_str) {
                            if env.contains_key(&alias)
                                || alias == "operator"
                                || r.get("as").map(py_str).as_deref() == Some(alias.as_str())
                            {
                                errs.push(finding(
                                    ext(&at_r, segs!["creates"]),
                                    "names",
                                    format!("'{alias}' is already an alias"),
                                ));
                            }
                            env.insert(alias, ct.clone());
                        }
                    }
                } else if flag(r, "creates") {
                    let made = self.created_types(&f.tn, &f.x, &mut BTreeSet::new());
                    if made.len() != 1 {
                        let what = if made.is_empty() {
                            "none".to_string()
                        } else {
                            format!(
                                "objects of more than one type, {}",
                                made.iter().cloned().collect::<Vec<_>>().join(", ")
                            )
                        };
                        errs.push(finding(
                            ext(&at_r, segs!["creates"]),
                            "names",
                            format!(
                                "names objects {}.{} creates, and its effect creates {what}",
                                f.tn,
                                str_or(r, "transition", "")
                            ),
                        ));
                    }
                    for alias in strings(r, "creates") {
                        if env.contains_key(&alias)
                            || alias == "operator"
                            || r.get("as").map(py_str).as_deref() == Some(alias.as_str())
                        {
                            errs.push(finding(
                                ext(&at_r, segs!["creates"]),
                                "names",
                                format!("'{alias}' is already an alias"),
                            ));
                        }
                        if made.len() == 1 {
                            env.insert(alias, made.iter().next().cloned().unwrap_or_default());
                        }
                    }
                }
                continue;
            }
            let Some(im) = st.get("import") else { continue };
            let tn = str_or(im, "type", "");
            let ai = ext(&here, segs!["import"]);
            if !self.kinds.get(&tn).is_some_and(|t| flag(t, "mirror")) {
                errs.push(finding(
                    ext(&ai, segs!["type"]),
                    "names",
                    format!("imports a {tn}, and only a mirror type is written by the import"),
                ));
                continue;
            }
            let t = self.kinds[&tn].clone();
            let state = str_or(im, "state", "");
            if !obj(&t, "states").contains_key(&state) {
                errs.push(finding(
                    ext(&ai, segs!["state"]),
                    "names",
                    format!("{tn} declares no state {state}"),
                ));
            }
            let attrs = obj(&t, "attributes");
            for (k, v) in obj(im, "values") {
                match attrs.get(k) {
                    None => errs.push(finding(
                        ext(&ai, segs!["values", k]),
                        "names",
                        format!("{tn} declares no attribute '{k}'"),
                    )),
                    Some(spec) => errs.extend(self.value_errors(
                        v,
                        Some(spec),
                        env,
                        &ext(&ai, segs!["values", k]),
                        k,
                    )),
                }
            }
            let alias = str_or(im, "as", "");
            if env.contains_key(&alias) || alias == "operator" {
                errs.push(finding(
                    ext(&ai, segs!["as"]),
                    "names",
                    format!("'{alias}' is already an alias"),
                ));
            }
            env.insert(alias, tn);
            for (j, rs) in list(im, "sequences").iter().enumerate() {
                errs.extend(self.raise_errors(rs, env, &ext(&ai, segs!["sequences", j])));
            }
        }
        errs
    }

    fn stubs_errors(&self, stubs: Option<&Value>, at: &Path) -> Found {
        super::doc::as_obj(stubs.unwrap_or(&Value::Null))
            .keys()
            .filter(|k| !self.functions.contains(*k))
            .map(|k| {
                finding(
                    ext(at, segs![k]),
                    "names",
                    format!(
                        "stubs {k}, which no module checked declares as an evaluator's function"
                    ),
                )
            })
            .collect()
    }
}

/// An examples file's findings, and the transitions its examples request.
/// The aliases' mark of a `publish` step, after which nothing is checked
/// here; no alias has a space.
const PUBLISHED: &str = " published";

pub fn example_errors(
    doc: &Value,
    library: &Obj,
    modules: &BTreeMap<String, Value>,
) -> (Found, Requested, Exercised) {
    let module = str_or(doc, "examples_for", "");
    if !modules.contains_key(&module) {
        return (
            vec![finding(
                segs!["examples_for"],
                "names",
                format!("examples for {module}, which is not among the modules checked"),
            )],
            Requested::new(),
            Exercised::new(),
        );
    }
    // an observation kind is recorded by its generated creation, `record`
    let mut kinds = library.clone();
    for m in modules.values() {
        for (tn, t) in obj(m, "types") {
            for ob in obj(t, "observations").values() {
                let fields = obj(ob, "attributes").clone();
                let mut attrs = fields.clone();
                attrs.insert("subject".into(), json!({"reference": tn}));
                attrs.insert(
                    "corrects".into(),
                    json!({"reference": str_or(ob, "kind", ""), "optional": true}),
                );
                attrs.insert("occurred_at".into(), json!({"type": "timestamp"}));
                attrs.insert("recorded_at".into(), json!({"type": "timestamp"}));
                let required: Vec<String> = std::iter::once("subject".to_string())
                    .chain(
                        fields
                            .iter()
                            .filter(|(_, f)| !optional(f))
                            .map(|(a, _)| a.clone()),
                    )
                    .collect();
                let optional_inputs: Vec<String> = std::iter::once("corrects".to_string())
                    .chain(
                        fields
                            .iter()
                            .filter(|(_, f)| optional(f))
                            .map(|(a, _)| a.clone()),
                    )
                    .collect();
                let mut transitions = Map::new();
                transitions.insert(
                    "record".into(),
                    json!({"kind": "initial", "to": "RECORDED", "required_inputs": required, "optional_inputs": optional_inputs}),
                );
                if fields.values().any(|f| flag(f, "personal")) {
                    transitions.insert(
                        "forget".into(),
                        json!({"kind": "erasure", "inputs": {"reason": {"type": "string"}}}),
                    );
                }
                kinds.insert(
                    str_or(ob, "kind", ""),
                    json!({"attributes": attrs, "states": {"RECORDED": {"final": true}},
                           "invariants": ob.get("invariants").cloned().unwrap_or(json!({})), "transitions": transitions}),
                );
            }
        }
    }
    let enums: BTreeMap<String, BTreeSet<String>> = modules
        .values()
        .flat_map(|m| {
            obj(m, "enumerations")
                .iter()
                .map(|(n, v)| {
                    (
                        n.clone(),
                        list(&json!({"v": v}), "v").iter().map(py_str).collect(),
                    )
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let functions: BTreeSet<String> = modules
        .values()
        .flat_map(|m| {
            obj(m, "evaluators")
                .iter()
                .flat_map(|(e, ev)| {
                    keys(ev, "functions")
                        .into_iter()
                        .map(move |f| format!("{e}.{f}"))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    let machines: BTreeSet<String> = modules.values().flat_map(|m| keys(m, "machines")).collect();
    let sequences: BTreeSet<String> = modules
        .values()
        .flat_map(|m| keys(m, "sequences"))
        .collect();
    let mut c = Checker {
        kinds,
        enums,
        functions,
        machines,
        sequences,
        exercised: RefCell::new(Exercised::new()),
        out: Found::new(),
    };
    let setups = obj(doc, "setups").clone();
    let order: Vec<String> = setups.keys().cloned().collect();
    let doc_keys: Vec<&String> = super::doc::as_obj(doc)
        .keys()
        .filter(|k| EXAMPLES_ORDER.contains(&k.as_str()))
        .collect();
    let want: Vec<&str> = EXAMPLES_ORDER
        .iter()
        .copied()
        .filter(|k| super::doc::as_obj(doc).contains_key(*k))
        .collect();
    if doc_keys.iter().map(|k| k.as_str()).collect::<Vec<_>>() != want
        && let Some(first) = doc_keys.first()
    {
        c.out.push(finding(
            segs![first.as_str()],
            "order",
            format!(
                "an examples file declares, in this order, {}",
                EXAMPLES_ORDER.join(", ")
            ),
        ));
    }
    let mut envs: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    fn env_of(
        c: &mut Checker,
        name: &str,
        seen: &mut Vec<String>,
        setups: &Map<String, Value>,
        order: &[String],
        envs: &mut BTreeMap<String, BTreeMap<String, String>>,
    ) -> BTreeMap<String, String> {
        if let Some(e) = envs.get(name) {
            return e.clone();
        }
        if seen.iter().any(|s| s == name) {
            c.out.push(finding(
                segs!["setups", name, "given"],
                "names",
                format!("setup {name} is given, through others, itself"),
            ));
            return BTreeMap::new();
        }
        let s = setups[name].clone();
        let mut env = BTreeMap::new();
        if let Some(g) = s.get("given").map(py_str) {
            let position = |n: &str| order.iter().position(|o| o == n);
            if !setups.contains_key(&g) {
                c.out.push(finding(
                    segs!["setups", name, "given"],
                    "names",
                    format!("is given the setup {g}, which this file does not declare"),
                ));
            } else if position(&g) > position(name) {
                c.out.push(finding(
                    segs!["setups", name, "given"],
                    "order",
                    format!("is given the setup {g}, declared after it; a setup is given one declared before it"),
                ));
            } else {
                seen.push(name.to_string());
                env = env_of(c, &g, seen, setups, order, envs);
                seen.pop();
            }
        }
        let stub = c.stubs_errors(s.get("evaluators"), &segs!["setups", name, "evaluators"]);
        c.out.extend(stub);
        let errs = c.steps_errors(list(&s, "steps"), &mut env, &segs!["setups", name, "steps"]);
        c.out.extend(errs);
        envs.insert(name.to_string(), env.clone());
        env
    }
    for name in &order {
        env_of(&mut c, name, &mut Vec::new(), &setups, &order, &mut envs);
    }
    let mut requested: Requested = BTreeMap::new();
    for (name, ex) in obj(doc, "examples").clone() {
        let at = segs!["examples", &name];
        let mut env = BTreeMap::new();
        if let Some(g) = ex.get("given").map(py_str) {
            if setups.contains_key(&g) {
                env = envs.get(&g).cloned().unwrap_or_default();
            } else {
                c.out.push(finding(
                    ext(&at, segs!["given"]),
                    "names",
                    format!("is given the setup {g}, which this file does not declare"),
                ));
            }
        }
        let stub = c.stubs_errors(ex.get("evaluators"), &ext(&at, segs!["evaluators"]));
        c.out.extend(stub);
        let errs = c.steps_errors(list(&ex, "steps"), &mut env, &ext(&at, segs!["steps"]));
        c.out.extend(errs);
        // the request runs under the version published, and is checked when
        // it runs (flow-format.md §11.3)
        if env.contains_key(PUBLISHED) {
            continue;
        }
        let want = ex.get("expect").cloned().unwrap_or(Value::Null);
        let verdict = str_or(&want, "verdict", "");
        let resolved = if let Some(a) = ex.get("approve") {
            let errs = c.decision_errors(a, &env, &ext(&at, segs!["approve"]));
            let failed = !errs.is_empty();
            c.out.extend(errs);
            if failed {
                continue;
            }
            let label = env[&str_or(a, "proposal", "")]
                .trim_start_matches("Proposal:")
                .to_string();
            let (tn, xn) = label
                .split_once('.')
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .unwrap_or_default();
            let x = obj(&c.kinds[&tn], "transitions")[&xn].clone();
            if let Some(left) = want.get("proposal").map(py_str) {
                let right: Vec<&str> = match verdict.as_str() {
                    "applied" => vec!["executed"],
                    "unavailable" => vec!["executed", "rejected", "withdrawn"],
                    _ => vec!["pending"],
                };
                if !right.contains(&left.as_str()) {
                    let how = if verdict == "applied" {
                        "that applies".to_string()
                    } else {
                        format!("refused as {verdict}")
                    };
                    let mut sorted = right.clone();
                    sorted.sort_unstable();
                    c.out.push(finding(
                        ext(&at, segs!["expect", "proposal"]),
                        "names",
                        format!(
                            "an approval {how} leaves its proposal {}, not {left}",
                            sorted.join(" or ")
                        ),
                    ));
                }
            }
            Resolved {
                tn,
                x,
                missing: Vec::new(),
                unprovided: Vec::new(),
            }
        } else {
            let request = ex.get("request").cloned().unwrap_or(Value::Null);
            let (errs, found) =
                c.request_errors(&request, &env, &ext(&at, segs!["request"]), false);
            c.out.extend(errs);
            if flag(&request, "creates") {
                c.out.push(finding(
                    ext(&at, segs!["request", "creates"]),
                    "names",
                    "the request under test names no objects it creates, since no step follows it"
                        .to_string(),
                ));
            }
            if want.get("proposal").is_some() {
                c.out.push(finding(
                    ext(&at, segs!["expect", "proposal"]),
                    "names",
                    "only an approval leaves a proposal to state".to_string(),
                ));
            }
            let Some(f) = found else { continue };
            f
        };
        let xn = if let Some(a) = ex.get("approve") {
            env[&str_or(a, "proposal", "")]
                .rsplit_once('.')
                .map(|(_, x)| x.to_string())
                .unwrap_or_default()
        } else {
            str_or(ex.get("request").unwrap_or(&Value::Null), "transition", "")
        };
        let (tn, x) = (resolved.tn.clone(), resolved.x.clone());
        requested
            .entry((tn.clone(), xn.clone()))
            .or_default()
            .push(name.clone());
        expect_errors(
            &mut c,
            &want,
            &verdict,
            &resolved,
            &tn,
            &xn,
            &x,
            &env,
            &ext(&at, segs!["expect"]),
        );
    }
    let exercised = c.exercised.take();
    (c.out, requested, exercised)
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn expect_errors(
    c: &mut Checker,
    want: &Value,
    verdict: &str,
    resolved: &Resolved,
    tn: &str,
    xn: &str,
    x: &Value,
    env: &BTreeMap<String, String>,
    here: &Path,
) {
    // a left-out input is refused as invalid input, unless a refusal ordered
    // before it decides the request first (D517)
    let earlier = matches!(verdict, "unavailable" | "not_requestable")
        || (verdict == "unsatisfied" && str_or(want, "clause", "") == "actor_known");
    if !resolved.missing.is_empty() && verdict != "invalid_input" && !earlier {
        c.out.push(finding(
            ext(here, segs!["verdict"]),
            "names",
            format!(
                "the request leaves out {}, which is refused as invalid_input, and the example expects {verdict}",
                resolved.missing.join(", ")
            ),
        ));
    } else if !resolved.unprovided.is_empty() && verdict == "applied" {
        c.out.push(finding(
            ext(here, segs!["verdict"]),
            "names",
            format!(
                "the request leaves out {}, which is refused by {}_provided, and the example expects it to apply",
                resolved.unprovided.join(", "),
                resolved.unprovided[0]
            ),
        ));
    }
    let t = c.kinds.get(tn).cloned().unwrap_or(Value::Null);
    if verdict == "applied" {
        for k in ["clause", "remedy"] {
            if want.get(k).is_some() {
                c.out.push(finding(
                    ext(here, segs![k]),
                    "names",
                    format!("an applied request names no {k}"),
                ));
            }
        }
        match want.get("state").map(py_str) {
            None => c.out.push(finding(
                here.clone(),
                "names",
                "an applied request's example states the state its object reaches".to_string(),
            )),
            Some(st) => {
                let allowed: BTreeSet<String> = match kind(x) {
                    "initial" | "external" => [str_or(x, "to", "None")].into(),
                    "internal" => sources(x).into_iter().collect(),
                    "assertion" => targets(x).into_iter().collect(),
                    _ => keys(&t, "states").into_iter().collect(),
                };
                if !allowed.contains(&st) {
                    c.out.push(finding(
                        ext(here, segs!["state"]),
                        "names",
                        format!("{tn}.{xn} does not leave an object in {st}"),
                    ));
                }
            }
        }
        let attrs = obj(&t, "attributes");
        for k in super::doc::as_obj(want.get("values").unwrap_or(&Value::Null)).keys() {
            let known = attrs.contains_key(k)
                || obj(&t, "derived_attributes").contains_key(k)
                || BUILT_IN_MEMBERS.contains(&k.as_str())
                || (k == "recorded_by_kind" && attrs.contains_key("subject"));
            if !known {
                c.out.push(finding(
                    ext(here, segs!["values", k]),
                    "names",
                    format!("{tn} declares no attribute '{k}'"),
                ));
            }
        }
        if let Some(o) = want.get("occurred_at") {
            let e = c.value_errors(
                o,
                Some(&json!({"type": "timestamp"})),
                env,
                &ext(here, segs!["occurred_at"]),
                "occurred_at",
            );
            c.out.extend(e);
        }
        for cc in strings(want, "cascaded") {
            let (ct, cx) = cc.split_once('.').unwrap_or((&cc, ""));
            if !c
                .kinds
                .get(ct)
                .is_some_and(|t| obj(t, "transitions").contains_key(cx))
            {
                c.out.push(finding(
                    ext(here, segs!["cascaded"]),
                    "names",
                    format!("{cc} names no transition a module checked declares"),
                ));
            } else {
                c.exercised
                    .borrow_mut()
                    .insert((ct.to_string(), cx.to_string()));
            }
        }
        for w in strings(want, "warned") {
            let (ct, cn) = w.rsplit_once('.').unwrap_or(("", &w));
            if ct.is_empty() {
                if obj(x, "guards").get(cn).map(py_str).as_deref() != Some("warn") {
                    c.out.push(finding(
                        ext(here, segs!["warned"]),
                        "names",
                        format!("{cn} is not a guard {tn}.{xn} warns by"),
                    ));
                }
            } else if !c
                .kinds
                .get(ct)
                .is_some_and(|t| obj(t, "conditions").contains_key(cn))
            {
                c.out.push(finding(
                    ext(here, segs!["warned"]),
                    "names",
                    format!("{w} names no condition of a type a module checked declares"),
                ));
            }
        }
        return;
    }
    for k in ["state", "values", "cascaded", "warned", "occurred_at"] {
        if want.get(k).is_some() {
            let what = if k == "state" { "state" } else { k };
            c.out.push(finding(
                ext(here, segs![k]),
                "names",
                format!("a request refused as {verdict} leaves no {what} to state"),
            ));
        }
    }
    if verdict == "not_requestable" && !flag(x, "only_via") {
        c.out.push(finding(
            ext(here, segs!["verdict"]),
            "names",
            format!("{tn}.{xn} is not only via others, so it is never refused as not_requestable"),
        ));
    }
    match verdict {
        "unsatisfied" => {
            let (Some(clause), Some(remedy)) = (
                want.get("clause").map(py_str),
                want.get("remedy").map(py_str),
            ) else {
                c.out.push(finding(here.clone(), "names", "an unsatisfied request's example names the clause that refuses it and its remedy".to_string()));
                return;
            };
            let (ct, cn) = match clause.split_once('.') {
                Some((a, b)) => (a.to_string(), b.to_string()),
                None => (tn.to_string(), clause.clone()),
            };
            let conds = c
                .kinds
                .get(&ct)
                .map(|t| obj(t, "conditions").clone())
                .unwrap_or_default();
            let provided: BTreeSet<String> = strings(x, "required_inputs")
                .into_iter()
                .map(|a| format!("{a}_provided"))
                .collect();
            let functions = c.functions.clone();
            let asks = |cond: Option<&Value>| {
                let text = squash(
                    &cond
                        .map(|c| str_or(c, "expression", ""))
                        .unwrap_or_default(),
                );
                remedy == "temporal"
                    && functions
                        .iter()
                        .any(|f| re(&format!(r"\b{}\(", regex::escape(f))).is_match(&text))
            };
            if ct == tn && obj(x, "guards").contains_key(&cn) {
                let declared = conds
                    .get(&cn)
                    .map(|c| str_or(c, "remedy", ""))
                    .filter(|r| !r.is_empty());
                if let Some(d) = declared
                    && d != remedy
                    && !asks(conds.get(&cn))
                {
                    c.out.push(finding(
                        ext(here, segs!["remedy"]),
                        "names",
                        format!("{tn}.{cn} declares the remedy {d}, not {remedy}"),
                    ));
                }
            } else if ct != tn && conds.contains_key(&cn) {
                let declared = str_or(&conds[&cn], "remedy", "None");
                if declared != remedy && !asks(conds.get(&cn)) {
                    c.out.push(finding(
                        ext(here, segs!["remedy"]),
                        "names",
                        format!("{ct}.{cn} declares the remedy {declared}, not {remedy}"),
                    ));
                }
            } else if !GENERATED_GUARDS.contains(&cn.as_str()) && !provided.contains(&cn) {
                c.out.push(finding(
                    ext(here, segs!["clause"]),
                    "names",
                    format!("{clause} is not a guard of {tn}.{xn}, a generated guard, or a condition of a type its effect reaches"),
                ));
            }
        }
        "invariant_violated" => {
            let Some(clause) = want.get("clause").map(py_str) else {
                c.out.push(finding(
                    here.clone(),
                    "names",
                    "an invariant_violated request's example names the invariant".to_string(),
                ));
                return;
            };
            let (ct, cn) = match clause.split_once('.') {
                Some((a, b)) => (a.to_string(), b.to_string()),
                None => (tn.to_string(), clause.clone()),
            };
            let t = c.kinds.get(&ct).cloned().unwrap_or(Value::Null);
            let mut generated: BTreeSet<String> = obj(&t, "states")
                .iter()
                .filter(|(_, v)| flag(v, "required_attributes"))
                .map(|(s, _)| format!("{}_invariant", s.to_lowercase()))
                .collect();
            generated.extend(
                obj(&t, "attributes")
                    .iter()
                    .filter(|(_, v)| flag(v, "unique"))
                    .map(|(a, _)| format!("{a}_unique")),
            );
            if !obj(&t, "invariants").contains_key(&cn) && !generated.contains(&cn) {
                c.out.push(finding(
                    ext(here, segs!["clause"]),
                    "names",
                    format!("{clause} is not an invariant of {ct}"),
                ));
            }
        }
        "call_refused" if want.get("clause").is_some() => {
            let clause = str_or(want, "clause", "");
            let (ct, cn) = clause.split_once('.').unwrap_or((&clause, ""));
            let t = c.kinds.get(ct).cloned().unwrap_or(Value::Null);
            let known = obj(&t, "conditions").contains_key(cn)
                || obj(&t, "invariants").contains_key(cn)
                || GENERATED_GUARDS.contains(&cn);
            if cn.is_empty()
                || (!known
                    && !(cn.ends_with("_provided")
                        || cn.ends_with("_invariant")
                        || cn.ends_with("_unique")))
            {
                c.out.push(finding(
                    ext(here, segs!["clause"]),
                    "names",
                    format!("{clause} is not a condition, invariant or generated guard of a type, written <Type>.<clause>"),
                ));
            }
        }
        _ if want.get("clause").is_some()
            || (want.get("remedy").is_some() && verdict != "call_refused") =>
        {
            c.out.push(finding(
                here.clone(),
                "names",
                format!("a request refused as {verdict} names no clause or remedy"),
            ));
        }
        _ => {}
    }
}

/// The transitions of the examined module's types that no example requests
/// under test, in two lists: those no example takes at all, and those a
/// setup, a step or an expected cascade takes (ADR-0173 decision 6);
/// a bound machine's counted for each type that binds it.
pub fn uncovered(
    mdoc: &Value,
    kinds: &Obj,
    requested: &Requested,
    exercised: &Exercised,
) -> (Vec<String>, Vec<String>) {
    let (mut untaken, mut untested) = (Vec::new(), Vec::new());
    for tn in keys(mdoc, "types") {
        let Some(t) = kinds.get(&tn) else { continue };
        if flag(t, "abstract") {
            continue;
        }
        for xn in keys(t, "transitions") {
            let key = (tn.clone(), xn.clone());
            if requested.contains_key(&key) {
                continue;
            }
            if exercised.contains(&key) {
                untested.push(format!("{tn}.{xn}"));
            } else {
                untaken.push(format!("{tn}.{xn}"));
            }
        }
    }
    untaken.sort();
    untested.sort();
    (untaken, untested)
}

/// The coverage notice's two parts, each where it names a transition
/// (`flow-format.md` §11.5).
pub fn coverage_notices(module: &str, untaken: &[String], untested: &[String]) -> Vec<String> {
    let n = |left: &[String]| {
        let s = if left.len() == 1 { "" } else { "s" };
        format!("{} transition{s} of {module}", left.len())
    };
    let mut out = Vec::new();
    if !untaken.is_empty() {
        out.push(format!(
            "no example takes {}, by its request, a step, a setup or a cascade it expects: {}",
            n(untaken),
            untaken.join(", ")
        ));
    }
    if !untested.is_empty() {
        out.push(format!(
            "no example requests {} as the request under test, which only a setup, a step or an expected cascade takes: {}",
            n(untested),
            untested.join(", ")
        ));
    }
    out
}
