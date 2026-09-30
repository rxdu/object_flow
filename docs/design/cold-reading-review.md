# Cold reading of what the engine does beyond a request, 2026-09-30

Status: **findings recorded, triage pending.** At the author's direction ("review the open questions, unless particularly important, you may infer and make the decisions"), three readers with no context of the design read the parts no earlier round had, each asked whether two careful builders following the text could build engines that behave differently (`TODO.md`, the items on cold readers). They found 97 findings, about 85 distinct once the overlaps between reports are merged. Their decisions are to follow, and this record will carry them.

**Method.** Each reader was given only what a builder gets for its area and barred from the decision records, the defect register, `TODO.md`, the lessons, the reviews and the code; each checked its quotations against the files by script. Their reports follow as they wrote them.

- **§1, the unreviewed keys** (30 findings): recording, flow change, evaluators and the keys no reviewed example used, over the format's example modules, and which objects three refusals name (ADR-0122's decision 40).
- **§2, metrics** (26 findings): intervals, buckets, windows, aggregates, dimensions, the standard metrics and flags, computed over `metric-scenarios.md` and made-up histories.
- **§3, beyond a request** (41 findings): recording and correcting, publishing with a migration, the import, subscription and delivery, erasure, the proposal and flow-change lifecycles, and the adversarial harness.

## 1. The unreviewed keys, as the reader wrote it

Read: `docs/design/flow-format.md` whole, `flow.schema.json`, `examples.schema.json`, the seven modules and their examples files, and only the sections of `declaration-syntax.md` and `DESIGN.md` that `flow-format.md` cites for these constructs (listed at the end, with every place I had to leave `flow-format.md`). Nothing in the repository was changed.

### What is determined

Without choosing between readings I could reconstruct: recording as the generated creation `record` taking `subject`, the fields and an optional `corrects`, landing in `RECORDED`; the generated guards' order (`occurred_within`, `whole_open`, `not_erased`, `successor_not_superseded`, then for a recording `subject_open`, `corrects_current`, `subject_owned`) and their remedies; that every read and metric sees only uncorrected observations, that a second correction of one record is refused by `corrects_current`, and that a correction is exempt from `subject_open`, so every verdict in `service.examples.yaml` follows. Supersession in `customers.yaml` is determined: the successor rules, a self-merge refused as `call_refused` by `Customer.successor_not_superseded` with `self_serviceable`, contacts surviving `merged_into`, and erasure following the chain. Evaluators are verdict-only, the whole condition or its negation, and one that does not answer fails its guard as unknown with `temporal` under any enforcement, and every evaluator a request's guards name is consulted first, before any other check (DESIGN.md §6). A migration reaches every object, final ones included, and applies its mappings as recorded `migrated` events. A publish that adds a required attribute with no backfill, or removes a member an observation holds, is refused. An admission stands until its invariant holds again (DESIGN.md §6 step 7), so a service-v2 DONE job admitted for `photo_when_done` can still be erased. `inventory-v2.yaml` declares no `migration`, although the brief described it as having one, so its publish writes no object. `service-v2.yaml`'s `removed_members` maps a member that no attribute holds, since `ReassignmentReason` is only the type of `reassign`'s declared input, so by "Events keep the old value" it rewrites nothing, although its description says it "merges" the reasons (finding 10). The grouping, windowing and current-span rules of `median_time_in_state` are determined. So is the objects list of a guard reading another object (below). By the determined meaning of `.overrides`, `customers.yaml`'s `overrides` metric, described as "How many times a customer's state was overridden, by reason", counts every erasure of an ACTIVE customer (reason `erasure`) and every migration `admit` (finding 18). No finding in: `scope`, `remove`, `superseding` and `supersede` within a request, `inherited_parts` beyond finding 17, `approvals.yaml` and `people.yaml`.

### Which objects a refusal names

1. **A guard reading another object.** `service.examples.yaml` `a_job_is_not_given_to_an_engineer_who_left`: `reassign` to Ben after `leave`, refused by `engineer_active: inputs.engineer.state == User.ACTIVE`. The rule of DESIGN.md §5.5 gives exactly `[ben]`: determined, but only by DESIGN.md. `flow-format.md` §4.8's refusal paragraph never mentions that an `unsatisfied` verdict names objects.
2. **An invariant.** `inventory.examples.yaml` `a_serial_is_in_stock_once`, `serial_unique`: **not exactly determined** (finding 29). Where two invariants fail, which one the refusal names is also not determined (finding 30).
3. **Over a collection.** `service.examples.yaml` `a_failed_check_holds_the_job_open`, `no_unresolved_failure`, a nested aggregate: **not exactly determined** (finding 28). By contrast `customers.yaml`'s `no_open_orders`, one aggregate over `referrers` with a `where`, gives exactly the open orders. The objects of a generated guard's refusal are not determined either (finding 27).

### Findings

#### 1. `max_recording_delay` has no stated meaning in the format
- **Construct:** `observations.<o>.max_recording_delay`; a recording's `occurred_at`
- **The readings:** (a) It is the model's `occurred within`. A supplied `occurred_at` must lie within the bound and not after the recorded time, and on a kind without the key any supplied `occurred_at` is refused by `occurred_within`. (b) It is what the schema says, a bound on the delay only. A future `occurred_at` passes, and a kind without the key takes any `occurred_at`. A `record` of `InspectionResult` with `occurred_at` one hour ahead is refused `unsatisfied occurred_within` by (a) and applied by (b). On a kind declared without the key, an `occurred_at` of yesterday is refused by (a) and applied by (b). No example can settle it: an examples-file request has no `occurred_at` (the `request` properties of `examples.schema.json`), and it is never an input. The same gap applies to `backdating_limit`.
- **Where the text fails:** `flow-format.md` §4.4: "Each has a `kind` (a type name), a `description`, its `attributes` (§4.3), and MAY have a `max_recording_delay`, a duration such as `7 days`, and `invariants` (§4.6)." That is the only sentence on the key, and §6 has no row for observations at all, so the conversion to `occurred within` has to be inferred. `flow.schema.json`, `observation.max_recording_delay`: "How long after the fact a datapoint may still be recorded." `declaration-syntax.md` §6.8: "Omitted, the occurred time is the recorded time, and a request that supplies one is refused by `occurred_within`." `declaration-syntax.md` §4.2: "The occurred time arrives as the request's `occurred_at` field, never as an input."
- **Severity:** changes-outcome
- **Suggested fix:** Add a §6 row converting `max_recording_delay: d` to `occurred within d`, state both bounds and the omitted case in §4.4, and give an examples request an `occurred_at`.

#### 2. The occurred time of a correction
- **Construct:** `record` with `corrects`; `.occurred_at`; `occurred_within` on a correction
- **The readings:** (a) A correction is a new recording. Without an `occurred_at` its occurred time is its own recorded time, and one that supplies the replaced record's time is held to the kind's bound like any recording. (b) A correction replaces the record, so it keeps the replaced observation's `occurred_at`. As it is exempt from `subject_open`, it is exempt from the bound too. Take service's `inspection_pass_rate`, windowed on `r.occurred_at`, over the last 7 days, and a result recorded 10 days ago and corrected today: (a) counts the corrected row in the window and (b) does not. A correction that supplies the 10-day-old time is refused `occurred_within` under (a) and applied under (b).
- **Where the text fails:** `flow-format.md` never mentions `corrects` on a recording. §11.3 lists only "`subject` and the kind's fields as inputs". `declaration-syntax.md` §6.8 gives the input: "an optional `corrects`, an observation of the same kind and subject that this one replaces". It gives the bound: "`occurred_within`, that the request's `occurred_at`, if it supplies one, is no further back than the kind's `occurred within` bound and no later than the time it is recorded (ADR-0099);". It says nothing of a correction's own occurred time, although `subject_open` gets an exemption: "A correction is exempt, since it adds no fact to a settled whole but replaces one already recorded".
- **Severity:** changes-value
- **Suggested fix:** State in §4.4 what a correction's `occurred_at` is when it supplies none, and whether `occurred_within` applies to it.

#### 3. Whether a recording re-checks its subject's invariants
- **Construct:** a recording; an invariant of the subject that reads its observation collection (§5 lets a type's expression read "observation kinds")
- **The readings:** (a) The new observation is reached through the subject's part end, so the subject's traversal invariants are re-checked. As the destination whole of an owner write, the subject also counts as written and its version advances. (b) "recording one re-checks nothing else", and a recording "advances no version". Give ServiceJob the invariant `count(r in inspections where r.outcome == CheckOutcome.FAIL) <= 2` and record a third FAIL on a WORKING job. (a) refuses it `invariant_violated`. (b) applies it, and the job then stands in an unadmitted violation that refuses its next transition. A concurrent `start` that carries the job's earlier `expected_version` is `stale` under (a) and not under (b).
- **Where the text fails:** `declaration-syntax.md` §3.4: "An invariant is checked after every write to the object, and after a write to any other object it reads." Its traversal row adds "this object is written, or any object reachable through that end". `declaration-syntax.md` §6.8: "They are checked when the observation is recorded, and they are local by construction, so recording one re-checks nothing else." and "It advances no version and writes no attribute". DESIGN.md §6 step 7: "where a transition writes a part's `owner`, **both** the source whole and the destination whole count as written". `flow-format.md` is silent.
- **Severity:** changes-outcome
- **Suggested fix:** Say in §4.4 whether a recording re-checks its subject and advances its version, and allow or refuse subject invariants over observations to match.

#### 4. What an attribute correction replaces
- **Construct:** `corrects` on an internal or external transition
- **The readings:** (a) A correction is an ordinary write at its own time, with a provenance mark. The interval of the wrong value stands until the correction, and `.held(…)`, `entered_at(…)` and `time_in` read the wrong value for the past. (b) A correction says the record was wrong, as an observation's correction does, so history is read with the corrected value from the original write on. Take a hypothetical ServiceJob `correct_engineer` with `corrects: [engineer]`. `time_working` grouped by `engineer`, which reads `i.held(engineer)` (§6), attributes the WORKING time before the correction to the wrong engineer under (a) and to the right one under (b). customers' `correct_email` cannot show it, since `email` is not tracked.
- **Where the text fails:** `flow-format.md` §4.13: "**A correction** is an internal or external transition marked `corrects: [<attribute>, …]`, whose events carry the `corrected` provenance." `declaration-syntax.md` §6.4: "`corrects <attribute>, …` produces the `corrected` provenance; the outcome must write exactly the attributes named, and a `reason` input is required." The transitions rows of §6.9 carry no `.corrected` member.
- **Severity:** changes-value
- **Suggested fix:** State whether a correction rewrites intervals and `.held`, or is a change at its own time. In the second case, give transition rows a `.corrected` member.

#### 5. Whether an unadmitted invariant violation refuses a publish
- **Construct:** `migration`, `admit`; an invariant a migrated object breaks; a new invariant
- **The readings:** (a) The migration runs a request's steps 5 to 8, so an invariant the migrated objects violate without an `admit` refuses the publish. (b) The violation is "Reported as a decision, not failed": the publish applies, and the objects stand in violation with no admission. Take `service-v2.yaml` without its `admit` line. Every job is written by the `site` backfill, and v1 DONE jobs without a photo break `photo_when_done`. (a) refuses the publish. (b) applies it, and afterwards `forget` on such a job is refused `invariant_violated`, because step 7 exempts only admitted violations: an erasure is blocked. The same split holds for a new invariant over objects that no mapping writes. An example is an inventory v3 that adds `state != SOLD or photo is not null` with no migration, over items sold under v1's `audit` guard. Step 7 checks only written objects, yet `admit` reaches "each object that violates the invariant after the publish".
- **Where the text fails:** `flow-format.md` §4.16 says only "invariants the migrated objects may still break", and its `migration` notice lists "a new required attribute with no backfill, a member gone with no mapping, and an attribute gone where one of its type is new", with no invariant. DESIGN.md §6: "A **migration** a publish applies runs steps 5 to 8 for each object as part of the publish's transaction, with the publish's `admit` lines as its admissions (§5.9, ADR-0105)." `declaration-syntax.md` §10, check 23: "Reported as a decision, not failed: every invariant the objects a mapping writes would violate, with the objects, until an `admit` covers it or the mapping or the objects change (§6.6, ADR-0105)". `declaration-syntax.md` §6.6: "Neither is refused by construction: the violation is resolved by the objects' own transitions before the publish, by a different mapping, or by an **`admit`**".
- **Severity:** changes-outcome
- **Suggested fix:** State in §4.16 either that a publish is refused while any live object would violate an invariant without an admission, new invariants over unwritten objects included, or that it is not, and in that case what governs those objects afterwards.

#### 6. The order in which a migration applies its mappings, and whether a backfill overwrites
- **Construct:** `backfill` beside `renamed_attributes`, `removed_states`, `removed_members` and other backfills
- **The readings:** (a) The mappings apply in one order (renames, members, states, then backfills in the order written), and a backfill reads the object as the earlier mappings left it. (b) Every backfill reads the object as it was before the publish, in one simultaneous write. A backfill `ServiceJob: { photo_known: "completion_photo is not null" }` beside the rename reads the renamed value under (a) and an absent one under (b). A backfill reading `state` on an object in a removed state gets the new state under (a) and the old one under (b). Separately, "each attribute a live object must be given" suggests filling only objects that lack a value, and "applies it to each live object" suggests overwriting all. A v2 that makes inventory's `recipient` required with `backfill: { InventoryItem: { recipient: '"unknown"' } }` keeps "Acme" on a reserved item under the first reading and overwrites it under the second.
- **Where the text fails:** `flow-format.md` §4.16: "for each type, each attribute a live object must be given, and the expression that gives it" and "a backfill MUST write a stored attribute and read only the object's own members and literals, not `now` or an input". Neither says which version's members. `declaration-syntax.md` §6.6: "publishing applies it to each live object as a migration transition with provenance `migrated`."
- **Severity:** changes-value
- **Suggested fix:** State the order of application, which version's names a backfill reads, and whether a backfill writes only objects that lack a value.

#### 7. What a rename carries
- **Construct:** `renamed_attributes`
- **The readings:** The text leaves three questions open. (i) A rename that changes type or optionality, such as `photo: file?` to `completion_photo: string`, is either refused or carries the values across unchanged. (ii) A renamed tracked member either keeps its history or counts as "newly declared" and is tracked from the publish. Rename ServiceJob's `engineer` to `technician`: `time_working` by `i.held(technician)` reports pre-publish spans under their engineer if history is kept, and under the absent value if tracking starts at the publish. (iii) A rename of an observation kind's field, which the schema's `typeName` key admits for `InspectionResult`, either renames the stored field or is refused because a migration never reaches an observation. `avg(r.voltage)` over results recorded before the rename differs between the two.
- **Where the text fails:** `flow-format.md` §4.16: "for each type, each attribute renamed, from its old name to its new" and "a mapping MUST name a state, member or attribute this version no longer has and move it to one it has". `declaration-syntax.md` §6.6: "a member the publish makes **tracked** — a new enum attribute or singular stored reference, or one newly declared on an existing type — is tracked from the publish" and "It never reaches an **observation**, which is born final (PRD D4)".
- **Severity:** changes-outcome
- **Suggested fix:** Require a rename to keep type and optionality, state that it carries history, and state whether it applies to observation kinds.

#### 8. A removed state mapped to a final or superseding state
- **Construct:** `removed_states` whose target is a final state of a whole, or a `superseding` state
- **The readings:** (a) The move is not a transition, so no cascade names it and the parts stay as they are. (b) The move enters a final state, so the parts go as the coverage rule requires of every entry into one: cascaded as `close` would cascade them, or the publish is refused for want of a cover. For a superseding target, (a) leaves an object superseded with no successor and (b) refuses the mapping. Take a customers v2 that drops DORMANT with `removed_states: { Customer: { DORMANT: CLOSED } }`. Under (a), a dormant customer's contacts stay LISTED under a CLOSED customer, and nothing can ever discard them, since `discard` is only via `close`. Under (b) they are discarded, or the publish is refused.
- **Where the text fails:** `flow-format.md` §4.16: "for each type, each state the previous version had and this one does not, and the state its objects move to". §4.3: "a part lives no longer than its whole" and "Every transition of the whole into a final state MUST be covered by a cascade or by `survives`, and none by both (step 3, `names`; the model's check 37, ADR-0058)." §4.19: "a transition into it names the successor with a `supersede` step". DESIGN.md §6 runs "steps 5 to 8" for a migration, and step 6 takes only the cascades that a transition's `on` names.
- **Severity:** changes-outcome
- **Suggested fix:** State that a mapping into a final state drives no cascade and is refused where the type has live parts or the state is superseding, or say what it drives.

#### 9. An object's `declaration_version` after a publish
- **Construct:** the built-in member `declaration_version`; every metric's `version` dimension
- **The readings:** It could be (a) the version the object was created under, (b) the version of its latest event, so that migrated objects move to the new version and unmigrated ones stay, or (c) the version now installed. Read service's `handoffs_per_job` (source `objects`) by `version` after the v2 publish, whose backfill writes every job. Jobs opened under v1 fall under version 1 in (a) and under 2 in (b) and (c). After `inventory-v2`, which migrates nothing, items fall under 1 in (a) and (b) and under 2 in (c).
- **Where the text fails:** `flow-format.md` §5: "Every object also has `id`, `open` (true while its state is neither `closed` nor final), `created_at`, `created_by_kind`, `recorded_from` and `declaration_version`, and every mirror `imported_at`, each read by its name as `state` is (`declaration-syntax.md` §8, ADR-0096, ADR-0106)." The cited §8 defines `created_at`, `created_by_kind` and `open`, and not `declaration_version`. §4.9: "and `version` and `actor_kind`, which every metric has unless it declares them".
- **Severity:** changes-value
- **Suggested fix:** Define an object's `declaration_version` in §5.

#### 10. Pending proposals across a publish
- **Construct:** `proposable`; a publish with `removed_members` or `renamed_attributes`
- **The readings:** A proposal filed under v1 and approved under v2 is (a) invalidated, because v2 "can no longer express" it, (b) executed with the migration's mapping applied to its recorded inputs, or (c) refused on approval as `invalid_input` and left pending. Had service's `reassign` been proposable, a pending proposal with `reason: ReassignmentReason.SKILLS` would give the three different outcomes under service-v2, and so would a pending `finish` carrying `photo`. The criterion "can no longer express" is defined nowhere I was pointed to, and `flow-format.md` never mentions invalidation. An example cannot expect it either: the `proposal` expectation takes only `pending`, `executed`, `rejected` or `withdrawn`. The same example shows the related gap. service-v2's `removed_members` maps a member that no attribute holds, so it merges nothing, yet its description says it "merges the skills reason for a reassignment into workload".
- **Where the text fails:** DESIGN.md §9: "A guard that fails leaves the proposal pending with the verdict attached, and the approval returns that verdict, as the request would; a declaration that can no longer express the request invalidates it (ADR-0036, ADR-0044)." `flow-format.md` §11.4, `proposal`: "for an `approve`, optional: the state it leaves the proposal in, `executed` where it applies, `pending` where its request is refused, and the state it was in where it is refused as `unavailable`". `declaration-syntax.md` §6.6: "Events keep the old value, which is read under the version that wrote it (check 23);"
- **Severity:** changes-outcome
- **Suggested fix:** Define in §4.16 which pending proposals a publish invalidates and whether mappings apply to their inputs, and add `invalidated` to the examples' `proposal`.

#### 11. "Live objects"
- **Construct:** the objects a `migration` moves
- **The readings:** (a) Every existing object, final ones included. (b) The objects not yet closed or final. That is the word every module here uses for a category (`categories: [live, closed]`). Under (b), service-v2's backfill and `admit` would skip DONE jobs, which its own description ("admits the finished jobs") rules out. `declaration-syntax.md` §6.6 settles (a); `flow-format.md` never says so.
- **Where the text fails:** `flow-format.md` §4.16: "A module's `migration` section says how a publish of this version moves the live objects of the version before it". `declaration-syntax.md` §6.6: "a migration reaches **every object the change affects, terminal ones included**".
- **Severity:** wording
- **Suggested fix:** Say "every object, final ones included" in §4.1 and §4.16.

#### 12. `fresh`: whose clock, and whether a verdict is reused
- **Construct:** `evaluators.<e>.functions.<f>.fresh`
- **The readings:** (a) The engine consults the evaluator on every request and stamps the as-of time itself. A verdict is then at most a transaction old, and `fresh` refuses only a consultation that outlives the bound through retries. (b) The evaluator reports its own as-of time, for a cached answer, and the engine refuses one older than `fresh`. (c) The engine may reuse a verdict across requests while it is younger than `fresh`, which the customers condition's description ("as Xero said within the last half hour") implies. Suppose Xero answers "valid" for an order's invoice at 09:00 and voids the invoice at 09:05, and `fulfil` is requested at 09:10. (c) applies it on the 09:00 verdict. (a) and (b) consult again and refuse it `unsatisfied invoice_valid_in_xero`. Under (c), `contact_exists`, which has no `fresh`, could be reused forever or never. No example can express a stale verdict: the stubs are `satisfied`, `unsatisfied` or `unanswered`, and the clock does not move within a request.
- **Where the text fails:** `flow-format.md` §4.18: "a function declares a `description`, its `arguments` in the order a call passes them, each as a declared input is (§4.8), and, optionally, `fresh`, how old a verdict may be and still be used, a duration such as `30 min`." and "A verdict too old for its `fresh` bound is refused as `temporal`". DESIGN.md §5.5: "Evaluators are consulted **before** the write transaction opens, never inside it, and their verdict carries an as-of time recorded on the event; a declaration may set a freshness bound, past which the verdict is refused as `temporal` (ADR-0049)." `customers.yaml`, `invoice_valid_in_xero`: "The order's invoice is valid in Xero, as Xero said within the last half hour."
- **Severity:** changes-outcome
- **Suggested fix:** State who sets a verdict's as-of time, whether one verdict may serve more than one request, and what an absent `fresh` means. Add a stale stub to §11.3.

#### 13. Which evaluator calls a request makes
- **Construct:** consultation of an evaluator; an absent argument; guards of transitions a request may reach
- **The readings:** It is determined that a request consults everything first (DESIGN.md §6). Two things are open. (i) An absent argument: (a) the call is made with no value, or (b) no call is made and the guard is unknown, with `temporal` or with the condition's remedy. (ii) A guard of a transition the request reaches only through a `call`, a `create`, a cascade or a loop: (a) it is consulted if the transition is reachable at all, or (b) only if the effect, run over committed state, would reach it. Take a customers example that requests `fulfil` on an order with no invoice number, expects `unsatisfied invoiced`, and stubs no evaluator. It passes under (i)(b) and fails under (i)(a), since "A call with no stubbed answer fails the example". A live engine likewise sends Xero an empty invoice number or does not.
- **Where the text fails:** `flow-format.md` §4.18: "A condition that asks an evaluator is consulted when its transition is requested, before the transaction, and by no read". DESIGN.md §6: "External evaluators and metrics named by any guard in the request are consulted **first, outside the transaction**, and their verdicts or values carried in with their as-of times (ADR-0049, ADR-0084)." `flow-format.md` §11.3: "A call with no stubbed answer fails the example."
- **Severity:** changes-outcome
- **Suggested fix:** State what a call with an absent argument does, and which reached transitions' evaluator guards a request consults.

#### 14. `fresh` refuses `1 day`
- **Construct:** the duration of `fresh`
- **The readings:** (a) Following the schema, only `s`, `min`, `h`, `days` and `weeks` are allowed, so `fresh: 1 day` is refused at step 2. (b) `fresh` takes the duration every other key takes, with §8.3's units, so `1 day` is valid. `max_recording_delay: 1 day` and `backdating_limit: 1 day` both pass the schema.
- **Where the text fails:** `flow.schema.json`, `evaluatorFunction.fresh`: `"pattern": "^[0-9]+ (s|min|h|days|weeks)$"`. `definitions.duration`: `"pattern": "^[0-9]+ (s|min|h|days?|weeks?)$"`. `declaration-syntax.md` §8.3: "A duration literal is a number and one of `s`, `min`, `h`, `day` or `days`, `week` or `weeks`". `flow-format.md` §4.18 calls `fresh` "a duration such as `30 min`".
- **Severity:** changes-outcome
- **Suggested fix:** Point `fresh` at `#/definitions/duration`.

#### 15. What `optional`, `default` and `personal` mean on an evaluator argument
- **Construct:** `functions.<f>.arguments`
- **The readings:** Arguments are declared "each as a declared input is", so the schema admits `optional`, `default` and `personal` on them. (a) An optional argument may be left out of a call, and a default fills it. (b) Every call passes every argument, as "passes as many arguments as it takes" says, and the markings mean nothing. A call passing one argument to a function that declares two, the second optional, is valid under (a) and refused at step 3 under (b). Nothing says whether a `personal` argument is sent to the other system, or withheld from the attempt log.
- **Where the text fails:** `flow-format.md` §4.18: "its `arguments` in the order a call passes them, each as a declared input is (§4.8)" and "The call names a declared function and passes as many arguments as it takes (step 3, `names`; the model's check 19)." In `flow.schema.json`, `evaluatorFunction.arguments` refers to `#/definitions/input`.
- **Severity:** changes-outcome
- **Suggested fix:** Allow only `type` or `reference`, and `description`, on an argument, or define the other keys there.

#### 16. What `transition_count` counts
- **Construct:** `measure: transition_count` with `transition`
- **The readings:** (a) The events of the named transition, "the number of times `transition` is taken". (b) Every event along the transition's source and target states, as §4.9's rule and the §6 conversion say, whatever transition recorded it. That includes an assertion between the same states, an erasure or migration event at an internal transition's states, and an earlier version's transition of another name. The step-3 rule shifts with it, since an assertion may or may not count as "another" transition sharing a source and target. Take a customers metric `{ measure: transition_count, transition: activate }`. (a) counts `activate`. (b) also counts every `correct_state` from PROSPECT to ACTIVE, or is refused at step 3 because `correct_state` shares PROSPECT→ACTIVE. Under (b), an inventory `transition_count` of `regrade` would count every migration or erasure event on an AVAILABLE or RESERVED item.
- **Where the text fails:** `flow-format.md` §4.9: "`median_time_in_state`, the median time objects spend in `state`; or `transition_count`, the number of times `transition` is taken" and "A `transition_count` counts the transitions along its transition's source and target states, so it MUST NOT name a transition that shares a source and target with another." §6: "a metric over the type's transitions along the transition's states, valued `count()`".
- **Severity:** changes-value
- **Suggested fix:** Convert to `where t.transition == "<name>"`, or state which kinds (assertion, erasure, migration) are counted and what "another" covers.

#### 17. Which entries into a final state must be covered
- **Construct:** `cascade` and `survives` coverage, including `inherited_parts`; an assertion or initial transition into a final state
- **The readings:** (a) As `flow-format.md` says, every transition into a final state must be named in a cascade or in `survives`. That includes an assertion whose `to` lists a final state, and an initial transition into one. (b) As the model says, only a `do` into a terminal state. Add `CLOSED` to customers' `correct_state` `to`. A `flow-format.md` checker refuses the description, because contacts cascade on `[close]` and survive only `[merged_into]`. The model's check 37 accepts it, and an override to CLOSED then leaves the contacts LISTED under a closed customer, where nothing can discard them.
- **Where the text fails:** `flow-format.md` §4.3: "Every transition of the whole into a final state MUST be covered by a cascade or by `survives`, and none by both (step 3, `names`; the model's check 37, ADR-0058)." §4.14: "every transition of the subtype into a final state is cascaded on or survived, never both". `declaration-syntax.md` §4.2: "A **terminal transition** is a `do` whose to-state is terminal; a creation into a terminal state is not one." Check 37: "a terminal transition of the whole covered by neither a `cascade on` clause nor `survives`, or covered by both".
- **Severity:** changes-outcome
- **Suggested fix:** Name in §4.3 which transition kinds must be covered, and make check 37 agree.

#### 18. Which invariants an erasure admits and records
- **Construct:** `kind: erasure`; the transition row members `.overrides` and `.reason`
- **The readings:** (a) An erasure records an admission for every invariant that reads an attribute it erased, broken or not. (b) It records one only for the invariants it breaks, as a declared admission is recorded only where the request breaks the invariant. This moves customers' own `overrides` metric (`filter: t.overrides`, by `t.reason`). In `erasing_follows_a_merge`, Dana and her duplicate are both PROSPECTs, so `active_invariant`, which reads `email`, still holds after the erasure. (a) records two admissions and the metric counts 2 under reason `erasure`; (b) counts 0.
- **Where the text fails:** `flow-format.md` §4.13: "It admits every invariant that reads what it erases" and, of assertions, "one the request does not in fact break records no admission". `declaration-syntax.md` §6.4: "So an `erase` carries an automatic admission for every invariant reading an attribute it erased, recorded on the event with the reason, exactly as a declared admission is." `declaration-syntax.md` §6.9: "`.overrides` is true on an override's event: an assertion's, and any event that records an admission — an assertion's, an erasure's, or a publish's `admit` (ADR-0105);"
- **Severity:** changes-value
- **Suggested fix:** State that an erasure records an admission only for an invariant it leaves violated, or for every one it reads. Separately, the example metric's description should say that it counts erasures and migration admissions too.

#### 19. What inputs a chain erasure passes
- **Construct:** `supersede`; an erasure following the supersession chain
- **The readings:** Erasing one member runs every other member's own erasure in one request, but nothing says which inputs those erasures receive. (a) The requested erasure's inputs are passed by name, and a member whose erasure requires another input refuses the whole request as `invalid_input`. (b) Each member's erasure runs with a fixed reason, such as the fixed `.reason` value `erasure`, or with none. In `erasing_follows_a_merge` the duplicate's `forget` requires `reason`: (a) records "asked"; (b) records something else, or refuses the chain for the missing input. Across types, as with a work item superseded by one of another type (§4.19), the erasures' declared inputs need not match at all.
- **Where the text fails:** `flow-format.md` §4.19: "Erasure follows the chain, through each member's own erasure, which §4.13 already requires of every type holding personal data (step 3, `names`; the model's check 60)." `declaration-syntax.md` §6.4: "erasing any member erases every predecessor and successor, each through its own type's `erase`, in one request."
- **Severity:** changes-outcome
- **Suggested fix:** State that each member's erasure receives the requested erasure's inputs by name, and refuse at step 3 a supersession whose members' erasures take different required inputs.

#### 20. What filing a proposal checks
- **Construct:** `proposable`; the examples' `propose` step
- **The readings:** (a) Filing records the request as given and checks only that the transition is proposable; everything else waits for approval. (b) Filing runs the request's checks up to its guards (existence, `unknown transition`, `unavailable`, `invalid input`), or its guards too, and refuses a proposal the request would be refused for. Consider a `propose` of a transition from a state the object is not in. (a) files it, so the setup step applies. (b) refuses it, which fails every example given that setup.
- **Where the text fails:** `flow-format.md` §4.8: "`true` if a request for the transition may be filed as a proposal instead, executed as its approver when approved (ADR-0036)". §11.3: "A `propose` files a request for approval instead of executing it, as a caller does on a transition marked `proposable` (§4.8, `DESIGN.md` §9)" and "Its transition MUST be proposable (step 3, `names`)". DESIGN.md §9: "Any caller may file one on a transition marked `proposable`".
- **Severity:** changes-outcome
- **Suggested fix:** State what filing validates, and the verdict of a refused filing.

#### 21. Uniqueness over absent values
- **Construct:** `unique: true` or `unique: { with: […] }` on an optional attribute, or with an optional companion
- **The readings:** (a) An absent value is not a value, so any number of objects may lack one. Three-valued logic makes an invariant comparing two absent values unknown, and so holding, and a database unique index behaves the same way by default. (b) Absence is a value, so at most one object may lack it, or one per combination for `with`. The format states partiality for external identifiers only. Give customers `email: { type: string, optional: true, personal: true, unique: true }` and register a second prospect without an email: it is applied under (a) and refused `invariant_violated email_unique` under (b).
- **Where the text fails:** `flow-format.md` §4.12 heads its table "No two objects hold the same value", with the row "`unique: true` | across the type". Of external identifiers only, §4.15 says "its values are unique for each source and it MAY be optional", and `declaration-syntax.md` §3.1 says "uniqueness per source is automatic and partial over absent values".
- **Severity:** changes-outcome
- **Suggested fix:** State in §4.12 whether every form of uniqueness is partial over absent values.

#### 22. Uniqueness of an attribute inherited from an abstract base
- **Construct:** `unique` on a base's attribute; `extends`
- **The readings:** (a) The attribute is unique across the family, the base's type scan ranging over every subtype. (b) It is unique within each concrete type. Give §4.14's `Asset` `name: { type: string, unique: true }`, and name a Laptop and a hypothetical Monitor both "HQ-01". (a) refuses the second `invariant_violated name_unique`; (b) applies both.
- **Where the text fails:** `flow-format.md` §4.14: "A type that declares `extends: <Base>` inherits the base's attributes, derived attributes and invariants, and its `tracking` if it declares none, and nothing else". §4.12, `unique: true`: "across the type", without saying which type for an inherited attribute. §4.12 also says "Each is enforced by a unique index on every backend (`storage-schema.md` §8)", a document outside my reading.
- **Severity:** changes-outcome
- **Suggested fix:** State in §4.14 whether an inherited `unique` spans the family.

#### 23. `range` over a value that is not a positive integer
- **Construct:** `foreach` with `range`
- **The readings:** Nothing covers a range value that is absent, zero or negative, or not an integer. (a) An absent or non-positive value runs no step, and a decimal is truncated. (b) Absence refuses the request, under a verdict nobody names, and a non-integer is refused at step 3. With `range: inputs.copies` where `copies` is `decimal(4,1)`, a value of 2.5 runs twice, runs three times, or is refused.
- **Where the text fails:** `flow-format.md` §4.8: "or, with `range: <expression>` in place of `array`, for each number from 1 to that value (SCXML's `<foreach>`)". No type is required of the expression, and nothing says what an absent one does.
- **Severity:** changes-outcome
- **Suggested fix:** Require an `int` range at step 3, and state that an absent or non-positive value runs no step, or name the verdict that refuses it.

#### 24. The scope and precedence of a `create`'s `result`
- **Construct:** `create.result`
- **The readings:** Name resolution puts a loop binder ahead of the object's own attributes, but a `result` is not a loop binder. (a) A `result` shadows an attribute of the same name in the later steps, as a binder does. (b) The attribute wins. (c) Step 3 refuses the collision. Whether a `result` bound inside a `foreach` is visible after the loop is also not said. After `create: { …, result: engineer }` on ServiceJob, a later step reading `engineer` gets the new object under (a) and the job's engineer under (b).
- **Where the text fails:** `flow-format.md` §4.8: "`result` names the new object for the steps after it (UML's `CreateObjectAction`)". §5: "and in an effect, also the name a `foreach` or `create` step binds". `declaration-syntax.md` §9.2: "A bare name in an expression resolves in this order, the first match winning:", "1. a loop binder in scope;" and "3. `state`, or an attribute, counter, derivation or relationship end of `this`, inherited ones included;".
- **Severity:** changes-outcome
- **Suggested fix:** Refuse a `result` or `item` that shares a name with a member, and state a `result`'s scope.

#### 25. `summary` on an abstract base
- **Construct:** `abstract: true` together with `summary`
- **The readings:** (a) Following the schema, whose abstract branch excludes only states, transitions, `state_machine`, conditions, observations, metrics and `mirror`, `summary` is valid, and by "nothing else" it is not inherited. (b) The text lists what an abstract type "may declare", a list that leaves out `summary`, and attributes the rule to step 2, so the description is refused.
- **Where the text fails:** `flow-format.md` §4.14: "It declares `description`, and may declare `tracking`, `attributes`, `derived_attributes` and `invariants`, and nothing of a lifecycle: no states, transitions, `state_machine`, conditions, observations or metrics (step 2, `schema`)." In `flow.schema.json`, the third `oneOf` branch of `type` sets `not.anyOf` to `state_machine`, `states`, `transitions`, `conditions`, `observations`, `metrics` and `mirror`.
- **Severity:** changes-outcome
- **Suggested fix:** Either exclude `summary` in the schema's abstract branch, or list it in §4.14 and say whether subtypes inherit it.

#### 26. What `cascaded` lists, and how it is compared
- **Construct:** the examples' `expect.cascaded`
- **The readings:** The key names "the transitions the request's cascades took". Yet `erasing_reaches_the_contacts` expects `[Contact.forget]`, which a `call` inside `forget`'s `foreach` takes, and `erasing_follows_a_merge` expects `[Customer.forget]`, which the supersession chain takes. "Cascades" therefore means every caused transition, as DESIGN.md §6 uses "cascaded transition". Four questions remain open: whether creations are listed (for `add_contact`, `Contact.add`), whether a transition taken on two parts appears once or twice, whether order matters, and whether the list must be complete. Give Dana two contacts in `erasing_reaches_the_contacts`. A runner comparing sequences then needs `[Contact.forget, Contact.forget]`, while one comparing sets passes `[Contact.forget]`.
- **Where the text fails:** `flow-format.md` §11.4, `cascaded`: "for `applied`, optional: the transitions the request's cascades took, each as `<Type>.<transition>`". By contrast `warned` says "in the order they were decided" and "an empty list says none did".
- **Severity:** changes-outcome
- **Suggested fix:** Define `cascaded` as the exact sequence, in execution order, of the transitions the request causes on other objects: calls, creations, cascades and chain erasures.

#### 27. Which objects a generated guard's refusal names
- **Construct:** the objects of an `unsatisfied` verdict from `subject_open`, `corrects_current`, `successor_not_superseded`, `whole_open`, `not_erased` or `occurred_within`
- **The readings:** The rule computes the objects from the references "in the failing clause", and a generated guard has no written clause. `subject_open` refusing a recording on a DONE job names `[job]`, since the subject is another object than the new observation, or names nothing. `corrects_current` in `a_result_is_corrected_once` names `[mistake]`, possibly with its correction, or nothing. `successor_not_superseded` refusing an already superseded successor names that successor or nothing.
- **Where the text fails:** DESIGN.md §5.5 covers one generated guard: "The generated `actor_known` names none, since what it finds missing is an object (ADR-0122)". `flow-format.md` §6 gives an expression only for `<attribute>_provided` and `<state>_invariant`.
- **Severity:** advice-only
- **Suggested fix:** Give each generated guard's expression in §6, or state its objects in DESIGN.md §5.5.

#### 28. The objects of a clause with a nested aggregate
- **Construct:** `no_unresolved_failure` on service's `finish`
- **The readings:** In `a_failed_check_holds_the_job_open`, ids follow creation: ana, ben, battery, job, and then the FAIL result `f`. (a) If only the outer collection's selected elements count, the list is `[f]`. (b) If every reference the `where` evaluates counts too, `r.check` and `s.check` included (compared, never read), it is `[battery, f]`. With more data the inner aggregate matters. Suppose checks `battery` and `fan`, a FAIL `f1` and a later PASS `p1` on battery, and a FAIL `f2` on fan. (a) gives `[f2]`. (b) gives `[battery, fan, f2]`. (c) If the inner `none` also contributes what it selects while the outer `where` is evaluated "in full" for elements the outer `where` then rejects, the list is `[battery, fan, p1, f2]`. Whether a bound name such as `r` or `s` counts as "a reference in the failing clause" is also open, and if it does, every inspection is named.
- **Where the text fails:** DESIGN.md §5.5: "The objects are every object a reference in the failing clause evaluates to when the clause is evaluated in full, every operand of `and`, `or` and `implies` and the derived attributes it reads included, whether the clause reads a member of the object or only compares the reference, other than the object it is evaluated on; where it evaluates a collection, the elements its `where` selects, or every element where it has none, so an empty collection contributes none; in ascending id order."
- **Severity:** advice-only
- **Suggested fix:** State whether only the outermost aggregate's selected elements and the references outside any aggregate are named, or give the rule for nested aggregates and bound names.

#### 29. The objects of a duplicate's `invariant violated`
- **Construct:** `unique`, and so `serial_unique`, in `a_serial_is_in_stock_once`
- **The readings:** (a) If only the written object is checked, the verdict names `robot2`, failing `serial_unique`, with conflicting object `robot`. (b) If every object whose invariant the write could break is checked, the existing holder is re-checked as a type scan is. The verdict then names `robot` (conflicting `robot2`) and `robot2` (conflicting `robot`), both failing `serial_unique`. (c) If the index refuses and no clause is evaluated, there are no conflicting objects. The clause and remedy (`serial_unique`, `self_serviceable`) are the same under all three, so the example passes whichever reading an engine takes. The list differs, and the invariant that `unique` stands for is never written, so the conflicting objects have no clause to be chosen from.
- **Where the text fails:** `flow-format.md` §4.12: "`unique` takes one of four forms, each the model's sugar for an invariant (`declaration-syntax.md` §3.1)." §4.8: "and last, invariant violated, naming each object on which an invariant fails, in ascending id order". DESIGN.md §5.5: "Names each object on which an invariant fails, in ascending id order, every invariant that fails on it, in the order its type declares them, and the conflicting objects, chosen as an unsatisfied clause's are, the object the invariant fails on excluded (ADR-0122)".
- **Severity:** advice-only
- **Suggested fix:** Give in §6 the invariant each `unique` form expands to, and say that a type scan's other holders are re-checked.

#### 30. Where generated invariants sit in "the order its type declares them"
- **Construct:** `<state>_invariant`, `<attribute>_unique`, and inherited and machine-required invariants, among a type's `invariants`
- **The readings:** The refusal's clause and remedy come from the first failing invariant, in declared order. But generated invariants are not declared under `invariants`, and inherited ones are declared on the base. Generated invariants could come (a) before the declared ones, (b) after them, or (c) in some other order, such as by state and then by attribute. Give customers a declared invariant `active_needs_xero: state != ACTIVE or xero_contact_id is not null`. `correct_state` to ACTIVE on a prospect with neither email nor Xero contact breaks both `active_invariant` and `active_needs_xero`. The refusal's clause is `active_invariant` under (a) and `active_needs_xero` under (b), so an example's `clause` passes under only one.
- **Where the text fails:** `flow-format.md` §4.8: "naming each object on which an invariant fails, in ascending id order, and on each every invariant that fails, in the order its type declares them, each with the remedy `DESIGN.md` §5.5 infers for it, the first of which is the refusal's (ADR-0122)."
- **Severity:** changes-value
- **Suggested fix:** State where generated, inherited and machine-required invariants fall in that order.

### Where I left `flow-format.md`

- `declaration-syntax.md` §6.8, for recording: the `record` creation's `corrects` input, the state `RECORDED`, the four recording guards, the meaning of `occurred within`, "recording one re-checks nothing else", and `unit` as a printed label. `flow-format.md` states none of these.
- §6.6 and checks 22 and 23, for migration: final objects included, backfills as `migrated` events, and what refuses a publish.
- §6.4, for erasure: chain erasure, `not_erased`, and automatic admissions.
- §6.1 to §6.3, for supersession, evaluator arguments resolved twice, and admissions.
- §2, §3.1, §3.2 and §3.4, for families, `unique`, cascades and when an invariant is re-checked.
- §4.2, for `occurred_at` as a request field and for "terminal transition".
- §5.1 and §5.2, for remedies, skip rules and loops.
- §6.9, for the members of transition rows (`.overrides`, `.reason`, `.migrated`), span durations, and `fresh` on a metric.
- §8, §9.2 and §10, for three-valued logic, name resolution and the text of the checks.
- DESIGN.md §5.5, for the objects a refusal names, the generated guards' remedies and the evaluator's as-of time. `flow-format.md` never says a refusal names objects.
- DESIGN.md §6, for the order of execution, standing admissions, what `whole_open` is (`flow-format.md` names it and never defines it), and a migration as steps 5 to 8.
- DESIGN.md §9, for proposals and their invalidation.
- DESIGN.md §10, for reads not consulting evaluators.

### Count

30 findings: **changes-outcome 19** (1, 3, 5, 7, 8, 10, 12, 13, 14, 15, 17, 19, 20, 21, 22, 23, 24, 25, 26), **changes-value 7** (2, 4, 6, 9, 16, 18, 30), **advice-only 3** (27, 28, 29), **wording 1** (11).

## 2. Metrics, as the reader wrote it

**Question.** Could two careful builders, each following this text, compute different values for the same metric over the same history?

**Read.** `DESIGN.md` §5.12, §5.13 and §7. `design/declaration-syntax.md` §6.9, §6.10, §6.11 and §8. `design/flow-format.md` §4.9, and §5, because §4.9 defers to it (“as §5 says”). `design/metric-scenarios.md` in full, for its worked values only. My read of `DESIGN.md` §5.12–§7 also displayed §6, but no finding relies on it. I did not read ADRs, `defects.md`, `TODO.md`, `LESSONS.md`, reviews, case studies or code. Every quotation below is delimited by curly quotes and was checked verbatim against the files by a script.

**Method.** I computed every metric read in `metric-scenarios.md` from the specification, and made up small histories for the edges. Durations and calendar facts were checked with Python (`datetime`, `decimal`). No PostgreSQL is installed here, so I did not probe PostgreSQL's `percentile_disc`: what finding 11 says about it is from memory of its source and is marked as such.

### What is determined

Each line gives a case, the value I computed, and the text that fixes it. Where a line matches a scenario's stated value, it says so.

1. `metric(Delivery.time_in_state, keep: [state])` at 2026-09-28, PREPARATION: **3d 12h**. Eight spans; the nearest rank is ⌈0.5 × 8⌉ = 4. Matches scenarios §3.2.
2. `Delivery.time_in_state_p80`, same eight spans: **27d**. 0.8 × 8 = 6.4, so the rank is 7.
3. `Delivery.time_in_state`, CANCELLED (one current span in a closed state): **absent**. The group is still reported, not dropped. DELIVERED: **6d**, from D03's ended stay alone. Matches §3.2.
4. `delivery_wait`, C-BOREAL / M-ROVER: **1d 1h**. D02's span counts once, although two Rover units reach M-ROVER. Matches §3.5.
5. `Delivery.oldest_open`, PREPARATION: **44d**, with gap D08. `Delivery.work_in_progress`: **4**, with no gap. Matches §3.3.
6. `Delivery.refusals`, filled / dependent / agent over all weeks: **6**. Sunday 20 September is 2026-W38, and the 21st starts W39. Matches §4.
7. `Robot.override_counts`, AVAILABLE / SUPPLIER_EXCEPTION: **4**. The imports of R01–R03 are excluded. Matches §5.
8. `Robot.time_in_state`, PROCUREMENT: **4d 13h**. The backdated receipt ends R30's stay at its occurred time, and the median of four is the lower middle. Matches §11.
9. `supplier_lead_time_p80`, Armtek, over the last 365 days at 2026-11-02: **42d** (⌈2.4⌉ = rank 3). Roverline: **35d** (⌈4.8⌉ = rank 5). Matches §16.
10. `metric(first_pass_rate, model := M-ARM, over last 30 days)` at 2026-11-07T00:00Z: **0.750**. The window is [2026-10-08T00:00Z, now], so Q1, Q2, Q6 and Q7 are in it and D09's results are out. Matches §19.
11. `Delivery.refusal_rate` bound to `complete_sale`, `keep: [actor_kind]`: human **0.333** (4 of 12), agent **1.000** (7 of 7). Both inputs are aggregated before combining. Matches §30. This read drops `clause`, so finding 1 does not touch it.
12. `ServiceJob.acted_by_non_assignee.engineer`: reassign **1.000**, start **0.000**, finish **0.500**. Creations are excluded, because their `.held` is absent. Matches §31.
13. `ServiceJob.cycle_time_by_assignee.engineer`, `keep: []` at 2027-04-19: **23d**, the third of 3d, 8d, 23d, 89d and 89d 23h. Matches §43.
14. `time_to_done` by version: v6 **9d**, v9 **3d**, v10 **3d**. Migrated closes are excluded by the metric's own filter. Matches §27.
15. `first_pass_yield`, M-ROVER, 2027-01: **0.000**, from (1 − 1) × 1.000 / 1. Matches §32. So is every other scenario value I recomputed, with one exception in the `gaps` column (finding 4).
16. A metric windowed on a `date`, read at 2026-09-28T10:00Z `over last 1 day`: the date 2026-09-28 is **in**, and 2026-09-27 is **out**, because it is placed at 00:00Z, before the window opens at 09-27T10:00Z.
17. `week(2027-01-01T05:00Z)` is **2026-W53**, while `month` of the same instant is 2027-01. This follows from ISO 8601; 2026 has 53 ISO weeks.
18. Splitting by month a span from 2026-09-20T00:00Z to 2026-10-10T00:00Z gives two pieces, **11d** and **9d**. Without `period` in `by`, `count()` is **2**, because the rows are pieces.
19. `1 * 1.000 / 16` as a metric value: **0.062** (half to even at scale 3). `avg` of the ints 1 and 2: **1.500000**.
20. A guard's `metric(throughput, state := X) >= 1` with no rows is **false**, not unknown, because `count()` over no rows is 0. With `median`, the value is absent and the guard is **unsatisfied, unknown**.
21. `yield_or_signed` on a model with a floor and no first results in the window: 0/0 makes the metric **unknown**, so the guard is unknown and the request is **refused** unless a sign-off exists.
22. A guard binding `engineer := engineer` on a job with no engineer: the metric is **unknown**. It does not read the absent-engineer group.
23. A creation straight into a closed state: `.completes` is **false**, since the transition is not “from an open state”. It is **not** in `throughput`.
24. Determined, but probably not what is meant:
    - `handoffs_by_object` is **−1** for a job never assigned, because its source has no `any(…)` filter while `handoffs` has one.
    - An assignee going A → none → A gives `handoffs` **1** and `returns_by_object` **1**, although only one engineer ever held the job. The scenario's prose, “Handoffs are the engineers a job has had, less one”, would give 0.

### Findings

#### 1. A combined metric whose input lacks a dimension the combination names (`refusal_rate` and `clause`)
- **Construct:** `combine`, and specifically the standard `refusal_rate`. Its `by` names `clause`, which `transition_counts` does not have.
- **Case:** the scenario history at 2026-09-28, read as `metric(Delivery.refusal_rate, keep: [transition, clause, actor_kind])`, for `complete_sale` / human.
  - Refused: `not_internal` 1, and one with no clause (`unavailable`) 1.
  - Applied: D01, D02 and D03, all by people, 3.
- **The readings:**
  - (a) Following flow-format §4.9, `applied` is contributed to every clause group. `not_internal` / human is 1/(1+3) = **0.250**, and the absent-clause row is **0.250**.
  - (b) Following declaration-syntax §6.9, `applied`'s rows have no clause, so they are “counted under the absent value”. In the `not_internal` group, `applied` “has no rows” and contributes 0. `not_internal` / human is 1/(1+0) = **1.000**, and the absent-clause row is **0.250**.
  - Even under (a), a group where only `applied` has rows (a transition never refused) has no clause value to be broadcast to. It either gives a row with an absent clause and 0.000, or no row at all.
- **Where the text fails:**
  - declaration-syntax §6.9: “A `combine` aggregates each input over every dimension its `by` does not name before combining them, and keeps `version` and `actor_kind` only when named; a row whose dimension has no value is counted under the absent value, never dropped; the inputs are joined over every group any of them has, an input with no rows in a group contributing its value over no rows (ADR-0122).”
  - flow-format §4.9: “`group_by` names dimensions its inputs have, or `version` and `actor_kind`, which every metric has; an input without one of them contributes its value to every group that differs only in that dimension.”
  - The model's text never states the broadcast. Its standard `refusal_rate` depends on it, and its own join rule gives (b).
- **Severity:** changes-value, in a standard metric every type has.
- **Suggested fix:** State the broadcast in declaration-syntax §6.9, and say what row a group that only the broadcast input has produces.

#### 2. A refusal by a called or cascaded clause is a row of which type?
- **Construct:** the rows of `<Type>.attempts` and `attempt_counts`, and so `refusals`, `flags_raised` and `refusal_rate`.
- **Case:** the scenario history of §8.
  - R33's receipt through `S1.receive_unit` fails the `occurred_within` of its call to `Robot.receive`.
  - R32's own `receive_unit` fails its own `occurred_within`.
  - The reads are `metric(Shipment.refusals, keep: [transition, clause, remedy])` and `Robot.refusals`.
- **The readings:**
  - (a) Following declaration-syntax §6.9, R33's row is Robot's: `receive`, R33, `occurred_within`. `Shipment.refusals` is **1**, and `Robot.refusals` has receive / occurred_within **1**.
  - (b) Following the scenario (§11, §12 finding 2, §35), the row is the requested transition's, with the call's clause. `Shipment.refusals` is **2**, and Robot has none.
  - D06's `complete_sale`, refused by R20's `sell` guard `mfr_serial` (§38), splits the same way: a `Delivery.refusals` row under (b), a `Robot.refusals` row under (a).
  - Nothing says which type a refusal with no clause belongs to when an invariant of another written object is what refused.
- **Where the text fails:**
  - declaration-syntax §6.9: “A row belongs to the type whose clause failed, and its `.object`, `.transition` and `.clause` are that transition's, so a flag a part's creation raises inside its whole's request is a row of the part's type (DESIGN.md §5.5)”.
  - metric-scenarios §12 calls the opposite determined: “A refused call's attempt counts under the requested transition, with the call's clause** (determined).”
  - The scenario's stated value disagrees with the specification's sentence.
- **Severity:** changes-value.
- **Suggested fix:** Say whether an enforced refusal's row belongs to the requested transition or to the failing clause's transition (the sentence may mean flags only), and make §11 and §35 agree with it.

#### 3. `.held(member)` on a span that the member-writing transition itself begins
- **Construct:** `.held` on interval rows, and so the standard `time_in_state_by_holder`.
- **Case:** the scenario history of §29 at 2027-01-25, read as `time_in_state_by_holder.engineer, keep: [state, assignee]`, state OPEN. Each of J1–J4's OPEN spans begins at the creation that writes `engineer`: J1 1d (Ben), J2 6d 23h (Chen), J3 11d 14h (Ben), J4 10d 14h (Diya). SJ1 is 154d, with no engineer at its start.
- **The readings:**
  - (a) `.held` is the value in force during the span, after the writes. OPEN / U-BEN is median(1d, 11d 14h) = **1d**, OPEN / U-CHEN **6d 23h**, OPEN / U-DIYA **10d 14h**, OPEN / absent **154d**.
  - (b) `.held` is the value before the writes, and “a creation's is absent” applies to the span a creation begins. All five spans fall under absent, median **10d 14h**, and there is no OPEN row for any engineer.
  - Any transition that changes state and writes the assignee in one step splits the same way.
- **Where the text fails:**
  - declaration-syntax §6.9: “`.held(<member>)` is the value a tracked member held when the span began, or, on a transition, before the transition's own writes, so time in a state is attributed to whoever was assigned then, not to whoever is now, and a reassignment to whoever it took the object from; a creation's is absent”.
  - DESIGN §5.13: “The assignee a transition row is attributed to is the one who held the object before the transition's own writes”.
- **Severity:** changes-value, in a standard assignment metric.
- **Suggested fix:** Say that on a span, `.held` is the value after the writes of the transition that began it (or before), and that “a creation's is absent” is the transition row's only.

#### 4. `gaps` under a window: any object created inside it, or only an object whose record is incomplete?
- **Construct:** `gaps`, the first listed cause.
- **Case:** scenarios §16, `metric(supplier_lead_time, keep: [supplier], over: "365 days")` at 2026-11-02. The window starts 2025-11-02. L02, L03 and L04 (`recorded_from` their legacy creation, 2026-01-12, 03-02 and 06-01) and R30–R33 (created 2026-10-01) all have `recorded_from` after the window starts.
- **The readings:**
  - (a) Literal: Armtek's gaps are {L02, L05}, and Roverline's are {L03, L04, R30, R31, R32, R33}. Every object created inside any window would be a gap.
  - (b) Following the scenario: Armtek {L05}, Roverline none.
  - Split variant: D08's span began on 15 Aug, before its `recorded_from` of 28 Aug. Split by month, is D08 a gap in the September row, whose piece starts on 1 Sep? “an interval … that began before it” says yes. “read something from before” says no.
- **Where the text fails:**
  - DESIGN §5.12: “the objects for which it read something from before what their record vouches for: a window beginning before the object's `recorded_from`; an interval, legacy or current, that began before it”.
  - declaration-syntax §6.9: “a window beginning before the object's `.recorded_from`, an interval that began before it”.
  - The scenario disagrees with the literal list.
- **Severity:** changes-value, in the result's `gaps`. The figure itself is unaffected.
- **Suggested fix:** Count a window as a gap only where the object existed before its `recorded_from` (its known start precedes it), and say whether a piece inherits its span's gap.

#### 5. A current span that runs through a closed stretch
- **Construct:** `.duration` of a current span, for state or member.
- **Case:** a job WORKING (live) with engineer Ben from day 0. `finish` WORKING → DONE (closed, not terminal) on day 5. `archive` DONE → ARCHIVED (closed, terminal) on day 20. Read `time_with_assignee` for Ben on day 30.
- **The readings:**
  - (a) Literal: the span “began before the object entered that state”, ARCHIVED, so it runs to day 20: **20d**, 15 of them on a closed job.
  - (b) By the stated consequence, “nothing accrues on a finished object”: it stops at the first closed entry, **5d**.
  - A reopened job splits the same way: DONE on day 5, reopened to WORKING on day 10, open on day 12. (a) gives **12d**, and (b) gives **7d**.
  - “one” may also cover ended spans. Then a member span that began and ended after the object entered its current closed state (an `act` reassigning a closed job) has no duration rather than its own.
- **Where the text fails:**
  - declaration-syntax §6.9: “A span still current runs to `now` while its object is open. On an object now in a `closed` or terminal state, one that began before the object entered that state runs to that entry, and one that began after it, like a span in that state itself, has no duration; so nothing accrues on a finished object”.
  - DESIGN §5.12: “a span still open on an object in a `closed` or terminal state stops at its entry into that state, so an engineer still named on a closed job accrues no time”.
- **Severity:** changes-value, in `time_with_assignee`, `time_unassigned` and `time_in_state_by_holder`.
- **Suggested fix:** Say whether the stop is the entry into the current state or the start of the unbroken closed stretch, and whether earlier closed stretches are subtracted.

#### 6. Several aggregates over a group whose rows all lack a body
- **Construct:** a reported group whose rows all lack a body, under a value with more than one aggregate.
- **Case:** `from i in Job.intervals where i.state == DONE` (closed, terminal), three current DONE spans, none with a duration.
  - `value sum(i.duration) / count()`.
  - `value count(where i.duration > 1 day) * 1.000 / count()`.
- **The readings:**
  - (a) Each aggregate is evaluated normally. `sum` over no present bodies is 0 and `count()` is 3, giving **0 s** and **0.000**.
  - (b) The value is “the value its expression has over no rows”. `count()` is 0, so both are 0/0 and **unknown**. A guard `metric(…) < 1 day` is true under (a) and unsatisfied under (b).
  - Which body decides “all lack a body” is also undefined when the aggregates' bodies differ, and `count()` has none.
- **Where the text fails:**
  - declaration-syntax §6.9: “A group whose rows all lack a body is still reported, with the value its expression has over no rows, so a state objects sit in without accruing is told apart from one no object is in (ADR-0122, ADR-0123).”
  - flow-format §4.9: “a group whose rows all lack a body is reported, with the value its expression has over no rows (ADR-0123)”.
- **Severity:** changes-outcome.
- **Suggested fix:** Say that the group is reported and each aggregate is computed over the rows where its own body is present (no substitution of “no rows”).

#### 7. A flag whose condition is unknown
- **Construct:** `flag … when` (`flag_when`).
- **Case:** a metric by engineer with `value count(where r.outcome == CheckOutcome.FAIL) * 1.000 / count(where r.outcome != CheckOutcome.NOT_APPLICABLE)` and `flag high_fail when value > 0.100`. One engineer's rows are all NOT_APPLICABLE, so the value is 0/0, unknown. The same happens with `flag slow when value > 10 days` on a group reported with an absent median.
- **The readings:**
  - (a) Not raised, as a filter leaves out an unknown row.
  - (b) Raised, failing closed as flow-format §5 says a guard does.
  - (c) Reported as unknown, a third state.
- **Where the text fails:**
  - declaration-syntax §6.9: “**A flag** is a named condition over `value` and the dimensions, declared once so that a scheduler or an agent queries the flag rather than restating the threshold (PRD C2).”
  - §8.2's table lists a guard, an invariant, a derivation and a filter, but no flag.
  - flow-format §5: “a guard that is unknown fails as a false one does, so a `deny` guard refuses and an `audit` or `warn` guard is recorded or raised”.
- **Severity:** changes-outcome: what a scheduler querying the flag is told.
- **Suggested fix:** Add a row for a metric's flag to §8.2's table, and say what a reader sees.

#### 8. A flag on a read that keeps or binds fewer dimensions
- **Construct:** how finely a flag is evaluated.
- **Case:** scenarios §31 at 2027-01-25, `metric(ServiceJob.handoffs_by_object.engineer, keep: [])`. The value is SJ1 1 + J1 0 + J2 2 + J3 0 + J4 1 = 4, and the flag is `changed_hands_more_than_once when value > 1`.
- **The readings:**
  - (a) The flag is evaluated on each returned row. It is raised on the single row (4 > 1) and names no job.
  - (b) The flag is evaluated on the declared groups and reported whatever the read keeps: J2 alone.
  - A flag that reads a dimension the read dropped (flow-format lets it read the dimensions, `version` and `actor_kind`) is left with no value for it.
- **Where the text fails:**
  - declaration-syntax §6.9: “A reader keeps every declared dimension by default and gets one row per group, and may name the dimensions to keep with `keep`, aggregating over the rest (ADR-0101).”
  - declaration-syntax §6.11: “`handoffs_by_object` and `returns_by_object` name the objects, so `metric()` returns the jobs that changed hands more than once or went back to an earlier assignee, with the flag saying which (PRD UC-18).”
  - flow-format §4.9: “A flag reads `value`, the metric's dimensions, and `version` and `actor_kind`”.
- **Severity:** changes-outcome.
- **Suggested fix:** Say that flags are evaluated on the declared groups (or on the rows a read returns), and what a flag reading a dropped dimension gives.

#### 9. Split pieces at a window edge, and a span with no duration under `over last`
- **Construct:** `split by` with `over last`.
- **Case:** now is 2026-10-31T00:00Z. `over last 30 days` makes the window [2026-10-01T00:00Z, 2026-10-31T00:00Z]. The metric is `split by month`, `by period = i.period, state = i.state`, `value count()`.
  - Span A, WORKING, runs from 2026-09-20T00:00Z to 2026-10-01T00:00Z. It ends exactly on the month boundary, which is also the window's start.
  - Span B is a DONE stay entered at 2026-10-15T00:00Z on a closed job, with no duration.
- **The readings:**
  - Does A “cross” the 10-01 boundary and leave a zero-length October piece? Its September piece meets the window at one instant. Is that “no part in it”? September / WORKING is **0** (dropped) or **1** (duration 0), and October / WORKING is 0 or 1.
  - B is “one piece … with none”. Is a point inside the window kept (DONE / 2026-10 has count **1**) or left out as having no part (the group **vanishes**)?
  - A no-duration piece's `.piece_end` is not given.
- **Where the text fails:**
  - declaration-syntax §6.9: “the span cut at every bucket boundary it crosses”.
  - declaration-syntax §6.9: “a span with no duration is one piece, in the bucket it began, with none. `count()` counts a span once in each period it covers.”
  - declaration-syntax §6.9: “`over last` clips each piece to the window, leaving out one with no part in it.”
  - The window is “from `now` less the duration to `now`, both included”.
- **Severity:** changes-value.
- **Suggested fix:** Define pieces as half-open [start, end), and say whether a no-duration piece is kept when its start lies in the window.

#### 10. In a split metric, does the filter see a piece before or after the window clips it?
- **Construct:** the order of `where`, `over last` clipping and a reader's `filter` in a split metric.
- **Case:** `from i in Job.intervals split by month where i.duration >= 1 day`, `value sum(i.duration)`. The span runs from 2026-09-29T12:00Z to 2026-10-02T12:00Z, so its October piece is 1d 12h. Read at 2026-10-31T00:00Z `over last 29 days`, the window starts at 10-02T00:00Z and clips the piece to 12h.
- **The readings:**
  - (a) Filter, then clip: the piece passes on 1d 12h and is summed at **12h**.
  - (b) Clip, then filter: 12h < 1 day, so the result is **0**.
  - A reader's `filter` on `.duration`, `.piece_start` or `.piece_end` splits the same way.
- **Where the text fails:**
  - declaration-syntax §6.9: “`over last` clips each piece to the window”.
  - declaration-syntax §6.9: “A `filter` passed to `metric()` narrows the rows before they are aggregated.”
  - Neither passage orders filtering against clipping.
- **Severity:** changes-value.
- **Suggested fix:** Say that the window clips first, and that `.duration`, `.piece_start` and `.piece_end` read the clipped piece everywhere (or the reverse).

#### 11. Nearest rank: exact `p × n`, or PostgreSQL's `percentile_disc`?
- **Construct:** `percentile(p, …)`.
- **Case:** `percentile(0.55, x)` over x = 1…100.
- **The readings:**
  - (a) In exact decimal, 0.55 × 100 = 55, so rank 55 and the value **55**.
  - (b) `percentile_disc` takes a double-precision fraction, and 0.55 × 100 in IEEE double is 55.00000000000001 (checked in Python). PostgreSQL computes the row as ⌈fraction × rows⌉; this is from memory of `orderedsetaggs.c`, not probed here. That gives rank 56 and the value **56**.
  - `percentile(0.28, …)` over 25 rows splits the same way: rank 7 or 8.
  - p = 0.5, 0.8, 0.9, 0.95 and 0.99 agree for every n up to 200 000 (checked), so the standard metrics are unaffected.
  - A p outside [0, 1] is undefined: PostgreSQL raises an error, and the text is silent.
- **Where the text fails:**
  - declaration-syntax §6.9: “`percentile(p, …)` is **nearest-rank**: the smallest value whose rank in the group, ascending, is at least `p × n`, which PostgreSQL computes as `percentile_disc` and SQLite by row number, so both give the same value”.
- **Severity:** changes-value, for a declared metric with such a p.
- **Suggested fix:** Compute the rank exactly in decimal and pass it to both backends as an integer, and make a p outside [0, 1] a publish error.

#### 12. The fixed measure `transition_count`: by name or by state pair, and what it excludes
- **Construct:** the fixed measure `transition_count` in flow-format.
- **Case:** the scenario history at 2026-09-28. `Robot`'s `transition_count` of `inventorize` (INTAKE → AVAILABLE).
- **The readings:**
  - (a) By name: R10's `inventorize`, **1**.
  - (b) “along its transition's source and target states”: every event from INTAKE to AVAILABLE, the five `correct_state` overrides (R20, R21, R04, R22, R23) included, **6**.
  - Neither says whether imported, migrated or redacted events are excluded, as the standard `transition_counts` excludes them.
- **Where the text fails:**
  - flow-format §4.9: “`transition_count`, the number of times `transition` is taken”.
  - flow-format §4.9: “A `transition_count` counts the transitions along its transition's source and target states, so it MUST NOT name a transition that shares a source and target with another.”
- **Severity:** changes-value.
- **Suggested fix:** Say that it counts the named transition's events, with the standard metrics' exclusions (or list exactly which moves along the pair count).

#### 13. The fixed measure `median_time_in_state`: per span or per object?
- **Construct:** the fixed measure `median_time_in_state`.
- **Case:** one object has two WORKING stays, 2d and 4d. Another has one, 5d.
- **The readings:**
  - (a) Per span, as `time_working` is: median(2d, 4d, 5d) = **4d**.
  - (b) Per object, as “the median time objects spend” reads: median(6d, 5d) = **5d**.
- **Where the text fails:**
  - flow-format §4.9: “`median_time_in_state`, the median time objects spend in `state`”.
  - flow-format §4.9: “A fixed measure groups and windows as service's `time_working` does in the model”. This sentence points at `time_working` for grouping and windowing, not for the value.
- **Severity:** changes-value.
- **Suggested fix:** "the median `.duration` over the spans in `state`".

#### 14. A set-valued dimension where some members reach no value, or two set-valued dimensions
- **Construct:** a set-valued dimension.
- **Case:** `configuration = i.object.units.model`, where `model` is optional. A delivery has units R1 (M-ARM) and R2 (no model), and a span of 3d.
  - A second metric has two set-valued dimensions, `units.model` and `units.site`, with R1 (M-ARM, S1) and R2 (M-ROVER, S2).
- **The readings:**
  - (a) The span counts under M-ARM only. (b) It also counts under the absent value.
  - With two set-valued dimensions: (a) the cross product, including (M-ARM, S2). (b) Only the pairs one member reaches.
- **Where the text fails:**
  - declaration-syntax §6.9: “**A set-valued dimension** — a path through a set, such as a delivery's configurations — counts the row once under each distinct value its path reaches: once under each member where the path ends at the set, and once under a value two members reach where it goes past it”.
  - declaration-syntax §6.9: “a row whose dimension has no value is counted under the absent value, never dropped”.
- **Severity:** changes-value.
- **Suggested fix:** Say whether an absent value is among the "values reached", and whether several set-valued dimensions are crossed or taken per member.

#### 15. A reader's `filter` on a combined metric
- **Construct:** `metric(…, filter: …)` on a `combine`.
- **Case:** an application scopes its reader to Acme's deliveries and reads `Delivery.refusal_rate`. Its inputs bind `a` (over `attempt_counts`) and `t` (over `transitions`), and a refused creation has no `.object`.
- **The readings:**
  - (a) The filter is refused, because there is no binder.
  - (b) It applies to the input whose binder it names, which gives a rate with a scoped numerator over an unscoped denominator, or the reverse.
  - (c) It is re-bound by member name to each input.
- **Where the text fails:**
  - declaration-syntax §6.9: “a `filter`, a boolean expression over the declaration's own binder that narrows the rows before they are aggregated”.
  - declaration-syntax §6.9: “A combined metric has no rows”.
  - DESIGN §5.12 relies on it: “a reader limited to part of the data gets the value over that part (PRD M2)”.
- **Severity:** changes-value, for a scoped reader's figure.
- **Suggested fix:** Give a filter per input for a combined metric (for example `filter: {refused: …, applied: …}`), or refuse one.

#### 16. `.returns` and legacy or migrated history
- **Construct:** `.returns`, and so `rework`.
- **Case:** L02 was ported with legacy intervals REQUESTED → INTAKE → AVAILABLE, and is now SOLD. After the port, a declared transition takes it from SOLD into INTAKE.
- **The readings:**
  - (a) “held before” includes legacy intervals, so `.returns` is true and `rework` is **1**.
  - (b) It includes only the recorded history, so `rework` is **0**.
  - A state renamed by a migration mapping (OLD → NEW, then a transition into NEW) splits the same way.
- **Where the text fails:**
  - declaration-syntax §6.9: “`.returns` a transition that changes state into one the object held before, which is rework”.
- **Severity:** changes-value.
- **Suggested fix:** Say "held before in any interval the index holds, legacy included" (or not), and how a migrated state counts.

#### 17. The actor kind of an interval a port opens, and of legacy intervals
- **Construct:** the implicit `actor_kind` (`.entered_by_kind`) on intervals.
- **Case:** D07, ported with no history. The import's event opens its current PREPARATION interval. Read `Delivery.time_in_state, keep: [state, actor_kind]`.
- **The readings:**
  - (a) The kind is the import event's actor's (“the actor who made the change”), for instance `service`.
  - (b) The kind is the legacy record's, and `unknown` where the record names none.
  - Scenarios §31 shows `human` for SJ1's two legacy `engineer` intervals, although its `legacy_members` line names no actor. The rule for such rows gives `unknown`. It also gives D01's creator as `unknown` in §30 for that reason. This is unless `publish-and-import.md`, which I did not read, extends `entered_by` to member intervals.
- **Where the text fails:**
  - DESIGN §7: “the transition that set it, the actor who made the change and its kind”.
  - DESIGN §7: “The import's event opens each current interval at the time the legacy record says the object took that value, and carries that time”.
  - DESIGN §5.12: “A legacy row whose record names no actor has the kind `unknown` (ADR-0106)”.
- **Severity:** changes-value.
- **Suggested fix:** Say that intervals opened by a port or a refresh take the legacy record's actor kind, else `unknown` (or the importer's).

#### 18. Backdating: which clock orders “when the span began”?
- **Construct:** `.held` against backdated transitions.
- **Case:** job J has engineer A.
  - At 09:30 `reassign` makes B the engineer.
  - At 10:00, `start` OPEN → WORKING is requested with `occurred_at` 09:00. That is within bounds, since the OPEN stay began earlier.
  - The WORKING span therefore begins at 09:00 by occurred time, but after B's assignment by position.
- **The readings:**
  - (a) By occurred time, `.held(engineer)` on the WORKING span is **A**.
  - (b) By position, it is **B**. The transition row's `.held` is B under “before the transition's own writes”, so reading (a) makes the span and the transition disagree.
- **Where the text fails:**
  - DESIGN §7: “entry and exit by position and by occurred time”.
  - declaration-syntax §6.9: “the value a tracked member held when the span began”.
- **Severity:** changes-value, in `time_in_state_by_holder`.
- **Suggested fix:** Say whether `.held` on a span reads the member at the span's entry position or at its occurred time.

#### 19. Spans a migration opens, against "none counts … a publish's migrations"
- **Construct:** interval metrics against a migration mapping.
- **Case:** v2 removes QUEUED and maps it to WAITING. A job is in QUEUED from day 0, is migrated on day 4, and `start`s out of WAITING on day 10. Read `time_in_state` for WAITING.
- **The readings:**
  - (a) Interval rows carry no `.migrated`, so the 6d span the migration entered counts, under the publisher's kind and version 2.
  - (b) The span is the publish, not the flow, so it is left out.
  - `oldest_open` reads `entered_at(WAITING)`, which is day 4 under both.
- **Where the text fails:**
  - DESIGN §5.12: “none counts the import's events, a publish's migrations or an erasure's redactions of other objects, which are the port, the publish and the erasure, not the flow”.
  - The interval row members in declaration-syntax §6.9 (“`.object`, `.state`, `.entered_at`, `.left_at`, `.duration`, `.entered_by_kind`, `.declaration_version`, `.legacy`, `.held(<member>)`”) give no way to tell such a span apart.
- **Severity:** changes-value.
- **Suggested fix:** Either state that interval metrics count spans a migration opens, or add `.migrated` to interval rows and filter on it.

#### 20. “for every live object” when a publish starts tracking a member
- **Construct:** the intervals opened at a publish.
- **Case:** a publish makes the enum `priority` tracked. At that moment job J is OPEN (category `inbound`), K is WORKING (`live`), and L is DONE (`closed`, not terminal).
- **The readings:**
  - (a) “live” means existing: all three get a current interval.
  - (b) “live” means the category `live`: only K does. Under (b), J's `intervals(priority)` stays empty until its next change, and `.held(priority)` on J's spans is absent.
- **Where the text fails:**
  - DESIGN §7: “A member a publish makes tracked opens its current interval at the publish event, for every live object.”
  - The same word is a category, as in declaration-syntax §6.10: “states   ACTIVE category live”.
- **Severity:** changes-value.
- **Suggested fix:** "for every object that exists at the publish".

#### 21. A `sum` that overflows its element type's precision
- **Construct:** the result type of `sum`.
- **Case:** `sum(b.mean_health)` over twelve `BatteryDay` rows of 94.2, where the field is `decimal(4,1)`. The total, 1130.4, needs precision 5.
- **The readings:**
  - (a) The read fails, as a `set` would.
  - (b) The value is unknown.
  - (c) The value is widened, and **1130.4** is returned.
- **Where the text fails:**
  - declaration-syntax §8.3: “`count` yields `int`, `sum`/`min`/`max` the element type”.
  - declaration-syntax §8.3: “Precision is checked at runtime instead, and a value that overflows its declared precision refuses the request the way an over-limit loop does.”
  - declaration-syntax §6.9: “A metric's value is rounded half to even to the scale of its expression's type, as a `set` of it would be”.
- **Severity:** changes-value.
- **Suggested fix:** State the widened type of `sum` in a metric (for example `decimal(p+19,s)`).

#### 22. `time_in(<current closed state>)` against `.duration`
- **Construct:** `o.time_in(STATE)` against a span's `.duration`.
- **Case:** a job has been DONE (closed, terminal) since day 10. Read on day 30.
- **The readings:**
  - (a) Following §8.3, `o.time_in(DONE)` is **20d**.
  - (b) Following §6.9, the same span's `.duration` is absent. So a metric `median(o.time_in(DONE))` gives 20d while `median(i.duration) where i.state == DONE` gives **absent**. A builder who derives `time_in` from the spans, as §8.3 says it reads them, also gets absent.
- **Where the text fails:**
  - declaration-syntax §8.3: “`time_in(<STATE>)` a `duration`, the total over every span in that state, a current one counted up to `now`. They read the object's own intervals”.
  - declaration-syntax §6.9: “so nothing accrues on a finished object”.
- **Severity:** changes-value.
- **Suggested fix:** Say whether `time_in` follows `.duration`'s rule for a closed object.

#### 23. Rounding of an intermediate quotient
- **Construct:** decimal `/` inside a metric value.
- **Case:** `value count(where r.ok) * 1.000 / count() / 2`, with 1 of 7 rows ok.
- **The readings:**
  - (a) Computed exactly, then rounded once: 1/14 is **0.071**.
  - (b) Rounded half to even at each step: 0.143, then 0.0715, gives **0.072**.
  - (c) Truncated at each step: **0.071**.
- **Where the text fails:**
  - declaration-syntax §8.3: “`/` takes its scalar on the **right** and yields the left operand's type”.
  - That section's rounding is stated only for `set`, money and like quantities. The metric rule rounds only “A metric's value”.
- **Severity:** changes-value (rare).
- **Suggested fix:** Say that a metric's expression is computed exactly and rounded once, half to even, at the end.

#### 24. `avg` and `median` of durations: resolution unstated in the sections read
- **Construct:** `avg` of a duration.
- **Case:** `avg` of 1 s and 2 s.
- **The readings:** (a) **1.5 s**. (b) **2 s**, half to even at whole seconds.
- **Where the text fails:**
  - declaration-syntax §6.9: “`avg` keeps its body's type: an average duration is a duration and an average amount an amount of its currency, rounded half to even”.
  - The resolution a duration is rounded to is not stated in §6.9 or §8, and §8.3 gives scale only to `decimal` and `money`. §3.1, which I was not to read, may settle it; if it does, this finding falls.
- **Severity:** changes-value (conditional).
- **Suggested fix:** State a duration's resolution where `avg` is specified.

#### 25. flow-format §4.9's summary of check 56 omits split metrics
- **Construct:** the publish check on `over last`.
- **Case:** a guard reads `over last 30 days` on a metric with `split_by` and no `time_dimension`. A checker built from the summary refuses it, while the rest of §4.9 and declaration-syntax §6.9 allow it.
- **Where the text fails:**
  - flow-format §4.9: “a guard's `metric(…)` binds only the metric's dimensions and windows only a metric with a `time_dimension`, or a combined one whose every input has one (ADR-0128)”.
  - The same section, earlier: “a guard's `over last` clips each piece to the window”.
  - declaration-syntax §6.9: “except on a metric that splits its spans, whose window clips its pieces (ADR-0142), and on a combined metric whose every input declares one or splits”.
- **Severity:** wording.
- **Suggested fix:** Add "or that splits its spans" to both clauses of the summary.

#### 26. The form of a bucket value and `.period` is not given
- **Construct:** time buckets as dimension values.
- **Case:** `month(…)` could be returned as `2026-10` (as the scenarios write it) or as `2026-10-01T00:00Z`. `week` could be `2026-W09` or `2026-W9`, which also changes the order of rows. A reader's `filter` on a bucket has no literal to compare against.
- **Where the text fails:**
  - declaration-syntax §6.9: “`.period`, the bucket it falls in, as the bucket function writes it”.
  - No section read gives the type or form of a bucket function's result.
- **Severity:** wording.
- **Suggested fix:** State each bucket's type and form, for example `YYYY-MM`, and `YYYY-Www` of the ISO week-year.

### Areas where I found nothing

- **Windows on non-split metrics:** both ends are included, a row with no time is excluded, and a `date` is placed at 00:00 UTC. Determined, with nothing to add.
- **Aggregates in general:** values over no rows, absent bodies left out, `count(distinct …)`, the lower middle as median, `avg` of ints as `decimal(19,6)`, and half-to-even at the value's scale. Determined, apart from findings 6, 21, 23 and 24.
- **A guard reading a metric:** binding, leaving a dimension unbound, a bound value that is unknown, no rows, consultation before the transaction with the value recorded whichever side of an `or` decides, and the window ending at the request's `now` (flow-format §5, “`now` is the time of the request”). Determined.
- **Dimensions:** paths read “as it is now”, a dropped set-valued dimension counts a row once, a group with no rows is not reported by a declared metric, and an actor-naming dimension is marked. Determined, apart from finding 14.
- **Standard metrics:** the rows, filter and value are determined for `throughput`, `work_in_progress`, `oldest_open`, `transition_counts`, `override_counts`, `open_work`, `time_unassigned`, `time_to_first_assignment`, `cycle_time_by_assignee`, `handoffs`, `reassigned_back`, `acted_by_non_assignee`, `handoffs_by_object` and `returns_by_object`, apart from the edges in findings 5 and 17 and the surprising values in determined item 24. `time_in_state` and its p80 are affected by findings 5, 17 and 19; `time_in_state_by_holder` by 3 and 18; `refusals` and `flags_raised` by 2; and `refusal_rate` by 1, 2 and 15.
- **Not examined, being outside the sections read:**
  - What state an `erase` leaves an object in, and so whether its current spans stop.
  - What the generated `occurred_within` guard checks, which is what the text's claim “no duration is ever negative” rests on for backdated transitions.

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-value | 21 | 1, 2, 3, 4, 5, 9–24 |
| changes-outcome | 3 | 6, 7, 8 |
| wording | 2 | 25, 26 |
| **Total** | **26** | |

Two findings are disagreements between a scenario's stated result and the specification: finding 2 (§11 and §35's `Shipment.refusals`) and finding 4 (§16 and §20's `gaps`). A third, in finding 17, is a likely disagreement (§31's `entered_by_kind` of legacy intervals).

## 3. Beyond a request, as the reader wrote it

Reader: a builder with no project history, building from the text alone. Read: `docs/DESIGN.md` §5.11, §6 (context only), §7, §8, §9, §11; `docs/design/publish-and-import.md` (whole); `docs/design/adversarial-harness.md` (whole); `docs/design/storage-schema.md` §3, §7, §8, §10 (the sections the above cite); `docs/design/flow-format.md` §4.13 and §4.19. Not read: ADRs, `defects.md`, `TODO.md`, `docs/LESSONS.md`, reviews, case studies, the runner or checker source. `storage-schema.md` §9 (erasure's storage steps) lay inside a range I opened for §8 and §10. The sections under test do not cite it, so no finding relies on it. Where it bears on a finding, the entry says so.

Every quotation below was checked by script against the source files, character for character.

### What is determined

**1. Recording and correcting a datapoint.** A recording is a creation request on a generated `record` type with one terminal state and an `owner` end to its subject. The recording is refused on a subject in a terminal state (`subject_open`) and on a mirror's object (`subject_owned`). It writes the collection's position on the subject and advances no version. A correction is a new observation of the same kind and subject, naming one that no other observation already corrects. A correction of a correction is allowed. A correction may be recorded on a finished subject. Guards and metrics see only uncorrected observations, while `history` and `export` keep both. A late recording is bounded by `max_recording_delay` through `occurred_within`. An aggregating guard records the observation set it read. `forget` can be requested on one observation. A label's note is personal, its name is not, and its occurred time is its recorded time.

**2. Publishing with a migration.** `publish` is the approval transition of a `DeclarationChange`, and it installs everything or nothing. `submit` and `refresh` run the dry run. The approver names the version of the change they read. A migration writes per-object events with source `migrated`, the publish event as cause and the publishing actor as actor. A renamed attribute is DDL only, with no events. A removed attribute gets nothing. An `admit` carries an enum reason and records admissions on migration events, and an unchanged violator gets an admission-only `migrated` event. An admitted invariant's constraint is dropped. Migrations reach terminal objects and never an observation. Removing an enum member that an observation holds refuses the publish. A backfill that fails for one object refuses the whole publish. The impact report's eight items, with ids, are listed in DESIGN §9. Other open changes are superseded in the publish's transaction.

**3. The import.** The import writes only into mirrors, through the built-in assertion. Its events have provenance `asserted`, `state_source` is `imported`, and every event is marked `imported`. Each object gets a new id and keeps its legacy key as an external identifier. Ids are assigned before any row is written. Rows go in dependency order, and a cycle of required references commits with its foreign keys deferred. Legacy entries are read-only, of kind `legacy`, and keep only their own object's fields, with a kept-fields list for erasure. The creation time and kind come from the mapping. Without them, `created_at` is the earliest time the port knows of, the object is marked undated and the kind is `unknown`. `entered` opens the current interval, and earlier values arrive as legacy intervals. Silent spans are declared and `recorded_from` is recorded. Sequences are raised and never lowered. Soft-deleted rows land in the deleted state. A disposition class is identified by (type, rule, shape). `ADMIT` applies only to invariant classes. `ADMIT` and `EXCLUDE` need a reason. No import runs while a class is `UNDECIDED`. A mirror has attributes, states, an external identifier and no transition but `erase`, and only the import writes it. Cutover is a publish that removes the marking.

**4. Subscription and delivery.** `pull` is the only delivery. The settled cursor orders events by (transaction, position). Each object's events arrive in its own order, and delivery is at least once. The acknowledged cursor is runtime state and emits no events. A filter selects on type or family, transition names and `changes_state`. The attempt log is never delivered. Events of an erased object stay in the log, redacted in place, and an `erased` event is added. The archive is read through the same surface.

**5. Erasure.** The reach is listed in DESIGN §8: personal attributes, the events that carried them, legacy entries not kept, proposals and their events, personal inputs, observations through `forget`, label notes, personal enum values in the interval index, personal external identifiers, callers' events up the cause chain, taint-copies downstream with `erased` events on those objects, and the supersession chain. The event skeleton, label names, the attempt log and read sets stay. After an object's own erasure, `not_erased` refuses a later write of its personal attributes.

**6. Proposals and flow changes.** A proposal's lifecycle is `pending → executed | rejected | withdrawn | invalidated`. Approval executes the request as the approver, with the approval as cause, under the current declaration. A decision on a proposal that is no longer pending is refused as `unavailable`. `DeclarationChange` has `draft`/`revise`/`submit`/`refresh`/`publish`/`reject`/`withdraw` transitions and the states `DRAFTED` and `SUBMITTED`. `publish` requires a change drafted against the installed version.

**7. The adversarial harness.** These rows are precise enough: a stored end disagreeing with its far side; `satisfied` with no event; a pull that skips an event or breaks one object's order; an event not naming the actor and kind; an erased object's shared file deleted while another object holds it. The rows covered in findings 38 to 41 are not.

Every area has at least one finding.

---

### Findings

#### 1. A correction's occurred time, and whether the recording bound applies to it
- **Area:** 1, recording and correcting.
- **Scenario:** The kind `inspection_result` has `max_recording_delay` of 7 days. Observation A occurred on 1 March and was recorded on 2 March. On 30 April someone records B, correcting A's mistyped reading. A metric buckets results by month of occurred time.
- **The readings:** (a) B is an ordinary recording. Its occurred time is a request field bounded by `occurred_within`, so a B dated 1 March is refused. A B dated in late April is accepted, and the March bucket loses A while April gains B. (b) B replaces A's fact. It takes A's occurred time and the bound is waived, just as the terminal-subject rule is. March keeps one result, corrected. The outcomes differ (accepted or refused) and so does the bucket the fact lands in.
- **Where the text fails:** DESIGN §5.11: "the occurred time is a request field bounded by a generated `occurred_within` guard (ADR-0099)"; "a correction may be recorded after the subject is finished, since it replaces a fact rather than adding one". §6 step 4 lists "for a recording `subject_open`, `corrects_current` and `subject_owned`". A correction is exempted from the terminal-subject rule only; nothing is said about time.
- **Severity:** changes-outcome
- **Suggested fix:** State a correction's occurred time (its own, or the corrected observation's) and whether `occurred_within` binds it.

#### 2. Which reads hide a corrected observation
- **Area:** 1
- **Scenario:** A is corrected by B, both on subject S. A caller runs `query` on the kind filtered to S, then `get(A)`, then `history(S)`.
- **The readings:** (a) "Every read of the kind" means every read, so `query` returns only B. (b) The dash clause names guards and metrics only. `query` and `get` are object reads and return both A and B. For `history(S)`, one reading includes the observations' creation events, since a recording has "a place in history". The other returns only S's own events, so A and B appear only under their own ids.
- **Where the text fails:** DESIGN §5.11: "Every read of the kind — a guard through the subject's part, or a metric — sees only the uncorrected ones, and `history` and `export` keep both (ADR-0095, ADR-0096)." DESIGN §10: "**history**(id, follow?) | Events with provenance, legacy entries, and optionally the combined timeline through supersession". No read-surface row mentions corrections.
- **Severity:** changes-record
- **Suggested fix:** For each read-surface operation, say whether corrected observations are returned, and whether a subject's `history` includes its observations' events.

#### 3. An outcome that records an observation while entering a terminal state
- **Area:** 1
- **Scenario:** `Service.complete: IN_PROGRESS → DONE` (final), whose effect has a `create` step recording an `inspection_result` on `this`. This is the case §5.11 offers as "how a transition records what it decided on".
- **The readings:** (a) The new state is written first, and the creation's guards are evaluated against the state so far. `subject_open` sees DONE and refuses, and the whole request aborts, so `complete` can never be taken. (b) `subject_open` guards direct recordings only, so an outcome recording on its own object is exempt. `complete` applies and records the result.
- **Where the text fails:** DESIGN §5.11: "A transition's outcome may record one with an ordinary `create` step, which is how a transition records what it decided on." It also says an observation "may not be recorded on a subject in a terminal state". §6 step 6: "for each of the rest evaluate its guards **against the state produced so far**". flow-format §4.19: "A transition's new state is written before its effect runs". Read literally, the text refuses the case it names as the purpose.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether `subject_open` applies to a `create` step in a transition on the subject itself, or have a check refuse such a declaration at publish.

#### 4. A subject invariant over an observation collection, on recording
- **Area:** 1
- **Scenario:** `Service` declares an invariant that at most three of its `inspection_results` failed. A fourth failed result is recorded.
- **The readings:** (a) A recording "advances no version and writes no attribute", so the subject is not a written object and its invariants are not checked. The recording applies and the invariant is left false with no admission, which the harness fails. (b) The written observation "could violate" the subject's invariant, so step 7 checks it and the recording is refused. (c) A publish check forbids an invariant from reading an observation collection.
- **Where the text fails:** DESIGN §5.11: "It advances no version and writes no attribute, so it conflicts only with a transition whose guard read that collection; a recorded fact matters to the flow only through a guard that reads it (ADR-0099)." Labels are read "**never by a guard or an invariant**", which implies other observations may be. §6 step 7: "Check every invariant the written objects could violate".
- **Severity:** changes-outcome
- **Suggested fix:** State whether a subject's invariants may read an observation collection, and if so that recording checks them.

#### 5. The same-subject rule for a correction has no clause
- **Area:** 1
- **Scenario:** A correction names observation A, which is on subject S1, and gives subject S2.
- **The readings:** (a) A correction takes its subject from A and has no subject input. (b) A subject input is required and a mismatch is refused, under `corrects_current` in one engine and as `invalid input` in another. The attempt row names a different clause in each engine.
- **Where the text fails:** DESIGN §5.11: "A correction is a new observation of the same kind and subject, naming the one it corrects, which must not already be corrected". The generated guards listed in §6 step 4 name no clause for the same-subject rule.
- **Severity:** changes-record
- **Suggested fix:** Derive a correction's subject from the corrected observation, or name the clause that refuses a mismatch.

#### 6. *Live object* carries two meanings, and the publish rules rely on it
- **Area:** 2, publishing.
- **Scenario:** Version 2 adds a required `manufacturer_serial` with a `backfill`, makes `site` tracked, and removes the final state `SCRAPPED`, mapping it to `RETIRED`. Some robots are in `RETIRED` or `SCRAPPED`.
- **The readings:** (a) *Live* means every existing object. Retired robots are backfilled and get a `site` interval, and the scrapped robots appear in the report. (b) *Live* means not in a final state, as in deletion's rule, or in the category the example calls `live`. Retired robots are then not backfilled, so a required attribute is absent and the harness fails. They get no `site` interval. The scrapped robots are left out of the report, yet storage §10 says a migration reaches terminal rows too, so the approver is never shown objects the publish rewrites.
- **Where the text fails:** P&I §1 step 3: "**Read the live objects.**" DESIGN §9: "the live objects violating a new invariant". DESIGN §7: "opens its current interval at the publish event, for every live object". storage §10: "the mapping moves live objects out of it first" and "has moved every live row off it". Against these, storage §10: "**A migration reaches every row the change affects, terminal ones included, and no observation's**". DESIGN §8 uses the other sense: "gated on there being no live object referencing this one". The P&I appendix declares `categories: [live, closed]`.
- **Severity:** changes-outcome
- **Suggested fix:** Replace "live" in the publish and report rules with "every object of the type" (or define it once) and use it the same way everywhere.

#### 7. One migration event per object, or one per mapping per object
- **Area:** 2
- **Scenario:** The appendix's version 2. A robot in `LEGACY_HOLD` with no manufacturer serial is reached by both the removed-state mapping and the `backfill`. In a variant, version 2 also lists `manufacturer_serial` as a required attribute of `AVAILABLE`.
- **The readings:** (a) One migration transition per object carries every mapping that reaches it. That gives one event and one version increment, and invariants are checked once, over the final row. (b) One transition per mapping per object, in the listed order: two events and two increments. Step 7 runs after the state move, while the attribute that `AVAILABLE` requires is still absent. The publish is refused, or needs an `admit` that records an admission the next event discharges. The order among objects, and so the global positions, is not stated either.
- **Where the text fails:** P&I §1 step 5: "apply each mapping — a removed state, a removed enum member, a `backfill` — as a recorded migration transition per object, checking every invariant over what it writes". P&I §3: "A removed state's mapping is applied as **one recorded migration transition per object**". Also: "A removed enum member's mapping and a `backfill` are applied the same way, one recorded migration transition per object that needs one". Also: "an object no other mapping changes receiving a `migrated` event carrying only the admission". DESIGN §6: "runs steps 5 to 8 for each object as part of the publish's transaction".
- **Severity:** changes-outcome
- **Suggested fix:** Specify one migration transition per object, carrying every mapping that reaches it, with its invariants checked once, and fix the object order (ascending id).

#### 8. DDL before the mappings, or after
- **Area:** 2
- **Scenario:** Version 2 removes enum member `OBSOLETE`, mapped to `FAILED`, and adds a compilable invariant that live rows violate until the `backfill` runs.
- **The readings:** (a) Step 5's order: emit the DDL, which replaces the `CHECK` and runs `ADD CONSTRAINT`, then apply the mappings. The database rejects the DDL because rows still hold `OBSOLETE` or violate the constraint, so every such publish fails. (b) storage §10's order: mappings first, then the `CHECK` and the constraint.
- **Where the text fails:** P&I §1 step 5: "Otherwise write `of_declaration` with the change's id, emit the DDL of the storage schema §10, apply each mapping". storage §10 says the opposite for three rows: "the `CHECK` is replaced after a `removed member` mapping has moved every live row off it by recorded migration transitions (ADR-0099)"; "no DDL; the `CHECK` is replaced, and the mapping moves live objects out of it first"; "`ADD CONSTRAINT`, after the report says how many live objects violate it, and only where no `admit` covers it".
- **Severity:** changes-outcome
- **Suggested fix:** Order step 5 as: DDL that adds (columns, renames), then mappings, then DDL that constrains (`CHECK`, constraints).

#### 9. Is a new invariant evaluated before the mappings or after
- **Area:** 2
- **Scenario:** Version 2 adds the invariant `state != AVAILABLE or manufacturer_serial is not null` and backfills `manufacturer_serial` for every robot.
- **The readings:** (a) "Which violate a new invariant" is read over the objects as they are. Every `AVAILABLE` robot violates it, which is a DECISION that blocks the publish until an `admit` names the invariant. The admissions are then recorded on rows that no longer violate, or not recorded at all. (b) New invariants are evaluated over the rows as the simulated mappings leave them. Nothing violates, and the publish proceeds with no `admit`.
- **Where the text fails:** P&I §1 step 3 lists "Which violate a new invariant, which are in a removed state" and then, separately, "**Apply every mapping in simulation** and evaluate every invariant over the objects it writes". DESIGN §9: "the live objects violating a new invariant, or an invariant a mapping would break". P&I §2: "A new invariant that eleven live objects violate is not a defect in the declaration and not something to wave through: it is a decision".
- **Severity:** changes-outcome
- **Suggested fix:** Say that new invariants are evaluated over the state the mappings leave.

#### 10. What "names any object or proposal the attached one did not" compares
- **Area:** 2
- **Scenario:** (i) Object O is listed in the attached report among those that would gain an available transition. By approval time it violates the new invariant, which an `admit` covers. (ii) While change C1 waits for approval, someone drafts C2, and C1's recomputed report names C2 among the changes it will supersede.
- **The readings:** For (i): (a) the comparison is over the union of ids. O was named, so the publish proceeds and records an admission on O that the approver was never shown as a violation. (b) The comparison is per impact, as the `affected` field suggests, so the publish is refused `impact_unchanged`. For (ii): (a) C2 is an object the attached report did not name, so the publish is refused. Any new draft then blocks every pending approval until a `refresh`. (b) The supersession list is not compared, so the publish proceeds and C2 is superseded.
- **Where the text fails:** P&I §1 step 5: "refuse the publish as `impact_unchanged` if the recomputed report names any object or proposal the attached one did not". DESIGN §9: "a recomputed report naming nothing the attached one did not". P&I §2: "# ids per impact, compared again at approval (ADR-0096)". DESIGN §9 lists "the other open changes the publish will supersede", and says a change is an object: "Being an object, every step is attributed".
- **Severity:** changes-outcome
- **Suggested fix:** Say the comparison is per impact category, and whether the superseded-changes list takes part.

#### 11. The recomputed report's `now`, and external guards
- **Area:** 2
- **Scenario:** A change moves `expire`'s guard from `end_date <= now` to a later threshold. It is submitted on Monday and approved on Wednesday. Between the two, more objects fall between the old and new thresholds. In a variant, the changed guard names an external evaluator.
- **The readings:** (a) The report is recomputed at approval with that moment's `now` and live verdicts. New objects gain or lose availability, so the publish is refused `impact_unchanged`. A change to a time- or evaluator-dependent guard can then be approved only straight after a `refresh`. (b) The report's availability is computed as `availability` computes it, with no external evaluator, and time-dependent differences are not treated as new impact. The approval succeeds.
- **Where the text fails:** DESIGN §9: "the live objects that would gain or lose an available transition under the new guards". P&I §1 step 5: "run steps 1 to 4 again". DESIGN §10: "Neither `available`, `check` nor `availability` calls an external evaluator; each says which guards it did not evaluate." Nothing fixes the dry run's `now`, or says whether it consults evaluators.
- **Severity:** changes-outcome
- **Suggested fix:** State the dry run's `now` and whether external evaluators and metrics are consulted when computing gained and lost transitions.

#### 12. No severity is given for each impact item
- **Area:** 2
- **Scenario:** A change makes an observing clause enforced, and 40 live objects would now be refused by it. No invariant changes.
- **The readings:** (a) It is a DECISION, which blocks the publish. But no mechanism records a decision about a guard, since `admit` names an invariant, so the publish cannot proceed. (b) It is a NOTICE and the publish proceeds. The same question arises for "gain or lose an available transition" and for invalidated proposals.
- **Where the text fails:** P&I §2: "Anything a person must decide blocks the publish until the decision is recorded, and the decision is recorded in a file rather than in an argument to the command." The only item given a severity is "A new invariant that eleven live objects violate is not a defect in the declaration and not something to wave through: it is a decision". The item lists in P&I §1 step 3 and DESIGN §9 carry none.
- **Severity:** changes-outcome
- **Suggested fix:** Give each impact item its severity (FATAL, DECISION or NOTICE) and, for each DECISION, the text that records it.

#### 13. Which pending proposals a publish "breaks"
- **Area:** 2 (and 6)
- **Scenario:** Under the appendix's two versions, three proposals are pending. P1 is `retire` with `retirement_reason: OBSOLETE`, a member removed and mapped to `FAILED`. P2 is `name` with input `old_name`, an attribute renamed to `new_name`. P3 is `hold`, a transition removed.
- **The readings:** Every reading invalidates P3. For P1: (a) the member mapping rewrites the proposal's input to `FAILED` and P1 stays pending; (b) the new declaration cannot express `OBSOLETE`, so P1 is invalidated; (c) P1 stays pending and its later approval returns `invalid input`. For P2: (a) the input is renamed and P2 stays pending; (b) P2 is invalidated. The proposals executed later differ, and so do the report's "pending proposals invalidated" and `impact_unchanged`.
- **Where the text fails:** DESIGN §9: "a declaration that can no longer express the request invalidates it (ADR-0036, ADR-0044)". P&I §1 step 5: "invalidate the pending proposals the change breaks". P&I §3: "A renamed attribute is DDL and no events: the values did not change, only the name they are read under."
- **Severity:** changes-outcome
- **Suggested fix:** Enumerate what invalidates a pending proposal at publish, and whether mappings (renames, member mappings) rewrite proposal inputs.

#### 14. A member tracked by the same publish that migrates it, and a member untracked
- **Area:** 2
- **Scenario:** (i) Version 2 declares a new attribute `site` that is tracked, required, and backfilled with `"UNKNOWN"`. (ii) Version 3 stops tracking `site`, and version 4 tracks it again.
- **The readings:** For (i): (a) the backfill's migration event updates the interval index under step 8, so the interval opens at the migration event; (b) step 5 opens it "at the publish event", after the mappings in the listed order; (c) both happen, giving an absent-value interval from the publish event closed at the migration event. For (ii): (a) version 3 closes the open `site` intervals at its publish event; (b) it leaves them open, and version 4 opens a second current interval over the first.
- **Where the text fails:** DESIGN §7: "**A publish.** A member a publish makes tracked opens its current interval at the publish event, for every live object." P&I §1 step 5 lists "apply each mapping" before "open the current interval of every member the change makes tracked for every live object". §6 step 8: "update the interval index for every changed state and tracked value". storage §10's table has a row for "a new tracked member" and none for a member no longer tracked.
- **Severity:** changes-record
- **Suggested fix:** Say which event opens the interval when the publish also writes the value, and what a publish that untracks a member does to its open intervals.

#### 15. `exceptions` after a migration moves an asserted object
- **Area:** 2
- **Scenario:** A robot was put into `LEGACY_HOLD` by an assertion, so its `state_source` is `asserted`. Version 2 maps `LEGACY_HOLD` to `AVAILABLE`.
- **The readings:** (a) `exceptions` reads `state_source`, which the migration leaves as `asserted`, so the robot is listed as an object "whose state an assertion last set", although a migration set `AVAILABLE`. (b) `exceptions` follows its definition, the last state-setting event, so the robot is not listed.
- **Where the text fails:** storage §3 defines `state_source` as "the `source` of the event that last **changed** `state`" and in the same cell says "A migration leaves it as it was, which is why `migrated` is not a value (ADR-0105)" and "`exceptions(type)` reads `asserted` (ADR-0083)". DESIGN §10: "The objects whose state an assertion last set, the data import excluded". DESIGN §8: "`exceptions(type)` lists the objects whose state an assertion last set".
- **Severity:** changes-record
- **Suggested fix:** Say whether a migration's state change ends an object's listing in `exceptions`, and make the `state_source` definition agree.

#### 16. Existing observations when their kind changes
- **Area:** 2
- **Scenario:** Version 2 adds a required field to `inspection_result`, and a new kind invariant over its fields that some existing observations violate.
- **The readings:** (a) The publish is refused: no migration can reach an observation, and the `NOT NULL` column and the invariant cannot hold. (b) The publish is accepted. Old observations are born final and exempt, the column stays nullable, and the invariant is checked only at recording, but the harness then fails "a required attribute absent" or "an invariant false, with no admission recorded". (c) Admissions are recorded on the violators, but an observation gets no migration event to carry them.
- **Where the text fails:** storage §10: "**A migration reaches every row the change affects, terminal ones included, and no observation's** (ADR-0101): it is the publish's recorded change, not a transition the object takes, and an observation is born final, so a publish removing an enum member an observation holds is refused." Only that case is stated. DESIGN §8: "and never an observation, which is born final (ADR-0101)". DESIGN §5.11: "a kind may declare invariants over its own fields, checked when it is recorded".
- **Severity:** changes-outcome
- **Suggested fix:** State what a publish does for each change to a kind with existing observations: a new required field, a new invariant, a new required state attribute.

#### 17. What a failing batch leaves behind
- **Area:** 3, the import.
- **Scenario:** An `import_batch` of 500 services. Row 212 carries a legacy interval earlier than the creation time the mapping supplies, or a violation the disposition file does not cover.
- **The readings:** (a) The batch is one transaction. Nothing is written, the first unplaceable row is named, and a resumed run re-sends the whole batch. (b) Rows 1 to 211 commit, and resumption continues from 212, which is what "`lookup('legacy', key)` says whether it arrived" suggests. (c) Only row 212 is refused and the other 499 are written. The store's contents after the call differ in each case.
- **Where the text fails:** DESIGN §10: "**import_batch**(batch, dry_run?)". P&I §4: "Rows go in dependency order where one exists, so a failure names the first row that could not be placed; a cycle of required references has no such order and commits with its foreign keys checked at commit". P&I §8: "**An import resumes rather than being redone.**" DESIGN §11: "an entry time or a legacy interval earlier than a creation time the mapping does supply is refused". Nothing says what "refused" refuses.
- **Severity:** changes-outcome
- **Suggested fix:** State the batch's transaction boundary, and whether a refusal refuses the row, the batch or the run.

#### 18. Re-sending an object that already arrived, and a refresh that changes nothing
- **Area:** 3
- **Scenario:** A resumed run re-sends service S, which has already been imported. Or a nightly refresh sends S with values identical to the stored ones.
- **The readings:** (a) S is skipped because it has arrived. No event is written and `imported_at` stays old, so a guard bounding `imported_at` starts refusing although the data is current. (b) The send is a refresh and writes an `imported` event, advancing the version and delivering the event to subscribers, so the fold reproduces `imported_at`. (c) `imported_at` is updated without an event, and the harness row "an event whose replay does not reproduce the row" then fails the engine.
- **Where the text fails:** storage §3: "**A mirror type's table has `imported_at`**, the time its last import wrote the row, which a guard may bound (ADR-0100)." DESIGN §7: "A refresh that changes a tracked value does the same". P&I §4: "A refresh of a mirror carries, for each value that changed since the last import, the same `entered`". P&I §8: "so `lookup('legacy', key)` says whether it arrived and a second run continues."
- **Severity:** changes-record
- **Suggested fix:** Say whether a refresh with no change writes an event and moves `imported_at`, and how the import tells a resumed port from a refresh.

#### 19. What the disposition file does at import time
- **Area:** 3
- **Scenario:** (i) Class C is marked `CLEAN_UPSTREAM`, but the re-extract still has three objects in it. (ii) A later refresh brings a new object that falls in class D, already marked `ADMIT`. (iii) An imported object's optional reference points at a legacy key in an `EXCLUDE` class.
- **The readings:** For (i): (a) the class is decided, so the import runs and the three objects fail, with the consequences in finding 17; (b) the import is refused until the count is zero; (c) the three objects are treated as excluded. For (ii): (a) the `ADMIT` applies to any object matching (type, rule, shape), with the class's reason; (b) the refresh is refused until a new dry run is decided. For (iii): (a) the reference is imported as absent; (b) it becomes a new class; (c) the import is refused as an unresolvable key.
- **Where the text fails:** P&I §6: "**An import will not run while any class is `UNDECIDED`.**"; "The identity of a class is (type, rule, shape) and not the object set"; "An excluded object leaves a row in the file and nothing in the store, which is the only record that it was left behind." P&I §4: "an unresolvable legacy key is found here and not by the database". `import_batch(batch, dry_run?)` takes no disposition, and nothing says how the file reaches the engine.
- **Severity:** changes-outcome
- **Suggested fix:** Say how `import_batch` receives the disposition file, and how each disposition applies to objects still or newly in its class and to references into excluded objects.

#### 20. Whether an imported admission is listed by `exceptions`
- **Area:** 3
- **Scenario:** A disposition marks an invariant class `ADMIT`, and twelve services are imported with admissions.
- **The readings:** (a) The twelve are listed, per P&I §6 and DESIGN §10's second clause. (b) They are excluded, because every event the import writes is marked `imported`, and `exceptions` excludes what is so marked.
- **Where the text fails:** P&I §6: "An admitted violation becomes an admission on the object, which `exceptions(type)` returns until the invariant holds again (ADR-0054)." DESIGN §11: "every event it writes is marked `imported`, which is what `exceptions` and the override metrics exclude". DESIGN §10: "The objects whose state an assertion last set, the data import excluded, and those holding an admitted invariant violation".
- **Severity:** changes-record
- **Suggested fix:** Say whether the import's exclusion from `exceptions` covers its admissions.

#### 21. Which declaration the import evaluates
- **Area:** 3
- **Scenario:** Stage 1 ports `Service` as a mirror with attributes, states and `erase`. The owned version to be published at cutover has creation guards and invariants.
- **The readings:** (a) The import evaluates the mirror's own declaration. A mirror has no creation transition, so "a creation guard unmet" never arises. The owned version's violations surface only in the cutover publish's report, where an `admit` is a counted override. (b) The import evaluates the owned version to come. Creation-guard classes appear and `ADMIT`s are recorded at import, uncounted, against invariants the installed mirror does not declare.
- **Where the text fails:** DESIGN §11: "The importer evaluates the declaration and reports every violated invariant and unsatisfied structural guard". P&I §6: "a structural guard — a required attribute or reference absent, a creation guard unmet — has no admission shape". DESIGN §11: "A type this store holds and does not yet own is declared **`mirror`**: attributes, states and an external identifier, no transitions but `erase`, written only by the import path (ADR-0101)", and "every type is ported as a mirror and cut over by the publish that removes the marking".
- **Severity:** changes-outcome
- **Suggested fix:** Name the declaration the import's dry run evaluates, and say whether a mirror may declare invariants.

#### 22. `recorded_from` for an object with legacy entries but no intervals
- **Area:** 3
- **Scenario:** The mapping supplies no creation time, no `entered` and no legacy intervals, but the object has audit entries back to 2024. It is ported in 2026. A variant has state intervals back to 2024 and no assignee intervals.
- **The readings:** `created_at` is 2024 (undated), and the current interval "begins at the import". (a) The entries count as history, so `recorded_from` is 2024. A time-in-state metric over 2024 to 2026 then reads nothing and reports no gap. (b) History means intervals, so `recorded_from` is the port and the span is a gap. In the variant, `recorded_from` is one value per object, so an assignee metric sees continuity it does not have.
- **Where the text fails:** P&I §4: "It sets each object's `recorded_from`: its legacy creation where its history is whole, the end of its last silent span where there is one, and the port where it supplies no history." Also "Where the legacy data says nothing, the current interval begins at the import". DESIGN §11: "Each object records `recorded_from`, the earliest time from which its record is continuous."
- **Severity:** changes-record
- **Suggested fix:** Define "history" here as intervals, set `recorded_from` to the port when the current interval begins at the port, and say whether it is per object or per dimension.

#### 23. The built-in assertion's own rules on a refresh
- **Area:** 3
- **Scenario:** (i) A refresh changes only an attribute of mirror object M. Its state is unchanged. (ii) M was imported soft-deleted, into the final deleted state. The legacy system restores it, and a refresh carries its new state.
- **The readings:** For (i): (a) the built-in assertion is an assertion, and an assertion naming the current state is refused `invalid input`, so no refresh can change attributes alone; (b) the built-in assertion is exempt. For (ii): (a) an assertion is never taken at a final state, and DESIGN §8 names erasure and a migration as the only things that reach a terminal state, so M stays deleted while the legacy system shows it live; (b) the refresh is applied.
- **Where the text fails:** DESIGN §8: "The import uses a **built-in** assertion". flow-format §4.13: "A request whose `to` is not one of the listed states, or is the object's current state, is refused with `invalid input`, since no transition leaves a state for itself; an assertion is never taken at a final state." DESIGN §8: "it is the one transition that runs at a terminal state; a publish's migration, which is the publish's recorded change and not a transition the object takes, is the other thing that reaches one". DESIGN §11: "Soft-deleted rows land in the type's deleted state."
- **Severity:** changes-outcome
- **Suggested fix:** State which of §4.13's assertion rules the built-in assertion keeps, and what a refresh does to an object in a final state.

#### 24. A refresh whose legacy times precede what the store holds
- **Area:** 3
- **Scenario:** The port recorded that M `entered` state S on 10 May. A later refresh says M moved to S' on 8 May, after a late edit in the legacy system, or carries intermediate values dated before the last import. In a variant, a publish migrated M's value at time T, and the refresh dates its legacy change before T.
- **The readings:** (a) The refresh is refused for M. (b) The entry is clamped to the current interval's start. (c) It is accepted as given, and the harness then fails the engine for an interval "out of order with the object's others".
- **Where the text fails:** DESIGN §7: "A refresh that changes a tracked value does the same, and may carry the intermediate values the legacy record shows since the last import, opened and closed in order on its event." The harness fails "an interval an import or a publish opened that is out of order with the object's others". Nothing says what the import does with such input.
- **Severity:** changes-outcome
- **Suggested fix:** State what the import does with an `entered` time or an intermediate value earlier than the object's latest interval entry.

#### 25. Importing into an erased mirror object: refuse or skip, and which values
- **Area:** 3 (and 5)
- **Scenario:** Mirror customer C is erased, so its phone and email are absent. The legacy system still holds both, and now also a `mobile` that C did not have when erased. A refresh carries all three, plus a changed non-personal `tier`.
- **The readings:** (a) The refresh of C is refused "as the import already is", mirroring `not_erased`, so `tier` is not updated and the batch may fail (finding 17). (b) The personal values are dropped silently and `tier` is applied. For `mobile`: (a) it is written, because the import "never rewrites a personal value an erasure removed" and no mobile was removed; (b) it is dropped, because the import "never writes a personal column of an object whose erasure is recorded".
- **Where the text fails:** DESIGN §8: "a request whose outcome would write one of its personal attributes is refused by the generated guard `not_erased`, as the import already is". DESIGN §11: "It never rewrites a personal value an erasure removed". DESIGN §10: "never rewrites what an erasure removed". storage §10: "**A request or an import never writes a personal column of an object whose erasure is recorded**".
- **Severity:** changes-outcome
- **Suggested fix:** Give one rule: every personal attribute of an erased object is skipped (or the object refused) on import. Say which, and what happens to the rest of the row.

#### 26. The actor on a ported current interval
- **Area:** 3
- **Scenario:** The mapping supplies `entered` for M's state: 3 March, changed by employee E, who is ported as an actor. Operator O runs the import.
- **The readings:** The interval row's "actor who made the change" is (a) E, as the mapping says, or (b) O, the actor of the import's event, with E carried only on the event. Interval-based metrics that split by actor differ.
- **Where the text fails:** DESIGN §7 says the interval index records "the transition that set it, the actor who made the change and its kind". P&I §4: "the mapping may supply `entered`: when the object took its current value and who changed it, where the legacy record says. The import's event opens the current interval at that time and carries it". "It" is the time, and nothing says where the changer goes.
- **Severity:** changes-record
- **Suggested fix:** Say that the ported current interval records the legacy changer, or kind `unknown`, and not the importing actor.

#### 27. A field about an object not yet ported
- **Area:** 3
- **Scenario:** Stage 1 ports services, whose audit rows carry the customer's name. Customers port in stage 3, or in a later batch of the same run.
- **The readings:** (a) The store holds no customer object yet, so the field "is not ported" and is lost for good. (b) The field is held until the customer arrives and then moved, although legacy entries are read-only once written. (c) The import is refused until the customer is ported.
- **Where the text fails:** P&I §4 (Attach): "a field describing another ported object, such as a customer's name on a service's audit row, moves to that object's history"; "a field about a person or thing the store holds no object for, such as the IP address of the employee who acted, is not ported". P&I §7: "The type arrives as a `mirror` in one, is kept current by repeated import while it is one". The text does not say whether "ported" means already ported or ported at some point.
- **Severity:** changes-record
- **Suggested fix:** Say whether a field waits for its object's port, and where it waits.

#### 28. Filtering by transition name on events that are no transition of their object
- **Area:** 4, subscription and delivery.
- **Scenario:** A subscription filters on type `Customer` and transitions `[update_contact]`. Customer events arrive from a publish's `migrated` event, an admission-only `migrated` event, the import's event, a refresh, a taint-redaction `erased` event on a customer that was not itself erased, and a supersession-chain member's erase.
- **The readings:** (a) Each such event carries a reserved name (`migrate`, `import` and so on), and only a filter naming it, or naming no transitions, receives it. (b) It carries no name, and the engine chooses whether a transition filter passes or drops it. Delivery differs.
- **Where the text fails:** DESIGN §7 defines the filter as "a filter over a type or family, transition names and `changes_state`;" and says "**Every object whose value an erasure redacts gets an event.** The event has source `erased`, is caused by the erasure's own event, and names the attributes redacted and never their values." No transition name is given to a migration, import, refresh or redaction event.
- **Severity:** changes-record
- **Suggested fix:** Give each non-request event (migration, import, refresh, taint redaction, change supersession, proposal invalidation) its transition name, and say how a transition filter treats it.

#### 29. `pull`'s cursor, `acknowledge`'s cursor, death and revival
- **Area:** 4
- **Scenario:** A relay restarts and calls `pull(sub)` with no cursor. Another calls `pull(sub, c_old)` with a cursor before the acknowledged one. A third calls `acknowledge` with `c_old`. Separately, a subscription past its death threshold is pulled, and a revoked one is pulled.
- **The readings:** (a) `pull` reads from the cursor it is given, and the acknowledged cursor only feeds lag. With no cursor it starts at the log's beginning. `acknowledge` may move backwards, which is what "revived at any cursor" means. Dead and revoked subscriptions still pull. (b) `pull` defaults to the acknowledged cursor and `acknowledge` only advances. A dead subscription is held until revived, and a revoked one is refused.
- **Where the text fails:** DESIGN §7: "`pull` and `export` both return it, and `acknowledge` takes it."; "Lag and death are **derived** from that cursor and the thresholds"; "A subscription may be revived at any cursor, because nothing is pruned (ADR-0034, ADR-0043)."; "a lifecycle of `active → revoked`."; "stopping it loses nothing, since the cursor waits where it was acknowledged." DESIGN §10: "**pull**(subscription, cursor)". No operation named "revive" exists.
- **Severity:** changes-outcome
- **Suggested fix:** Define `pull`'s default cursor, whether `acknowledge` may move backwards, what revival is, and what death and revocation do to `pull`.

#### 30. The age behind lag
- **Area:** 4
- **Scenario:** An import at 12:00 writes 50,000 events carrying legacy times back to 2019. Or a transition is backdated by a week. A subscription's death threshold is one day.
- **The readings:** (a) Age is measured from the recorded time, so lag is near zero. (b) Age is measured from the time the event carries, so the subscription is past its death threshold at once.
- **Where the text fails:** DESIGN §7: "the lag is the age of the oldest event the filter selects after the acknowledged cursor". Every event carries "the time a backdated change happened beside the time it was recorded".
- **Severity:** changes-outcome
- **Suggested fix:** Say that lag is measured from the recorded time.

#### 31. A supersession "chain" that is a tree
- **Area:** 5, erasure.
- **Scenario:** CRM duplicates X and Y are both merged into Z (two `merge_in` requests). X is then erased.
- **The readings:** (a) X's chain is X's predecessors plus its successors, {X, Z}, so Y keeps its personal values. (b) Each member's erase follows its own chain, so Z's erase reaches Y and erasing X erases Y as well.
- **Where the text fails:** DESIGN §8: "It erases every member of the object's **supersession chain**, predecessors and successors, each through its own type's `erase`, since superseding declares that the successor continues the same thing". Also "so a chain never comes back on itself and ends in one object not superseded (ADR-0144)". flow-format §4.19: "Erasure follows the chain, through each member's own erasure". §4.19's own example is "a duplicate merged into the customer it duplicated", which produces the tree. The harness fails a value readable "in a predecessor", and Y is a predecessor of Z.
- **Severity:** changes-record
- **Suggested fix:** Say whether erasure reaches every object whose chain ends in the same survivor, or only the object's ancestors and descendants.

#### 32. What `not_erased` refuses: absence, and erasure itself
- **Area:** 5
- **Scenario:** (i) Erased customer C is closed by `close`, whose effect writes `phone := null`, or which takes an optional personal input that is left out. (ii) C is erased a second time, to retry a failed file deletion. (iii) Y is erased, and its chain includes Z, which is already erased.
- **The readings:** For (i): (a) any write to a personal attribute is refused, so C can never close; (b) writing absence restores nothing and is allowed. For (ii) and (iii): (a) an erase's outcome writes personal attributes, so `not_erased` refuses it, and in (iii) that aborts Y's whole erasure; (b) `erase` is exempt, and a repeat either writes another `erased` event (version advanced, delivered) or is a no-op.
- **Where the text fails:** DESIGN §8: "a request whose outcome would write one of its personal attributes is refused by the generated guard `not_erased`"; "A file deletion that fails is retried by the next erasure request". flow-format §4.13: "once an object's own erasure is recorded, a request that writes one of its personal attributes is refused by the generated guard `not_erased`". §6 step 6: "Any failure aborts the whole request and rolls back". storage §9, which the sections under test do not cite, says a re-requested erasure "is idempotent" and does not say what it records.
- **Severity:** changes-outcome
- **Suggested fix:** Say that `not_erased` refuses only a non-absent value and never an erasure, and state what erasing an already-erased object, directly or as a chain member, records.

#### 33. The caller up the cause chain: redacted, but does it get an event
- **Area:** 5
- **Scenario:** `Order.place` on order O creates contact K, passing input `email` into `K.email`, which is personal. K is erased, and the `email` input in O's `place` event is redacted. None of O's attributes holds anything personal.
- **The readings:** (a) A value in O's record was redacted, so O gets an `erased` event. Its version advances, Order subscribers see the change, and an old `expected_version` on O becomes stale. (b) "Value" means attribute value, and only the downstream taint objects get events. O gets nothing.
- **Where the text fails:** DESIGN §8: "It redacts, in every **caller's event** up the erased object's cause chain, the inputs that flowed into the erased object's personal attributes"; "**Every object whose value an erasure redacts gets an event.**" DESIGN §7's table: "on the erased object and on every object whose personal value it redacted". §8's "What else it reaches" list gives events only to the downstream path: "erases what flowed from it into other objects' personal attributes".
- **Severity:** changes-record
- **Suggested fix:** Say whether redacting an input in an object's past event, whether a caller's event or a proposal's, gives that object an `erased` event.

#### 34. Under which declaration an attribute is personal
- **Area:** 5
- **Scenario:** (i) Version 1 declares `Customer.phone` personal and version 2 removes `phone`. The column and history stay. Customer C is erased under version 3. (ii) C is erased under version 1, and version 2 marks `Customer.nickname` personal.
- **The readings:** For (i): (a) erasure reaches the currently "declared personal attributes", so `phone` stays in C's row and in C's version-1 events, readable through `history`; (b) erasure reaches every attribute that was personal under any version C's events were written under. For (ii): (a) nothing happens, and C's nickname stays; (b) the publish redacts it on objects already erased. A harness judging personalness by the current declaration passes (a). One judging across versions fails it under "a personal value still readable after its subject's erasure — on an object, in an event".
- **Where the text fails:** DESIGN §8: "**Erasure** replaces the values of declared personal attributes with absence, on the object and in every event that carried them". P&I §3: "An attribute that is **removed** needs no mapping and gets no DDL. It hides from the declaration and its recorded values stay in history". storage §10: "A removed attribute leaving its column in place is deliberate".
- **Severity:** changes-record
- **Suggested fix:** State that erasure reaches every attribute and input that was personal under any version the object's events were written under, and what a publish adding `personal` does to objects already erased.

#### 35. The `DeclarationChange` lifecycle is not printed
- **Area:** 6, proposals and flow changes.
- **Scenario:** (i) `revise` is requested on a SUBMITTED change. (ii) A `submit` finds a FATAL problem in its dry run. (iii) `withdraw` or `reject` is requested on a DRAFTED change. (iv) A change is superseded by another's publish, and someone calls `get(it, follow)`.
- **The readings:** For (i): (a) refused `unavailable`; (b) allowed and still SUBMITTED, with a report describing the old source; (c) allowed and back to DRAFTED. For (ii): (a) `submit` is refused and the change stays DRAFTED; (b) the change becomes SUBMITTED with `accepted: false`. For (iii): allowed in one engine, refused in another. For (iv): (a) "supersedes" is §8 supersession, a final state naming the publishing change, and `get(follow)` returns it; (b) it is a plain final state, like withdrawn.
- **Where the text fails:** DESIGN §9: "It is a built-in type with a declared lifecycle, printed in the rule set like any type:". The bullets that follow name only `DRAFTED` and `SUBMITTED`: "`submit` moves it to `SUBMITTED` and runs the dry run"; "`reject` and `withdraw` end it, and a publish **supersedes** every other open change drafted against an older version". No state is named after publish, reject, withdraw or supersession, and no transition gives its `from`.
- **Severity:** changes-outcome
- **Suggested fix:** Print the lifecycle: every state, each transition's `from` and `to`, what `submit` does with a fatal report, and whether "supersedes" is §8 supersession.

#### 36. A proposal approval whose request is refused
- **Area:** 6
- **Scenario:** Proposal P is approved and the executed request fails a guard. In variants it fails an invariant, is `unavailable`, or is `stale`.
- **The readings:** (a) The approval commits a proposal event from pending to pending, attaching the verdict. P's version advances and subscribers see it, and the refused request is also an attempt row. (b) The approval is one request and rolls back entirely. Attached then means the attempt row names P, so P gets no event and no version change. Whether non-guard refusals also leave P "pending with the verdict attached" differs as well.
- **Where the text fails:** DESIGN §9: "Approving it executes the recorded request **as the approver**, with the approval as cause, and every guard is re-evaluated at execution under the **current** declaration. A guard that fails leaves the proposal pending with the verdict attached, and the approval returns that verdict, as the request would". §6: "**A request that does not apply**" is written to the attempt log "in its own short transaction after the rollback".
- **Severity:** changes-record
- **Suggested fix:** Say whether a refused approval writes an event on the proposal or only an attempt row, and for which verdicts.

#### 37. "Expiry is derived" with nothing to derive it from
- **Area:** 6
- **Scenario:** A proposal pending for 90 days is approved.
- **The readings:** (a) Proposals have no expiry, and the approval proceeds. (b) The engine derives an expiry and refuses the approval. No bound, attribute or state is named for it to use.
- **Where the text fails:** DESIGN §9: "Expiry is derived, not driven." The lifecycle `pending → executed | rejected | withdrawn | invalidated` has no expired state.
- **Severity:** wording
- **Suggested fix:** Name what expiry derives from and what an expired proposal refuses, or delete the sentence.

#### 38. "an object in a state no transition reaches"
- **Area:** 7, the adversarial harness.
- **Scenario:** Mirror objects, whose type has no transition but `erase`. Imported objects in states only the built-in assertion reaches. Objects of a type a publish removed ("no creation, no transition").
- **The readings:** (a) The built-in assertion counts as a transition. It reaches every state, so this row can never fire. (b) It does not count, so every mirror object fails the run. (c) Reachability is judged under the object's own declaration version, or under the current one. Two harnesses pass and fail the same engine differently.
- **Where the text fails:** harness §1: "| an object in a state no transition reaches | the machine was not the only way in |". DESIGN §8: "The import uses a **built-in** assertion, so no type is unimportable by omission." storage §10: "It is retired: no creation, no transition".
- **Severity:** changes-outcome
- **Suggested fix:** Name the transitions that count (declared ones, declared assertions, the built-in assertion) and the declaration version, and exempt mirrors explicitly or not.

#### 39. "a part alive under a whole in a terminal state" and "a required attribute absent"
- **Area:** 7
- **Scenario:** A non-observation part marked `survives` stays under a whole that entered a final state. A required state attribute is absent under a recorded admission.
- **The readings:** (a) Both fail the run. (b) Surviving parts and admitted required attributes are exempt. The engine behaves the same either way, but the harness verdicts differ.
- **Where the text fails:** harness §1: "| a required attribute absent | a creation wrote less than it declared |" and "| a part alive under a whole in a terminal state | a cascade was skipped or a coverage rule was wrong |". flow-format §4.19: "A superseded whole's parts are cascaded or survive its superseding transition as for any final transition (§4.3)." flow-format §4.13: "An admitted invariant is the type's own, one generated for a state's `required_attributes` included".
- **Severity:** changes-outcome
- **Suggested fix:** Exempt parts that survive by declaration and required attributes an admission covers, or say that they fail.

#### 40. The legacy-interval shape conditions
- **Area:** 7
- **Scenario:** The last legacy interval ends on 1 May and the current interval's `entered` is 5 May, with no silent span declared between them. Separately, a legacy interval runs from 1 to 20 April and a silent span from 10 to 30 April.
- **The readings:** For the gap: (a) under DESIGN §7 the last legacy interval must end where the first recorded one begins, so the gap fails; (b) under the harness only "runs past" fails, and a gap "is reported, not failed". For the partial overlap: (a) it counts as inside the span and fails; (b) "cross" means spanning the whole silent span, so it passes. For "out of order": by position, all of an import's intervals share one event, whereas by occurred time they do not.
- **Where the text fails:** DESIGN §7: "none overlaps another of the same dimension, and the last ends where the object's first recorded interval begins (ADR-0096)". The harness fails "a legacy interval that overlaps another of its dimension, runs past the object's first recorded interval or crosses a declared silent span, or an interval an import or a publish opened that is out of order with the object's others", and says "A gap in legacy history is reported, not failed". DESIGN §11: "the import leaves no legacy row inside one". P&I §4: "no legacy interval may cross one".
- **Severity:** changes-outcome
- **Suggested fix:** State the shape once. Say whether the last legacy interval must meet the first recorded one, whether partial overlap with a silent span fails, and that order is by occurred time.

#### 41. "an erased value written back" and "a refusal missing from the attempt log"
- **Area:** 7
- **Scenario:** (i) After C's erasure, a sync writes a new phone number, not the erased one, to C. (ii) The route-tester's `import_batch` on an owned type is refused. (iii) `maintain` prunes attempt rows during a run.
- **The readings:** For (i): (a) any value written to an erased object's personal attribute fails the run; (b) only the erased value itself fails. For (ii): (a) `import_batch` is an operational call rather than a §6 request, so its refusal is not an attempt row and the row does not apply; (b) the refusal must be logged, so an engine that does not log it fails. For (iii): the pruned rows count as "missing" to one harness and not to another.
- **Where the text fails:** The harness fails "an erased value written back by any request, a sync included" and "a refusal missing from the attempt log". DESIGN §10: "Operational calls that are not object operations are `acknowledge`, which advances a subscription's progress (§7); `publish`". §6: "**A request that does not apply**".
- **Severity:** changes-outcome
- **Suggested fix:** Make the row cover any value written to a personal attribute of an object whose erasure is recorded, and list which operations' refusals the attempt log must hold, and whether pruning is excluded.

---

### Counts

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 26 | 1, 3, 4, 6, 7, 8, 9, 10, 11, 12, 13, 16, 17, 19, 21, 23, 24, 25, 29, 30, 32, 35, 38, 39, 40, 41 |
| changes-record | 14 | 2, 5, 14, 15, 18, 20, 22, 26, 27, 28, 31, 33, 34, 36 |
| wording | 1 | 37 |
| **Total** | **41** | |

By area: datapoints 5, publish 11, import 11, delivery 3, erasure 4, lifecycles 3, harness 4.
