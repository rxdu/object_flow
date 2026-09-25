# ADR-0114: ObjectFlow records who acted and never evaluates it

- **Status:** Accepted by the author, 2026-09-25, against `docs/PRD.md` revision 10: F1, F7, D6, D12, C2, M2, T1, T5, T6, T7, N7 and UC-10, UC-11, UC-15, UC-19, UC-22, UC-24. The author's words: "I think objectflow only need to define states/conditions/actions/transtions/emissions, should not need to care who can and will create a trigger and who should receive a notification, this should be wired by the upper layer applications"; then "go ahead", accepting the three points put to them: no condition reads the actor, no read is filtered by who is asking, and a request naming an actor the engine does not know is still refused.
- **Date:** 2026-09-25
- **Supersedes:** ADR-0030 (read visibility), ADR-0112 (who may read a metric)
- **Refines:** ADR-0025 and ADR-0079 (the actor descriptor), ADR-0036 (proposals), ADR-0082 (`recorded by`, `labels by`), ADR-0095, ADR-0096 and ADR-0100 (values withheld from a reader), ADR-0097 and ADR-0101 (the built-in capabilities and the approval of a flow change), ADR-0103 (`requests by`), ADR-0105 (a subscription's reader), ADR-0110 §2 and §3 (capabilities from the upper layer, the refusal of an actor who is not live)
- **Relates to:** ADR-0012 and ADR-0104 (the engine initiates nothing), ADR-0010 (the declaration is data), ADR-0023 (the expected version)

## Context

The author first asked, on 2026-09-25, to "leave the auth & authz for upper layers to decide". ADR-0110 moved authentication and the mapping of roles to capabilities upstream, but kept in the engine what a capability permits and who may see what. That part was the design's reasoning from F1, T1, T5, F7 and D12, not the author's words.

Writing the walkthrough of ObjectFlow in use then showed the cost. Every workflow declared its own capabilities and its own people, and an upper layer mapping its roles onto an ever-growing, workflow-named list could not keep track of it. A flow published with a new capability that no role mapped onto would refuse everyone, silently. The author rejected a role vocabulary declared once per deployment as too complicated, and then stated the boundary above.

## Decision

1. **A flow's rules never read who is asking.** `actor` may not appear in a guard, an invariant, a derivation, a filter or a metric, nor in any clause of a type or an observation kind (check 64). An outcome may write `actor.id`, `actor.kind` or `actor.principal` into an attribute or an argument, such as the approver of an approval, which records who acted and decides nothing.
2. **The engine records who acted.** A request names its actor. The engine resolves the id to an object of a type that marks an `actor` identity and refuses a request naming an actor it does not know, as `actor_known`, so every event names someone the record can resolve. It records the actor's id, the kind its type declares (ADR-0110), the principal and the caller's `context` on the event or the attempt. Whether the actor is still live is not the engine's to judge: an upper layer revokes a departed user's session, and the record shows who acted if it did not.
3. **Upper-layer applications decide who may do and see what.** The engine no longer has:
   - **capabilities:** the `capability` vocabulary, `actor.has`, `provides capability` and `requires capability`, the descriptor's `capabilities`, and the eight built-in capabilities (ADR-0079, ADR-0097, ADR-0105, ADR-0110, ADR-0112);
   - **visibility:** a type's `visible when`, and every filtering of a read by its reader: `not found` for an object the reader may not see, values withheld from a verdict, an event or an attempt, and results marked partial (ADR-0030, ADR-0095, ADR-0096, ADR-0100);
   - **metric audiences**, and the default that closes splits by person (ADR-0112);
   - **`recorded by` and `labels by`**, which said who may record an observation or label an object (ADR-0082);
   - **`requests by <kind> require …`** (ADR-0103);
   - **the rules on who drafts and approves a flow change**, that the approver is not the drafter and that a person approves an agent's draft, and **who may end a proposal** (ADR-0036, ADR-0097, ADR-0101);
   - **a subscription's reader** (ADR-0105).
4. **Reads return what they are asked for.** `get`, `query`, `history`, `pull`, `export`, `metric` and `diagnostics` apply no filter of their own for the reader. An upper layer decides who may make a read and narrows it by the filters those operations take. A metric is computed over every row its filter selects, so every reader of the same definition over the same filter gets the same value (C2). The declaration marks every metric dimension whose value names an actor, so an upper layer can put reads split by person behind its own door (UC-24).
5. **The routes around the rules stay declared and recorded.** An assertion a type declares, an erasure, the import and a publish's migration are still the only ways to change governed state without satisfying the enforced rules (F2). Each is recorded with its actor, kind, reason and cause, and counted. Who may take one is the upper layer's decision.
6. **Proposals and flow changes keep their lifecycles.** A proposal is a request held for approval: any caller may file one on a transition marked `proposable`, and approving it executes the request as the approver under the current rules. A `DeclarationChange` still carries its impact report and is refused if drafted against anything but the installed version. Both record who drafted, proposed, approved or rejected, and their kinds.
7. **What the upper layer relies on.** `availability` and `check` answer by the flow's rules alone. An upper layer that checks a permission against something it read, such as who the assignee is, sends the version it read as `expected_version`, and the engine refuses the request as `stale` if the object changed in between, so a check outside the engine cannot race a change inside it.
8. **The guarantee.** Governed state changes only through F2's four routes, each recorded with who and why, and bypasses the enforced rules only through a recorded override. Who is allowed to take a route is outside the guarantee. Under T2's threat model, mistakes and not malice, an upper layer that lets the wrong actor through makes a mistake the record shows and the counts measure.

## Alternatives rejected

- **A role vocabulary declared once per deployment, which the flows' rules read.** It kept the upper layer's mapping small and stable, but the author judged it an over-complication: it is a second authorization system beside the upper layer's own.
- **Keeping a fixed set of built-in capabilities for the routes around the rules and for per-person figures.** It left two systems deciding who may do what. Recording every override with who made it, and counting overrides, is what keeps those routes honest when mistakes, not malice, are the threat.
- **Keeping conditions about identity and kind**, such as "only the assigned engineer finishes" and "an agent never commits a financial write". Each is a rule about who may. The upper layer holds the same facts, reads the object, and sends the version it read (§7).
- **Keeping visibility in the engine because metrics add rows up.** An engine that filters by reader needs facts about the reader, which is the dependency this decision removes. A filter the upper layer passes narrows a metric exactly as it narrows a query.

## Consequences

- **PRD revision 10** rewrites F1, F7, D6, D12, C2, M2, T1, T5, T6, T7 and N7, and the acceptance of UC-10, UC-11, UC-15, UC-19, UC-22 and UC-24.
- **`DESIGN.md`:** §2, §3, §5 and §5.2, §5.5, §5.8, §5.11 to §5.13, §6, §7, §8, §9, §10, §13 and §14 are amended. §5.8 becomes "Actors, recorded".
- **`declaration-syntax.md` iteration 26:** the forms listed in §3 of this decision are withdrawn; check 63 is withdrawn, and checks 7, 14, 19, 29, 33, 35, 54 and 55 lose their clauses about capabilities, actor guards, visibility, `recorded by`, `labels by` and `requests by`; check 64 is new. The examples are rewritten without them, and so are the checked modules and case studies.
- **`library-api.md`:** `Actor` loses `capabilities`; `Unsatisfied` loses `capability` and `withheld`; `MetricPage` loses `complete` and `refused`; `Object` loses `partial`; `ReadSet` loses `capabilities` and `withheld`; `NotFound` names only an object that does not exist.
- **`storage-schema.md`:** the constraints on a change's approver, a subscription's reader and the indexes for visibility go.
- **`renderers.md`:** the rule set prints no authority; the metrics tool names every metric.
- **`adversarial-harness.md`:** the guarantee is over routes and records, and its permission routes become the upper layer's.
- **The walkthrough of ObjectFlow in use** is rewritten to match.
