# ADR-0182: Before 1.0.0, the author's decisions

- **Status:** Decided by the author, 2026-10-11 ("go with your recommendations on the fifteen questions"), on the recommendation `design/before-1.0.md` §4 states with each question.
- **Date:** 2026-10-11
- **Refined by:** ADR-0187 — `taken_by` for the approval of a version, declared by a module as `publishing: { taken_by: [...] }` and read from the installed version.
- **Accepts:** ADR-0174, ADR-0175, ADR-0177, ADR-0178, ADR-0179, ADR-0180, ADR-0181
- **Refines:** ADR-0176 decision 1 (`taken_by`), ADR-0114 and `DESIGN.md` §5.8 (an actor's state), ADR-0048 (what a filter reads), ADR-0180 decision 6 (what a release maintains)
- **Relates to:** `PRD.md` revision 24 (UC-22); `design/before-1.0.md`; `design/hiring-third-review.md`; `design/flow-format.md`; `guides/wire.md`; `guides/integrate.md`

## Context

`design/before-1.0.md` gathered what the author had to review before the engine's release is numbered 1.0.0, from which ADR-0180's promise binds: seven ADRs built or proposed at the author's direction, the flow format as format 1 would freeze it, and six questions the third cold review raised. Four of those six would refuse, or answer differently, what a release accepts today, which the promise forbids within a major version, so each is decided now.

## Decision

1. **ADR-0180 is accepted**, and the release that carries these decisions is numbered **1.0.0**, from which the promise binds; its upgrade from 0.2.0 is tested on stores the 0.2.0 binary made (ADR-0180 decision 7).
2. **ADR-0174, ADR-0175, ADR-0177, ADR-0178, ADR-0179 and ADR-0181 are accepted as built**, a page of 100 by default and writes one at a time among them.
3. **`flow-format.md` §9's questions 2 and 3 stay open past 1.0.0**: either answer would accept or add, which the promise allows.
4. **Draft 2 is format 1**: `flow-format.md`'s status, the schemas' titles and their `$id`s say so; the `$id` stays relative until the schema is published at an address.
5. **A type may declare `taken_by`**, which every transition of the type that declares none takes, an erasure and an assertion included, so an author marks a type `[human]` once and opens what agents may take with `[human, agent]`. A subtype does not inherit it, as it inherits no transition, and a machine's transitions bound by the type take it as the type's own do.
6. **A request whose actor or principal is in a final state is refused**, naming the generated clause **`actor_active`**, remedy `dependent`, after `actor_known` and before every other verdict; an offer to such an actor is unavailable with the same verdict, a publish's approval and an import by it are refused too, and an example's steps meet it as a request does. `DESIGN.md` §5.8's sentence that the store never reads an actor's state is reversed: revoking a session stays the gateway's, and a key revoked in the store now stops there too. This contradicted the PRD, UC-22's acceptance and revision 10's "a departed one is recorded", which the recommendation had not checked; put to the author, they kept the decision and revised the PRD (revision 24), and `metric-scenarios.md` §40 and §41 show the departed user's request refused and recorded as theirs.
7. **A query's filter may read any member of its type**, answered over the type's objects, an index serving it where a backend has one; `DESIGN.md` §5.2 and §13 say so, and a filter is refused only as malformed (ADR-0181 decision 3).
8. **What a release maintains**: a major version is maintained for 12 months after the next major's first release, with security fixes in a release of its latest minor version, and `/v1` is served throughout that time beside any `/v2`. These are the author's numbers, a policy and not a measurement.
9. **Every built-in attribute holding JSON is answered as JSON**: `DeclarationChange.source`, `report` and `evidence`, and `Proposal.inputs`, on `get` and a query's fields, on both backends, as their inputs and `publish`'s report already are.
10. **No engine-owned timers before 1.0.0**: a deadline is an attribute the flow writes and the application's scheduler queries (`integrate.md` §8); an engine that starts transitions is a different design, reconsidered if a pilot's scheduler queries prove costly.

## Alternatives rejected

- **A module-wide `taken_by` default**: one module holds the types agents work on beside those they must not, so a default per type says what the author means.
- **Reusing `actor_known` for an actor in a final state**: the remedy is the same, but the caller's next move is not: an unknown actor is onboarded, a departed one is not, so the clause says which.
- **A `json` built-in type for the four attributes**: a type an author could declare, for a need only the built-ins have; the service and the sandbox answer the four by name.
- **Refusing a filter on a member no index serves**: every application that read one would break at the first release that refused it, and the core answers any filter over the type's objects already.

## Consequences

- ADR-0174, ADR-0175 and ADR-0177 to ADR-0181 are accepted; `DESIGN.md` says every ADR is accepted or superseded.
- `flow-format.md` §3 reserves `actor_active`, §4.2 and the card give a type's `taken_by`, and the schema takes it.
- The workspace is release 1.0.0, with a release entry upgrading from 0.2.0 and fixtures the 0.2.0 binary made.
