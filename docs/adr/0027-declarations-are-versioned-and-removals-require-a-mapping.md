# ADR-0027: Declarations are versioned; removals require a mapping applied as recorded migrations; additions apply forward

- **Status:** Accepted — taken in autonomous design iteration 2 (2026-09-07); pending author review
- **Date:** 2026-09-07
- **Refined by:** ADR-0075 — cutting a mirrored type over is an ordinary version advance; ADR-0077 — the number an object and an event record is the declaration version of the publish in force, which fixes every type's version. ADR-0085 — a flow changes through a governed `DeclarationChange`, and the publish checks stay. ADR-0054 — a violation admitted at publish uses the built-in assertion, and an admission is tolerated until the invariant holds again. ADR-0099 — a publish changes a live row only by a recorded migration: a new required attribute needs a `backfill` mapping, a removed enum member a `removed member` mapping, and a removed type is retired.
- **Amended by:** ADR-0056 §4 — an object records its type's version and dependencies propagate.

## Context

TODO.md asked what happens to objects mid-lifecycle when a type gains a guard or an attribute, and ADR-0015 noted that import is the same situation for every object at once. Ticket systems edit workflows under thousands of live issues, and their behaviour is a working answer: removing a status forces the administrator to map the issues in it; adding anything applies forward. Case study §3.

## Decision

1. **Every type declaration has a version**, a monotonic integer. Publishing a version is a recorded event. Older versions are retained for reading history and printing the rule set that applied.
2. **Every object records the declaration version in force at its last recorded change**, on the object and on each event.
3. **Additions apply forward.** A new state, transition, attribute, relationship, derived attribute or guard changes nothing on existing objects. *(Refined by ADR-0099 §7, repairing D250: a new **required** attribute on a type with live objects needs a `backfill` mapping, which publishing applies to each live object as a recorded migration transition with provenance `migrated`, and without which the publish is refused (check 23); a declared `default` applies at creation only, so no live row changes without an event.)* A new guard bites at the next transition (ADR-0002). A new invariant is evaluated against live objects at publish time and every violation is reported, exactly as the importer reports (ADR-0015); each violation is resolved by a mapping or admitted, by a person. An admission names the object and the invariant, suppresses that invariant's check for that object, and is discharged automatically when the invariant holds again (ADR-0054).
4. **Removing a state requires a mapping.** Publishing must supply, for each removed state, the target state or transition for objects in it. The mapping is applied as a **migration transition** per object: recorded, with provenance `migrated`, the publish event as cause, and the publishing actor. Removing a transition affects no object. Removing an attribute hides it; recorded values stay in history. A rename is a removal plus an addition with a mapping.
5. **Import is version zero.** ADR-0015's import is this mechanism with every object arriving from an unversioned source under a mapping.

## Alternatives rejected

### Eager migration to satisfy new guards

Rejected: guards attach to transitions (ADR-0002); an object that would fail a new guard is simply blocked at its next transition, with the reason, which is what a person needs to see.

### Forward-only declarations

Never remove anything. Rejected: types must be able to shrink, and a machine that keeps every state it ever had is the union machine ADR-0026 rejected.

### Objects keep the machine they were created under

Rejected: two machines for one type, and every consumer must handle both. Retained only for *reading* history.

## Consequences

- The schema-evolution question in TODO.md is closed.
- Publish is an operation with a validation report and a possibly large recorded migration; it is a designed path, not a config reload.
- The printable rule set is per version.
