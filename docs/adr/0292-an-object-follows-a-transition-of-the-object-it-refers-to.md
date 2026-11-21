# ADR-0292: An object follows a transition of the object it refers to

- **Status:** **Decided** by the author 2026-11-21 ("supervise a session to address the issue with your recommendations, then release together with v0.32.0"), as recommended the same day for issue #103 at the author's word ("check more open issues"): decision 1, its direction, and decisions 2 to 4, where it departs from the issue's text. The trace put with the direction was not answered, and is skipped. Decisions 5 to 10 settle what the direction leaves open, accepted in advance under the author's direction of 2026-09-28 as confident recommendations. To be built for 0.32.0, with ADR-0289 to ADR-0291. Built the same day, `dd1d3f7b` to `b53901c4`.
- **Date:** 2026-11-21
- **Refined by:** ADR-0293 — three points its text left open: a clause's refused arithmetic names the follower, a missing `limit` is refused at step 2, and a machine's required attribute declares no follow.
- **Supersedes in part:** ADR-0024 decision 3: a reference is still never cascaded by the type it refers to, and its own type may now declare that it follows.
- **Relates to:** ADR-0019 (a transition's outcome cascades, atomically and acyclically); ADR-0038 (cascades apply in turn); ADR-0108 (a cascade is shown where it lands); ADR-0122 (`over-limit`); ADR-0163 (an override is never caused); ADR-0170 (a cascaded part is of a concrete type); ADR-0217 (`only_via: [any]`); `docs/research/following-a-referenced-objects-transitions.md`; issue #103

## Context

Issue #103, filed 2026-11-21 at 0.31.0, asked that an object follow a transition of the object it refers to. Its case is two modules. One declares a slot, and the other, importing it, a hold referring to a slot. The first may not import the second. When a slot is cancelled or moved, its holds should end or move in the same request.

Nothing can declare it today:

- A `cascade` stands only on a composite end, which the whole declares (`flow-format.md` §4.3, *Composition*).
- Both ends of a relationship are in one module (§4.3, *Relationships*), so the slot cannot declare an end to its holds.
- A `call` names its target's type, which the slot's module cannot import.
- A guard reading `referrers` can refuse the cancel, but cannot end the hold.
- ADR-0019 rejected reacting to events after the fact, since it cannot make the change and its consequences one atomic fact (PRD F2).

Reproduced on the 0.32.0 release build, `8a8f32dd`, on two modules of the repository's own (`TODO.md`, #103). The slot's `cancel` is satisfied and the hold stays held.

ADR-0024 decision 3 holds that references are never cascaded by deletion. Its rejected alternative was a registry beside the state machine that drove deletions through references unguarded, so that one row's deletion reached 265 records.

Four of five established systems read declare the rule on the referencing side, and the referenced side declares nothing of it: PostgreSQL's `ON DELETE` on the referencing table, Django's `on_delete` on the `ForeignKey`, Odoo's `ondelete` on the `Many2one`, and Salesforce's deletion option on the child's relationship field. Rails declares `dependent` on the parent, as ObjectFlow declares a cascade on the whole. Their actions are fixed, never a transition of the referring record's own. PostgreSQL's constraints and Rails' callbacks can refuse the change, as a cascaded part's guards can (ADR-0019) ([research note](../research/following-a-referenced-objects-transitions.md)).

## Decision

1. **A reference may declare `follows`**, a list of clauses `{ on: [<transition of the type it refers to>, …], transition: <transition of its own type>, inputs: { … }, limit: <n> }`. When an object takes a transition in `on`, each object holding the reference to it takes `transition`, in the same request. The module declaring the referred-to type neither imports nor names the follower.
2. **A follower is driven as a cascade's part is.** One in a final state is passed over and not counted. Every other takes `transition`. Its refusal refuses the whole request, a state `transition` is not taken from included, with the follower's own verdict naming it. `limit` is required, and more followers than it are refused `over_limit`, never truncated.
3. **The key is `follows`, not `cascade`.** A `cascade` drives the objects its end refers to, and a `follows` drives the object that declares it. One key with two directions would give one term two meanings.
4. **The first build is narrowed.** A follow stands on a single reference, never a set. It stands on no part's end back to its whole, which a cascade covers. No mirror declares one. Neither `on` nor `transition` names an erasure.
5. **Only a reference with no `opposite` follows, of a concrete type, to a concrete type.** An end with an `opposite` has a way back, by which the referred-to type's transitions reach it with a `call`. A follow there would be a second way to declare one rule. `on` names transitions of the referred-to type, never an initial one, and may name an assertion, as a cascade's `on` may. `transition` names a transition of the follower's own type. It is never initial or an assertion (ADR-0163), and receives every input it requires and none it does not take.
6. **A clause's `inputs` are expressions over the follower**, as every expression its type declares is. The referred-to object is read through the reference. `inputs.<name>` reads the triggering transition's inputs, and MUST be an input of every transition in `on`. Each is evaluated as its follower is driven, seeing every write the request made before (ADR-0038).
7. **Followers run after the transition's effect and its cascades, depth-first**, as cascades do (`DESIGN.md` §6). Clauses run in the order the flow declares them: modules in the order they are checked, then types, attributes and clauses in declaration order. One clause's followers run in creation order. Each follower's event carries the referred-to transition's event as its cause. A follower's transition may be `only_via` the referred-to transition, as a part's may be `only_via` its whole's.
8. **The graph of transitions stays acyclic across modules** (ADR-0019). Calls, creations, cascades and follows are all its edges. The module declaring a follow is checked with the edges of the types it imports, since only it knows both ends.
9. **A follow is shown where it lands and where it starts** (ADR-0108). In a flow holding both modules, the printed rules name it under the referred-to transition and under the follower's transition. Wherever a cascade clause is read, checked, printed or rendered, a follows clause is too.
10. **On the wire, a follow adds no field.** An `over_limit` names its loop as `the follow on <Type>.<reference>`, as a cascade's names `the cascade on <attribute>`. A follower's refusal is its own verdict, a guard's clause qualified by the follower's type, as a part's is (`DESIGN.md` §5.5, `call refused`).

The referred-to type's guards are unchanged. They are evaluated before its outcome, so a guard reading `referrers` counts a follower as it counts any referrer. A type whose transition its referrers follow does not guard that transition on them.

## Alternatives rejected

- **A follower in a state `transition` is not taken from passed over**, as the issue's text asks. A live follower would then be left silently referring to an object that ended. A cascade refuses instead, and a follow is driven as a part is.
- **`cascade` on the referring end.** It gives one key two directions (decision 3).
- **An end declared across modules, by the importing module on the imported type.** A type would no longer read completely on its own, and its end's cascade would be a composite's, which a hold referring to a slot is not.
- **A job reacting to the referred-to object's events.** It is not atomic, and the follower's guard cannot refuse the change (ADR-0019).
- **A call over `referrers` from the referred-to type, by a transition's name.** It names transitions of types the module cannot import, so step 3 could check neither the transition nor its inputs.
- **A follower's reference left out of `referrers` while the transition is taken.** A guard would then mean something different depending on modules its author does not know.
- **A clause's expressions over the referred-to object**, as a cascade's are over its whole. A clause written inside the follower's type would then read another type's attributes by bare name, against `flow-format.md` §5.
- **A set of references, and an end with an `opposite`, in the first build.** A set's follower is reached through several objects at once, and an end with an `opposite` is reached by a `call` already (decision 5). Either may be added once a case needs it.

## Consequences

- **The docs:** `flow-format.md` §4.3 (the key and its rules), §4.8 (an object changed by a follow) and §7 (step 3's and step 4's refusals); `flow.schema.json`; `declaration-syntax.md` with `check-syntax-doc.py`; `DESIGN.md` §5.4 (*Shown where it lands*), §6 (the order) and §8 (deletion); `renderers.md` §2; `errors.md` (`over_limit`'s loop); the flow-format card.
- **The code:** both checkers, the core's request, `flowrun.py`, the renderers and the Studio's sandbox. They answer alike, held by the backends-agree tests and the core's on both backends.
- **A follower's guard can refuse a transition of a module that does not know it.** The printed rules show it there (decision 9), and a request refused by it names the follower.
