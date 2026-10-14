<!-- The audit of ADR-0204 §6, area as named, at commit 698034b, 2026-10-14: the reviewer's inventory as returned, kept so each item's fix and check can be traced (ADR-0205). Its scripts and stores stayed in the session's scratch directory, which is not kept. -->

# Inventory, area 2: rules and the record on the write path (promises R and A)

Auditor a2, checkout 698034b, release binary `audit/bin/objectflow-service`. Paths below are relative to the checkout (`audit/repo`) unless they start with `a2/`.

**Reproducing.** Every "reproduced" item is a script in `a2/probes/` run against a sandbox, its output kept in `a2/out/`:

```
bin/objectflow-service --port 0 &          # prints "listening on http://127.0.0.1:<port>"
export OF=http://127.0.0.1:<port>
cd a2/probes && python3 <probe>.py         # helper: a2/of.py
```

Each probe loads its own flow, and where it matters first runs the core's checks on it through a change's publish preview (`checked()` in `a2/of.py`), printing `checks: clean []` when the core checker accepts the flow, so a finding is shown on a flow a deployment could publish.

## 1. How the area was enumerated, and why the enumeration is complete

**The write operations.** `docs/guides/wire-v1.json` lists every operation. The ones that write governed state or the record are `request` (and `check`, which runs the same decision without writing), `publish`, `import_batch`, `maintain`, `acknowledge` and `upgrade`. I took `request`, `publish` and `import_batch` whole, and every key `request` takes: `transition`, `actor`, `object_id`, `type`, `inputs`, `occurred_at`, `propose`, `expires_at`, `expected_version`, `target_version`, `principal`, `idempotency_key`, `context`. `maintain` only for its actor check. `acknowledge` and `upgrade` are area 5's.

**Every route a request reaches an object by.** `decide_as` (`core/crates/objectflow-core/src/request.rs:621-649`) dispatches every request to one of four deciders:
- `decide_change` for a DeclarationChange;
- `file` for `propose`;
- `decide_proposal` for a Proposal;
- `decide_or_stop` for everything else.

Inside `decide_or_stop`, every object is reached through `take()` (`request.rs:1028-1959`), by one of these routes:
- the requested transition;
- the effect's `Step` variants (`model.rs` `Step`: `Assign`, `Clear`, `Add`, `Remove`, `Create`, `Call`, `Foreach`, `Supersede`; `effects()`, `request.rs:3839-4090`);
- `cascades()` (`4118-4209`);
- an erasure's walks: `forget_observations` (`3527-3586`), `erase_chain` (`3593-3677`), `redact_others` (`2157-2244`) and `invalidate_proposals` (`2466-2604`).

Outside requests, objects are written only by:
- a publish's install: the publish event, then a `migrate`, `redact`, `supersede` or `invalidate` event per object (`store.rs:2521-3120`);
- the import's placed objects (`import.rs:320-1262`).

I enumerated every place an `Event` or an `Attempt` is constructed:
- `request.rs:1496, 1897, 2217, 2499, 2576, 2774, 2832, 3193, 3405, 5739`;
- `import.rs:1202`;
- `store.rs:867` (`event_of`), and the publish's uses of it at `2803`, `2990`, `3056` and `3099`.

So every write and every record is covered.

**The rules.**
- **Generated guards and refusals,** read from `decide_or_stop` and `take()`:
  - `actor_known` and `actor_active` (`active`, `430`);
  - `not_found` (`664-675`), `unknown_transition`, `not_requestable` (`706-715`), `unavailable` (`717-727`);
  - `invalid_input` (`729-773`, `1077-1137`), `<input>_provided` (`1359`);
  - `occurred_within` (`1171-1210`), `whole_open` (`1211-1240`), `not_erased` (`1241-1266`), `successor_not_superseded` (`1267-1289`);
  - `corrects_current`, `subject_open` and `subject_owned` (`1290-1358`);
  - `value_in_range` (`397-424`, `3853-3924`, `4250`) and `over_limit` (`4059`, `4175`);
  - `stale` (`466`), `not_proposable` (`2652`), `dry_run_clean` (`3158`), `publish_taken_by` (`3034`; `store.rs:2563`).
- **Declared guards:** `Enforce` is `Deny`, `Warn` or `Audit` (`model.rs:52-59`). The brief's "flag" is `warn`.
- **Invariants:** everything `failing_invariants_reading` (`request.rs:4994-5127`) checks:
  - declared invariants, a state's required attributes, `<attr>_unique` (Yes, With, Where, InScope), an external identifier's uniqueness, and `identity_taken`;
  - the invariant `taken_by` generates (`model.rs:1283-1339`);
  - checked on every object `affected_by_writes` (`4865-4990`) returns.
- **Other rules:** `only_via`, `proposable`, the call depth (`1037`), and supersession.

**Actor and identity resolution.**
- `resolve_actor` (`request.rs:208-271`), `request_who` (`447`), `is_the_operator` and `may_approve` (`store.rs:1808-1834`);
- `identity_holders` (`request.rs:4352`), `wire::identity_fault` (`wire.rs:131-162`);
- the key's actor and digest (`request_digest`, `98-132`; `request_told`, `5490-5602`).

**What I left out, and why:**
- **Reads, tools and `availability`**, except where a write tool's `request` matters: not the write path.
- **The racer:** SQLite's writers are serial (`BEGIN IMMEDIATE`), and the harness defers the racer to PostgreSQL. A race on this store is covered only by the existing deployment test `concurrent_writes_lose_no_update`.
- **What erasure must remove** (P, area 1), whether values are computed right (V, area 3), cost (area 4), `pull`/`export` delivery and upgrade (E/D, area 5), and the guides (area 6). Where my probes saw something there, it is in the closing list.
- **The reference runner's agreement** (K, area 3).

## 2. The inventory

Grades follow ADR-0204 §4. Promise bullets:
- A1: every change and every refusal is recorded with the actor the request named and the kind its type declares;
- A2: a `satisfied` verdict names an event of that request;
- A3: an identity names one actor;
- A4: a key replays only the request it was first sent with;
- R: a flow's rules hold for every request, through every route.

"Unusual" means §4 major's "unusual but legitimate flow, load or sequence". "Normal" means §4's normal operation.

### 2a. Broken

| # | Instance | Where | Held by today | Evidence | Grade | Promise, bullet | The check that would keep it held |
|---|---|---|---|---|---|---|---|
| A-01 | A subject's erasure removes its labels' notes with no event on the label. The label's version does not advance, and the erasure's own event names only the subject's attributes. This contradicts `DESIGN.md:575` ("every object whose value an erasure redacts … gets an event … its version advances"); `declaration-syntax.md:911` and `store.rs:4821-4823` say "as no transition". | `request.rs:3569-3584`; `store.rs:4821-4848` | none; the harness's "no erased event of its own" condition (`adversarial-harness.md:58`) is not in the Built slice | reproduced `p3_label_note_erasure.py` (`out/p3_label_note_erasure.txt`): label-2 is version 1 before and after, its note goes, its history shows only `record` | **blocker** | A1, normal (a label's note, `wire.md:128`, on a person's record) | Harness: after each step, every object whose attributes or past event payloads changed in that step's transaction has an event in it (diff `get` and `history` before and after). Suite: `an_erasure_records_a_redact_event_on_each_label_it_reaches`: label version +1, a `redact` event with source `erased`, caused by the erasure, payload `{"redacted": ["note"]}`. |
| R-01 | `type` beside `object_id` is ignored when the id names a Proposal or a DeclarationChange. `decide_as` routes on the target's own type, and `decide_proposal` and `decide_change` never compare `req.type_`, so a write tool bound to `Claim` decides a proposal or a change. An agent whose `claim_pay` tool is propose-only (`tools.rs:516-525`) executed `pay` itself through its `claim_approve` tool given the proposal's id. A person withdrew the operator's change through `Claim.withdraw`. This breaks `wire.md:50` and `agents.md:135`, and is the next instance of D644's class. | `request.rs:631-643`, `3290-3327`, `2976-3029` (contrast `671`, `2624`) | `an_object_named_beside_another_type_is_not_found` covers declared types only | reproduced `p1_type_binding.py` (flow checker-clean; tools listed in `out/p1_type_binding.txt`) | **major** | R (route: id and type), unusual (a tool call given another family's id) | Suite: for `approve`, `reject` and `withdraw` on a pending proposal, and for every DeclarationChange transition, a request naming a declared `type` beside the built-in object's id answers `not_found`, changes nothing and writes an attempt. Harness: the guesser sends `type` beside every `object_id`, sometimes another type's. Fail if a request naming type T is satisfied on an object outside T's family. |
| R-02 | `not_erased` reads only the effect's top-level steps. An `add` or `assign` to a personal attribute inside a `foreach` writes it again on an erased object. This contradicts `DESIGN.md:577`. | `request.rs:1253-1256` | `an_erasure_redacts_its_history_reaches_its_observations_and_stays` (top level only) | reproduced `p2_not_erased_foreach.py` (checker-clean): after the erasure `add_note` is refused `not_erased`, `add_phones` (foreach) is satisfied, and `phones` holds a new number | **major** | R (generated guard); also P; unusual flow | Harness: fail if any satisfied step leaves a non-absent, non-empty personal attribute on an object whose erasure is recorded (the design's §1 row). Suite: `not_erased` refuses an assign or add nested in a `foreach`. |
| R-03 | `whole_open` is checked only for attribute inputs of an existing object. An effect's `assign` of the owner end re-parents a live part into a whole in a final state. A `create` of a part, from another whole's transition, into a whole in a final state is not checked at all, because creations are skipped. | `request.rs:1211-1240` | `a_part_is_not_moved_into_a_finished_whole` (input route only); the harness's "part alive under a terminal whole" row is not built | reproduced `p12_reparent_effect.py` and `p12b_create_into_closed.py` (both checker-clean): item-5 and item-4 are live (`IN`) under the SHIPPED box-3 | **major** | R (composition/cascade rule), unusual flow | Harness: fail if a part is in a non-final state while its whole is final, unless the part's type survives the transition that made the whole final and the part was in it then (`adversarial-harness.md:50`). Suite: both routes refused `whole_open`. |
| R-04 | An import's refresh of a mirror breaks an owned object's invariant that reads the mirror through a reference. The import checks only the objects it places, never `affected_by_writes`. The batch is answered clean, and `exceptions` lists nothing. | `import.rs:898-962` (contrast `request.rs:833`) | `each_class_is_applied_as_the_file_decides_it` covers placed objects only | reproduced `p16_import_breaks_owned_invariant.py` (checker-clean, the `Laptop` and `Employee` pattern of `flow-format.md:498`): laptop-3 is OUT, held by employee-2 who has LEFT | **major** | R (invariants, route: imports), unusual flow | Suite: an import whose writes make another object's invariant false refuses the batch, naming the object and the invariant. Harness: an importer behaviour, with the invariant condition run after each batch. |
| R-05 | `only_via` is enforced only against a direct request. A `call` from a transition `only_via` does not name takes it. Both checkers accept such a flow: core check 13 tests only that the named parents reach the transition, which is half of `declaration-syntax.md:596`, and `scripts/check-flows.py:2048-2056` checks only names. | runtime `request.rs:1060-1075` (no `only_via` test at depth > 0); `check/model.rs:1201-1260` | `the_order_of_refusals_is_designs` (direct request only) | reproduced `p20_only_via_call.py` (core checks clean): `post` is refused `not_requestable` directly, then taken through `Account.sneak` | **major** | R (`only_via`), unusual flow | Suite: a call or cascade into an `only_via` transition from a transition it does not name is refused `not_requestable`. Both checkers refuse the flow (check 13's converse). Mutation: drop the runtime test, and the suite fails. |
| I-01 | An actor's identity attribute may be written after creation, and the record names actors by identity: the events, `proposer`, `drafted_by` and the key's actor. Once an identity changes and another actor takes it, old events name the new holder. In the probe, the new holder withdrew the old holder's proposal as "its proposer", bypassing `taken_by` (`request.rs:3342`), and had the old holder's key replay. ADR-0192 decision 4 renames records only on erasure. | `request.rs:1707-1712` (no guard on an identity write); `208-271`, `3342-3347`, `5503-5509` | none | reproduced `p4_identity_change.py` (checker-clean, `change_email` as a sync carries an email change): Jo Two withdraws Jo One's proposal of a `taken_by: [service]` transition, and replays Jo One's key | **major** | A3 (also A1, A4), unusual flow (an email identity that changes and is reassigned) | Harness: keep, for each identity ever held, the first object to hold it. Fail if another object holds it later (unless renamed by an erasure), or if an event's `actor_id` resolves to a different object than made it. Suite: a write of an `actor_kind` attribute after creation is refused, or the change renames the record as an erasure does. |
| I-02 | D669, open. A type added by a later version makes an identity shaped `<type>-<n>` an object id. `resolve_actor` tries `world.get` first, so the person `gadget-12` was resolved to the agent object `gadget-12` and recorded as agent `unit-a`. That is worse than D669's register text: an impersonation, not only a shadow. | `request.rs:223-225`; publish has no test of held identities | none (ADR-0202 decision 8 deferred) | reproduced `p5_d669.py`: `recorded as unit-a agent` | **major** | A3, A1 (actor and kind recorded wrong), unusual sequence (a publish adding a type) | Suite: a publish adding a type T while an identity matching `t-<digits>` (any case) is held is fatal, or a decision. A request whose actor names both an object id and a held identity is refused. Deployment test on `--create` store. |
| I-03 | A publish's backfill writes an actor identity without `identity_fault` or a type fit (string into identity). The probe backfilled `login` from `staff_no = "bot-2"`. Afterwards `bot-2` names two actors, the request resolves to the agent, and the person can act as no one. The checks are clean. | `publish.rs:694-721` | none | reproduced `p29_backfill_identity.py`: the request by `bot-2` is recorded as `helper agent`; ana is refused `actor_known` | **major** | A3, unusual flow (a backfill of an identity) | Suite: a backfill value that the attribute's decoding would refuse (`identity_fault`, or the wrong type) is a fatal finding naming the objects. Both checkers type a backfill's expression against its attribute. |
| K-01 | A key's digest masks an input by its name in any type (`eval::personal_names`). A non-personal input that shares a personal member's name elsewhere (`Product.name` beside `Contact.name`) therefore replays a different request's verdict. "Floor Lamp" was answered `satisfied`, `replayed`, and never ran. This is D654's class. `wire.md:63` speaks of inputs "marked personal". | `request.rs:98-112` | `an_erased_requests_key_confirms_no_guess`, harness key condition: neither varies one non-personal input | reproduced `p26_key_masks_by_name.py` (checker-clean): `reprice` with a new price gets 409, `rename` with a new name is replayed | **major** | A4, unusual sequence (one actor reusing a key) | Harness: the retrier also resends with one input's value changed. Fail unless 409 `key_reused`, the personal inputs of the transition excepted. Suite as above. |
| K-02 | The same, by design (ADR-0201 decision 6, `wire.md:63`): a request with a different value of a personal input replays the first's verdict even before any erasure. The second Contact was never created. | `request.rs:98-112` | none | reproduced `p26_key_masks_by_name.py` (last block): `Bo Chan` replayed as `Ana Lima`'s contact-3 | **major** (the rubric has no "decided" exception; ADR-0204 §8 floor) | A4, unusual sequence | Suite: before an erasure a different personal value under a key is `key_reused`; after it, every request under the key is `key_reused` (none replayed). |
| K-03 | A key whose digest was made before masking, as 0.2.0 made them, gets the digest `erased` once an erasure redacts what it sent. Thereafter any request under it from that actor replays the first verdict (`wire.md:63` documents this). It is D654 on upgraded stores, within `--idempotency-retention`. | `request.rs:5516`; `store.rs:1280`, `4950-4957` | none | unconfirmed (needs a 0.2.0-made idempotency row; making one would mean writing a store file) | **major** | A4, unusual sequence (an upgraded store, an erasure, a reused key) | Suite: an idempotency row holding `erased` answers every request under its key `key_reused`. Upgrade fixture (0.2.0 to 1.0.0) with such a row. |
| A-02 | A refused `publish` writes no attempt. Probed: an agent's approval refused `publish_taken_by`, an unknown actor refused `actor_known`, a stale version refused `stale`. The DeclarationChange's attempt export is empty. | `store.rs:2521-2573`; `objectflow-service/src/main.rs:2650-2677` (no `record_attempt`) | none | reproduced `p18_publish_refusal_attempt.py` | **major** | A1 (every refusal), unusual sequence (a refused approval; a stale one may be normal) | Deployment test: each refused publish adds a `DeclarationChange.attempts` row with transition `publish`, the actor, the kind, the clause or verdict, and the change's id. Harness condition (§1 row): fail on a refusal verdict with no attempt row. |
| A-03 | An erasure that redacts an input in another object's past event records no event on that object if it was erased before. `redact_others` skips every erased object outright. The probe erased a Customer, then its Contact: the contact's erasure nulled `phone` in the customer's `add_contact` event, with no event and no version change. | `request.rs:2186-2188` | none | reproduced `p30b_caller_event_redacted_without_event.py` (checker-clean) | **major** | A1, unusual sequence (caller erased before callee) | Same harness condition as A-01. Suite: a `redact` event on the erased caller naming the redacted input. |
| K-04 | The digest holds `principal` as sent, not resolved, while the actor is resolved (ADR-0192 decision 7). The same request with its principal named by object id, or in another case, is a 409 `key_reused`. | `request.rs:124` | none | reproduced `p7_keys.py`: `principal: "user-1"` and `"ana"` after `"Ana"` give 409 | **minor**: a wrong refusal of the same request; nothing applied twice or misattributed | A4 | Suite: one keyed request sent with the principal by identity, object id and other case replays all three. |
| R-06 | `propose: true` on a DeclarationChange transition takes it instead of filing it or refusing `not_proposable`, because `changing` is tested before `propose`. A `withdraw` asked as a proposal withdrew the change. | `request.rs:635-643` | none | reproduced `p8_propose_routes.py` | **minor**: no flow rule is bypassed, and the actor may take it directly; but a change happened where a refusal was due | R (proposals route) | Suite: `propose: true` naming a DeclarationChange, or a Proposal, is refused `not_proposable` and changes nothing. |
| A-04 | `taken_by` refused through a cascade answers remedy `unreachable_from_here`, not `delegable`. Through a call it answers `delegable`. `flow-format.md:176` says delegable through a cascade too. | `request.rs:5215-5291` (`writes_of` sees create and call, not cascades) | `taken_by_generates_the_record_and_the_invariant_over_it` (no cascade case) | reproduced `p6_taken_by_routes.py`: `close_all` gives `unreachable_from_here` | **minor**: a message that misleads (an agent is told it can never be done, not to hand it to a person) | A1 (the refusal's record) | Suite: a `taken_by` failure reached by a cascade has remedy `delegable`. |
| A-05 | `check`'s satisfied verdict names an object id and event positions it allocated in its simulation. The next real request takes them for something else. | `request.rs:5620-5646` | `a_check_simulates_writes_nothing_and_names_what_it_did_not_ask` | reproduced `p28_check_names_events.py`: the check named `order-2`, `events [3]`; position 3 became `user-2`'s `add` | **minor**: misleads without risk | A2 | Suite: a check's satisfied verdict lists its events and new ids as `not_computed`, or names none. |
| A-06 | A publish that changes an attribute's type sets each object's value aside (`storage-schema.md:779`) with no event on the object. The report's `type_changed` notice says `affected: 0` while objects held values. | `publish.rs:599-611`; `store.rs:2754-2783` | `a_version_unmarking_dropping_or_retyping_a_personal_value_leaves_it_erasable` | reproduced `p17_publish_record.py`: note-1's `score` 7 becomes absent, version unchanged | **minor**: a declared drop-and-add; the report understates | A1 | Suite: the `type_changed` finding names each object holding a value, and each gets a `migrate` event listing the attribute set aside. |
| I-04 | A pair of identities equal but for case, written as 0.2.0 took them, stays two actors once a publish admits `identity_taken` (D619). A third spelling resolves to the first made. | `request.rs:255-270`; ADR-0195 decision 2 | `identities_equal_but_for_case_are_found_and_decided_by_an_admission` | unconfirmed (needs a 0.2.0 store) | **minor**: a recorded admission over legacy data; each event keeps its stored case | A3 | Keep D619's test. A start notice exists (`actors_equal_but_for_case_are_said_at_the_start`). |
| R-07 | On an unchecked flow, an effect that clears a required attribute is answered `500 internal` from the table's `NOT NULL`. No refusal and no attempt. The core checker refuses such flows (check 17). | `request.rs:3933-3943` | check 17; the schema | reproduced `p27_clear_required.py` | **minor**: sandbox alone | A1 | Suite: an absent required attribute after the effect is refused `<attr>_provided`, never a 500. |

### 2b. Holds

All in `request.rs` unless named. "Reproduced" names the probe.

| # | Instance | Where | Held by | Evidence | Grade | Promise | The check that keeps it held |
|---|---|---|---|---|---|---|---|
| H-01 | Actor unknown is refused `actor_known`; an actor in a final state `actor_active`; a principal likewise | `430-459` | `the_order_of_refusals_is_designs`; deployment `a_requests_principal_is_read_checked_and_recorded` | reproduced `p9_attempts.py`, `p21_identity_rules.py` | holds | R, A1 | as held, plus a harness actor that has left |
| H-02 | The same actor check on `file`, `decide_change`, `decide_proposal`, `publish`, `import_batch` and `maintain` | `2616`, `2984`, `3298`; `store.rs:2533`, `2312`; `import.rs:335-352` | `maintain_names_an_actor_the_store_knows` | reproduced `p18` (`publish` by `nobody`), `p16b_import_routes.py` (`import_batch` by `nobody`), `p9` | holds | R, A1 | Deployment test: each write operation by an unknown and by a final actor |
| H-03 | `type` beside the id of a declared type's object, a label or a subscription is `not_found` outside the family | `671`, `2624` | `an_object_named_beside_another_type_is_not_found` | read; tools name `request` (`tools.rs:408-412`) | holds | R | extended by R-01's check |
| H-04 | The Operator's transitions are not requestable | `706-710` | `the_operators_retirement_is_requested_by_nothing` | read | holds | R | as held |
| H-05 | `only_via` refuses a direct request | `711-715` | `the_order_of_refusals_is_designs` | reproduced `p20_only_via_call.py` | holds | R | as held |
| H-06 | State: `unavailable`; an assertion from any non-final state, an erasure from any | `717-727`, `1060-1068` | `the_order_of_refusals_is_designs` | reproduced `p8_propose_routes.py` (MERGED) and `p31` | holds | R | as held |
| H-07 | Inputs: unknown, wrong type, no such object (family-aware), missing; `_provided` | `729-773`, `1077-1137`, `1359-1366` | `a_flag_or_inputs_of_another_type_is_refused`, `a_value_of_another_type_is_refused_on_every_key` | reproduced `p9_attempts.py` | holds | R | as held |
| H-08 | `expected_version`: `stale` on request, filing, change and proposal | `466-478`, `701`, `2651`, `3022`, `3322` | `a_request_naming_a_moved_version_is_stale` | reproduced `p9`, `p8` | holds | R | as held |
| H-09 | `target_version` on approval; `400` elsewhere | `3376-3386`; `main.rs:2217-2221` | Studio `test_an_approval_naming_what_its_approver_read_is_held_to_it_alike` | read | holds | R | as held |
| H-10 | Proposals: `not_proposable`; expiry refuses approval; reject and withdraw held to `taken_by` (own withdraw excepted); approval runs as the approver, guards re-evaluated now, `taken_by` held, occurred bound measured from filing | `2652`, `3333-3372`, `3450-3494`, `1178` | `a_proposal_is_filed_decided_and_invalidated_by_an_erasure` | reproduced `p13_proposals.py`, `p13b_backdated_approval.py` | holds | R, A1 | Harness: the retrier also proposes, and a second actor approves |
| H-11 | `deny` refuses. An unknown or unanswered guard refuses, `temporal` for an evaluator. | `1422-1445` | `a_rule_is_evaluated_whole_for_its_record_and_decides_as_before` | reproduced `p9` | holds | R | as held |
| H-12 | `warn` returns a flag and writes an attempt row with `flagged`, `enforced: false` and `applied_position`. `audit` writes the row unflagged. A replay returns the recorded flags. | `1446-1466`, `1493-1525`, `147-153` | `a_refused_call_is_a_row_of_the_request_with_the_call_s_clause` | reproduced `p9` | holds | A1 | Harness condition (design §1): each flag returned is an attempt row of its event |
| H-13 | Declared invariants are checked on every object a write could break, along reference chains and type scans | `833-925`, `4865-4990`, `4467-4856` | `the_invariants_a_write_could_break_are_checked_and_no_others`, `a_traversal_invariant_is_checked_from_what_was_written`; harness "invariant" | reproduced `p21` (`leave` refused for `person-5`'s `boss_active`) | holds | R | as held; extend to imports (R-04) |
| H-14 | A state's required attributes; a non-optional attribute never absent (check 17, plus the column's `NOT NULL`) | `5012-5016` | harness "required"; check 17 | reproduced `p27` (the checker refuses) | holds | R | as held |
| H-15 | Uniqueness by family, with, where and in-scope, and an external identifier's uniqueness across types | `5017-5115`, `4483-4509` | `a_uniqueness_names_its_failure`, `a_family_shares_its_base_s_members_scans_and_uniqueness`, `an_external_identifier_is_found_by_its_source_and_unique_per_source` | reproduced `p9` (`ref_unique`) | holds | R | as held |
| H-16 | `identity_taken` across actor types, case-folded, the operator's included. Imports never admit it. Identities are well formed on inputs and imports. | `5120-5125`, `4352-4374`; `import.rs:1014-1023`; `wire.rs:131-162`, `277` | `an_identity_two_actors_hold_is_admitted_by_nothing`, `an_identitys_check_sweeps_no_actors` | reproduced `p21` | holds | A3 | as held |
| H-17 | An identity is written by an effect only from a value typed `identity` (check 17) | checker | check 17 | reproduced `p19_identity_by_effect.py` (the checker refuses; the sandbox writes it unchecked) | holds | A3 | as held |
| H-18 | `taken_by` holds on a direct request, a `create`, a `call` in a `foreach`, a cascade, and an approval | `model.rs:1283-1339` | `taken_by_generates_the_record_and_the_invariant_over_it` | reproduced `p6_taken_by_routes.py`, `p13` | holds | R | Harness: an agent actor in the fixtures |
| H-19 | `publish_taken_by` on `publish` and on moving another's change; the operator always may | `store.rs:2563-2573`; `3034-3055` | `a_version_is_approved_only_by_the_kinds_the_installed_one_declares` | reproduced `p18` | holds | R | as held |
| H-20 | Backdating: within `backdating_limit` (or `max_recording_delay`), never in the future, never before the current interval of what changes; a correction takes none | `1171-1210`, `2406-2459`; `model.rs:943-961` | `a_backdated_request_is_held_to_its_bound_and_its_object_s_intervals`; harness backdater | reproduced `p9`, `p13b` | holds | R | as held |
| H-21 | `over_limit` for a `foreach` and a cascade; the call depth | `4059-4070`, `4175-4182`, `1037-1041` | `a_chain_of_calls_stops_without_a_crash` | reproduced `p25_limits.py` | holds | R | as held |
| H-22 | `value_in_range` for arithmetic, decimals, money, the calendar and a spent sequence | `397-424`, `3853-3924`, `4250-4255` | `an_effects_arithmetic_past_its_bound_is_refused`, `a_wrapped_sequence_refuses_only_what_mints_from_it` | read | holds | R | as held |
| H-23 | Supersession: `successor_not_superseded`; no chain closes | `1267-1289` | `an_effect_writes_and_a_supersession_records_its_successor`; harness customers fixture | reproduced `p31_customers_supersession_erasure.py` | holds | R | as held |
| H-24 | `whole_open` on an attribute input; `not_erased` on a top-level step | `1211-1266` | `a_part_is_not_moved_into_a_finished_whole` | reproduced `p12` (`move_to`), `p2` (`add_note`) | holds on these routes; see R-02, R-03 | R | R-02 and R-03's checks |
| H-25 | Recording guards `subject_open`, `corrects_current`, `subject_owned` | `1290-1358` | `a_recording_is_dated_corrected_once_and_kept_off_a_finished_subject` | read | holds | R | as held |
| H-26 | Assertions: `to` among those listed, `admits` within `may_admit`; standing admissions until discharged; reason recorded | `1140-1164`, `862-901`, `1941-1946` | `an_assertion_admits_what_it_names_until_it_holds_again` | reproduced `p6` (`force` recorded by bot, agent, reason) | holds | R, A1 | as held |
| H-27 | Erasure: own, group, `forget`, `redact` of others, invalidated proposals; every event by the requester, with its cause | `1587-1700`, `2157-2604`, `3527-3677` | `an_erasure_redacts_its_history_reaches_its_observations_and_stays`, `a_groups_erasure_reads_no_type_whole` | reproduced `p31` | holds, except A-01 and A-03 | A1 | as held |
| H-28 | A cascade's part refusal refuses the request, qualified; final parts are skipped | `4118-4209`, `519-530` | the examples' expected cascades (`check/examples.rs`); harness delivery fixture | reproduced `p6`, `p25` | holds | R | Suite: a cascade whose part refuses names the part's qualified clause and writes nothing |
| H-29 | Publish: every object's invariants, required attributes, reachable states and stranded parts after the mappings; decisions and admits; invalidated proposals and superseded changes; each a recorded event by the approver | `publish.rs:572-886`, `1088-1142`; `store.rs:2786-3119` | `what_no_mapping_covers_is_fatal_and_an_unadmitted_violation_a_decision`, `an_approval_is_refused_on_anything_its_approver_did_not_see` | reproduced `p17` (`migrate` by operator) | holds, except I-03 and A-06 | R, A1 | as held |
| H-30 | Import: mirrors (and observations on mirror subjects) only; placed objects' invariants, required attributes, external ids and identities; an erased object's personal values skipped; events by the importer, marked `imported` | `import.rs:400-423`, `622-629`, `898-1052`, `1202-1223` | `a_shared_identifier_an_erasure_and_a_mirror_s_observations_are_kept_to_their_rules` | reproduced `p16b_import_routes.py` (owned type refused; `import` by `operator`, `human`; unknown actor refused) | holds, except R-04 | R, A1 | as held |
| H-31 | A mirror takes no requested transition but erasure; a retired type takes none | `flow-format.md:498`; `publish.rs:558-565` | checker | reproduced `p16b_import_routes.py` (`hand_out` on a mirror: `unknown_transition`) | holds | R | as held |
| H-32 | Every transition's event records the actor's identity as held, its type's kind and the resolved principal | `1897-1958` | harness "record" (actor only); `a_principal_is_resolved_recorded_and_written_by_an_outcome` | reproduced `p6`, `p13`, `p31` | holds | A1 | Harness: also compare `actor_kind` with the actor type's kind, operator included (today skipped, `harness/src/lib.rs:304`) |
| H-33 | A satisfied verdict names its request's events: request, filing, change, approval (proposal event first), publish | `967-974`, `2796-2803`, `3208-3214`, `3499-3521`; `store.rs:2660-2669` | harness "record" | reproduced throughout | holds (A-05 aside) | A2 | as held |
| H-34 | An attempt for every refused request: actor as resolved, or as named with kind `unknown`; principal; type and object (none for an unknown id); clause and remedy; `key_reused` too | `5583-5594`, `5558-5578`, `5672-5775` | `a_refused_attempt_keeps_no_input_its_transition_does_not_declare` | reproduced `p9` | holds (A-02 aside) | A1 | Harness condition (§1): every refusal verdict has a row |
| H-35 | A refused approval is a row of the proposed transition naming the proposal | `3488-3494`, `5679-5682` | Studio tests | reproduced `p13` | holds | A1 | as held |
| H-36 | A key belongs to the resolved actor (object id, identity, case); the same request replays its verdict and events; a different non-personal request is `key_reused` | `5503-5521`, `98-132` | `a_key_replays_what_it_applied_and_refuses_another_request`; harness "key" | reproduced `p7`, `p26` (`reprice`) | holds (K-01 to K-04 aside) | A4 | as held |
| H-37 | An erased actor's identity is replaced by its object's id wherever recorded, so it names no other later holder | `1693-1697` (`renamed`) | `an_erased_actors_identity_leaves_the_store` | read | holds | A3 | as held |
| H-38 | A proposal records its proposer by identity; a change its drafter and approver with kinds | `2746`, `3101-3104`; `store.rs:2798-2802` | D611's tests | reproduced `p13`, `p18` | holds (I-01 aside) | A1 | as held |
| H-39 | Writers are serial; a prepared dry run is taken only where nothing moved | `3138-3142`; `BEGIN IMMEDIATE` | `concurrent_writes_lose_no_update`, `a_submit_takes_a_prepared_dry_run_only_where_nothing_moved` | read | holds | R (races) | as held |

### 2c. Not a finding

| # | Instance | Where | Why |
|---|---|---|---|
| N-01 | `expected_version` on a creation is ignored | `466-478` | A creation names no object read. `p8` |
| N-02 | A key whose first request was refused records nothing; a later different request applies under it | `5524-5537` | Only what commits replays (ADR-0041). `p7` |
| N-03 | A key replays after its actor left | `5512-5521` (before `662`) | A replay applies nothing. `p7` |
| N-04 | Refusals of `import_batch`, `maintain` and `acknowledge` are values with no attempt | `main.rs` | Operational calls (`adversarial-harness.md:56`). The import names its refusal in its answer. |
| N-05 | A backdated internal change that moves nothing tracked may precede its object's creation | `1188-1210` | `declaration-syntax.md:594` checks only what changes. `p21` |
| N-06 | `check` records nothing | `5620-5646` | A simulation by design (ADR-0054) |

## 3. The broken items, most severe first, with the smallest fix

1. **A-01 (blocker).**
   - **Fix:** in `forget_observations` (`request.rs:3571-3584`), give each label whose note is removed a `redact` event, as `redact_others` does: version +1, source `erased`, cause the erasure, payload `{"redacted": ["note"]}`. The simplest way is to push the label into the `cleared` set `redact_others` reads. Align `declaration-syntax.md:911` and `storage-schema.md` §9 step 14 with `DESIGN.md:575`.
   - **Refuses nothing.**
2. **R-01 (major).**
   - **Fix:** in `decide_as` (`request.rs:631-643`), before routing to `decide_proposal` or `decide_change`: where `req.type_` is given and the target is not of its family, refuse `not_found`. The refusal is recorded as an attempt.
   - **Refuses** a request naming a declared type beside a proposal's or a change's id, which 0.2.0 ran.
3. **I-02 (major, D669).**
   - **Fix:** a publish adding a type T is fatal where an identity matching `t-<digits>`, in any ASCII case, is held. Also: `resolve_actor` refuses an actor string that names both an object and a held identity.
   - **Refuses** such publishes, which 0.2.0 took.
4. **I-01 (major).**
   - **Fix:** a generated guard refuses a write, by input or by effect, to an attribute marked `actor_kind` on an existing object (erasure excepted), with both checkers refusing such a transition. The alternative is a change of identity renaming the record as an erasure does, with no reuse of an identity once held.
   - **Refuses** flows and requests 0.2.0 took.
5. **R-04 (major).**
   - **Fix:** run `affected_by_writes` over the import's working world. An invariant it breaks on an object the batch does not place refuses the batch, naming object and invariant. A disposition cannot admit it, since the object is not the batch's.
   - **Refuses** batches 0.2.0 wrote.
6. **R-03 (major).**
   - **Fix:** move `whole_open` after the effect, for every object the transition writes, creations included: a composite owner end whose value changed (or was set at creation) to a whole in a final state is refused.
   - **Refuses** requests 0.2.0 applied.
7. **R-02 (major).**
   - **Fix:** recurse into `Foreach` at `request.rs:1253`. Or, more robustly, check after the effect that no personal attribute of an erased object changed to a present value.
   - **Refuses** requests 0.2.0 applied.
8. **R-05 (major).**
   - **Fix:** in `take()` at depth > 0, a transition with `only_via` is refused `not_requestable` unless the causing transition (`Type.transition` of `ctx.causes`' top) or the cascading whole's transition is listed. Add check 13's converse to both checkers.
   - **Refuses** flows and requests 0.2.0 took.
9. **K-01 and K-02 (major).**
   - **Fix:** mask only the inputs personal to the transition (`input_personal`, or a personal attribute of its type), not by name in any type. For those, keep a digest over their values that an erasure replaces with a marker refusing every request under the key (`key_reused`) instead of replaying.
   - **Refuses** a different request under a reused key, which this candidate replays; 0.2.0 refused it too.
10. **K-03 (major).**
    - **Fix:** treat `ERASED_DIGEST` as matching no request (`request.rs:5516`).
    - **Refuses** an identical retry under a 0.2.0-era key after an erasure, which is now replayed.
11. **A-02 (major).**
    - **Fix:** write an attempt (type `DeclarationChange`, transition `publish`, actor, kind, clause or verdict, the change's id) for every refused publish, in `store.publish` or its handler.
    - **Refuses nothing.**
12. **A-03 (major).**
    - **Fix:** in `redact_others` (`request.rs:2186-2188`), skip only the clearing of attributes for an erased object, and still record its `redact` event naming the redacted inputs.
    - **Refuses nothing.**
13. **I-03 (major).**
    - **Fix:** decode each backfilled value as an input to its attribute is decoded (`identity_fault`, type fit, scale, calendar). A failure is fatal, naming the objects.
    - **Refuses** publishes 0.2.0 took.
14. **K-04 (minor).** Resolve `principal` in `request_digest` as the actor is. Refuses nothing.
15. **R-06 (minor).** Refuse `propose: true` on a DeclarationChange or a Proposal, `not_proposable`, in `decide_as`. Refuses requests 0.2.0 took.
16. **A-04 (minor).** Count a cascade as one hop in `writes_of`, or give a `taken_by` failure `delegable` always. Refuses nothing.
17. **A-05 (minor).** Give a check's events and new ids as `not_computed`. Refuses nothing.
18. **A-06 (minor).** List the objects holding a value in `type_changed`, and record a `migrate` event on each. Refuses nothing.
19. **I-04 (minor).** None beyond D619's admission; documented.
20. **R-07 (minor, sandbox).** Refuse an absent required attribute after the effect, `<attr>_provided`. Refuses nothing that runs.

## 4. Seen outside this area, one line each, for its owner

- **Area 1 (P):** `export` of the log answers a caller's input that is personal only by flowing into another object's personal attribute (`Customer.add_contact.phone` into `Contact.phone`, event 4 in `out/p30_caller_taint_record.txt`), though it omits personal values. The caller's own erasure leaves that input in its event (`p30b`).
- **Area 1 (P):** an erasure admits only the invariants it breaks one hop away (`request.rs:3692-3706`). One two hops away refuses the erasure, so it is never carried out. Unconfirmed.
- **Area 3 (K):** check 13's converse (a call from an unnamed parent) is in neither checker (`check/model.rs:1201`; `scripts/check-flows.py:2048-2056`). See R-05.
- **Area 3 (V):** a backfill's value is not typed or decoded against its attribute; a string into an identity is accepted (`publish.rs:694-721`). See I-03.
- **Area 3 (K):** an `add` or `remove` of an absent value answers `501 not_run` (`request.rs:3949-3951`). Unconfirmed on a checked flow.
- **Area 3 (V):** an effect may write a string past the 64 KiB a request may send back; no bound on effect-written text (`request.rs:3853-3931`). Unconfirmed.
- **Area 3 (K) or 6 (G):** the core checker refuses `backdating_limit: 1 day` (check 58, "days") while `wire.md:38` lists `day(s)`. Seen in `p13`.
- **Area 5 (E):** `acknowledge` reads no actor (`main.rs:2143-2154`): any name moves any subscription's cursor. A-01's and A-03's changes never reach `pull`.
- **Area 4 (C):** under the writer:
  - `corrects_current` reads every object of each type with `corrects` (`request.rs:1310-1319`);
  - `invalidate_proposals` reads every Proposal on every erasure (`2476-2482`, `2532-2538`);
  - `forget_observations` reads each kind's and label's objects whole (`3546-3578`).
- **Area 6 (G):** `agents.md:135` promises the type binding R-01 breaks. `wire.md:63` says inputs "marked personal" count by name, while the engine masks by name in any type (K-01).
- **The harness, for the author:**
  - `recorded()` checks the actor's id only, not its kind, and skips the operator (`harness/src/lib.rs:304`);
  - `erased()` runs only right after the erasure, so a later write-back (R-02) is unseen (`lib.rs:231-235`);
  - there is no attempt-log condition;
  - the guesser never sends `type` beside an id;
  - the retrier never varies one input.
