# Third cold reading, of the text written for ADR-0147, 2026-10-04

Status: **disposed**, 2026-10-04: the contradictions and regressions fixed and the rest recorded as open questions (§5), as ADR-0148 records. At the author's request ("run another cold reading over today's text"), four readers with no context read the text written on 2026-10-04 to state ADR-0147's decisions, each asked whether two careful builders following it could build engines that behave differently, and to watch for a rule stated twice in two places differently. They found 139 findings.

**Method.** As in `cold-reading-2-review.md`: each reader was given only what a builder gets for its area, pointed at the passages citing ADR-0147 and told to read their whole sections, and barred from the decision records, both earlier review records, the defect register, `TODO.md`, the lessons, the code and git history; each checked its quotations against the files by script. Their reports follow as they wrote them, each opening heading renamed to place it in this record.

- **§1, Recording, correcting and dating a request, evaluators, corrections and the examples format** (29 findings).
- **§2, erasure, proposals and flow changes, delivery, the API's shapes and the model's details** (42 findings).
- **§3, publishing with a migration, the import, the harness and an engine upgrade** (46 findings).
- **§4, metrics and time in a state** (22 findings).

## 1. Recording, correcting and dating a request, evaluators, corrections and the examples format, as the reader wrote it

**What I read.** `docs/design/flow-format.md` from its start through §11.5, which covers the assigned §2, §3, §4.4, §4.8, §4.13, §4.18, §5, §6 and §11. Because I read the file in one sequential pass, I also read §1, §4.1–§4.3, §4.5–§4.7, §4.9–§4.12, §4.14–§4.17, §4.19 and §7–§10. I did not read Appendix A or Appendix B. In `docs/DESIGN.md` I read §5.5, §5.11, §6 and §7. In `docs/design/declaration-syntax.md` I read §4.2, §6.2, §6.4, §6.8 and all of §6.9 (not only `.corrected_by` and `.held`), plus §8.2 and §8.3. The same sequential reads also took in §6.3 and §6.5–§6.7 (lines 714–790) and §8.1 (lines 1284–1309). I found check 61's row in §10 by grep, and check 58's row came up in the same grep. I read all of `examples.schema.json`. From `flow.schema.json` I read only the parts that bear on my area: `duration`, the type's `observations` key, the transition definition (`corrects`, `backdating_limit`), `observation`, `evaluatorFunction` and `combinedMetric`. I read the example modules `service.yaml`, `service.examples.yaml`, `reimbursement.yaml`, `reimbursement.examples.yaml`, `people.yaml`, `customers.yaml` and `customers.examples.yaml` in full.

**Where I left those sections.** I followed three citations out of them:
- `DESIGN.md` §5.12, the `recorded_from` and `gaps` bullets at lines 400–404, which I reached from `.recorded_from` in declaration-syntax §6.9.
- `DESIGN.md` §9, the Proposal paragraph and its Approval bullet at lines 598–600, which I reached from the `DESIGN.md` §9 citation in flow-format §11.3.
- `declaration-syntax.md` §8.1 on `.created_at`, which declaration-syntax §6.9 cites as "(§8.1)".

I also listed the file names in `docs/design/flow-format/examples/` and counted the `ADR-0147` lines in each file. Both were done by grep, without reading content.

**What I did not read.** Nothing under `docs/adr/`, `defects.md`, `TODO.md`, `LESSONS.md`, any review or cold-draft file, the case studies, `scripts/`, `.remember/`, and no git history.

**How I checked.** A Python script asserts that every quotation in this report (each one between “ and ”) occurs character for character in the file named after it. All of them pass. All time arithmetic was done in Python: the examples' times, and the made-up cases below, all from the examples' clock origin `2026-01-01T00:00:00Z`. I also loaded sample scalars with PyYAML 5.4.1 (`yaml.safe_load`) for finding 26.

### What is determined

- **A recording and a correction.** The generated guards of a recording run in the order `occurred_within`, `subject_open`, `corrects_current`, `subject_owned`, each with its stated remedy and named objects.
  - A `corrects` naming an observation of another subject is refused as `invalid input`, before the guards.
  - A correction that supplies any occurred time is refused by `occurred_within` (`self_serviceable`).
  - A correction is exempt from the kind's bound and from `subject_open`. It takes the corrected observation's `.occurred_at`.
  - A correction requested directly has its event, `.created_at` and `.recorded_at` at the request's recorded time.
  - `subject_open` reads the subject as the request found it, and a subject the request created as it stands. `corrects_current` reads the state produced so far.
- **Dating.**
  - The bound is inclusive ("no further back than") and an occurred time may not be after the recorded time. Only the requested transition checks the bound.
  - Every caused transition records the same occurred time and checks the "not before the current interval" half on its own object. An internal transition that changes no state or tracked value checks nothing, even if it is dated before its object was created.
  - A call's failure gives `call refused` with the clause `<Type>.occurred_within`. A cascade's failure gives the part's own verdict.
  - Assertions and erasures are never backdated.
  - `now` is the recorded time in every expression, including effects and defaults, while `entered_at` reads occurred time.
- **The service examples.** Their times all agree with the text. Computed: `a_late_start_is_dated_when_it_happened` gives now `2026-01-02T01:00Z` and occurred `2026-01-01T01:00Z`, which is within 2 days and not before OPEN at `2026-01-01T00:00Z`, so it applies. `…further_back_than_two_days` reaches back 3 days and is refused. `…before_the_job_opened` gives occurred `2025-12-31T23:00Z`, before OPEN, so it is refused. `an_inspection_is_dated_when_it_was_done` reaches back 2 days, within 7. `…more_than_a_week_late…` reaches back 8 days and is refused.
- **Evaluators.**
  - They are consulted before the transaction, never inside it.
  - An absent argument makes no call, and the guard is unknown with the condition's remedy.
  - An evaluator that does not answer gives `temporal`.
  - An answer with no as-of time is dated at its consultation.
  - An example's stub is live.
- **Attribute corrections.**
  - A correction relabels the current interval of each listed attribute from its start, opens no interval for those attributes, and merges with an equal interval before it.
  - Its own transition row credits the corrected value.
  - It is a write for `changed_since`, and its events carry `corrected` provenance.
  - It must write exactly what it lists and must take a `reason` input.
- **The examples format.** The clock starts at `2026-01-01T00:00:00Z` and `after` advances it before its step. The timestamp forms are `…T09:00Z`, `…T09:00:30Z` and `now ± <duration>`, where `now` is the step's clock. A creation's step names its type, and a recording is requested as `record` with `subject`, the kind's fields and optionally `corrects`.

### Findings

#### 1. A function with no `fresh`: a zero bound, or the stamp ignored

- **Construct:** evaluator freshness when no `fresh` is declared.
- **Case:** `customers.yaml`'s `xero.contact_exists` declares no `fresh`. A `link_xero` request has `now = 2026-10-04T10:00:02Z`, and the adapter returns `satisfied` stamped as of `10:00:00Z`. That could be its honest stamp of a cached answer, or simply the evaluator's own clock.
- **The readings:**
  - (a) The bound is zero, compared with `now` as for a declared `fresh`. Two seconds is more than zero, so the guard is unknown and refused with `unsatisfied` / `temporal`.
  - (b) Without `fresh` the stamp is ignored ("whatever its stamp"), so the answer is used and the request applies. Under (b) the same cached answer stamped an hour old is used, while a function declaring `fresh: 30 min` refuses it. The stricter declaration, zero, behaves more loosely than a 30-minute one.
- **Where the text fails:**
  - flow-format §4.18: “the oldest an answer may be, by the as-of time the evaluator reports for it compared with the request's `now`”, then “without it the bound is zero, so only an answer obtained for this request is used, whatever its stamp”.
  - DESIGN §5.5: “a function's `fresh` bounds its age, zero where none is declared, so only an answer obtained for this request is used, and past it the guard is unknown, refused as `temporal`”.
  - flow.schema.json: “without it, only a live answer is used.”
- **Severity:** changes-outcome.
- **Suggested fix:** State one rule for a function without `fresh`: either its stamp is compared with a zero bound (and against which instant, see finding 6), or its stamp is not read. State which one the schema and DESIGN repeat.

#### 2. An unanswered evaluator's `temporal` remedy cannot be written in an example

- **Construct:** the remedy of a guard whose evaluator does not answer, against the examples rule that an expected remedy must be the declared one.
- **Case:** In `reimbursement.examples.yaml`, an example given `recorded` stubs `xero.bill_matches: unanswered` and requests `verify_xero_record`. The guard `bill_matches_xero` declares `remedy: delegable`.
- **The readings:**
  - (a) Step 3 holds the expected remedy to the declared one. `expect: {verdict: unsatisfied, clause: bill_matches_xero, remedy: temporal}` is then refused at step 3. `remedy: delegable` passes step 3, but the run returns `temporal` and the example fails. No valid example of an unanswered evaluator exists.
  - (b) Step 3 allows `temporal` for an evaluator's guard. The `temporal` example passes.
- **Where the text fails:**
  - flow-format §11.3: “the last refusing the guard as unknown with the remedy `temporal`”.
  - flow-format §11.4: “An `unsatisfied` expectation MUST name its clause and remedy, and the remedy MUST be the one the clause declares”.
  - flow-format §4.7: “The declared class is carried as written by every refusal the condition makes, whatever state it fails from; an engine neither infers nor corrects it”, against §4.18: “An evaluator that does not answer refuses the guard as unsatisfied and unknown, with the remedy `temporal`, whatever the condition declares”.
- **Severity:** changes-outcome (the examples check accepts or refuses).
- **Suggested fix:** State the exceptions to "the remedy the clause declares", namely an evaluator not answering and a verdict too old, in §4.7 and in §11.4's step-3 rule.

#### 3. A guard reached during execution that committed state did not reach has no consultation

- **Construct:** consulting evaluators "as committed state stands", per element.
- **Case (made up):**
  - `Order.release` has the effect `[call: {target: inputs.line, transition: ready}, foreach: {item: l, array: lines, where: "l.state == Line.READY", limit: 50, steps: [call: {target: l, transition: ship}]}]`.
  - `Line.ship` has a guard `carrier.accepts(sku)`.
  - In committed state, line L2 is DRAFT. The `where` leaves it out, so its `ship` guard is not consulted.
  - In the transaction, the first step makes L2 READY. The loop's `where` is evaluated when the loop starts, so it now selects L2 and reaches `ship`, whose guard has no verdict.
  - The same happens for an element that an earlier `create` step of the same effect added.
- **The readings:**
  - (a) The guard has no answer, so it is treated as an evaluator that did not answer: `call refused`, clause `Line.carrier_accepts`, `temporal`.
  - (b) It is treated as "the world moved": the request consults again, again "as committed state stands", which never selects L2. Retries run out and the request is refused `stale` / `temporal`.
  - (c) The engine consults inside the transaction and the request applies, which the text forbids.
  - (d) It is a publish error. But check 61 covers arguments, not a `where` or a created element.
- **Where the text fails:**
  - flow-format §4.18: “a part a cascade passes over and a call a `where` leaves out are not consulted”.
  - flow-format §4.8: “The collection and `where` are evaluated once, when the loop starts”.
  - DESIGN §5.5: “Evaluators are consulted **before** the write transaction opens, never inside it”.
  - Nothing states what a guard does when execution reaches it but committed state did not.
- **Severity:** changes-outcome.
- **Suggested fix:** State what an evaluator guard reached without a consultation does: refused (with which verdict and remedy), or ruled out at publish. If it is ruled out, extend check 61 to a `where` or collection that an earlier step can change.

#### 4. Which transitions a request "would reach", and so which consultations it makes and which stubs an example needs

- **Construct:** the scope of up-front consultation.
- **Case:** In `reimbursement.examples.yaml`, an example given `approved` (claim in APPROVED) requests `verify_xero_record`, which is taken only from RECORDED, and stubs no evaluator. It expects `{verdict: unavailable}`.
- **The readings:**
  - (a) "Would reach as committed state stands" means a dry run: the transition is unavailable, so nothing is consulted, no stub is needed, the verdict is `unavailable`, and the example passes.
  - (b) Every guard of the requested transition is consulted up front ("named by any guard in the request"). `xero.bill_matches` has no stub, so the example fails.
  - The same split decides whether a call is consulted when the requested transition's own non-evaluator guard fails in committed state. It also decides what an attempt row lists as "the evaluator verdicts it consulted".
- **Where the text fails:**
  - DESIGN §6: “External evaluators and metrics named by any guard in the request are consulted **first, outside the transaction**”, then “the guards of the transitions the request would reach as committed state stands”.
  - flow-format §4.18: “a function no consultation reaches needs no stub in an example”.
  - flow-format §11.3: “A call with no stubbed answer fails the example.”
- **Severity:** changes-outcome.
- **Suggested fix:** State whether "would reach" applies the checks of steps 3–4 and the guards that do not consult anything, or is a static closure over the effect. Say which of the requested transition's guards are consulted when the request is refused before its guards.

#### 5. Check 61: the general rule against its enumerated forms

- **Construct:** the publish check on evaluator arguments written earlier in the same request.
- **Case (made up):**
  - `Shipment.dispatch` has the effect `[call: {target: label, transition: print}, call: {target: booking, transition: book}]`.
  - `Label.print` writes `code`.
  - `Booking.book` has a guard `carrier.code_valid(label.code)`, where `Booking.label` references the same label.
  - The argument reads a value written earlier in the request by a sibling call's outcome. It was not written by `book`'s caller's own outcome, and not before a composite cascade.
- **The readings:**
  - (a) By the general rule, this is a publish error under check 61.
  - (b) By the enumerated forms, which name only a caller binding from its own outcome and a member written by "a caller's or a whole's outcome … before a cascade", it publishes. At run time the argument is resolved as the old `code` before the transaction and the new one inside it. That mismatch repeats on every retry, so the request is refused `stale` / `temporal`.
  - declaration-syntax §6.9 narrows the rule further, to what "the calling outcome writes".
- **Where the text fails:**
  - flow-format §4.18: “an argument that reads a value an outcome applied earlier in the same request writes, which could never agree before the transaction, is refused at publish”.
  - declaration-syntax §10, check 61: “an input a caller binds from a member its own outcome sets, in a transition reached by a `call` or `create`, or a member of another object a caller's or a whole's outcome wrote before a cascade reached the guard”.
  - declaration-syntax §6.9: “An argument that reads a value the calling outcome writes could never agree before the transaction, so it is a publish error (check 61).”
- **Severity:** changes-outcome (publish refused, or runtime `stale`).
- **Suggested fix:** State whether the colon list is exhaustive or illustrative. Say whether "outcome" includes the writes of transitions it caused, and whether "cascade" there means any caused transition.

#### 6. When `now` (the recorded time) is fixed is not stated

- **Construct:** the instant `now` stands for, relative to consultation, the transaction, its retries and the attempt row.
- **Case:**
  - (i) A function without `fresh` returns an answer with no as-of time, so it is "taken as of its consultation" at 10:00:00.4. The transaction then starts at 10:00:01.
  - (ii) Requests A and B on one object, neither backdated. A fixes `now = 10:00:01` and commits first. B fixed `now = 10:00:00` before waiting on the lock, and its external transition then commits. B's occurred time precedes A's state interval start.
- **The readings:**
  - (a) `now` is fixed on arrival, before consultation. In (i) the answer's age is ≤ 0, so it is fresh. In (ii) B's occurred time precedes the current interval: B is refused by `occurred_within`, with `self_serviceable`, though it supplied no time. Or, if a builder skips the check when no time is supplied, it writes a negative interval.
  - (b) `now` is fixed when the transaction starts, or again on each retry. In (i) the age is about 0.6 s, which is more than a zero bound, so it is refused `temporal` under finding 1(a). In (ii) the clock is monotonic under the lock and B applies.
  - The same choice decides an attempt row's `at`: the request's `now`, or when the row is inserted after the rollback.
  - The text also gives two reference instants for "too old": `now` for an evaluator, commit for a metric.
- **Where the text fails:**
  - flow-format §5: “`now` is the time the request is recorded”.
  - DESIGN §6: “the `now` the request fixed”, and “the row itself dated by its recorded time, `at`”.
  - declaration-syntax §6.9: “refuses a value older than the bound when the transaction commits”.
- **Severity:** changes-outcome.
- **Suggested fix:** State when a request fixes `now`: on arrival, at transaction start, or anew on each retry. State that the interval check, freshness and the attempt row's `at` all read that one instant.

#### 7. Idempotency: does a defaulted occurred time belong to "what the key identifies"?

- **Construct:** the occurred time and idempotency.
- **Case:** Request R with key K and no `occurred_at` applies at 10:00:00, so its occurred time defaults to 10:00:00. The caller retries R with K and no `occurred_at` at 10:00:05.
- **The readings:**
  - (a) The comparison is on the request as sent, where `occurred_at` is absent both times. The requests are the same, and the original result is returned as a replay.
  - (b) The comparison is on the effective occurred time, 10:00:00 against 10:00:05. The requests differ, so "a key applied before to a different request" is a fault and nothing is returned.
- **Where the text fails:**
  - DESIGN §6: “the time the change happened, its occurred time (§7), which belongs to what the key identifies”, and “A key applied before to a different request is a fault, not a verdict”.
  - declaration-syntax §4.2: “Without the marking on the requested transition, the occurred time is the recorded time”.
- **Severity:** changes-outcome.
- **Suggested fix:** State that the key identifies the supplied `occurred_at` field, absent included, and not the time defaulted from the recorded time. Or state the reverse.

#### 8. An attempt row's occurred time when the request supplied none

- **Construct:** dating an attempt row.
- **Case:** `customers.examples.yaml`'s `an_open_order_keeps_the_customer_open`: `close` is refused by `no_open_orders` at `2026-01-01T00:00:00Z` and supplied no `occurred_at`.
- **The readings:**
  - (a) The row's occurred time is the recorded time, `2026-01-01T00:00:00Z`, as an applied request's would be.
  - (b) The row's occurred time is absent, since the request "gave" none.
- **Where the text fails:** DESIGN §6: “every non-personal input and its occurred time, the row itself dated by its recorded time, `at`”.
- **Severity:** changes-value.
- **Suggested fix:** State the attempt row's occurred time for a request that supplied none, and for one refused before a recorded time could apply.

#### 9. A caused transition: is the interval half checked with its generated guards or when its outcome writes?

- **Construct:** when a called or cascaded transition checks that the occurred time does not precede the current interval, relative to its own guards.
- **Case (made up), times computed:**
  - `Order.ship` (OPEN→SHIPPED, `backdating_limit: 2 days`) has the effect `call: {target: shipment, transition: send}`.
  - `Shipment.send` (PACKED→SENT) has the guard `labelled: deny`, with remedy `unreachable_from_here`.
  - The order was opened at `2026-01-01T00:00Z`. The shipment was PACKED at `05:00Z` with no label.
  - `ship` is requested at `06:00Z` with `occurred_at: now - 2 h`, which is `04:00Z`. That is within the bound and not before OPEN, but before PACKED.
- **The readings:**
  - (a) A caused transition's state is checked with its generated guards, as the requested transition's is: `call_refused`, clause `Shipment.occurred_within`, `self_serviceable`.
  - (b) "When it runs" means when it writes its state, after its own guards: `call_refused`, clause `Shipment.labelled`, `unreachable_from_here`.
- **Where the text fails:**
  - flow-format §4.8: “and checks, when it runs, that the time does not precede the current interval”, then “the requested transition's state and the tracked values its inputs write are checked with the generated guards”, which fixes the order only for the requested transition.
  - declaration-syntax §4.2: “Every transition the request takes checks the second half against what it changes on its own object, when it runs”.
- **Severity:** changes-outcome (clause and remedy).
- **Suggested fix:** State that a caused transition checks its state and its input-written tracked values with its generated guards, and its effect-computed values when the step writes them. Or state the order that applies instead.

#### 10. "Ends different" decided at the step that writes, before the transition has ended

- **Construct:** when "changes" is decided for a tracked value an effect computes.
- **Case (made up):** An internal transition with `backdating_limit: 2 days` has the effect `[assign: {location: stage, expr: Stage.REVIEW}, assign: {location: stage, expr: inputs.back_to}]`, where `stage` is an enumeration and `back_to` equals the current `stage`, A. It is requested with an occurred time before A's interval start.
- **The readings:**
  - (a) The check is made at the first step, which writes REVIEW ≠ A: refused, `occurred_within`.
  - (b) The value "ends" A, so it is unchanged and nothing is checked: the request applies.
- **Where the text fails:** flow-format §4.8: “a value it changes being one that ends different” against “and a tracked value an effect computes when the step writes it, after the transition's own guards”. declaration-syntax §4.2 says the same.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether "ends different" compares the value before and after the step, or before and after the transition. If it is the latter, state that the check runs when the transition's effect has finished.

#### 11. A backdated attribute correction: does it check the corrected attribute's current interval?

- **Construct:** the interval half of `occurred_within` on a `corrects` transition.
- **Case (made up), times computed:**
  - `Job.correct_engineer` is internal, with `corrects: [engineer]`, `required_inputs: [engineer]`, an input `reason`, and `backdating_limit: 2 days`.
  - `engineer` was ana from `00:00Z`, and ben from `02:00Z` (the current interval).
  - The correction is requested at `05:00Z` with `occurred_at: now - 4 h`, which is `01:00Z`.
- **The readings:**
  - (a) The corrected attribute "ends different", so it is a value the transition changes. `01:00Z` precedes ben's interval start, so the request is refused: `unsatisfied`, `occurred_within`.
  - (b) A correction opens no interval and its occurred time "dates its event only". Nothing can go negative, nothing is checked, and the request applies.
- **Where the text fails:**
  - flow-format §4.8: “a value it changes being one that ends different”.
  - flow-format §4.13: “and opens no interval of its own” and “its own occurred time dates its event only”.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether a correction's listed attributes count as values it changes for `occurred_within`.

#### 12. An external correction: "opens no interval" and "dates its event only", against the state span it opens

- **Construct:** a correction that is an external transition.
- **Case (made up):**
  - `do fix_and_close REVIEW -> CLOSED corrects engineer`, with `backdating_limit: 1 day`.
  - It is requested at `2026-01-02T00:00Z` with `occurred_at` `2026-01-01T12:00Z`, after the REVIEW span began.
- **The readings:**
  - (a) The CLOSED span opens at the occurred time, `12:00Z`, as every state change's does. "No interval of its own" refers only to the corrected attributes.
  - (b) "Its own occurred time dates its event only", so the CLOSED span opens at the recorded time, `2026-01-02T00:00Z`.
  - Separately, does the CLOSED span, whose `.held(engineer)` was never anything but the corrected value, carry `.corrected_by`? Builders could go either way.
- **Where the text fails:**
  - flow-format §4.13: “**A correction** is an internal or external transition marked `corrects: [<attribute>, …]`”, with “and opens no interval of its own” and “its own occurred time dates its event only”.
  - DESIGN §6: “update the interval index for every changed state and tracked value, using the request's occurred time, which every transition it takes records”.
- **Severity:** changes-value.
- **Suggested fix:** Restrict "opens no interval" and "dates its event only" to the listed attributes. Then state how the state span of an external correction is dated and whether it carries `.corrected_by`.

#### 13. Which rows carry `.corrected_by`: "relabels" against "changes"

- **Construct:** `.corrected_by` on interval rows.
- **Case:** `engineer` is ana, and a correction with `corrects: [engineer]` writes ana, the same value. Or, for a value merge, see finding 14.
- **The readings:**
  - (a) The correction relabels the current interval, so that row carries `.corrected_by`.
  - (b) No row's value or `.held` changed, so no row carries it.
- **Where the text fails:**
  - flow-format §4.13: “every interval row it relabels carries `.corrected_by`” against, later in the same paragraph, “every interval row whose value or `.held` it changes, a state span included, carries the latest correction that reached it as `.corrected_by`”.
  - declaration-syntax §6.9: “`.corrected_by`, on an interval row, is the event of the latest correction that changed its value or its `.held`, absent where none did”.
  - "Reached" and "changed" are a third pair.
- **Severity:** changes-value.
- **Suggested fix:** Use one criterion in all three places, "changed its value or its `.held`", and say what a correction to the value already held does to the index.

#### 14. The row that results from a merge, and a merge with a legacy interval

- **Construct:** "a relabelled interval equal in value to the one before it merges with it".
- **Case:**
  - Engineer intervals are [T0,T1) ana, recorded by a request whose actor was human, and [T1,now) ben, recorded by a request whose actor was an agent.
  - Correction C2 says the current engineer is ana. The earlier row had `.corrected_by` = C1.
  - In a variant, [T0,T1) ana is a legacy interval from a port.
- **The readings:**
  - (a) The merged row is the earlier row extended. It keeps `.entered_by_kind = human`, its transition and its version, and `.corrected_by = C1`, since C2 did not change its value.
  - (b) The merged row is a new row. It carries `.corrected_by = C2`, as the relabelled part did.
  - In the legacy variant:
    - (a') The current row merges into a read-only legacy row, which changes imported history or makes a row partly legacy.
    - (b') There is no merge with a legacy row. Two adjacent "ana" rows remain, and `handoffs` counts a change of holder.
- **Where the text fails:**
  - flow-format §4.13: “a relabelled interval equal in value to the one before it merges with it, so a correction never counts as a change of holder”.
  - DESIGN §7 gives every row “entry and exit by position and by occurred time, the transition that set it, the actor who made the change and its kind, and the declaration version”, and legacy rows are “imported history, read-only and marked legacy”.
- **Severity:** changes-value.
- **Suggested fix:** State which row survives a merge and which of its members (`.corrected_by`, `.entered_by_kind`, `.legacy`, its version) it keeps, and whether a recorded interval merges with a legacy one.

#### 15. "From when it was written" against "from that interval's start"

- **Construct:** where an attribute correction's relabel begins.
- **Case:** A port's import event, at the cutover time P, opens `engineer`'s current interval at the legacy time L < P: ben from L. A correction to carl follows.
  - A second case: ana was written at T0, and a reassign at T1 wrote ana again (the same value, so no new interval). Then a correction follows.
- **The readings:**
  - (a) The relabel runs from the interval's start: L in the first case, T0 in the second.
  - (b) It runs from "when the wrong value was written": P (the import event) in the first case. In the second it is T1 (the last write), which splits the interval at T1.
  - The readings differ in `.held` on state spans begun in [L,P) or [T0,T1), and in `handoffs`.
- **Where the text fails:**
  - flow-format §4.13: “the value each attribute it lists holds now is read, from when it was written, as the corrected one”, and later “It relabels the current interval of each attribute it lists, from that interval's start”.
  - declaration-syntax §6.4: “the index of who held what reads the corrected value from when the wrong one was written”.
  - DESIGN §7: “**A port.** The import's event opens each current interval at the time the legacy record says the object took that value”.
- **Severity:** changes-value.
- **Suggested fix:** Replace "from when it was written" with "from the start of the current interval", or state that the two coincide by definition.

#### 16. "The latest correction": latest by position or by occurred time

- **Construct:** `.corrected_by` when two corrections reach one row.
- **Case:** Correction transitions with a `backdating_limit`. C1 is recorded at t10 with occurred time t9. C2 is recorded at t11 with occurred time t8. Both relabel the same current interval.
- **The readings:**
  - (a) Latest by log order: C2.
  - (b) Latest by occurred time: C1.
- **Where the text fails:** flow-format §4.13: “carries the latest correction that reached it”.
- **Severity:** changes-value.
- **Suggested fix:** State the order in which "latest" is taken.

#### 17. An observation correction recorded by an effect in a backdated request: when its event is dated

- **Construct:** a correction's own event and `.created_at`, against "every transition the request takes records the same occurred time".
- **Case (made up), times computed:** `ServiceJob.start` (`backdating_limit: 2 days`) gains the effect step `create: {type: InspectionResult, transition: record, inputs: {subject: this, check: …, outcome: …, corrects: inputs.wrong}}`. It is requested at `2026-01-02T01:00Z` with `occurred_at: now - 1 day`, which is `2026-01-01T01:00Z`.
- **The readings:**
  - (a) Every transition of the request records the same occurred time. The correction's event and `.created_at` are `2026-01-01T01:00Z`.
  - (b) "Dated when the correction is made" means the recorded time. Its event and `.created_at` are `2026-01-02T01:00Z`.
  - In both readings, `.occurred_at` is the corrected observation's.
- **Where the text fails:**
  - flow-format §4.4: “its own event and `.created_at` being dated when the correction is made”.
  - flow-format §4.8: “Every transition the request takes, the requested one and each it calls, creates or cascades to, records the same occurred time, a recording an effect makes included”.
  - declaration-syntax §4.2: “and a correction an effect records taking its corrected observation's `.occurred_at`”.
- **Severity:** changes-value.
- **Suggested fix:** State whether "when the correction is made" is the request's occurred time or its recorded time.

#### 18. A recording an effect makes, on a kind with no `max_recording_delay`

- **Construct:** the occurred time of a caused recording when its kind has no bound.
- **Case (made up), times computed:** The backdated `start` of finding 17 records instead, through its effect, an observation of a kind with no `max_recording_delay`. It is requested at `2026-01-02T01:00Z` with `occurred_at` `2026-01-01T01:00Z`.
- **The readings:**
  - (a) Every transition records the request's occurred time, so `.occurred_at` is `2026-01-01T01:00Z`.
  - (b) On a kind without the bound, "the occurred time is the recorded time", so it is `2026-01-02T01:00Z`.
- **Where the text fails:** flow-format §4.4: “on a kind without it the occurred time is the recorded time, a request supplying one being refused by `occurred_within`” against §4.8: “a recording an effect makes included, whose kind's `max_recording_delay` does not hold it”.
- **Severity:** changes-value.
- **Suggested fix:** Limit §4.4's sentence to a recording that is the requested transition, or state that it also governs recordings made by effects.

#### 19. Must a correction supply `subject`?

- **Construct:** the subject of an observation correction.
- **Case:** In `service.examples.yaml`, the correction step of `a_corrected_result_replaces_the_mistake`, written without `subject`: `inputs: { check: battery, outcome: CheckOutcome.PASS, corrects: mistake }`.
- **The readings:**
  - (a) `subject` is a required input of `record`, so leaving it out is `invalid input`.
  - (b) The correction "names the corrected observation's subject as its own", so the engine takes the subject from `mistake` and the step applies.
- **Where the text fails:**
  - flow-format §4.4: “A recording takes `subject`, its fields and optionally `corrects`”.
  - DESIGN §5.11: “It names the corrected observation's subject as its own, a request naming another being refused as `invalid input`”.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether a correction must supply `subject`, or whether it may be left out and is taken from the corrected observation.

#### 20. `recorded_from` of a backdated creation

- **Construct:** coverage of an object created with an occurred time earlier than its recorded time.
- **Case (made up), times computed:** A creation with `backdating_limit: 2 days` is requested at `2026-01-02T00:00Z` with `occurred_at: now - 1 day`. Its `.created_at` is `2026-01-01T00:00Z`.
- **The readings:**
  - (a) `recorded_from` is `2026-01-01T00:00Z`, the creation's occurred time, so no gap.
  - (b) `recorded_from` is `2026-01-02T00:00Z`, when the record began. A metric window over that day then reports the object in `gaps`, as one "that existed then".
- **Where the text fails:**
  - DESIGN §5.12: “Each object records the time from which its record is continuous: its creation”.
  - declaration-syntax §8.1: “`.created_at` is when the creation **happened**: the event's occurred time for a backdated creation”.
  - DESIGN §5.12: “a window beginning before the `recorded_from` of an object that existed then”.
- **Severity:** changes-value.
- **Suggested fix:** State whether "its creation" means the creation's occurred time or its recorded time.

#### 21. `expect.occurred_at` for a correction, and for a refusal

- **Construct:** the expectation key `occurred_at`.
- **Case:** Given `job_in_progress`, `mistake` is recorded at `2026-01-01T01:00Z`. A correction is then requested `after: 1 h`, at `02:00Z`, with `expect: {verdict: applied, state: RECORDED, occurred_at: now - 1 h}`. A second example puts `occurred_at` on an `unsatisfied` expectation.
- **The readings:**
  - (a) "The occurred time the request recorded" is the event's, `02:00Z`, which is `now`. The first example fails.
  - (b) It is the recorded observation's `.occurred_at`, `01:00Z`, which is `now - 1 h`. The first example passes.
  - For the refusal:
    - (a') The table says "for `applied`", so step 3 refuses `occurred_at` on a refusal.
    - (b') The rule lists only "state, values or warnings", so it is accepted and compared with the attempt row's time.
- **Where the text fails:**
  - flow-format §11.4: “for `applied`, optional: the occurred time the request recorded, a timestamp such as `now - 1 day`”, and “a refusal names no state, values or warnings”.
  - flow-format §4.4: “its own event and `.created_at` being dated when the correction is made”.
- **Severity:** changes-outcome.
- **Suggested fix:** State that `occurred_at` is the event's occurred time, or the observation's `.occurred_at` for a recording. Add `occurred_at` and `cascaded` to the keys a refusal may not name.

#### 22. `values: { occurred_at: … }` is outside what `values` may name

- **Construct:** the members an expectation's `values` may name.
- **Case:** `service.examples.yaml`'s `an_inspection_is_dated_when_it_was_done` expects `values: { occurred_at: now - 2 days }` on an `InspectionResult`.
- **The readings:**
  - (a) `occurred_at` is not an attribute (a field), not a derived attribute, and not one of §5's members of every object. Step 3 refuses it as an expectation the request cannot give, and the examples file fails.
  - (b) A member every observation has counts. It is accepted, compared with `2026-01-02T01:00Z`, and passes.
- **Where the text fails:**
  - flow-format §11.4: “for `applied`, optional: attributes, derived attributes or the members every object has (§5)”.
  - flow-format §5: “Every object also has `id`, `open` (true while its state is neither `closed` nor final), `created_at`, `created_by_kind`, `recorded_from` and `declaration_version`”.
  - declaration-syntax §6.8: “Every observation also has `.recorded_at`”.
  - service.examples.yaml: “values: { occurred_at: now - 2 days }”.
- **Severity:** changes-outcome (the examples check).
- **Suggested fix:** Add the members every observation has (`occurred_at`, `recorded_at`, `recorded_by_kind`, `subject`, `corrects`) to what `values` may name, or use `expect.occurred_at` in the example.

#### 23. `values` naming `id` or `declaration_version`, whose values an example cannot know

- **Construct:** built-in members newly admitted in `values`.
- **Case:** `expect: { …, values: { id: job, declaration_version: 1 } }`.
- **The readings:**
  - (a) `id` is written as the alias, which resolves to the object, and the version under check is 1. The example passes.
  - (b) `id` is an `identity`, written "by the identity itself", but it comes from an injected source whose form is not stated. The version under check is not numbered. Neither can be written, so step 3 refuses, or the run fails.
- **Where the text fails:**
  - flow-format §11.3: “object ids come from an injected source”, and “an `identity`, by the identity itself, such as `ana` or `acct-dev`”.
  - flow-format §11.2: “one that holds declaration version 0, with its operator, and the version under check”.
- **Severity:** changes-outcome.
- **Suggested fix:** State how `id` is written in `values` (by alias) and the number the version under check takes in an example's store.

#### 24. Is `forget` generated on every observation kind, or only on kinds with personal fields?

- **Construct:** the generated `forget` in a step and in `cascaded`.
- **Case:** Given `job_in_progress` in `service.examples.yaml`, an `InspectionResult` is recorded (it has no personal field). The request is `{object: job, transition: forget, inputs: {reason: asked}}`, with `expect: {verdict: applied, state: WORKING, cascaded: [InspectionResult.forget]}`.
- **The readings:**
  - (a) Every kind has `forget`, and the subject's erasure runs it on every observation. The example passes.
  - (b) Only a kind with personal fields has one. `cascaded` should be `[]`, the example fails, and step 3 may refuse `InspectionResult.forget` as unresolvable, as it would a step `{object: r, transition: forget}` on an `InspectionResult`.
  - Separately, the generated `forget`'s `reason` has no stated type, and nothing says what the subject's erasure passes it.
- **Where the text fails:**
  - flow-format §11.3: “`record` on an observation kind and `forget` on a kind with personal fields”, against §11.4: “the generated `forget` an erasure takes on each observation”.
  - declaration-syntax §6.8: “an erasure, `erase forget`, taking a `reason`, which erases the observation's personal fields”, and “The subject's `erase` runs `forget` on every observation in its collection, corrected ones included”.
  - DESIGN §5.11: “each kind has a generated `forget`”.
- **Severity:** changes-outcome.
- **Suggested fix:** Make §11.3 agree with §6.8, saying either every kind or only kinds with personal fields. State the type of `forget`'s `reason` and what the subject's erasure passes as it.

#### 25. A proposal's occurred time when none is given, what filing refuses with, and whether approval re-checks the bound

- **Construct:** `occurred_at` on `propose`, and the dating of an approved request.
- **Case (times computed):**
  - (i) A proposable, non-backdatable transition is filed at `2026-01-01T00:00Z` with no `occurred_at` and approved at `02:00Z`. The expectation says `occurred_at: now`.
  - (ii) The same transition is filed with an `occurred_at`.
  - (iii) A transition with `backdating_limit: 2 days` is filed at `2026-01-02T00:00Z` with `occurred_at: now - 1 day`, and approved 3 days later, when the time is 4 days back.
- **The readings:**
  - (i)
    - (a) The proposal "holds … occurred time", which is the filing time, `00:00Z`. Executed with it, the example fails.
    - (b) A non-backdatable transition's occurred time is the recorded time of its execution, `02:00Z`. The example passes.
  - (ii)
    - (a) Filing refuses it, by `occurred_within` (`unsatisfied`).
    - (b) Filing refuses it as `invalid input`.
    - (c) Filing checks no guard, so it accepts, and approval refuses it.
  - (iii)
    - (a) The bound was held at filing, so the approval applies.
    - (b) "Every guard is re-evaluated at execution", including `occurred_within`, so the approval is refused and stays pending.
- **Where the text fails:**
  - flow-format §11.3: “held to the transition's `backdating_limit` when the proposal is filed”, and “`approve` executes the proposal's request as the approver, every guard evaluated as it stands then”.
  - DESIGN §9: “its target, transition, inputs, proposer, `context` and occurred time”, “**Filing** checks what a request checks short of the object's state and the guards”, “A proposal of a backdatable transition keeps the occurred time the request gives, held to the transition's bound when it is filed”, and “every guard is re-evaluated at execution under the **current** declaration”.
- **Severity:** changes-outcome.
- **Suggested fix:** State the occurred time of a proposal filed without one and when it executes. State the verdict filing gives for an occurred time that fails the bound. State that approval re-checks only the interval half.

#### 26. A timestamp with seconds, or a date, written unquoted, is a timestamp or date to a standard YAML loader

- **Construct:** timestamp and date values in an examples file.
- **Case:**
  - A request step with `occurred_at: 2026-01-01T09:00:30Z`, a form §11.3 allows.
  - `reimbursement.examples.yaml`'s `expense_date: 2025-12-30`.
  - PyYAML's `safe_load` gives `datetime.datetime(2026, 1, 1, 9, 0, 30, tzinfo=UTC)` and `datetime.date(2025, 12, 30)`, while `2026-01-01T09:00Z` stays a string. I verified this by running it.
- **The readings:**
  - (a) A builder on a YAML 1.1 loader fails the seconds form at step 2 against `"type": "string"`, while the minutes form passes.
  - (b) A builder whose loader keeps every scalar a string accepts both, as §11.3 says.
- **Where the text fails:**
  - flow-format §11.3: “a timestamp, in UTC, as `2026-01-01T09:00Z`, seconds optional, `2026-01-01T09:00:30Z`, and no other form”.
  - flow-format §2 settles booleans only: “Only `true` and `false` are booleans.”
  - examples.schema.json types `occurred_at` as a string: “When the request says the change happened: a timestamp, such as now - 1 day, held to the transition's backdating_limit or the kind's max_recording_delay.”
- **Severity:** changes-outcome (step 2).
- **Suggested fix:** State in §2 that timestamps and dates are not resolved by the loader and stay strings, as `yes` and `no` do. Or require them to be quoted.

#### 27. Evaluator stubs across a setup chain

- **Construct:** the scope of `evaluators` in setups and examples.
- **Case:** An example given `verified` stubs `xero.bill_matches: unsatisfied` and requests `verify_xero_record` again, expecting `unavailable`. `verified`'s own step was stubbed `satisfied`.
- **The readings:**
  - (a) A setup's map serves only its steps. The setup applies and the example passes.
  - (b) One merged map, with the example's override, serves the whole run. The setup's verify is refused, which is reported as the setup's failure.
  - Likewise, it is unstated whether a setup given another inherits that setup's map.
- **Where the text fails:**
  - flow-format §11.2: “the stubbed answers of §11.3, for this setup's steps”.
  - flow-format §11.3: “an example's map extends its setup's, and overrides it where both name a function”.
- **Severity:** changes-outcome.
- **Suggested fix:** State which map each step of a setup chain and of an example sees.

#### 28. A creation "failing" `occurred_within`

- **Construct:** the refusal of a caused creation by `occurred_within`.
- **Case:** none possible. A created object has no current interval, and only the requested transition checks the bound.
- **The readings:** a single reading. The clause is vacuous for a creation, but it suggests a check that does not exist.
- **Where the text fails:** flow-format §4.8: “A failure in a call or creation refuses the request as `call refused`, naming the step and its target with the clause `<Type>.occurred_within`”.
- **Severity:** wording.
- **Suggested fix:** Say "a failure in a call". Or state what a creation checks.

#### 29. A correction of an attribute that has no interval

- **Construct:** relabelling for a `corrects` of an untracked attribute.
- **Case:** `reimbursement.yaml`'s `correct_xero_bill_id` and `customers.yaml`'s `correct_email` correct strings. Strings are not tracked, so they have no interval to relabel.
- **The readings:** a single reading. Nothing is relabelled and only the event records the correction. But the text reads as if every listed attribute had an interval.
- **Where the text fails:** flow-format §4.13: “It relabels the current interval of each attribute it lists, from that interval's start”.
- **Severity:** wording.
- **Suggested fix:** Say "each tracked attribute it lists", and that an untracked attribute's correction changes only the value and the event.

### Counts by severity

| Severity | Count |
|---|---|
| changes-outcome | 18 |
| changes-value | 9 |
| wording | 2 |
| **Total** | **29** |

## 2. Erasure, proposals and flow changes, delivery, the API's shapes and the model's details, as the reader wrote it

**What I read.** `docs/DESIGN.md` §5.4, §5.5, §5.6, §5.9, §6, §7, §8, §9, §10 and §14, whole. `docs/design/flow-format.md` §3, §4.3, §4.8, §4.11, §4.12, §4.13, §4.14, §4.19 and §11 (§11.1 to §11.5), whole. `docs/design/declaration-syntax.md` §2 and §2.1, §6.4, and the check-table rows for checks 26, 37 and 60. `docs/design/library-api.md`, whole. `docs/design/storage-schema.md` §12, whole. `docs/design/flow-format/examples.schema.json`, whole. In `flow.schema.json` I read the definitions of `type` (with its `oneOf` branches for inline, bound, abstract and mirror types), `state`, `transition`, the effect steps (`call`, `create`, `foreach`, `supersede`) and the machine's `requires`; I located the rest by `grep` and did not read it line by line. The example modules `customers.yaml`, `customers.examples.yaml`, `delivery.yaml`, `delivery.examples.yaml`, `servicedesk.yaml` and `people.yaml`, whole, and the first lines of every other example module, to see which modules import what.

**Where I left those sections**, following citations or searching for a construct: `DESIGN.md` §5.2 and §5.3 (cited for `summary` and attributes), §5.11 (cited by §8 and §9 for observations and `forget`); `flow-format.md` §4.4 to §4.7, §4.10, §4.15, §6, §7 with §7.1, and §10; `declaration-syntax.md` §3.2 (lines 319 to 345: the `Approval` example and the cascade-skip rule, reached by searching for `approvals`), §4.1, §6.1 to §6.3, §6.5, §6.6, §6.8 (lines 870 to 906), and the §10 table rows for checks 1 to 10, 27, 29, 39 and 61 with the paragraphs above the table. A `grep` across the builder documents also showed me isolated lines of `DESIGN.md` §5.1 (line 170), `declaration-syntax.md` §8.2 (line 1326), and `storage-schema.md` §3 (line 145), §6 (line 495) and §9 (lines 707 and 716). I did not read those sections and use none of those lines below.

**What I did not read.** Nothing under `docs/adr/`, `docs/design/defects.md`, `TODO.md`, `docs/LESSONS.md`, any review file, the cold drafts, the case studies, `scripts/` or `.remember/`, and no git history. I modified no repository file.

**Quotations.** I checked every quotation below (the text between “ and ”) against its file with a Python script that asserts each one occurs verbatim, character for character. The report was not finished until every check passed.

### What is determined

- **Erasure.** An erasure runs from any state, a final one included. Its guards are all `deny`, and it never has a `backdating_limit`. It admits each invariant it leaves violated (customers example `erasing_reaches_the_contacts`: an `ACTIVE` customer erased of the email her state requires still applies). `not_erased` is decided with the generated guards, in the order `occurred_within`, `whole_open`, `not_erased`, `successor_not_superseded`, then the `<attribute>_provided` guards, then the transition's own. Its remedy is `unreachable_from_here`, it names no objects, it never refuses an erasure, a `clear` passes it, and a supplied attribute input written to a personal attribute is refused by it (`what_was_erased_stays_erased`). The group is every object whose chain ends in the same survivor. The other members are erased after the requested erasure's effect and cascades, in ascending id order, once per request, as caused transitions recorded as the requester's. A member's refusal gives `call refused` with the step `group erasure`, and a member whose type declares no erasure is passed over (`erasing_follows_a_merge`). Check 60 requires an erasure of every concrete type that holds, or ever held, a personal attribute or input.
- **Supersession.** `successor_not_superseded` refuses a successor input that is the object itself or is already superseded. Its remedy is `self_serviceable`, and it names the successor even when the successor is the object itself. Through a `call` it gives `call_refused` with the clause qualified (`a_customer_merged_into_itself_is_refused`). An assertion's `to` cannot list a superseding state, and a migration mapping into one is refused. `get(id, follow)` returns the last member of the chain.
- **Proposals.** The built-in type has the states `PENDING`, `EXECUTED`, `REJECTED`, `WITHDRAWN` and `INVALIDATED`, and the transitions `approve`, `reject`, `withdraw` and the only-via `invalidate`. Filing a transition that is not proposable is refused as `not requestable`. A decision on a proposal no longer pending is `unavailable` (`a_rejected_proposal_is_not_approved`). A refused approval returns the request's verdict and leaves the proposal pending (`a_proposal_approved_too_soon_stays_pending`). An approval executes as the approver under the current declaration, and an approval that applies returns `Satisfied` naming the target object.
- **Flow changes.** The `DeclarationChange` states, transitions, guards and remedies are tabulated. A publish supersedes every other open change in its own transaction. An upgrade does nothing at the same release and refuses from a release the new one does not name. It checks the flows, records the next version and supersedes open changes only where the built-ins differ.
- **Delivery.** `subscribe` creates a subscription `ACTIVE`, and `revoke` moves it to `REVOKED`. The first acknowledged cursor is the log's settled cursor at creation. Cursors are exclusive, a pull without a cursor reads after the acknowledged one, and `acknowledge` may move it back. The lag is the age of the oldest pending selected event, and the subscription is lagging or dead once the lag reaches the respective threshold. A revoked or unknown subscription is refused as a value (`EventPage.refused`). Every event names a transition.
- **The model's details.** The objects an unsatisfied clause names are in ascending id order. A nested aggregate and the arguments of `changed_since` add none, and each generated guard names its stated objects. An invariant refusal is ordered by object id, then by the type's declaration order, and its remedy is the first failure's. Uniqueness is partial over an absent attribute, scope or `with` attribute, and the existing holders are named and not re-checked. A `unique` declared on a base spans the family. `cascaded` lists calls, creations, cascades, a group's erasures and `forget`, in the order each began, and excludes `redact` events and a proposal's invalidation. An effect writes the new state first, runs its steps in order, then its cascades. A loop's collection is a snapshot taken when the loop starts, a `range` runs from 1, and `limit` counts the elements its `where` selects. A name an effect binds may not be a member of the type, one of the listed words, or its enclosing loop's name. A type with no `summary` lists its id and state, and a family lists each object's own type's summary. An abstract base has no objects, no lifecycle and no `summary`, and its subtypes inherit its attributes, derived attributes, invariants and tracking.

### Findings

#### 1. The order of an approval's events contradicts the rule for `Satisfied.events`

- **Construct:** approval of a proposal, `Satisfied.events`.
- **Case:** a proposal of `Delivery.cancel` is filed, assuming it were proposable, on a delivery with two open checklist items. It is approved. The request appends the proposal's `approve` event, the delivery's `cancel` event, and two `ChecklistItem.discard` events.
- **The readings:** (a) By §9, the target's event comes first, then the proposal's: `[cancel, …, approve]`, with the discards placed either before or after the proposal's event. (b) By library-api §4, the parent's event comes first. The requested transition is the proposal's `approve`, and it is the cause of the target's event, so the order is `[approve, cancel, discard, discard]`. The two readings return different `Satisfied.events`. Reading (a) does not fix where the cascaded events go.
- **Where the text fails:** [DESIGN §9] “An approval that applies returns `Satisfied` naming the target object, its event first, then the proposal's.” and [DESIGN §9] “`approve`, `reject` and `withdraw` are its transitions out of `PENDING`, requested on the proposal like any transition”, executed [DESIGN §9] “with the approval as cause”. Against this, [library-api §4] “`Satisfied.events` is the positions of every event the request appended, the parent's first. A cascade of three parts returns four.”
- **Severity:** changes-value
- **Suggested fix:** State the whole order of an approval's events, including where the target's cascaded and caused events go relative to the proposal's event, and make library-api §4 name the approval as an exception, or restate its rule.

#### 2. Whether making a transition no longer proposable invalidates its pending proposals

- **Construct:** a publish invalidating proposals.
- **Case:** v2 drops `proposable: true` from `Delivery.complete` while the proposal `handover` is pending.
- **The readings:** (a) By §9, v2's publish invalidates `handover`: it moves to `INVALIDATED`, and the impact report lists it. (b) By §5.9's list, `handover` is not invalidated and stays `PENDING`. A later approval re-evaluates every guard, and proposability is not a guard, so it executes `complete`: a non-proposable transition is executed through a proposal.
- **Where the text fails:** [DESIGN §5.9] “the new version cannot express are invalidated: those whose transition it removes, whose input it no longer takes, or one of whose input values its type no longer holds”. Against this, [DESIGN §9] “A publish invalidates a pending proposal whose transition it removes or makes no longer proposable, whose input it no longer takes, or one of whose input values its type no longer holds”. Approval re-checks only guards: [DESIGN §9] “and every guard is re-evaluated at execution under the”.
- **Severity:** changes-outcome
- **Suggested fix:** Give one list of what invalidates a pending proposal, in both places, and say whether an approval re-checks that the transition is still proposable.

#### 3. The order of filing's checks, and which transition a refused filing is counted under

- **Construct:** filing a proposal.
- **Case 1:** `propose` on a nonexistent object id, naming a transition the type does not declare. Case 2: `propose` on a nonexistent object, naming a transition that exists but is not proposable.
- **The readings:** (a) Checks in the order a request runs: `not found` comes first in both cases. (b) Checks in the order §9 lists them: case 1 is `unknown transition`, and case 2 is `not requestable`. Separately, for the attempt row of a refused filing: (c) a row of the proposed transition on its target, as a request's would be, which counts in that transition's refusal metrics and diagnostics; (d) a row of the `Proposal` type's creation.
- **Where the text fails:** [DESIGN §9] “checks what a request checks short of the object's state and the guards: an unknown transition, a transition not proposable, refused as `not requestable`, an object that does not exist, and an input the transition does not take, of the wrong type, or required and left out, refused as a request would refuse them.” Against this, [DESIGN §6] “If the object does not exist, refuse with `not found`; if its type, or for a creation the type named, declares no transition of that name the request can take, refuse with `unknown transition` (ADR-0122).” The attempt row holds [DESIGN §6] “the transition and the verdict;”, without saying which transition a filing's row holds.
- **Severity:** changes-outcome
- **Suggested fix:** State the order of filing's checks, or say that it is the request order, and state the type, object and transition a refused filing's attempt row records.

#### 4. Which input faults filing refuses, and whether it checks `expected_version`

- **Construct:** filing a proposal.
- **Case:** `propose` `Customer.merge_in` with `duplicate` naming no object. Separately, a filing that carries an `expected_version` that is out of date.
- **The readings:** (a) Filing refuses every fault a request's step 4 refuses: a reference naming no object or one of another type, a repeated set element, or a required input given as null. The filing is refused as `invalid input`. It also checks step 3's `expected_version` and refuses a stale one as `stale`. (b) Filing refuses only the three faults listed. The filing applies, and the faults surface at approval. The version is ignored, or kept for approval.
- **Where the text fails:** [flow-format §11.3] “Filing checks the object and the inputs as a request would, refusing an input the transition does not take, one of the wrong type, or a required one left out as `invalid input` (ADR-0147)”. A request also refuses [DESIGN §6] “an input the transition does not take, of the wrong type, naming no object or one of another type, repeating an element of a set,”, and [DESIGN §6] “If an `expected_version` is given and differs, refuse with `stale`.” The library allows a version on a filing (`Request` carries both `expected_version` and [library-api §3] “propose: bool = False”).
- **Severity:** changes-outcome
- **Suggested fix:** List every refusal filing makes, or say "every input fault step 4 refuses", and say whether a filing's `expected_version` is checked, kept for approval, or refused as meaningless.

#### 5. A proposal's occurred time, the bound at approval, and the executed event's context

- **Construct:** a proposal's occurred time and context, at approval.
- **Case A:** `Delivery.complete`, which has no `backdating_limit`, is filed at T0 without `occurred_at` and approved at T0 + 2 h. Case B: a transition with `backdating_limit: 2 days` is filed at T0 with `occurred_at: T0 - 1 day` and approved at T0 + 3 days. Case C: the filing sends `context: "agent/sweep"` and the approval sends `context: "ui/approve"`.
- **The readings:** In case A, (a) the proposal holds an occurred time anyway, its filing time, and the executed event records T0 as occurred. Because a supplied occurred time on a transition without a bound is refused by `occurred_within`, the approval is either refused or the event is backdated without a bound. (b) No occurred time is held, and the event's occurred time is T0 + 2 h. In case B, (c) every guard, `occurred_within` included, is re-evaluated at approval, so the time is 4 days back, the approval is refused, and the proposal stays pending for good. (d) The bound was settled at filing, so the approval applies with T0 - 1 day. In case C, the executed event's `context` is either the proposer's, recorded on the proposal, or the approver's.
- **Where the text fails:** the proposal holds [DESIGN §9] “its target, transition, inputs, proposer, `context` and occurred time, and an optional expiry”, and [DESIGN §9] “A proposal of a backdatable transition keeps the occurred time the request gives, held to the transition's bound when it is filed, since that is when the store was told; the check against the current interval of what it changes runs at approval”. Against this, [DESIGN §9] “and every guard is re-evaluated at execution under the” current declaration. A supplied time without a bound is refused, as for an assertion: [flow-format §4.13] “so an `occurred_at` it is given is refused, since it has no `backdating_limit`”. [DESIGN §9] “Approving it executes the recorded request” does not say whose `context` the executed event carries.
- **Severity:** changes-outcome
- **Suggested fix:** State what occurred time a proposal holds when none is given, whether `occurred_within`'s bound is re-checked at approval or only its interval check, and whose `context` the executed event and a refused approval's attempt row record.

#### 6. The `Proposal` record has no defined shape: its creation transition, its attributes and the attached verdict

- **Construct:** the built-in `Proposal` type, a refused approval's attempt row, and `get` of a proposal.
- **Case:** a filing applies, and two later approvals are refused. A subscriber filters on the proposal's creation event by transition name, and a reader calls `get(proposal)`.
- **The readings:** The creation transition has no name, so one builder records `propose`, another `file`, another `create`. The attributes have no names: one builder writes `target`/`transition`/`inputs`/`proposer`/`context`/`occurred_at`/`expires_at`, another something else. "The verdict attached" is (a) the last refused approval's attempt row, (b) all of them, or (c) none once retention prunes the rows. The attempt row "naming the proposal" has no field to name it in: `Attempt` in library-api §5 has none, so one builder adds a field and another puts the id in `context`.
- **Where the text fails:** [DESIGN §9] “a request marked `propose` creates one, `PENDING`”, while every event names a transition, [DESIGN §7] “and an event of a built-in type, its own transition”. Compare `Subscription`'s named creation, [DESIGN §7] “`subscribe` creates one, `ACTIVE`, and `revoke` moves it to the final `REVOKED`”. The refused approval: [DESIGN §9] “it is one attempt row of the proposed transition, on its object, carrying the clause and naming the proposal, and the proposal gets no event, the verdict attached being that row, which a read of the proposal returns”. But [DESIGN §6] “Rows are pruned after the deployment's retention period”.
- **Severity:** changes-outcome
- **Suggested fix:** Name the proposal's creation transition and its attributes, say which attempt rows a read of a proposal returns and what it returns after the prune, and add the proposal's id to `Attempt`.

#### 7. `expired` is a state no lifecycle declares: what `Unavailable`, `get` and the examples report, and whether an erasure invalidates an expired proposal

- **Construct:** proposal expiry.
- **Case:** a proposal filed with `expires_at = T`. At T an approval is attempted, `get(proposal)` is read, and the proposal's target is then erased.
- **The readings:** (a) `Unavailable.state = "expired"`, and `get` reports the state `expired`, a value outside the declared states and in lower case. (b) `Unavailable.state = "PENDING"`, the stored state, and expiry is reported some other way. An example can express neither: the `propose` step has no expiry key, and `expect.proposal` has no `expired`. For the erasure, (c) the expired proposal counts as pending, is invalidated and gets an `invalidate` event, or (d) it is only redacted, since "counts as pending" is stated for a publish alone.
- **Where the text fails:** [DESIGN §9] “and so is an approval from the proposal's expiry instant on, naming the state `expired`, which a read reports; an expired proposal may still be rejected or withdrawn, and counts as pending for a publish's invalidation”. The library's state field is [library-api §4] “# the state the object is in, which the transition does not leave”. The examples schema has [examples.schema.json] “"enum": ["pending", "executed", "rejected", "withdrawn", "invalidated"]”, and the expiry is in the library only, [library-api §3] “# a proposal's expiry, with `propose` only”. An erasure acts on proposals [DESIGN §8] “invalidating the pending ones (ADR-0087, ADR-0147)”.
- **Severity:** changes-outcome
- **Suggested fix:** Say what string `Unavailable.state` and a read carry for an expired proposal, add expiry to the examples' `propose` step and expectations or say it is untestable there, and say whether an erasure treats an expired proposal as pending.

#### 8. Which observations have a generated `forget`, how labels appear in `cascaded`, and what `forget` receives

- **Construct:** an observation's generated `forget`, taken by the subject's erasure.
- **Case:** a `Customer` with an observation kind `Visit` that has no personal fields, one `Visit` recorded, and one label with a note. The customer is erased with `reason: asked`.
- **The readings:** (a) Every kind has `forget`, so the erasure takes `Visit.forget`, records an event and lists it in `cascaded`. (b) Only a kind with personal fields has `forget`: nothing is taken on the `Visit`, and `cascaded` omits it. For the label, (c) its `forget` is a caused transition listed in `cascaded`, but `label.forget` cannot match the expectation's pattern, which requires an upper-case type, or (d) the note is redacted without a transition, by a `redact` event. For `forget`'s required `reason`, the value is either the subject erasure's `reason`, passed by name, or something else. Nothing says.
- **Where the text fails:** [DESIGN §5.11] “each kind has a generated `forget`, which the subject's `erase` runs and which may be requested on one observation alone”, and the kind expands to [declaration-syntax §6.8] “an erasure, `erase forget`, taking a `reason`, which erases the observation's personal fields (ADR-0096)”. Against this, [flow-format §11.3] “`record` on an observation kind and `forget` on a kind with personal fields (ADR-0147)”. On labels: [DESIGN §8] “It reaches observations through each one's generated `forget`, which the subject's `erase` takes without a declared step, after its declared steps and cascades, in ascending id order, as a transition it causes, which `cascaded` lists (ADR-0147), redacts label notes, which are treated as personal”, with [DESIGN §9] “and `label` is a built-in observation kind (§5.11)” and [declaration-syntax §6.8] “Its `forget` redacts the note, so a note is erasable on a type that declares no `erase` of its own.” The `cascaded` pattern is [examples.schema.json] “^[A-Z][A-Za-z0-9]*\\.[a-z][a-z0-9_]*$”.
- **Severity:** changes-outcome
- **Suggested fix:** State which kinds have `forget`, whether a label's `forget` is a transition the erasure takes and how `cascaded` names it, and what inputs the erasure passes to each `forget`.

#### 9. Whether observations' `forget` comes before or after the other group members, and their order across collections

- **Construct:** the order of an erasure's caused transitions.
- **Case:** customers flow plus a kind `Visit` with a personal note. `dana` (id 1) has `Visit` v5, and the merged `duplicate` (id 2) has `Visit` v3. A second kind `Call` on `dana` has observation c4. `dana` is erased.
- **The readings:** (a) `dana`'s observations' `forget` belongs to her erasure, so `cascaded` is `[Call.forget(c4), Visit.forget(v5), Customer.forget(dup), Visit.forget(v3)]`, ordered by id across collections. (b) The group comes right after effect and cascades, so it is `[Customer.forget(dup), Visit.forget(v3), Call.forget(c4), Visit.forget(v5)]`. (c) As (a), but collection by collection in declaration order: `[Visit.forget(v5), Call.forget(c4), …]`.
- **Where the text fails:** both are placed "after effect and cascades": [DESIGN §8] “The other members are erased after the requested erasure's effect and cascades, in ascending id order, each once per request, as transitions it causes”, and the `forget` sentence quoted in finding 8, [DESIGN §8] “after its declared steps and cascades, in ascending id order”. `cascaded` is [flow-format §11.4] “the exact sequence, in the order each began, of the transitions the request took on other objects, calls, creations, cascades, a group's erasures and the generated `forget` an erasure takes on each observation”.
- **Severity:** changes-value
- **Suggested fix:** State the full order of an erasure's caused transitions: declared steps, cascades, the object's observations, then the group, with each member's own observations, and whether ids are ordered across collections or per collection.

#### 10. `not_erased`: decided from the steps that are written, or from the ones that would write

- **Construct:** `not_erased`.
- **Case:** after `dana` is erased, a new internal transition `update_details` takes `new_email: { type: string, optional: true }` with `effect: [assign: { location: email, expr: inputs.new_email }]`. It is requested without `new_email`. Variants: the `assign` sits in a `foreach` whose collection is empty, or its expression evaluates to absent.
- **The readings:** (a) Static: the step exists and assigns a personal attribute "whatever it computes", so the request is refused with `not_erased`. (b) Dynamic: the step is skipped or runs no time, so nothing non-absent is written and the request applies.
- **Where the text fails:** [DESIGN §8] “a request whose outcome would write a value that is not absent to one of its personal attributes is refused by the generated guard `not_erased`, decided with the generated guards from the inputs the request supplies and the effect steps that assign or add to a personal attribute, whatever they compute, a `clear` passing (ADR-0147)”. Absence skips a step: [DESIGN §5.4] “A write whose right-hand side is an unsupplied optional input is skipped, not applied”. A range or collection may run nothing: [flow-format §4.8] “one that is absent or not positive running no step”.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a step that is skipped, runs zero times, or would write absence counts against `not_erased`.

#### 11. A personal set: refused by check 9, yet named by `not_erased`'s `add`

- **Construct:** personal attributes and `not_erased`'s `add`.
- **Case:** `aliases: { type: "string[]", personal: true }` with a transition that `add`s to it.
- **The readings:** (a) A set is never optional, and a personal attribute must be optional, so the declaration is refused at step 3 and `not_erased`'s mention of `add` can never apply. (b) A set is not "required" either, so it is accepted, and an erasure empties it, though a set is "never absent". The `add` rule then refuses after erasure.
- **Where the text fails:** [flow-format §4.13] “Because an erasure writes absence, a `personal` attribute or field MUST be optional (step 3, `names`; the model's check 9).” and [flow-format §4.3] “A set, of values or of references, is never optional and never absent”. Against these, check 9 and step 3 speak of "required": [declaration-syntax §10] “| 9 | A required attribute marked `personal` |” and [flow-format §7] “a personal attribute that is required”. `not_erased` names `add` (finding 10's quotation).
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a set may be personal, and if it may, what erasure writes to it.

#### 12. Which event carries an erasure's admission on an object the erasure did not otherwise write

- **Construct:** erasure admissions.
- **Case:** `Order` declares a traversal invariant `state != OPEN or customer.email is not null`, which `Customer.orders` makes traversable. `dana`, with an open order O, is erased. The invariant fails on O, which takes no transition and has nothing redacted.
- **The readings:** (a) O gets an event carrying the admission, as a publish gives an unmapped object a `migrated` event. O's version advances, and the override counts against `Order`. (b) The admission is recorded on `dana`'s erasure event and names O. O gets no event, and the override counts against `Customer` (as the `overrides` metric of customers.yaml describes).
- **Where the text fails:** [DESIGN §8] “is recorded as an admission on the object the invariant fails on, whichever object's value broke it (ADR-0147);” against [declaration-syntax §6.4] “recorded on the event with the reason, on the object the invariant fails on, exactly as a declared admission is”. Only a publish states the extra event: [declaration-syntax §6.6] “an object no other mapping changes receives a `migrated` event carrying only the admission”. The metric is [customers.yaml] “How many overrides a customer's events record, by reason, an erasure that leaves an invariant violated and a migration's admission counted with a state overridden.”
- **Severity:** changes-value
- **Suggested fix:** Say which event records an erasure's admission on another object, and whether that object gets an event of its own.

#### 13. Erasing again: is an invariant already admitted admitted again?

- **Construct:** erasing again.
- **Case:** `dana`, `ACTIVE`, is erased, which admits `active_invariant`. She is erased again, and the violation still stands.
- **The readings:** (a) The second erasure "leaves it violated", so it records a new admission, and the override counts rise by one. (b) It did not break the invariant, and a standing admission is not re-checked, so it records none.
- **Where the text fails:** [DESIGN §5.6] “An erasure admits each invariant it leaves violated, recording the admission on the object the invariant fails on” and [DESIGN §8] “runs the whole erasure again: it redacts every personal value present now”. Against these, [DESIGN §8] “An erasure records an admission only for an invariant it leaves violated, as a declared admission is recorded only where the request breaks the invariant.” and [DESIGN §6] “except those with an admitted violation still standing”.
- **Severity:** changes-value
- **Suggested fix:** Say whether an erasure records an admission for an invariant whose admission still stands.

#### 14. "Erasures requiring the same inputs": which inputs, which types, and which check

- **Construct:** the inputs of the group's erasures.
- **Case 1:** `Customer.forget` takes `reason: string` and `detail: string?`, personal. Type B, joined to `Customer` by supersession, has `forget` taking only `reason: string`. Case 2: A's `reason` has a `default` and B's does not. Case 3: the successor input is typed as an abstract base, `Asset`, or A is superseded by B and B by C.
- **The readings:** In case 1, (a) every declared input must match by name, type and optionality, so the publish is refused, or (b) only required inputs must match, so it publishes. Under (b), erasing a customer with `detail` passes `detail` to B, and B either refuses it as `invalid input` (giving `call refused`, step `group erasure`) or receives only what it takes. In case 2, B receives either the supplied inputs (`reason` missing, so refused) or the resolved ones. In case 3, the types joined are either just the declared successor type or every concrete subtype, and either one supersession step or the transitive closure. So the publish is refused or not.
- **Where the text fails:** [flow-format §4.19] “the types a supersession joins that declare erasures MUST declare ones requiring the same inputs, by name, type and optionality, a type holding nothing personal declaring none and being passed over by the group's erasure”, with members [DESIGN §8] “each through its own type's `erase`, receiving the requested erasure's inputs by name”. Check 60's row, [declaration-syntax §10] “A concrete type with a personal attribute, inherited ones included, or with a transition taking a personal input”, ends with [declaration-syntax §10] “and a type declares at most one `erase` (ADR-0147)” and carries no same-inputs clause, and step 3's list in flow-format §7 has none either. Relatedly, [DESIGN §8] “so two duplicates merged into one are both reached, whichever member is named” fails for a member whose type declares no erasure, since it cannot be named.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether "the same inputs" means every declared input or the required ones, whether defaults are resolved before passing, and which types a supersession joins (families, transitively), and put the rule in check 60's row.

#### 15. At most one erasure, when the machine declares one and the binder declares its own

- **Construct:** at most one erasure per type.
- **Case:** machine `M` declares erasure `forget`, and binder `T` declares its own erasure `erase_all`.
- **The readings:** (a) `T` has two erasures and is refused. (b) The binder's erasure replaces the machine's, as its creations do, so `erase_all` is `T`'s one erasure and is what the group's erasure runs. (c) Only a name clash is refused, so both exist and "at most one" is broken.
- **Where the text fails:** [flow-format §4.13] “MUST declare an erasure, its own or its machine's” and [flow-format §4.19] “and a type MUST declare at most one erasure”. Replacement is stated for creations only, and [flow-format §4.10] “Any other transition, and any condition, MUST NOT share a name with one of the machine's”.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a binder's own erasure is refused beside a machine's, or replaces it.

#### 16. An erasure's side events: which objects get a `redact` event, their source, cause and order

- **Construct:** the events an erasure records besides its transitions.
- **Case:** the customers example `erasing_reaches_the_contacts`. `dana`'s `add_contact` event carried `phone`, and it caused `Contact.add`. Erasing the contact redacts that input as a caller's event, but `dana` is herself being erased. Add a pending proposal targeting `dana`.
- **The readings:** (a) `dana`'s `forget` event covers it, and she gets no extra event. (b) She also gets a `redact` event, caused by the contact's erasure (which redacted the value) or by her own. The proposal's single invalidating event has source either `erased` (as a redaction) or `observed` (as a transition). The order of these events in `Satisfied.events` and in the log is stated nowhere beyond the parent coming first.
- **Where the text fails:** [DESIGN §8] “an attribute it holds or a value in one of its past events, gets an event”, [DESIGN §8] “The event has source `erased`, is caused by the erasure's own event, and names the attributes redacted and never their values.” A caller's event is redacted [DESIGN §8] “up the erased object's cause chain, the inputs that flowed into the erased object's personal attributes”, and for proposals, [DESIGN §8] “the invalidation and the redaction being one event”. The case is built from [customers.yaml] “- create: { type: Contact, transition: add, inputs: { customer: this, role: inputs.role, phone: inputs.phone } }”. Ordering: [library-api §4] “`Satisfied.events` is the positions of every event the request appended, the parent's first. A cascade of three parts returns four.”
- **Severity:** changes-value
- **Suggested fix:** Say whether an object erased in the request also gets `redact` events, which erasure event a group member's or a part's redaction is caused by, the source of an invalidation that redacts, and where these events fall in `Satisfied.events`.

#### 17. Which personal inputs of an affected proposal are redacted

- **Construct:** proposal redaction on erasure.
- **Case:** pending proposal P on customer X (not erased), for a transition with input `a`, which a call writes into `dana.email`, and input `b`, marked personal and written to X's own personal attribute. `dana` is erased.
- **The readings:** (a) Every personal input of P is redacted, `a` and `b`, so X's data goes although X was not erased. (b) Only the inputs flowing into `dana`'s personal attributes are redacted: `a`.
- **Where the text fails:** [DESIGN §8] “And it invalidates every pending proposal that targets the erased object or carries inputs flowing into its personal attributes, redacting only the personal inputs, in one event per proposal (ADR-0147).” with [DESIGN §8] “an input being personal by marking or by flowing into a personal attribute (ADR-0078)”.
- **Severity:** changes-value
- **Suggested fix:** Say whether "the personal inputs" means every personal input of the proposal or those reaching the erased object.

#### 18. What a newly personal attribute reaches, at the publish and at a later erasure

- **Construct:** a publish's redaction of a newly personal attribute, and the reach per written value.
- **Case:** `Customer.phone` is not personal in v1, where values are written. v2 marks it personal. `dana` was erased under v1. `erin` was not, and is erased under v2. Variant: v2 marks an input of `add_contact` personal.
- **The readings:** For `dana`, (a) the publish redacts only the current value, or (b) also every v1 event's value. For `erin`, (c) her erasure reaches only values written under a version that marked the attribute personal, so her current value and her v1 events keep the phone, or (d) it reaches every value of an attribute that is personal now (the current value and every event), plus values written under earlier marking versions. For the newly personal input, its v1 events are redacted, or not. For an observation already forgotten, a newly personal field is redacted by the publish, or not.
- **Where the text fails:** [DESIGN §8] “It reaches every value written under a version that marked its attribute personal, and a publish that newly marks an attribute personal redacts it on objects already erased, as part of the publish” against [DESIGN §8] “replaces the values of declared personal attributes with absence, on the object and in every event that carried them”, and for inputs [DESIGN §8] “and every event payload that carried a personal input, whether or not that input was written to an attribute”.
- **Severity:** changes-value
- **Suggested fix:** State the rule per value. Is a value reached when its attribute or input is personal now, when it was personal when written, or either? Say whether the publish's redaction also rewrites past events and forgotten observations.

#### 19. "Superseded" when state and successor disagree: an import into a superseding state, and the window before `supersede`

- **Construct:** `successor_not_superseded`, superseding states outside a declared transition.
- **Case 1:** a mirror ported from a legacy system declares `MERGED: { final: true, superseding: true }`, and the import writes an object in `MERGED`. Case 2: `T.move` (into a superseding `MOVED`) has the effect `[create T.spawn(origin: this) as n, supersede: n]`. `spawn`'s effect calls `W.merged_into(successor: inputs.origin)` while `origin` is in `MOVED` with no successor yet.
- **The readings:** In case 1, (a) the import's built-in assertion may not reach a superseding state, so the row is refused, or (b) it is written, leaving an object superseded by nothing. `get(follow)` and the group then treat it as a chain end or not. In case 2, (c) "already superseded" means being in a superseding state, so the call is refused, or (d) it means having a successor, so it applies and the chain becomes `W → origin → n`.
- **Where the text fails:** the rule covers declared assertions, [flow-format §4.19] “an assertion's `to` MUST NOT list a superseding state, a merge being a declared transition”, and migrations, [declaration-syntax §6.6] “state is refused, since it would leave its objects superseded by nothing”, but the import is [DESIGN §8] “assertion, so no type is unimportable by omission”. Case 2 turns on [flow-format §4.19] “A transition's new state is written before its effect runs, so the guard also refuses a chain closed within one request” and on `get`'s [DESIGN §10] “optionally following supersession to the last member of its chain, the one not superseded”.
- **Severity:** changes-outcome
- **Suggested fix:** Define "superseded" (by state or by successor), and say whether a mirror may declare a superseding state and whether the import may write one.

#### 20. Where legacy entries go in `history(follow)`

- **Construct:** `history(id, follow)`.
- **Case:** two customers, ported with legacy entries and later cut over, are merged. `history(dana, follow=True)` is read.
- **The readings:** (a) Each member's legacy entries come before its own events. (b) All members' legacy entries come first, by `occurred_at` or by `ordinal`. (c) Only the named object's legacy entries are returned. Legacy entries have no position, so "position order" cannot place them.
- **Where the text fails:** [DESIGN §10] “Events with provenance, legacy entries, and optionally the combined timeline of the whole group supersession joins, every member whose chain ends where the named object's does, in position order (ADR-0147)”. A legacy entry carries [library-api §5] “ordinal: int” and no position.
- **Severity:** changes-value
- **Suggested fix:** State where each member's legacy entries fall in the combined timeline.

#### 21. The engine upgrade's record, its checks, and its version's source

- **Construct:** an engine upgrade.
- **Case:** a release changes a built-in type. Two changes are open, and the installed version's examples fail under the new built-ins.
- **The readings:** The upgrade's own event: it is on the `Operator`, with transition and context unnamed, or there is none. Each superseded change's event names a cause: the upgrade's event, if one exists, or nothing, since `cause` is an event and a release is not. "Checks the installed flows" runs the publish checks only, so the upgrade applies, or the examples too, so it is refused. Over version 0, the new version copies no source reference or digest, though only version 0 is said to lack them.
- **Where the text fails:** [storage-schema §12] “it records them as the next version, with the installed version's source, reference and digest and the new built-ins, naming the release as its cause, and supersedes every open `DeclarationChange`, as a publish does” and [storage-schema §12] “recorded as that operator's act naming the release, as creating the store is (ADR-0126)”. The check is only [storage-schema §12] “it checks the installed flows against the new one”, while for a publish [DESIGN §5.9] “a failing one refuses the publish”. A superseded change is [DESIGN §9] “a plain final state naming the publishing change and not supersession of §8”, an event's cause is [library-api §5] “cause: int | None”, and `declaration` returns the source [DESIGN §10] “neither for version 0 (ADR-0135)”.
- **Severity:** changes-outcome
- **Suggested fix:** Name the upgrade's event (object, transition, context), what each superseded change's event names and is caused by, which checks the upgrade runs, examples included or not, and the source reference of a version recorded over version 0.

#### 22. `publish` through `request`, without the `expected_version` the operation requires

- **Construct:** the `publish` transition and the `publish` operation.
- **Case:** `store.request(Request(transition="publish", object_id=change, actor=a))`, with no `expected_version`.
- **The readings:** (a) It applies, since a request's version is optional. (b) It is refused, or raises, since `publish` requires one. (c) `publish` may be taken only through the `publish` operation. Separately, `publish(…, expected_version=None, dry_run=False)` maps to neither a listed verdict nor one of the five exceptions.
- **Where the text fails:** [library-api §3] “`publish` keeps its own required `expected_version`.” and [library-api §6] “# required unless dry_run (ADR-0101)”, while [DESIGN §10] “`publish`, which requests a `DeclarationChange`'s `publish` transition, and whose dry run is the impact report (§9)”, and a request names [DESIGN §6] “and optionally the version the caller last read”.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether `publish` may be requested through `request` and what a missing version gives on either path.

#### 23. The built-in types' inputs and attributes are not named, and `get` has no field for a subscription's lag

- **Construct:** the built-in `Subscription` type (and `DeclarationChange`'s `draft`).
- **Case:** a caller subscribes to `Delivery` events with transition `complete`, `lagging_after: 1 h` and `dead_after: 30 min`, then reads `get(subscription)`.
- **The readings:** `subscribe`'s inputs (names, types, whether the filter is one value or three, whether the type, the transitions and the thresholds are required) are left to each builder. `get` returns an `Object`, whose only free-form field is `attributes`, so the lag and "lagging or dead" go under keys each builder chooses, or into `state`. With `dead_after` below `lagging_after`, "which of the two" is either one, at 45 min. `draft`'s inputs beyond `source_ref` are likewise unnamed.
- **Where the text fails:** [DESIGN §7] “a filter over a type or family, transition names and `changes_state`;”, [DESIGN §7] “lag thresholds, `lagging_after` and `dead_after`, two durations;”, [DESIGN §7] “`get` of a subscription returns its lag and which of the two it is in”, and the library's [library-api §6] “def get(self, actor: Actor, id: str, follow: bool = False) -> Object | None: ...”.
- **Severity:** changes-outcome
- **Suggested fix:** Declare each built-in type's inputs and attributes, as a flow type's would be, with the subscription's lag fields and the order the thresholds must have.

#### 24. Cursor values the API cannot express: the log's start, a refused page's cursor, a malformed cursor

- **Construct:** `pull` and `acknowledge` cursors.
- **Case:** a reader wants to replay from the log's start. A revoked subscription is pulled. A cursor string that was never issued is passed to `pull` or `acknowledge`, or a cursor ahead of the settled one is acknowledged.
- **The readings:** The start is either a sentinel each builder invents (`""`, `"0"`) or not expressible at all. A refused `EventPage` must carry `settled: str`: an empty string, the acknowledged cursor, or the current settled cursor. A malformed cursor is either an exception outside the closed five or a refusal value the API does not have. A cursor ahead of the settled one is either accepted or refused.
- **Where the text fails:** [DESIGN §7] “a pull given an earlier cursor, such as the log's start, reads from there”, while [library-api §8] “A settled cursor encodes a (transaction, position) pair on PostgreSQL and a position on SQLite, and a caller never parses it”. The refusal value is [library-api §5] “"revoked" or "unknown", with no events”, the exceptions are closed, [library-api §7] “These are the five things that raise, and the list is closed so that a second binding cannot differ on it.”, and the settled cursor is [DESIGN §14] “the furthest a pull or export reader may acknowledge, and never ahead of a late commit”.
- **Severity:** changes-outcome
- **Suggested fix:** Define the start-of-log cursor, a refused page's `settled`, and the result of a cursor the store never issued or one ahead of the settled cursor.

#### 25. A page's content: the cursor after skipped events, page size, the subscription's own creation event, and unsettled events in the lag

- **Construct:** `pull` pages, the first cursor, and the lag.
- **Case:** a subscription filters `Delivery.complete`. After the cursor come one selected event, then three that the filter skips, and the page is not full. Also: a subscription filtering the `Subscription` type is created, and the lag is read while a committed event is not yet settled.
- **The readings:** The page hands back either the selected event's pair (the last pair returned) or the settled cursor past the three skipped events (as an empty page does). The page size is unstated. The subscription's own `subscribe` event lies either after its first cursor, so it is delivered, or before it, so it is not. The lag counts settled events only, or every committed one.
- **Where the text fails:** [DESIGN §7] “in (transaction, position) order, and hands back the last pair as the cursor” against [DESIGN §7] “a page with no event hands back the settled cursor past what the filter skipped”. [DESIGN §7] “A new subscription's acknowledged cursor is the log's settled cursor at its creation, so it starts with nothing old”, and [DESIGN §7] “the lag is the age, by recorded time, of the oldest event the filter selects after the acknowledged cursor, zero where none is pending”.
- **Severity:** changes-value
- **Suggested fix:** Say which cursor a non-empty page hands back, what bounds a page, whether the creating transaction's own event is after the first cursor, and whether the lag reads unsettled events.

#### 26. `migrate`, `import` and `redact` are not reserved transition names

- **Construct:** a subscription's transition filter, and a transition-name metric.
- **Case:** a `Document` type declares its own transition `redact`. A subscription filters on `redact`.
- **The readings:** (a) The name is refused at step 3, as `any` is. (b) It is accepted, and the filter delivers both kinds of event under one name, telling them apart only by `source`. The `transition_count` metric over `redact` excludes the document's own events as "redacted", or does not.
- **Where the text fails:** [DESIGN §7] “a migration's, `migrate`; an import's or a refresh's, `import`; an erasure's redaction of another object, `redact`”, while [flow-format §3] “a transition MUST NOT be named `any`” reserves nothing else. The metric is defined [flow-format §6] “where `t.transition` is the transition's name and the event is not imported, migrated or redacted (ADR-0146)”.
- **Severity:** changes-outcome
- **Suggested fix:** Reserve the engine's event names as transition names, or say how a filter and `transition_count` tell them apart.

#### 27. The objects a refusal names: references inside an aggregate's collection or `where`, and in a conditional's untaken branch

- **Construct:** the objects an unsatisfied clause or invariant names.
- **Case 1:** servicedesk `Change.cab_approved` fails. The outermost aggregate ranges over `affected_service.change_approvers`, which are identities, and the clause also evaluates the reference `affected_service`, a `Service`. Case 2: the type-scan invariant `none(o in Approval where o.subject == subject and o.approver == approver)` of declaration-syntax §3.2 fails. Case 3: a guard `if inputs.urgent then inputs.a.state == X else inputs.b.state == Y` fails with `urgent` false.
- **The readings:** In case 1, (a) the `Service` is named, as a reference the clause evaluates, or (b) an aggregate contributes only its elements, which are values, so nothing is named. In case 2, the refusal names (c) the conflicting approval only, or (d) also the delivery that `subject` evaluates to. In case 3, it names (e) `b` only, or (f) `a` too, by "evaluated in full".
- **Where the text fails:** [DESIGN §5.5] “The objects are every object a reference in the failing clause evaluates to when the clause is evaluated in full, every operand of `and`, `or` and `implies` and the derived attributes it reads included” and [DESIGN §5.5] “where it evaluates a collection, the elements the outermost aggregate's `where` selects, or every element where it has none, so an empty collection contributes none, a nested aggregate adding none, a name an aggregate binds being no reference”. Case 1 is [servicedesk.yaml] “all(a in affected_service.change_approvers: any(o in approvals where o.stage == ReviewStage.CAB”.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether references in an aggregate's collection expression and inside its `where` (other than the bound name itself) contribute, and whether "in full" extends to both branches of `if … then … else`.

#### 28. `InvariantViolated.objects` when several failing objects conflict with each other

- **Construct:** the objects an invariant refusal names.
- **Case:** one request creates A and B with the same value of a `unique: true` attribute. Each fails `code_unique`, conflicting with the other.
- **The readings:** (a) The exclusion is per failure, then the lists are merged: `objects = [A, B]`. (b) The exclusion runs over every failing object: `objects = []`. (c) Only "existing holders" are conflicting objects, which these are not: `[]`.
- **Where the text fails:** [DESIGN §5.5] “and the conflicting objects, chosen as an unsatisfied clause's are, the object the invariant fails on excluded (ADR-0122)” for a refusal with one list, [library-api §4] “# every object in the conflict”, and [flow-format §4.12] “A duplicate's refusal names the written object, and as its conflicting objects the existing holders, which are not themselves re-checked.”
- **Severity:** changes-outcome
- **Suggested fix:** Say whether the exclusion is per failure or over the whole refusal, or give each `InvariantFailure` its own conflicting objects.

#### 29. A cascaded part's refusal must name the part, and `Unsatisfied` has no field for it

- **Construct:** the verdict of a part a cascade drives.
- **Case:** `Delivery.cancel` cascades `ChecklistItem.discard`, here given a guard that fails on item 7.
- **The readings:** (a) Item 7 goes into `Unsatisfied.objects`, against its "other than the one it is evaluated on". (b) The part is not named, since there is no field for it. The same holds for an `unavailable` part, since `Unavailable` carries only a state.
- **Where the text fails:** [DESIGN §5.5] “a part a cascade drives refuses with its own verdict instead, naming the part” and [flow-format §4.8] “and one in a cascade with the part's own verdict, naming the part, as any caused refusal does”, while `Unsatisfied.objects` is [library-api §4] “other than the one it is evaluated on”.
- **Severity:** changes-outcome
- **Suggested fix:** Say where a part's verdict names the part: a field on the verdict, or an exception to the `objects` rule.

#### 30. The strings that identify a step, a loop and a cascade clause are unspecified

- **Construct:** `CallRefused.step`, `OverLimit.loop`.
- **Case:** customers `forget`'s `call c.forget` inside its `foreach` is refused, and in another request a cascade clause exceeds its `limit`.
- **The readings:** `step` is an index path (`effect[0].foreach.steps[0]`), the target transition (`Contact.forget`), the loop's item name, or something else. `loop` for a cascade is the end's name, the clause index, or the part transition. Only `group erasure` is named, and the step a generated `forget` is taken by is not.
- **Where the text fails:** [library-api §4] “# the effect step whose call or creation was refused” and [library-api §4] “loop: str”, while the verdict [DESIGN §5.5] “names the step that made it, the object it targeted, a loop's element among them”. The one name given is [DESIGN §8] “its step named `group erasure`”.
- **Severity:** changes-value
- **Suggested fix:** Define the format of a step's and a loop's identifier, including for a cascade clause and for `forget` taken without a declared step.

#### 31. How an inherited invariant is named in a refusal, an admission and an example

- **Construct:** abstract bases, invariant names.
- **Case:** base `Asset` declares invariant `named`, and `Laptop` extends it. A `Laptop` violates `named`. Or the inherited `key_unique` (from `Asset.key`, which is `unique: true`) is violated, an assertion's `may_admit` names it, or a migration `admit`s it.
- **The readings:** (a) It is the subtype's own: the clause is `named`/`key_unique`, the admission is `named` or `Laptop.named`. (b) It is the base's: `Asset.named`/`Asset.key_unique`, "qualified where it is another type's". An example expecting one fails under the other.
- **Where the text fails:** [flow-format §11.4] “for `invariant_violated`, the invariant, qualified where it is another type's”, with [flow-format §4.14] “a `unique` declared on the base spanning the whole family, since the type scan it expands to ranges over every subtype (ADR-0146)” and admissions [flow-format §4.13] “or `<Type>.<invariant>`, an invariant of a type reachable from it by relationships whose ends name each other”. The servicedesk base declares [servicedesk.yaml] “identifier: { sequence: asset_key, format: "IT-{n}" }” with `unique: true`.
- **Severity:** changes-outcome
- **Suggested fix:** Say how an inherited invariant is named in a verdict, a `may_admit`, an `admit` and an example's `clause`.

#### 32. The order of inherited invariants across several levels of base, and a category that cannot occur

- **Construct:** the order of a type's invariants in a refusal.
- **Case:** `C extends B extends A`, each declaring one invariant, and one request violates all three.
- **The readings:** (a) "The base's order" applied recursively, own first: `[C.i, B.i, A.i]`. (b) Inherited ones root first, as for `<attribute>_unique`'s "inherited attributes first": `[C.i, A.i, B.i]`. The refusal's remedy is the first invariant's, so it can differ. Separately, "those a bound machine declares" describes nothing in the flow format, where a machine only requires invariants.
- **Where the text fails:** [DESIGN §5.5] “its own declared ones in the order written, a required invariant a machine names among them, then inherited declared ones in the base's order, then those a bound machine declares, in its order”, and [DESIGN §5.5] “The refusal's is the first named invariant's (ADR-0122)”. A base may have a base, [flow-format §4.14] “so a base may extend another”, and [flow-format §3] “a machine declares, in this order, `description`, `requires`, `states`, `conditions` and `transitions`”.
- **Severity:** changes-outcome
- **Suggested fix:** Give the order across several levels of base, and drop or define the machine-declared category.

#### 33. External identifiers: unique per type or per source, and what invariant a duplicate violates

- **Construct:** uniqueness.
- **Case:** customer X requests `link_xero` with a `xero_contact_id` that customer Y holds. Separately, a `Supplier` type with `external: xero` holds the same value as a customer.
- **The readings:** (a) A generated invariant refuses it as `invariant_violated`, named `xero_contact_id_unique` or something else, in an unstated place in the order. (b) Some other refusal. For two types of one source: (c) refused, since `lookup(source, value)` returns one object, or (d) allowed, unique per type.
- **Where the text fails:** [flow-format §4.3] “the system that owns the value, such as `xero`: its values are unique for each source” and [DESIGN §5.2] “naming the system that owns the value, with uniqueness per source automatic (ADR-0037)”, while [flow-format §3] “`<attribute>_unique` is the invariant a `unique` attribute, or the stored end of a one-to-one relationship, generates, and the name a refusal of a duplicate carries” omits it. `lookup` returns [DESIGN §10] “The object whose external identifier for that source matches”.
- **Severity:** changes-outcome
- **Suggested fix:** Say what invariant an external identifier generates, its name and place in the order, and whether its scope is the type or the source across the store.

#### 34. When an identifier is minted relative to the effect's steps

- **Construct:** the effect's step order, identifier minting.
- **Case:** a creation whose effect is `[create Child(parent_no: number), assign: { location: owner, expr: inputs.o }]`, where `number` is an identifier scoped by `owner`.
- **The readings:** (a) It is minted after the attribute inputs and before every effect step, so the scope `owner` is still absent at the mint, which then has no scope value. (b) It is minted after the effect's writes, so after the `assign`. The `create` before it then reads an absent `number`, or the mint moves before the `create`, against step order.
- **Where the text fails:** [flow-format §4.12] “An identifier is minted during the creation, after the creation's writes and before its effect's other steps”, and the scope may be written by an effect, [flow-format §4.12] “a required input of every initial transition, or an attribute each assigns a value that is provably present”, while [DESIGN §6] “write its new state, then run its effect in step order, attribute writes, calls and creations interleaved as the steps come” does not mention the mint.
- **Severity:** changes-value
- **Suggested fix:** Fix the mint's position: after the attribute inputs, or after a named class of effect steps. State in DESIGN §6 step 5 how a scope written by an effect step is reached.

#### 35. Loop order over values of types the rule does not cover

- **Construct:** loop order over values.
- **Case:** `foreach t in inputs.when` (a `timestamp[]`) with step `assign: { location: last_seen, expr: t }`. Or loops over `date[]`, `money(SGD)[]`, `bool[]`, `identity[]`, `file[]` or `event[]` that create objects.
- **The readings:** Timestamps, dates and durations ascend by time, or by their text, or keep input order. `money` counts as a number, or does not. Identities ascend by code point, since "compared as a string", or keep input order. `bool`, `file` and `event` have no stated order. The final `last_seen`, and the order of creations and their ids, differ. The rule's last sentence also orders "that snapshot in ascending id order", which a collection of values does not have.
- **Where the text fails:** [flow-format §4.8] “a collection of values running in ascending value order, strings by code point, numbers by value and enumeration members in declaration order, and a range from 1 up (ADR-0147)” and [flow-format §4.8] “and the loop runs over that snapshot in ascending id order even where its steps change the collection (ADR-0122)”, over the types [flow-format §4.3] “a built-in type (`string`, `bool`, `int`, `decimal(p,s)`, `money(ccy)`, `timestamp`, `date`, `duration`, `identity`, `actor_kind`, `file`, `event`)”.
- **Severity:** changes-value
- **Suggested fix:** Give the order for every element type a set may hold, and limit "ascending id order" to collections of objects.

#### 36. Names an effect binds: two bindings of one name

- **Construct:** names an effect binds.
- **Case:** `effect: [create {type: T, transition: spawn, result: n}, create {type: T, transition: spawn, result: n}, supersede: n]`. Or a `create` inside a `foreach` whose `result` is the loop's `item`.
- **The readings:** (a) Refused at step 3 as a duplicate binding. (b) Accepted, with the later binding shadowing the earlier one, so `supersede: n` names the second object. (c) Accepted, with the first binding kept. Whether an observation's collection name or a derived attribute counts as "a member of the type" is not said either.
- **Where the text fails:** [flow-format §4.8] “a name a `create` or a loop binds MUST NOT be a member of the type or a word an expression resolves first or every object has” and [flow-format §4.8] “and a nested loop MUST NOT reuse its loop's name”, with [flow-format §4.8] “`result` names the new object for the steps after it, within the loop that binds it where a loop does”.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a name may be bound twice in one effect, at one level or across nested levels, and whether observations and derived attributes are members for this rule.

#### 37. A mirror family cannot be written: an abstract base cannot be a mirror

- **Construct:** abstract bases with mirrors.
- **Case:** a legacy asset schema with base `Asset` and kinds `Laptop` and `Server`, all ported as mirrors.
- **The readings:** (a) `Asset` must be abstract, and the schema forbids an abstract mirror, so the family is inexpressible. (b) A concrete mirror may extend an abstract base that is not a mirror, since a base with no objects is not "a type this store owns", so the family is written that way. (c) That is refused by check 53 as joining a mirror to an owned type.
- **Where the text fails:** [declaration-syntax §2] “since every type is ported as one (ADR-0101): a delivery and its checklist items, or a family, are mirrors together” and [flow-format §4.15] “while mirrors may extend and compose with each other (step 4, `check 53`)”, while [flow-format §4.14] “A base MUST be declared in the module or imported and MUST be abstract”. The schema's abstract branch lists `mirror: true` among what an abstract type may not declare (flow.schema.json, the third branch of the type's `oneOf`, paraphrased), and §4.15 forbids a mirror [flow-format §4.15] “or join it to a type this store owns by `extends` or a composition”.
- **Severity:** changes-outcome
- **Suggested fix:** Say whether an abstract base may be a mirror, or whether a mirror may extend a non-mirror abstract base, and make the schema agree.

#### 38. The delivery examples use `Agent`, from a module `delivery` does not import

- **Construct:** proposals in examples; name resolution in an examples file.
- **Case:** `delivery.examples.yaml`'s three proposal examples create `Agent` objects and file proposals as them.
- **The readings:** (a) An examples file resolves names against its module's closure, `delivery` plus `inventory`, so `Agent` is unresolved and the file fails step 3, and those examples are never run. (b) It resolves against every module given to the checker, or installed, so the file is valid.
- **Where the text fails:** [delivery.yaml] “imports: { inventory: [InventoryItem] }” and [delivery.examples.yaml] “- request: { type: Agent, transition: issue, as: scout, inputs: { key_id: scout, name: Scout } }”, against [flow-format §11.5] “at step 3 against those modules: every name of §11.1 to §11.4 resolves” and, for descriptions, [flow-format §3] “a reference MUST name a type the module declares or imports (step 3, `names`)”.
- **Severity:** changes-outcome
- **Suggested fix:** State the scope an examples file's type names and actors resolve in, at check and at publish, and either import `people` in `delivery.yaml` or say why it need not.

#### 39. `DeclarationChange`'s stated remedies contradict the remedy classes' definitions

- **Construct:** the `DeclarationChange` lifecycle table.
- **Case:** `submit` is refused by `dry_run_clean`, and `publish` by `fatal` or `impact_unchanged`.
- **The readings:** Both builders return `self_serviceable`, since it is stated. But no argument of `submit` or `publish` cures either. The cure is another transition (`revise`, `refresh`), which the definitions call `unreachable_from_here`, and the format's own notice flags `self_serviceable` on a transition taking no input.
- **Where the text fails:** [DESIGN §5.5] “| `self_serviceable` | Satisfiable by an argument of the transition requested (ADR-0122) | Supply it, or correct it |”, [DESIGN §9] “| `submit` | external | `DRAFTED` → `SUBMITTED` | `dry_run_clean`, the dry run finds nothing fatal, `self_serviceable` |”, and [flow-format §4.7] “a condition whose transition takes no input is never `self_serviceable`”.
- **Severity:** wording
- **Suggested fix:** Either give these guards the class the definitions imply, or say why a built-in's stated class departs from them.

#### 40. The library's `Unsatisfied.objects` comment omits the generated guards' exception

- **Construct:** the objects a refusal names, in the library API.
- **Case:** `successor_not_superseded` refuses a customer merged into itself, and names that customer.
- **The readings:** No different engine if DESIGN is read, but the comment, read alone, excludes the very object the generated guard names.
- **Where the text fails:** [library-api §4] “# the objects the failing clause reads evaluated in” and [library-api §4] “full, other than the one it is evaluated on”, against [DESIGN §5.5] “A generated guard names its stated objects even where one is the object it is evaluated on (ADR-0147)”.
- **Severity:** wording
- **Suggested fix:** Cite the exception in the comment, or point to §5.5 for the whole rule.

#### 41. "Only via" a publish, an erasure or an upgrade, for transitions that are "not transitions the request took"

- **Construct:** `Proposal.invalidate` and `DeclarationChange.supersede`.
- **Case:** an erasure invalidates a proposal, and an upgrade supersedes a change.
- **The readings:** `only_via` names transitions that take this one by a call, a create or a cascade. A publish is a transition, but an erasure is any type's, and an upgrade is not a transition at all. `cascaded` excludes the invalidation, as the text says. No engine differs, but the marking is used outside its definition.
- **Where the text fails:** [DESIGN §9] “and `invalidate`, out of `PENDING` too, is only via a publish or an erasure (ADR-0147)”, [flow-format §11.4] “a `redact` event and a proposal's invalidation are not transitions the request took (ADR-0146, ADR-0147)”, and [flow-format §4.8] “the transitions, written `Type.transition`, that alone may take this one, by a `call`, a `create` or a cascade”.
- **Severity:** wording
- **Suggested fix:** Describe these as taken by the engine on a publish, an erasure or an upgrade, rather than as `only via`.

#### 42. Whether an approval's `cascaded` includes the proposal's own transition

- **Construct:** `cascaded` for an `approve` example.
- **Case:** `an_agents_proposal_is_executed_when_approved`.
- **The readings:** The proposal's `approve` (to `EXECUTED`) is a transition on another object, so by the definition it could be listed. The example expects only `InventoryItem.deliver_externally`, which settles it for that example, but the text does not state the rule.
- **Where the text fails:** [flow-format §11.4] “an approval under test, written as a step's: the expectation is of the request it executes” and [delivery.examples.yaml] “expect: { verdict: applied, state: DELIVERED, cascaded: [InventoryItem.deliver_externally], proposal: executed }”.
- **Severity:** wording
- **Suggested fix:** Say that an approval's `cascaded` excludes the proposal's own transition.

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 26 | 2, 3, 4, 5, 6, 7, 8, 10, 11, 14, 15, 19, 21, 22, 23, 24, 26, 27, 28, 29, 31, 32, 33, 36, 37, 38 |
| changes-value | 12 | 1, 9, 12, 13, 16, 17, 18, 20, 25, 30, 34, 35 |
| wording | 4 | 39, 40, 41, 42 |
| **Total** | **42** | |

## 3. Publishing with a migration, the import, the harness and an engine upgrade, as the reader wrote it

**What I read.** `docs/design/publish-and-import.md` whole. `docs/design/flow-format.md` §4.15, §4.16, §5, §6 (the conversion table, which sits between §5 and §7) and §7 with §7.1. `docs/DESIGN.md` §5.9, §5.12, §6, §7, §9, §11, §14. `docs/design/declaration-syntax.md` §2 with §2.1, §6.6, §6.9, and §10: I read the whole check table (rows 1 to 64), not only rows 22, 23 and 60, and the "Reported without failing" paragraph after it. `docs/design/storage-schema.md` §3 (with §3.1 to §3.4), §6, §8, §9, §10, §11 (it lies inside the range I printed) and §12. `docs/design/adversarial-harness.md`, its preamble of amendments, §1, and the first lines of §2. `docs/design/library-api.md`, the definitions of `import_batch`, `publish`, `PublishResult`, `EnteredAt`, `ImportedObject`, `ImportBatch`, `ImportReport` and `LegacyInterval`, and the bullets after the operation list. The examples `service.yaml`, `service-v2.yaml`, `inventory.yaml` with `inventory-v2.yaml` (I read them by diff and in full for v2), and `people.yaml`. **Places I went outside those sections, following citations:** `flow-format.md` §2 rule 2 (the order of sections), §4.3 (the attribute keys, where `survives: true` is defined), §4.12 (the "Uniqueness" paragraph), and §11.2 and §11.5 (an example's clock); `declaration-syntax.md` §3.2 (the three `survives` forms, which I read by search only); `DESIGN.md` §5.5 (availability is three-way), §5.6 (invariants) and §8 (erasure: the paragraph on a publish that newly marks an attribute personal, and the bullets on erasure's admissions); `library-api.md` §5 (`Availability`, `Object`, `Interval`; the import's dataclasses, which I was sent to, are in §5 as well) and §7 (the exceptions table). I also printed the heading lists of each file. **What I did not read:** anything under `docs/adr/`, `docs/design/defects.md`, `TODO.md`, `docs/LESSONS.md`, any `*review*.md`, `docs/design/reimbursement-cold-draft.md`, the case studies, `scripts/`, `.remember/`, the untracked `docs/design/jira-flows.html`, and no git history. **Quotations:** every quotation below is written as («…») after the file it comes from, and a Python script checked each one, character for character, against that file. All of them passed (see the end of the report).

### What is determined

- **The shape of approval.** `submit` and `refresh` run steps 1 to 4 and attach a report with a fixed `now`. `publish` re-runs them at that `now`. It is refused as `current`, `fatal`, `impact_unchanged` or `decision_unresolved`, and the change stays `SUBMITTED`. A refusal for any reason leaves the store untouched, because the publish is one transaction. The changes it supersedes are reported but not compared. An approval is refused if any object or proposal joins a category, which is how a newly created object or a new violation forces a `refresh`.
- **Order inside the transaction**, as `publish-and-import.md` §1 step 5 gives it: write `of_declaration`, then add columns and do renames, then widen each `CHECK`, then apply the mappings, then check invariants, then replace each `CHECK` and apply `NOT NULL`, then open and close intervals, then redact, then supersede and invalidate. Every event of the publish occurs and is recorded at the approval.
- **Mapping order:** renames, then removed members, then removed states, then backfills in the order they are written. Each mapping reads the object as the earlier ones left it. A set that already holds the target keeps it once.
- **What a migration event is:** one migration per object with a changed row. Its transition is `migrate`, its source is `migrated`, it is caused by the publish event, and its actor is the publishing actor. A rename writes no event. An object no mapping changed gets a `migrated` event only when an admission must be recorded on it.
- **The service v1→v2 pair works out without ambiguity.** For four jobs J1 `OPEN`, J2 `WORKING`, J3 `DONE` with no photo and J4 `DONE` with a photo: the rename is DDL only. `SKILLS→WORKLOAD` reaches no row, since no attribute holds `ReassignmentReason`, and no proposals exist, since `reassign` is not proposable. `site := "HQ"` changes all four rows, so there are four migration events in id order. `photo_when_done` fails on J3 alone, the `admit` covers it, and the admission goes on J3's event, with a notice for an admit on an object in a final state. The invariant is not compiled. `site` gets `NOT NULL` after the mappings. `photo_attached` becoming `deny` names no object, because it reads an input. I worked the units pair in the appendix the same way: R2 `LEGACY_HOLD`→`AVAILABLE`, R3 `OBSOLETE`→`FAILED`, every robot backfilled, and R4, retired with no reason, admitted. Its only open question is finding 4.
- **Severities that are settled:** `fatal` covers a removed state left unmapped (check 23), a removed member that live objects or any observation hold, a new required value left absent, and stranding. `decision` covers an invariant violation. The notices are those listed in §2.
- **The import is settled at the level of:** one transaction per batch; only mirrors and their datapoints; the built-in assertion with no generated guards; source `asserted`, marked `imported`, with `state_source` `imported`; ids assigned before rows; foreign keys deferred for cycles; erased personal attributes skipped; legacy entries kept through erasure by the fields the mapping lists; a refresh that changes nothing writes nothing; a refresh whose times go back before the latest entry is refused; a disposition file that may not contain `UNDECIDED`; `ADMIT` only on `INVARIANT` classes; and `CLEAN_UPSTREAM` refusing the batch.
- **The upgrade's draining protocol** and `EngineMismatch` are settled.
- **An example's clock** starts at `2026-01-01T00:00:00Z` (`flow-format.md` §11.2), so the fixed `now` does not reach the examples.

### Findings

#### 1. Which objects the invariants are evaluated over at a publish

- **Construct:** the invariant check of the dry run (step 3) and of the publish (step 5).
- **Case:** v2 adds `invariant closed_with_note: state != CLOSED or note is not null` to type `Ticket` and carries no mapping for `Ticket`. Ticket T1 is `CLOSED` with no `note`. The same arises at a cutover publish that only removes `mirror` and adds owned invariants.
- **The readings:** (a) Invariants are evaluated only over "every object a mapping wrote and every object whose invariant reads one". No mapping wrote T1, so the violation is never seen and v2 installs. T1 then violates an invariant with no admission, which fails harness row 2. (b) Invariants are evaluated over every object as the mappings leave it. T1 is a decision, and approval is refused as `decision_unresolved` until an `admit` covers it. Under (a) there are further splits: whether a rename counts as "wrote", since it writes no event, and whether a backfill that leaves a row unchanged does.
- **Where the text fails:** (`publish-and-import.md` §1 step 3) «evaluate every invariant, a new one included, once, after all the mappings, over every object a mapping wrote and every object whose invariant reads one»; (`publish-and-import.md` §1 step 5) «check every invariant once, after all the mappings, over every object a mapping wrote and every object whose invariant reads one»; (`DESIGN.md` §6) «its step 7 checks every invariant once, after all the mappings, over every object a mapping wrote and every object whose invariant reads one». Against: (`flow-format.md` §4.16) «evaluates every invariant, a new one included, over every object as the mappings leave it»; (`DESIGN.md` §5.9) «is checked against every object as the mappings leave it»; (`declaration-syntax.md` §10, check 23) «over every object as the mappings leave it»; and within the same file, (`publish-and-import.md` §4) «the owned version's violations surface in the cutover publish, whose `admit` is counted», which only reading (b) delivers.
- **Severity:** changes-outcome (a contradiction).
- **Suggested fix:** State one set of objects for both the dry run and the publish. If that set is "every object of every type whose invariants changed, plus those a mapping wrote, plus those whose invariants read either", say so in all four places.

#### 2. Which objects receive a migration event

- **Construct:** one event per object.
- **Case:** v3 adds `backfill: Job: { priority: 'if priority is null then Priority.NORMAL else priority' }` over 100 jobs, 60 of which already hold a priority. Separately, a v3 whose only mapping is a rename.
- **The readings:** (a) An event goes only to rows that change: 40 events for the backfill, and none for the rename. (b) Every object of the type is "reached" and gets one event carrying every mapping that reaches it: 100 events, and one per object for the rename as well.
- **Where the text fails:** (`DESIGN.md` §5.9) «one event per object, carrying every mapping that reaches it, with source `migrated`, caused by the publish event, objects taken in ascending id order (ADR-0105, ADR-0146). It reaches every object of the type, final ones included»; (`declaration-syntax.md` §6.6) «a migration reaches **every object the change affects, terminal ones included**»; against (`publish-and-import.md` §1 step 5) «apply the mappings as **one recorded migration transition per object** whose row they change, in ascending id order».
- **Severity:** changes-value.
- **Suggested fix:** Say in `DESIGN.md` §5.9 and `declaration-syntax.md` §6.6, as `publish-and-import.md` does, that the event goes to each object whose row the mappings change, plus admission-only objects.

#### 3. "Each type the change touches" is undefined, so which objects step 3 reads is undefined

- **Construct:** the objects the dry run reads, which decides the gained and lost transitions and the refusals of newly enforced clauses.
- **Case:** `Order.ship` has the guard `unit.leasable` (a path to `Unit`). v2 changes `Unit.leasable` and leaves `Order`'s text unchanged. `Order` does not depend on `Unit` by machine, enum, sequence, evaluator or base, so its version does not advance.
- **The readings:** (a) "Touches" means a type whose computed version advances. `Order`'s objects are not read, and no `Order` is reported as gaining or losing `ship`. (b) "Touches" includes every type whose rules read something changed. The orders that lose `ship` are listed, and they take part in `impact_unchanged`.
- **Where the text fails:** (`publish-and-import.md` §1 step 3) «of each type the change touches, and of every type whose invariant reads one, final ones included»; (`DESIGN.md` §5.9) «Because a type's behaviour depends on the machine, enums, sequences, evaluators and base types it uses, advancing any of those requires advancing every dependent type».
- **Severity:** changes-value (the report's notices, and its ids for `impact_unchanged`).
- **Suggested fix:** Define "touches", either as a type whose version advances or as every type any of whose rules reads a changed declaration.

#### 4. Gained and lost transitions: which state, how many categories, and at what grain approval compares

- **Construct:** the gain/loss category and the `impact_unchanged` comparison.
- **Case:** the units pair in the appendix. R2 is in `LEGACY_HOLD`, which v2 maps to `AVAILABLE`, and `name` narrows its `from` from `[AVAILABLE, LEGACY_HOLD]` to `AVAILABLE`.
- **The readings:**
  - (a) Availability is judged in the object's current state under both versions, so R2 "loses" `name` and `retire` (a narrowed `from`) and gains nothing.
  - (b) The new availability is judged in the state after the mappings, `AVAILABLE`. R2 keeps `name`, `retire` becomes "available with input", and nothing is lost but `release`.
  - Separately, "gained or lost" may be one category or two. It may also be keyed per transition, as the rewrite category is keyed per mapping. Under (c), one category, an object that moves from "gained" to "lost" between submit and approval is not new, and the approval installs. Under (d), two categories or per-transition keys, it is new, and the approval is refused as `impact_unchanged`.
  - The same question applies to a rewrite category: an object that is rewritten by a second mapping at approval is in the same category but under a new key.
- **Where the text fails:** (`publish-and-import.md` §1 step 3) «the objects that would gain or lose an available transition under the new guards, a removed transition and a narrowed `from` being losses»; «The report keeps the ids of each, keyed by category, a mapping's per mapping and a violation's per invariant.»; (`publish-and-import.md` §1 step 5) «as `impact_unchanged` if it names, in any impact category, an object or proposal the attached one did not name in that category».
- **Severity:** changes-outcome (`impact_unchanged`).
- **Suggested fix:** State the state in which the new availability is judged, list the impact categories by their keys, and say whether approval compares per key or per category.

#### 5. A new invariant of an observation kind: notice or decision

- **Construct:** severities, and observations read for a kind's invariants.
- **Case:** v2 adds `value_in_range` to `InspectionResult` (in `service.yaml` it already exists; suppose it is new), and an observation already recorded has `value = 75`.
- **The readings:** (a) It is a notice, never admitted, and the publish installs (`flow-format.md`, `DESIGN.md`). (b) `publish-and-import.md`, which `storage-schema.md` names as the owner of what publishing reports, lists the severities exhaustively and has no such notice. The observation is "an invariant an object would violate after the publish", which is a decision. A decision cannot be admitted, since an admission may not name a kind's invariant, so the publish can never install. It is also unsaid whether the observation's id is an impact category for `impact_unchanged`, and what an invariant changed, rather than new, does.
- **Where the text fails:** (`publish-and-import.md` §2) «Each impact has its severity (ADR-0146).»; «A decision: an invariant an object would violate after the publish»; (`publish-and-import.md` §1 step 3) «observations are read for a removed enumeration member and for a kind's invariants»; against (`flow-format.md` §4.16) «A new invariant of an observation kind applies to recordings after the publish; the observations already recorded that violate it are reported as a notice and never admitted, since no migration reaches an observation (ADR-0146).»; (`flow-format.md` §4.16) «an admission MUST name an invariant of the type, not of an observation kind»; (`storage-schema.md` §10) «What publishing checks, what it reports and when it refuses is [`publish-and-import.md`](publish-and-import.md)».
- **Severity:** changes-outcome.
- **Suggested fix:** Add the kind's invariant notice to §2's list of notices, and say whether its observations are compared at approval.

#### 6. "An attribute made required counting as new": fatal even when no object lacks it?

- **Construct:** fatal impacts and check 23.
- **Case:** v1 `contact: { type: string, optional: true }`; v2 makes it required, with no backfill. All ten live objects hold a contact.
- **The readings:** (a) It counts as a new required attribute on a type with live objects and no `backfill`, which check 23 refuses, so the change is fatal. (b) Only objects "lacking" the value are named, here none, so v2 installs and `NOT NULL` is applied.
- **Where the text fails:** (`publish-and-import.md` §2) «a new required value with no `backfill`, an attribute made required counting as new, or one a backfill leaves absent»; (`publish-and-import.md` §1 step 3) «those lacking a required attribute with no `backfill`, an attribute made required counting as new, or one a backfill leaves absent»; (`declaration-syntax.md` §10, check 23) «Also a new required attribute or singular reference on a type with live objects and no `backfill`»; (`storage-schema.md` §10) «an attribute made required is treated the same way».
- **Severity:** changes-outcome.
- **Suggested fix:** Say whether a made-required attribute is fatal by the type having objects or by objects lacking the value.

#### 7. Impacts and reports given no severity

- **Construct:** §2's severity list against step 3's categories.
- **Case:** v2 maps `LEGACY_HOLD→AVAILABLE`, which names R2 in "objects in a removed state" and every robot in "objects each mapping will rewrite". It also carries an `admit` that no object needs.
- **The readings:** (a) Each of these is a `NOTICE` `Finding`. (b) They are ids in `affected` only, with no finding. (c) An unused `admit` is a notice, a decision or nothing.
- **Where the text fails:** (`publish-and-import.md` §2) «Each impact has its severity (ADR-0146).» followed by lists that omit both categories; (`declaration-syntax.md` §6.6) «An `admit` naming an invariant the publish does not leave violated on any object is reported».
- **Severity:** changes-value.
- **Suggested fix:** Give every category of step 3, and the unused `admit`, a severity in §2.

#### 8. `PublishReport.accepted` is not defined

- **Construct:** the report's shape.
- **Case:** a dry run with no fatal finding and one unresolved decision.
- **The readings:** (a) `accepted` is `True`, since nothing is fatal (`submit`'s `dry_run_clean`). (b) `accepted` is `False`, since approval would refuse it.
- **Where the text fails:** (`publish-and-import.md` §2) «accepted: bool», which appears with no comment and is used nowhere else in the text.
- **Severity:** changes-value.
- **Suggested fix:** Define `accepted` against `fatal` and `decision_unresolved`.

#### 9. Whether a refused approval, or a refused import batch, is written to the attempt log

- **Construct:** the approval's refusals, and harness row 10.
- **Case:** `publish` is refused as `impact_unchanged`.
- **The readings:** (a) It is a refused request on the `DeclarationChange`, so it gets an attempt row (`DESIGN.md` §6). (b) `publish` is an "operational call", whose refusal is no attempt, so no row is written and the harness does not expect one. The same holds for `import_batch`.
- **Where the text fails:** (`publish-and-import.md` §1) «The `publish` transition is the approval and runs step 5.»; (`DESIGN.md` §6) «is written to the **attempt log** in its own short transaction after the rollback»; against (`library-api.md` §6) «the operational calls that are not object operations, `acknowledge` and `publish`»; (`adversarial-harness.md` §1) «an operational call's refusal being no attempt».
- **Severity:** changes-value.
- **Suggested fix:** Say whether a refused `publish` transition and a refused batch produce attempt rows.

#### 10. What a "standing admission" covers when the invariant's text changes

- **Construct:** decisions, and the standing admission.
- **Case:** under v1, `qty_ok: qty >= 0` is admitted on O1 (`qty = -1`). v2 keeps the name and tightens it to `qty >= 5`. O1 still violates.
- **The readings:** (a) An admission covers by invariant name. O1 is no decision, and v2 installs. (b) An admission covers the invariant as it was admitted. O1 is a decision, and approval is refused as `decision_unresolved` until an `admit` covers it. It is also unsaid whether a removed invariant's standing admissions are discharged.
- **Where the text fails:** (`publish-and-import.md` §1 step 3) «reporting each violation no standing admission covers, with its objects, as a decision»; (`publish-and-import.md` §1 step 5) «a violation a standing admission covers needing none».
- **Severity:** changes-outcome.
- **Suggested fix:** State what identifies an invariant across versions for an admission (its name or its text), and what happens to admissions of an invariant a version removes.

#### 11. A backfill's `declaration_version`

- **Construct:** what a backfill reads.
- **Case:** the store is at version 5. Object O was last written under version 3 (§5: it keeps "its own"). The v6 backfill is `tier := if declaration_version < 4 then "legacy" else "current"`.
- **The readings:** (a) "The version before the publish" is version 5 for every object, so O gets `"current"`. (b) It is the object's own value before the publish, 3, so O gets `"legacy"`.
- **Where the text fails:** (`flow-format.md` §4.16) «its `declaration_version` being the version before the publish (ADR-0147)»; (`publish-and-import.md` §1 step 5) «with the version before the publish as its `declaration_version`»; against (`flow-format.md` §5) «`declaration_version`, the version of its latest event, so a migration moves it and an object no publish writes keeps its own (ADR-0146)».
- **Severity:** changes-value.
- **Suggested fix:** Say whether a backfill reads the installed version or the object's own pre-publish value.

#### 12. A backfill that names an observation kind

- **Construct:** backfills and their target.
- **Case:** `backfill: { InspectionResult: { note: '"n/a"' } }`.
- **The readings:** (a) It is refused, since a migration never reaches an observation. (b) It is accepted, and it writes nothing. (c) It is accepted and applied, since an observation kind "is a type". Nothing in §4.16's rules or step 3's `names` list refuses it, although a rename and an admission are both explicitly kept off a kind.
- **Where the text fails:** (`flow-format.md` §4.16, the `backfill` row) «for each type, each attribute every object of it is given»; (`storage-schema.md` §3) «**An observation kind is a type** and gets a table like any other (ADR-0082)»; (`DESIGN.md` §5.9) «never an observation and never a retired type's object (ADR-0147)».
- **Severity:** changes-outcome.
- **Suggested fix:** State that a backfill naming an observation kind is refused, and name the check that refuses it.

#### 13. Stranded parts: judged before or after the part's own mapping

- **Construct:** the fatal impact for stranding.
- **Case:** v2 removes `Delivery.HOLD→DONE` (final) and `ChecklistItem.PAUSED→DELETED` (final). Delivery D is in `HOLD` and has item I in `PAUSED`.
- **The readings:** (a) Whether I is "not in a final state" is judged as the objects stand. I is `PAUSED`, so v2 is refused as fatal. (b) It is judged after all the mappings in simulation. I is `DELETED`, so v2 installs.
- **Where the text fails:** (`publish-and-import.md` §3) «a publish whose objects in the removed state have parts the mapping would strand is refused, a part being stranded when it is not in a final state and its end does not declare `survives: true`»; (`publish-and-import.md` §2) «an object whose parts a mapping into a final state would strand (ADR-0147)».
- **Severity:** changes-outcome.
- **Suggested fix:** Say that stranding is judged on the state after all the mappings, or before them.

#### 14. Discharging an admission during a migration, or by an import's refresh

- **Construct:** a standing admission and recompilation.
- **Case:** R4 holds the admission `retired_with_reason`. The v3 backfill `retirement_reason := if retirement_reason is null then RetirementReason.LOST else retirement_reason` makes the invariant hold. In a second case, v3 relaxes an admitted invariant and no mapping writes the object. In a third, a mirror object admitted at import is refreshed with a value that satisfies the invariant.
- **The readings:**
  - (a) The migration runs DESIGN §6's step 7, which discharges R4's admission at its migration event. The invariant is then compiled at the next publish.
  - (b) The publish's own step 7 (`publish-and-import.md`) records admissions only, so R4 stays in `exceptions` until a transition writes it, which in a final state is never. The invariant stays uncompiled for good.
  - In the second case, an object nothing writes cannot be discharged under either reading, unless something records an event on it.
  - In the third case, it is unsaid whether a refresh discharges an import's admission.
- **Where the text fails:** (`DESIGN.md` §6) «A **migration** a publish applies runs steps 5 to 8 once for each object»; (`DESIGN.md` §6 step 7) «an admitted violation whose invariant now holds is discharged, and the discharging position recorded»; (`publish-and-import.md` §1 step 5) «recording each `admit`'s admissions on the violating object's migration event»; (`publish-and-import.md` §3) «is compiled at the next publish after its last is discharged»; (`publish-and-import.md` §6) «which `exceptions(type)` returns until the invariant holds again (ADR-0054)».
- **Severity:** changes-value.
- **Suggested fix:** State whether a migration discharges admissions whose invariant now holds, how an object nothing writes is discharged, and whether a refresh discharges.

#### 15. Where admission-only events fall among the migration events

- **Construct:** event order.
- **Case:** O1 and O3 are changed by a backfill. O2 is changed by nothing but is admitted.
- **The readings:** (a) O1, O2 and O3 get their events in ascending id order. (b) The mappings' events come first (O1, O3) and the admission-only event after them (O2), since admissions are known only once all the mappings are done.
- **Where the text fails:** (`publish-and-import.md` §1 step 5) «in ascending id order»; «an object no mapping changed receiving a `migrated` event carrying only the admission».
- **Severity:** changes-value (event positions and the order of a pull).
- **Suggested fix:** Say that every event of the publish, admission-only ones included, is ordered by object id, or say what the order is.

#### 16. Where a newly tracked member's interval opens when the backfill leaves it unchanged

- **Construct:** tracked members' intervals.
- **Case:** v2 adds the tracked enum `priority` (optional), with backfill `priority := if state == WORKING then Priority.HIGH else null`, and also maps O's removed state. O gets a migration event, and its `priority` stays null.
- **The readings:** (a) A backfill "writes" `priority` on O, so its interval opens at O's migration event. (b) Nothing "gives it a value", so the interval opens at the publish event.
- **Where the text fails:** (`publish-and-import.md` §1 step 5) «at the event that first gives it a value in the publish, the object's migration where a backfill writes it and else the publish event, each marked `.migrated`».
- **Severity:** changes-value (`entered_position`).
- **Suggested fix:** Say whether a backfill that evaluates to absent, or to the existing value, counts as writing the member.

#### 17. A publish's redaction of a newly personal attribute

- **Construct:** the backfill's skip of personal attributes on erased objects, and the publish's `redact`.
- **Case:** E's erasure is recorded. v2 marks `contact` personal and also backfills `contact := if contact is null then "none" else contact`. `contact`'s old values appear in E's events written under v1.
- **The readings:** (a) "Personal" is judged under the new version, so the backfill skips E. (b) It is judged under the old version, so E's migration event carries `"none"` in `contact` and the `redact` then clears the row. Independently, (c) the `redact` reaches only the row, and (d) it also rewrites past events and payloads, as an erasure does. Under (b) with (c), E's history still shows values of a personal attribute.
- **Where the text fails:** (`DESIGN.md` §8) «a publish that newly marks an attribute personal redacts it on objects already erased, as part of the publish, recording on each a `redact` event, source `erased`, caused by the publish event, with an erasure's admission of what it leaves violated; a publish's backfill skips an erased object's personal attributes, as the import does»; (`DESIGN.md` §8) «It reaches every value written under a version that marked its attribute personal»; (`storage-schema.md` §9) «`personal` can be added to an attribute by a later publish, at which point events already archived under the old answer are in the wrong place.».
- **Severity:** changes-value.
- **Suggested fix:** Say which version's `personal` marking the backfill's skip reads, and what the publish's `redact` rewrites besides the row.

#### 18. A removed required attribute, or one made optional, keeps its `NOT NULL`

- **Construct:** `NOT NULL` and the DDL of §10.
- **Case:** v1 `Robot.model` is required, so its column is `NOT NULL`. v2 removes `model`. In a second case, v2 makes `serial` optional.
- **The readings:**
  - (a) A removed attribute gets "nothing", so the column stays `NOT NULL`. Every v2 `add` then fails in the database, as `StorageUnavailable` or `SchemaMismatch`.
  - (b) The builder drops `NOT NULL`, which goes against "nothing".
  - For a made-optional attribute, §10 has no row, so a `clear` or a creation without the value fails in one engine and not in another.
- **Where the text fails:** (`storage-schema.md` §3) «A declared-required attribute takes `NOT NULL`.»; (`storage-schema.md` §10) «**nothing.** A dropped attribute hides; its recorded values stay in history (ADR-0027)»; (`publish-and-import.md` §3) «An attribute that is **removed** needs no mapping and gets no DDL.».
- **Severity:** changes-outcome (it cannot be implemented as written).
- **Suggested fix:** Add §10 rows for a removed required attribute and for an attribute made optional, each dropping `NOT NULL`, by a table rebuild on SQLite.

#### 19. An attribute whose type changed: two columns of one name

- **Construct:** the DDL of §10.
- **Case:** v1 `level: int`; v2 `level: decimal(5,2)`.
- **The readings:** It is "a removed attribute and a new one of the same name". The removed one keeps its column, and the new one is an `ADD COLUMN` of the same name, which is impossible. (a) The builder renames the old column aside and adds an empty one, so every value is absent. (b) The builder alters the column's type in place, so the values are converted and kept, which goes against "a drop and an add".
- **Where the text fails:** (`storage-schema.md` §10) «one whose type changed is a removed attribute and a new one of the same name (ADR-0147)»; «`ADD COLUMN`, nullable.»; «A removed attribute leaving its column in place is deliberate: the column is how history stays readable, and dropping it would make a fold of the log fail on an event the store still holds.».
- **Severity:** changes-value (it cannot be implemented as written).
- **Suggested fix:** Give the DDL for a type change: where the old column goes, and that the new one starts absent.

#### 20. A compiled invariant removed or changed: no DDL drops its constraint

- **Construct:** replacing `CHECK`s, and compilation.
- **Case:** v1 `qty_ok: qty >= 0` is compiled to a `CHECK`. In one case, v2 removes it and a later request sets `qty = -1`. In another, v2 relaxes it to `qty >= -10` and a backfill writes `qty - 5`.
- **The readings:** (a) "Replace each `CHECK` by the new version's" covers only state and enumeration `CHECK`s (they are what is "widened ... to the union of both versions' values"). The old constraint stays, so the request is refused by the database, and the backfill fails mid-publish against the old constraint. (b) It covers every `CHECK`, so the publish installs, though the old constraint is still in force while the mappings write. `storage-schema.md` §10 has no row for a removed or changed invariant.
- **Where the text fails:** (`publish-and-import.md` §1 step 5) «widen each `CHECK` to the union of both versions' values»; «only then replace each `CHECK` by the new version's, apply `NOT NULL` to a new required attribute, by a table rebuild on SQLite, and add the constraints the change brings».
- **Severity:** changes-outcome.
- **Suggested fix:** Add §10 rows that drop a removed or changed invariant's constraint before the mappings, and add the new one after them.

#### 21. When an admitted, or once-mirror, invariant is compiled

- **Construct:** compilation of admitted invariants and a mirror's invariants.
- **Case:** (i) v2 adds a compilable invariant with an `admit` and no object violates it. (ii) A mirror's local invariant has no admissions, and the cutover removes `mirror`. (iii) An admitted invariant's last admission is discharged.
- **The readings:**
  - (i) "An `admit` covers it", so it is not compiled (§3, §10), or no admission stands, so it is compiled (§8).
  - (ii) The cutover compiles it, or nothing does, since §10's only compile row is for "a new invariant".
  - (iii) The next publish compiles it, but no DDL row says so.
- **Where the text fails:** (`publish-and-import.md` §3) «A new invariant an `admit` covers is not compiled to a constraint»; «it stays uncompiled while any admission of it stands, whatever recorded it, and is compiled at the next publish after its last is discharged, and a mirror's invariants are never compiled, since the import may admit them»; (`storage-schema.md` §10) «`ADD CONSTRAINT`, after the report says how many live objects violate it, and only where no `admit` covers it»; (`storage-schema.md` §8) «any invariant that an assertion may `admit`, or of which an admission stands, whatever recorded it».
- **Severity:** changes-value (the report's `Compilation`, and the constraints).
- **Suggested fix:** State one rule for compiling: for an unused `admit`, at a cutover, and after the last discharge, with a DDL row for compiling an existing invariant.

#### 22. A mirror's `unique` and external identifier: constraint or admissible invariant

- **Construct:** a mirror's invariants at import.
- **Case:** two legacy employees share a `serial` that the mirror declares `unique`. In a variant, they share an external identifier.
- **The readings:** (a) Every `unique` is "enforced by a unique index on every backend". The batch fails in the database, and `ADMIT` is impossible. (b) `unique` is sugar for an invariant, and a mirror's invariants get no constraint, so it is an `INVARIANT` class and may be admitted. An external identifier's uniqueness is also enforced by `of_external_id`'s primary key, whatever the class says.
- **Where the text fails:** (`flow-format.md` §4.12) «`unique` takes one of four forms, each the model's sugar for an invariant»; «Each is enforced by a unique index on every backend (`storage-schema.md` §8)»; against (`storage-schema.md` §8) «and every invariant of a mirror, which the import may admit (ADR-0147) | **none, on either**»; (`storage-schema.md` §6) «PRIMARY KEY (source, value)».
- **Severity:** changes-outcome.
- **Suggested fix:** Say whether a mirror's `unique` and external identifier are constraints, which make it a `GUARD`-like class, or admissible invariants.

#### 23. The legacy key's namespace

- **Construct:** matching by the legacy key.
- **Case:** customer legacy key `17` and service-job legacy key `17`, both ported under source `legacy`.
- **The readings:** (a) The raw key is stored. The second insert hits `of_external_id`'s primary key and the batch is refused, or a later batch's job `17` "has arrived" and is matched as a refresh of customer `17`. (b) Keys are qualified by type, and both port. Separately, `of_external_id.attribute` is `NOT NULL`, but a mirror whose identifier is, say, `employee_id external: hr` holds the legacy key in no attribute. Only the `declaration-syntax.md` example declares one under source `"legacy"`.
- **Where the text fails:** (`publish-and-import.md` §4) «keeps its legacy key as an external identifier with source `legacy`»; «An object whose legacy key has arrived is refreshed.»; (`storage-schema.md` §6) «PRIMARY KEY (source, value)»; «attribute  TEXT NOT NULL»; (`flow-format.md` §4.15) «while the import matches objects by the legacy key it keeps under source `legacy`, which is not personal and survives erasure (ADR-0147)»; (`declaration-syntax.md` §2) «attr legacy_key string external "legacy" indexed».
- **Severity:** changes-outcome.
- **Suggested fix:** State whether a legacy key is unique per store or per type, how `lookup('legacy', …)` names the type, and what `attribute` holds for it.

#### 24. `recorded_from`: supplied or computed, what "whole" means, and on a refresh

- **Construct:** an import's `recorded_from`.
- **Case:** the mapping supplies a legacy creation `C = 2024-12-01`. The state intervals run without a gap from `2025-01-01`, there is no silent span, and the mapping's `ImportedObject.recorded_from` is `None`.
- **The readings:**
  - (a) `library-api.md`: the mapping supplies `recorded_from`, and `None` means the port.
  - (b) `publish-and-import.md` computes it from the intervals. If "whole" means "no silent span", it is `C`. If it means "unbroken from the legacy creation", nothing applies, and it is the earliest entry, `2025-01-01`.
  - (c) On a refresh, it is either recomputed or left alone. Suppose the first import began the assignee at the port P1, so `recorded_from` is P1, and a refresh then dates a reassignment. Recomputing moves `recorded_from` before P1, to a time for which no assignee history exists.
  - `DESIGN.md` §5.12 omits the fallback to "the earliest entry".
- **Where the text fails:** (`publish-and-import.md` §4) «It sets each object's `recorded_from`, one value per object read from its intervals: its legacy creation where its intervals are whole, the end of its last silent span where there is one, and the port where its current interval begins at the port, the latest of those that apply over every tracked dimension, and for an object none describes the earliest entry of its intervals»; (`library-api.md` §5) «the earliest time its legacy record vouches for;» and «# None means the port»; (`DESIGN.md` §5.12) «for a ported object the latest of its legacy creation where its intervals are whole, the end of its last silent span and the port where its current interval begins there, over every tracked dimension (§11, ADR-0147)».
- **Severity:** changes-value (and a metric's `gaps`).
- **Suggested fix:** Decide whether it is supplied or computed, and remove the other. Define "whole", and say whether a refresh changes it.

#### 25. Silent spans: "overlap at all" or "cross", and their bounds

- **Construct:** silent spans.
- **Case:** the silent span is `[2025-03-01, 2025-06-01]`. Legacy interval A is `[2025-04-01, 2025-05-01)`, entirely inside it. Legacy interval B is `[2025-01-01, 2025-03-01)`, ending exactly at its start.
- **The readings:** (a) "Overlap at all" refuses A. (b) "Cross" may be read as straddling a boundary, which A does not, so A is accepted. For B, a closed span refuses it and a half-open one accepts it. `silent_spans` is a pair of datetimes with no stated openness.
- **Where the text fails:** (`publish-and-import.md` §4) «no legacy interval may overlap one at all, the import refusing rather than trimming it»; (`library-api.md` §6) «no legacy interval may cross a silent span (ADR-0106)»; (`library-api.md` §5) «silent_spans: Sequence[tuple[datetime, datetime]] = ()».
- **Severity:** changes-outcome (the batch is refused or not).
- **Suggested fix:** Use one verb, and state whether a span's ends are included.

#### 26. A first import whose legacy intervals overlap, or run past the current entry

- **Construct:** the import's refusals against the harness's row 8.
- **Case:** the legacy state interval `OPEN [01-01, 01-12)` is given with `entered` for `WORKING` at `01-10`. In a variant, two legacy intervals overlap each other.
- **The readings:** (a) The import refuses, which keeps the harness's shape rule. (b) The import accepts, since its refusals are listed and this is not among them (silent spans, earlier than the creation time, and a refresh before the latest entry are), and the harness then fails.
- **Where the text fails:** (`publish-and-import.md` §4) «An entry time or a legacy interval earlier than a creation time the mapping supplies is refused (ADR-0096, ADR-0106)»; (`DESIGN.md` §7) «the harness checks them for shape rather than against the log: none overlaps another of the same dimension, and the last ends no later than the object's first recorded interval begins, a gap between them reported and not failed»; (`adversarial-harness.md` §1) «a legacy interval that overlaps another of its dimension, runs past the object's first recorded interval or overlaps a declared silent span at all».
- **Severity:** changes-outcome.
- **Suggested fix:** State that the import refuses overlapping legacy intervals, and ones ending after the current interval's entry.

#### 27. A refresh's intermediate values: legacy or not, how they close the current interval, and what counts as "already held"

- **Construct:** refreshes per dimension.
- **Case:** the first import P1 makes assignee A current from `01-10`. A refresh carries the intermediate `B [01-15, 01-20)` and the current `C` entered `01-20`. In variants, it re-sends `A [01-10, 01-15)`, or the object has been erased since P1 and the re-sent batch carries unredacted intervals and entries.
- **The readings:**
  - (a) B travels in `legacy_intervals` and is stored `legacy = 1` at position 0. It then "runs past the object's first recorded interval", and the harness fails.
  - (b) B is opened and closed on the refresh's event, `legacy = 0`.
  - The re-sent A either closes the current A or is a new interval at the same entry, which overlaps.
  - For the erased object, held values compared after redaction match, and the refresh does nothing. Compared raw, they do not match, their times precede the latest entry, and the batch is refused.
  - Whether `created_at` or `silent_spans` may change on a refresh, and whether they count towards "changes nothing", is unsaid.
- **Where the text fails:** (`publish-and-import.md` §4) «may carry the intermediate values the legacy record shows, which the import opens and closes in order on its event»; «intervals and entries the store already holds, sent again, being left as they are»; (`library-api.md` §5) «# earlier values only; on a refresh,» and «# the values since the last import»; (`storage-schema.md` §6) «legacy intervals use 0»; (`publish-and-import.md` §4) «redacts the legacy entries and intervals it attaches to an erased object as the erasure would have».
- **Severity:** changes-outcome (the erased re-send is refused or not); otherwise changes-value.
- **Suggested fix:** Say where intermediate values travel and how they are marked, that the current interval closes at the first intermediate's entry, and how "already held" is compared (on the redacted form, and on which fields).

#### 28. The kind of a port's changer when the mapping supplies one

- **Construct:** a port's changers.
- **Case:** `entered = {at: L, by: "alice", by_kind: HUMAN}`, and the store holds no actor `alice`. In a variant, the store holds `alice` as a `User` (`human`) and the mapping says `AGENT`.
- **The readings:** (a) `unknown`, because the store holds no actor. (b) `human`, the kind the mapping supplied, which `EnteredAt.by_kind` exists to carry. In the variant, the store's kind and the mapping's disagree.
- **Where the text fails:** (`publish-and-import.md` §4) «a named changer the store holds no actor for is recorded by its identity with the kind `unknown`»; (`library-api.md` §5, `EnteredAt`) «UNKNOWN where the record is silent».
- **Severity:** changes-value.
- **Suggested fix:** Say whether `by_kind` is used, and which kind wins when it disagrees with the store's.

#### 29. The `REFERENCE` class's boundary, and `DROP` on a required reference

- **Construct:** the classes `REFERENCE` and `DROP`, and `EXCLUDE` of a held object.
- **Case:** (i) Job J references customer C. C is held from an earlier batch and is in an `EXCLUDE` class now. (ii) C was excluded by an earlier batch's file and is not held. (iii) J's reference to C is required, and the class is decided `DROP`.
- **The readings:**
  - (i) The reference resolves, so there is no class. Or C is "an excluded object", so it is a `REFERENCE` class.
  - (ii) It is a `REFERENCE` class. Or it is an "unresolvable legacy key" refused at id assignment.
  - (iii) The dry run refuses `DROP`, as it refuses `ADMIT` on a guard. Or the import clears a required reference and the batch fails on `NOT NULL` or a `GUARD` violation.
- **Where the text fails:** (`publish-and-import.md` §4) «a reference into an excluded object, and a field about an object not yet held, are classes of their own, of kind `REFERENCE` and `FIELD`, decided by `DROP`, which clears the optional reference or drops the field, or by `EXCLUDE`»; «an unresolvable legacy key is found here and not by the database (ADR-0018, ADR-0077)»; (`publish-and-import.md` §6) «and one the store already holds is left as last imported, which the dry run reports (ADR-0147)»; «REFERENCE and FIELD classes: clear the optional reference,».
- **Severity:** changes-outcome.
- **Suggested fix:** Define "excluded object" (in this file, or ever, and whether held or not), and say that the dry run refuses `DROP` on a required reference.

#### 30. The `FIELD` class: batch order, against "a thing the store holds no object for", and who splits the fields

- **Construct:** the `FIELD` class.
- **Case:** a job's audit row names employee E's display name. E is a declared mirror and is in the same batch, but nothing depends on E, so no dependency order places E before the job.
- **The readings:**
  - (a) The engine keeps the client's order, E comes after the job, and the field is a `FIELD` class that blocks the import until it is decided.
  - (b) The engine sorts independent rows by some key, E comes first, and the field moves to E's history.
  - Also, a field about an object of a declared type not yet held is a `FIELD` class, while one about "a person or thing the store holds no object for" is silently not ported. Whether E's IP address is the one or the other is unsaid.
  - `library-api.md` has the client send "only this object's fields", which leaves the engine nothing to move or classify.
- **Where the text fails:** (`publish-and-import.md` §4) «moves to that object's history where the store already holds that object when the entry is attached, an object earlier in the same batch included (ADR-0147)»; «a field about a person or thing the store holds no object for, such as the IP address of the employee who acted, is not ported»; «Rows go in dependency order where one exists, so a failure names the first row that could not be placed»; (`library-api.md` §5) «# only this object's fields».
- **Severity:** changes-outcome.
- **Suggested fix:** Fix the order of independent rows, say how a field is marked as describing another object and which side splits it, and separate "not yet held" from "holds no object for".

#### 31. A class's identity ("shape"), and an object at import that matches no class

- **Construct:** the disposition file applied at import.
- **Case:** the dry run classifies `qty >= 0` violations, with a sample at `qty = -1`. A re-extract adds an object at `qty = -40`.
- **The readings:** (a) Same rule and same shape, so it falls under the decided `ADMIT` class. (b) The shape includes the value, so it is a new class, `UNDECIDED`, and the import does not run. And for an object that violates at import under no class at all: (c) the batch is refused, or (d) it is imported with no admission, which the harness's row 2 fails.
- **Where the text fails:** (`publish-and-import.md` §6) «The identity of a class is (type, rule, shape) and not the object set»; «# what is wrong, in one line»; «a class applies to every object in it at import».
- **Severity:** changes-outcome.
- **Suggested fix:** Define how `shape` is computed, and what an import does with a violation no class in its file matches.

#### 32. Two channels for an import's admissions

- **Construct:** `ADMIT`.
- **Case:** class K is decided `ADMIT` with reason R, and `ImportBatch.admitted` lists, for one of K's objects, a different reason, or lists an admission for an object in no class.
- **The readings:** (a) The disposition class governs. (b) `admitted` governs. (c) Both are recorded.
- **Where the text fails:** (`library-api.md` §5) «admitted: Mapping[str, Sequence["Admission"]] = field(default_factory=dict)» with «# disposition: legacy key -> admissions»; against (`publish-and-import.md` §6) «an object newly in an `ADMIT` class is admitted with that class's reason».
- **Severity:** changes-value.
- **Suggested fix:** Remove `ImportBatch.admitted`, or state how it relates to the file.

#### 33. An owned type in a batch

- **Construct:** the import's refusals.
- **Case:** a batch of 500 objects includes 3 of an owned type.
- **The readings:** (a) The whole batch is refused, naming the first such row. (b) Those types are skipped and listed in `refused_types`, and the rest is written.
- **Where the text fails:** (`publish-and-import.md` §4) «is **one transaction**: a refusal refuses the whole batch, naming the first row it could not place»; (`library-api.md` §5) «# owned types, which the import may not write».
- **Severity:** changes-outcome.
- **Suggested fix:** State which, and say what `refused_types` means on a refused batch.

#### 34. When an import checks invariants within a batch

- **Construct:** the built-in assertion's invariant check across rows.
- **Case:** a mirror `Delivery` declares `count(c in checklist_items) >= 1`. Its items follow it in dependency order in the same batch.
- **The readings:** (a) It is checked per row, so the delivery violates when its row is written, and the dry run makes an `INVARIANT` class that needs an `ADMIT`. (b) It is checked once after every row, as a publish's migration is, so it holds and there is no class.
- **Where the text fails:** (`publish-and-import.md` §4) «It writes each object through the **built-in assertion**»; «Rows go in dependency order where one exists»; (`DESIGN.md` §6) «step 7 refuses on any invariant violation the request did not explicitly admit». Nothing says when, in a batch, the invariants are checked.
- **Severity:** changes-outcome.
- **Suggested fix:** State when an import checks invariants: per row, or once per batch over the objects it wrote and those whose invariants read them.

#### 35. A legacy value outside the declared states or members has no class kind

- **Construct:** the disposition file's kinds.
- **Case:** the legacy status `ON_HOLD` is not among the mirror's states, or a member is not in the enumeration.
- **The readings:** (a) It is a `GUARD` class, so the objects can be `EXCLUDE`d and the rest imports. (b) It fits none of the four kinds, so the dry run reports no class and the batch fails on the `CHECK`.
- **Where the text fails:** (`publish-and-import.md` §6) «a structural guard — a required attribute or reference absent»; (`publish-and-import.md` §5) «every value present in production must appear or the import cannot place those rows».
- **Severity:** changes-outcome.
- **Suggested fix:** Give an undeclared value (and a value of the wrong type) a class kind, or say that it refuses the batch.

#### 36. Sequences: "to it" or "past it", and scoped sequences

- **Construct:** the batch's `sequences`.
- **Case:** `unit_serial` is scoped by `model`, and the batch sends `{"unit_serial": 512}`.
- **The readings:** (a) `next_value` becomes 513 ("past it"). (b) The sequence is set to 512 ("to it"), and the first mint collides. For a scoped sequence, the batch names no scope key: one engine raises every scope's row, another refuses.
- **Where the text fails:** (`storage-schema.md` §6) «the import raises next_value past it and never lowers it»; (`library-api.md` §5) «# the store raises the sequence to it, never lowers it (ADR-0103)»; (`storage-schema.md` §6) «PRIMARY KEY (name, scope_key)».
- **Severity:** changes-value.
- **Suggested fix:** Say "past it" in both places, and key `sequences` by name and scope.

#### 37. `created_by_kind` when the mapping supplies a kind but no time

- **Construct:** created times and kinds.
- **Case:** the mapping gives `created_by_kind = HUMAN` and no `created_at`.
- **The readings:** (a) The kind is `unknown`, since "where it supplies none" covers both time and kind. (b) The kind is `HUMAN`, and only the time is undated.
- **Where the text fails:** (`publish-and-import.md` §4) «where it supplies none, `.created_at` is the earliest time the port knows of the object — its first legacy interval, entry or entry time — or the port itself, the kind is `unknown`, and the object is marked undated».
- **Severity:** changes-value.
- **Suggested fix:** Separate the rule for a missing time from the rule for a missing kind.

#### 38. Harness row 4 at a cutover

- **Construct:** the harness's failure conditions.
- **Case:** the mirrors `Delivery D` (`DONE`, final) and `ChecklistItem I` (`ACTIVE`) were ported as the legacy record had them. One publish removes `mirror` from both and maps nothing.
- **The readings:**
  - The publish refuses stranding only where a mapping moves a whole into a final state, so the cutover installs.
  - (a) "A mirror's" means a whole that is a mirror now. D is owned, and the harness fails.
  - (b) It means one written while a mirror. Exempt.
  - Also, the row says "alive" where the publish says "not in a final state". A part in a `closed` non-final state is "not final", but may not be "alive".
- **Where the text fails:** (`adversarial-harness.md` §1) «a part alive under a whole in a terminal state, except one its type declares `survives` that transition, or `survives: true` where a migration moved the whole there, and a mirror's, which the import writes as the legacy record has them (ADR-0147)»; (`publish-and-import.md` §4) «a part left live under a whole the legacy record has finished all port as the record has them (ADR-0147)»; (`publish-and-import.md` §3) «a publish whose objects in the removed state have parts the mapping would strand is refused».
- **Severity:** changes-outcome (the harness's verdict, and whether a cutover should refuse).
- **Suggested fix:** Say whether a cutover refuses, or reports, final wholes with live parts, and word the row's exemption, and "alive", to match.

#### 39. Harness row 1, against check 15's exception

- **Construct:** the harness's failure conditions.
- **Case:** a mirror is cut over to a type that binds a machine and replaces its creation. The state only that creation entered still holds ported objects.
- **The readings:** Check 15 reports, and does not fail, "a state ... that only a creation this binder replaced used to reach", so the publish installs. Row 1 then fails, since no transition of the owned declaration reaches the state. The row's "so a cutover leaves every ported object in a state its owned declaration reaches" describes a guarantee no publish check makes.
- **Where the text fails:** (`adversarial-harness.md` §1) «an object in a state no transition reaches, counting the type's declared transitions and assertions and not the import's built-in assertion, under the installed version, so a cutover leaves every ported object in a state its owned declaration reaches (ADR-0147); a mirror's objects and a retired type's are exempt»; (`declaration-syntax.md` §10, check 15) «a state nothing can reach, **except** one that only a creation this binder replaced used to reach, which is reported instead (§2.1)».
- **Severity:** changes-outcome (a contradiction: a conforming publish produces a store that fails acceptance).
- **Suggested fix:** Add a publish check that refuses objects in a state the new version's transitions do not reach, or exempt that case from row 1.

#### 40. An engine upgrade over a store still at version 0

- **Construct:** `storage-schema.md` §12 step 3.
- **Case:** the store was created by release R1, and nothing has been published. Release R2 changes a standard metric's definition.
- **The readings:** The step records "the next version, with the installed version's source, reference and digest". Version 0 has no reference, and the `CHECK` requires one for every version but 0. (a) The upgrade fails at commit and the store stays at R1. (b) No version is recorded, only the release row. (c) A placeholder reference is written.
- **Where the text fails:** (`storage-schema.md` §12) «it records them as the next version, with the installed version's source, reference and digest and the new built-ins, naming the release as its cause, and supersedes every open `DeclarationChange`, as a publish does»; (`storage-schema.md` §6) «((version = 0) = (source_ref IS NULL) AND (source_ref IS NULL) = (source_digest IS NULL))».
- **Severity:** changes-outcome (it cannot be implemented as written).
- **Suggested fix:** State what an upgrade over version 0 records.

#### 41. What an engine upgrade checks, and what its table migration may write

- **Construct:** the upgrade's check step and its step 2.
- **Case:** R2's built-in `Proposal` gains a required attribute, or loses a state that pending proposals are in. The installed flows' examples use `Proposal`.
- **The readings:**
  - "Checks the installed flows" means (a) the sixty-four checks only; (b) also the examples; (c) also step 3's reads of the built-in types' objects.
  - Under (c), check 23 finds a new required attribute with no backfill, or a removed state with no mapping, and no deployment can supply a built-in migration, so the upgrade is refused for ever.
  - Under (d), step 2's "migration of ... the tables §3 makes of each type" fills or moves those rows itself, with no event.
  - It is also unsaid whether "pass" means "nothing fatal" or "nothing fatal and no decision".
- **Where the text fails:** (`storage-schema.md` §12) «it checks the installed flows against the new one. Where they pass»; «It runs the new release's migration of the store's own tables, the `of_` tables of §6 and the tables §3 makes of each type, which is that release's own work and all this step writes besides its record.».
- **Severity:** changes-outcome.
- **Suggested fix:** List the upgrade's checks, say how a release migrates built-in objects (by recorded events or not), and define "pass".

#### 42. What an engine upgrade records

- **Construct:** the upgrade's version, and its supersessions.
- **Case:** an upgrade that changes the built-ins, with one open change in `SUBMITTED`.
- **The readings:** The new `of_declaration` row has `change_id` and `published_by` unsaid: the operator, or the release (as for version 0). "Naming the release as its cause" has no column. The `supersede` transition's event needs a cause, which for a publish is the publish event on the `DeclarationChange`. The upgrade has no object or event of its own: (a) a cause-less `supersede`; (b) an event on the `Operator`, whose type has no such transition; (c) the cause stored on the version row.
- **Where the text fails:** (`storage-schema.md` §12) «recorded as that operator's act naming the release, as creating the store is (ADR-0126)»; (`storage-schema.md` §6) «for version 0, the engine release that created the store»; «the DeclarationChange it installed (ADR-0085); none for 0»; (`DESIGN.md` §9) «external, only via a publish or an engine upgrade».
- **Severity:** changes-value.
- **Suggested fix:** Name the event an upgrade records, the object it belongs to, and the `of_declaration` fields it fills.

#### 43. "Reported without failing" lists what check 23 calls a blocking decision

- **Construct:** severities.
- **Case:** a new invariant that eleven objects violate.
- **The readings:** The trailing paragraph of §10 lists it among the things reported without failing, while check 23 says it blocks. A builder following the paragraph would treat it as a notice. Every other passage gives it as a decision, so I expect careful builders to agree.
- **Where the text fails:** (`declaration-syntax.md` §10) «Reported without failing:», «how many live objects would violate an invariant this publish adds, and a sample of them» and «the invariants a mapping would leave violated, with each object and whether an `admit` covers it»; against (`declaration-syntax.md` §10, check 23) «Reported as a decision, which blocks the publish until resolved and is not a defect of the declaration».
- **Severity:** wording.
- **Suggested fix:** Move these two items out of "Reported without failing", or note there that they are decisions.

#### 44. The order of mappings, as `flow-format.md` and `declaration-syntax.md` word it

- **Construct:** the order of mappings.
- **Case:** a migration whose `backfill` key is written before `removed_states`. Nothing orders a migration's keys (`flow-format.md` §2 rule 2 orders modules, machines, metrics and types).
- **The readings:** "In the order written" may attach to all three of removed members, removed states and backfills, interleaving them in document order. Then a backfill written first reads the old state. `publish-and-import.md` fixes the kinds' order and keeps "the order written" for backfills only, which I take as settling it.
- **Where the text fails:** (`flow-format.md` §4.16) «renames first, then removed members, removed states and backfills in the order written»; (`declaration-syntax.md` §6.6) «the mappings applied renames first, then removed members, removed states and backfills in the order written»; against (`publish-and-import.md` §1 step 5) «renames first, writing no event of their own, so the other mappings read the new names; then removed enumeration members, a set holding the target already keeping it once; then removed states; then backfills in the order written».
- **Severity:** wording.
- **Suggested fix:** Use `publish-and-import.md`'s wording in the other two.

#### 45. "Classed as `availability`"

- **Construct:** gained and lost transitions.
- **Case:** a transition whose only guard reads `inputs.photo`.
- **The readings:** The phrase can be read as "a metric classed as availability", a class nothing defines, or as "classed as the `availability` query classes it". I took the second (`library-api.md` §5's `AVAILABLE_WITH_INPUT`), and so would a careful builder.
- **Where the text fails:** (`publish-and-import.md` §1 step 3) «a transition whose verdict depends on an input, an evaluator or a metric classed as `availability` lists it, available with input or not evaluated, and so neither gained nor lost».
- **Severity:** wording.
- **Suggested fix:** Write "which the `availability` query lists as available with input, or as not evaluated".

#### 46. "Replaced or removed in the next" reads as something the engine does

- **Construct:** a migration section's lifetime.
- **Case:** service v3 keeps v2's `backfill: ServiceJob: { site: '"HQ"' }` and drops the removed member, which `--previous` would refuse.
- **The readings:** Both builders apply the backfill again, overwriting every job's real `site` with `"HQ"`. Nothing refuses a migration carried forward, so the sentence describes an authoring habit and not a rule. It is not a divergence, but a reader may believe the engine applies a migration once.
- **Where the text fails:** (`flow-format.md` §4.16) «it is written with the version it publishes, and replaced or removed in the next (ADR-0120)»; (`publish-and-import.md` §3) «A `backfill` writes every object of its type».
- **Severity:** wording.
- **Suggested fix:** Say that a backfill left in the next version runs again, or refuse a migration section unchanged from the previous version's.

### Counts by severity

| Severity | Count |
|---|---|
| changes-outcome | 24 |
| changes-value | 18 |
| wording | 4 |
| **Total** | **46** |

*Quotation check:* a Python script extracted every «…» quotation in this file with the file named before it, and asserted that each occurs verbatim in that file: 141 quotations, none failing.

## 4. Metrics and time in a state, as the reader wrote it

**What I read.** These are the sections I was given: `declaration-syntax.md` §6.9, §6.10, §6.11, §8.2 and §8.3; `flow-format.md` §4.9, §5 and the metric rows of §6; `DESIGN.md` §5.5, §5.12, §5.13 and §7; in `library-api.md`, the `metric` operation, `MetricRow`, `MetricPage`, `RaisedFlag` and the bullets about `metric`; `storage-schema.md` §3 and §3.1; and the whole of `metric-scenarios.md`. I left those sections for the following places, either by following a citation or by searching the six files for `ADR-0147`:
- `declaration-syntax.md`: §4.2, for the rules on `backdatable` and occurred time. §8.1, for literal scale and `.created_at`, reached through "as a guard compares it". §10, for checks 23, 26, 56, 58, 60 and 61, because §6.9 cites check 56. Line 40 of the Iterations table, which a search for ADR-0147 found.
- `flow-format.md`: §4.8, the `backdating_limit` row and the refusals paragraph. §4.4, the observations paragraph. §4.14, families, because my brief names abstract bases. §4.16, lines 502 to 525. All of these were found by the search.
- `DESIGN.md`: §5.7, which §6.9 cites for hops. §5.9, line 353. §6, steps 5 to 8 and the attempt row. §8 and §9, lines 569 to 621, by the search but unused. §10, the query and history rows. §11, which §6.9 and §5.12 cite.
- `library-api.md`: the amendment notes, the `ActorKind` and `Remedy` enums, the verdict classes including `CallRefused`, `Attempt` and `EventPage`, and §7 (exceptions).

**What I did not read.** Nothing under `docs/adr/`, `defects.md`, `TODO.md`, `LESSONS.md`, any review or cold-reading file, the case studies, `scripts/`, `.remember/`, or git history. The scenarios cite four documents that are outside the builder set, and I did not open them: `unit-journey.md`, `publish-and-import.md`, `returns-module.md` and `renderers.md`. Where a finding depends on what the journey declares, it says that I inferred it.

**Method.** I computed every time and arithmetic value with Python (`datetime`, `fractions`, `decimal`). I recomputed the scenario values in §3.2, §3.5, §11, §15, §16, §19, §24, §27, §30, §31, §32, §39 and §43 from the current text. Every value agrees. The gaps and the row order do not, as findings 6, 16 and 19 show. **Every quotation in this report was checked by a script.** The script asserts that each `“…”` string occurs character for character in the file named before it.

### What is determined

The text settles the following without ambiguity.

**Spans and their durations**
- A current span runs to `now` while its object is open. On an object in a closed or terminal state, a span that began before the current unbroken closed stretch runs to the stretch's start. A span that began within the stretch has no duration, and it is absent, not zero. Beginning at the stretch's own start counts as within, as the current state's span shows. "Before" and "within" are judged by occurred time. A span that has ended keeps its exit less its entry. `time_in` is real elapsed time.
- A backdated transition may not precede the current interval of anything it changes, so no span has negative length.

**Pieces of a split span**
- Pieces are half-open, and no zero-length piece is made at a bucket boundary.
- A span with no duration is one piece, with none. A zero-length span is one piece of zero duration. Both fall in the bucket where the span began.
- `count()` counts a span once per period it covers. `over last` clips pieces, and a filter reads the clipped piece.

**Aggregates and arithmetic**
- Every aggregate over a group reads only present bodies, `count()` counts rows, and a group whose bodies are all absent is still reported.
- A percentile is nearest-rank: the rank is the smallest integer at least `p × n`, computed exactly (`ceil(Fraction(p)·n)`), with `n` the number of present bodies, and the value is selected by row number. The median is the lower middle.
- `avg` keeps its body's type, and the average of integers is `decimal(19,6)`.
- `int / int` truncates toward zero at the operator. Apart from that, a metric's value is exact and is rounded once, half to even, to its expression's scale.
- Durations are whole seconds.

**Dimensions, windows and guards**
- An absent dimension value forms its own group.
- A set-valued dimension counts a row once per distinct value, taken per member and never crossed.
- Buckets and their string forms are in UTC and use ISO week-years. Python's `isocalendar` puts 2027-01-01 in 2026-W53.
- A window includes both ends, and a row with no timestamp is in no window.
- In a guard, binding a dimension to an unknown value leaves the metric unknown.

**Flags**
- A flag is evaluated on the rounded value, on the groups `by` declares, over the rows the read selects. It reads `version` and `actor_kind` only where `by` names them. An unknown condition is reported as unknown and is not raised.

**Combined metrics**
- `gaps` is a set of ids, and a combined row's gaps are the union of its inputs'.
- Binding a dimension that a broadcast input lacks leaves that input whole.
- At the declared level, a group that only a broadcast input has gives no row.
- A combined metric read with fewer dimensions is computed from re-aggregated inputs, never from combined values.

**Row members and attempt rows**
- `.returns` is fixed when its event is recorded, over legacy and migration-opened spans.
- `.migrated` spans count in the standard metrics.
- A refusal is one attempt row of the requested transition, carrying the first named invariant for a violation.
- `ActorKind`'s declaration order puts `unknown` last (`library-api.md` §3).

**The standard metrics.** The table and the blocks agree on sources, filters and values. Where the table omits `version` and `actor_kind`, the implicit defaults give what the blocks write out.

### Findings

#### 1. Whether an input "broadcasts" is judged by the declaration or by the read
- **Construct:** a combined metric read with a `keep` that drops the dimension a broadcast input lacks.
- **Case:** a made-up Delivery history has three `open` applied, two `complete_sale` applied and one `complete_sale` refused by `filled`. The read is `metric(Delivery.refusal_rate, keep: [transition])`. `refusal_rate` names `clause`, which `transition_counts` lacks.
- **The readings:**
  - (a) `transition_counts` is a broadcast input by declaration. The group `open` exists only in it, so it gives no row. The page holds only `complete_sale = 0.333`.
  - (b) Broadcasting is judged against the dimensions the read keeps. Once both inputs are aggregated to `[transition]`, neither lacks one, so they join over every group. The page holds `complete_sale = 0.333` and `open = 0.000`.

  Scenario §30 does not decide between them: binding `complete_sale` and keeping `actor_kind`, every group exists in `refusals`.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “An input whose `by` lacks a dimension the combination names contributes its one value to every group that differs only in that dimension, and a group that only such a broadcast input has gives no row (ADR-0146).”
  - `declaration-syntax.md` §6.9: “A combined metric has no rows, so it is read by aggregating each input over the rows the read selects to the dimensions it keeps, and combining after”
  - `flow-format.md` §4.9: “an input without one of them contributes its value to every group that differs only in that dimension, binding it leaving that input whole, and a group only such a broadcast input has gives no row”
- **Severity:** changes-outcome. The set of rows differs, so "refusal rate per transition" either omits or shows the transitions never refused.
- **Suggested fix:** State whether "lacks a dimension" and "a group only such an input has" are judged against the combination's declared dimensions or against the dimensions a read keeps or binds.

#### 2. Does a combined metric have `version` and `actor_kind` when its `by` does not name them?
- **Construct:** the implicit dimensions on a combined metric.
- **Case:** `metric(first_pass_yield, keep: [model, month, version])`. `first_pass_yield` (scenario §13) has `group_by: [model, month]`.
- **The readings:**
  - (a) `first_pass_yield` has no `version`, so the `keep` names a dimension the metric lacks. The read is refused, or the extra dimension is ignored.
  - (b) A combined metric passes `version` through, so the read returns rows per (model, month, version).
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “and a combined metric keeps them when its `by` names them, so any metric can be split both ways (ADR-0122)”. "Any metric can be split both ways" is false for `first_pass_yield` under the first half of the sentence.
  - `DESIGN.md` §5.12: “Every metric, declared or standard, has the `version` and `actor_kind` dimensions from its rows unless it declares them, and a combined metric passes them through.”
  - `declaration-syntax.md` §6.11: “a combined one passes them through from the metrics it combines”
  - `flow-format.md` §4.9: “so a combined metric has exactly the dimensions it names, and `version` and `actor_kind` only when it names them (ADR-0122)”
- **Severity:** changes-outcome.
- **Suggested fix:** Say in all three documents that a combined metric has `version` and `actor_kind` only when its `by` names them, and say what a read keeping or binding them does. Then reword "any metric can be split both ways" and "passes them through".

#### 3. Windowing a combined metric: nested inputs, and a reader's `over`
- **Construct:** `over last` or `over` on a combined metric.
- **Case:**
  - (i) C2 combines `x = C1` and `y = M`. C1 is a combined metric, every input of C1 declares `window on`, and M declares one too. A guard reads `metric(C2, over last 30 days)`.
  - (ii) A reader calls `metric(C, over: 30 days)` on a combined metric C whose every input declares `window on`.
- **The readings:**
  - (i)(a) The guard publishes, per §6.9. (i)(b) It is a publish error, per check 56 and `flow-format.md`, neither of which has the nested case.
  - (ii)(a) The read is refused, because C "declares no `window on` and does not split". (ii)(b) The read is allowed, "as a guard's is at publish", since a guard's window on C publishes.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “and on a combined metric whose every input declares one, splits, or is a combined metric whose own inputs do (ADR-0147)”
  - `declaration-syntax.md` §10: “other than one that splits its spans or a combined one whose every input declares one or splits (ADR-0128, ADR-0142)”
  - `flow-format.md` §4.9: “windows only a metric with a `time_dimension` or a `split_by`, or a combined one whose every input has one”
  - `declaration-syntax.md` §6.9: “A reader's `over` on a metric that declares no `window on` and does not split is refused as invalid, as a guard's is at publish (ADR-0147).”
  - `DESIGN.md` §5.12: “a filter on a combined metric and a window on a metric that declares none being refused (ADR-0147)”
- **Severity:** changes-outcome. A publish is accepted or refused, and a read is answered or refused.
- **Suggested fix:** Put the nested case in check 56 and `flow-format.md` §4.9, or remove it from §6.9. Then state the reader's rule for a combined metric in the same terms as the guard's.

#### 4. Check 56 lacks two rules that §6.9 attributes to it
- **Construct:** the publish checks for combined metrics and for percentiles.
- **Case:**
  - (i) `combine a = A, b = B; by x, y`, where A is `by x` and B is `by y`, so no input has both.
  - (ii) `percentile(p_expr, i.duration)`, where `p_expr` is not a literal, such as `0.5 + 0.3` or an attribute.
- **The readings:**
  - (a) Both are publish errors, as §6.9 and `flow-format.md` say.
  - (b) A builder implementing check 56 from its row in §10 accepts both. Case (i) then publishes a metric that can never give a row, because every group is one that only a broadcast input has. Case (ii) has no `p` to range-check at publish.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “Binding a dimension a broadcast input lacks leaves that input whole, and one input has every dimension the combination names (check 56, ADR-0147).”
  - `declaration-syntax.md` §6.9: “`p` being a literal (ADR-0147)”
  - Check 56 in `declaration-syntax.md` §10 holds only the range rule: “a `percentile` whose `p` is outside `(0, 1]`”
  - `flow-format.md` §4.9 states the first rule: “one input MUST have every dimension the combination names (step 4, `check 56`; ADR-0147)”
- **Severity:** changes-outcome. A publish is accepted or refused.
- **Suggested fix:** Add both rules to check 56's row, and say whether a constant expression counts as a literal.

#### 5. `metric()` has no way to refuse
- **Construct:** the refusals of `metric()`.
- **Case:** `metric(first_pass_yield, filter: "r.unit.model == M-ARM")`, or `metric(Delivery.refusals, over: 30 days)`. `refusals` declares no `window on` and does not split.
- **The readings:** The text says both are refused, but there is no channel to refuse through.
  - (a) Raise an exception. The list of things that raise is closed, and this is not on it.
  - (b) Return an empty `MetricPage`. That cannot be told apart from "no data".
  - (c) Ignore the argument. That returns unfiltered or unwindowed rows.

  The text also says nothing of a `keep` or `bind` naming a dimension the metric lacks, of a reader's filter reading a personal attribute, or of an unknown metric name.
- **Where the text fails:**
  - `library-api.md` (`metric` bullets): “A `filter` on a combined metric is refused, since it has no rows to narrow”
  - `library-api.md` §7: “Everything in §4 is a value. These are the five things that raise, and the list is closed so that a second binding cannot differ on it.”
  - `library-api.md` (amendments): “`MetricPage` loses `complete` and `refused`”. `MetricPage` now has only `rows`, `cursor`, `flags` and `history_complete`, while `EventPage` kept a `refused` value.
- **Severity:** changes-outcome. This cannot be implemented as written.
- **Suggested fix:** Give `MetricPage` a refusal value, as `EventPage.refused` is, or name the verdict it returns. List every reader argument that is refused, with its reason.

#### 6. `recorded_from` "over every tracked dimension" contradicts the scenarios' gaps
- **Construct:** `recorded_from` and `gaps` on ported objects.
- **Case:** Scenario §2 imports D01 with `created` and a state `entered` of 2026-08-20, and with `customer: C-ACME` but no entry time for `customer`. If `Delivery.customer` is a singular stored reference, and so tracked, its current interval opens at the port, 2026-09-01. That follows §3.1's own rule for an unstated entry, applied to a member. I inferred that it is a reference: I did not read `unit-journey.md`. `recorded_from` is then the port, not "20 Aug". D01's PREPARATION span (20 Aug to 4 Sep) began before it, so D01 joins the gaps in two places: §3.2's PREPARATION row, which states only `D08`, and §3.5's (PREPARATION, C-ACME, M-ARM) row, which states none. The same holds for other ported objects:
  - L01 to L04 and L06, through `model` and, for L02 and L06, `sold_to`. Their legacy `INTAKE` intervals precede the port, so each joins §16's gaps (for example “| Armtek | 2025-11 | 42d | |”), and §16 states only L05.
  - SJ1, through `kind`, `robot` and `customer`, which the YAML gives no entry, unlike `engineer`. SJ1 joins the gaps of §31's `time_to_first_assignment` row “| 2026-08 | 2d | |” and of `handoffs_by_object` and `returns_by_object`.
- **The readings:**
  - (a) Literally, these objects' `recorded_from` is the port, and the gaps above are added.
  - (b) A member the mapping gave no entry time counts as held since the object's creation, or does not count. This gives the scenario's stated `recorded_from` ("20 Aug, its legacy creation"; "28 Aug" for D08) and gaps.
- **Where the text fails:**
  - `DESIGN.md` §5.12: “Each object records the time from which its record is continuous: its creation, or for a ported object the latest of its legacy creation where its intervals are whole, the end of its last silent span and the port where its current interval begins there, over every tracked dimension (§11, ADR-0147).”
  - `DESIGN.md` §11: “It may supply each object's legacy creation time and creator's kind, and for its state and each tracked member when it took its current value and who changed it.”
  - `metric-scenarios.md` §3.1: “at the port where it says nothing”
  - `metric-scenarios.md` §3.1: “20 Aug, its legacy creation”
  - `metric-scenarios.md` §3.2: “| PREPARATION | 3d 12h | D08 |”
- **Severity:** changes-outcome. The gaps differ.
- **Suggested fix:** State where a ported object's member interval begins when the mapping gives no entry time, and whether such a member moves `recorded_from`. Then correct the scenario's stated `recorded_from` values and gaps.

#### 7. Counting hops: the definition, the format and the scenario disagree
- **Construct:** the two-hop limit on a dimension path.
- **Case:** a path whose final attribute is a reference, `i.object.units.model.maker`. I made up `maker` as a reference to a hypothetical `Manufacturer` type.
- **The readings:**
  - (a) Under §6.9's definition, `.object` is not a hop, `units` and `model` are followed (2), and the final attribute `maker` is "none". The path has 2 hops and publishes.
  - (b) Under the scenario's counting, `units.model` is already "two hops", because a final reference counts. The path has 3 hops and fails check 56.
  - (c) Under `flow-format.md`'s "from the row", `.object` counts too, giving 3 or more hops, and it is refused. Note that `i.object.customer.account_manager` has 1 hop under (a), 2 under (b) and 3 under (c).
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “A hop is a reference followed, the final attribute read being none, and a path's hops are counted from the row's object, so `.object` on a dataset row, and `.subject` on an observation's or a label's, is not a hop (ADR-0147)”
  - `metric-scenarios.md` §3.5: “a path through the set `units` and one hop more, two hops from the row's object (§6.1)”. Under the definition, `units.model` is one hop.
  - `flow-format.md` §4.9: “a dimension reaches at most two hops from the row”
- **Severity:** changes-outcome. A publish is accepted or refused.
- **Suggested fix:** Use the §6.9 definition in `flow-format.md` ("from the row's object"), and correct the scenario's "two hops" to one.

#### 8. Which dimensions are marked as naming an actor
- **Construct:** the actor marking that `declaration(type)` reports.
- **Case:**
  - (i) `login = t.object.engineer.login`, a path ending in `User.login`, which is marked `actor`.
  - (ii) `who = u.id` over `from u in User`, which is the object's own id and not its actor identity.
  - (iii) A combined metric's `assignee` dimension, which has no expression of its own.
- **The readings:**
  - (i) It is marked under `declaration-syntax.md` §6.9, and probably under `DESIGN.md` ("an actor's id"). It is not marked under `flow-format.md` §4.9, whose list has only two cases.
  - (ii) It is not marked under any list, though it names a person. A builder might mark it anyway.
  - (iii) The text does not say whether the dimension inherits its inputs' marks.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “A dimension whose value names an actor — an object of a type that marks an `actor` identity, an `.actor_id`, or a path ending in an attribute marked `actor` (ADR-0147) — is **marked**”
  - `flow-format.md` §4.9: “**A dimension whose value names an actor is marked**, an object of a type that marks an `actor_kind` identity or a row's `.actor_id`”
  - `DESIGN.md` §5.12: “The declaration marks every dimension whose value names an actor — an object of a type that marks an `actor` identity, or an actor's id”
- **Severity:** changes-outcome. `declaration(type)` reports a different set of marks, and an upper layer's door reads that set.
- **Suggested fix:** Bring `flow-format.md` in line with the three cases. Say whether `.id` of an actor type is marked, and how a combined metric's dimension gets its mark.

#### 9. An abstract base may and may not declare metrics
- **Construct:** metrics over a family.
- **Case:** an abstract base `Payment` declares a metric over its family's transitions.
- **The readings:**
  - (a) It is legal, and its datasets range over the family (`declaration-syntax.md` §6.11).
  - (b) It is a schema error (`flow-format.md` §4.14). Since a metric "is declared under the type whose rows it reads", the format cannot express a family-wide metric at all.
- **Where the text fails:**
  - `declaration-syntax.md` §6.11: “an abstract base having none of its own, though it may declare metrics over its family's rows, its datasets ranging over the family (ADR-0147)”
  - `flow-format.md` §4.14: “and nothing of a lifecycle: no states, transitions, `state_machine`, conditions, observations or metrics (step 2, `schema`)”
  - `flow-format.md` §4.9: “It is declared under the type whose rows it reads”

  Neither text says how a family metric's `state` dimension orders states drawn from several machines (see finding 18).
- **Severity:** changes-outcome. A publish is accepted or refused.
- **Suggested fix:** Allow `metrics` on an abstract base in `flow-format.md` §4.14, or drop the clause from §6.11.

#### 10. An absent body inside an aggregate that is not a metric group's
- **Construct:** `sum`, `min` and `max` over a collection whose element body is absent. This happens with `i.duration` on a closed object's current span, inside a metric's value or inside a guard or derivation.
- **Case:** at `now` 2027-01-25 (scenario §29), take `from o in ServiceJob`, `value avg(sum(i in o.intervals: i.duration))`. J1 and J2 are DONE, so their current DONE spans have no duration. The totals are J1 3d, J2 8d, J3 11d 14h, J4 10d 14h and SJ1 154d.
- **The readings:**
  - (a) Absent bodies are skipped, as SQL does. The average is over 5 jobs: **37d 10h 24m**.
  - (b) An absent operand makes `sum` unknown, so the job's row has an absent body and is left out. The average is over 3 jobs: **58d 17h 20m**.

  In a guard, take a return now RESOLVED (closed) for 1 day after an earlier 8-day RESOLVED span. `require old: sum(i in this.intervals where i.state == RESOLVED: i.duration) >= 7 days` is satisfied under (a) and unsatisfied, with the verdict unknown, under (b). `time_in(RESOLVED)` is 9 days.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “Each aggregate reads the rows of its group where its own body is present, as SQL leaves out a null, and `count()` counts rows (ADR-0146).” This speaks of a group's rows only.
  - `declaration-syntax.md` §8.2: “Any operator with an `unknown` operand yields `unknown`, with three exceptions.”
  - `declaration-syntax.md` §8.2 speaks only of the filter: “Inside a guard, an invariant or a derivation, an aggregate is not a place of its own: an element whose filter is `unknown` makes `all`, `any` and `none` unknown unless another element decides them, and `count`, `sum`, `min` and `max` unknown”
  - `declaration-syntax.md` §8.3: “whose elements have the members those datasets have in a metric (§6.9)”. That brings in the absent `.duration`.
- **Severity:** changes-outcome. A guard decides differently, and a value differs.
- **Suggested fix:** State what an element whose body is absent does in an aggregate over a collection, both inside a metric's value and inside a guard, invariant or derivation.

#### 11. The instant: what "held to the second" does, and which `now` a guard's metric window uses
- **Construct:** the clock's resolution and the instant `now` stands for.
- **Case:** a request reaches the engine at 2026-10-04T09:59:59.6Z. A guard reads `time_in(RESOLVED) >= 7 days`, and RESOLVED was entered at 2026-09-27T10:00:00Z.
- **The readings:**
  - (a) The clock is truncated, so `now` is 09:59:59 and the elapsed time is 6d 23h 59m 59s: unsatisfied.
  - (b) The clock is rounded, so `now` is 10:00:00 and the elapsed time is 7d: satisfied.

  The same choice moves an event at Sunday 2026-10-04T23:59:59.7Z from 2026-W40 into W41. It also decides what happens to a caller's `occurred_at` or an `over` `timedelta` with fractional seconds: truncated, rounded or refused.

  A second question: in a guard, `over last` is anchored at "`now`". The metric is consulted before the transaction, and the request's `now` is its recorded time. The text does not say whether the window ends at the consult's as-of time or at the request's fixed `now`, nor whether a retry re-fixes `now`.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “a duration's resolution is the second, a timestamp being held to the second, the engine reading its clock to the second, so a duration is whole seconds and exact (ADR-0146, ADR-0147)”
  - `storage-schema.md` §3.1: “held to the second, the engine reading its clock to the second, so a duration is whole seconds (ADR-0147)”
  - `flow-format.md` §5: “`now` is the time the request is recorded”
  - `DESIGN.md` §6: “and the `now` the request fixed”
  - `declaration-syntax.md` §6.9: “It is consulted before the transaction, as an evaluator is”
- **Severity:** changes-outcome. A guard decides differently, and a row lands in a different bucket.
- **Suggested fix:** Say "truncated to the second" (or rounded), for the clock and for supplied times alike. Say when a request fixes `now` relative to its metric consultations and its retries.

#### 12. A flag's literal finer than the value's scale
- **Construct:** a flag compared on the rounded value, with a decimal literal.
- **Case:** a pass rate of scale 3 is 9/10, so 0.900, with `flag low when value < 0.9005`.
- **The readings:** The literal "takes the scale of whatever it is compared ... with", which is 3, but the text gives no rounding for it.
  - (a) Exact comparison: 0.900 < 0.9005, so the flag is raised.
  - (b) The literal is rounded half to even to 0.900, so 0.900 < 0.900 is false and the flag is not raised.
  - (c) The literal is truncated to 0.900, which is also not raised.
  - (d) It is a publish error.

  The same literal rule, read alone, gives `count() * 1.000` the scale of an `int`. Only §6.9's example ("three places for that pass rate") says that the literal keeps its written 3. That part is wording.
- **Where the text fails:**
  - `declaration-syntax.md` §8.1: “One with a point is a `decimal`, and it takes the scale of whatever it is compared or combined with”
  - `declaration-syntax.md` §6.9: “It is evaluated on the rounded value, as a guard compares it”
  - `declaration-syntax.md` §6.9: “which is why the pass rate above multiplies by `1.000`”
- **Severity:** changes-outcome. The flag set differs, and so would a guard's decision.
- **Suggested fix:** State how a literal with more places than its scale converts (refused, or rounded half to even), and that a literal combined with an `int` keeps its written scale.

#### 13. A zero-length piece under a window, and a no-duration piece's bounds
- **Construct:** split pieces with `over last`.
- **Case:** A unit enters INTAKE and leaves it in the same second, 2026-10-03T12:00:00Z. Two requests can land in one second, or a backdated transition can sit exactly at the current interval's start. The metric is `from i in Robot.intervals split by day where i.state == Robot.INTAKE`, `value count()`, read `over 7 days` at `now` 2026-10-04T00:00Z.
- **The readings:**
  - (a) The half-open piece [12:00:00, 12:00:00) has "no part" in the window, so it is left out: count 0, and no row if the unit was the only one.
  - (b) It is kept, like a piece with no duration whose start lies in the window: count 1.

  Without a window, both give 1. The text also never gives `.piece_start` or `.piece_end` for a piece with no duration, so a filter reading `i.piece_end` can differ.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “each piece half-open, from its start up to and not including its end, so no piece of no length is made at a boundary (ADR-0146)”
  - `declaration-syntax.md` §6.9: “and a span of zero length is one piece of zero duration, in the bucket it began (ADR-0147)”
  - `declaration-syntax.md` §6.9: “leaving out one with no part in it and keeping one with no duration whose start lies in the window (ADR-0146)”
- **Severity:** changes-outcome. The row set and `count()` differ.
- **Suggested fix:** Say whether a zero-length piece is kept by a window whose closed interval contains its start. Give a no-duration piece's `.piece_start` and `.piece_end`.

#### 14. `.held` on a backdated span, and at an instant two intervals share
- **Construct:** `.held(<member>)` on a span whose entry, as an occurred time, precedes its event's recorded time.
- **Case:** a ServiceJob (§6.10) is opened 2026-10-01T09:00Z with engineer A. `reassign` to B at 2026-10-02T09:00Z. `start` (`backdatable within 2 days`) is recorded at 2026-10-02T12:00Z with `occurred_at` 2026-10-02T08:00Z. That is legal: it is within the bound, it is after OPEN's start, and it changes no engineer.
- **The readings:** `time_working` and `time_in_state_by_holder` attribute the WORKING span differently.
  - (a) The value after `start`'s writes, read at its position, is **B**.
  - (b) The span's entry precedes the event that opened it, so the interval index's value at 08:00 applies, which is **A**.

  For a ported span whose entry falls exactly where one legacy member interval ends and the next begins, the text also gives no half-openness for the interval index (it states it only for pieces). Either value can then be read.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “`.held(<member>)` is, on a span, the value a tracked member held when the span began, after the writes of the transition that began it, read at that transition's position”
  - `declaration-syntax.md` §6.9: “on a span whose entry precedes the event that opened it, a port's or a refresh's, it is the value the interval index holds at the entry, legacy intervals included, absent where none covers it (ADR-0147)”. The text does not say whether "a port's or a refresh's" is the whole list or examples, nor whether "precedes the event" means its occurred or its recorded time.
  - `declaration-syntax.md` §4.2: “The event keeps both times and the interval index uses the occurred one”
- **Severity:** changes-outcome. The groups differ.
- **Suggested fix:** State that the second rule applies only to port and refresh spans, or to every span whose occurred entry precedes its event's recorded time. State that index intervals are half-open.

#### 15. Which version's categories decide `.completes` and the closed stretch
- **Construct:** closed or terminal status of a past state after a publish changes the state's category or removes it.
- **Case:** in v1, DELIVERED is `closed`, and D completes PREPARATION → DELIVERED under v1. v2 republishes DELIVERED as `live`. Nothing I read forbids that; this is my inference.
- **The readings:**
  - (a) `.completes` is judged under the recording version, so `throughput` counts D's completion.
  - (b) It is judged under the version read now, so the completion is live → live and is not counted.

  The same choice decides whether a past DELIVERED span belongs to the "unbroken stretch" of closed states. That moves where a member's current span stops on an object now in a terminal state. For a removed state, the current version has no category to read at all. Only `.returns` is said to read a span "under the version that recorded it".
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “`.completes` marks every transition from an open state into a closed or terminal one”
  - `declaration-syntax.md` §6.9: “On an object now in a `closed` or terminal state, a span still current that began before the unbroken stretch of such states the object is now in runs to the stretch's start”
  - `declaration-syntax.md` §6.9: “a span keeps the state it recorded, read under the version that recorded it, a removed state included (ADR-0147)”
  - `DESIGN.md` §5.9: “a metric is computed on read under the engine's meaning over all history”
- **Severity:** changes-outcome. The rows counted differ, and so does a span's duration.
- **Suggested fix:** State, for `.completes` and for the closed stretch, which version's categories apply, as §6.9 does for `.returns`.

#### 16. A refused call's attempt row: its clause and its verdict
- **Construct:** the attempt row of a request refused through a `call`.
- **Case:** scenario §11. R33's receipt is refused in `S1.receive_unit`'s call to `Robot.receive`, and R32's first receipt is refused by `receive_unit`'s own bound.
- **The readings:**
  - (a) The call's clause is written `<Type>.occurred_within`. `Shipment.refusals` then has two rows: `occurred_within = 1` and `Robot.occurred_within = 1`.
  - (b) The clause is unqualified, and there is one row, `occurred_within = 2`, as the scenario states. The same split hits §35's human row.

  The row's `.verdict` is "the verdict kind", which could be `call refused` or the inner `unsatisfied`. The scenario prints `unsatisfied` for D06's refused call, which matters to any metric grouped by `a.verdict`.
- **Where the text fails:**
  - `flow-format.md` §4.8: “A failure in a call or creation refuses the request as `call refused`, naming the step and its target with the clause `<Type>.occurred_within`”
  - `declaration-syntax.md` §6.9: “a refusal is one row of the requested transition, carrying the clause its verdict names”
  - `metric-scenarios.md` §11: “| receive_unit | occurred_within | self_serviceable | 2 | |”
  - `metric-scenarios.md` §12: “so the two causes of `occurred_within` above count together”
  - `library-api.md` (`Attempt`): “verdict: str                          # the verdict kind, or the fault's name”
  - `metric-scenarios.md` §38: “| complete_sale | human | unsatisfied | mfr_serial | dependent | R20 |”
- **Severity:** changes-outcome. The rows differ.
- **Suggested fix:** State the exact string a call's clause carries in an attempt row, and whether `.verdict` is the outer or the inner kind. Then align `flow-format.md` §4.8 and the scenario.

#### 17. Flags and `history_complete` across pages
- **Construct:** how a paged read carries flags and completeness.
- **Case:** a read keeps fewer dimensions than `by` (or adds the implicit `version`) and spans two pages. The rows (Ben, 2026-09, v1) and (Ben, 2026-09, v2) of `time_working` fall on different pages, and both draw on the flag group (Ben, 2026-09). Only page 1 has a row with gaps.
- **The readings:**
  - (a) Each page repeats the flag, and `history_complete` is per page: page 2 says `true`.
  - (b) A flag is carried once per read, and `history_complete` covers the whole read: `false` on every page.

  Page size is not a parameter of `metric()`, so even reading (a) gives different pages, and so different flags per page, between builders.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “and a read returns each flag raised with the group it was raised on, each page those of the groups its rows draw on”
  - `declaration-syntax.md` §6.9: “and the result says whether any row has one”
  - `DESIGN.md` §5.12: “The page says whether any row has one.”
  - `library-api.md` (`MetricPage`): “history_complete: bool                # no row has gaps: the record covers every span it”
- **Severity:** changes-outcome. The flags and the completeness on a page differ.
- **Suggested fix:** State the page size or make it a parameter. Say whether a flag repeats on every page that draws on its group, and whether `history_complete` is per page or per read.

#### 18. `MetricPage` order is unstated for several dimension types, for `flags` and for `gaps`
- **Construct:** the ordering of a page.
- **Case:** `metric(ServiceJob.acted_by_non_assignee.engineer, keep: [transition])` (§31).
- **The readings:** `.transition` is "a transition's name written as a string".
  - (a) Code point order gives `finish, reassign, start`.
  - (b) Declaration order, as for states, gives `reassign, start, finish`, which is how the scenario prints it.

  Equally unstated are:
  - clause names and verdict kinds;
  - `.reason`, which mixes members of several enums with the string `erasure`;
  - a removed state, which has no current declaration order;
  - states of several machines in a family metric;
  - `int`, `decimal`, `bool`, `date` and raw `timestamp` dimensions, for example `version` 6, 9, 10 by value or by text;
  - the order of `MetricPage.flags`;
  - the order of a row's `gaps`, a `Sequence[str]`.
- **Where the text fails:**
  - `library-api.md` (`metric` bullets): “a `MetricPage` ascends by the dimensions kept, in the order `keep` names them, or by the declared dimensions in their declared order when `keep` is none, a time bucket in time order and an absent value last, the implicit `version` and `actor_kind` after the declared, states, enumeration members and kinds of actor in declaration order, strings by code point and objects by id (ADR-0127, ADR-0147)”
  - `declaration-syntax.md` §6.9: “`.transition` is compared with a transition's name written as a string”
  - `declaration-syntax.md` §6.9: “or, on an erasure's events, the fixed value `erasure`”
  - `library-api.md` (`MetricRow`): “gaps: Sequence[str] = ()”
- **Severity:** changes-outcome. The order differs.
- **Suggested fix:** Extend the ordering bullet to transition and clause names, remedies and verdicts, mixed-enum `reason`, removed states, scalar types, `flags` and `gaps`.

#### 19. The scenarios' row order is not the page order
- **Construct:** the order of the scenarios' result tables.
- **Case:**
  - §16 keeps `[supplier, month]`, yet interleaves the suppliers: “| Roverline | 2025-09 | 37d | |” comes before “| Armtek | 2025-11 | 42d | |”. Under the code-point rule, all Armtek rows come first.
  - §10 keeps `[model, week]` and interleaves M-ROVER, M-ARM, M-ROVER.
  - §4 keeps `[transition, clause, remedy, actor_kind, week]` and orders by week within `complete_sale`.
- **The readings:** (a) The tables show the result's order. (b) The tables are sets. §7.3 says the script compares them as sets, but that was written before ADR-0127 ordered pages.
- **Where the text fails:**
  - `metric-scenarios.md` §7: “The script compares unordered reads as sets”
- **Severity:** wording.
- **Suggested fix:** Reorder the tables to the page order, or state at the top that their row order is not the page's.

#### 20. Two restatements of the duration rule drop its second half
- **Construct:** the summaries of the closed-stretch rule in other documents.
- **Case:** D01's current DELIVERED span began at the stretch's start. Read alone, the DESIGN summary has it "stop at the start", which a builder would make 0, not absent. §3.2's DELIVERED median then becomes 0 over [0, 0, 0, 6d] instead of 6d.
- **The readings:** (a) It is absent, per §6.9, and the median is 6d. (b) It is clamped to 0, per a reading of DESIGN alone, and the median is 0.
- **Where the text fails:**
  - `DESIGN.md` §5.12: “a span still current on an object in a `closed` or terminal state stops at the start of the unbroken stretch of such states it is in”
  - `flow-format.md` §4.9: “Its pieces cover each span's duration as `declaration-syntax.md` §6.9 says, a current span cut at `now`”
  - `declaration-syntax.md` §6.9 settles the case: “and one that began within it, like the span of the current state, has no duration”
- **Severity:** wording. §6.9 decides the case.
- **Suggested fix:** Add "and one that began within it has none" to DESIGN §5.12. Say "a current span cut at `now`, or as §6.9 says on a closed object" in `flow-format.md`.

#### 21. "Nothing accrues on a finished object" is false for two kinds of span
- **Construct:** the explanatory claims about the closed-stretch rule.
- **Case:**
  - (i) Engineer A is named from day 0, and the job closes (DONE, not terminal) on day 5. An `act` reassigns it to B on day 8. A's span has now ended, so it keeps 8 days: three days accrue after the job finished, retroactively. Before the reassignment it read 5.
  - (ii) The job closes on day 5 and reopens on day 20, and A is still named. A's span then counts the 15 closed days.
- **The readings:** The rule is determined. The prose summarising it misleads.
- **Where the text fails:**
  - `declaration-syntax.md` §6.9: “So nothing accrues on a finished object”
  - `DESIGN.md` §5.12: “so an engineer still named on a closed job accrues no time”
  - `declaration-syntax.md` §6.9: “a span that has ended keeps its exit less its entry”
- **Severity:** wording.
- **Suggested fix:** Narrow the claim to a span still current, and note the two cases above.

#### 22. Scenario §39's "165 days in preparation"
- **Construct:** a stated duration.
- **Case:** D06 was in PREPARATION from 2026-09-25T00:00Z to `mark_ready` at 2027-02-22T04:05Z, which is 150d 4h 5m. 165d 10h 10m is its opening-to-completion cycle time.
- **The readings:** —
- **Where the text fails:**
  - `metric-scenarios.md` §39: “so D06, 165 days in preparation, and D20 are not counted”
- **Severity:** wording.
- **Suggested fix:** Say "165 days from opening to completion".

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 18 | 1–18 |
| changes-value | 0 | — (findings 10, 12, 13, 15 also change values; each is counted once, at its highest severity) |
| wording | 4 | 19–22 |
| **Total** | **22** | |

## 5. Disposition

Decided by the author, 2026-10-04 ("go ahead with 1 and 2, and do 3 too"), and recorded as ADR-0148: the contradictions and regressions are fixed, each in its rule's one normative home, and every other finding is an open question, answered when a builder reaches it.

**Fixed** (ADR-0148 decision 2), each fix naming the findings it settles, by report and number (A for §1, B for §2, C for §3, D for §4):

| Fix | Findings | What is stated |
|---|---|---|
| F1 | C1, C3 | A publish evaluates every invariant over every object as the mappings leave it, an invariant it did not change needing no new evaluation over an object nothing it wrote affects; a type the change touches is one whose declaration it alters, or whose rules read such a declaration |
| F2 | C2 | A migration event goes to each object whose row the mappings change, and an admission-only event to one only an `admit` reaches |
| F3 | B2 | One list of what invalidates a pending proposal, in `DESIGN.md` §9, which §5.9 cites |
| F4 | B1 | An approval's events: the proposal's `approve` first, then the target's and what it causes; `Satisfied` names the target |
| F5 | A1 | A function without `fresh` does not read the stamp: only an answer obtained for the request is used |
| F6 | A2 | An evaluator that does not answer, or answers too old, refuses with `temporal`, an exception to the declared remedy that an example's expectation follows |
| F7 | A13, A15, A29 | An attribute correction marks the rows whose value or `.held` it changes, relabels from the current interval's start, and for an untracked attribute changes only its value and its event |
| F8 | A19 | A correction supplies `subject`, the corrected observation's |
| F9 | A22 | An expectation's `values` may name an observation's members, `occurred_at`, `recorded_at`, `recorded_by_kind`, `subject` and `corrects` |
| F10 | A24, B8 | Only a kind with a personal field has a generated `forget`, which the subject's erasure takes; a label's note is redacted on the subject's erasure event |
| F11 | A26 | Scalars stay strings but for booleans and numbers: timestamps and dates are not resolved by the loader |
| F12 | A28 | A creation never fails the interval check, having no current interval |
| F13 | B11 | A set may be personal, and an erasure empties it |
| F14 | B26 | `migrate`, `import` and `redact` are reserved transition names |
| F15 | B37 | An abstract base may be a mirror, its family ported together |
| F16 | B38 | An examples file resolves its names against every module the check is given |
| F17 | B39 | The built-in `DeclarationChange` guards' remedy is `unreachable_from_here`, the cure being another transition |
| F18 | B40 | The library's comment on a refusal's objects cites `DESIGN.md` §5.5 |
| F19 | C18, C19, C20 | The DDL for a removed or newly optional required attribute, a changed type, and a removed or changed compiled invariant |
| F20 | C39 | A publish leaving an object in a state no transition or assertion of its new version reaches is fatal |
| F21 | C40 | An upgrade over a store still at version 0 records version 1 with no source, naming the release |
| F22 | C43, C44 | The model's §10 lists and the order of mappings follow `publish-and-import.md` |
| F23 | D4 | Check 56's row states both rules §6.9 attributes to it |
| F24 | D5 | `MetricPage` carries a refusal, as `EventPage` does |
| F25 | D6 | `recorded_from` is read from the state's intervals and those the mapping supplies; a member opened at the port does not move it, its reads before then being gaps of their own |
| F26 | D9 | An abstract base declares no metrics; a family's figure is combined from its subtypes' |
| F27 | D20, D21 | The restatements of the closed-stretch rule cite it, and its summary is narrowed to a span still current |

**For the author** (ADR-0148 decision 3): D21's case, that a member's span ending after its object closed counts the closed days retroactively, is a question about what the rule should be, not about its wording. *Decided by the author, 2026-10-04 ("yes, stop it at the close"), and recorded as ADR-0148 decision 4: a member's span accrues only while its object is open.*

**Open questions** (ADR-0148 decision 3), every other finding: A3 to A12, A14, A16 to A18, A20, A21, A23, A25 and A27; B3 to B7, B9, B10, B12 to B25, B27 to B36, B41 and B42; C4 to C17, C21 to C38, C41, C42, C45 and C46; D1 to D3, D7, D8, D10 to D19 and D22. Each is answered when a builder reaches it, the Rust core's first slice and its comparison with the Python reference being the instrument, and the answer is written into the rule's home with this record cited.
