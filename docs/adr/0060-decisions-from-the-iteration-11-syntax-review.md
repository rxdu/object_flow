# ADR-0060: Twelve decisions from the iteration-11 syntax review

- **Status:** Accepted — repair of D63–D96, 2026-09-08; pending author review
- **Refines:** ADR-0045, ADR-0047, ADR-0051, ADR-0052, ADR-0053, ADR-0055, ADR-0056, ADR-0058, ADR-0059
- **Date:** 2026-09-08
- **Refined by:** ADR-0061 — ten decisions from the payments-ledger review; ADR-0062 — a normative statement must cite the check that enforces it; ADR-0064 — a binder's own creations replace the machine's.

## Context

Two independent reviews of iteration 10 of the declaration syntax both returned a blocking verdict. One wrote a clinical-trial model in the language as a first-time user; the other audited every check in §10 for decidability against the grammar. They agreed on the failure mode, and it is the one `docs/LESSONS.md` already records from the `stored` gap of the same day: **a term that exists only in the check list, reading correctly on each side**. Eleven such terms were found, one of them (`analysable`, in check 36) undefined anywhere in the repository including in the ADR that introduced it.

The earlier fix was scoped to the single term that had been found. This is what that costs.

Most of the thirty-four defects are documentary. Twelve are decisions, recorded here.

## Decisions

### 1. A clause continues when the next line is indented more deeply

Iteration 10 said a line continues while it **ends** in an operator or an open bracket. Seven of the document's own lines broke that rule and every comma-separated list broke it, because the operator that joins two lines usually opens the second. Indentation is what every example was already doing, it is one rule instead of a list, and it is what a reader sees.

### 2. `accepts` and `only via` belong to the transition head

The document placed `accepts` in the body in its stated clause order and in the head in all seven examples. The head is right: `accepts` and `only via` are part of a transition's signature, which is what a caller and an agent tool schema read, while inputs, guards and outcomes are its implementation.

### 3. A `default` applies at creation only

An accepted attribute not supplied to a `do` or an `act` is not written, and the stored value stands. The prior rule wrote the default on every transition, so a partial edit that omitted a field reset it to the value chosen for new objects — the opposite of what omitting a field means in an edit.

### 4. A guard that is unknown fails; an invariant that is unknown holds

Three-valued evaluation was stated in DESIGN.md §5.7 and absent from the syntax document, which claims to be self-contained. A first-time user wrote a derivation over an attribute absent early in the lifecycle and produced a guard nobody could ever satisfy. Guards failing and invariants holding is the asymmetry that makes partially filled objects workable, and it is what a database `CHECK` constraint does.

### 5. Erasure admits the invariants it breaks

Writing absence can violate an invariant reading what was erased, and refusing an erasure is not available. So an `erase` carries an automatic admission for every invariant reading an attribute it erased, recorded on the event as a declared admission is. Without it, the only safe model was one where nothing personal could be asserted at all, which made check 9's ban on required personal attributes into a ban on constraining them at all.

### 6. `may admit` may name an invariant on a reachable type

Written `<Type>.<invariant>`, restricted to types reachable by a declared inverse. The invariant an assertion breaches is usually not its own: putting a subject back into an enrolled state breaks the site's cap, and the cap belongs on the site. Restricting admission to the asserting type's own invariants left those assertions with no path, which is how a declared override becomes an out-of-band database edit.

### 7. Cascade coverage gains a transition set, a per-transition `survives`, and a subtype form

`cascade on { a, b } to T.x limit n` replaces one identical line per trigger; the document's own Delivery had four per part and a reviewer's model reached sixteen. `survives on { … }` says a part outlives some terminal transitions and not others, which is the real shape: an adverse event follows a withdrawn trial subject and is closed when the record is archived. A part declared in an `abstract` base carries no cascade clauses, since the base has no transitions; each concrete subtype supplies its own with a top-level `cascade <part> on …`. Without the third, the abstract base ADR-0056 recommends for a part serving two wholes could not be written at all.

### 8. A counter is an attribute

Whether a `counter` was an "attribute" silently changed six checks, and one reading rejected the document's own stock example. It is an attribute of type `int`, never absent, never negative, initialised to zero, and it takes markings like any other. *(Reversed in part by ADR-0072 §5: non-negativity is a fact about stock rather than about counters, so it is an invariant the type declares.)*

### 9. `.state` and stored relationship ends are always available to a filter

Every object has `.state`; the store indexes it and the stored end of every relationship because that is how objects are found. `indexed` is a marking for attributes, counters and derivations. Without this a type-scan invariant over a composed type read its `owner`, failed check 7, and had no syntax available to fix it.

### 10. Enum members, states and categories have literal syntax

`<Enum>.<MEMBER>` and `<Type>.<STATE>` are qualified; a category is bare, being one global vocabulary; a bare state of the type's own machine resolves by a stated order. Reading an enum-typed attribute is the second-commonest guard shape after a capability test and could not be written at all.

### 11. Remedy-class inference is report-only and never contradicts a declared class

The inference now has a stated priority order, which is what makes it deterministic, since most guards match several rules. It never overrules a declared class: ADR-0047's rules applied literally rejected this document's own stock guard, and a checker second-guessing an author's statement about their own domain has no standing to do so. Check 20 verifies only that a declared class is one of the five.

### 12. Publishing takes a module and its `use` closure

Capabilities and categories are one vocabulary across that closure, since a capability meaning different things in two modules would make every shared machine unsafe. The rename half of check 23 becomes a report rather than a failure, because a rename with no mapping is textually identical to a drop plus an add and no information in either declaration distinguishes them.

## Alternatives rejected

- **Keep the operator-ends-the-line continuation rule and fix the examples.** Rejected: thirty-odd sites, and the resulting text puts `and` at the end of a line where a reader expects it at the start of the next.
- **Let the checker overrule a declared remedy class.** Rejected as above; it rejected the document's own guard, and the class is a claim about the author's domain, not about the text.
- **Forbid an invariant from reading a `personal` attribute.** This was the simpler way to make erasure safe and it removes the ability to require a consent signature, which is the case that motivated personal attributes. The admission path costs one recorded event and keeps the constraint.
- **Infer the cascade for every terminal transition.** Rejected again, for ADR-0058's reason: which part transition to drive is a modelling choice. The transition set is explicit about the target and still fails publishing when a new terminal transition appears.
- **Mandate a casing convention to separate state literals from attribute names.** Rejected in favour of rejecting the collision outright (check 33), which needs no style rule and gives a better error.

## Consequences

- `docs/design/declaration-syntax.md` is at iteration 11; §8, §9 and §10 were rewritten rather than patched, and §3.4 is new.
- Nine checks were added, 42 to 50, closing sixteen of the seventeen rules that were stated and unchecked. The seventeenth, type-body clause order, is demoted to a convention.
- Checks 42, 43 and 47 are implemented and confirmed by injecting each into the document's own examples. The checker enforces 22 of 50.
- DESIGN.md §5.6 and §5.7 gain the invariant-unknown and erasure-admission rules so the two documents agree.
- The syntax document no longer depends on DESIGN.md for the invariant forms, the remedy classes or the expression semantics. It was presenting itself as the artefact a declaration author reads while keeping seven of its load-bearing terms elsewhere.
