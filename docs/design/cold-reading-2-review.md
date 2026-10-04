# Cold reading of the text written for ADR-0146, 2026-10-04

Status: **decided**, 2026-10-04: three questions by the author (§5), the rest in this record's triage (§6), and ADR-0147 records them. At the author's request ("run another cold reading over the ADR-0146 text"), four readers with no context of the design read the text written between 2026-10-01 and 2026-10-04 to state ADR-0146's decisions, each asked whether two careful builders following the text could build engines that behave differently. They found 133 findings, 124 distinct once the overlaps are merged, as §6 numbers them.

**Method.** As in `cold-reading-review.md`: each reader was given only what a builder gets for its area, pointed at the passages citing ADR-0146 and told to read their whole sections, and barred from the decision records, the defect register, `TODO.md`, the lessons, the reviews, the code and git history; each checked its quotations against the files by script. Their reports follow as they wrote them, each opening heading renamed to place it in this record.

- **§1, Recording, correcting and dating a request, and evaluators** (25 findings).
- **§2, erasure, proposals and flow changes, delivery, and the model's details** (35 findings).
- **§3, publishing with a migration, the import, and the harness's conditions** (43 findings).
- **§4, metrics** (30 findings).

## 1. Recording, correcting and dating a request, and evaluators, as the reader wrote it

**What I read.** `docs/design/flow-format.md` §2, §3, §4.4, §4.8, §4.18, §5, §6 and all of §11. `docs/DESIGN.md` §5.5, §5.11, §6 and §7. `docs/design/declaration-syntax.md` §4.2, §6.8 and §8.2, and in §6.9 only `.corrected_by` and `.held`. Both schemas, `flow.schema.json` and `examples.schema.json`. The example modules `service.yaml`, `service.examples.yaml`, `reimbursement.yaml`, `reimbursement.examples.yaml` and `people.yaml`. **Where I went beyond those sections.** In flow-format I also read §1 and §4.1 to §4.7, because I read the file in line ranges. I read §4.13 because §4.8's `corrects` row and §4.4 send there, and §4.19 for "A transition's new state is written before its effect runs", which the `supersede` step cites. From §7 and §7.1 I took only the list of notices. A grep showed me one line of §4.9, which defines "tracked". In DESIGN I read §9, the Proposal paragraph, which flow-format §11.3 cites for `propose`. In declaration-syntax I read §6.2, which flow-format §4.18 cites, and §6.4, which §4.13 cites for corrections. I read the `.created_at` paragraph of §8.1 and the `entered_at`, `changed_since` and duration-unit bullets of §8.3; flow-format §5 cites §8 for these members, and decl-syntax §4.2 names `entered_at`. While looking for `.corrected_by` and `.held` I also saw §6.9's table of sources and its list of row members. No finding rests on anything else in §6.9. **What I did not read.** Nothing under `docs/adr/`, `defects.md`, `TODO.md`, `LESSONS.md`, any review or cold-draft file, the case studies, `scripts/`, `.remember/`, and no git history. **Quotations.** Every quotation below is on a line of the form `> [file §n] "…"`. A Python script pulls out each one and asserts that it occurs character for character in the file named; all 71 passed. A second script checked that every shorter phrase quoted inline in the prose occurs verbatim in the builder documents. Text in italics in a suggested fix is proposed wording, not a quotation. I computed every clock value with Python's `datetime`. Where I say what a builder would do, that is my inference, and I mark it as such.

### What is determined

- **The bound's arithmetic.** A supplied occurred time passes `occurred_within` when it satisfies three conditions:
  - it is no earlier than the recorded time less the bound;
  - it is no later than the recorded time;
  - for the transitions the request takes, it is no earlier than the start of the current interval of what each changes on its own object.

  A time exactly at any of these limits passes. Only the requested transition checks the bound. Guards read `now`, and availability is judged on the current state, never on the state at the occurred time.
- **Supplying a time where none may be given.** It is refused by `occurred_within`, with remedy `self_serviceable` and no object named, in four cases: a transition without `backdating_limit`, a kind without `max_recording_delay`, an assertion, and an observation correction. The refusal holds even when the time supplied equals `now`.
- **The service examples' dating and recording cases.** I recomputed every one, with the clock starting at `2026-01-01T00:00Z`, and each gives its expected verdict under every reading in this report.
  - `a_late_start_is_dated_when_it_happened`: the time supplied is 01-01T01:00. It is one day back, within the 2 days, and after OPEN began at 00:00, so the start applies.
  - `a_start_is_not_dated_further_back_than_two_days`: three days back, so it is refused.
  - `a_start_is_not_dated_before_the_job_opened`: the time supplied is 12-31T23:00, before 01-01T00:00, so it is refused.
  - `a_finish_says_no_time_of_its_own`: `finish` has no `backdating_limit`, so it is refused.
  - `an_inspection_is_dated_when_it_was_done`: the time supplied is 01-02T01:00, two days back against a bound of 7, so it applies.
  - `an_inspection_more_than_a_week_late_is_refused`: eight days back, so it is refused.
  - `a_correction_takes_the_time_of_what_it_corrects`: it supplies a time, so it is refused.
  - `a_corrected_result_replaces_the_mistake`: guards see only what nothing has corrected, so the job finishes.
  - `a_result_is_corrected_once`: refused by `corrects_current` with `self_serviceable`, as DESIGN §5.5 states.
- **The order of a recording's generated guards.** flow-format §4.8 and DESIGN §6 step 4 agree: `occurred_within` first, then `subject_open`, `corrects_current` and `subject_owned`.
- **The remedies of the generated guards** are stated in DESIGN §5.5.
- **A recording's occurred time and the subject.** The time is checked against the bound and the recorded time only. It is never checked against the subject's creation or its states, because a recording changes nothing tracked on the subject. So an inspection may be dated before its job was opened.
- **A correction of an observation:**
  - it takes the corrected observation's occurred time;
  - it is exempt from the bound and from `subject_open`, but not from `subject_owned`;
  - its fields are supplied afresh;
  - its `.recorded_at` is its own.
- **`.held` on a span** is read at the position of the transition that began the span, not at its occurred time.
- **When evaluators are consulted.** It happens before the transaction, so even a request later refused as `unavailable` or `invalid input` has consulted them.
- **What an evaluator refusal says:**
  - an evaluator that does not answer gives `unsatisfied`, unknown, `temporal`;
  - an absent argument makes no call, and the guard is unknown with the condition's declared remedy;
  - a verdict too old is refused as `temporal`, whatever its answer.
- **Stubbed answers in examples.** Each stub gives one answer per function, and an example's map extends its setup's. The reimbursement evaluator examples (`refuse_mismatched_xero_record`, `correct_bill_id_then_verify`, `pay_verified_claim`, `refuse_payment_not_yet_made`) give their expected verdicts.
- **An attribute correction:**
  - it must write exactly what it lists, and must declare a `reason` input;
  - the original write's event is unchanged;
  - a span or transition begun inside the corrected span reads the corrected value;
  - an untracked attribute, such as reimbursement's `xero_bill_id`, has no interval rows to relabel.

### Findings

#### 1. A correction that names another subject: `invalid input` or `corrects_current`
- **Construct:** a recording with `corrects`, and the generated guard `corrects_current`.
- **Case:** jobs `job1` and `job2`. An inspection `mistake` is recorded on `job1`. Then `record` is requested with inputs `{ subject: job2, check: battery, outcome: PASS, corrects: mistake }`.
- **The readings:**
  - (a) flow-format §4.4 and DESIGN §5.11 give the verdict `invalid_input`, naming the input, with remedy `self_serviceable`. This refusal comes at step 4, before any guard.
  - (b) declaration-syntax §6.8 defines `corrects_current` as also requiring "the same subject". A builder implementing §4.8's closed list of `invalid input` causes finds no entry for this case, because the observation named is of the right type. The request then reaches the guards: `unsatisfied`, clause `corrects_current`, `self_serviceable`. DESIGN §5.5 says that clause names "the correction already recorded", and here there is none to name.

  The verdicts differ, and so do the clause and the objects named. Under (a) the "same subject" half of `corrects_current` can never fail.
- **Where the text fails:**
  > [flow-format.md §4.4] "a correction naming another subject is refused as `invalid input`"

  > [declaration-syntax.md §6.8] "`corrects_current`, that a `corrects` names an observation of this kind on the same subject which nothing has corrected yet, so a correction can neither reach another subject's results nor branch a chain"

  > [flow-format.md §4.8] "`invalid input` when an input is one the transition does not take, is not of its declared type, names no object or one of another type, or repeats an element of a set"

  > [DESIGN.md §5.5] "`corrects_current` the correction already recorded"
- **Severity:** changes-outcome
- **Suggested fix:** Add the mismatched subject to the enumerated causes of `invalid input` in §4.8 and DESIGN §6 step 4, or drop it from them. Then state `corrects_current` with only the half that can still fail.

#### 2. Whether a correction must supply `subject`
- **Construct:** the subject of a correction.
- **Case:** `record` is requested with inputs `{ check: battery, outcome: PASS, corrects: mistake }`, giving no `subject`.
- **The readings:**
  - (a) `subject` is a required input of `record` in every case, since the owner end is not optional. Leaving it out gives `invalid_input`.
  - (b) A correction takes the corrected observation's subject, so it may leave `subject` out, and only "giving another" is refused. The request applies, on `job1`.
- **Where the text fails:**
  > [declaration-syntax.md §6.8] "a creation, `create record -> RECORDED`, taking each field as an input, optional exactly where the field is, plus `subject`, and an optional `corrects`, an observation of the same kind and subject that this one replaces, whose subject and occurred time it takes (ADR-0146)"

  > [DESIGN.md §5.11] "It takes the corrected observation's subject, a request giving another being refused as `invalid input`"

  > [flow-format.md §11.3] "A datapoint is recorded by requesting its kind's generated creation, `record`, with `subject` and the kind's fields as inputs"
- **Severity:** changes-outcome
- **Suggested fix:** State whether `subject` is required, optional or refused on a request that names `corrects`. The occurred time already has such a statement ("supplying none of its own").

#### 3. `subject_open` for an observation recorded by an outcome step
- **Construct:** `subject_open` when the recording is made by a `create` step.
- **Case:** `ServiceJob.finish` (WORKING → DONE, where DONE is final) is given the effect step `create: { type: InspectionResult, transition: record, inputs: { subject: this, check: inputs.check, outcome: CheckOutcome.PASS } }`.
- **The readings:**
  - (a) DESIGN §5.11: the subject is read "as the request found it", which is WORKING. `subject_open` passes, and `finish` applies.
  - (b) DESIGN §6 step 6 evaluates a creation's guards against the state produced so far. flow-format §4.19 writes the new state before the effect runs. So the subject is DONE, which is terminal. `subject_open` fails, and the verdict is `call_refused`, clause `InspectionResult.subject_open`, remedy `unreachable_from_here`.

  Second case: a creation (`open`) that records on `this`. Under (a) the request found no subject at all, and the text does not say whether that passes or is unknown. Under (b) the subject is OPEN, and the recording passes.
- **Where the text fails:**
  > [DESIGN.md §5.11] "A transition's outcome may record one with an ordinary `create` step, which is how a transition records what it decided on, even as it enters a final state, since the subject is read as the request found it (ADR-0146)."

  > [DESIGN.md §6] "for each of the rest evaluate its guards **against the state produced so far**, then apply its outcome immediately (ADR-0038)"

  > [flow-format.md §4.19] "A transition's new state is written before its effect runs"

  > [declaration-syntax.md §6.8] "`subject_open`, that the subject is not in a terminal state, for a new observation."
- **Severity:** changes-outcome
- **Suggested fix:** Make `subject_open` an explicit exception to step 6, if that is the intent, and say which reading of the subject's state it uses. Also say what it does when the subject was created in the same request.

#### 4. The time a correction's own event and its `.created_at` carry
- **Construct:** the occurred time of an observation correction.
- **Case:** `mistake` is recorded at 01-01T01:00. A correction is recorded at 01-01T02:00 and supplies no time.
- **The readings:**
  - (a) The correction's creation event has occurred time 01:00, the one it takes, and recorded time 02:00. Its `.created_at`, "the event's occurred time", is then 01:00.
  - (b) Only the observation's `.occurred_at` column holds 01:00. The event's occurred time is 02:00, the time the correction was made, and so is `.created_at`.

  These readings give different event rows and a different `.created_at`. That changes what anything windowed on the event's occurred time selects.
- **Where the text fails:**
  > [flow-format.md §4.4] "whose subject and occurred time it takes"

  > [declaration-syntax.md §6.8] "and `.occurred_at`, when the fact happened, stored in its own column."

  > [declaration-syntax.md §8.1] "`.created_at` is when the creation **happened**: the event's occurred time for a backdated creation"
- **Severity:** changes-value
- **Suggested fix:** State whether the time a correction takes is its event's occurred time, and so its `.created_at`, or only the value of its `.occurred_at` column.

#### 5. The order of a recording's generated guards is listed differently in declaration-syntax
- **Construct:** the order in which a recording's generated guards are evaluated.
- **Case:** a correction that names an observation already corrected, and that also supplies `occurred_at`.
- **The readings:**
  - (a) flow-format §4.8 and DESIGN §6 put `occurred_within` first, so the refusal is `occurred_within`, naming no object.
  - (b) A builder who takes declaration-syntax §6.8's list as an order (`subject_open`, `corrects_current`, `occurred_within`, `subject_owned`) reports `corrects_current`, naming the earlier correction.

  §6.8 does not claim to give an order, and flow-format is explicit, so I judge that a careful builder follows (a).
- **Where the text fails:**
  > [declaration-syntax.md §6.8] "The creation carries four guards, named in a verdict like any other:"
- **Severity:** wording
- **Suggested fix:** List the four in the order of flow-format §4.8, or say that the list is not the evaluation order.

#### 6. Whether objects created by `create` steps take the request's occurred time, recordings and corrections among them
- **Construct:** how calls, creations and cascades inherit the time.
- **Case:** `ServiceJob.start` (with `backdating_limit: 2 days`) is given an effect step that records an `InspectionResult` on `this`. The request has `occurred_at: now - 1 day`.
- **The readings:**
  - (a) A creation is part of "each it calls or cascades to". The observation's occurred time is `now - 1 day`, and its kind's bound is never checked, since "only the requested transition checks the bound".
  - (b) §4.8 names calls and cascades only. A `create` is neither, so the new object's occurred time is its recorded time, `now`. On this reading, `.created_at` and the first interval begin at `now`.
  - (c) The observation's own generated `occurred_within` holds the inherited time to its kind's rule. With service's 7 days the recording applies, as under (a). With a kind that has no `max_recording_delay`, the inherited time counts as a time supplied, and the effect is refused as `call_refused`, clause `InspectionResult.occurred_within`.

  So the same request applies under (a) and (b) but is refused under (c), and (a) and (b) record different times.

  The same contradiction appears without any backdating when an outcome step records a correction. §4.8 makes every transition record the request's time, which is `now`. §4.4 makes the correction take the corrected observation's time. The two cannot both hold.
- **Where the text fails:**
  > [flow-format.md §4.8] "Every transition the request takes, the requested one and each it calls or cascades to, records the same occurred time"

  > [flow-format.md §4.4] "on a kind without it the occurred time is the recorded time, a request supplying one being refused by `occurred_within`"

  > [declaration-syntax.md §4.2] "every transition the request cascades to records the same occurred time, since one request is one change (ADR-0095)"

  > [declaration-syntax.md §8.1] "`.created_at` is when the creation **happened**: the event's occurred time for a backdated creation"
- **Severity:** changes-outcome
- **Suggested fix:** State whether a `create` step's object, an observation included, records the request's occurred time. Say which of the kind's rules then apply. Say which time wins for a correction recorded by an outcome step.

#### 7. The interval dating of called and cascaded transitions that are not themselves backdatable
- **Construct:** what the occurred time dates in a cascade.
- **Case:** suppose `Claim.withdraw` has `backdating_limit: 2 days` and is requested with `occurred_at: now - 1 day`. It cascades `ClaimLine.close`, which is `only_via` and so can carry no `backdating_limit`.
- **The readings:**
  - (a) flow-format §4.8: every transition records the same occurred time. The line's LISTED interval closes, and CLOSED opens, at `now - 1 day`, as the claim's do.
  - (b) DESIGN §6 step 8 uses the occurred time only "where the transition is backdatable". `close` is not, so the line's interval changes at `now`, one day after the claim's. declaration-syntax §4.2's "Without the marking, the occurred time is the recorded time" points the same way.

  The line's time in LISTED differs by one day between the readings.
- **Where the text fails:**
  > [DESIGN.md §6] "update the interval index for every changed state and tracked value, using the occurred time where the transition is backdatable"

  > [declaration-syntax.md §4.2] "Without the marking, the occurred time is the recorded time, and a request that supplies one is refused by the same clause."

  > [flow-format.md §4.8] "records the same occurred time and checks, when it runs, that the time does not precede the current interval of a state or tracked value it changes on its own object"
- **Severity:** changes-value
- **Suggested fix:** Change DESIGN §6 step 8 to read *using the request's occurred time*, and limit "Without the marking" to the requested transition.

#### 8. The verdict and clause of an `occurred_within` failure in a call or cascade
- **Construct:** what a refusal says when a caused transition fails the interval check.
- **Case:** the backdated `withdraw` of finding 7, with occurred time 01-04T12:00, on a claim whose line was added at 01-05T06:00. The line's `close` would then precede its current interval.
- **The readings:**
  - (a) flow-format §4.8: the request is refused "as `unsatisfied` by `occurred_within`". The expectation is `{ verdict: unsatisfied, clause: occurred_within, remedy: self_serviceable }`, with no object named, as DESIGN §5.5 says for this guard.
  - (b) DESIGN §5.5 and flow-format §11.4: a cascade refuses with the part's own verdict, naming the part, and with its clause qualified, `ClaimLine.occurred_within`. For a `call` the verdict is `call_refused`, with the clause `<Type>.occurred_within` and the object targeted.

  For a call, the readings give different verdicts. For a cascade, they give different clause text and different objects named.
- **Where the text fails:**
  > [flow-format.md §4.8] "A failure in a call or cascade refuses the call, and so the request, as `unsatisfied` by `occurred_within` (ADR-0122)"

  > [DESIGN.md §5.5] "a part a cascade drives refuses with its own verdict instead, naming the part"

  > [flow-format.md §11.4] "for `call_refused`, optional, the refused call's or creation's clause, always qualified"

  > [DESIGN.md §5.5] "and `actor_known`, `not_erased` and `occurred_within` none"
- **Severity:** changes-outcome
- **Suggested fix:** In §4.8, state the failure in the general form (*gives `call refused`, or the part's own verdict, with the clause qualified*), and say what the refusal names.

#### 9. When "a state or tracked value it changes" is decided: before the effect, or after it
- **Construct:** the interval half of `occurred_within`.
- **Case:** a type with a tracked enum `priority`, and an internal transition `reprioritise` with `required_inputs: [priority]` and `backdating_limit: 2 days`. `priority` became HIGH at 01-01T10:00. At 12:00, `reprioritise` is requested with `priority: HIGH` (no change) and `occurred_at` 05:00.
- **The readings:**
  - (a) Static: the transition writes a tracked value, so the time is checked against the HIGH interval, which began at 10:00. The request is refused by `occurred_within`.
  - (b) Dynamic: nothing changes, so nothing is checked, and the request applies, dated 05:00.

  Where the value comes from an `assign` with a computed expression, whether it changes is known only when the step runs ("each value read at the moment it is applied"). That is after the generated guards, which §4.8 evaluates first. So under (b) the check cannot run in the order §4.8 gives. A builder must either move it after the effect, which can report an own guard's clause before `occurred_within`, or fall back to (a). As written, the passage cannot be implemented for computed writes.
- **Where the text fails:**
  > [flow-format.md §4.8] "checks, when it runs, that the time does not precede the current interval of a state or tracked value it changes on its own object, so an internal transition that changes neither checks none"

  > [declaration-syntax.md §4.2] "The bound, both how far back and that the time not precede the current interval, is a generated guard named `occurred_within`"

  > [flow-format.md §4.8] "`unsatisfied` when a guard fails, the generated guards first"

  > [DESIGN.md §6] "each value read at the moment it is applied (ADR-0054)"
- **Severity:** changes-outcome
- **Suggested fix:** State whether "changes" means *declares a write to* or *ends with a different value*. If it is the latter, say where in the order of refusals the check falls for a write an effect computes.

#### 10. What `now` is in an effect of a backdated request
- **Construct:** what the occurred time dates and what the recorded time dates.
- **Case:** suppose `Claim.submit` had `backdating_limit: 2 days`. A request is made with `occurred_at: now - 1 day`, and the effect runs `assign: { location: submitted_at, expr: now }`.
- **The readings:**
  - (a) `now` is the time of the request, its recorded time, everywhere, so `submitted_at` is the recorded time.
  - (b) The text states only that *guards* still read `now`. An effect writes the change, which the occurred time dates, so `submitted_at` is the occurred time.

  `submission_to_payment` shifts by one day between them.
- **Where the text fails:**
  > [flow-format.md §5] "`state` is the object's current state, and `now` is the time of the request."

  > [declaration-syntax.md §4.2] "The event keeps both times and the interval index uses the occurred one; guards still read `now`."
- **Severity:** changes-value
- **Suggested fix:** Say that `now` in every expression of a request, effects and inputs included, is its recorded time. The alternative is to say which places read the occurred time.

#### 11. Whether `occurred_at` belongs to the request an idempotency key identifies
- **Construct:** dating a request, together with replay.
- **Case:** a scheduler requests `expire` with key K and `occurred_at` set to 01-03T00:00. The request applies. A retry carries the same K and `occurred_at` set to 01-03T00:05.
- **The readings:**
  - (a) The occurred time is part of the request, so the retry is "a different request" and is a fault.
  - (b) Like `expected_version`, it is not part of the request, so the retry is a replay and returns the first result, dated 00:00.

  DESIGN §6's list of what a request names leaves `occurred_at` out entirely, though §5.11 calls it a request field.
- **Where the text fails:**
  > [DESIGN.md §6] "A request names an object, a transition, its inputs and the actor, and optionally the version the caller last read, an idempotency key (ADR-0014), and a free-form `context` string"

  > [DESIGN.md §6] "If this actor's idempotency key has been applied before to this request, return the original result, marked as a replay, **whatever `expected_version` the retry carries**"

  > [DESIGN.md §5.11] "the occurred time is a request field bounded by a generated `occurred_within` guard (ADR-0099)"
- **Severity:** changes-outcome
- **Suggested fix:** Add `occurred_at` to DESIGN §6's list of request fields, and state whether a retry that differs only in it is a replay or a different request.

#### 12. Whether a proposal carries an occurred time, and from when the bound is measured
- **Construct:** dating a proposed request.
- **Case:** `start` is made `proposable` and keeps `backdating_limit: 2 days`; nothing forbids the combination. A proposal is filed on 01-10, saying the start happened on 01-09, and is approved on 01-12.
- **The readings:**
  - (a) A proposal holds "target, transition, inputs and proposer", and the examples' `propose` has no `occurred_at`. So the approved start is dated 01-12, and a proposable transition can never be backdated.
  - (b) A proposal carries the occurred time. Measured at approval it is three days back, so the start is refused by `occurred_within` and the proposal stays pending.
  - (c) The bound is measured at filing, one day back, so the start applies, dated 01-09.
- **Where the text fails:**
  > [DESIGN.md §9] "A **Proposal** holds a request filed for approval instead of executed: target, transition, inputs and proposer"

  > [DESIGN.md §9] "Approving it executes the recorded request **as the approver**, with the approval as cause, and every guard is re-evaluated at execution under the **current** declaration."

  > [flow-format.md §11.3] "it takes a `request`'s `type` or `object`, `transition`, `inputs`, `actor` and `after`, and `proposal`"
- **Severity:** changes-outcome
- **Suggested fix:** State whether a proposal may carry an occurred time and, if so, the time against which the bound is measured.

#### 13. The day a refused or flagged backdated request is counted on
- **Construct:** what is dated by the recorded time.
- **Case:** a `start` with `occurred_at` set to 01-02T23:00 is recorded at 01-03T01:00 and refused, or it applies and raises a `warn`.
- **The readings:**
  - (a) The attempt row, and the daily rollup that outlives it, count the request on the day it was recorded, 01-03.
  - (b) They count it on the day of its occurred time, 01-02, which the row also holds.
- **Where the text fails:**
  > [DESIGN.md §6] "the request's own values: every non-personal input and its occurred time"

  > [DESIGN.md §6] "the prune adds each day's counts to a permanent daily rollup in the same transaction"
- **Severity:** changes-value
- **Suggested fix:** State which of the attempt's two times dates it, for the rollup and for reads.

#### 14. Where the span an attribute correction relabels begins
- **Construct:** `corrects: [<attribute>]` and its relabelling.
- **Case:** take `ServiceJob.engineer` and a hypothetical internal transition `correct_engineer` with `corrects: [engineer]`. The engineer is `ana` from t0. A reassignment writes `ben` at t1, and a second reassignment writes `ben` again at t1.5, which does not change the tracked value. At t2 the engineer is corrected to `carl`, and at t3 corrected again to `dan`.
- **The readings:**
  - (a) The phrase "from when it was written" means the last write of the attribute. The first correction relabels from t1.5, so the `ben` interval row [t1, t2) must be split, or only partly relabelled. The second correction relabels from t2 only, the first correction's own write, so [t1, t2) stays `carl`.
  - (b) It means the start of the current value's interval. The first correction relabels [t1, t2) whole, and the second relabels [t1, t3), because `carl` now reads from t1.

  Under (b) the [t1, t2) row has been relabelled twice. The definition "the event of the correction that relabelled it" is singular, so the text does not say whether that row's `.corrected_by` holds the first correction or the second.
- **Where the text fails:**
  > [flow-format.md §4.13] "It replaces the wrong value for the span it was held, explicitly: the value each attribute it lists holds now is read, from when it was written, as the corrected one"

  > [declaration-syntax.md §6.4] "the index of who held what reads the corrected value from when the wrong one was written"

  > [declaration-syntax.md §6.9] "`.corrected_by`, on an interval row, is the event of the correction that relabelled it, absent where none did"
- **Severity:** changes-value
- **Suggested fix:** Define the span as one of these: the current value's interval, or the time since the attribute's last writing event. State what `.corrected_by` holds on a row relabelled twice.

#### 15. Whether a correction opens a new interval at its own time
- **Construct:** the interval index after an attribute correction.
- **Case:** as in finding 14 without the repeat: `ana` from t0 (01-01), `ben` from t1 (01-02), corrected to `carl` at t2 (01-04). The derived attribute `handoffs` is `count(i in this.intervals(engineer) where i.value is not null) - 1`.
- **The readings:**
  - (a) Merged. The relabelled row becomes `carl` [t1, open). `handoffs` is 1, and `entered_at(engineer)` is t1 = 01-02.
  - (b) Split. DESIGN §6 step 8 updates the index "for every changed state and tracked value", and the correction changes the value from `ben` to `carl`. So it closes the relabelled row at t2 and opens `carl` [t2, open). `handoffs` is 2, and `entered_at(engineer)` is t2 = 01-04.

  At 01-04T01:00, a guard `entered_at(engineer) <= now - 2 days` holds under (a) (01-02 ≤ 01-02T01:00) and fails under (b). A backdated correction raises the further question of whether the relabelled span ends at the correction's occurred time or at its recorded time.
- **Where the text fails:**
  > [DESIGN.md §6] "update the interval index for every changed state and tracked value"

  > [declaration-syntax.md §8.3] "`entered_at(<member>)`, for a tracked member, when it took its current value"

  > [service.yaml] "count(i in this.intervals(engineer) where i.value is not null) - 1"
- **Severity:** changes-outcome, through a guard over `entered_at`; changes-value for `handoffs`.
- **Suggested fix:** State whether a correction's write opens an interval of its own or continues the relabelled one. Also say what a backdated correction dates.

#### 16. Which interval rows carry `.corrected_by`
- **Construct:** `.corrected_by`.
- **Case:** as in finding 15, with a WORKING span begun at t1.5 while `ben` was assigned. After the correction its `.held(engineer)` reads `carl`.
- **The readings:**
  - (a) The phrase "every interval row it relabels" means the corrected member's own rows, in `intervals(engineer)`. The WORKING row's `.held` changes, but its `.corrected_by` stays absent.
  - (b) A state row whose `.held` value the correction changed is relabelled too, so it carries the correction's event.

  Reading (b) is the only one under which a state span's `.corrected_by`, a member §6.9 gives `<Type>.intervals`, could ever be present, since a correction never changes a state. A count of the corrected spans in `ServiceJob.intervals` is 0 under (a) and 1 under (b).
- **Where the text fails:**
  > [flow-format.md §4.13] "and every interval row it relabels carries `.corrected_by`"

  > [declaration-syntax.md §6.9] "`.legacy`, `.migrated`, `.corrected_by`, `.held(<member>)`"
- **Severity:** changes-value
- **Suggested fix:** Say whether a state span whose `.held` the correction changes is a relabelled row.

#### 17. `.held` on the correction's own transition row
- **Construct:** `.held` after a correction.
- **Case:** as in finding 15. `carl` himself requests the correction at t2. Consider a metric over transitions that counts `t.actor_id != t.held(engineer).actor_id`.
- **The readings:**
  - (a) On a transition, `.held` is "the value before the transition's own writes". For the correction's row that is `ben`, the value it replaced, so the row counts as acted on by someone other than the assignee.
  - (b) The correction relabels the span up to itself, so its own row credits `carl` and does not count.
- **Where the text fails:**
  > [declaration-syntax.md §6.9] "and, on a transition, the value before the transition's own writes (ADR-0146)"

  > [flow-format.md §4.13] "so a span or a transition begun in that span credits the corrected value"
- **Severity:** changes-value
- **Suggested fix:** State what `.held` gives for the corrected attribute on the correction's own row.

#### 18. Whether a correction is a change for `changed_since`
- **Construct:** how a correction relabels history, as `changed_since` reads it.
- **Case:** an `approve` records an approval at event E, after `price` was written. Then a correction `corrects: [price]` applies. Finally the guard `any(a in approvals where not changed_since([price], a.at_event))` is evaluated.
- **The readings:**
  - (a) The correction is a write. The index of the event that last wrote `price` moves to the correction, `changed_since` is true, and the guard refuses.
  - (b) The corrected value "is read, from when it was written, as the corrected one". The value has been the corrected one since before E, so `changed_since` is false, and the guard passes.

  I infer that (a) is the likelier reading, but the text does not say that relabelling leaves `changed_since` alone.
- **Where the text fails:**
  > [flow-format.md §5] "`changed_since([<attribute>, …], <event>)` is true when any of the attributes, or any part of a composite end named, has changed since that event"

  > [DESIGN.md §7] "A per-attribute index of the event that last wrote it makes `changed_since` a constant-time comparison (ADR-0048)"

  > [flow-format.md §4.13] "the value each attribute it lists holds now is read, from when it was written, as the corrected one"
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a correction counts as a change for `changed_since`.

#### 19. Which events carry the `corrected` provenance
- **Construct:** the provenance of corrections.
- **Case 1:** an external transition marked `corrects: [engineer]`.
- **Case 2:** an observation correction (`record` with `corrects`).
- **The readings:**
  - Case 1: flow-format §4.13 gives its event `corrected`. DESIGN §7 produces `corrected` only from "an internal transition", so a builder following it records `observed`, or refuses `corrects` on an external transition.
  - Case 2: the correction is a creation, which is not a transition marked `corrects`, so its event is `observed`. A builder could equally mark it `corrected`, since it records a corrected value.
- **Where the text fails:**
  > [flow-format.md §4.13] "**A correction** is an internal or external transition marked `corrects: [<attribute>, …]`, whose events carry the `corrected` provenance."

  > [DESIGN.md §7] "An internal transition that records a corrected value for an earlier mistake, with a reason; the earlier event is not rewritten"
- **Severity:** changes-value
- **Suggested fix:** Change DESIGN §7's row to read *an internal or external transition marked `corrects`*. State the provenance of an observation correction's event.

#### 20. Which consultations "every transition its declaration can reach" makes
- **Construct:** when the engine consults evaluators.
- **Case:** suppose `ClaimLine.close` had a guard `archive.has(receipt)`. `Claim.withdraw` is requested on a claim whose only line is REMOVED, which is final. The cascade will pass over that line, so `close` never runs.
- **The readings:**
  - (a) Static. `close` is reachable by declaration, so the guard is consulted, once per line in committed state, and the consultation is recorded on the event. In an example without a stub for `archive.has`, the example fails, since "A call with no stubbed answer fails the example."
  - (b) Consultation covers only the parts the cascade will drive, judged from committed state, so here nothing is consulted, and the example applies.
  - (c) The guard is consulted once, not once per element. That cannot be done when each element supplies its own argument.

  The same split arises for a call inside a `foreach` whose `where` selects nothing, and for a call whose target reference is absent. The example's verdict differs between the readings, and so do the verdicts recorded on the event or the attempt row.
- **Where the text fails:**
  > [flow-format.md §4.18] "A request consults the evaluator guards of every transition its declaration can reach, a call's, a creation's and a cascade's included, each argument resolved against committed state; an argument that is absent makes no call, and its guard is unknown and refused with its condition's remedy."

  > [flow-format.md §11.3] "A call with no stubbed answer fails the example."
- **Severity:** changes-outcome (the example runner and publish); changes-value (the verdicts recorded).
- **Suggested fix:** State whether reachability is by declaration or by committed state, and whether the guard is consulted per element, including the parts a cascade passes over. Say whether a function no consultation reaches needs a stub.

#### 21. `fresh`: against what time an answer's age is measured, and what "live" is
- **Construct:** `fresh` and a bound of zero.
- **Case:** a function with no `fresh`, so its bound is zero. The adapter answers at 12:00:00.2 and stamps that as the answer's as-of time. The transaction evaluates the guard at 12:00:00.5, perhaps after a serialisation retry.
- **The readings:**
  - (a) Age is `now` less the as-of time, where `now` was fixed when the request arrived at 12:00:00.0. The age is negative, so the answer is used.
  - (b) `now` is fixed when the transaction opens, after the consultation, or the age is judged when the guard is evaluated. The age is 0.3 s, more than zero, so the guard is refused as `temporal`. Every function without `fresh` then always refuses.
  - (c) "Live" is a property of the answer, uncached, whatever its stamp.

  The text says neither when `now` is fixed relative to the consultation, nor what applies when an evaluator reports no as-of time. Nor does it say what as-of time a stubbed answer in an example carries.
- **Where the text fails:**
  > [flow-format.md §4.18] "optionally, `fresh`, the oldest an answer may be, by the as-of time the evaluator reports for it, a duration such as `30 min`; without it the bound is zero, so only a live answer is used."

  > [DESIGN.md §5.5] "a function's `fresh` bounds its age, zero where none is declared, so only a live answer is used, and past it the verdict is refused as `temporal`"

  > [DESIGN.md §6] "External evaluators and metrics named by any guard in the request are consulted **first, outside the transaction**"
- **Severity:** changes-outcome
- **Suggested fix:** State the instant an answer's age is measured to, in relation to the request's `now` and to retries. State how an answer without an as-of time is treated, and that a stub counts as live.

#### 22. A verdict too old for `fresh`: false or unknown?
- **Construct:** what an evaluator refusal says.
- **Case:** `xero.bill_paid` answers `satisfied`, with an as-of time 20 minutes old against `fresh: 10 min`.
- **The readings:**
  - (a) The verdict is unusable, so the guard is unknown, like an evaluator that does not answer. DESIGN's "as a verdict too old to use is refused" can be read as saying that a verdict too old is refused the same way.
  - (b) The guard is false, with only the remedy forced to `temporal`.

  The refusal's false-or-unknown marker differs, and so does the attempt row's `.unknown`.
- **Where the text fails:**
  > [flow-format.md §4.18] "A verdict too old for its `fresh` bound is refused as `temporal`"

  > [DESIGN.md §5.5] "**An evaluator that does not answer** refuses its guard as unsatisfied and unknown, with the remedy `temporal`, since it may answer a moment later, as a verdict too old to use is refused (ADR-0049, ADR-0138)."
- **Severity:** changes-value
- **Suggested fix:** Say whether a verdict too old makes the guard unknown or false.

#### 23. The scope of check 61 is stated three ways
- **Construct:** which evaluator arguments a publish refuses.
- **Case:** an internal transition on `Claim` with `required_inputs: [xero_bill_id]` and the guard `bill_matches_xero`, which reads `xero_bill_id`. The guard reads the value from before the request, which committed state holds, so the two resolutions agree.
- **The readings:**
  - (a) DESIGN §5.5: the request's own outcome writes the argument, so the publish refuses it.
  - (b) flow-format §4.18: the outcome *reaching* the guard is what counts, and the transition's own writes do not reach its own guard, so the flow is valid.
  - (c) declaration-syntax §6.2 confines the check to "an input bound from a member its caller sets" in a transition reached by a call or create. On that reading even the cascaded case of (b), where a whole writes the value a part's guard reads, passes the publish. At run time it then mismatches on every retry.
- **Where the text fails:**
  > [DESIGN.md §5.5] "an argument the request's own outcome writes is refused at publish (ADR-0092)"

  > [flow-format.md §4.18] "an argument that reads a value the outcome reaching the guard writes, which could never agree before the transaction, is refused at publish (step 4, `check 61`)"

  > [declaration-syntax.md §6.2] "An argument that reads a value the outcome reaching the guard writes — an input bound from a member its caller sets, in a transition reached by a `call` or `create` — could never agree before the transaction, so it is a publish error (check 61)."
- **Severity:** changes-outcome (the publish)
- **Suggested fix:** State check 61 once, with "reaching" defined: is a transition's own write included, and is a write through a path or by a cascade included? Make the other two passages cite it.

#### 24. Expectations cannot state a transition's date, and `values` states an observation's
- **Construct:** the examples format for dating.
- **Case 1:** `a_late_start_is_dated_when_it_happened` says the start "is dated when it happened". It expects only `{ verdict: applied, state: WORKING }`, so an engine that ignored `occurred_at` entirely would pass it.
- **Case 2:** `an_inspection_is_dated_when_it_was_done` expects `values: { occurred_at: now - 2 days }`. But §11.4 defines `values` as attributes, and `occurred_at` is a member every observation has, "stored in its own column", not a field.
- **The readings:**
  - (a) `values` admits an observation's built-in members.
  - (b) A strict step 3 refuses the name as one that "does not resolve".

  No form exists to expect a transition's occurred time, or an interval's start, on a non-observation object.
- **Where the text fails:**
  > [flow-format.md §11.4] "`values` | for `applied`, optional: attributes and the values they hold afterwards, written as §11.3's values"

  > [service.examples.yaml] "expect: { verdict: applied, state: RECORDED, values: { occurred_at: now - 2 days } }"

  > [service.examples.yaml] "it is dated when it happened, since a start may reach back two days."
- **Severity:** wording. The example file shows the intent, but the dating examples do not distinguish an engine that dates correctly from one that does not.
- **Suggested fix:** Say that `values` may name an object's built-in members (`occurred_at`, `created_at` and the rest). Consider an expectation of the occurred time the request recorded.

#### 25. The form of a recording request and of timestamp values
- **Construct:** the examples format for recording.
- **The cases:**
  - (a) §11.3 requires `transition` to be "a transition the type declares, its own, its machine's or its base's". `record` is generated, not declared, so a literal step 3 would refuse every recording step, against the Recording paragraph.
  - (b) That paragraph lists `subject` and the fields but not `corrects`, which the examples pass as an input.
  - (c) A timestamp is written "in UTC, as `2026-01-01T09:00Z`". Whether seconds (`…T09:00:00Z`, the form in which the clock's start is written), fractions or an offset such as `+02:00` are accepted, refused or converted is unstated.
- **The readings:** for (c), one runner rejects such a value as an input of the wrong form at step 3, and another accepts it.
- **Where the text fails:**
  > [flow-format.md §11.3] "a transition the type declares, its own, its machine's or its base's"

  > [flow-format.md §11.3] "a timestamp, in UTC, as `2026-01-01T09:00Z`, or as `now`, `now + <duration>` or `now - <duration>`, the clock at that step"

  > [flow-format.md §11.3] "Every example's clock starts at `2026-01-01T00:00:00Z`"
- **Severity:** wording
- **Suggested fix:** Add the generated transitions (`record`, `forget`) to what a step may name, and `corrects` to the Recording paragraph. Give the grammar of a timestamp value.

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 13 | 1, 2, 3, 6, 8, 9, 11, 12, 15, 18, 20, 21, 23 |
| changes-value | 9 | 4, 7, 10, 13, 14, 16, 17, 19, 22 |
| wording | 3 | 5, 24, 25 |
| **total** | **25** | |

## 2. Erasure, proposals and flow changes, delivery, and the model's details, as the reader wrote it

I read the assigned builder text: `docs/DESIGN.md` §5.4, §5.5, §5.6, §7, §8, §9, §10 and §14; `docs/design/flow-format.md` §3, §4.3, §4.8, §4.11, §4.12, §4.13, §4.14, §4.19 and all of §11; `docs/design/declaration-syntax.md` §2, §6.4 and the check-table rows 26 and 37; `docs/design/library-api.md` whole; `flow.schema.json` and `examples.schema.json`; and the example modules `customers.yaml`, `customers.examples.yaml`, `delivery.yaml`, `delivery.examples.yaml`, `servicedesk.yaml` and `people.yaml`. I left those sections in the following places, either following a citation or because a read or search returned the text next to an assigned section: DESIGN §5.1–§5.3 (creation order, `summary`, coverage of a composition), §5.9 (publishing and proposals' invalidation), §5.11 (observations, labels and their `forget`) and all of §6 (the execution order that §5.5, flow-format §4.8 and §11 cite); flow-format §1, §2, §4.1, §4.2, §4.4–§4.7, the first lines of §4.9, §4.10 (machines' required invariants), §4.15–§4.18, §5 and §6 (generated guards and invariants), the first lines of §7 and the introduction of Appendix A; declaration-syntax §6.1–§6.3, §6.5 and the first lines of §6.6, read next to §6.4, the §10 introduction and check rows 1–4, 8, 9, 10, 11, 39 and 60 (rows 10, 39 and 60 are cited by §6.4 and DESIGN §8), plus the single lines that searches for `ADR-0146`, `supersede` and the aggregate forms returned from §6.6–§6.11, §8.2, §9.2–§9.5 and check rows 23, 33, 49 and 56. I did not read anything under `docs/adr/`, `docs/design/defects.md`, `TODO.md`, `docs/LESSONS.md`, `docs/design/cold-reading-review.md`, `docs/design/reimbursement-cold-draft.md`, any `*review*.md`, the case studies, `scripts/`, `.remember/`, any other example module, `publish-and-import.md`, `storage-schema.md`, `renderers.md`, `metric-scenarios.md` or the PRD, and I used no git history. A Python script (`verify_report.py` in the scratchpad) extracted every quotation from this report and checked it, character for character, against the builder files: 123 quotations, none missing, each found in the file this report attributes it to. A quotation is delimited by curly double quotes, characters the builder files never use, and a `\n` inside one stands for a line break in the source. Where I say what an engine "would" do, that is my inference from the text, and I have worked it from concrete cases.

### What is determined

- **Erasure.** It writes absence to the object's personal attributes and keeps the event skeleton. It runs at any state, a final one included, and it is irreversible. It reaches the legacy entries, the label notes, the observations through their generated `forget`, the personal enum values in the interval index, and the event payloads that carried a personal input. Every object whose attribute it redacts gets an `erased` event, caused by the erasure, which advances its version. An object the taint analysis reached is not "erased" and may be written again. `not_erased` has the remedy `unreachable_from_here` and names no object. It is evaluated among the generated guards in the order `occurred_within`, `whole_open`, `not_erased`, `successor_not_superseded`, and it never refuses an erasure. An import skips an erased object's personal attributes. The group is every object whose chain ends in the same survivor, whichever member is named. Computed from the examples: `erasing_reaches_the_contacts` applies with `cascaded: [Contact.forget]`, its `active_invariant` failure admitted; `erasing_follows_a_merge` applies with `[Customer.forget]`; `what_was_erased_stays_erased` is refused by `not_erased` before `email_provided`.
- **Supersession.** A superseding state is final. `supersede` is written once, at the top level, and its successor is a required input or an earlier `create` result, never `this`. `successor_not_superseded` refuses the object itself or an already superseded successor, remedy `self_serviceable`. Because the new state is written before the effect runs, a chain closed inside one request is refused. `a_customer_merged_into_itself_is_refused` gives `call_refused` with `Customer.successor_not_superseded`.
- **Proposals.** The lifecycle is `pending → executed | rejected | withdrawn | invalidated`. Approval executes the recorded request as the approver, under the current declaration. A refused approval returns the request's verdict and leaves the proposal pending. A decision on a proposal that is not pending is `unavailable`. A publish invalidates on three stated grounds, and its mappings never rewrite a proposal's inputs. All six proposal examples in `delivery.examples.yaml` compute as they expect.
- **DeclarationChange.** Its states and most of its transitions are named, publishing is guarded by `expected_version`, and a publish supersedes other open changes in the same transaction.
- **Delivery.** The settled cursor is a (transaction, position) pair below the snapshot's `xmin`, or the position on SQLite. Each object's events arrive in order, at least once. `acknowledge` moves the cursor either way. Lag is measured by recorded time, and death stops no pull. A filter selects by type or family, transition names and `changes_state`. Migrations name `migrate`, imports and refreshes `import`, and redactions of another object `redact`.
- **The model's details.** A final transition of a whole, an assertion that lists a final state included, is covered by exactly one of a cascade or `survives` (flow-format §4.3, check 37 and §4.14 agree). An abstract base has no `summary`, and the schema agrees. `unique` is a type scan, partial when the attribute itself is absent, and a base's `unique` spans the family. A loop's `limit` counts the elements `where` selects; a loop over the limit is refused, never truncated, and the snapshot is taken when the loop starts. `range` runs 1..n, and an absent or non-positive value runs no step. `result` is scoped to the loop that binds it. The named-objects rule for unsatisfied clauses and each listed generated guard is stated, and so are the outer structure of the invariant order and the depth-first, ascending-id order of cascades and creations. Parts in a final state are passed over.

### Findings

#### 1. Does an erasure admit every invariant that reads an erased attribute, or only the ones it leaves violated?
- **Construct:** erasure's admissions.
- **Case:** `customers.yaml`, setup `a_prospect` (Dana in `PROSPECT`, with a name and no email); request `dana.forget`. `active_invariant` reads `email` (it is `state != ACTIVE or email is not null`) and still holds afterwards.
- **The readings:** (a) DESIGN §5.6: the erasure admits and records `active_invariant` on Dana. The erasure event is then an override, `exceptions(Customer)` lists Dana, and the `overrides` metric counts one. (b) DESIGN §8 and flow-format §4.13: nothing is admitted, the event is not an override, and `exceptions` does not list Dana.
- **Where the text fails:** DESIGN §5.6 says “An erasure admits every invariant reading an attribute it erased, and records the admission, since writing absence may break one and refusing the erasure is not available (ADR-0060).” DESIGN §8 says “An erasure records an admission only for an invariant it leaves violated, as a declared admission is recorded only where the request breaks the invariant.” flow-format §4.13 has “It admits each invariant it leaves violated, recording the admission as a declared one is”. An override is “any event that records an admission: an assertion's, an erasure's, or a publish's `admit`.” (DESIGN §8).
- **Severity:** changes-value.
- **Suggested fix:** Change the §5.6 sentence to the §8 rule, or the reverse, so that one criterion decides which invariants an erasure records as admitted.

#### 2. On which object is an erasure's admission recorded when the invariant fails on an object the erasure did not write?
- **Construct:** erasure's admissions; the object an admission names.
- **Case:** suppose `Order` declares the traversal invariant `customer.email is not null` (through `orders`/`customer`, whose ends name each other). Dana has an open order, and she is erased. Step 7 re-checks the order's invariant by reverse traversal, and it fails on the order.
- **The readings:** (a) The admission names (order, `Order.inv`). The order's next request is not refused for that invariant, and `exceptions(Order)` lists the order. (b) As §8 lists it, the admission is recorded "on the erased object", that is (Dana, `Order.inv`). Since an admission suppresses the check only "for that object", the order's next transition, such as `fulfil`, is refused as invariant violated, and `exceptions(Customer)` lists Dana instead.
- **Where the text fails:** DESIGN §8 says the admission “is recorded as an admission on the erased object, or on the object whose redacted value broke the invariant;” while an admission “names an object and an invariant, suppresses that invariant's check for that object” (DESIGN §8). In this case Dana is both the erased object and the one "whose redacted value broke" the invariant, and neither of them is the order the invariant fails on.
- **Severity:** changes-outcome.
- **Suggested fix:** State that an erasure's admission names the object the invariant fails on. Say also whether an invariant broken by a non-erasure transition that an erasure's effect calls is admitted as well.

#### 3. `not_erased`: is `clear` refused, and is the guard decided before the outcome or when the write happens?
- **Construct:** the generated guard `not_erased`.
- **Case 1:** Dana is erased, and a transition `forget_email: { kind: internal, effect: [ clear: [email] ] }` is requested.
  - **Readings:** (a) DESIGN §8 and flow-format §4.13 refuse only a write of "a value that is not absent", so the request applies. (b) declaration-syntax §6.4 refuses any write of a personal attribute, and `clear` "is a write like any other", so the request is refused by `not_erased`.
- **Case 2:** an erased object's transition has a `deny` guard `g` that fails, and an effect `assign: { location: nickname, expr: "if flag then name else null" }` on a personal `nickname`.
  - **Readings:** (a) The guard is decided statically in step 4, among the generated guards, as "would write" a personal attribute. The refusal names `not_erased`, whatever the expression yields. (b) The guard is decided when the step writes. `g` refuses first, and once `g` holds the request applies whenever the expression yields absence.
- **Where the text fails:** DESIGN §8 has “a request whose outcome would write a value that is not absent to one of its personal attributes is refused by the generated guard `not_erased`”. flow-format §4.13 has “a request that writes a value that is not absent to one of its personal attributes is refused by the generated guard `not_erased`, which never refuses an erasure”. declaration-syntax §6.4 has “a request whose outcome writes one of its personal attributes is refused by the generated guard”. DESIGN §5.4 has “Clearing a value deliberately is the `clear` step, which writes absence and is a write like any other (ADR-0073).” DESIGN §6 step 4 has “Evaluate the parent transition's guards, the generated ones first in the order the format states”, which places the guard before the outcome, when no computed value is yet known.
- **Severity:** changes-outcome.
- **Suggested fix:** Align declaration-syntax §6.4 to "a value that is not absent". State that `not_erased` is decided in step 4, from the attribute inputs supplied and the effect steps that target a personal attribute, and say how a computed value that may be absent is treated.

#### 4. Group erasure: where the members' erasures run, in what order, and how a member's refusal is reported
- **Construct:** erasure of the group supersession joins; the exact `cascaded` sequence.
- **Case:** take `customers.examples.yaml` `erasing_follows_a_merge`, but give each customer one contact first (`add_contact` on both Dana and the duplicate). Contacts survive `merged_into`. Then request `dana.forget`.
- **The readings:** (a) The requested erasure runs with its effect first, then each other member with its effect, giving `cascaded: [Contact.forget, Customer.forget, Contact.forget]`. (b) The members are erased first, giving `[Customer.forget, Contact.forget, Contact.forget]`. (c) Each member's own erasure triggers the group erasure again, as "erasing any member erases the whole group", so Dana is erased a second time, as an "already erased" retry, which adds `Customer.forget` once more. An examples file can pass in one engine and refuse the publish in another. If a member's erasure has a `deny` guard that fails, the verdict is undetermined: `call refused` needs "the step that made it", and no step made a group member's erasure. The actor and the cause of a member's event are not stated either; §6 gives them only for cascaded transitions.
- **Where the text fails:** DESIGN §8 has “each through its own type's `erase`, receiving the requested erasure's inputs by name, since superseding declares that the successor continues the same thing (ADR-0146)”. declaration-syntax §6.4 has “erasing any member erases the whole group supersession joins, every object whose chain ends in the same survivor, each through its own type's `erase` receiving the requested erasure's inputs by name, in one request (ADR-0146)”. flow-format §11.4 has `cascaded`: “the exact sequence, in the order they ran, of the transitions the request caused on other objects, calls, creations, cascades and a group's erasures, each as `<Type>.<transition>` (ADR-0146)”. DESIGN §6 step 6 orders only “take each cascaded transition or creation in turn — the transition's effect first, each step in order and each call or creation as it is reached, then the cascades of its composite ends”. DESIGN §6 has “Every cascaded transition is recorded as the requesting actor's.”
- **Severity:** changes-outcome.
- **Suggested fix:** State when the other members' erasures run relative to the requested erasure's effect and in what order among themselves (for example ascending id). State that each member is erased once per request, and give the verdict and the event cause and actor for a member's erasure.

#### 5. Group erasure inputs: what "requiring the same inputs" means, and which erasure a member takes
- **Construct:** group erasure inputs.
- **Case:** `Customer.forget` takes `reason: string` and an optional `detail: string, personal`. The superseding type `Lead`'s erasure takes only `reason: OverrideReason` (an enumeration). A request `dana.forget(reason: asked, detail: "…")` reaches a `Lead` member.
- **The readings:** (a) "The same inputs" means the same required input names. The member receives `reason` only, `detail` being dropped because `Lead`'s erasure does not take it. (b) Every input is passed by name. `detail` is then `invalid input` on the member, which refuses the whole erasure, and so does `reason: asked`, which is not an `OverrideReason`. (c) The rule demands identical inputs, names, types and optional ones alike, so step 3 refuses this pair of types. A type that declares two `kind: erasure` transitions, which nothing forbids, gives no answer to which one the group runs. A member type whose erasure is `only_via` its whole's erasure is either `not requestable` from the group or run regardless.
- **Where the text fails:** flow-format §4.19 says “the types a supersession joins MUST declare erasures requiring the same inputs (step 3, `names`; the model's check 60; ADR-0146)”. DESIGN §8 has “receiving the requested erasure's inputs by name”. No passage limits a type to one erasure.
- **Severity:** changes-outcome.
- **Suggested fix:** Define "the same inputs" (names, types, optionality), say which inputs a member receives, require exactly one erasure on such types (or say which one is used), and say whether `only_via` applies to a group member's erasure.

#### 6. Must a type joined by supersession declare an erasure when it holds no personal data?
- **Construct:** supersession and erasure coverage, check 60.
- **Case:** a `Customer` (personal `name`) can be superseded by a `Ticket` that holds no personal attribute and takes no personal input, and that declares no erasure.
- **The readings:** (a) flow-format §4.19: `Ticket` MUST declare an erasure requiring the same inputs, so step 3 refuses the module. (b) The check 60 row, which §4.19 cites, and flow-format §4.13: a type with neither personal attributes nor personal inputs need not declare one, so the module is valid. Erasing a `Ticket` member of a group then either skips it or fails, which is not stated.
- **Where the text fails:** flow-format §4.19 says “the types a supersession joins MUST declare erasures requiring the same inputs (step 3, `names`; the model's check 60; ADR-0146)”. The declaration-syntax §10 row says “| 60 | A concrete type with a personal attribute, inherited ones included, or with a transition taking a personal input, a bound machine's included, that declares no `erase`, its own or its machine's, so every personal value can be erased (PRD D8, ADR-0133, ADR-0134); a machine is not held to it, its binders are; until ADR-0133 only a type taking part in supersession was held to it”. flow-format §4.13 says “an abstract base and a machine need not, a type that binds the machine must, and a type with neither need not”.
- **Severity:** changes-outcome.
- **Suggested fix:** Add the supersession requirement to the check 60 row (or to its own check) and say whether it binds types without personal data, or remove it from §4.19 and say what group erasure does with a member that has no erasure.

#### 7. Erasing again: does the effect run, are new group members and new proposals reached, and is an event recorded when nothing is retried?
- **Construct:** erasing again.
- **Case 1:** Dana is erased, and afterwards `add_contact(role: Sales, phone: "+65 …")` is requested on her. `not_erased` does not refuse it, since the phone is written to the Contact. Then `dana.forget` is requested again.
  - **Readings:** (a) "writes nothing else": the effect does not run, and the new contact keeps the phone. (b) The erasure runs its effect, and `Contact.forget` erases the new phone.
- **Case 2:** after Dana's group is erased, a new customer X is merged into Dana (`dana.merge_in(duplicate: X)`). X is now in the group. Dana is erased again.
  - **Readings:** (a) X is not erased, since the retry "writes nothing else". (b) X is erased, as a group member that is not yet erased.
- **Case 3:** there is nothing to retry. Readings: an `erased` event naming nothing is recorded (the version advances, so a caller holding the old version gets `stale`), or no event is recorded.
- **Where the text fails:** DESIGN §8 says “erasing an object already erased, directly or as a member of a group, records an `erased` event naming what it retried, such as a file deletion, and writes nothing else (ADR-0146)”. The group is “every object whose chain ends in the same survivor, so two duplicates merged into one are both reached, whichever member is named” (DESIGN §8), with no time fixed for its membership.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether a repeated erasure runs its effect and the erasure of group members not yet erased, whether it invalidates proposals filed since, and whether it records an event when nothing is retried.

#### 8. "Personal under any version": what is redacted, and must an erasure still exist after the marking is removed?
- **Construct:** erasure reaching values personal under any version.
- **Case:** in version 1, `Customer.nickname` is personal. Version 2 removes the marking, and Dana's events under version 2 write `nickname` again. Dana is erased under version 2.
- **The readings:** (a) The rule applies by attribute: `nickname` is redacted everywhere, the version 2 events and the current value included. (b) It applies by value: only values written by version 1 events are redacted, and the current value written under version 2 stays. Further, if version 2 removes the type's last personal attribute and drops its `forget`, check 60 (over the current declaration) accepts the publish, and the version 1 values can then never be erased. An engine that enforces "every personal value has an erasure" over history refuses that publish.
- **Where the text fails:** DESIGN §8 has “It reaches every value that was personal under any version the object's events were written under” and “publishing requires that `erase` of every concrete type holding a personal attribute or taking a personal input, so every personal value has an erasure that reaches it (check 60, ADR-0133, ADR-0134)”.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether the reach is per attribute or per written value, and whether check 60 counts attributes and inputs personal under an earlier version.

#### 9. A publish's redaction of an attribute it newly marks personal: its event, its source, and its admissions
- **Construct:** erasure reaching values personal under a later version; flow changes.
- **Case:** version 2 marks `Customer.phone_note` personal, Dana was erased under version 1 and holds a value in it, and an invariant `active_invariant2: state != ACTIVE or phone_note is not null` exists.
- **The readings:** for the event, (a) Dana gets a `redact` event with source `erased`, caused by the publish, or (b) the redaction is part of her `migrate` event with source `migrated`, or (c) she gets no event of her own. For the invariant, (a) it is admitted automatically, as an erasure's would be, or (b) it is an unadmitted violation, which refuses the publish unless the change carries an `admit`.
- **Where the text fails:** DESIGN §8 has “a publish that newly marks an attribute personal redacts it on objects already erased, as part of the publish”. flow-format §4.16 has “**A publish leaves no object in an invariant violation nobody admitted** (ADR-0146).” The source table lists “| `erased` | An erasure (§8), on the erased object and on every object whose personal value it redacted |” and “| `migrated` | A flow change's mapping (§5.9): F2's third route, with the publish as its cause |”, and the publish is neither.
- **Severity:** changes-outcome.
- **Suggested fix:** Give the event (transition name, source, cause) that this redaction records, and say whether it carries an erasure's automatic admission.

#### 10. Can a publish's backfill write a personal value into an erased object?
- **Construct:** "what was erased stays erased"; flow changes.
- **Case:** version 2 adds `Customer.preferred_name: string, optional, personal` with `backfill: { Customer: { preferred_name: '"(unknown)"' } }`, and Dana is already erased.
- **The readings:** (a) A migration runs only steps 5 to 8, so `not_erased` (step 4) does not apply, and Dana receives `"(unknown)"`. (b) As the import does, the migration skips an erased object's personal attributes, and Dana's value stays absent.
- **Where the text fails:** DESIGN §6 has “A **migration** a publish applies runs steps 5 to 8 once for each object”. flow-format §4.16 has `backfill`, “for each type, each attribute every object of it is given, and the expression that gives it”. Only the import has the exception: “an import skips its personal attributes, applying the rest of the row” (DESIGN §8).
- **Severity:** changes-value.
- **Suggested fix:** State whether a backfill writes a personal attribute of an object whose erasure is recorded.

#### 11. When an erasure redacts a caller's event, does the caller's object get an event and a new version?
- **Construct:** erasure reaching other objects' events.
- **Case:** object A's transition `t` takes a personal input `p` and passes it to `B.create(name := inputs.p)`, which writes B's personal `name`. B is erased, so A's event for `t` has `p` redacted (up the cause chain). A's current attributes are unchanged.
- **The readings:** (a) A's "value" was redacted, since its event changed, so A gets a `redact` event and its version advances. A caller holding A's old `expected_version` is then refused as `stale`, and a subscriber on A receives a `redact` event. (b) Only an object whose attribute is redacted gets an event. A is untouched, and the same request applies.
- **Where the text fails:** DESIGN §8 says erasure redacts in every caller's event “up the erased object's cause chain, the inputs that flowed into the erased object's personal attributes, which check 10's taint analysis identifies”, and “**Every object whose value an erasure redacts gets an event.** The event has source `erased`, is caused by the erasure's own event, and names the attributes redacted and never their values. The object's version advances, so a subscriber sees the change and a request carrying the old `expected_version` is refused as `stale` (ADR-0105).” declaration-syntax §6.4 has “An object whose personal value the taint analysis redacts receives an event with source `erased`”.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether redacting a payload in an object's past event (rather than an attribute it holds) gives that object an event and a new version.

#### 12. Erasure and proposals: which inputs are redacted, which proposals are invalidated, and how many events result
- **Construct:** erasure reaching proposals; invalidation.
- **Case:** a pending proposal to `complete` delivery D (no inputs), and a pending proposal to `rename` D with a personal input. D's type holds a personal attribute, and D is erased.
- **The readings:** for the inputs redacted, (a) all of a targeting proposal's inputs, or (b) only its personal ones (by marking or by flow). For invalidation, (a) both proposals are invalidated, since both target D, or (b) only the one whose inputs were redacted, the "affected" one. For events, the proposal gets one event (a redaction and invalidation together) or two.
- **Where the text fails:** DESIGN §8 says erasure reaches “the inputs of every proposal that targets the object or whose inputs flow into its personal attributes, invalidating the pending ones (ADR-0087)”, then “And it redacts every proposal targeting an erased object or carrying inputs that flow into one.”, and then “it redacts an affected proposal's inputs in the proposal's own event as well as its record;”.
- **Severity:** changes-value.
- **Suggested fix:** State which of a proposal's inputs an erasure redacts, whether invalidation follows from targeting alone, and the events the proposal receives.

#### 13. `successor_not_superseded` names "the successor", even when the successor is the object it is evaluated on
- **Construct:** which objects a refusal names; supersession.
- **Case:** `customers.examples.yaml` `a_customer_merged_into_itself_is_refused` (`dana.merge_in(duplicate: dana)`). The called `merged_into` is evaluated on Dana with `successor = dana`.
- **The readings:** (a) The generated-guard rule applies, and the refused call's verdict names `[dana]`. (b) The general rule's exclusion applies, and it names `[]`.
- **Where the text fails:** DESIGN §5.5 has “`successor_not_superseded` the successor”. The general rule in the same cell takes objects “other than the object it is evaluated on”. flow-format §4.19 has “**A successor given as an input is neither the object itself nor already superseded**”.
- **Severity:** changes-outcome.
- **Suggested fix:** Say whether the generated guards' named objects are subject to the general exclusion of the object evaluated on.

#### 14. May an assertion put an object into a superseding state?
- **Construct:** supersession, one successor; assertions.
- **Case:** `correct_state: { kind: assertion, to: [ACTIVE, MERGED], inputs: { successor: { reference: Customer }, … }, effect: [ supersede: inputs.successor ] }`.
- **The readings:** (a) Check 26 refuses it: when `to: ACTIVE` is chosen, the `supersede` runs outside a superseding to-state, and an assertion listing `MERGED` without a supersede step enters a superseding state without one. (b) It is valid, and the step runs only when the chosen `to` is superseding. (c) It is valid only when every listed state is superseding. In each reading a different set of descriptions publishes, and in (b) a request to `ACTIVE` sets no successor.
- **Where the text fails:** flow-format §4.19 has “a transition into it names the successor with a `supersede` step; a `supersede` step belongs only to such a transition”. Check 26 has “| 26 | A `supersede` outside a `superseding` to-state, a `superseding` state entered without one”. flow-format §4.13 says of an assertion “Its `effect` MAY write the object, as any transition's does”, and nothing excludes a superseding state from its `to`.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether an assertion's `to` may list a superseding state, and if so how its successor is given.

#### 15. `history(id, follow)`: the forward chain or the whole group?
- **Construct:** supersession reads.
- **Case:** a duplicate merged into Dana, then `history(dana, follow=true)`.
- **The readings:** (a) It follows the pointer forward, as `get` does. Dana is the survivor, so only her events are returned. (b) It returns the whole group's combined timeline, which adds the duplicate's events, as erasure's notion of a group would suggest. The interleaving order of a combined timeline (by position or by occurred time) is not stated either.
- **Where the text fails:** DESIGN §10 has “| **history**(id, follow?) | Events with provenance, legacy entries, and optionally the combined timeline through supersession |” against `get`, “optionally following supersession to the last member of its chain, the one not superseded”.
- **Severity:** changes-value.
- **Suggested fix:** State which objects `follow` includes in `history` and the order of the combined events.

#### 16. Filing a proposal: a missing required input, a transition that is not proposable, and a creation
- **Construct:** proposals, filing.
- **Case 1:** `propose: { object: dana, transition: add_contact, inputs: {} }` (with `add_contact` made proposable), which leaves out its required declared input `role`, `{ type: string }`, not optional. A request would be refused as `invalid input`, since `role` writes no attribute and so has no `<attribute>_provided` guard.
  - **Readings:** (a) Filing refuses it as `invalid input`, "as a request would". (b) Filing checks only that each supplied input is of the right type, and the omission is found at approval.
- **Case 2:** filing on `cancel`, which is not proposable, through the engine (step 3 catches it in an examples file, not in a live call).
  - **Readings:** the verdict could be `unknown transition`, `not requestable` or `invalid input`, with different remedies. No request verdict means "not proposable".
- **Case 3:** a creation proposal. The examples schema allows `propose` with `type`, but DESIGN requires "the object exists".
- **Where the text fails:** DESIGN §9 has “checks that the transition is proposable, the object exists and the inputs are of the transition's types, refusing as a request would; the object's state and the guards wait for approval (ADR-0146)”. flow-format §11.3 has “Filing checks the object and the inputs as a request would, refusing an input the transition does not take or one of the wrong type as `invalid input`” and “it takes a `request`'s `type` or `object`, `transition`, `inputs`, `actor` and `after`, and `proposal`”. A request's `invalid input` covers an input “or required and left out or null (other than a required input of an optional or defaulted attribute, which the generated `<attribute>_provided` guard refuses) with `invalid input`” (DESIGN §6 step 4). The examples schema's `propose` has “{ "required": ["type"], "not": { "required": ["object"] } },\n        { "required": ["object"], "not": { "required": ["type"] } }”.
- **Severity:** changes-outcome.
- **Suggested fix:** List the verdicts filing can return (missing required input, not proposable, unknown transition, unknown object, unknown actor), and say whether a creation may be proposed and what "the object exists" means for one.

#### 17. Approving after the transition has stopped being proposable
- **Construct:** proposal approval under the current declaration.
- **Case:** a pending proposal to `complete`, after which version 2 removes `proposable: true` from `complete` and changes nothing else. An approval follows.
- **The readings:** (a) The request executes, since `proposable` was checked at filing, guards are re-evaluated, and the marking is not a guard. (b) The approval is refused, since the transition no longer accepts a proposal. (c) The publish invalidates the proposal. The invalidation list does not include this case, so (c) needs a reading beyond the text.
- **Where the text fails:** DESIGN §9 has “Approving it executes the recorded request **as the approver**, with the approval as cause, and every guard is re-evaluated at execution under the **current** declaration.” and “A publish invalidates a pending proposal whose transition it removes, whose input it no longer takes, or one of whose input values its type no longer holds”.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether losing `proposable` invalidates pending proposals, refuses their approval, or neither.

#### 18. A refused approval's attempt row and an applied approval's result: under which transition and object?
- **Construct:** proposal approval; attempt counting.
- **Case:** `delivery.examples.yaml` `a_proposal_approved_too_soon_stays_pending` (refused by `signed_off`), and `an_agents_proposal_is_executed_when_approved` (applies).
- **The readings:** for the refusal, (a) the attempt row is of `Proposal`, object the proposal, transition `approve` (the requested transition) and clause `signed_off`, or (b) it is of `Delivery`, object the delivery, transition `complete`, with the proposal referenced. `<Type>.attempt_counts`, `diagnostics`' "clauses that refuse most" and per-transition refusal counts differ between them. For the applied approval, `Satisfied.object_id` is the delivery or the proposal, and `Satisfied.events` starts with the proposal's event (the "parent") or with the delivery's.
- **Where the text fails:** DESIGN §9 has “it is one attempt row naming the proposal, and the proposal gets no event, the verdict attached being that row, which a read of the proposal returns”. flow-format §4.8 has “A refusal is one row of the requested transition, carrying the clause its verdict names, so a refused call counts under the transition requested”. library-api §4 has “`Satisfied.events` is the positions of every event the request appended, the parent's first.”
- **Severity:** changes-value.
- **Suggested fix:** State the attempt row's type, object and transition for a refused approval, and the `Satisfied` object and event order for an applied one.

#### 19. What a proposal records of the request: occurred time, context, key
- **Construct:** proposals, filing and approval.
- **Case:** a proposal for a transition marked `backdating_limit: 2 days`, filed with `occurred_at = now - 1 day` and a `context`, and approved two days later.
- **The readings:** (a) The proposal holds only target, transition, inputs and proposer. The executed request has no occurred time (recorded time is used) and the approver's context, or none. (b) The proposal keeps the filer's occurred time and context. The occurred time is then three days before the recording, so `occurred_within` refuses the approval.
- **Where the text fails:** DESIGN §9 has “A **Proposal** holds a request filed for approval instead of executed: target, transition, inputs and proposer”. The library's `Request` carries `occurred_at`, `context` and `idempotency_key`, but no shape files a proposal (finding 21), and the examples schema's `propose` has no `occurred_at`.
- **Severity:** changes-value.
- **Suggested fix:** List every request field a proposal records and which of them the approval's request uses.

#### 20. An expired proposal: can it be rejected or withdrawn, and what state does the refusal name?
- **Construct:** proposal expiry.
- **Case:** a proposal filed with an expiry one day ahead. Two days later, someone `reject`s it, then someone approves it.
- **The readings:** (a) Expired means no longer pending. The `reject` is `unavailable` and the proposal stays `pending` in storage. (b) Only approval is barred. The `reject` applies, and the proposal becomes `rejected`. For the approval, `Unavailable.state` is `pending` (a state `approve` normally leaves, against "names the state it is in") or a derived `expired`. An approval exactly at the expiry instant is either "after" it or not. Whether an expired proposal counts as "pending" for a publish's invalidation and for the impact report's "the pending proposals invalidated;" is not stated.
- **Where the text fails:** DESIGN §9 has “A decision on a proposal no longer pending is refused as `unavailable`, as a transition from a state it does not leave is (§5.5), and so is an approval after the proposal's expiry, which a read reports; expiry is derived, not driven, and a proposal filed with none does not expire.” DESIGN §5.5 has “| `unavailable` | The object is not in a state the transition may be taken in; names the state it is in (ADR-0122) |”.
- **Severity:** changes-outcome.
- **Suggested fix:** State which decisions an expired proposal refuses, the state its refusal names, the boundary instant, and whether expiry counts as no longer pending elsewhere.

#### 21. Built-in types' transitions and events are unnamed, and the API cannot file a proposal or create a subscription
- **Construct:** event transition names; proposals; subscriptions; the DeclarationChange lifecycle.
- **Case:** a subscription filtered on `Proposal` events by transition name; a proposal's inputs redacted by an erasure; a proposal invalidated by a publish; a change moved to `SUPERSEDED` by another change's publish.
- **The readings:**
  - Filing, approval, execution and invalidation of a Proposal, and the creation and revocation of a Subscription, have no stated transition names. Each builder chooses (`file`/`propose`, `approve`/`execute`, `invalidate`, `create`/`subscribe`, `revoke`), and a filter by name matches in one engine and not in another.
  - A proposal's redaction is both “an erasure's redaction of another object, `redact`” and “an event of a built-in type, its own transition”, so one engine records `redact` and another a proposal transition.
  - A change superseded by a publish gets an event whose name (`supersede`? `publish`?), actor and cause are unstated.
  - The library offers no operation or `Request` shape for filing, approving, rejecting or withdrawing a proposal, or for creating or revoking a subscription. Its nineteen operations do not include them, and DESIGN says "Any caller may file one".
- **Where the text fails:** DESIGN §7 has “a request's event names the transition taken; a migration's, `migrate`; an import's or a refresh's, `import`; an erasure's redaction of another object, `redact`; and an event of a built-in type, its own transition.” DESIGN §9 gives only “with a lifecycle `pending → executed | rejected | withdrawn | invalidated`”. DESIGN §7 gives “a lifecycle of `active → revoked`.” DESIGN §9 has “moving each to `SUPERSEDED`, a plain final state naming the publishing change and not supersession of §8” and “it redacts an affected proposal's inputs in the proposal's own event as well as its record;”. library-api §6 has “Nineteen operations: the sixteen of §10, the write path of §6, and the operational calls that are not object operations, `acknowledge` and `publish`.” DESIGN §9 has “Any caller may file one on a transition marked `proposable`”.
- **Severity:** changes-value (and, for the API, cannot be implemented as written).
- **Suggested fix:** Declare `Proposal` and `Subscription` as DeclarationChange is declared, with named transitions, inputs and states. Name the event of a publish's supersession and of an erasure's invalidation, and give the `Request` shapes (or operations) that file and decide a proposal and create and revoke a subscription.

#### 22. Which open changes a publish supersedes: "older" than what?
- **Construct:** the DeclarationChange lifecycle.
- **Case:** version 3 is installed. Changes A and B are both drafted against version 3, and A is published, installing version 4.
- **The readings:** (a) "Older" means older than the version the publish installs. B (drafted against 3) is moved to `SUPERSEDED` in A's transaction, and a later `publish` of B is `unavailable`. (b) "Older" means older than the version A was drafted against. B stays `SUBMITTED`, a later `publish` of B is `Unsatisfied` naming `current`, and B can still be `revise`d. Reading (a) leaves `current` with nothing to refuse, which is what makes (b) plausible.
- **Where the text fails:** DESIGN §9 has “`reject` and `withdraw` end it, and a publish **supersedes** every other open change” followed by “every other open change drafted against an older version, in the same transaction, moving each to `SUPERSEDED`”, and “It requires a change drafted against the installed version and a recomputed report naming nothing the attached one did not.” library-api §6 has “The result carries the verdict — `Satisfied`, `Stale`, or `Unsatisfied` naming `current` or `impact_unchanged` — beside the report”.
- **Severity:** changes-outcome.
- **Suggested fix:** Say which version "older" is measured against.

#### 23. The DeclarationChange lifecycle as declared: `revise` has two kinds, and `submit`'s refusal names no clause
- **Construct:** the DeclarationChange lifecycle.
- **Case:** `revise` on a `DRAFTED` change and on a `SUBMITTED` one; a `submit` whose dry run finds a fatal finding; a `publish` refused by `impact_unchanged`; a `refresh` whose rerun finds something fatal.
- **The readings:**
  - `revise` is internal at `DRAFTED` and external from `SUBMITTED`. The format gives each transition one kind, and "no transition leaves a state for itself". Builders either split it into two transitions, which cannot share a name, or make one transition that is external from `DRAFTED` to `DRAFTED`. The latter closes and reopens the `DRAFTED` interval, which changes time-in-state.
  - `submit`'s refusal has no clause name or remedy, and `current` and `impact_unchanged` have no remedy. Each builder chooses, and the remedy is carried by every refusal.
  - Whether `refresh` is refused when its dry run finds something fatal is not stated.
- **Where the text fails:** DESIGN §9 has “`revise` keeps a draft a draft and returns a submitted change to `DRAFTED`; `submit` leaves `DRAFTED`, and is refused while the dry run finds anything fatal; `refresh` stays `SUBMITTED`” and “It is a built-in type with a declared lifecycle, printed in the rule set like any type”. flow-format §4.8 has “No transition leaves a state for itself: an external transition always changes the state, and a change that keeps it is an internal transition”. flow-format §4.7 has “The declared class is carried as written by every refusal the condition makes”.
- **Severity:** changes-outcome.
- **Suggested fix:** Write the lifecycle out as a flow description would declare it, with each transition's kind, from and to states, guard names and remedies.

#### 24. A revoked subscription's pull "is refused", but no refusal can be expressed
- **Construct:** delivery, `pull` and `acknowledge`.
- **Case:** `pull(actor, sub)` after `sub` is revoked; `acknowledge(actor, sub, c)` after revocation.
- **The readings:** (a) An exception is raised, but the closed list of five has none for this. (b) A refusal value is returned, but `EventPage` has no verdict field, and `pull` returns only `EventPage`. (c) An empty page is returned. For `acknowledge` on a revoked subscription, nothing is said, and it returns `None`. "Revived" lets a builder read `acknowledge` as un-revoking a subscription, although progress emits no events and so cannot change its lifecycle state.
- **Where the text fails:** DESIGN §7 has “a revoked subscription's pull is refused (ADR-0146).” library-api §2 has “**A refusal is a value, never an exception.**” library-api §7 has “These are the five things that raise, and the list is closed so that a second binding cannot differ on it.” `EventPage` has only “    events: Sequence[Event]\n    settled: str”, and `pull` is “    def pull(self, actor: Actor, subscription: str,\n             cursor: str | None = None) -> EventPage: ...”. DESIGN §7 has “`acknowledge` may move the cursor back as well as forward, which is how a subscription is revived at any cursor, since nothing is pruned (ADR-0034, ADR-0043)”, and library-api §6 has “# back as well as forward: moving it back revives the subscription there (ADR-0146)”. DESIGN §7 has “Delivery progress is **runtime state, not object state**: the acknowledged cursor is stored beside the idempotency record, is not versioned and emits no events”.
- **Severity:** changes-outcome (cannot be implemented as written).
- **Suggested fix:** Give `pull` (and `acknowledge`) a refusal shape, such as a verdict field or a result union, for a revoked or unknown subscription. Replace "revived" with what moving the cursor does.

#### 25. Where a pull starts with no cursor and no acknowledgement, and whether the cursor is inclusive
- **Construct:** delivery, cursors.
- **Case:** a subscription created on a store holding a year of events, never acknowledged, and pulled with no cursor. Then a pull from the returned cursor, and a pull whose filter selects nothing in the scanned range.
- **The readings:** (a) It starts at the beginning of the log. The first pull returns a year of events, and lag and death are past their thresholds at creation. (b) It starts at the subscription's creation, and returns nothing old. A pull from a cursor returns events after it (exclusive) or from it (inclusive, the last event delivered again). For an empty page, the settled cursor is the one given or advances past events the filter skipped.
- **Where the text fails:** DESIGN §7 has “`pull` reads from the cursor it is given, and without one from the acknowledged cursor” and “in (transaction, position) order, and hands back the last pair as the cursor.”
- **Severity:** changes-value.
- **Suggested fix:** State the initial acknowledged cursor of a new subscription, whether a cursor is exclusive, and what a page that returns no event hands back.

#### 26. Lag and death: no read returns them, and their edge cases are unstated
- **Construct:** delivery, lag.
- **Case:** a subscription with thresholds 1 h and 1 day and no event pending; one whose oldest pending event is exactly 1 h old.
- **The readings:** nothing in §10 or the library returns lag or death, so each builder adds a field, a derived attribute or nothing. With nothing pending, lag is either zero or absent. At exactly a threshold, the subscription is lagging or not. The two thresholds are presumably "lagging" and "dead", but their names and which is which are not stated.
- **Where the text fails:** DESIGN §7 has “the lag is the age, by recorded time, of the oldest event the filter selects after the acknowledged cursor, and its two thresholds are durations (ADR-0100)”, “Death is for alerting and stops no pull.” and a subscription holds “lag thresholds;”.
- **Severity:** changes-value.
- **Suggested fix:** Name the two thresholds, give the comparison and the no-pending value, and add lag and death to a read (for example `get` of a subscription).

#### 27. Uniqueness partial over absence: an absent `with` attribute or scope
- **Construct:** uniqueness, partial over absence.
- **Case:** `code: { type: string, unique: { with: [region] } }` with `region` optional. Two objects have `code = "A"` and `region` absent.
- **The readings:** (a) The form is partial only where `code` is absent. The two objects collide (absent equals absent, as a unique index with nulls not distinct treats them), and the second is refused with `code_unique`. (b) The tuple is partial where any member is absent (`o.region == this.region` is unknown, so the scan's `where` does not select, and SQL's default unique index treats nulls as distinct). Both apply. The same question arises for `unique: in_scope` when the scope attribute is absent.
- **Where the text fails:** flow-format §4.12 has “a type scan over the other objects, every form partial over absent values, so any number of objects may lack one (ADR-0146)”, “| `unique: { with: [a, …] }` | together with the attributes listed, which the type MUST declare (step 3, `names`) |”, “| `unique: in_scope` | within one value of the identifier's `scope`, which the attribute MUST have (step 3, `names`) |” and “Each is enforced by a unique index on every backend”.
- **Severity:** changes-outcome.
- **Suggested fix:** State whether an absent `with` attribute or scope makes the form partial, and that the unique index matches.

#### 28. The order of a type's invariants in a refusal: inherited `unique`, machine-required invariants, and "by attribute"
- **Construct:** the order of a type's invariants in a refusal.
- **Case:** the abstract `Asset` declares `code: { type: string, unique: true }`. Its subtype `Phone` declares its own `serial: { type: string, unique: true }`, and its creation takes both as inputs. A creation supplies a `code` that one existing laptop holds and a `serial` that one existing phone holds, so `code_unique` and `serial_unique` both fail on the new phone.
- **The readings:** (a) An inherited `unique` is one of the "inherited ones in the base's order", giving `[code_unique, serial_unique]`. (b) It is one of "the generated", placed "by attribute", giving `[code_unique, serial_unique]` if inherited attributes come first and `[serial_unique, code_unique]` if the subtype's own come first. The first named invariant fixes the refusal's remedy and the examples' `clause`. If `Phone` also declared its own states with `required_attributes`, its `<state>_invariant`s would come after `code_unique` under (a) and before it under (b). Similarly, an invariant a machine `requires` is declared by the binder, so it is either "its own in the order written" or "a machine's" (in the order of `requires`). Whether every `<state>_invariant` precedes every `<attribute>_unique` is implied by the word order only.
- **Where the text fails:** DESIGN §5.5 has “in the order its type declares them — its own in the order written, then inherited ones in the base's order, then a machine's, then the generated, each `<state>_invariant` by state and each `<attribute>_unique` by attribute (ADR-0146)”, and flow-format §4.8 has “its declared invariants in the order written, then inherited ones in the base's order, then a machine's, then the generated, each `<state>_invariant` by state and each `<attribute>_unique` by attribute; ADR-0146”. flow-format §4.10 has “A machine's states MUST NOT declare `required_attributes`, since the invariant that generates belongs to each binder, which declares it instead and names it under `requires`”. flow-format §4.14 has “a `unique` declared on the base spanning the whole family, since the type scan it expands to ranges over every subtype (ADR-0146)”. DESIGN §5.5 has “The refusal's is the first named invariant's (ADR-0122)”.
- **Severity:** changes-outcome.
- **Suggested fix:** Place generated invariants a base declares, and invariants a machine requires, explicitly in the order, and state the attribute order "by attribute" uses for a subtype.

#### 29. Named objects: a collection inside an aggregate that the aggregate does not bind
- **Construct:** which objects a refusal names (nested aggregates).
- **Case:** `delivery.examples.yaml` `ticking_after_the_sign_off_makes_it_stale`. `signed_off` fails, and its `where` reads `changed_since([price, checklist_items], a.at_event)`, where `checklist_items` evaluates to `battery` and `manual`.
- **The readings:** (a) `checklist_items` is a reference evaluated "in full", so the refusal names `[battery, manual]` (in id order). (b) Only the outermost aggregate's `where` selection counts, which is none, so it names `[]`. `changed_since` is not an aggregate, so the "nested aggregate adding none" clause does not settle it.
- **Where the text fails:** DESIGN §5.5 has “The objects are every object a reference in the failing clause evaluates to when the clause is evaluated in full, every operand of `and`, `or` and `implies` and the derived attributes it reads included, whether the clause reads a member of the object or only compares the reference, other than the object it is evaluated on; where it evaluates a collection, the elements the outermost aggregate's `where` selects, or every element where it has none, so an empty collection contributes none, a nested aggregate adding none and a name an aggregate binds being no reference; in ascending id order.” The clause is “any(a in approvals where a.state == Approval.GIVEN\n              and not changed_since([price, checklist_items], a.at_event))” (`delivery.yaml`).
- **Severity:** changes-outcome.
- **Suggested fix:** State whether references inside an aggregate's `where` or body, other than the bound name, contribute objects, and how a composite end named in `changed_since` counts.

#### 30. `cascaded`: observation forgets, redactions, invalidations, and what "the order they ran" means
- **Construct:** the exact `cascaded` sequence.
- **Case:** a subject with two observations, one with a personal field and one without, and a recorded label, is erased. The erasure also taint-redacts another object's attribute and invalidates a pending proposal targeting it.
- **The readings:**
  - Each observation's generated `forget` is listed, as a transition caused on another object, or it is not, since it is run "without a declared step". Its position (before or after the erase's declared steps) is unstated, and so is whether an observation with no personal field gets a `forget` at all.
  - A label's `forget` cannot be written: `label` is lower case, and the item pattern requires `<Type>`.
  - The `redact` event and the proposal's invalidation are either "transitions the request caused on other objects" or not.
  - For a call inside a called transition's effect, "ran" means start order (the event position is allocated when the transition begins) or completion order.
- **Where the text fails:** flow-format §11.4 has “the exact sequence, in the order they ran, of the transitions the request caused on other objects, calls, creations, cascades and a group's erasures, each as `<Type>.<transition>` (ADR-0146)”. DESIGN §8 has “It reaches observations through each one's generated `forget`, which the subject's `erase` runs without a declared step”. DESIGN §5.11 has “each kind has a generated `forget`, which the subject's `erase` runs and which may be requested on one observation alone”.
- **Severity:** changes-outcome.
- **Suggested fix:** List exactly which caused events `cascaded` includes (observation forgets, `redact`, invalidations) and where each falls in the sequence.

#### 31. DESIGN §6 applies the parent's writes "in full" before its calls, which breaks `result` and reorders values
- **Construct:** names an effect binds (`result`); the order of an effect.
- **Case:** effect `[ call: { target: unit, transition: t, inputs: { v: price } }, assign: { location: price, expr: "0" } ]`. Also any `supersede: r` after `create: { …, result: r }`, as flow-format §4.19 allows.
- **The readings:** (a) Steps run in effect order. `t` receives the old price, and `r` is bound before `supersede`. (b) As DESIGN §6 reads, step 5 applies the parent's new state and all its writes first, and step 6 then takes its calls and creations. `t` receives `0`, and `supersede: r` (a write) runs before the `create` that binds `r`, which cannot be done.
- **Where the text fails:** DESIGN §6 has “5. Apply the parent's outcome in full: its new state and its attribute writes, each value read at the moment it is applied (ADR-0054).” followed by step 6's “take each cascaded transition or creation in turn — the transition's effect first, each step in order and each call or creation as it is reached, then the cascades of its composite ends”. flow-format §4.8 has “`result` names the new object for the steps after it, within the loop that binds it where a loop does”, and §4.19 has “The successor is a required input that names an object, or a name a `create` step earlier at the top level of the effect binds”.
- **Severity:** changes-value (and a contradiction).
- **Suggested fix:** Restate steps 5 and 6 so that the parent's effect runs in step order, writes, calls and creations interleaved, followed by the cascades.

#### 32. Names an effect binds may collide with attributes and with the words every object has
- **Construct:** names an effect binds (`result`, `item`) and members.
- **Case:** in `Delivery`, `foreach: { item: courier, array: …, steps: [ assign: { location: x, expr: courier } ] }`, or `create: { …, result: state }`, or `item: open`.
- **The readings:** (a) The bound name shadows the attribute or the built-in (`state`, `open`, `id`, `now`) inside its scope. (b) The attribute or built-in wins, since "an expression resolves those words before any attribute". (c) Step 3 refuses the collision. Each reading writes a different value, or refuses the description. Nested loops reusing one `item` name raise the same question.
- **Where the text fails:** flow-format §5 has “and in an effect, also the name a `foreach` or `create` step binds”. The reservations cover attributes and members only: “an attribute, including an observation's, MUST NOT be named `state`, `inputs`, `actor`, `this`, `now`, `referrers` or `this_event`, since an expression resolves those words before any attribute” and “a member MUST NOT be named `id`, `open`, `created_at`, `created_by_kind`, `recorded_from` or `declaration_version`, which every object has” (flow-format §3).
- **Severity:** changes-value.
- **Suggested fix:** State the scope and precedence of names a `foreach` or `create` binds, and reserve the same words for them, or refuse a collision.

#### 33. A loop over values has no ids to order by
- **Construct:** loops, `array` over a value set, and `range`.
- **Case:** `delivery.yaml` `Delivery.open` with `checklist: [Manual included, Battery charged]` (a `string[]` input).
- **The readings:** (a) Elements run in the order supplied: "Manual included" is created first, with position `DLV-000001-01` and the lower id. (b) Elements run in value order, so "Battery charged" is created first. (c) The order is unspecified, since a set has none. The examples use the one order where (a) and (b) agree. For `range`, "ascending id order" does not apply either, and ascending numeric order is only inferred.
- **Where the text fails:** flow-format §4.8 has “The collection and `where` are evaluated once, when the loop starts, and the loop runs over that snapshot in ascending id order even where its steps change the collection (ADR-0122).” DESIGN §6 has “over collection elements in ascending object-id order, which is creation order (§5.1)”. `delivery.yaml` has “checklist: { type: "string[]", description: The labels of the checklist items to create. }”. flow-format §4.8 has “or, with `range: <expression>` in place of `array`, for each number from 1 to that value, an `int`, one that is absent or not positive running no step”.
- **Severity:** changes-value.
- **Suggested fix:** State the iteration order over a collection of values (and over `range`), or say that a set input keeps the order the caller supplied.

#### 34. Abstract bases and `summary`: what a family query returns by default, and whether an abstract type may extend one
- **Construct:** abstract bases (no `summary`).
- **Case:** `query(Asset)` with no `fields`, where `servicedesk.yaml`'s `Laptop` and `Server` declare no summary either. Separately, an abstract `Device` that `extends: Asset`.
- **The readings:** for the query, the default `fields` are every attribute, `state` alone, each object's own type's summary, or the base's members only. A type with no `summary` has no stated default at all. For `Device`, (a) it is valid: the schema allows `extends` on an abstract type, and check 43's "come back to it" implies chains. (b) It is invalid, since §4.14 lists what an abstract type "may declare", and `extends` is not in that list.
- **Where the text fails:** flow-format §4.14 has “and no `summary`, which each subtype declares for itself” and “It declares `description`, and may declare `tracking`, `attributes`, `derived_attributes` and `invariants`”. DESIGN §10 has “| **query**(type or family, filter, order, cursor, fields) |” and “`fields` defaults to the type's summary set”. flow-format §4.14 has “following a type's bases MUST NOT come back to it”.
- **Severity:** changes-value.
- **Suggested fix:** State the default `fields` for a type with no summary and for a family query, and say whether an abstract type may extend another.

#### 35. Examples: an invalidated proposal cannot be expected, and a refused approval in a setup is undecided
- **Construct:** proposals in examples (§11).
- **Case:** a setup files a proposal on a proposable transition of a customer, then erases the customer (which invalidates it), and the example approves it, expecting `{ verdict: unavailable, proposal: invalidated }`. Separately, a setup step `approve` whose request is refused.
- **The readings:** (a) The text allows "the state it was in", so the expectation is valid. (b) The schema's enum lacks `invalidated`, so step 2 refuses the file. For the setup, (a) the decision itself applied, the proposal staying pending, so the setup holds. (b) The step "did not apply", so every example given the setup fails.
- **Where the text fails:** examples.schema.json has “"enum": ["pending", "executed", "rejected", "withdrawn"]”. flow-format §11.4 has “and the state it was in where it is refused as `unavailable`”. flow-format §11.2 has “Every step of a setup MUST apply.”, and §11.3 has “`approve` executes the proposal's request as the approver, every guard evaluated as it stands then; where it applies the proposal is `executed`, and where it is refused the proposal stays `pending`, carrying the verdict.”
- **Severity:** changes-outcome.
- **Suggested fix:** Add `invalidated` to the enum (and a way to file with an expiry, if expiry is to be testable), and state whether a refused approval step fails a setup.

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 22 | 2, 3, 4, 5, 6, 7, 8, 9, 11, 13, 14, 16, 17, 20, 22, 23, 24, 27, 28, 29, 30, 35 |
| changes-value | 13 | 1, 10, 12, 15, 18, 19, 21, 25, 26, 31, 32, 33, 34 |
| wording | 0 | — |
| **total** | **35** | |

Outright contradictions between passages: 1 (DESIGN §5.6 against §8), 3 (declaration-syntax §6.4 against DESIGN §8 and flow-format §4.13), 6 (flow-format §4.19 against the check 60 row), 31 (DESIGN §6 steps 5–6 against flow-format §4.8 and §4.19), and 35 (examples schema against flow-format §11.4). Passages that cannot be implemented as written: 21 (no API shape for proposals or subscriptions), 24 (a refused pull has no expressible form) and 23 (`revise` as one transition of two kinds).

## 3. Publishing with a migration, the import, and the harness's conditions, as the reader wrote it

**What I read.** `docs/design/publish-and-import.md` whole. `docs/design/flow-format.md` §4.15, §4.16, §5 and §7 with §7.1; the same read also showed §4.17 to §4.19 and §6, which I did not need. `docs/DESIGN.md` §5.9, §5.12, §6, §7, §11 and §14. I read the whole of §5.12, though only "What the standard metrics count" was assigned. `docs/design/declaration-syntax.md` §6.6, all of §6.9 (more than the row tables and member bullets), and the §10 introduction with the check rows 1 to 9, 22 and 23. `docs/design/storage-schema.md` §3 (with §3.1 to §3.4), §6, §9, §10 and §12; the same reads also showed the start of §7 and §11. `docs/design/adversarial-harness.md`: the whole file, though only §1 was assigned. `docs/design/library-api.md`: all of §6, which holds `import_batch` and `publish`. The examples `service.yaml`, `service-v2.yaml`, `inventory.yaml`, `inventory-v2.yaml` (read as a diff against v1) and `people.yaml`.

**Where I left the assigned sections**, each by following a citation the text needs:
- DESIGN §5.5: the remedy table and "three-way" availability. Step 3's "as `availability` does" led there.
- DESIGN §9 (Proposal, DeclarationChange, the impact report's list), cited by §5.9.
- DESIGN §10 (the read surface), for `availability`.
- flow-format §4.3: composition and `survives`, to read "strand" in publish-and-import §3.
- declaration-syntax §2: the paragraphs on families and `mirror`, cited by flow-format §4.15.
- declaration-syntax §6.8: the generated recording guards, cited by DESIGN §6 step 4 (`subject_owned`).
- declaration-syntax §6.11: the standard metrics table and the paragraph after it, cited by DESIGN §5.12.
- storage-schema §8, cited by publish-and-import §3.
- Searches showed short snippets elsewhere in these files. I used them only to find passages, and cite none here except the following, read in place: DESIGN §5.2's sentence on what is tracked, declaration-syntax §2's sentence on abstract types, and check 32's row.

**What I did not read:** anything under `docs/adr/`, `defects.md`, `TODO.md`, `docs/LESSONS.md`, `cold-reading-review.md`, `reimbursement-cold-draft.md`, any `*review*.md`, the case studies, `scripts/`, `.remember/`, and git history. I modified no file in the repository.

**Quotations.** A Python script checked every quotation below, character for character, against the file it names: each span in curly quotation marks is asserted to occur in that file, and each short phrase in straight quotation marks to occur in the sources. The only exceptions are the label "fleet" and the example literal `'"NONE"'`, which are mine. The script also checked three absence claims by parsing the files: `PublishReport` declares no `now`, `ViolationKind` has only `INVARIANT` and `GUARD`, and `of_migration` has no ordering column. Everything else not in quotation marks is my reading, and inference is marked as such.

**A made-up pair used below ("fleet").** Version 1 has these types:
- `RobotModel`: states DRAFT, ACTIVE, and RETIRED (final).
- `Robot`: states AVAILABLE, LEGACY_HOLD, and RETIRED (final). It holds a required `model` reference to RobotModel, `retirement_reason: RetirementReason` (optional), `nickname` (optional string) and `defects: Defect[]`.
- `Delivery`: states OPEN, ON_HOLD, DONE (final) and CANCELLED (final), with a composite part `ChecklistItem` (OPEN, CHECKED final).
- The mirror `Customer`: states ACTIVE, DORMANT and LEFT (final), with the invariant `has_contact`.

Its objects, in ascending id order:
- R1: Robot, AVAILABLE, model M2.
- M2: RobotModel, DRAFT.
- R3: Robot, LEGACY_HOLD.
- R4: Robot, RETIRED, reason OBSOLETE.
- R5: Robot, RETIRED, reason absent.
- D6: Delivery, ON_HOLD, with part K7 OPEN.
- C8: Customer, DORMANT, imported, with a standing import `ADMIT` of `has_contact`.

Each finding says what its version 2 changes.

### What is determined

- **The publish transaction.** A publish installs a version or nothing. It is the `publish` transition of a `DeclarationChange`, drafted against the installed version. The approver names the change's version, and a `refresh` since then gives `stale`. Steps 1 to 4 run again at the attached report's `now`. Objects or proposals newly named in an impact category refuse it as `impact_unchanged`, and an object that left a category does not. A publish supersedes other open changes and invalidates the proposals it breaks, and the publish event is the cause of both.
- **The migration.** Each object gets one event, with the transition `migrate`, provenance `migrated`, the publish event as cause and the publishing actor as actor. Objects are taken in ascending id order, which reads as global across types. Within an object the mappings run renames, then removed members, removed states and backfills in the order written. Each reads what the mappings before it left and names this version's members. An object that only an `admit` reaches gets a `migrated` event carrying only the admission. An `admit`'s reason is an enum member. A publish's admissions are overrides, listed by `exceptions` and counted by `override_counts`; §6.11's filter `t.overrides and not t.imported` counts them.
- **`state_source`** is never changed by a migration. **`declaration_version`** moves with a migration event, and an object no event reaches keeps its own.
- **For publishing, "live" means every object of the type, final ones included, observations excepted.**
- **Mappings.** A removed attribute needs nothing. A rename keeps its type and optionality and carries its history, and never names an observation kind's field. A mapping into a superseding state is refused, and a mapping into a final state drives no cascade.
- **Proposals.** The list of proposals a publish invalidates is exhaustive (transition removed, input no longer taken, input value no longer held), and no mapping rewrites a proposal's inputs. A pending proposal that now lacks a newly required input stays pending.
- **The severities in publish-and-import §2**, for what they list.
- **The standard metrics: §6.11 settles what DESIGN §5.12 leaves loose.** `time_in_state` reads every interval: legacy ones, those the port opened, and those a migration opened. Throughput, rework, transition counts and cycle time leave out imported and migrated events.
- **The import.**
  - It writes one transaction per batch, and a refusal refuses the whole batch.
  - It writes only mirrors, and the observations and labels on mirror objects, through the built-in assertion.
  - Its events are marked `imported` with provenance `asserted`, and it sets `state_source` to `imported`.
  - It skips the personal attributes of an erased object and applies the rest of the row.
  - The legacy key is kept under source `legacy`, and every id is assigned before any row is written.
  - A cycle of required references commits with its foreign keys checked at commit.
  - A sequence is raised past the legacy value and never lowered.
  - An object whose creation time the mapping does not supply is undated, created by kind `unknown`.
  - A legacy entry keeps the payload fields its kind lists.
- **The disposition file.** A class is identified by (type, rule, shape). `ADMIT` and `EXCLUDE` need a reason. A guard class cannot be admitted. No import runs while a class is `UNDECIDED`. An object in a `CLEAN_UPSTREAM` class refuses the batch, and an object newly in an `ADMIT` class is admitted.

**service v1 → v2, object by object.** Take J1 (OPEN), J2 (WORKING), J3 (DONE, with a photo) and J4 (DONE, no photo).
- DDL adds `site` as a nullable column and renames `photo` to `completion_photo`.
- J1 to J4 each get one `migrate` event writing `site = "HQ"`.
- J4's event also records an admission of `photo_when_done` with reason `MigrationReason.LEGACY_DATA`, reported as a notice because J4 is final. That invariant is not compiled.
- No object holds `ReassignmentReason.SKILLS`, since `reason` is only an input of `reassign`. That mapping therefore reaches no object, and no transition is `proposable`, so no proposal is invalidated.
- `InspectionCheck` is unchanged.
- The text decides all of this, except whether the rename is on those events (finding 1) and what the `finish` change does to the impact categories (findings 9 and 10).

**The units pair of the appendix.** Take R1 to R5 as in "fleet", minus Delivery.
- R3 (LEGACY_HOLD) moves to AVAILABLE. Its state interval closes at its migration, and an AVAILABLE span with `migrated = 1` opens.
- R4's reason OBSOLETE becomes FAILED. Its `retirement_reason` interval is split in the same way.
- Every robot is backfilled with `"UNKNOWN"`.
- R5 receives an admission of `retired_with_reason`, a notice because it is final.
- The CHECKs on state and on `retirement_reason` are replaced.
- The text decides all of this, except the rename (finding 1) and the effect on rework of mapping LEGACY_HOLD (finding 35).

### Findings

#### 1. A rename: DDL with no event, or a mapping that the migration event carries

- **Construct:** renames within the one migration transition per object; `declaration_version`; the impact category "which each mapping will rewrite".
- **Case:** fleet v3 whose only mapping is `renamed_attributes: Robot: { nickname: display_name }`. Robot R9 is created between `submit` and approval.
- **The readings:**
  - (a) As publish-and-import §3 says, the rename is DDL alone. No Robot gets an event, every Robot keeps `declaration_version` 2, and no object is named under the rename. R9 changes no category, so the publish installs.
  - (b) As step 5, declaration-syntax §6.6 and DESIGN §5.9 say, the rename is a mapping that reaches each Robot. Each gets a `migrate` event, its version advances and its `declaration_version` becomes 3, and each is named under the rename. R9 is newly named, so the publish is refused as `impact_unchanged`.
- **Where the text fails:**
  - publish-and-import.md §3: “A renamed attribute is DDL and no events: the values did not change, only the name they are read under”
  - publish-and-import.md §1 step 5: “carrying every mapping that reaches the object in this order — renames, removed enum members, removed states, then backfills in the order written”
  - declaration-syntax.md §6.6: “A migration is one transition per object, carrying every mapping that reaches it, objects taken in ascending id order, the mappings applied renames first”
  - flow-format.md §5: “`declaration_version`, the version of its latest event, so a migration moves it and an object no publish writes keeps its own”
  - publish-and-import.md §1 step 3: “Which are in a removed state, which each mapping will rewrite”
- **Severity:** changes-outcome
- **Suggested fix:** State whether a rename reaches an object, writing an event and naming it in an impact category. If it does not, drop "renames" from the list of what the migration event carries.

#### 2. A backfill that leaves a value as it was

- **Construct:** whether a backfill "reaches" or "rewrites" an object whose value it does not change.
- **Case:** fleet v2 backfills `Robot: { retirement_reason: 'if retirement_reason is null and state == RETIRED then RetirementReason.LOST else retirement_reason' }`. It changes only R5; R1, R3 and R4 keep their values. R9 is created, AVAILABLE, after `submit`.
- **The readings:**
  - (a) The backfill writes every Robot. R1, R3, R4 and R5 each get a `migrate` event and are named under the backfill. R9 is newly named, so the publish is refused.
  - (b) Only a row that changes is reached or rewritten. Only R5 gets an event, R9 is not named, and the publish installs.
  - The words "writes every object" support (a). The words "rewrite" and "reaches", and the rule that no row changes without an event, invite (b).
- **Where the text fails:**
  - publish-and-import.md §3: “A `backfill` writes every object of its type; one that fills only an absent value says so, `if x is null then … else x`.”
  - publish-and-import.md §3: “so a publish never changes a row without an event (ADR-0099)”
  - publish-and-import.md §1 step 3: “Which are in a removed state, which each mapping will rewrite”
  - declaration-syntax.md §6.6: “a backfill filling only an absent value saying so, `if x is null then … else x`”
- **Severity:** changes-outcome
- **Suggested fix:** State whether a backfill that writes an object's current value records an event and names the object in the rewrite category.

#### 3. A migration cannot write a mirror, yet a publish moves every object

- **Construct:** a publish's mappings, `admit` and backfills on a type that stays a mirror.
- **Case:** fleet v2 keeps `Customer` a mirror. It drops DORMANT with `removed_states: Customer: { DORMANT: ACTIVE }`, or adds a required attribute with a backfill, or adds an invariant and admits it. C8 is DORMANT.
- **The readings:**
  - (a) A migration is F2's third route and reaches every object. C8 gets a `migrate` event, written by the publish.
  - (b) Only the import and erasure write a mirror, so a publish whose mapping, backfill or `admit` reaches a mirror's object is refused.
  - (c) The mapping skips mirror objects and C8 waits for the next import. The state CHECK then either fails on C8's row or is not replaced.
- **Where the text fails:**
  - flow-format.md §4.15: “**A mirror** is a type another system owns: this store holds it, and only the import writes it”
  - declaration-syntax.md §2: “Check 53 is what makes the marking mean something: nothing here may write it but the import and its own erasure.”
  - declaration-syntax.md §2: “A mirror declares attributes, states, its external identifier and the invariants the import checks, and **no transitions but `erase`**”
  - flow-format.md §4.16: “A module's `migration` section says how a publish of this version moves every object of the version before it, final ones included and observations excepted (ADR-0146)”
  - DESIGN.md §5.9: “It reaches every object of the type, final ones included, and never an observation”
- **Severity:** changes-outcome
- **Suggested fix:** State whether a publish's migration is a writer of a mirror beside the import and erasure. If it is not, say what a publish that changes a mirror's states, members, required attributes or invariants does.

#### 4. An admission already standing, in the dry run's decisions

- **Construct:** decisions, and how each is resolved, when the object already holds an admission of the invariant.
- **Case:** the cutover publish removes `mirror: true` from `Customer` and keeps `has_contact`, with no `admit` line. C8 holds a standing import `ADMIT` of it. A second case: R5 was asserted into RETIRED admitting `retired_with_reason`, and v2 backfills Robot.
- **The readings:**
  - (a) A standing admission counts as admitted. There is no decision, the publish installs and no new admission is recorded.
  - (b) Every violation is a decision unless an `admit` in the change covers it. The publish is refused until the change admits `Customer.has_contact`. With that line, C8 gets a second admission on its migration event, which `override_counts` counts, though it never counted the import's.
- **Where the text fails:**
  - publish-and-import.md §1 step 3: “evaluate every invariant, a new one included, over every object as the mappings leave it, reporting each violation with its objects as a decision, and whether an `admit` in the change covers it”
  - publish-and-import.md §1 step 5: “refuse it too while the report holds a decision unresolved, a violation neither fixed nor backfilled nor admitted, since a publish leaves no object in a violation nobody admitted (ADR-0146)”
  - DESIGN.md §6 step 7: “except those with an admitted violation still standing (ADR-0045, ADR-0054)”
  - DESIGN.md §6: “its step 7 checks every invariant once over the row it leaves, and a violation neither fixed, backfilled nor admitted refuses the publish”
  - publish-and-import.md §4: “the owned version's violations surface in the cutover publish, whose `admit` is counted”
  - declaration-syntax.md §6.6: “records an admission, with the reason, a member of a declared enum as an assertion's reason is, so overrides stay countable per reason, on each object that violates the invariant after the publish”
- **Severity:** changes-outcome
- **Suggested fix:** State whether a violation already covered by a standing admission is a decision, and whether the change's `admit` adds an admission on such an object.

#### 5. Invariants across objects: which types are read, and when each is checked

- **Construct:** the objects step 3 reads, and when the migration checks invariants.
- **Case A, timing:** fleet v2 adds to Robot the traversal invariant `model_released: model.state != RobotModel.DRAFT`, backfills Robot, and maps `RobotModel: { DRAFT: ACTIVE }`. R1 (lower id) references M2 (higher id, DRAFT). After all the mappings M2 is ACTIVE, so the dry run reports no violation.
- **Case B, scope:** fleet v2 maps `Robot: { LEGACY_HOLD: RETIRED }`. An unchanged type `Lease` has `robot.state != Robot.RETIRED or state == ENDED`, and Lease L10 (ACTIVE) references R3.
- **The readings:**
  - A(a): each object's invariants are checked at its own migration. R1's runs first and reads M2 still DRAFT, so it violates and the publish is refused. If an `admit` covers the invariant, the admission is recorded and then discharged when M2 migrates.
  - A(b): the check is made as the dry run made it, after all the mappings, and the publish installs.
  - B(a): `Lease` is not a type the change touches, so it is not read and not reported. At R3's migration, step 7 either re-checks L10 (refused, although the dry run reported nothing) or does not ("over the row it leaves"). In the second case the publish installs with L10 in an unadmitted violation, which fails harness row 2.
  - B(b): "every object" includes L10, so there is a decision.
- **Where the text fails:**
  - publish-and-import.md §1: “3. **Read every object** of each type the change touches”
  - publish-and-import.md §1 step 3: “evaluate every invariant, a new one included, over every object as the mappings leave it, reporting each violation with its objects as a decision, and whether an `admit` in the change covers it”
  - publish-and-import.md §1 step 5: “checking every invariant once over the row it leaves”
  - DESIGN.md §6: “7. Check every invariant the written objects could violate”
  - adversarial-harness.md §1: “an invariant false, with no admission recorded, except a kind's invariant on an observation recorded before a publish declared it, which nothing can change (ADR-0146)”
- **Severity:** changes-outcome
- **Suggested fix:** Define the types step 3 reads: the change's own, or also every type with an invariant or guard that reads them. State whether a migration checks invariants per object as it goes, including other objects' invariants that read it, or once after all the mappings.

#### 6. The CHECK is replaced only after the mappings, though a mapping may target a new state or member

- **Construct:** when constraints are added, against the order of the mappings.
- **Case:** fleet v2 renames a state the only way the format allows: it drops LEGACY_HOLD, adds STORED, and maps `Robot: { LEGACY_HOLD: STORED }`. Or it adds `RetirementReason.OUTDATED` and maps `OBSOLETE: OUTDATED`.
- **The readings:**
  - (a) In the literal order, R3's migration writes `STORED` while the CHECK still lists v1's states, so the database refuses. The publish fails, a fault the dry run never reported.
  - (b) The CHECK is widened before the mappings and narrowed after, and the publish installs.
- **Where the text fails:**
  - publish-and-import.md §1 step 5: “emit the DDL of the storage schema §10 that adds columns and renames; apply the mappings”
  - publish-and-import.md §1 step 5: “only then replace the `CHECK`s and add the constraints the change brings”
  - flow-format.md §4.16: “a mapping MUST name a state, member or attribute this version no longer has and move it to one it has”
  - storage-schema.md §3: “with a `CHECK` over the declared state set, so an unreachable value is refused by the database”
  - storage-schema.md §10: “the `CHECK` is replaced after a `removed member` mapping has moved every live row off it by recorded migration transitions (ADR-0099)”
  - storage-schema.md §10: “| a removed state | no DDL; the `CHECK` is replaced, and the mapping moves live objects out of it first |”
- **Severity:** changes-outcome
- **Suggested fix:** State that the CHECKs are widened to the union of both versions' values before the mappings, and replaced by the new version's set after.

#### 7. A member made tracked: which event opens its interval, and whether it is `.migrated`

- **Construct:** a member a publish makes tracked opening its interval; `.migrated` interval rows.
- **Case:** fleet v2 adds a required `grade: { type: Grade }` backfilled with `Grade.B`, and an optional `site` reference with no backfill.
- **The readings:**
  - (a) As step 5, DESIGN §5.9 and DESIGN §7 say, `grade`'s interval opens at each Robot's migration event, and `site`'s interval of absence at the publish event.
  - (b) As storage §10 says, both open at the publish event.
  - Separately, the `migrated` flag of an interval opened at the publish event is 0 to one builder (the publish event is not a migration) and 1 to another (the publish opened it).
- **Where the text fails:**
  - storage-schema.md §10: “| a new tracked member | no DDL; an `of_interval` row per live object holding its current value, opened at the publish event, which a fold reproduces (ADR-0106) |”
  - publish-and-import.md §1 step 5: “open the current interval of every member the change makes tracked, for every object, at the event that first gives it a value in the publish, the object's migration where a backfill writes it and else the publish event”
  - storage-schema.md §6: “-- opened by a publish's migration (ADR-0146)”
  - declaration-syntax.md §6.9: “`.migrated` is true on a span a publish's migration opened, which the standard metrics count, the time in it being real (ADR-0146)”
- **Severity:** changes-value
- **Suggested fix:** Make storage §10's row say what step 5 says, and state the `migrated` flag of an interval opened at the publish event.

#### 8. The time a publish records, given the dry run's fixed `now`

- **Construct:** the fixed `now`, against the time of the publish and migration events.
- **Case:** a change is submitted at T1, so its report's `now` is T1. R1 is named under v2's backfill of a tracked `grade`. R1 is retired at T2, which names it in no new category. The change is approved at T3.
- **The readings:**
  - (a) `now` serves only the evaluation in steps 1 to 4. The events and `grade`'s interval on R1 record T3.
  - (b) The publish request's time is the attached `now`, so its events and intervals record T1. They then precede R1's T2 event by occurred time while following it by position, which harness row 8 fails.
  - The report must record `now`, but the `PublishReport` dataclass has no field for it (checked by script).
- **Where the text fails:**
  - publish-and-import.md §1 step 3: “at a `now` the dry run fixes and records on the report”
  - publish-and-import.md §1 step 5: “run steps 1 to 4 again at the attached report's `now`”
  - adversarial-harness.md §1: “or an interval an import or a publish opened that is out of order, by occurred time, with the object's others”
- **Severity:** changes-value
- **Suggested fix:** State the occurred and recorded times of the publish and migration events, and add the dry run's `now` to `PublishReport`.

#### 9. Which clauses count as "newly enforced"

- **Construct:** the impact category of clauses that would refuse objects, which the approval compares.
- **Case:** fleet v2 adds a brand-new `deny` guard to `Robot.retire`; separately, turns a `warn` into `deny`; separately, turns an `audit` into `deny`. Robot R9 enters AVAILABLE after `submit`.
- **The readings:**
  - (a) Under step 3, only the `audit` case belongs, as a clause the change stops observing.
  - (b) Under §2 and DESIGN §9, all three are newly enforced.
  - In the new-guard case, R9 is newly named under (b), so the publish is refused, and under (a) it installs.
- **Where the text fails:**
  - publish-and-import.md §1 step 3: “which a clause the change stops observing would now refuse”
  - publish-and-import.md §2: “Notices: a clause newly enforced that would refuse objects, transitions gained or lost, proposals invalidated, changes superseded, and an `admit` on objects in a final state, which nothing can discharge.”
  - DESIGN.md §9: “those a clause newly enforced would refuse”
- **Severity:** changes-outcome
- **Suggested fix:** Define the category once, saying whether a new enforced guard, and a flag made enforced, belong beside an observing clause made enforced.

#### 10. Evaluating, with no request, a clause that reads an input or an unconsulted evaluator

- **Construct:** evaluators and metrics in the dry run; the categories for transitions gained or lost and for clauses that would now refuse.
- **Case:**
  - In service v1 → v2, `finish.photo_attached` goes from `audit` to `deny` and reads `inputs.completion_photo`. The same happens to `deliver_externally` in inventory v1 → v2.
  - J2 is WORKING, and J9 enters WORKING after `submit`.
  - Separately, a transition guarded by an evaluator.
  - The units v2 change removes `hold` and narrows the `from` of `name`.
- **The readings:**
  - (a) Evaluate with no inputs: the clause is false or unknown, so it refuses J2. J2 is named under "would now refuse" and as losing `finish`. J9 is newly named, so the publish is refused.
  - (b) Treat the clause as input-dependent, "available-with-input": nothing is refused or lost, the categories are empty, and the publish installs.
  - A guard listed as not evaluated may be counted as passing, as failing, or not at all.
  - Step 3's comparison with `availability` contradicts its own rule, since `availability` does compute internal metrics while step 3 consults none.
  - Whether a removed transition or a narrowed `from` counts as "lost" under "the new guards" is unsaid.
- **Where the text fails:**
  - publish-and-import.md §1 step 3: “consulting no evaluator and no metric, a guard that reads one listed as not evaluated, as `availability` does (ADR-0146)”
  - publish-and-import.md §1 step 3: “which would gain or lose an available transition under the new guards”
  - publish-and-import.md §1 step 3: “which a clause the change stops observing would now refuse”
  - DESIGN.md §5.5: “Availability is therefore three-way for a listing: available, available-with-input naming what must be supplied, or blocked with a verdict.”
  - DESIGN.md §10: “`check` and `availability` do compute internal metrics, since no third party is involved”
- **Severity:** changes-outcome
- **Suggested fix:** State how step 3 classifies a clause or transition whose verdict depends on an input, or on an evaluator or metric it does not consult, against the three ways of availability. State whether a removed transition or a narrowed `from` is a loss.

#### 11. What one "impact category" is, and what the approval compares

- **Construct:** the approval's comparison of the attached report with the recomputed one.
- **Case A:** at `submit`, X is named for violating `inv1`, which the change admits. After `submit`, X also violates `inv2`, which the change admits too.
- **Case B:** C2 is drafted after C1's `submit`, against the same version, and then C1 is approved.
- **The readings:**
  - A(a): each invariant is a category, so (X, `inv2`) is new and the publish is refused.
  - A(b): there is one category of violations, X is already named, and the publish installs.
  - B(a): publish-and-import does not compare superseded changes, so the publish installs.
  - B(b): DESIGN §9 requires the recomputed report to name nothing the attached one did not. C2 is newly named, so the publish is refused.
- **Where the text fails:**
  - publish-and-import.md §1 step 5: “refuse the publish as `impact_unchanged` if the recomputed report names, in any impact category, an object or proposal the attached one did not name in that category, the changes it will supersede being reported and not compared”
  - DESIGN.md §9: “It requires a change drafted against the installed version and a recomputed report naming nothing the attached one did not.”
  - publish-and-import.md §2 (`affected`): “# ids per impact, compared again at approval (ADR-0096)”
- **Severity:** changes-outcome
- **Suggested fix:** List the keys of `affected`, saying whether a category is per mapping and per invariant, and give DESIGN §9 the exemption for superseded changes.

#### 12. The refusal for an unresolved decision has no verdict name

- **Construct:** the approval's verdicts.
- **Case:** the units v2 change is approved while R5 violates `retired_with_reason` and the change carries no `admit`.
- **The readings:** the refusal is `Unsatisfied` naming `impact_unchanged`, or naming `current`, or naming a clause the text does not give. A fatal finding at the recompute has the same problem.
- **Where the text fails:**
  - publish-and-import.md §1 step 5: “refuse it too while the report holds a decision unresolved, a violation neither fixed nor backfilled nor admitted, since a publish leaves no object in a violation nobody admitted (ADR-0146)”
  - library-api.md §6: “The result carries the verdict — `Satisfied`, `Stale`, or `Unsatisfied` naming `current` or `impact_unchanged` — beside the report”
- **Severity:** changes-value
- **Suggested fix:** Name the clause for an unresolved decision, and for a fatal finding at the recompute.

#### 13. Observations: left out of the read, yet reported and refused on

- **Construct:** an observation kind's new invariant; a removed member an observation holds; "live".
- **Case:**
  - service v3 narrows `value_in_range` to 0 to 50 under the same name, while R11, an `InspectionResult` with value 55, is recorded.
  - service v3 removes `CheckOutcome.NOT_APPLICABLE` while observations hold it.
  - v3 writes `admit: InspectionResult.value_in_range`.
  - An import records a legacy observation that violates its kind's invariant.
- **The readings:**
  - (a) Step 3 reads no observation. There is no notice, and the removed member installs; the CHECK on the observation table then fails or is not replaced.
  - (b) Observations are read for these purposes, giving a notice and a refusal.
  - A changed invariant is "new" to one builder (a notice, and the publish installs). To another it is an existing invariant whose violations are decisions no `admit` can resolve, so the publish is refused for as long as the observation exists.
  - The `admit` is refused by one builder and reported as covering nothing by another.
  - The import may `ADMIT` the observation, since it is an `INVARIANT` class, or may not, since observations are "never admitted".
- **Where the text fails:**
  - publish-and-import.md §1 step 3: “final ones included and observations excepted”
  - flow-format.md §4.16: “the observations already recorded that violate it are reported as a notice and never admitted, since no migration reaches an observation (ADR-0146)”
  - flow-format.md §4.16: “A new invariant of an observation kind applies to recordings after the publish”
  - declaration-syntax.md §6.6: “so a publish removing an enum member any observation holds is refused (check 23)”
  - adversarial-harness.md §1: “except a kind's invariant on an observation recorded before a publish declared it, which nothing can change (ADR-0146)”
- **Severity:** changes-outcome
- **Suggested fix:**
  - Say step 3 reads observations for a removed member and a kind's invariants.
  - Say whether a changed kind invariant counts as new.
  - Say whether a publish's `admit`, or an import's `ADMIT`, may name a kind's invariant.

#### 14. A possible rename: a notice or a decision

- **Construct:** an attribute dropped and another of its type added, with no mapping.
- **Case:** fleet v3 drops `nickname` (optional string) and adds `display_name` (optional string), with no mapping.
- **The readings:**
  - (a) A notice: the publish installs as a drop and an add. `display_name` is absent on every Robot, and `nickname`'s values survive only in history.
  - (b) The word "asks" makes it a decision, so the publish is blocked. The migration grammar has no line that says a pair is not a rename, so (b) ends only by declaring the rename.
- **Where the text fails:**
  - declaration-syntax.md §10 (check 23): “A **rename is reported, not failed**: a rename with no mapping is textually identical to a drop plus an add, and no information in either declaration distinguishes them, so publishing lists each dropped and added pair and asks.”
  - flow-format.md §4.16: “an attribute gone where one of its type is new, which may be a rename”
  - publish-and-import.md §2: “Each impact has its severity (ADR-0146).” Its list does not include this case.
- **Severity:** changes-outcome
- **Suggested fix:** Give the dropped-and-added pair its severity in §2's list. If it is a decision, give the way to answer that a pair is not a rename.

#### 15. A mapping into a final state: what is "stranded", and `survives`

- **Construct:** mappings into a final state; a whole's parts.
- **Case:** fleet v2 drops ON_HOLD with `Delivery: { ON_HOLD: CANCELLED }`, where D6 has part K7, OPEN. Three variants of the composite end: `survives: true`; `survives: [cancel]`, where `cancel` enters CANCELLED; or a cascade on `cancel`.
- **The readings:**
  - (a) Any part not in a final state is stranded, whatever `survives` says, so all three are refused.
  - (b) `survives: true` covers the migration as one of "every final transition", so variant 1 installs.
  - (c) A `survives` list naming a transition into the target state counts, so variant 2 installs.
  - Harness row 4 requires `survives` for "that transition". For a migrated whole that is `migrate`, which no list can name, so the harness fails variant 2 under (c).
  - The severity of this refusal is not in §2's list.
- **Where the text fails:**
  - publish-and-import.md §3: “A mapping into a final state drives no cascade, so a publish whose objects in the removed state have parts the mapping would strand is refused”
  - declaration-syntax.md §6.6: “one into a final state drives no cascade, so objects it would leave with stranded parts refuse the publish (check 23, ADR-0146)”
  - flow-format.md §4.3: “`survives` is `true` if the parts outlive every final transition of the whole, or the list of transitions they outlive.”
  - adversarial-harness.md §1: “a part alive under a whole in a terminal state, except one its type declares `survives` that transition”
- **Severity:** changes-outcome
- **Suggested fix:** Define "stranded" for a migration: parts not in a final state, and whether `survives: true` or a list naming a transition into the target exempts them. Make harness row 4 agree.

#### 16. An import writing a mirror whole into a final state with live parts

- **Construct:** the import writing an object in a final state; mirrors composing; the harness's condition on parts.
- **Case:** the legacy data has mirror Delivery D12 DONE with mirror part K13 OPEN.
- **The readings:**
  - (a) The import writes the rows as they are, since a mirror has no lifecycle rules. Harness row 4 then fails, because unlike row 1 it exempts no mirror.
  - (b) The import refuses the row, refusing the batch, or the dry run raises a class.
- **Where the text fails:**
  - publish-and-import.md §4: “it may write attributes with the state unchanged, and an object in a final state, as a refresh mirrors the legacy record”
  - declaration-syntax.md §2: “**A mirror may compose with and extend other mirrors**”
  - adversarial-harness.md §1 row 1: “a mirror's objects and a retired type's are exempt”
  - adversarial-harness.md §1 row 4: “a part alive under a whole in a terminal state, except one its type declares `survives` that transition”
- **Severity:** changes-outcome
- **Suggested fix:** Say whether an import may leave a live part under a final whole. If it may, exempt mirrors in row 4; if not, name the class the dry run raises.

#### 17. The generated guards on the import's built-in assertion

- **Construct:** the built-in assertion, porting observations, a refresh that re-parents.
- **Case:**
  - A mirror type has an observation kind with `occurred within 7 days`, implied possible by declaration-syntax §6.8's "A mirror's observations".
  - A legacy result occurred on 2024-03-01, and its subject is DONE.
  - A refresh moves a part to a final whole.
- **The readings:**
  - (a) The built-in assertion keeps no rule and writes what the legacy record says, so everything imports.
  - (b) An asserting transition evaluates the generated guards every request does. `occurred_within` and `subject_open` refuse, and so does `whole_open` on the re-parent, refusing the batch.
  - `subject_owned` would refuse every observation on a mirror, so (b) must exempt at least that one, and the text never says which others.
- **Where the text fails:**
  - publish-and-import.md §4: “It writes each object through the **built-in assertion**, which keeps none of a declared assertion's rules”
  - publish-and-import.md §4: “with the observations and labels on a mirror's objects, so legacy datapoints port with their subjects”
  - DESIGN.md §6: “An **asserting** transition (§8) differs in two ways: step 4 evaluates the generated guards every request does and its own guards”
  - DESIGN.md §6 step 4: “the generated ones included — `occurred_within`, `whole_open`, `not_erased`, `successor_not_superseded`, and for a recording `subject_open`, `corrects_current` and `subject_owned`”
  - declaration-syntax.md §6.8: “`subject_open`, that the subject is not in a terminal state, for a new observation.”
  - declaration-syntax.md §6.8: “Omitted, the occurred time is the recorded time, and a request that supplies one is refused by `occurred_within`.”
  - declaration-syntax.md §6.8: “`subject_owned`, that the subject is not a `mirror`.”
  - DESIGN.md §10: “which no request may record there”
- **Severity:** changes-outcome
- **Suggested fix:** List the generated guards the built-in assertion evaluates and those it skips.

#### 18. A refresh's legacy times: compared with which latest entry

- **Construct:** refreshes.
- **Case:** a mirror job's state interval (WORKING) was entered at T5, and its `engineer` interval at T3. A refresh changes the engineer, entered at T4.
- **The readings:**
  - (a) Comparing with the object's latest entry over every dimension: T4 is before T5, so the refresh is refused with its batch.
  - (b) Comparing per dimension: T4 is after T3, so the refresh applies.
- **Where the text fails:**
  - publish-and-import.md §4: “one whose legacy times precede the object's latest interval entry is refused, which refuses its batch (ADR-0146)”
- **Severity:** changes-outcome
- **Suggested fix:** Say whether the comparison is per dimension or over the whole object.

#### 19. A refresh whose current values did not change

- **Construct:** refreshes; `imported_at`.
- **Case:** since the last import, the legacy engineer went from A to B at T1 and back to A at T2. Separately, a refresh brings only new audit rows.
- **The readings:**
  - (a) Nothing changed, so there is no event. B's interval is dropped, and the new entries are attached without an event or not at all.
  - (b) The intermediate values are changes. The refresh writes an event with intervals A up to T1, B from T1 to T2 and A from T2, and moves `imported_at`.
- **Where the text fails:**
  - publish-and-import.md §4: “A refresh of a mirror carries, for each value that changed since the last import, the same `entered`, and may carry the intermediate values the legacy record shows”
  - publish-and-import.md §4: “A refresh that changes nothing writes no event and does not move `imported_at`”
- **Severity:** changes-value
- **Suggested fix:** Define "changes nothing" in terms of the current values, the intermediate values and the legacy entries.

#### 20. Sending again a batch that arrived

- **Construct:** one transaction per batch; resuming an import.
- **Case:** batch B1 committed, but the client lost the reply. The resumed run sends B1 again, with each object's full legacy intervals and entries.
- **The readings:**
  - (a) The engine sees that every key has arrived, skips the batch, and returns success.
  - (b) Each record is a refresh. Its carried legacy intervals precede the latest entry, so the batch is refused and the resumed run cannot pass.
  - (c) The refresh attaches the entries again: as duplicates, or failing on the primary key `(object_id, ordinal)`.
- **Where the text fails:**
  - publish-and-import.md §8: “a batch is one transaction, so a batch that failed wrote nothing and is sent again, and one that arrived is skipped, its objects refreshed only where the legacy record changed (ADR-0146)”
  - publish-and-import.md §4: “a refusal refuses the whole batch, naming the first row it could not place, and a resumed run sends the batch again (ADR-0146)”
  - publish-and-import.md §4: “An object whose legacy key has arrived is refreshed.”
  - publish-and-import.md §4: “one whose legacy times precede the object's latest interval entry is refused, which refuses its batch (ADR-0146)”
  - storage-schema.md §6 (`of_legacy_entry`): “PRIMARY KEY (object_id, ordinal)”
- **Severity:** changes-outcome
- **Suggested fix:** Say who skips an arrived batch, the client or the engine. Say what a refresh does with legacy intervals and entries the store already holds.

#### 21. Who a port-opened interval, and a ported record, names

- **Construct:** intervals the port opens, and who changed them.
- **Case:**
  - A mirror job is imported by the service actor `importer` with no `entered` for its state, so its interval begins at the import.
  - Another job's mapping names the legacy user `jsmith`, whom the store holds no actor for.
  - Observations are ported on mirror objects.
- **The readings:**
  - For the interval beginning at the import, `entered_by` is `importer` of kind `service`, or `unknown`, since the changer is "never the importing actor".
  - For `jsmith`, the kind is `unknown`, or `human` with a bare identity.
  - An imported observation's `.recorded_by_kind` is `service` or `unknown`.
  - Each reading changes the `actor_kind` dimension of every metric over these rows.
- **Where the text fails:**
  - publish-and-import.md §4: “the interval records the legacy changer, as a ported actor, where the mapping names one, and the kind `unknown` otherwise, never the importing actor (ADR-0146)”
  - publish-and-import.md §4: “Where the legacy data says nothing, the current interval begins at the import”
  - publish-and-import.md §4: “Earlier values arrive as legacy intervals of state and of tracked members — assignment included — marked `legacy`, each with who made the change where the record says, and the kind `unknown` where it does not”
  - storage-schema.md §6: “who made the change; a legacy row's where known”
  - declaration-syntax.md §6.8: “`.recorded_by_kind`, the kind of actor that recorded it”
- **Severity:** changes-value
- **Suggested fix:** State the actor and kind for an interval that begins at the port, for a named changer the store holds no actor for, and for an imported observation.

#### 22. `recorded_from` when the cases overlap

- **Construct:** `.recorded_from`.
- **Case:**
  - O1: legacy creation at T0, a silent span from T2 to T3, and a current engineer interval beginning at the port, Tp.
  - O2: a whole state history from T0, but no engineer history, so the engineer interval begins at the port.
  - O3: undated, with no silent span and a current interval from `entered`.
- **The readings:**
  - O1 gets T3 or Tp. DESIGN §5.12's "earliest time its legacy history vouches for" gives T0.
  - O2 gets T0, if "intervals are whole" is read for its state, or Tp.
  - For O3 none of the three cases applies.
  - The `gaps` of every metric depend on these values.
- **Where the text fails:**
  - publish-and-import.md §4: “It sets each object's `recorded_from`, one value per object read from its intervals: its legacy creation where its intervals are whole, the end of its last silent span where there is one, and the port where its current interval begins at the port (ADR-0146).”
  - DESIGN.md §5.12: “Each object records the time from which its record is continuous: its creation, or for a ported object the earliest time its legacy history vouches for (§11).”
- **Severity:** changes-value
- **Suggested fix:** Give the cases a precedence, such as the latest applicable time over every tracked dimension, and a value when none applies. Make DESIGN §5.12's wording agree.

#### 23. Silent spans: crossing, inside, or overlapping, and refused or dropped

- **Construct:** silent spans in the import; the harness's condition on intervals.
- **Case:** a silent span from T2 to T4. One legacy interval runs from T1 to T3, across T2; another runs from T2.5 to T3, inside the span.
- **The readings:**
  - Under publish-and-import's "cross", the first is refused and the second accepted.
  - Under DESIGN's "inside", the second is dropped or refused, and the first is unclear.
  - Under the harness's "at all", both fail.
  - The import may refuse the batch, or clip or drop the interval so that it "leaves no legacy row".
- **Where the text fails:**
  - publish-and-import.md §4: “The mapping declares, per object, where the legacy record is silent, and no legacy interval may cross one; nothing is invented to fill it.”
  - DESIGN.md §11: “The mapping declares the record's known silent spans, and the import leaves no legacy row inside one.”
  - adversarial-harness.md §1: “overlaps a declared silent span at all”
- **Severity:** changes-outcome
- **Suggested fix:** State one rule, no overlap, and state that the import refuses rather than trims.

#### 24. The two new classes have no disposition that lets the import proceed

- **Construct:** the disposition file; a field about another object; a reference into an excluded object.
- **Case:**
  - A service's audit row carries a customer's name, and `Customer` is not ported yet.
  - A service holds an optional reference to a customer the file excludes.
  - The customer arrives in the same batch as the service.
- **The readings:**
  - (a) Only the existing dispositions apply. `CLEAN_UPSTREAM` refuses the batch, `ADMIT` is for invariant classes only, and `EXCLUDE` drops the whole service. So the service is excluded, and the import writes less.
  - (b) The field is dropped, as the Attach row says, and the reference is cleared. The class is informational, or the builder invents a disposition.
  - In the same-batch case, the field moves or is dropped depending on the order in which the batch attaches entries.
- **Where the text fails:**
  - publish-and-import.md §4 (Dry run): “in which a reference into an excluded object, and a field about an object not yet held, are classes of their own (ADR-0146)”
  - publish-and-import.md §6: “INVARIANT classes only”
  - publish-and-import.md §6: “An import will not run while any class is `UNDECIDED`.”
  - publish-and-import.md §4 (Attach): “and is otherwise not ported, the dry run having reported it as a class to decide, such as by porting that type first (ADR-0146)”
  - publish-and-import.md §4 (Attach): “moves to that object's history where the store already holds that object when the entry is attached”
  - `ViolationKind` has only `INVARIANT` and `GUARD`, and `Disposition` only `UNDECIDED`, `CLEAN_UPSTREAM`, `ADMIT` and `EXCLUDE` (checked by script).
- **Severity:** changes-outcome
- **Suggested fix:** Give these classes their kind and their dispositions, such as dropping the field or clearing an optional reference. Say whether "already holds" includes the same batch.

#### 25. `EXCLUDE` on an object already imported

- **Construct:** the disposition file applied to refreshes.
- **Case:** C8 was imported in batch 1, and a refresh in batch 7 puts it in an `EXCLUDE` class.
- **The readings:** C8 is skipped and left stale; or the batch is refused; or the store's copy is treated as out of scope, which "nothing in the store" cannot describe, since C8 is already there.
- **Where the text fails:**
  - publish-and-import.md §6: “# do not import these objects at all”
  - publish-and-import.md §6: “An excluded object leaves a row in the file and nothing in the store”
  - publish-and-import.md §6: “a class applies to every object in it at import”
- **Severity:** changes-value
- **Suggested fix:** Say what `EXCLUDE` does to an object the store already holds.

#### 26. Mappings on a family's abstract base

- **Construct:** backfills and admits naming a type.
- **Case:** an abstract `Unit`, extended by `Robot` and `Rover`, gains a required `asset_tag`. The change writes `backfill: { Unit: { asset_tag: '"NONE"' } }`, or `admit: Unit.tagged`.
- **The readings:**
  - (a) A mapping on the base reaches every member of the family, and the publish installs.
  - (b) The base has no objects, so the mapping reaches none. Robots and Rovers lack `asset_tag`, which is fatal.
  - (c) Only `Robot: { asset_tag: … }` is legal, or naming an inherited member on the concrete type is refused.
- **Where the text fails:**
  - flow-format.md §4.16: “| `backfill` | no | for each type, each attribute every object of it is given”
  - flow-format.md §4.16: “an admission MUST name an invariant of the type and a member of a declared enumeration”
  - declaration-syntax.md §2: “an `abstract` type may state it and needs none, having no objects”
  - DESIGN.md §5.9: “A type may **extend** a base declaration for attributes, relationships, invariants and derived attributes”
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a mapping on a base reaches its family, and whether a concrete type's mapping may name an inherited member.

#### 27. A removed member in a set of enum values

- **Construct:** removed members.
- **Case:** R1 holds `defects = {OBSOLETE_PART, WORN}`, and v2 maps `Defect: { OBSOLETE_PART: WORN }`.
- **The readings:** the set becomes {WORN}, de-duplicated; or the side table holds WORN in two rows; or the publish is refused.
- **Where the text fails:**
  - flow-format.md §4.16: “| `removed_members` | no | for each enumeration, each member the previous version had and this one does not, and the member live values become |”
  - DESIGN.md §5.2: “Numbers, free text and sets are not tracked, a set of enum values included”
- **Severity:** changes-value
- **Suggested fix:** State what a removed member's mapping does to a set that already holds the target member.

#### 28. A backfill that leaves a required attribute absent

- **Construct:** backfills; the fatal finding for a new required value with no `backfill`; `NOT NULL`.
- **Case:** fleet v2 adds a required `site_code` with backfill `nickname`, and R4's `nickname` is absent.
- **The readings:**
  - (a) Requiredness is not an invariant, so the dry run is silent and R4 is written with the value absent. Then either `NOT NULL` is added and the database refuses, a fault; or it is never added, since the text says when columns are added nullable but not when `NOT NULL` follows (on SQLite that needs a table rebuild). In the second case the publish installs with a required value absent, which fails harness row 3.
  - (b) The fatal finding covers a backfill that yields absence, so the publish is refused cleanly.
- **Where the text fails:**
  - publish-and-import.md §3: “A `backfill` writes every object of its type; one that fills only an absent value says so, `if x is null then … else x`.”
  - declaration-syntax.md §6.6: “a new required singular **reference** is filled by `backfill` in the same way, from an expression the object's own members yield”
  - storage-schema.md §3: “A declared-required attribute takes `NOT NULL`.”
  - declaration-syntax.md §6.6: “DDL never writes a row: a new column is added nullable”
  - adversarial-harness.md §1: “a required attribute absent, except where an admission covers it”
- **Severity:** changes-outcome
- **Suggested fix:** Say that a required attribute a backfill leaves absent is fatal in the dry run, and when and how `NOT NULL` is applied on each backend.

#### 29. Changes the publish text does not cover: optional made required, and a type changed in place

- **Construct:** comparison with the installed version.
- **Case:** inventory v3 makes `photo` required, where it was optional, while some items lack one. Separately, it changes `retirement_reason` from `string` to an enum.
- **The readings:**
  - Optional made required is a "new required attribute", fatal without a backfill, to one builder. To another it is not covered: the publish installs, or fails at `NOT NULL`.
  - A type changed in place is refused, applied as `ALTER COLUMN` with no event, or treated as a drop and an add.
  - My inference is that the DDL table in storage §10 has no row for either. Its rows are a new type, a new attribute, a new indexed attribute, removed, renamed, a removed state, a removed member, a removed type, a new invariant, an `admit` and a new tracked member.
- **Where the text fails:**
  - declaration-syntax.md §6.6: “a new **required attribute** on a type with live objects needs a `backfill`”
  - storage-schema.md §10: “| a new attribute | `ADD COLUMN`, nullable.”
- **Severity:** changes-outcome
- **Suggested fix:** State that making an attribute required counts as a new required attribute for step 3. State whether an attribute's type may change in place, and if so how.

#### 30. A retired type's objects, and a removed member they hold

- **Construct:** "live", a removed type, removed members.
- **Case:** v2 retires the type `Lease`, whose objects hold `Condition.DAMAGED`, and maps `Condition: { DAMAGED: REFURBISHED }`.
- **The readings:**
  - (a) Lease's objects are live, so they are migrated, against "no transition applies".
  - (b) They are not migrated, but the CHECK on Lease's table is replaced and fails.
  - (c) Their CHECK is left alone.
  - (d) The publish is refused.
- **Where the text fails:**
  - declaration-syntax.md §6.6: “a removed **type** or **observation kind** is retired: nothing is created in it and no transition applies, and its objects, table and history stay readable under the versions that wrote them”
  - declaration-syntax.md §10 (check 23): “and a removed enum member that live objects hold with no `removed member` mapping”
  - storage-schema.md §10: “the `CHECK` is replaced after a `removed member` mapping has moved every live row off it by recorded migration transitions (ADR-0099)”
- **Severity:** changes-outcome
- **Suggested fix:** Say whether a retired type's objects are live for removed members, and whether its table's CHECKs are replaced.

#### 31. The engine upgrade installs a version without a `DeclarationChange`

- **Construct:** how the built-ins change.
- **Case:** the store is at v7 when release 1.4 changes the built-in `Proposal` type. An open change C3 is drafted against v7.
- **The readings:**
  - (a) Per storage §12, `upgrade` installs v8 with no change. C3 is then drafted against an older version, but no publish superseded it, so its `publish` is refused as `current`.
  - (b) Per publish-and-import and DESIGN §5.9, the built-ins change only at C3's publish, whose report lists the change.
- **Where the text fails:**
  - storage-schema.md §12: “Where they pass, it records them as the next version, with the installed version's source, reference and digest and the new built-ins, and no `DeclarationChange`”
  - publish-and-import.md §1: “A flow changes only through a **`DeclarationChange`**”
  - publish-and-import.md §1: “an engine release changes the built-ins in a store only through a publish (ADR-0105)”
  - DESIGN.md §5.9: “an engine release that changes a standard metric's definition or a built-in type takes effect in a store only through a publish, whose report lists the change”
- **Severity:** changes-outcome
- **Suggested fix:** Reconcile the two: either the upgrade's version is a publish, or the built-ins take effect only at the next publish. Say what happens to open changes, and to built-in objects whose type changed.

#### 32. Which open changes, "drafted against an older version", a publish supersedes

- **Construct:** superseding other changes.
- **Case:** C1 and C2 are both drafted against v5 and submitted. C1 is published as v6.
- **The readings:**
  - (a) The word "older" means older than the version now installed, v6, so C2 is `SUPERSEDED`.
  - (b) The word "older" means older than v5, the version installed when they were drafted. C2 stays `SUBMITTED`, and its own publish is later refused as `current`.
  - The reason clause, "no longer describes", supports (a).
- **Where the text fails:**
  - publish-and-import.md §1: “a publish supersedes every other open change drafted against an older version, since its dry run no longer describes what it would do”
  - DESIGN.md §9: “a publish **supersedes** every other open change drafted against an older version, in the same transaction, moving each to `SUPERSEDED`”
  - DESIGN.md §9: “It requires a change drafted against the installed version”
- **Severity:** changes-outcome
- **Suggested fix:** Write "every other open change".

#### 33. Compiling an invariant an `admit` covers when no admission results, and after the last discharges

- **Construct:** when constraints are added; the report's compilation lists.
- **Case:** in units v2, R5 is given a reason before the approval, so `retired_with_reason` admits nothing at the publish. A later case: every admission of it has discharged.
- **The readings:**
  - (a) It is not compiled, because an `admit` covers it.
  - (b) It is compiled, because no admission stands.
  - After the last discharge it is compiled at the next publish, or never. A request cannot run the DDL.
- **Where the text fails:**
  - publish-and-import.md §3: “A new invariant an `admit` covers is not compiled to a constraint, and an existing one that is compiled has its constraint dropped before the migration writes, since a constraint cannot yield for one row; it stays uncompiled while any admission of it stands, and the report says so”
  - storage-schema.md §10: “`ADD CONSTRAINT`, after the report says how many live objects violate it, and only where no `admit` covers it”
  - storage-schema.md §8: “any invariant that an assertion may `admit`, or of which an admission stands, whatever recorded it: a publish's `admit` or an import's `ADMIT`”
- **Severity:** changes-value
- **Suggested fix:** State which fact governs compilation, the `admit` line or the standing admissions, and when an invariant whose admissions have all discharged is compiled.

#### 34. A mirror's invariants and the import's `ADMIT`: compiled or not

- **Construct:** a mirror's own invariants.
- **Case:** `Customer.has_contact` is local, so it compiles to a CHECK, and the import `ADMIT`s 11 customers.
- **The readings:**
  - (a) It is compiled at the publish, and the import drops the constraint inside `import_batch`, which is DDL in an import that no text describes.
  - (b) A mirror's invariants are never compiled, since the import may admit them.
  - (c) It is compiled, and the batch fails at the CHECK.
- **Where the text fails:**
  - storage-schema.md §8: “any invariant that an assertion may `admit`, or of which an admission stands, whatever recorded it: a publish's `admit` or an import's `ADMIT`”
  - publish-and-import.md §3: “A new invariant an `admit` covers is not compiled to a constraint, and an existing one that is compiled has its constraint dropped before the migration writes, since a constraint cannot yield for one row; it stays uncompiled while any admission of it stands, and the report says so”
  - flow-format.md §4.15: “declares its attributes, its states and its external identifier, which the import matches objects by”. Its next words say a mirror may declare invariants the import checks and admits.
- **Severity:** changes-outcome
- **Suggested fix:** State that a mirror's invariants are not compiled, or that the import drops the constraint in its transaction.

#### 35. Rework over a mapped state

- **Construct:** `.returns` with mapped states; the standard metric `rework`.
- **Case:** in units v2, before the publish R1 went from AVAILABLE through `hold` to LEGACY_HOLD and back through `release` to AVAILABLE. R3 is still in LEGACY_HOLD at the publish and is moved by its migration.
- **The readings:**
  - (a) The mapping is a fact about the declaration and applies to every object's history. R1's rework becomes 2 if a recorded change of state counts, or 0 if the change is judged under the mapping, since AVAILABLE to AVAILABLE is no change.
  - (b) It applies only to objects the migration moved. R1's rework stays 1 (`release` returns).
  - A chain across versions, v2 mapping A to B and v3 mapping B to C, is unaddressed.
- **Where the text fails:**
  - declaration-syntax.md §6.9: “a state a migration mapped counting as the state it was mapped to, which is rework”
  - DESIGN.md §5.12: “A span a migration opened is counted, the time in it being real and its state the one the mapping says the old one now is, and `.migrated` marks it, so a metric can tell it apart (ADR-0146)”
- **Severity:** changes-value
- **Suggested fix:** Say which objects' history a mapped state is reread in, whether "changes state" is judged before or after the mapping, and whether mappings compose across versions.

#### 36. The final-state admission notice: two definitions

- **Construct:** notices.
- **Case:** v2 adds `serial_recorded: manufacturer_serial is not null`, which can fail in any state, and admits it on R4 (RETIRED) and on R1 (AVAILABLE).
- **The readings:**
  - (a) Per object, R4 is in a final state, so there is a notice.
  - (b) Per the invariant's shape, it fails outside final states too, so there is no notice.
  - The claim that "nothing can discharge" such an admission is also untrue: a later publish's backfill runs a migration whose step 7 discharges admissions.
- **Where the text fails:**
  - publish-and-import.md §2: “Notices: a clause newly enforced that would refuse objects, transitions gained or lost, proposals invalidated, changes superseded, and an `admit` on objects in a final state, which nothing can discharge.”
  - flow-format.md §4.16: “and an `admit` of an invariant that fails only in a final state, which nothing can ever discharge, as `service-v2.yaml`'s admission of `photo_when_done` shows”
  - DESIGN.md §6 step 7: “an admitted violation whose invariant now holds is discharged”
- **Severity:** changes-value
- **Suggested fix:** Use one definition. The publish's notice is per object, and the checker's is per invariant, a different notice. Say that a later migration can discharge.

#### 37. Which external identifier the import matches by

- **Construct:** refreshes; erased objects.
- **Case:** the mirror `Employee` declares `employee_id: { external: hr }`, which holds `E-17`; its legacy key is the HR row id 4411. Separately, an Employee whose personal external identifier was erased.
- **The readings:**
  - (a) The import matches by the `legacy` key, so the refresh finds the erased object and skips its personal values.
  - (b) It matches by the declared `hr` identifier. After the erasure `lookup` fails, so a new object is created carrying the erased person's values, or the batch fails on the duplicate legacy key.
- **Where the text fails:**
  - flow-format.md §4.15: “declares its attributes, its states and its external identifier, which the import matches objects by”
  - publish-and-import.md §4: “Each row is written complete with its references and keeps its legacy key as an external identifier with source `legacy`.”
  - publish-and-import.md §8: “`lookup('legacy', key)` says whether it arrived”
  - storage-schema.md §9: “Deletes the `of_external_id` rows of the erased objects' personal external identifiers, so `lookup` no longer resolves them (ADR-0100).”
- **Severity:** changes-outcome
- **Suggested fix:** Name the one key a refresh matches by, and say that it survives erasure.

#### 38. What a backfill may read

- **Construct:** backfill expressions.
- **Case:** one backfill reads `handoffs` (a derived attribute over the object's own intervals), one reads `created_at`, and one reads `declaration_version`.
- **The readings:**
  - Each is "the object's own members" to one builder and is accepted. To another, the derived one reads beyond the object's stored row and is refused.
  - `declaration_version` reads as the old version or the new one during the migration.
- **Where the text fails:**
  - flow-format.md §4.16: “a backfill MUST write a stored attribute and read only the object's own members and literals, not `now` or an input”
  - flow-format.md §5: “Every object also has `id`, `open` (true while its state is neither `closed` nor final), `created_at`, `created_by_kind`, `recorded_from` and `declaration_version`”
- **Severity:** changes-outcome
- **Suggested fix:** List what a backfill may read (stored attributes; derived attributes or not; which built-in members), and the value of `declaration_version` during a migration.

#### 39. The stored record of the mappings cannot hold their order, or two admits of one invariant

- **Construct:** the order of the mappings; `of_migration`.
- **Case:** `backfill: Robot: { grade: …, grade_note: '<reads grade>' }`, which depends on the order written. Separately, an `admit` list naming `Robot.retired_with_reason` twice, with reasons `LEGACY_DATA` and `MIS_SCANNED`.
- **The readings:**
  - The order comes from the source text, or from `of_migration`, which has no ordering column (checked by script).
  - For the two admits: refused, the first wins, or the last wins. The primary key holds one row per invariant.
- **Where the text fails:**
  - publish-and-import.md §1 step 5: “renames, removed enum members, removed states, then backfills in the order written, each reading the object as the mappings before it left it and naming this version's members”
  - storage-schema.md §6: “PRIMARY KEY (version, type_name, kind, from_name)”
- **Severity:** changes-value
- **Suggested fix:** Give `of_migration` an ordinal, and say whether one invariant may be admitted twice in one change.

#### 40. Harness row 1, "under the object's current declaration", for an object cut over

- **Construct:** the harness's condition on states; the cutover.
- **Case:** C8 was imported in DORMANT. The cutover publish keeps DORMANT but declares no transition into it, only `reactivate` out of it. No mapping reaches C8, so its `declaration_version` stays the mirror's version.
- **The readings:**
  - (a) The current declaration is C8's recorded version, in which Customer is a mirror, so C8 is exempt.
  - (b) It is the installed version, in which Customer is owned, so the run fails.
- **Where the text fails:**
  - adversarial-harness.md §1: “an object in a state no transition reaches, counting the type's declared transitions and assertions and not the import's built-in assertion, under the object's current declaration”
  - flow-format.md §5: “`declaration_version`, the version of its latest event, so a migration moves it and an object no publish writes keeps its own”
- **Severity:** changes-outcome (whether the harness run passes)
- **Suggested fix:** Say which declaration row 1 reads, and whether objects ported into a type later cut over are exempt.

#### 41. Override counts: DESIGN §5.12 disagrees with the definition

- **Construct:** the standard metrics and the import's admissions.
- **Case:** the import admits 11 customers.
- **The readings:** DESIGN §5.12 first says every override is counted, and an event recording an admission is an override. It then excludes "the import's own". §6.11's definition, `t.overrides and not t.imported`, settles that the import's admissions are not counted. The admission's free-text reason could not have been a dimension anyway.
- **Where the text fails:**
  - DESIGN.md §5.12: “except the override counts, which count every override, a migration's or a redaction's admission included (ADR-0105)”
  - DESIGN.md §5.12: “override counts per target state and reason — assertions, and the admissions an erasure or a publish records — the import's own excluded (ADR-0105)”
  - DESIGN.md §14: “| **Override** | A change to governed state that does not satisfy the rules: an assertion's event, or any event recording an admission”
  - publish-and-import.md §6: “# required for ADMIT and EXCLUDE”
  - declaration-syntax.md §6.9: “Free text is never a dimension”
- **Severity:** wording
- **Suggested fix:** Say in DESIGN §5.12 that the override counts count every override except the import's.

#### 42. Where the refusals of a mapping are reported

- **Construct:** the checks behind the mapping rules.
- **Case:** a rename that changes optionality, a mapping into a superseding state, stranded parts.
- **The readings:**
  - flow-format §4.16 attributes these to step 3 (`names`). Step 3's list in §7 omits them, and declaration-syntax §6.6 attributes them to check 23, whose row omits them too.
  - A builder working from the tables misses them. One working from the prose reports them under a different `check` number and `code`.
- **Where the text fails:**
  - flow-format.md §4.16: “a mapping MUST NOT move objects into a superseding state, which would leave them superseded by nothing; and against the version before it a rename MUST keep its attribute's type and optionality”
  - flow-format.md §7 (step 3, `names`): “a migration mapping that names what this version still has or lacks the target of, a backfill of no stored attribute or one reading beyond the object, an admission of no invariant or reason, and, against the previous version, a state removed with no mapping, a mapping from what it did not have, or a required field added to an observation kind”
  - declaration-syntax.md §6.6: “one into a final state drives no cascade, so objects it would leave with stranded parts refuse the publish (check 23, ADR-0146)”
- **Severity:** wording
- **Suggested fix:** Put each refusal in one table row, under one check or step.

#### 43. The report and file shapes lack what the text requires

- **Construct:** `PublishReport`; the disposition file.
- **Case:** any publish or import.
- **The readings:**
  - `PublishReport` has no field for the fixed `now` the report must record (checked by script).
  - `ViolationKind` lacks the two classes ADR-0146 added.
  - A guard class's "creation guard unmet" cannot arise, since every import writes a mirror, which declares no creation.
- **Where the text fails:**
  - publish-and-import.md §1 step 3: “at a `now` the dry run fixes and records on the report”
  - publish-and-import.md §6: “a structural guard — a required attribute or reference absent, a creation guard unmet — has no admission shape”
  - declaration-syntax.md §2: “A mirror declares attributes, states, its external identifier and the invariants the import checks, and **no transitions but `erase`**”
- **Severity:** wording
- **Suggested fix:** Add `now` to `PublishReport` and the new kinds to `ViolationKind`, and drop the creation guard from the guard classes, or say where one could arise.

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 28 | 1, 2, 3, 4, 5, 6, 9, 10, 11, 13, 14, 15, 16, 17, 18, 20, 23, 24, 26, 28, 29, 30, 31, 32, 34, 37, 38, 40 |
| changes-value | 12 | 7, 8, 12, 19, 21, 22, 25, 27, 33, 35, 36, 39 |
| wording | 3 | 41, 42, 43 |
| **Total** | **43** | |

## 4. Metrics, as the reader wrote it

**What I read.** `docs/design/declaration-syntax.md` §6.9, §6.10, §6.11, §8.2 and §8.3; `docs/design/flow-format.md` §4.9, §5 and the metric rows of §6; `docs/DESIGN.md` §5.12, §5.13 and §7; `docs/design/library-api.md` for the `metric` operation, `MetricRow`, `MetricPage`, `RaisedFlag` and the bullets after the protocol; and `docs/design/metric-scenarios.md` whole. **Where I left those sections**, following a citation the text needed: `declaration-syntax.md` §4.2 (backdating, for the order of a closed stretch against a member's span), §6.4 (correction, for `.corrected_by` and `.held`), §6.6 (migration mappings, for `.migrated` spans and removed states), and check 56 in §10; `DESIGN.md` §5.2 (when a write opens an interval) and §5.5 (when a would-be refusal is recorded); `flow-format.md` §4.8 (the refusal order, for an invariant violation's clause) and §4.14 (families). To find passages, I searched the files with `grep` for `ADR-0146`, `hop`, `resolution` and `would-be`. **What I did not read**: anything under `docs/adr/`, `defects.md`, `TODO.md`, `LESSONS.md`, any review file, the case studies, `scripts/`, `.remember/`, git history, and also `returns-module.md`, `unit-journey.md` and `publish-and-import.md`, which the scenarios cite but which are outside the documents I was given. So where a value turns on a category those modules declare (whether the returns' `RESOLVED` is `closed`), I give it both ways. **Arithmetic** was done in Python (`datetime`, `fractions`, `decimal`). **Every quotation below was checked by a script** that pulls each `> \`file\` §…: "…"` line out of this report and asserts that the quoted text occurs character for character in the named file. Anything I say about PostgreSQL I did not run, and it is marked *(from memory)*.

### What is determined

Most of the area is settled. I recomputed every metric value stated in `metric-scenarios.md` §3 to §43 under the current text, though not the reads that are not metrics. All of them hold except D03's span in a closed state (finding 1). These are settled:
- **The eight sources and their row members.** That `count()` counts rows and every other aggregate reads only the rows whose body is present. That a group whose rows all lack a body is still reported, with the value its expression has over no rows: 0 for `count` and `sum`, absent for the others.
- **Nearest rank over distinct values.** `median` is the lower middle. p80 over 6 is the 5th and over 3 is the 3rd, as in §16.
- **Rounding.** Half to even, once, to the scale of the expression's type. An `avg` of integers is `decimal(19,6)`. An `avg` inside a larger expression is not rounded first.
- **Buckets.** UTC, ISO weeks from Monday, and the string forms. 2026-12-31 falls in 2026-W53.
- **Set-valued dimensions.** Each distinct value is counted once. Two dimensions through one set are taken per member. A member that reaches no value counts under the absent value too.
- **Windows.** A window is closed at both ends. A row whose timestamp is absent is in no window. A split metric's pieces are clipped before the filter reads them.
- **Broadcast in a combined metric.** An input lacking a dimension is broadcast at the declared grain, and a group that only the broadcast input has gives no row. A reader's `keep` aggregates each input before combining.
- **Flags on an unknown value.** A flag whose condition is unknown is not raised and is reported as unknown.
- **`transition_count`.** It counts by name and excludes imports, migrations and redactions. `median_time_in_state` counts each span, so an object that entered twice counts twice.
- **The standard metrics' table rows.** Their filters match the instantiated block, except for dimension order (finding 24).

### Findings

#### 1. Does the closed-stretch rule reach ended spans? The scenario says no; the rule's own example says yes

- **Construct:** `.duration` of a span on an object now in a `closed` or terminal state.
- **Case:** D03 of `metric-scenarios.md` §2: PREPARATION from 14 Sep to 15 Sep. `complete_sale` puts it in DELIVERED (closed, not terminal) on 15 Sep. `revoke` puts it in CANCELLED (closed) on 21 Sep. Now is 2026-09-28. This is exactly "a job moved from done to archived". The DELIVERED span began within the unbroken closed stretch D03 is now in, which runs from 15 Sep.
- **The readings:**
  - (a) The rule governs only a span still current. The DELIVERED span has ended, so it is exit less entry, **6d**. `Delivery.time_in_state` gives DELIVERED = 6d, and `delivery_wait` gives (DELIVERED, C-ACME, absent) = 6d. These are the scenario's values in §3.1, §3.2 and §3.5.
  - (b) The rule governs every span of the object, as "a job moved from done to archived accruing nothing in between" requires. The DELIVERED span began within the stretch, so it has **no duration**. Both groups are still reported, with an **absent** value.
  - (c) Reading (b) applied literally, "runs to the stretch's start", stretches an *ended* span that began before the stretch out to the stretch's start, unless the builder clips it at its exit. In §27, T3 waited in AWAITING_PARTS from 14 Dec 02:00 to 18 Dec 09:00. Under (c) that span runs on to the start of T3's closed stretch: 19 Dec 09:00 if `RESOLVED` is closed, giving 5d 7h, or 28 Dec 00:00 if it is not, giving 13d 22h. `Return.time_in_state` for AWAITING_PARTS would then be 5d 7h or 13d 22h, not the stated **4d 7h**.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "`.duration` is a span's exit less its entry. A span still current runs to `now` while its object is open."
  > `declaration-syntax.md` §6.9: "On an object now in a `closed` or terminal state, one that began before the unbroken stretch of such states the object is now in runs to the stretch's start, and one that began within it, like the span of the current state, has no duration; so nothing accrues on a finished object, a job moved from done to archived accruing nothing in between, and no duration is ever negative (ADR-0101, ADR-0146)."
  > `metric-scenarios.md` §3.1: "D03's DELIVERED span counts because D03 left DELIVERED, when the revoke cancelled it: a closed state holds no *current* time, but a stay that ended is a span like any other."
  > `metric-scenarios.md` §3.2: "Delivered has four spans and one duration, D03's 6d, so its median is 6d."
  > `metric-scenarios.md` §3.5: "| DELIVERED | C-ACME | absent | 6d | |"
  > `metric-scenarios.md` §12: "`DELIVERED` is closed but not final"

  "one that began" has no noun of its own. It can refer back to "A span still current", giving (a), or to any span, giving (b). The scenario computes (a), and the example in the same sentence requires (b).
- **Severity:** changes-value.
- **Suggested fix:** State whether the rule covers ended spans. If it does, say that a span which began before the stretch ends at the earlier of its exit and the stretch's start. Then correct §3.1, §3.2 and §3.5 of the scenarios, or else the example.

#### 2. DESIGN.md stops an open span at the entry into the current state, not at the start of the stretch

- **Construct:** a member span still open on a closed object, such as `time_with_assignee`.
- **Case:** a job with engineer Ben from 1 Sep. It enters DONE (closed) on 5 Sep and ARCHIVED (closed, terminal) on 10 Sep, with Ben still named. Now is 20 Sep.
- **The readings:**
  - (a) `declaration-syntax.md`: Ben's span runs to the stretch's start, 5 Sep, giving **4d**.
  - (b) `DESIGN.md`: the span "stops at its entry into that state", the state the object is in, ARCHIVED, giving **9d**.
- **Where the text fails:**
  > `DESIGN.md` §5.12: "a span still open on an object in a `closed` or terminal state stops at its entry into that state, so an engineer still named on a closed job accrues no time;"

  DESIGN.md was not brought into line with the stretch rule of finding 1. It also speaks only of spans "still open", which supports reading (a) of finding 1.
- **Severity:** changes-value.
- **Suggested fix:** Restate DESIGN.md §5.12 in terms of the unbroken closed stretch, and in the same scope as finding 1's answer.

#### 3. `time_in` on a closed object: §8.3 counts the current span to `now`; §6.9 gives it no duration

- **Construct:** `time_in(<STATE>)`, in a guard, a derivation and a metric row.
- **Case:** `do archive DONE -> ARCHIVED { require settled: time_in(DONE) >= 7 days … }`, where DONE is closed and not terminal. The object has been in DONE for 10 days. Separately, a metric with value `median(o.time_in(DONE))` over objects now in DONE.
- **The readings:**
  - (a) §8.3: the current DONE span is counted up to `now`. `time_in(DONE)` is 10d and the guard is **satisfied**.
  - (b) §6.9's rule applies everywhere `time_in` is read. The DONE span began within the stretch and has no duration. `time_in(DONE)` is either **0**, the total over no present durations, and the guard is **unsatisfied**; or it is **absent**, and the guard is **unknown**.
  - (c) The rule applies only inside a metric, where §6.9 states it. The guard is then as in (a), and the metric row as in (b).

  In the metric, 0 puts a zero into the median, while absent leaves the row out.
- **Where the text fails:**
  > `declaration-syntax.md` §8.3: "and `time_in(<STATE>)` a `duration`, the total over every span in that state, a current one counted up to `now`."
  > `declaration-syntax.md` §6.9: "`time_in` follows the same rule."
  > `declaration-syntax.md` §8.3: "`time_in` reads the clock, so an invariant may not use it (check 32), and neither may an `indexed` derivation (check 46)"
- **Severity:** changes-outcome.
- **Suggested fix:** Amend §8.3's definition of `time_in`. Say where the closed-stretch rule applies, and whether a state all of whose spans lack a duration gives 0 or absent.

#### 4. Is "began before the unbroken stretch" ordered by position or by occurred time? With backdating, a duration can come out negative

- **Construct:** a member span on a closed object, when the transition into the closed state is backdated.
- **Case:** Ben is engineer from 08:00 and the job is in WORKING from 08:00. At 10:00 `reassign` (an `act`) names Chen. At 11:00 a backdatable `finish` is requested with `occurred_at` 09:00. That is legal, since `finish` changes no tracked value except the state, whose stay began at 08:00. The stretch therefore starts at 09:00, while Chen's span began at 10:00 in occurred time but earlier in position.
- **The readings:**
  - (a) Order by position. Chen's span began before the stretch and "runs to the stretch's start", giving 09:00 − 10:00 = **−1h**. That contradicts "no duration is ever negative", so a builder clamps it to **0** or gives it none.
  - (b) Order by occurred time. Chen's span began within the stretch and has **no duration**. Ben's span, from 08:00 to 10:00, began before; it is **1h** if cut at the stretch's start, or **2h** under finding 1(a).
- **Where the text fails:**
  > `declaration-syntax.md` §4.2: "The occurred time may not precede the start of the current interval of any state or tracked value the request changes, so no interval is negative"
  > `declaration-syntax.md` §4.2: "Every transition the request takes checks the second half against what it changes on its own object, when it runs"
  > `DESIGN.md` §7: "entry and exit by position and by occurred time"

  The backdating bound protects only what the request changes. The stretch rule then compares a span it did not change against the stretch's start.
- **Severity:** changes-value.
- **Suggested fix:** Say that "before" and "within" are by occurred time, or else refuse a backdated entry into a closed state that precedes the current interval of any tracked member.

#### 5. `.held` on a span opened by a port: "when the span began" or "at that transition's position"?

- **Construct:** `i.held(engineer)` on a ported object's current interval, as in `time_in_state_by_holder`.
- **Case:** SJ1 in §2. It is OPEN from 2026-08-24, a current interval that the import event opened at the legacy time. `engineer` holds null from 24 to 26 Aug, Ben from 26 to 29 Aug (both legacy) and Chen from 29 Aug. The read is `metric(ServiceJob.time_in_state_by_holder.engineer, keep: [state, assignee])` at 2027-01-25.
- **The readings:**
  - (a) Read at the position of the transition that began the span, the import, after its writes. The holder is **U-CHEN**. The row (OPEN, U-CHEN) is the median of SJ1's 154d and J2's 6d 23h, which is 6d 23h. There is no (OPEN, absent) row.
  - (b) Read at the time the span began, 24 Aug, from the legacy intervals. The holder is **absent**. The row (OPEN, U-CHEN) is J2's 6d 23h, and there is an extra row (OPEN, absent) of **154d**.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "`.held(<member>)` is, on a span, the value a tracked member held when the span began, after the writes of the transition that began it, read at that transition's position"
  > `DESIGN.md` §7: "**A port.** The import's event opens each current interval at the time the legacy record says the object took that value, and carries that time, so one stay is never split in two at cutover."

  For a port, the span's beginning and the position of its transition are a week apart, with different holders.
- **Severity:** changes-outcome (a different set of rows).
- **Suggested fix:** Say what `.held` reads on an interval whose entry time precedes its opening event: a port, a refresh, or a migration that opens at a carried time.

#### 6. `.held` on a correction's own transition row

- **Construct:** `t.held(engineer)` on the event of a `corrects engineer` transition, as read by `acted_by_non_assignee`.
- **Case:** Chen is engineer from 08:00. At 10:00 Ben is named by mistake. At 12:00 Chen requests `fix_engineer`, which `corrects engineer` and sets Chen.
- **The readings:**
  - (a) The correction is "a transition begun in that span", so it credits the corrected value. The holder is **Chen**, and the row is an assignee acting.
  - (b) Read as "the value before the transition's own writes", taken from the correction event's recorded "value it replaces", the holder is **Ben**, and the row is a non-assignee acting.

  For a job whose only transition after creation is this correction, `acted_by_non_assignee` for `fix_engineer` is **0.000** under (a) and **1.000** under (b).
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "on a transition, the value before the transition's own writes (ADR-0146)"
  > `declaration-syntax.md` §6.4: "the index of who held what reads the corrected value from when the wrong one was written, so a span or a transition begun in that span credits the corrected value; the original write's event is never changed; the correction's event names each attribute, the value it replaces, the new value, the span it corrects, the reason and the actor; and every interval row it relabels carries `.corrected_by`, its event."
- **Severity:** changes-value.
- **Suggested fix:** Say which value the correction's own row holds.

#### 7. After a correction relabels a span, do adjacent spans of equal value merge?

- **Construct:** `intervals(<member>)` after a correction, as counted by `handoffs_by_object` and `returns_by_object` and their flags.
- **Case:** the job of finding 6. Before the correction its spans are [Chen 08–10] and [Ben 10–]. The correction relabels [Ben 10–] to Chen and itself writes Chen at 12:00.
- **The readings:**
  - (i) The relabelled span stays a separate row and the correction opens nothing, since the value now reads unchanged. There are 2 non-null spans: handoffs **1** and returns **1**, so **`returned_to_earlier` is raised**.
  - (ii) The correction's write changes the stored value from Ben to Chen and opens a span at 12:00. There are 3 spans: handoffs **2** and returns **2**, so **both flags are raised**.
  - (iii) Equal adjacent spans merge. Handoffs and returns are **0**, and no flag is raised, which is what "the corrected value from when the wrong one was written" means in substance.
- **Where the text fails:**
  > `DESIGN.md` §5.2: "A write that leaves a tracked value unchanged opens no interval, so counting a member's intervals counts its changes (ADR-0106)."
  > `declaration-syntax.md` §6.4: "the index of who held what reads the corrected value from when the wrong one was written, so a span or a transition begun in that span credits the corrected value; the original write's event is never changed; the correction's event names each attribute, the value it replaces, the new value, the span it corrects, the reason and the actor; and every interval row it relabels carries `.corrected_by`, its event."

  The phrase "every interval row it relabels" implies the row survives. Nothing says whether it merges, or whether the correction's own write opens a span.
- **Severity:** changes-outcome (flags).
- **Suggested fix:** State the interval rows that result from a correction, including whether a relabelled span merges with a neighbour of equal value.

#### 8. Spans in a state a publish removed: their group, and their category for the stretch

- **Construct:** `state = i.state` in `time_in_state`, and finding 1's stretch, after a `removed state` mapping.
- **Case:** §27. Version 10 removes the returns' `RESOLVED` and maps it to `CLOSED`. RESOLVED spans held before then remain: T2's from 20 to 21 Nov, T1's from 27 to 28 Nov, T4's, T7's, and T3's and T6's up to the migration. The read is `metric(Return.time_in_state, keep: [state])` at 2027-01-11.
- **The readings:**
  - (a) A span keeps the state it was recorded in. There is a **RESOLVED row**, for a state the current version does not declare.
  - (b) A span is read "as the state it was mapped to", as `.returns` reads it. There is **no RESOLVED row**, and the spans join CLOSED's group.

  Their durations also turn on the stretch rule. Under (b) every RESOLVED span lies in the closed stretch and has none. Under (a), if `RESOLVED` was not closed in version 9, T2's span began before T2's stretch and is 1d 1h. I could not check `RESOLVED`'s category, since `returns-module.md` was outside my reading.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "read over every interval the index holds, legacy ones included, a state a migration mapped counting as the state it was mapped to, which is rework"
  > `DESIGN.md` §5.12: "A span a migration opened is counted, the time in it being real and its state the one the mapping says the old one now is, and `.migrated` marks it, so a metric can tell it apart (ADR-0146);"
  > `declaration-syntax.md` §6.6: "Events keep the old value, which is read under the version that wrote it (check 23);"

  The mapped reading is stated for `.returns` only. The "read under the version that wrote it" reading is stated for enum members in events only.
- **Severity:** changes-outcome (a different set of rows).
- **Suggested fix:** State what `.state`, and the category used by the stretch, read on a span whose state a later publish removed.

#### 9. A span opened at the publish event, rather than by a migration: `.migrated`, actor kind and version

- **Construct:** `intervals(<member>)` for a member that a publish newly tracks, with no backfill.
- **Case:** version N adds an enum attribute `priority` to an existing type, with no backfill. Each object's `priority` interval opens "at the publish event". A user metric filters `where not i.migrated`, and its default `actor_kind` dimension is the kind of actor that "entered the interval".
- **The readings:**
  - (a) A publish event counts as a migration's opening. `.migrated` is true and the rows are excluded. `actor_kind` is the publish's actor.
  - (b) It is not a migration, since only "a span a publish's migration opened" is marked. `.migrated` is false and the rows are included. `actor_kind` is that of the publish's approver, or `unknown`.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "`.migrated` is true on a span a publish's migration opened, which the standard metrics count, the time in it being real (ADR-0146);"
  > `declaration-syntax.md` §6.6: "its current interval opens, for every object, at the event that first gives it a value in the publish, the object's migration where a backfill writes it and else the publish event"
- **Severity:** changes-value (for a declared metric that reads `.migrated` or splits by `actor_kind`; the standard metrics count the span either way).
- **Suggested fix:** Say whether a span opened at the publish event is `.migrated`, and what its `entered_by_kind` and `declaration_version` are.

#### 10. A span of zero length under `split by`

- **Construct:** splitting a span whose entry and exit are the same instant.
- **Case:** a request creates an object in A and, in the same request, a cascade moves it to B. Every cascaded transition records the same occurred time, so the span in A is [10:00, 10:00). A backdated transition dated exactly at the start of the current interval does the same, since equality is allowed.
- **The readings:**
  - (a) Half-open, the span is empty and makes **no piece**. The row disappears from a split metric. `count()` in A's group falls by one, and the group itself vanishes if this was its only row. The unsplit metric still counts it with duration 0.
  - (b) It is **one piece of duration 0** in the bucket where it began, as a span with no duration is.

  At a bucket boundary, "no piece of no length is made at a boundary" points to (a). Elsewhere nothing decides.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "each piece half-open, from its start up to and not including its end, so no piece of no length is made at a boundary (ADR-0146)"
  > `declaration-syntax.md` §6.9: "The pieces cover the span's duration as above and no more, and a span with no duration is one piece, in the bucket it began, with none."
  > `declaration-syntax.md` §4.2: "every transition the request cascades to records the same occurred time, since one request is one change (ADR-0095)"
- **Severity:** changes-outcome (a different set of rows and groups).
- **Suggested fix:** State that a span of zero length is one piece of zero duration in the bucket it began in, or else that it makes none.

#### 11. Percentile: what "rank" means under ties, and the PostgreSQL route

- **Construct:** `percentile(p, …)` and `median`.
- **Case:** values [1, 2, 2, 3] and p = 0.75, so the rank is ⌈3⌉ = 3. Separately, p = 0.07 over n = 100, where the exact rank is 7.
- **The readings:**
  - **Ties.** (a) Rank is the row number: the 3rd value is **2**. (b) Rank is SQL `RANK()`, under which ties share rank 2 and the next rank is 4: the smallest value whose rank is at least 3 is **3**.
  - **The PostgreSQL route.** "passed to either backend as an integer, which PostgreSQL computes as `percentile_disc`" cannot be done as written. `percentile_disc` takes a fraction, not a rank. A builder who passes p, or k/n, as a float gets PostgreSQL's own ceiling of a float product. In Python, `0.07*100` is `7.000000000000001`, and `(7/25)*25` is the same. PostgreSQL computes `ceil(percentile * rowcount)` in float8 *(from memory)*, so p = 0.07 over 100 rows gives the **8th** value, against the exact 7th. A builder who uses the row number on both backends gets the 7th.
  - **The publish check.** It refuses "a `p` outside `(0, 1]`", which presumes that `p` is a literal. A builder who accepts an expression for `p` cannot check it at publish, and must decide what an out-of-range runtime `p` gives.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "`percentile(p, …)` is **nearest-rank**: the smallest value whose rank in the group, ascending, is at least `p × n`, that rank being the smallest integer at least `p × n`, computed exactly and passed to either backend as an integer, which PostgreSQL computes as `percentile_disc` and SQLite by row number, so both give the same value, and a `p` outside `(0, 1]` being a publish error (check 56, ADR-0146)"
- **Severity:** changes-value.
- **Suggested fix:** Say that it is the k-th value in ascending order, with ties kept as separate rows. Say that both backends select by row number. Say whether `p` must be a literal.

#### 12. `int / int` inside a metric: "discards the remainder" against "computed exactly and rounded once"

- **Construct:** a metric value with an integer quotient.
- **Case:** `value sum(a.count) / count()` over two rows whose counts sum to 7, and `(sum(a.count) / count()) * 10` over the same rows.
- **The readings:**
  - (a) §8.3's types apply at each operator: 7/2 is **3**, and 3 × 10 is **30**.
  - (b) The value is computed exactly and rounded once at the end, to the `int` scale, half to even: 3.5 becomes **4**, and 35 stays **35**.

  For a negative quotient, "discards the remainder" also leaves open truncation against flooring.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "§8.3's arithmetic applies unchanged, which is why the pass rate above multiplies by `1.000`: `int / int` discards the remainder."
  > `declaration-syntax.md` §6.9: "It is computed exactly and rounded once, at the end, so neither an `avg` within it nor a combined metric's inputs are rounded first; a `sum` is not bounded by its element's precision; and a duration's resolution is the second (ADR-0146)."
  > `declaration-syntax.md` §8.3: "**`int / int` is `int` and discards the remainder**"
- **Severity:** changes-value.
- **Suggested fix:** Say that `int / int` truncates toward zero at the operator even inside a metric, and that exactness applies only to decimal, money and duration intermediates. Or say the opposite.

#### 13. "a duration's resolution is the second": the inputs, or the result?

- **Construct:** `sum`, `avg` and `median` of durations.
- **Case:** three spans of 0.5 s each, from timestamps carrying sub-second parts. The text states no timestamp resolution, and `library-api.md` uses Python `datetime` (inferred).
- **The readings:**
  - (a) Each span's duration is exact and only the value is rounded: 1.5 s becomes **2 s** under half to even.
  - (b) Every duration is at second resolution before it is aggregated, rounded half to even or truncated: 0 + 0 + 0 = **0 s**.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "It is computed exactly and rounded once, at the end, so neither an `avg` within it nor a combined metric's inputs are rounded first; a `sum` is not bounded by its element's precision; and a duration's resolution is the second (ADR-0146)."
- **Severity:** changes-value.
- **Suggested fix:** State the resolution of a timestamp, and say that "resolution" means the value is rounded to the second at the end. Or say the opposite.

#### 14. Does a flag read the rounded value or the exact one?

- **Construct:** `flag … when value < …`, and a guard's comparison.
- **Case:** `inspection_pass_rate` with 1,799 passes of 2,000. The exact value is 0.8995, which rounds half to even at three places to **0.900**. The flag is `low when value < 0.900`.
- **The readings:**
  - (a) The flag reads the metric's value, which is rounded: 0.900 < 0.900 is false, and the flag is **not raised**.
  - (b) The value is "rounded once, at the end", taken as at output, and the flag condition is part of the exact computation: 0.8995 < 0.900, and the flag is **raised**.

  A guard written `>= 0.900` divides the same way. Its result "has the value's type", which suggests (a), but the text never says so of flags.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "**A flag** is a named condition over `value` and the dimensions"
  > `declaration-syntax.md` §6.9: "A metric's value is rounded half to even to the scale of its expression's type, as a `set` of it would be: three places for that pass rate"
- **Severity:** changes-outcome (flags).
- **Suggested fix:** Say that a flag and a guard compare the rounded value.

#### 15. Do the "declared groups" a flag is evaluated on include the implicit `version` and `actor_kind`?

- **Construct:** flag evaluation, on a metric that does not declare `version` or `actor_kind`.
- **Case:** `time_working`, by engineer and month, with `slow when value > 10 days`. Ben's spans in September: 12d under version 1, and 3d and 4d under version 2, all entered by humans.
- **The readings:**
  - (a) Groups are only the dimensions written in `by`. The group (Ben, 2026-09) has the median of [3d, 4d, 12d], which is 4d. **Not raised.**
  - (b) Groups include the dimensions every metric has. (Ben, 09, v1, human) is 12d and **raised**. (Ben, 09, v2, human) is 3d and not raised.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "It is evaluated on each of the metric's declared groups, whatever a read keeps, and a read returns each flag raised with the group it was raised on; a flag whose condition is `unknown`, as over a group whose value is, is not raised and is reported as unknown (§8.2, ADR-0146)."
  > `declaration-syntax.md` §6.9: "**Every metric has the dimensions `version` and `actor_kind`** unless it declares them"
  > `flow-format.md` §4.9: "A flag reads `value`, the metric's dimensions, and `version` and `actor_kind`, which every metric has unless it declares them"
  > `library-api.md` RaisedFlag: "the declared dimensions' values"

  The format letting a flag read `version` points to (b). "Declared" points to (a).
- **Severity:** changes-outcome (flags).
- **Suggested fix:** State the group a flag is evaluated on, including whether the implicit dimensions are part of it.

#### 16. Flags and a read's `bind`, `filter`, `over` and `cursor`

- **Construct:** `MetricPage.flags`.
- **Case:** `metric(time_working, bind: {engineer: U-BEN}, over: 30 days, filter: "…")` on a metric with a flag.
- **The readings:**
  - (a) Flags are evaluated over the rows this read selects, at the declared grain. Only Ben's groups are evaluated, over the window and the filter.
  - (b) Flags are evaluated over every row, like a standing condition. Every engineer's raised flags are returned, whatever is bound or filtered.

  Separately, `MetricPage` carries a `cursor` and one `flags` sequence. Nothing says whether each page returns every flag, or only those for the groups behind its rows.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "It is evaluated on each of the metric's declared groups, whatever a read keeps, and a read returns each flag raised with the group it was raised on; a flag whose condition is `unknown`, as over a group whose value is, is not raised and is reported as unknown (§8.2, ADR-0146)."
  > `library-api.md` MetricPage: "each declared flag raised, or unknown, on a declared"

  Only `keep` is addressed.
- **Severity:** changes-outcome (flags).
- **Suggested fix:** Say whether bind, filter and window narrow the rows that flags are evaluated over, and how flags page.

#### 17. Binding, in a read or a guard, a dimension that a broadcast input lacks

- **Construct:** `refusal_rate`, which combines `refusals` (which has `clause`) and `transition_counts` (which lacks it), when `clause` is bound.
- **Case:** the §30 data at 2027-01-25. The read is `metric(Delivery.refusal_rate, bind: {transition: complete_sale, clause: filled}, keep: [])`. There are 6 refusals by `filled` (all the agent's) and 8 applied completions (all people's).
- **The readings:**
  - (a) The applied input is broadcast, so the binding leaves it whole: 6/(6+8) = **0.429**.
  - (b) The binding filters each input by `clause`. The applied rows have no clause, so none match and applied is 0: 6/6 = **1.000**.

  Kept by `actor_kind`, the human group has only applied rows. Under (a) that group is "only such a broadcast input" and **gives no row**, or under keep-then-combine it is **0.000**. Under (b) it is 0/0, **unknown**.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "An input whose `by` lacks a dimension the combination names contributes its one value to every group that differs only in that dimension, and a group that only such a broadcast input has gives no row (ADR-0146)."
  > `declaration-syntax.md` §6.9: "A declared dimension the reference does not bind is aggregated over: the value is computed over every row matching the bound ones."
- **Severity:** changes-outcome (a guard comparing with 0.5 decides differently, and rows differ).
- **Suggested fix:** State what binding a dimension does to an input that lacks it.

#### 18. Two inputs, each lacking a different dimension; and the format omits "gives no row"

- **Construct:** a combined metric whose inputs are A (by `site`) and B (by `model`), combined by `[site, model]`.
- **Case:** A has site S1, and B has models M1 and M2.
- **The readings:**
  - (a) Each input is broadcast across the other's values, giving rows (S1, M1) and (S1, M2).
  - (b) Every group is had only by broadcast inputs, so **no rows**.
  - (c) A publish error, if "names dimensions its inputs have" means every input.

  Separately, `flow-format.md` states the broadcast but not "a group that only such a broadcast input has gives no row". A builder working from the format alone may emit a row for a group only `applied` has, with `clause` absent, merging it with the genuine absent-clause group of `stale` and `unavailable` refusals.
- **Where the text fails:**
  > `flow-format.md` §4.9: "`group_by` names dimensions its inputs have, or `version` and `actor_kind`, which every metric has; an input without one of them contributes its value to every group that differs only in that dimension."
  > `flow-format.md` §4.9: "The inputs are joined over every group any of them has: an input with no rows in a group contributes the value its expression has over no rows, as §5 says"
- **Severity:** changes-outcome (a different set of rows).
- **Suggested fix:** Require at least one input to have every named dimension, or state the cross. Copy the "gives no row" clause into the format.

#### 19. Windowing a combined metric whose input is itself combined

- **Construct:** `over last` on a combined metric.
- **Case:** C combines X (a combined metric whose inputs both declare `window on`) with Y (which declares `window on`). A guard reads `metric(C, over last 30 days)`.
- **The readings:**
  - (a) X "declares" no `window on`, so the publish is **refused**.
  - (b) The rule applies recursively, so the publish is **accepted**, and the window selects the rows of X's inputs.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "and on a combined metric whose every input declares one or splits, where the window selects each input's rows by its own and the inputs combine over them (ADR-0124, ADR-0128)."
  > `flow-format.md` §4.9: "`input_metrics` maps the name the expression reads to a metric, `<Type>.<metric>` or a combined metric declared above"
- **Severity:** changes-outcome (the publish verdict on a guard).
- **Suggested fix:** Say whether a combined input counts as windowable when all of its own inputs are.

#### 20. `gaps`: a count, or the objects? And how a combined metric's row gets them

- **Construct:** `MetricRow.gaps`.
- **Case:** §3.2's PREPARATION row; and a combined metric's row, both of whose inputs read D08 with a gap.
- **The readings:**
  - (a) `library-api.md` returns an **int**: 1 for PREPARATION.
  - (b) `declaration-syntax.md` §6.9 and every scenario table name **the objects**: "D08".

  For a combined row, nothing says whether the inputs' gaps are **unioned**, giving 1, or **summed**, giving 2.
- **Where the text fails:**
  > `library-api.md` MetricRow: "gaps: int = 0"
  > `declaration-syntax.md` §6.9: "**Each row also reports `gaps`**, the objects for which it read something from before what their record vouches for"
  > `DESIGN.md` §5.12: "A row counts, as its `gaps`, the objects for which it read something from before what their record vouches for"
  > `metric-scenarios.md` §3.2: "| PREPARATION | 3d 12h | D08 |"
- **Severity:** changes-outcome (different gaps).
- **Suggested fix:** Make `gaps` the set of object ids (or a count, and change the scenarios), and state the union for a combined row.

#### 21. Under a window, which rows name an object whose record begins inside the window?

- **Construct:** the first gap condition, applied to a windowed read.
- **Case:** object X has `created_at` 1 May and `recorded_from` 1 Aug. A read of `transition_counts` over the last 90 days from 1 Sep starts the window on 3 Jun. X's only rows in the window fall in the group `approve`.
- **The readings:**
  - (a) Only groups that read one of X's rows name X: **`approve`**.
  - (b) X "existed then" and the window began before its record, so the read may be missing X's rows in **every** group. Every row names X.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "a window beginning before the `.recorded_from` of an object that existed then, its `.created_at` earlier"
  > `declaration-syntax.md` §6.9: "**Each row also reports `gaps`**, the objects for which it read something from before what their record vouches for"

  The words "read something" support (a). The window condition concerns rows that cannot have been read, which supports (b).
- **Severity:** changes-outcome (different gaps).
- **Suggested fix:** Say which rows a window gap attaches to.

#### 22. A reader's `over` on a metric with no `window on`, which is every standard metric

- **Construct:** `metric(name, over=…)`.
- **Case:** `metric(Delivery.time_in_state, over: 30 days)`. No standard metric in §6.11 declares `window on`.
- **The readings:**
  - (a) Refused, as the publish check refuses the same thing in a guard.
  - (b) Ignored, returning every row.
  - (c) Windowed on some implicit time.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "`over last` on a metric that declares no `window on` is a publish error (check 56)"
  > `declaration-syntax.md` §6.9: "`metric()` takes the same arguments a guard's reference does — dimension values to bind and an `over last` window"

  The check is a publish check on guards. A reader's argument is not published.
- **Severity:** changes-outcome.
- **Suggested fix:** State the read-time response.

#### 23. A reader's `filter` on a combined metric: DESIGN.md applies it; the model and the API refuse it

- **Construct:** `metric(combined, filter=…)`.
- **Case:** `metric(first_pass_yield, keep: [model, month], filter: "r.subject.customer == C-ACME")`. Both inputs bind `r`.
- **The readings:**
  - (a) Following DESIGN.md, the filter applies to the inputs' rows and a value is returned.
  - (b) Following §6.9 and `library-api.md`, the read is **refused**.
- **Where the text fails:**
  > `DESIGN.md` §5.12: "`metric()` binds dimension values, applies a window and a filter, and returns one row per group of the dimensions it keeps — all of them by default — where a guard aggregates over every dimension it leaves unbound to get its one value (ADR-0101), and a combined metric so read is computed from its inputs aggregated over those rows"
  > `declaration-syntax.md` §6.9: "A `filter` on a combined metric is refused, since it has no binder to read"
- **Severity:** changes-outcome.
- **Suggested fix:** Add the refusal to DESIGN.md §5.12. The scoping paragraph there, about how an upper layer scopes what its user may see, also needs a sentence for combined metrics.

#### 24. `MetricPage` order: which declared order, where the implicit dimensions go, and how non-bucket values sort

- **Construct:** the order of rows when `keep` is none, and value order within a dimension.
- **Case:** `metric(ServiceJob.transition_counts)` with no `keep`.
- **The readings:**
  - **Which declared order.** The table in §6.11 declares (transition, actor_kind, week). The instantiated block declares (transition, week, version, actor_kind). `refusals` and `refusal_rate` differ the same way.
  - **The implicit dimensions.** For a metric that does not declare `version` and `actor_kind`, nothing says whether they sort before or after the declared ones.
  - **Value order.** Nothing orders values that are not time buckets: states (by declaration or by name), enum members and `actor_kind` (declared order or alphabetical), transition and clause names (string order), objects (by id).

  The scenarios cannot settle this. §4's refusals table lists complete_sale before bind_slot, and within complete_sale lists clause filled, then absent, then not_internal, then filled again. That is week order, not keep order. §30 and §35 list `human` before `agent`. Page boundaries, and so the rows each `cursor` page returns, differ between builders.
- **Where the text fails:**
  > `library-api.md` §6: "a `MetricPage` ascends by the dimensions kept, in the order `keep` names them, or by the declared dimensions in their declared order when `keep` is none, a time bucket in time order and an absent value last (ADR-0127)."
  > `declaration-syntax.md` §6.11: "| `transition_counts` | `t in <T>.transitions where not t.imported and not t.migrated and not t.redacted` | `transition = t.transition`, `actor_kind = t.actor_kind`, `week = week(t.occurred_at)` | `count()` |"
  > `declaration-syntax.md` §6.11: "  by        transition = t.transition, week = week(t.occurred_at), version = t.declaration_version, actor_kind = t.actor_kind"
- **Severity:** changes-outcome (row order, and what each page returns).
- **Suggested fix:** Name the block as authoritative and align the tables. Place the implicit dimensions. Give a value order for each dimension type. Reorder the scenario tables.

#### 25. A dimension that reaches an actor's identity attribute without naming the actor object

- **Construct:** marking dimensions that name an actor.
- **Case:** `by assignee_login = o.engineer.login`, where `login` is `User`'s `identity actor` attribute.
- **The readings:**
  - (a) The value is neither "an object of a type that marks an `actor` identity" nor "an `.actor_id`", so it is **not marked**.
  - (b) It is the actor's id, so it is **marked**.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "A dimension whose value names an actor — an object of a type that marks an `actor` identity, or an `.actor_id` — is **marked** in the declaration a reader reads"
- **Severity:** changes-outcome (what `declaration()` returns, and so where an application puts its door).
- **Suggested fix:** Mark any dimension whose value is an attribute marked `actor`, or a path ending in one.

#### 26. What counts as a hop: an observation's `.subject`, and a final attribute

- **Construct:** the publish check of two hops on a dimension.
- **Case:** an observation row with `by customer_name = r.subject.customer.name`.
- **The readings:**
  - (a) `.subject` counts as a hop, as does the attribute read: **3 hops, refused**.
  - (b) `.subject` is "the object a row belongs to", like `.object`, and is not a hop: 2 hops, **accepted**.
  - (c) Only references count as hops, not a final attribute: 2 hops, **accepted**.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "A path's hops are counted from the row's object, so `.object` on a dataset row is not a hop, and a dimension path longer than two hops is a publish error (check 56)."
  > `declaration-syntax.md` §6.9: "So is a collection of the object a row belongs to — an observation's or a label's subject, an interval's or a transition's object —"
  > `metric-scenarios.md` §3.5: "a path through the set `units` and one hop more, two hops from the row's object (§6.1)"
- **Severity:** changes-outcome (the publish verdict).
- **Suggested fix:** Define a hop, and say what "the row's object" is for an observation or label row.

#### 27. Is an observing clause's would-be refusal recorded when the same request is then refused?

- **Construct:** `<Type>.attempts` rows, and a trial count such as `trial_refusals`.
- **Case:** a transition's `audit` clause fails, and a later `deny` clause then refuses the request.
- **The readings:**
  - (a) Both rows are recorded, and `trial_refusals` counts **1**.
  - (b) Only the refusal is recorded, since a would-be refusal is "linked to the event that applied" and none applied. `trial_refusals` counts **0**.
- **Where the text fails:**
  > `DESIGN.md` §5.5: "where it would refuse, the request proceeds and the would-be refusal is recorded in the attempt log, linked to the event that applied."
  > `flow-format.md` §4.9: "`attempts`, each request of the type that did not apply, each would-be refusal of an `audit` guard and each failure of a `warn` guard (§4.8)"
- **Severity:** changes-value.
- **Suggested fix:** State whether an observing clause's failure, or a flag's, is recorded on a request that is then refused.

#### 28. The clause of a refusal for a violated invariant that names several invariants

- **Construct:** `.clause` on an attempt row, as used by `refusals` by clause.
- **Case:** a request breaks invariants `a` and `b` on its object, and `c` on a part.
- **The readings:**
  - (a) One row, carrying the clause **`a`**: the first named, whose remedy is the refusal's.
  - (b) One row per invariant named.
  - (c) One row with an absent clause.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "a refusal is one row of the requested transition, carrying the clause its verdict names"
  > `flow-format.md` §4.8: "and last, invariant violated, naming each object on which an invariant fails, in ascending id order, and on each every invariant that fails"
- **Severity:** changes-value.
- **Suggested fix:** Say that the row carries the first invariant named, or else something else.

#### 29. Does an abstract base have standard metrics, and do a family's datasets belong to it?

- **Construct:** the sources `<Base>` and `<Base>.intervals`, and §6.11's "every type".
- **Case:** an abstract `Payment` with concrete `CardPayment` and `Transfer`.
- **The readings:**
  - (a) `Payment.work_in_progress` exists over the family, since a `<Type>` row is "current object of the type or family".
  - (b) An abstract type has no objects and no lifecycle, so no standard metrics.

  The datasets `<Type>.intervals`, `transitions` and `attempts` do not say "or family" at all.
- **Where the text fails:**
  > `declaration-syntax.md` §6.11: "Every type has these without declaring any"
  > `declaration-syntax.md` §6.9: "| `<Type>` | current object of the type or family |"
  > `flow-format.md` §4.14: "A type marked `abstract: true` has no objects of its own."
- **Severity:** changes-outcome (whether the metric exists, and which rows it has).
- **Suggested fix:** Say which metrics and dataset sources an abstract base has.

#### 30. "a span's whole duration is `.left_at - .entered_at`"

- **Construct:** the advice on a split piece about recovering a span's whole duration.
- **Case:** a current span, whose `.left_at` is absent; and D03's DELIVERED span under finding 1(b).
- **The readings:** the expression is determined. It is unknown for a current span, and 6d for D03's span, which has no duration. Neither is the span's whole duration as `.duration` defines it.
- **Where the text fails:**
  > `declaration-syntax.md` §6.9: "a span's whole duration is `.left_at - .entered_at`"
- **Severity:** wording.
- **Suggested fix:** Give a member for the span's own duration, or say that the expression is the raw exit less entry and is absent for a current span.

### Counts by severity

| Severity | Count | Findings |
|---|---|---|
| changes-outcome | 19 | 3, 5, 7, 8, 10, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 29 |
| changes-value | 10 | 1, 2, 4, 6, 9, 11, 12, 13, 27, 28 |
| wording | 1 | 30 |
| **Total** | **30** | |

## 5. Decisions

The author took three, one at a time, each given a sample; at the author's direction ("go with B, continue with your recommends") the rest are decided in §6. ADR-0147 records them.

1. **`time_in` is real elapsed time everywhere** (§4 finding 3). It totals every span in the state, a current one up to `now`, in a guard, a derivation and a metric row alike, so a guard such as `time_in(RESOLVED) >= 7 days` on a closed state can pass. The rule that nothing accrues on a finished object stays where it was made for, a span's `.duration` in a metric. *Rejected:* `time_in` following the closed stretch everywhere, which reads zero on a closed state and silently blocks such a guard; a meaning that depends on where the expression is written.
2. **A proposal keeps the request's occurred time and context, and its bound is measured at filing** (§1 finding 12, §2 finding 19). A proposal of a backdatable transition carries the time the change happened, held to the bound against when the proposer told the store; the check against the current interval of what it changes runs at approval, against the object as it then is. The idempotency key stays with each request. *Rejected:* the bound measured at approval, under which a proposal can only age past it and never pass; a proposal with no occurred time, which overstates time in the earlier state by the approval's delay.
3. **An engine upgrade that changes the built-ins is its own approved change** (§3 finding 31). `upgrade` gains a dry run, whose impact report says what the new built-ins change and which installed flows fail; running it is the operator's recorded act, and it installs a version naming the release as its cause, superseding every open `DeclarationChange` as a publish does. This amends ADR-0105, whose rule that built-ins change only through a publish is replaced by: only through a reported, recorded act. *Rejected:* the built-ins taking effect at the next publish, which ties the engine's fixes to a deployment's next flow change and keeps old built-ins running indefinitely; an upgrade installing them unreported, as `storage-schema.md` §12 had it.

## 6. The triage of the rest

Each item names the findings it decides, by report and number (A for §1, B for §2, C for §3, D for §4).

**Recording, correcting and dating a request**

1. (A1) A correction naming an observation of another subject is `invalid input`, listed among that verdict's causes in `flow-format.md` §4.8 and `DESIGN.md` §6 step 4; `corrects_current` requires only that nothing has corrected the observation named.
2. (A2) A correction names `subject`, as every recording does, and it must be the corrected observation's; leaving it out is `invalid input`, as for any required input.
3. (A3) `subject_open` is a stated exception to step 6: it reads the subject as the request found it, so an outcome may record on its object as it enters a final state; a subject the request created is read as it stands.
4. (A4) A correction's `.occurred_at` is the corrected observation's; its creation event, and so its `.created_at`, are the request's, dated when the correction was made.
5. (A5) `declaration-syntax.md` §6.8 lists a recording's generated guards in the format's order.
6. (A6) A creation an effect makes is a transition the request takes, so it records the request's occurred time; only the requested transition checks a bound, so a kind's `max_recording_delay` does not hold a recording an outcome makes; a correction an outcome records takes the corrected observation's `.occurred_at`, as item 4 says.
7. (A7) `DESIGN.md` §6 step 8 dates intervals by the request's occurred time for every transition the request takes; "without the marking" in `declaration-syntax.md` §4.2 concerns the requested transition.
8. (A8) A caused transition that fails the interval check refuses as any caused refusal does: a call or creation as `call refused`, naming the step and its target, with the clause `<Type>.occurred_within`; a cascade with the part's own verdict, naming the part.
9. (A9) A transition "changes" a value when the value ends different. The requested transition's state and the tracked values its inputs write are checked with the generated guards; a tracked value an effect computes is checked when the step writes it, so that refusal follows the transition's own guards.
10. (A10) `now` in every expression of a request, effects and defaults included, is its recorded time.
11. (A11) The occurred time is a field of the request, listed in `DESIGN.md` §6, and part of what an idempotency key identifies: a retry differing in it is a different request.
12. (A12, B19) Decided by §5's decision 2; a proposal also keeps the request's `context`.
13. (A13) An attempt row is dated by its recorded time, `at`, for the daily rollup and for reads; the occurred time it carries is one of the request's values.
14. (A24) An expectation's `values` may name an object's built-in members, `occurred_at`, `recorded_at`, `created_at` and the rest of §5's, and an expectation gains `occurred_at`, the occurred time the request recorded, so an example can show a transition's date.
15. (A25) A step may name a generated transition, `record` or `forget`; the Recording paragraph lists `corrects`; a timestamp value is `YYYY-MM-DDTHH:MM`, optionally `:SS`, then `Z`, or `now` with an optional `+` or `-` and a duration.

**Evaluators**

16. (A20) Consultation is judged from committed state: the evaluator guards of the transitions the request would reach as committed state stands, a call's, a creation's, a cascade's and each loop element's that the `where` selects, each consulted per element with that element's arguments; a part a cascade passes over and a call a `where` leaves out are not consulted, and a function no consultation reaches needs no stub.
17. (A21) A function without `fresh` uses only an answer obtained for this request, whatever its stamp; with `fresh`, the as-of time the evaluator reports is compared with the request's `now`; an answer reporting none is taken as of its consultation; a stub is live.
18. (A22) A verdict too old for `fresh` leaves the guard unknown, refused as `temporal`, as an evaluator that does not answer does.
19. (A23) Check 61 is stated once, in the model, and cited by `DESIGN.md` §5.5 and `flow-format.md` §4.18: an argument reading a value that an outcome applied earlier in the same request writes, through a caller's input, a path or a whole's cascade, is a publish error; a transition's own writes, which follow its guards, are not.

**Attribute corrections**

20. (A14, A15, D7) A correction relabels the current interval of each attribute it lists, from that interval's start; it opens no interval of its own, and a relabelled interval equal in value to the one before it merges with it, so a correction never counts as a change of holder. A merged or relabelled row carries the latest correction that reached it. A correction's own occurred time dates its event only.
21. (A16) Every interval row whose value or `.held` a correction changes carries `.corrected_by`, a state span included.
22. (A17, D6) A correction's own transition row credits the corrected value, being begun in the span it corrects.
23. (A18) A correction is a write, so `changed_since` counts it.
24. (A19) `DESIGN.md` §7's provenance row reads "an internal or external transition marked `corrects`", and an observation correction's event carries `corrected` too.

**Erasure and supersession**

25. (B1) `DESIGN.md` §5.6 takes §8's rule: an erasure admits only an invariant it leaves violated.
26. (B2) An erasure's admission is recorded on the object the invariant fails on.
27. (B3) `not_erased` is decided with the generated guards, from the inputs the request supplies and the effect steps that assign or add to a personal attribute, whatever they compute; `clear` writes absence and is not refused; `declaration-syntax.md` §6.4 says "a value that is not absent".
28. (B4) A group's other members are erased after the requested erasure's effect and cascades, in ascending id order, each once per request, as caused transitions recorded as the requester's, with the requested erasure's event as cause; a member's refusal refuses the request as a caused one does, its step named "group erasure".
29. (B5) Erasures "requiring the same inputs" have the same input names, types and optionality; a type declares at most one erasure (step 3); a member receives the inputs by name, and `only_via` does not bar the erasure a group causes.
30. (B6) A type a supersession joins that holds nothing personal, and so declares no erasure, is passed over by the group's erasure; `flow-format.md` §4.19's rule binds the types that declare one, and check 60's row says so.
31. (B7) Erasing again runs the whole erasure again: it redacts every personal value present now, a new part's and a new group member's included, invalidates proposals filed since, and records its event naming what it retried and what it newly erased, either of which may be nothing.
32. (B8) The reach is per written value: a value is personal where the version that wrote it marked its attribute personal. Check 60 counts an attribute personal under any version of the type, so a type keeps an erasure once it has needed one.
33. (B9) A publish's redaction of a newly personal attribute on an erased object records on it a `redact` event, source `erased`, caused by the publish event, with an erasure's admission of what it leaves violated.
34. (B10) A backfill skips an erased object's personal attributes, as the import does.
35. (B11) An object whose past event an erasure redacts gets an `erased` event and a new version, as one whose attribute it redacts does.
36. (B12) An erasure invalidates every pending proposal that targets the erased object or carries inputs flowing into its personal attributes, redacting only the personal inputs, in one event per proposal.
37. (B13) A generated guard names its stated objects even where one is the object evaluated on: `successor_not_superseded` names the successor.
38. (B14) An assertion's `to` may not list a superseding state (check 26); a merge is a declared transition.
39. (B15) `history(id, follow)` returns the events of the whole group, every member whose chain ends where the named object's does, in position order.

**Proposals, flow changes, delivery and the API**

40. (B16) Filing a proposal checks what a request checks short of the state and the guards: an unknown transition, one not proposable (refused as `not requestable`), an invalid input and a required one left out are refused as a request would refuse them. A creation may be proposed; "the object exists" holds for any other.
41. (B17) A publish that removes `proposable` from a transition invalidates its pending proposals.
42. (B18) A refused approval's attempt row is the proposed transition's, on its object, carrying the clause and naming the proposal; an applied approval's `Satisfied` names the target object, its event first, then the proposal's.
43. (B20) An expired proposal refuses approval as `unavailable`, naming the state `expired`, from its expiry instant on; it may still be rejected or withdrawn, and it counts as pending for a publish's invalidation.
44. (B21, B24) `Proposal` and `Subscription` are built-in types declared as `DeclarationChange` is, their lifecycles printed: a proposal is filed by a request marked `propose`, decided by its transitions `approve`, `reject` and `withdraw`, and ended by `invalidate`, which a publish or an erasure causes; a subscription is created by `subscribe` and ended by `revoke`. `pull` and `acknowledge` refuse a revoked or unknown subscription with a verdict value, and moving the acknowledged cursor back is described as what it does, not as reviving.
45. (B22, C32) A publish supersedes every other open change.
46. (B23) The `DeclarationChange` lifecycle is printed as a flow description declares one: `revise` internal at `DRAFTED`, `reopen` external from `SUBMITTED` to `DRAFTED`, the remedies of `submit`'s refusal and of `current` and `impact_unchanged` stated, and `refresh` refused while its dry run finds anything fatal.
47. (B25) A new subscription's acknowledged cursor is the log's settled cursor at its creation; a cursor is exclusive; a page with no event hands back the settled cursor past what the filter skipped.
48. (B26) The thresholds are `lagging_after` and `dead_after`; with nothing pending the lag is zero; a subscription is lagging or dead from the moment its lag reaches the threshold; `get` of a subscription returns its lag and its condition.
49. (B35) `invalidated` joins the examples schema's proposal states, since an erasure in an example can invalidate one, and a setup's approval that is refused fails the setup, since its step did not apply.
50. (C31) Decided by §5's decision 3.

**The model's details**

51. (B27) Uniqueness is partial where any attribute it ranges over, or its scope, is absent, as a unique index treats nulls.
52. (B28) A type's invariants are ordered: its own declared ones as written, a required invariant a machine names among them; then inherited declared ones in the base's order; then those a bound machine declares, in its order; then the generated, every `<state>_invariant` by state in declaration order, then every `<attribute>_unique` by attribute in declaration order, inherited attributes first.
53. (B29) An argument to `changed_since` names attributes and ends, not references, and contributes no object; inside an aggregate only the outermost aggregate's selected elements contribute.
54. (B30) `cascaded` lists every transition the request took on another object, in the order each began: calls, creations, cascades, a group's erasures, and the generated `forget` an erasure runs on each observation, after its declared steps; a `redact` event and a proposal's invalidation are not transitions the request took.
55. (B31) `DESIGN.md` §6 steps 5 and 6 are restated: the parent's new state is written, then its effect runs in step order, writes, calls and creations interleaved, then its cascades.
56. (B32) A bound name may be neither a member of the type nor a word an expression resolves first or every object has (`state`, `inputs`, `actor`, `this`, `now`, `referrers`, `this_event`, `id`, `open`, `created_at`, `created_by_kind`, `recorded_from`, `declaration_version`), and a nested loop may not reuse its loop's name; `flow-format.md` §4.8 states it beside the steps.
57. (B33) A loop over values runs in ascending value order, strings by code point and numbers by value, and `range` from 1 up.
58. (B34) A type with no `summary` returns its id and state by default; a family query returns each object's own type's summary; an abstract base may extend another.

**Publishing and migration**

59. (C1) A rename writes no event and names no object; the migration's "renames first" means its other mappings read the new names.
60. (C2) A backfill is applied to every object of its type; an object whose row it leaves unchanged gets no event and is named in no category.
61. (C3) A publish's migration writes a mirror beside the import and its erasure, since the declaration is this store's: it moves a mirror's objects into the new version's states and members, and its `admit` may name a mirror's invariant.
62. (C4) A violation an admission already covers is no decision, and the change's `admit` adds none on that object.
63. (C5) The dry run also reads every type whose invariant reads a type the change touches, and the publish checks every invariant once, after all the mappings, over every object a mapping wrote and every object whose invariant reads one, as step 7 does.
64. (C6) Each `CHECK` is widened to the union of both versions' values before the mappings and replaced by the new version's after.
65. (C7, D9) `storage-schema.md` §10's tracked-member row says what step 5 says; a span a publish opens, at a migration or at the publish event, is `.migrated`, entered by the publishing actor's kind under the new version.
66. (C8) The publish and migration events are recorded, and occur, at the approval; the dry run's `now` serves its evaluation only, and `PublishReport` records it.
67. (C9) A clause newly enforced is any `deny` clause the version adds, an `audit` or `warn` made `deny` included.
68. (C10) A clause or transition whose verdict depends on an input, an evaluator or a metric is classified as `availability` lists it, available with input or not evaluated, and is neither lost nor refusing; a removed transition and a narrowed `from` are losses.
69. (C11) `affected` is keyed per category, a mapping's per mapping and a violation's per invariant; `DESIGN.md` §9 gains the exemption for superseded changes.
70. (C12) An unresolved decision at approval is `Unsatisfied` naming `decision_unresolved`, and a fatal finding at the recompute `Unsatisfied` naming `fatal`.
71. (C13) Step 3 reads observations for a removed member and for a kind's invariants; a kind invariant changed under its name counts as new; a publish's `admit` may not name a kind's invariant, while an import's `ADMIT` may.
72. (C14) A dropped attribute and an added one of its type are a notice, `migration`, and check 23's row says so.
73. (C15) A part is stranded when it is not in a final state and its end does not declare `survives: true`, a `survives` list naming no migration; harness row 4 agrees.
74. (C26) A mapping on an abstract base reaches every object of its family, and a concrete type's mapping may name an inherited member.
75. (C27) A removed member mapped into one a set already holds leaves the set with it once.
76. (C28) A backfill leaving a required attribute absent is fatal in the dry run; `NOT NULL` is applied after the mappings, by a table rebuild on SQLite.
77. (C29) Making an attribute required counts as a new required attribute; an attribute's type does not change in place, a change being a drop and an add.
78. (C30) A retired type's objects are not migrated, and its table's `CHECK`s stay as they were.
79. (C33) Compilation follows the standing admissions: an invariant is uncompiled while an admission of it stands, and compiled at the next publish after its last is discharged.
80. (C34) A mirror's invariants are never compiled.
81. (C35, D8) A span keeps the state it recorded, read under the version that recorded it, a removed state included; `.returns` is fixed when its event is recorded; a mapped state counts as the state it maps to only on the span a migration opens.
82. (C36) The publish's notice is per object, an `admit` on an object in a final state; the flow checker's is per invariant, one that fails only in a final state; "nothing can discharge" becomes "no transition can discharge".
83. (C38) A backfill reads the object's stored attributes, derived attributes over them, and the built-in members of `flow-format.md` §5; its `declaration_version` is the version before the publish.
84. (C39) `of_migration` gains an ordinal, and one change may not admit an invariant twice.
85. (C41) `DESIGN.md` §5.12's override counts count every override but the import's.
86. (C42) Each mapping refusal is one row of step 3 and one clause of check 23.
87. (C43) `PublishReport` gains `now`, `ViolationKind` the two classes, and the guard classes lose the creation guard.

**The import and the harness**

88. (C16) An import may leave a live part under a final whole, as the legacy record has it; harness row 4 exempts mirrors, as row 1 does.
89. (C17) The built-in assertion evaluates no generated guard.
90. (C18) A refresh's legacy times are compared per dimension.
91. (C19) A refresh changes nothing when its current values, intermediate values and legacy entries are those the store holds.
92. (C20) The client skips an arrived batch, `lookup` telling it; a refresh sending again intervals and entries the store holds leaves them as they are.
93. (C21) An interval beginning at the port records the kind `unknown`; a named changer the store holds no actor for is recorded by identity with the kind `unknown`; an imported observation's `.recorded_by_kind` is the legacy recorder's where the mapping names one, and `unknown` otherwise.
94. (C22) `recorded_from` is the latest of the times that apply, over every tracked dimension, and an object none describes takes the earliest entry of its intervals; `DESIGN.md` §5.12 agrees.
95. (C23) No legacy interval may overlap a declared silent span at all, and the import refuses rather than trims.
96. (C24) The two classes ADR-0146 added are of kind `REFERENCE` and `FIELD`, with the disposition `DROP`, which clears the optional reference or drops the field, beside `EXCLUDE`; "already holds" includes an object earlier in the same batch.
97. (C25) An `EXCLUDE` class holding an object the store already holds leaves that object as last imported, and the dry run reports it.
98. (C37) The import matches by the legacy key, which is not personal and survives erasure.
99. (C40) Harness row 1 reads the installed version, so a cutover must leave every ported object in a state its owned declaration reaches.

**Metrics**

100. (D1, D2) The closed-stretch rule covers only a span still current, as the scenarios compute it; `DESIGN.md` §5.12 says so, and the misleading example becomes an engineer still named on a job moved from done to archived, who accrues nothing after done.
101. (D3) Decided by §5's decision 1. *(This corrects `cold-reading-review.md` §5 item 55, which had `time_in` follow the closed-stretch rule.)*
102. (D4) "Before" and "within" the stretch are by occurred time.
103. (D5) On a span whose entry precedes its opening event, a port's or a refresh's, `.held` reads the interval index at the entry, legacy intervals included, and is absent where none covers it.
104. (D10) A span of zero length is one piece of zero duration, in the bucket it began.
105. (D11) A percentile is the k-th row in ascending order, ties kept as separate rows, k the smallest integer at least p × n, selected by row number on both backends; `p` is a literal.
106. (D12) `int / int` truncates toward zero at the operator, inside a metric as everywhere; exactness concerns rounding, which happens once.
107. (D13) A timestamp is held to the second, the engine reading its clock to the second, so a duration is whole seconds and exact.
108. (D14) A flag and a guard compare the rounded value.
109. (D15) A flag's groups are the dimensions `by` declares, and a flag may read `version` or `actor_kind` only where `by` names it.
110. (D16) Flags are evaluated over the rows a read selects, bound, windowed and filtered, at the declared grain, and each page returns the flags of the groups its rows draw on.
111. (D17) Binding a dimension a broadcast input lacks leaves that input whole.
112. (D18) One input of a combined metric has every dimension the combination names (check 56), and `flow-format.md` §4.9 states that a group only a broadcast input has gives no row.
113. (D19) A combined input is windowable where all of its own inputs are.
114. (D20) `gaps` is the set of object ids, and a combined row's is the union of its inputs'.
115. (D21) A window's gap attaches to the rows that read the object.
116. (D22) A reader's `over` on a metric that declares no `window on` and does not split is refused as invalid.
117. (D23) `DESIGN.md` §5.12 states that a reader's `filter` on a combined metric is refused.
118. (D24) The standard metrics' block is authoritative and the table follows it; implicit dimensions sort after the declared; values sort by declaration order for states, enumeration members and kinds of actor, by code point for strings, and by id for objects.
119. (D25) A dimension ending in an attribute marked `actor` is marked.
120. (D26) A hop is a reference followed; the final attribute read is none, and `.subject`, like `.object`, is none.
121. (D27) An observing clause's would-be refusal and a raised flag are recorded only on a request that applies.
122. (D28) A refusal for violated invariants carries the first invariant named as its clause.
123. (D29) An abstract base may declare metrics over its family's rows, and has no standard metrics; its datasets range over the family.
124. (D30) "a span's whole duration is `.left_at - .entered_at`" becomes its raw exit less entry, absent for a current span.
