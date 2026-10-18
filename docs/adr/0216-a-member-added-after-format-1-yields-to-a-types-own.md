# ADR-0216: A member added after format 1 yields to a type's own

- **Status:** **Decided** 2026-10-18 under the author's direction ("we need to address the issues properly so that they don't have to work around it"), for issue #22, which the operations platform's developer filed.
- **Date:** 2026-10-18
- **Refines:** the model's check 33 (`declaration-syntax.md` §9.3, `flow-format.md` §3 rule 2)
- **Relates to:** ADR-0180 (a flow valid under one release stays valid under the later ones of its major version); ADR-0210 decision 3 (a withdrawn member's name kept); issue #22

## Context

An object answers fourteen members by name in an expression. Six are read by their bare names as `state` is, `id`, `open`, `created_at`, `created_by_kind`, `recorded_from` and `declaration_version`, and check 33 refuses an attribute named like one, which would shadow it. Seven more were added over time and are read by a path alone, `this.attempts`, `x.type`: `type`, `recorded_by_kind`, `actor_id`, `intervals`, `transitions`, `attempts` and `labels`. Check 33's list was never extended to them, in either checker, while both evaluators answered them before a type's attributes. So a type declaring a counter `attempts` passed the checker and had its counter shadowed in every expression: a guard `attempts == 0` compared the object's attempt log with 0, false on a counter holding 0, and nothing said so until an example reached `attempts + 1`, which was not run (issue #22). The checkers typed `attempts` as the type's int attribute, so the two halves of the engine disagreed on what the name means.

Three lists had to agree and did not: the evaluator's `member`, its `EVERY_OBJECT` and check 33's `OBJECT_MEMBERS`, each in both the core and the reference runner.

## Decision

1. **A member read by a path alone, added after format 1, yields to a type's own member of its name**: where a type declares an attribute, a derived attribute or an observation named `type`, `recorded_by_kind`, `actor_id`, `intervals`, `transitions`, `attempts` or `labels`, an expression reading that name on its objects reads the type's member, as the checkers already typed it, and the object's own is not read on that type. A member the engine adds later joins this list, so adding one never changes what an existing flow's expression reads.
2. **Step 3 notes it**: a notice, `hides_member`, names the member and what it hides, in both checkers.
3. **The six read by bare name stay refused** by check 33, as before.
4. **One list in each engine**: the core's `eval.rs` holds `OBJECT_MEMBERS` and `YIELDING_MEMBERS`, which check 33 reads and `member` answers by, so a built-in answered without being listed is never reached; the reference runner holds the same two, which its checker reads. A core test asks every listed member of a plain object and requires each answered as the object's own, and an unlisted name to fall to the type's members.

## Alternatives rejected

- **Refusing the seven names, as the issue proposed**: `issues.yaml` declares `WorkItem.labels`, a shipped sample and a flow of format 1's frozen corpus, which `check-compat` holds valid, so the refusal breaks a flow format 1 accepted, which ADR-0180 forbids from 1.0.0 and which a 0.x release should not do for a name whose meaning a rule can fix; and every member added later would break flows again. The issue's own counter would have to be renamed where the yield makes it right as written.
- **Namespacing the built-ins** (`@attempts`): a change to every flow reading `state` or `open`.
- **A unit test walking `member`'s arms against the list**: catches drift after the fact, where answering by the lists makes it impossible.

## Consequences

- `eval.rs`: `OBJECT_MEMBERS`, `YIELDING_MEMBERS`, `declares_member`; `member` answers a built-in only where the lists name it, `EVERY_OBJECT` gone. `check/names.rs` reads the evaluator's `OBJECT_MEMBERS`; `check/notices.rs` notes `hides_member`.
- `flowrun.py` holds the two lists and yields as the core does; `check-flows.py` reads them and notes `hides_member`.
- `flow-format.md` §3 rule 2, §5 and §7, and `declaration-syntax.md` §9.3, say so; `issues.yaml`'s `WorkItem.labels` is noted.
- Tests: `an_attribute_named_like_a_later_member_is_the_types` (the issue's counter, by bare name and by path, and the attempt log on a type without one) and `every_listed_member_is_answered_and_no_other`.
