#!/usr/bin/env python3
"""Contradictions within one type: conditions that can never hold together (ADR-0131 decisions 7 to 9, ADR-0137).

Three shapes are decided, each over one type's rules:
- a transition no request can take: from its states, with its required inputs, its blocking guards cannot all
  be true, or what it writes makes an invariant false whatever the request;
- a state no object can be in: its invariants cannot all hold at once, and a transition enters it;
- a guard that audits or warns and can never pass when its transition is taken, which is reported.

The analysis is sound in one direction, the one a refusal needs: it says "cannot hold" only when it has proved
it. A condition is expanded into cases, each a conjunction of atoms, under the three-valued logic of
declaration-syntax.md §8.2: a guard passes only when true, an invariant holds unless false, and a comparison is
unknown when a value it reads is absent. The atoms it decides are: a value present or absent; a state, an
enumeration value or a boolean within a set; a string or identity equal or not to a literal; and a difference
bound u - v <= c between two ordered values of one kind, or one and a constant, which decides comparisons of
numbers, money, durations and timestamps, and of counts over the object's own collections. Anything else, a path
to another object, a metric, an evaluator, `all`, arithmetic beyond adding a constant, is an opaque atom that may
be true, false or unknown, so it can only make conditions look satisfiable. A derived attribute is replaced by
its expression. Nothing is assumed of what an earlier request wrote beyond what the declaration makes it: an
invariant may stand violated under an admission, so an invariant counts against a transition only where it reads
nothing but what that transition writes and the state it enters.
"""
import math
from fractions import Fraction

import flowexpr

LIMIT = 4096            # the cases a set of conditions may expand to before it is reported rather than decided
SECONDS = {"s": 1, "min": 60, "h": 3600, "day": 86400, "days": 86400, "week": 604800, "weeks": 604800}
ZERO = ("zero",)
NOT = {"T": "F", "F": "T", "U": "U"}
UNKNOWN_VALUE = ("unknown value",)   # what a write leaves where the analysis cannot follow it


class TooLarge(Exception):
    pass


def product(case_lists):
    """Every conjunction taking one case from each list; an empty list makes the whole empty."""
    out = [frozenset()]
    for cases in case_lists:
        out = [a | b for a in out for b in cases]
        if len(out) > LIMIT:
            raise TooLarge()
        if not out:
            return []
    return out


def union(*case_lists):
    out = [c for cases in case_lists for c in cases]
    if len(out) > LIMIT:
        raise TooLarge()
    return out


def names_in(n):
    """The bare names an expression reads at the start of a path, a loop's own binder excepted."""
    if not isinstance(n, tuple) or not n:
        return set()
    k = n[0]
    if k == "name":
        return {n[1]}
    if k == "path":
        return names_in(n[1])
    if k == "agg":
        inner = (names_in(n[5]) if n[5] else set()) | (names_in(n[6]) if n[6] else set())
        return (names_in(n[4]) if n[4] else set()) | (inner - {n[3]})
    if k == "metric":
        return set().union(*(names_in(t) for _d, t in n[2]))
    if k == "changed":
        return set(n[1]) | names_in(n[2])
    if k == "call":
        return (set() if n[1][0] == "name" else names_in(n[1])).union(*(names_in(a) for a in n[2]))
    if k == "set":
        return set().union(*(names_in(i) for i in n[1]))
    return set().union(*(names_in(p) for p in n[1:] if isinstance(p, tuple)))


class TypeModel:
    """What the analysis knows of one type: the domain of each value a condition may read."""

    def __init__(self, tn, t, enums):
        self.tn, self.t, self.enums = tn, t, enums
        self.attrs = t.get("attributes") or {}
        self.derived = t.get("derived_attributes") or {}
        self.states = t.get("states") or {}
        self.categories = {v.get("category") for v in self.states.values()} | {"closed"}
        self.open_states = frozenset(s for s, v in self.states.items() if v.get("category") != "closed" and not v.get("final"))
        self.inputs = {}            # input name -> spec, for the transition under analysis
        self.present = set()        # the inputs a request must supply, so present when guards run
        self.created = False        # a creation: no value of the object exists before it

    def kind(self, spec, nullable):
        """(kind, domain, nullable); kind is finite, int, rat or eq, and None where a value is not modelled."""
        if not spec:
            return None
        if "reference" in spec:
            return None if spec["reference"].endswith("[]") else ("eq", None, nullable)
        typ = spec.get("type", "")
        if typ.endswith("[]"):
            return None
        if typ in self.enums:
            return ("finite", frozenset(self.enums[typ]), nullable)
        if typ == "bool":
            return ("finite", frozenset({True, False}), nullable)
        if typ == "counter":
            return ("int", "int", False)
        if typ == "int":
            return ("int", "int", nullable)
        base = typ.split("(")[0]
        if base in ("decimal", "money", "timestamp", "duration"):
            return ("rat", base, nullable)
        if typ in ("string", "identity", "file"):
            return ("eq", None, nullable)
        return None

    def var_info(self, var):
        if var[0] == "state":
            return ("finite", frozenset(self.states), False)
        if var[0] == "now":
            return ("rat", "timestamp", False)
        if var[0] == "entered":
            return ("rat", "timestamp", True)
        if var[0] == "count":
            return ("int", "int", True)
        if var[0] == "attr":
            spec = self.attrs.get(var[1]) or {}
            return self.kind(spec, self.created or bool(spec.get("optional")))
        if var[0] == "in":
            return self.kind(self.inputs.get(var[1]), var[1] not in self.present)
        return None


class Analysis:
    """A condition as cases, over one type and one transition's inputs."""

    def __init__(self, model, written=None, fixed_state=None):
        self.m = model
        self.written = written or {}        # attribute -> the tree of its value after the transition's writes
        self.fixed_state = fixed_state      # the state the object is known to be in, after a transition
        self.opaque_seen = False
        self.inlining = set()

    # ── the values a condition reads ────────────────────────────────────────
    def resolve(self, e):
        """A value node as ("var", var), ("lit", kind, value), ("null",) or None where it is not modelled."""
        m = self.m
        k = e[0]
        if e == UNKNOWN_VALUE:
            return None
        if k == "null":
            return ("null",)
        if k == "name":
            n = e[1]
            if n == "now":
                return ("var", ("now",))
            if n == "state":
                return ("lit", "state", self.fixed_state) if self.fixed_state else ("var", ("state",))
            if n in m.attrs:
                return self.resolve(self.written[n]) if n in self.written else ("var", ("attr", n))
            if n in m.states:
                return ("lit", "state", n)
            if n in m.categories:
                return ("lit", "category", n)
            return None
        if k == "path":
            base, member = e[1], e[2]
            if base == ("name", "inputs"):
                return ("var", ("in", member)) if member in m.inputs else None
            if base == ("name", "this"):
                return self.resolve(("name", member))
            if base[0] == "name" and base[1] in m.enums and member in m.enums[base[1]]:
                return ("lit", "enum", member)
            if base == ("name", m.tn) and member in m.states:
                return ("lit", "state", member)
            if base == ("name", "state") and member == "category" and self.fixed_state:
                return ("lit", "category", m.states[self.fixed_state].get("category"))
            return None
        if k == "call" and e[1] == ("name", "entered_at") and len(e[2]) == 1 and e[2][0][0] == "name" \
                and e[2][0][1] in m.states and not self.fixed_state:
            return ("var", ("entered", e[2][0][1]))
        if k == "num":
            return ("lit", "number", Fraction(str(e[1])))
        if k == "dur":
            return ("lit", "duration", Fraction(str(e[1])) * SECONDS[e[2]])
        if k == "money":
            return ("lit", "money", Fraction(e[2]))
        if k == "str":
            return ("lit", "string", e[1])
        if k == "bool":
            return ("lit", "bool", e[1])
        return None

    def order_kind(self, var):
        info = self.m.var_info(var)
        return None if info is None or info[0] not in ("int", "rat") else info[1]

    def linear(self, e):
        """An ordered value as (variable or None, constant, kind): x, a literal, or x ± a constant."""
        if e[0] == "bin" and e[1] in ("+", "-"):
            a, b = self.linear(e[2]), self.linear(e[3])
            if a is None or b is None or b[0] is not None:
                return None
            kind = sum_kind(a[2], b[2])
            return None if kind is None else (a[0], a[1] + b[1] if e[1] == "+" else a[1] - b[1], kind)
        if e[0] == "agg" and e[1] == "count" and e[3] is not None and self.own_collection(e[4]):
            return (("count", repr((e[4], e[5]))), Fraction(0), "int")
        r = self.resolve(e)
        if r is None or r[0] == "null":
            return None
        if r[0] == "var":
            kind = self.order_kind(r[1])
            return None if kind is None else (r[1], Fraction(0), kind)
        if r[1] in ("number", "duration", "money"):
            return (None, r[2], r[1])
        return None

    def own_collection(self, e):
        return e[0] == "name" and (e[1] in self.m.attrs or e[1] in (self.m.t.get("observations") or {}))

    def finite_value(self, info, lit):
        if info is None or info[0] != "finite" or lit is None or lit[0] != "lit":
            return None
        if lit[1] in ("state", "enum", "bool") and lit[2] in info[1]:
            return lit[2]
        return None

    # ── a condition as cases ────────────────────────────────────────────────
    def opaque(self, e, want):
        self.opaque_seen = True
        return [frozenset({("op", repr(e), want)})]

    def cases(self, e, want):
        """The conjunctions of atoms under which e has the value want: T, F or U."""
        k = e[0]
        if k == "bin" and e[1] == "implies":
            return self.cases(("bin", "or", ("not", e[2]), e[3]), want)
        if k == "bin" and e[1] in ("and", "or"):
            a, b = e[2], e[3]
            short, other = ("F", "T") if e[1] == "and" else ("T", "F")
            if want == short:
                return union(self.cases(a, short), self.cases(b, short))
            if want == other:
                return product([self.cases(a, other), self.cases(b, other)])
            return union(product([self.cases(a, "U"), self.cases(b, other)]),
                         product([self.cases(a, other), self.cases(b, "U")]),
                         product([self.cases(a, "U"), self.cases(b, "U")]))
        if k == "not":
            return self.cases(e[1], NOT[want])
        if k == "bool":
            return [frozenset()] if want == ("T" if e[1] else "F") else []
        if k == "isnull":
            r = self.resolve(e[1])
            if r == ("null",):
                return [frozenset()] if want == ("F" if e[2] else "T") else []
            if r is not None and r[0] == "var" and self.m.var_info(r[1]) is not None:
                if want == "U":
                    return []
                absent = (want == "T") != e[2]
                return [frozenset({("null" if absent else "present", r[1])})]
            return self.opaque(e, want)
        if k == "bin" and e[1] in flowexpr.COMPARISONS:
            return self.compare(e, want)
        if k == "in":
            return self.membership(e, want)
        if k == "agg" and e[1] in ("none", "any") and e[3] is not None and e[6] is None and self.own_collection(e[4]):
            count = ("count", repr((e[4], e[5])))
            if want == "U":
                return [frozenset({("null", count)})]
            empty = (e[1] == "none") == (want == "T")
            bound = ("le", count, ZERO, Fraction(0), False) if empty else ("le", ZERO, count, Fraction(-1), False)
            return [frozenset({("present", count), bound})]
        if k == "name" and e[1] == "open" and "open" not in self.m.attrs:
            if want == "U":
                return []
            if self.fixed_state:
                return [frozenset()] if (self.fixed_state in self.m.open_states) == (want == "T") else []
            allowed = self.m.open_states if want == "T" else frozenset(self.m.states) - self.m.open_states
            return [frozenset({("dom", ("state",), allowed)})]
        if k == "name" and e[1] in self.m.derived and e[1] not in self.inlining:
            self.inlining.add(e[1])
            try:
                return self.cases(flowexpr.parse(self.m.derived[e[1]]["expression"]), want)
            finally:
                self.inlining.discard(e[1])
        r = self.resolve(e)
        if r is not None and r[0] == "var":
            info = self.m.var_info(r[1])
            if info is not None and info[0] == "finite" and info[1] == frozenset({True, False}):
                if want == "U":
                    return [frozenset({("null", r[1])})]
                return [frozenset({("present", r[1]), ("dom", r[1], frozenset({want == "T"}))})]
        if r is not None and r[0] == "lit" and r[1] == "bool":
            return [frozenset()] if want == ("T" if r[2] else "F") else []
        return self.opaque(e, want)

    def compare(self, e, want):
        op, left, right = e[1], e[2], e[3]
        a, b = self.resolve(left), self.resolve(right)
        if ("null",) in (a, b):
            return [frozenset()] if want == "U" else []
        if a is not None and b is not None and a[0] == b[0] == "lit" and op in ("==", "!=") and a[1] == b[1]:
            truth = (a[2] == b[2]) == (op == "==")
            return [frozenset()] if want == ("T" if truth else "F") else []
        if a is not None and b is not None and {a[0], b[0]} == {"var", "lit"} and op in ("==", "!="):
            var, lit = (a, b) if a[0] == "var" else (b, a)
            info = self.m.var_info(var[1])
            value = self.finite_value(info, lit)
            if value is not None:
                if want == "U":
                    return [frozenset({("null", var[1])})]
                keep = (op == "==") == (want == "T")
                allowed = frozenset({value}) if keep else info[1] - {value}
                return [frozenset({("present", var[1]), ("dom", var[1], allowed)})]
            if info is not None and info[0] == "eq" and lit[1] == "string":
                if want == "U":
                    return [frozenset({("null", var[1])})]
                keep = (op == "==") == (want == "T")
                return [frozenset({("present", var[1]), ("eqv" if keep else "nev", var[1], lit[2])})]
        if left[0] == "path" and left[1] == ("name", "state") and left[2] == "category" and op in ("==", "!=") \
                and b is not None and b[0] == "lit" and b[1] == "category" and not self.fixed_state:
            if want == "U":
                return []
            with_it = frozenset(s for s, v in self.m.states.items() if v.get("category") == b[2])
            allowed = with_it if (op == "==") == (want == "T") else frozenset(self.m.states) - with_it
            return [frozenset({("dom", ("state",), allowed)})]
        la, lb = self.linear(left), self.linear(right)
        if la is not None and lb is not None and (la[0] is not None or lb[0] is not None) and comparable(la[2], lb[2]):
            return self.ordered(op, la, lb, want)
        return self.opaque(e, want)

    def ordered(self, op, la, lb, want):
        """u + cu op v + cv, as difference bounds on u - v."""
        u, cu, _ = la
        v, cv, _ = lb
        if want == "U":
            return [frozenset({("null", x)}) for x in (u, v) if x is not None]
        present = frozenset(("present", x) for x in (u, v) if x is not None)
        uu, vv, c = u or ZERO, v or ZERO, cv - cu
        le = lambda strict: ("le", uu, vv, c, strict)       # u - v <= c
        ge = lambda strict: ("le", vv, uu, -c, strict)      # v - u <= -c
        relation = {"<=": [[le(False)]], "<": [[le(True)]], ">=": [[ge(False)]], ">": [[ge(True)]],
                    "==": [[le(False), ge(False)]], "!=": [[le(True)], [ge(True)]]}
        negation = {"<=": ">", "<": ">=", ">=": "<", ">": "<=", "==": "!=", "!=": "=="}
        return [present | frozenset(atoms) for atoms in relation[op if want == "T" else negation[op]]]

    def membership(self, e, want):
        a = self.resolve(e[1])
        right = e[2]
        info = self.m.var_info(a[1]) if a is not None and a[0] == "var" else None
        if a is not None and a[0] == "lit" and right[0] == "set":
            values = [self.resolve(i) for i in right[1]]
            if all(v is not None and v[0] == "lit" and v[1] == a[1] for v in values):
                truth = any(v[2] == a[2] for v in values)
                return [frozenset()] if want == ("T" if truth else "F") else []
            return self.opaque(e, want)
        if info is None or info[0] != "finite":
            return self.opaque(e, want)
        if right[0] == "set":
            values = [self.finite_value(info, self.resolve(i)) for i in right[1]]
            if any(x is None for x in values):
                return self.opaque(e, want)
            members = frozenset(values)
        elif right[0] == "name" and right[1] in self.m.enums and info[1] == frozenset(self.m.enums[right[1]]):
            members = info[1]
        else:
            return self.opaque(e, want)
        if want == "U":
            return [frozenset({("null", a[1])})]
        allowed = members if want == "T" else info[1] - members
        return [frozenset({("present", a[1]), ("dom", a[1], allowed)})]


NUMERIC = {"int", "decimal"}


def sum_kind(a, b):
    """The kind of a ± b, b a constant, or None where the model's types do not add (declaration-syntax.md §8.3)."""
    if a == "timestamp" and b == "duration":
        return "timestamp"
    if a == b and a in ("duration", "money", "number"):
        return a
    if b == "number" and a in NUMERIC:
        return a
    return None


def comparable(a, b):
    """Whether two ordered kinds compare: one kind, or a numeric literal against a number or an amount."""
    if a == b:
        return a != "number"
    if "number" in (a, b):
        return (b if a == "number" else a) in NUMERIC | {"money"}
    return False


def satisfiable(case, model):
    """Whether a conjunction of atoms can hold: presence, finite domains, equality and difference bounds."""
    presence, domains, equal, unequal, bounds, opaque = {}, {}, {}, {}, [], {}
    for atom in case:
        k = atom[0]
        if k in ("null", "present"):
            if presence.setdefault(atom[1], k == "present") != (k == "present"):
                return False
        elif k == "dom":
            domains[atom[1]] = domains.get(atom[1], atom[2]) & atom[2]
        elif k == "eqv":
            if equal.setdefault(atom[1], atom[2]) != atom[2]:
                return False
        elif k == "nev":
            unequal.setdefault(atom[1], set()).add(atom[2])
        elif k == "le":
            bounds.append(atom[1:])
        elif k == "op":
            if opaque.setdefault(atom[1], atom[2]) != atom[2]:
                return False
    for var, present in presence.items():
        info = model.var_info(var)
        if not present and info is not None and not info[2]:
            return False
    if any(not d for d in domains.values()):
        return False
    if any(v in unequal.get(var, ()) for var, v in equal.items()):
        return False
    counts = {x for b in bounds for x in b[:2] if x[0] == "count"}
    return consistent(bounds + [(ZERO, c, Fraction(0), False) for c in counts], model)


def consistent(bounds, model):
    """Difference bounds u - v <= c, strict where marked, have a solution: no negative cycle (Floyd-Warshall).
    Over integers a strict bound tightens to the next integer, which is exact there and never used otherwise."""
    if not bounds:
        return True
    nodes = sorted({x for b in bounds for x in b[:2]}, key=repr)
    index = {n: i for i, n in enumerate(nodes)}
    integer = all(n == ZERO or (model.var_info(n) or ("",))[0] == "int" for n in nodes)
    size = len(nodes)
    dist = [[None] * size for _ in range(size)]
    for i in range(size):
        dist[i][i] = (Fraction(0), False)

    def better(a, b):
        return b is None or a[0] < b[0] or (a[0] == b[0] and a[1] and not b[1])
    for u, v, c, strict in bounds:
        c = Fraction(c)
        if integer:
            w = (Fraction(math.ceil(c) - 1), False) if strict else (Fraction(math.floor(c)), False)
        else:
            w = (c, strict)
        i, j = index[v], index[u]          # u - v <= c: an edge from v to u of weight c
        if better(w, dist[i][j]):
            dist[i][j] = w
    for k in range(size):
        for i in range(size):
            if dist[i][k] is None:
                continue
            for j in range(size):
                if dist[k][j] is None:
                    continue
                w = (dist[i][k][0] + dist[k][j][0], dist[i][k][1] or dist[k][j][1])
                if better(w, dist[i][j]):
                    dist[i][j] = w
    return all(dist[i][i][0] > 0 or (dist[i][i][0] == 0 and not dist[i][i][1]) for i in range(size))


def any_satisfiable(requirements, model):
    """Whether requirements, each (name, the cases that satisfy it), can all hold at once."""
    return any(satisfiable(case, model) for case in product([cases for _name, cases in requirements]))


def core(requirements, model):
    """A smallest set of the requirements, by deletion, that still cannot hold together."""
    kept = list(requirements)
    for r in list(kept):
        trial = [x for x in kept if x is not r]
        if trial and not any_satisfiable(trial, model):
            kept = trial
    return kept


def state_invariants(t):
    """A type's invariants by name, a state's required attributes among them as the invariant they generate."""
    out = {n: flowexpr.parse(i["expression"]) for n, i in (t.get("invariants") or {}).items()}
    for s, v in (t.get("states") or {}).items():
        if v.get("required_attributes"):
            out[f"{s.lower()}_invariant"] = flowexpr.parse(
                f"state != {s} or (" + " and ".join(f"{a} is not null" for a in v["required_attributes"]) + ")")
    return out


def together(names):
    """How many cannot hold together: both, or all."""
    return "both" if len(names) == 2 else "all"


def holds(analysis, tree):
    """The cases under which an invariant holds: true, or unknown."""
    return union(analysis.cases(tree, "T"), analysis.cases(tree, "U"))


def analyse_type(tn, t, enums):
    """Findings for one type, as (path, severity, code, message), and how many of its guards' conditions were
    decided in full rather than with an opaque part."""
    out, whole, total = [], 0, 0
    states = t.get("states") or {}
    conditions = t.get("conditions") or {}
    invariants = state_invariants(t)
    transitions = t.get("transitions") or {}

    for s in states:
        model = TypeModel(tn, t, enums)
        a = Analysis(model)
        here = ("types", tn, "states", s)
        try:
            reqs = [("in " + s, [frozenset({("dom", ("state",), frozenset({s}))})])]
            reqs += [(f"invariant {n}", holds(a, tree)) for n, tree in invariants.items()]
            if len(reqs) > 1 and not any_satisfiable(reqs, model):
                entering = sorted(xn for xn, x in transitions.items()
                                  if x["kind"] in ("initial", "external") and x.get("to") == s)
                if entering:
                    names = [r[0].split(" ", 1)[1] for r in core(reqs, model) if r[0].startswith("invariant")]
                    out.append((here, "fatal", "contradiction",
                                f"no object of {tn} can be in {s}: {' and '.join(names)} cannot {together(names)} hold there, "
                                f"and {', '.join(entering)} enter{'s' if len(entering) == 1 else ''} it"))
        except TooLarge:
            out.append((here, "notice", "undecided", f"the invariants of {tn} in {s} expand to more than {LIMIT} cases, "
                                                     "so whether they can hold together is not decided"))

    for xn, x in transitions.items():
        if x["kind"] in ("assertion", "erasure"):
            continue
        model = TypeModel(tn, t, enums)
        model.created = x["kind"] == "initial"
        declared = x.get("inputs") or {}
        model.inputs = {**{i: model.attrs.get(i) for i in x.get("required_inputs", []) + x.get("optional_inputs", [])},
                        **declared}
        model.present = set(x.get("required_inputs", [])) | {i for i, sp in declared.items()
                                                              if not (sp or {}).get("optional") or "default" in (sp or {})}
        here = ("types", tn, "transitions", xn)
        froms = x.get("from")
        froms = [] if froms is None else [froms] if isinstance(froms, str) else list(froms)
        try:
            a = Analysis(model)
            reqs = [] if x["kind"] == "initial" else [("from " + ", ".join(froms),
                                                      [frozenset({("dom", ("state",), frozenset(froms))})])]
            denied, flagged = [], []
            for g, mode in (x.get("guards") or {}).items():
                if g not in conditions:
                    continue
                a.opaque_seen = False
                r = (f"guard {g}", a.cases(flowexpr.parse(conditions[g]["expression"]), "T"))
                total += 1
                whole += not a.opaque_seen
                (denied if mode == "deny" else flagged).append((g, mode, r))
            reqs += [r for _g, _m, r in denied]
            if not any_satisfiable(reqs, model):
                names = [r[0] for r in core(reqs, model)]
                out.append((here, "fatal", "contradiction",
                            f"{tn}.{xn} can never be taken: {' and '.join(names)} cannot {together(names)} hold"
                            if len(names) > 1 else f"{tn}.{xn} can never be taken: its {names[0]} can never hold"))
                continue
            for g, mode, r in flagged:
                if not any_satisfiable(reqs + [r], model):
                    out.append((here + ("guards", g), "notice", "neverpasses",
                                f"{tn}.{xn}'s {mode} guard {g} can never pass when the transition is taken, so it "
                                + ("records a would-be refusal" if mode == "audit" else "is raised") + " on every request that applies"))
            # what the transition writes, against the invariants that read nothing but it and the state it enters
            written = {i: ("path", ("name", "inputs"), i) for i in x.get("required_inputs", [])}
            assigned = {}
            for st in x.get("effect") or []:
                if "assign" in st:
                    assigned[st["assign"]["location"]] = flowexpr.parse(st["assign"]["expr"])
                    written[st["assign"]["location"]] = assigned[st["assign"]["location"]]
                elif "clear" in st:
                    written.update({c: ("null",) for c in st["clear"]})
            for loc, tree in assigned.items():
                if names_in(tree) & set(written):       # read in the effect's own order, which is not followed here
                    written[loc] = UNKNOWN_VALUE
            post = x.get("to") if x["kind"] in ("initial", "external") else None
            after = Analysis(model, written, post)
            post_reqs = list(reqs)
            for n, tree in invariants.items():
                read = names_in(tree) - {"inputs", "now", "this"}
                if read and read <= set(written) | ({"state"} if post else set()) | set(model.states):
                    post_reqs.append((f"invariant {n}", holds(after, tree)))
            if len(post_reqs) > len(reqs) and not any_satisfiable(post_reqs, model):
                names = [r[0] for r in core(post_reqs, model)]
                broken = [n.split(" ", 1)[1] for n in names if n.startswith("invariant")]
                others = [n for n in names if not n.startswith("invariant")]
                out.append((here, "fatal", "contradiction",
                            f"{tn}.{xn} can never be taken: what it writes"
                            + (f" leaves {post}, where" if post else ",") + f" {' and '.join(broken)} cannot hold"
                            + (f", with {' and '.join(others)}" if others else "") + ", whatever the request"))
        except TooLarge:
            out.append((here, "notice", "undecided", f"the conditions of {tn}.{xn} expand to more than {LIMIT} cases, "
                                                     "so whether they can hold together is not decided"))
    return out, whole, total


# ── the analysis proven on small types, each decided one way ──────────────────
def _type(attrs=None, states=None, conditions=None, transitions=None, invariants=None, derived=None):
    return {"attributes": attrs or {}, "states": states or {"OPEN": {"category": "live"}, "DONE": {"category": "closed", "final": True}},
            "conditions": {n: {"expression": e, "remedy": "dependent"} for n, e in (conditions or {}).items()},
            "transitions": transitions or {}, "invariants": {n: {"expression": e} for n, e in (invariants or {}).items()},
            "derived_attributes": {n: {"expression": e} for n, e in (derived or {}).items()}}


def _taking(*guards, frm="OPEN", to="DONE", flags=(), **extra):
    x = {"kind": "external", "from": frm, "to": to, "guards": {g: "deny" for g in guards}}
    x["guards"].update({g: "warn" for g in flags})
    x.update(extra)
    return {"go": x}


def self_test():
    """Each shape decided on a small type, and each case where refusing would be wrong left alone."""
    E = {"Grade": ["A", "B", "C"]}
    num = {"amount": {"type": "decimal(10,2)"}, "n": {"type": "int"}, "note": {"type": "string", "optional": True},
           "grade": {"type": "Grade"}, "x": {"type": "int", "optional": True}, "eta": {"type": "timestamp"},
           "flag": {"type": "bool", "optional": True}, "a": {"type": "bool"}, "b": {"type": "bool"},
           "items": {"reference": "Item[]"}, "reason": {"type": "string", "optional": True}}
    closed = {"OPEN": {"category": "live"}, "CLOSED": {"category": "closed"}, "DONE": {"category": "closed", "final": True}}
    cases = [
        ("two bounds with no number between", {"c1": "amount > 100", "c2": "amount < 50"}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("two bounds with numbers between", {"c1": "amount > 100", "c2": "amount < 150"}, _taking("c1", "c2"), {}, {}, None),
        ("an int strictly between 3 and 4", {"c1": "n > 3", "c2": "n < 4"}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("a decimal strictly between 3 and 4", {"c1": "amount > 3", "c2": "amount < 4"}, _taking("c1", "c2"), {}, {}, None),
        ("a guard on a state the transition does not leave", {"c1": "state == DONE"}, _taking("c1"), {}, {}, "contradiction"),
        ("an enumeration value and its negation", {"c1": "grade == Grade.A", "c2": "grade != Grade.A"}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("an enumeration value among others", {"c1": "grade == Grade.A or grade == Grade.B", "c2": "grade != Grade.A"},
         _taking("c1", "c2"), {}, {}, None),
        ("a value both absent and equal to something", {"c1": "note is null", "c2": 'note == "x"'}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("an optional value compared, and tested absent", {"c1": "x > 3", "c2": "x is null"}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("an opaque condition and its negation", {"c1": "inputs.e.state == User.ACTIVE", "c2": "not (inputs.e.state == User.ACTIVE)"},
         _taking("c1", "c2", inputs={"e": {"reference": "User"}}), {}, {}, "contradiction"),
        ("an opaque condition alone", {"c1": "inputs.e.state == User.ACTIVE"},
         _taking("c1", inputs={"e": {"reference": "User"}}), {}, {}, None),
        ("a date both past and two days ahead", {"c1": "eta < now", "c2": "eta > now + 2 days"}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("none of a collection, and at least one of it", {"c1": "none(i in items where i.bad)", "c2": "count(i in items where i.bad) >= 1"},
         _taking("c1", "c2"), {}, {}, "contradiction"),
        ("a derived attribute replaced by its expression", {"c1": "big", "c2": "amount < 50"}, _taking("c1", "c2"), {}, {"big": "amount > 100"},
         "contradiction"),
        ("open, on a transition from a closed state", {"c1": "open"}, _taking("c1", frm="CLOSED", to="DONE"), {}, {}, "contradiction"),
        ("an implication, its premise and not its conclusion", {"c1": "a implies b", "c2": "a", "c3": "not b"}, _taking("c1", "c2", "c3"), {}, {},
         "contradiction"),
        ("not of an absent value is unknown, never true", {"c1": "not flag", "c2": "flag is null"}, _taking("c1", "c2"), {}, {}, "contradiction"),
        ("a state whose invariants cannot hold together", {}, _taking(),
         {"pos": "state != DONE or n > 0", "neg": "state != DONE or n < 0"}, {}, "contradiction"),
        ("an invariant left unknown by an absent value holds", {}, _taking(),
         {"pos": "state != DONE or x > 0", "neg": "state != DONE or x < 0"}, {}, None),
        ("a write that breaks the state entered", {}, _taking(effect=[{"clear": ["reason"]}]),
         {"reasoned": "state != DONE or reason is not null"}, {}, "contradiction"),
        ("a write the state entered does not read", {}, _taking(effect=[{"clear": ["note"]}]),
         {"reasoned": "state != DONE or reason is not null"}, {}, None),
        ("a flag that fails whenever the blocking guards pass", {"c1": "n > 5", "c2": "n < 3"}, _taking("c1", flags=("c2",)), {}, {}, "neverpasses"),
    ]
    ok = True
    for name, conds, transitions, invariants, derived, want in cases:
        t = _type(num, closed, conds, transitions, invariants, derived)
        found, _w, _n = analyse_type("T", t, E)
        codes = {c for _p, _s, c, _m in found}
        right = (want in codes) if want else not codes
        ok &= right
        if not right:
            print(f"  {name}: expected {want or 'nothing'}, found {sorted(codes) or 'nothing'}: {[m for *_x, m in found]}")
    print(f"logic: {len(cases)} small types, each contradiction found and each satisfiable set left alone: {'yes' if ok else 'NO'}")
    return ok


if __name__ == "__main__":
    import sys
    sys.exit(0 if self_test() else 1)
