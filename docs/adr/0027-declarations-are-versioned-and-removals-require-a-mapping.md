# ADR-0027: Declarations are versioned; removals require a mapping applied as recorded migrations; additions apply forward

- **Status:** Accepted — taken in autonomous design iteration 2 (2026-09-07); pending author review
- **Date:** 2026-09-07

## Context

TODO.md asked what happens to objects mid-lifecycle when a type gains a guard or an attribute, and ADR-0015 noted that import is the same situation for every object at once. Ticket systems edit workflows under thousands of live issues, and their behaviour is a working answer: removing a status forces the administrator to map the issues in it; adding anything applies forward. Case study §3.

## Decision

1. **Every type declaration has a version**, a monotonic integer. Publishing a version is a recorded event. Older versions are retained for reading history and printing the rule set that applied.
2. **Every object records the declaration version in force at its last recorded change**, on the object and on each event.
3. **Additions apply forward.** A new state, transition, attribute, relationship, derived attribute or guard changes nothing on existing objects. A new guard bites at the next transition (ADR-0002). A new invariant is evaluated against live objects at publish time and every violation is reported, exactly as the importer reports (ADR-0015); each violation is resolved by a mapping or admitted, by a person. An admission names the object and the invariant, suppresses that invariant's check for that object, and is discharged automatically when the invariant holds again (ADR-0054).
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
