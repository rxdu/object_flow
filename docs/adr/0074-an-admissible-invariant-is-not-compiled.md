# ADR-0074: An invariant an assertion may admit is not compiled to a database constraint

- **Status:** Accepted — derived while writing the storage schema, 2026-09-09; pending author review
- **Date:** 2026-09-09
- **Refines:** ADR-0041, ADR-0054

## Context

ADR-0041 compiles an invariant of known shape into a database constraint where the backend supports one — uniqueness, uniqueness per external source, interval exclusion per key on PostgreSQL. It is an optimisation and a second line of defence; correctness comes from serialisable isolation and does not vary by backend.

ADR-0054 gives an admission a narrow scope on purpose: it names **one object and one invariant**, and suppresses that invariant's check for that object only. Any other invariant, and the same invariant on any other object, is enforced normally. It is discharged automatically the first time the invariant holds again, and the discharge is recorded.

The two cannot both apply to one invariant. A database constraint has no per-row exemption. `ALTER TABLE … DROP CONSTRAINT` for the duration of one transaction is not one either: it is global, it is DDL inside a data transaction, and it would suspend the rule for every other object at the same time.

So an invariant that is both compiled and admissible produces a database refusing what the runtime decided to allow, with the assertion's reason recorded and its effect absent. That is the worst available outcome: the escape hatch appears to work, the record says it did, and the data says otherwise.

Nothing in the design record says this. It surfaced while writing `design/storage-schema.md`, which is the first document that had to hold both decisions at once.

## Decision

**Publishing does not compile an invariant that any of the type's assertions names in `may admit`.** Such an invariant is enforced dynamically, like a traversal or type-scan invariant, and the publish report says which invariants were left uncompiled for this reason rather than because the backend could not take them.

The two reasons are reported separately because they mean different things to whoever reads the report. "Your backend cannot express this" is a deployment fact. "You made this admissible, so it costs more to enforce" is a modelling consequence the author chose and can revisit.

## Alternatives rejected

- **Compile it, and drop the constraint for the transaction that admits.** Rejected: DDL inside a data transaction, and it suspends the rule for every object rather than the one.
- **Compile it, and let the admission fail.** This is the status quo and it is the failure this ADR exists to prevent. An assertion that records success and does not take effect is worse than one that is refused.
- **Refuse `may admit` on an invariant of compilable shape.** Symmetric, and it puts the constraint the wrong way round: the author would be told they cannot admit a uniqueness violation because of a storage optimisation. Compilation is the optimisation, so compilation is what yields.
- **Compile it as `NOT VALID`, or as a trigger that consults the admission table.** A trigger reading a table is not a constraint in any useful sense — it is the dynamic check with worse ergonomics and a second place for the rule to live.

## Consequences

- `design/storage-schema.md` §8 carries the rule as a row in the compilation table.
- An author who makes an invariant admissible pays for it in enforcement cost, and the publish report tells them so at the moment they do it.
- The commonest admissible invariants are the ones a data repair needs, and the commonest compilable ones are uniqueness. A type whose uniqueness is admissible is unusual, so the cost should be rare — but that is a prediction, and the publish report is what will show whether it is true.
