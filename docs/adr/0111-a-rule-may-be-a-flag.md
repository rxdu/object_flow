# ADR-0111: A rule may be a flag, seen on every request and never refusing

- **Status:** Accepted by the author, 2026-09-25 ("yes to all four, go ahead and write them"), against `docs/PRD.md` revision 9: F9, T1 and UC-23.
- **Date:** 2026-09-25
- **Refines:** ADR-0083 (the attempt log), ADR-0085 (a clause on trial)
- **Relates to:** ADR-0110 (the service every caller uses), `design/first-consumer-prd-check.md` §3.1

## Context

The first consumer, treated as a case study rather than a specification (PRD §5), is "permissive by default": it flags rather than blocks, and blocks only what is irreversible and lands outside the team. The engine had one way for a rule to refuse nothing: a clause marked `observe`, which is a trial. The rule set prints it as not yet enforced, enforcing it is a publish that removes the marking, and the caller is never told it would have refused, since `Satisfied` carries nothing about it. The author accepted a general mechanism: a rule any deployment can declare as a permanent flag (PRD F9).

## Decision

1. **A `require` clause may be marked `flag`.** It is evaluated like any other clause and never refuses. Where it fails, the request proceeds.
2. **The caller is told.** `Satisfied.flags` lists each flag the request raised, with its clause and the remedy that would clear it. `availability` lists, for each available transition, the flags it would raise, so a screen or an agent sees them before asking.
3. **It is recorded and counted.** A raised flag is written to the attempt log, marked `flagged` and linked to the event that applied, and the event names it. It is counted per clause, remedy, kind of actor and version like any attempt, and the permanent daily rollup keeps the counts after the rows are pruned (PRD T3).
4. **It guarantees nothing.** The rule set prints it as a flag, the publish report lists it, and the adversarial harness treats it as absent. T1 says so.
5. **A flag is not a trial.** `observe` is a rule on its way to enforcement; `flag` is a rule a deployment wants seen and never enforced. A clause carries one marking or neither. Neither may mark a clause of an `assert` or an `erase`, which are enforced whole or not at all (check 57).
6. **A flag raised inside a cascade** is raised as at the top of the request (DESIGN §6, step 6).

## Alternatives rejected

- **Reuse `observe`.** It would print a deliberate policy as a rule heading for enforcement, and the caller would still not be told.
- **Flags only as business-exception conditions (M7).** A condition is a query something asks later. A flag has to reach the person or agent making the request at the moment they make it.
- **Warnings kept outside the rules**, by the application that makes the request. That is a second copy of the rule, the duplication the project exists to end.

## Consequences

- `DESIGN.md` §5.5 defines the flag, §6 raises it and §13 claims no guarantee for it; PRD T1 names it.
- `declaration-syntax.md` §5.1 adds the marking; check 57 covers it; the attempts source in §6.9 gains `.flagged`.
- `library-api.md` adds `Flag`, `Satisfied.flags`, `TransitionOffer.flags`, `Event.flags` and `Attempt.flagged`. `storage-schema.md`'s `of_attempt` and its rollup gain `flagged`.
- `renderers.md` prints a flag as one; `adversarial-harness.md` excludes flags from the guarantee.
