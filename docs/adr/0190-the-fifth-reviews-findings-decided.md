# ADR-0190: The fifth review's findings, decided

- **Status:** **Decided** 2026-10-12 on the author's standing instruction, "continue the loop to get the stack ready for production, follow your recommendations", each decision the recommendation stated here; built in the order below.
- **Date:** 2026-10-12
- **Refines:** ADR-0185 (`value_in_range`), ADR-0189 decision 2 (the read bound), ADR-0176 and ADR-0181 (`taken_by`), ADR-0182 (actors)
- **Relates to:** `design/production-review.md` §9; `guides/wire.md`; `guides/integrate.md` §6, §9; `flow-format.md` §4.8, §7

## Context

The fifth production review (`production-review.md` §9) found one blocker and seven majors, each with a fix the reviewers named or the authors could; none needed a choice the PRD leaves to the author but the ones decided here on the standing instruction.

## Decision

1. **A time stays in the calendar.** An effect writing a date or a timestamp outside 0001-01-01 to 9999-12-31T23:59:59Z refuses its request `value_in_range`, as an arithmetic past its bound does (ADR-0185), and a date moved past that range has no value, as Python's dates have none, in both backends (BRICK).
2. **Reads are admitted in three pools.** Worklists, `query`, `available` and `pull`, take up to `--max-reads` at once; analytics, `metric`, `exceptions`, `diagnostics` and `export`, take up to `--max-reads` beside them; and a change's dry run, a `DeclarationChange`'s `submit` or `refresh`, a `publish` preview and a `check` of a change's submit, runs one at a time, each waiting within its read deadline. A transition named `submit` on any other type takes no slot (N-1, DRYRUN-MEMORY). Rejected: one pool, which let six-second metrics hold every worklist; a fair queue per caller, which the engine cannot key without authenticating callers.
3. **A query filter's cost is bounded where it is written**: refused `malformed_filter` where its aggregates nest more than two deep or a set it writes holds more than 1,000 members; the read deadline bounds the rest (FILTER-COST). Rejected: a cost model, which would be a guess.
4. **An actor's identity names one actor.** An identity is unique across every type declaring an actor kind, a creation or a write giving one another actor holds refused `invariant_violated`, clause `identity_taken`, named apart from an attribute's `<name>_unique`; and an identity holding a control, zero-width or bidirectional formatting character is `malformed` (IDENTITY). Rejected for now: Unicode normalisation, which needs a dependency the author has not approved; the formatting characters it would hide are refused instead.
5. **A correction may take `taken_by`**: the attribute `taken_by` generates is not among what a correction must write exactly, as it is not for an assertion or an erasure (ADR-0181 decision 1), so a type's `taken_by: [human]` holds on a type with a correction (NEW-CORRECTION).
6. **`events` takes each position once**: a position named twice is `400`, since its answer is a list aligned with the positions asked, null where the log has none, which answering each once would misalign (EVENTS).
7. **A reach a limit cannot always cover is noticed**: a cascade or a `foreach` declaring `limit: n` over a set no invariant of its type bounds at or below `n` is the checker's notice `unbounded_reach`, naming the transition refused `over_limit` once the set grows past it, an erasure among them; the samples declare the bound (ERASE-LIMIT). Rejected: lifting the limit for an erasure, which would let one request walk without bound.
8. **The wire takes a key once**: a body naming a key twice is `400`, where the last one won, an `actor` among them.
9. **A flow's source holds no bidirectional control or zero-width character**, refused by both checkers as `yaml`, where a source could read otherwise than it shows.
10. **A version an agent may approve is noticed**: where the version declares an agent kind of actor and no module declares `publishing.taken_by`, the checker's notice `publishing_open`.
11. **A log write that fails drops the line**, never stopping the request that wrote it; the minors that cost no decision are fixed with these, the rest recorded in `TODO.md`.

## Consequences

- `/v1` refuses what it took: a duplicate key, an identity another actor holds or carrying formatting characters, a filter past its bounds, a time past the calendar. Before 1.0.0 this costs no promise (ADR-0180).
- `integrate.md` §9 sizes a unit by two pools of `--max-reads` reads and one dry run, and says which reads wait.
- Held by tests named in `TODO.md`, each mutation-checked, the blocker's through both backends.
