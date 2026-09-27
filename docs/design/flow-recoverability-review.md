# Can a flow builder recover the flows from the specification and the YAML?

Status: **review, findings resolved**, 2026-09-27. Written at the author's request: "review the use case examples, both the inventory and the jira flow, assume you're a flow builder, given the specs yaml, can you recover the flow without causing any ambiguities". The answer was **not yet**: most of each flow was recoverable, and gaps in the specification and defects in the examples are recorded below as D395 to D409. The author accepted every recommendation the same day ("go ahead with your recommendations"); ADR-0122 records the decisions, D397's included, the examples are corrected, and a second review by fresh readers checks the result (§5).

**Method.** Two readers with no context of the design reconstructed the flows as a builder of an engine would, each given only what a builder gets: [`flow-format.md`](flow-format.md), the parts of `declaration-syntax.md` and `DESIGN.md` it cites, the schema, and the YAML under test. One read the robot inventory module of [`unit-journey.md`](unit-journey.md) §2, the other the twenty flows of [`jira-flows.md`](jira-flows.md) with their diagrams; neither read the prose around the YAML, the decision records or the case studies. Each wrote down its reconstruction, and every point where it had to choose between readings, quoting the specification that failed to decide. Every finding below was then checked against the cited text by this record's author, and the two the checker was said to pass were probed: a set reference with no opposite (D406) and a loop whose `where` tests an input (D405) both pass all four steps.

**What is recoverable.** Every state, category and final state, every transition's sources, target and `only_via`, the guards and their order, the effects and their order, invariants and when they are checked, derived attributes, and identifier formats are recovered by both readers exactly as intended. Of the robot inventory's 81 transitions, 62 are fully determined; of the twenty Jira flows, fourteen are, and all twenty diagrams match their modules under the stated notation.

## 1. Gaps in the specification

Each is a question two careful builders could answer differently, so their engines would accept or refuse different requests, or compute different values. The recommendation is this record's, and the author accepted each on 2026-09-27; ADR-0122 records them as decisions.

| | Question | Where it bites | What the text says | Recommendation |
|---|---|---|---|---|
| D395 | What a loop's or a cascade's `limit` counts: the whole collection, or the elements `where` selects; and whether a part passed over in a final state counts | the inventory's `retire`, `swap_unit` (`limit: 1`), the engagement's cascade (`limit: 20`), `commit`; the Jira sprints and versions | `flow-format.md` §4.8: "runs its steps for each element of a collection that satisfies `where` … `limit` … bounds that loop"; §4.3: "`limit` bounds the parts reached in one request"; the schema says "at most limit times", where the prose refuses | a loop's limit counts the elements that satisfy `where`, a cascade's the parts not passed over, and exceeding it refuses; correct the schema's wording |
| D396 | When a loop's collection is evaluated, when its own steps change it | `Delivery.cancel` and `revoke`, `ServiceJob.cancel` and `void`, whose calls remove each element from the collection they loop over | `DESIGN.md` §6 runs steps "over collection elements in ascending object-id order", guards "against the state produced so far"; nothing says when the collection is read | the collection and `where` are evaluated once, when the loop starts |
| D397 | Whether a transition `from: any` taken in its own target state re-enters it: a new interval, and `entered_at` now | every free-moving Jira workflow (`to_do`, `start`, `done`, `select`, `to_backlog`) | `flow-format.md` §4.8: `from: any` includes "the target state", and an external transition is UML's, which "exits and re-enters the state" (Appendix A); `DESIGN.md` §6 step 8 updates intervals "for every changed state" | the author's to decide; either `from: any` means every *other* state that is not final, or a self-transition re-enters, and the interval rule says so. Decided: the first (ADR-0122) |
| D398 | Whether a creation may omit a defaulted attribute it lists in `required_inputs` | `RobotModel.add` (`label_photo_required`, `manufacturer_serial_required`), `Delivery.open` (`internal`) | `flow-format.md` §4.17 says the input is taken "as the attribute's optionality says", so it is required; `declaration-syntax.md` §5.1, which the form converts to, says "An accepted attribute not supplied to a `create` takes its default". The format contradicts the model it defers to, and the sentence is this record's own | follow the model: a creation may list a defaulted attribute in `optional_inputs`, and one not supplied takes its default; correct §4.17 with a note, and move the examples' defaulted inputs |
| D399 | Whether a relationship of two single ends is one-to-one | `Lease.engagement` (stored) and `Engagement.lease` | `declaration-syntax.md` §3.3 says which end stores the value, and nothing about how many objects may store the same one | a stored single end whose opposite is single is unique, as a uniqueness invariant generated on it |
| D400 | What a combined metric does with the input dimensions its `group_by` leaves out, and with `version` and `actor_kind` | `pool_utilisation`, whose inputs also have `month` and `kind` | `flow-format.md` §4.9: "joined by their dimensions"; "`group_by` names dimensions its inputs have" | each input is aggregated to the `group_by` dimensions before they are combined, and `version` and `actor_kind` are kept only when named |
| D401 | Which group a row falls in when a dimension's value is absent | `Robot.retirements` by `reason`, absent for a unit an assertion retired | `declaration-syntax.md` §6.9 says an aggregate leaves out a row whose *body* is absent, and nothing of a dimension | an absent value is a group of its own |
| D402 | A sequence's first number, its step, and whether a refused creation uses a number | `unit_serial` and every identifier | `flow-format.md` §4.12 names SQL's `CREATE SEQUENCE`; `DESIGN.md`'s glossary calls a sequence "monotonic" and "not gapless" | numbers start at 1 and rise by 1, and a refused creation may leave a gap, as the glossary already says |
| D403 | Which verdict refuses a request or a call from a state its transition does not leave, a reference input naming no object or one of another type, and a set input with a repeated element | every transition; `Shipment.dispatch`'s `with_units` | `DESIGN.md` §5.5 lists satisfied, unsatisfied, `stale`, `not found`, `not requestable`, `over-limit` and invariant violated | a verdict for each, with its remedy; a set input's repeated element refused |
| D404 | Whether an explicit null for an optional input is a value or absence | every optional input | "each is written if supplied" (`flow-format.md` §4.8); `clear` "is the only way" to write absence (`declaration-syntax.md` §5.2) | a null is absence: the write is skipped, and only `clear` removes a value |

## 2. Gaps in the checker and the notation

| | Finding | Verified by |
|---|---|---|
| D405 | A loop whose `where` tests an input makes a step conditional, which `DESIGN.md` §5.4 forbids ("no step is conditional … declare one transition per case") and the model's check 48 refuses for an unsupplied optional input inside a step's expression; all four steps pass it. The Jira catalogue's sprints and versions use it (D409). | the catalogue's modules pass `scripts/check-flows.py` |
| D406 | A set reference with no `opposite` passes all four steps, although `flow-format.md` §4.3 says such an end "MUST be single" and the model's check 41 refuses it. A machine's required set end is the exception the text does not state: its opposite is on the binder. | a probe on copies of `delivery.yaml` and `service.yaml`, both passing |
| D407 | The diagram notation promises "the guards that refuse it", and the generated `<attribute>_provided` guards are not drawn; nor is an erasure, which runs at a final state; nor which types of a module are drawn. | `jira-flows.md` "Reading a flow", `scripts/flow-diagram.py` |

## 3. Defects in the examples

These are fully determined by the specification and wrong: each leaves an object stuck or does what its description says it does not.

**D408, the robot inventory** (`unit-journey.md` §2, and the flow review's appendix, which carries the same module):
1. `discard` keeps the unit's `peg`, and `peg_to` may be taken at `MISSING`: a unit pegged while missing and then discarded is `CANCELLED` and still pegged, so its delivery's `filled` never holds, and `Delivery.cancel` fails on `unpeg`, which a cancelled unit cannot take. The delivery can neither complete nor be cancelled.
2. `accept_return` keeps `used_in`: a part consumed by a service job and returned is `AVAILABLE` and still the job's part, so `ServiceJob.void` fails on `unconsume` and binding it to a delivery breaks `one_claim`.
3. `convert_lease` keeps the `binding` an internal delivery wrote, so revoking that delivery calls `unsell` on a unit sold by a lease; and `recall_internal` has no `home` guard, so it takes a unit on loan back to stock with its line still open.
4. `Engagement`'s `nonempty` and `fit` read ended lines, so an engagement whose unit was retired while it was scheduled can never be dispatched.
5. `pristine` blocks `revert_commit` for ever once a committed unit is deleted.
6. `swap_unit` with a unit that is not out on the engagement adds the incoming unit as a new line.

**D409, the Jira catalogue** (`jira-flows.md`):
1. The sprint's `complete` and the version's `release` and `delete` choose between two behaviours by a loop's `where` on an input (D405); each is two transitions in the model's terms. The sprint's note cites `case-study-payments.md` §5 for the idiom, which says the opposite; corrected in place, with a note.
2. The version's `delete` and `release` call a transition on every work item, archived ones included, which cannot take it, so one archived work item refuses them for ever.
3. `jsm_service_request_approvals` stores `approvers` as a set reference with no opposite (D406).
4. Task and process management cascade their archive to at most 100 subtasks, and nothing bounds how many are added, so a whole with more cannot be archived.
5. The sprint's `dated` guard is `self_serviceable`, "supply it", on a transition that takes no input; it is `unreachable_from_here`.
6. The Scrum and Kanban workflows keep a Done resolution when a work item leaves Done, the defect the model's `clear` exists to prevent (`declaration-syntax.md` §5.2).
7. No guard checks a target: a sprint completes into itself or a closed sprint, and a version is deleted or released into itself or a deleted one.
8. The problem's `complete` describes a known error as one with a root cause and a workaround, which nothing enforces.

## 4. Checked and dropped

- An assertion leaving a final state: the model's §4.2 settles that only an erasure runs at one.
- A loop truncated at its limit: the prose refuses, and governs the schema's wording (D395 corrects the wording).
- The order of guards and required inputs, and a traversal invariant re-checked when an object it reaches is written: settled by `flow-format.md` §4.8 and §6 and `declaration-syntax.md` §3.4.
- Category lists that differ across an import: `flow-format.md` §4.1 has each module list the categories its states use, and the model's vocabulary is one across the closure, so they need not match.
- Archiving an untouched work item counts as completing work (`declaration-syntax.md` §1): determined, and a question for the metrics rather than an ambiguity; `TODO.md` holds it.

## 5. The second review

After the corrections, two fresh readers under the same restrictions checked each earlier finding against the current text and reviewed the modules afresh. Every earlier finding was resolved, except that a required input of a defaulted attribute still converted to one the model lets be left out (D411) and the journey's import of `Customer` broke the new rule against re-exporting. The fresh review found seven more (D410 to D416): the journey's imports put two `Robot` types in one closure; refusals had no order and two had no verdict; `unreachable_from_here` had two meanings; a combined metric's join and an aggregate over no elements were unstated; defects of both examples, one of them an engagement that could never end past 20 units; and the schema and the declaration order lagged the prose. ADR-0122, amended, and corrections in place resolve each; a third review checks them (§6).

## 6. The third review

Fresh readers checked the second round's findings against the text alone: all resolved, but that the engagement lines' descriptions still misstated when a line is open. The Jira reader found 18 of the 20 flows fully determined and the inventory reader 83 of 85 transitions, and no object stuck but one path. That path was the largest finding: an assertion moved a unit and left its part claim behind, so a service job could never finish (D420). The rest were narrow and about what a refusal says: which verdict some malformed requests get and in which order the generated guards run (D417), which identity `actor.id` records (D418), and an assertion naming the current state, a repeated `add`, the name of a uniqueness invariant and a forward reference §3 did not permit (D419). ADR-0122, amended with decisions 20 to 24, and corrections in place resolve each; a fourth review checks them (§7).

## 7. The fourth review

Fresh readers found every third-round finding resolved. The Jira reader found all twenty flows fully determined, with no finding on a verdict; the inventory reader found 84 of 85 transitions determined in outcome, the exception two edge requests to the assertion. What remained was the assertion's guards and admissions and the order of a refusal's causes (D421), and wording that misled (D422), each now resolved. One finding is the author's to decide: a store with no legacy system has no way to create its first actor, since every request must name one (`TODO.md`). A fifth review checks the result (§8).

## 8. The fifth review

Fresh readers found every fourth-round finding resolved. The Jira reader found all twenty flows fully determined, with no finding on an outcome or a verdict, and one description that used `agent`, a reserved actor kind, for a support person. The inventory reader found no object stuck and 82 of 85 transitions determined in outcome. The three exceptions were a set attribute's value before anything writes it (D423) and whether a backdated request's calls check the occurred time against their own objects (D424). On verdicts, an invariant refusal on several objects had no order (D425). Four of the journey's guards also said "supply it" where nothing the request supplies can satisfy them (D426). A notice added to find that pairing found three more in the format's own examples. Two descriptions misled (D427). ADR-0122, amended with decisions 27 to 30, and corrections in place resolve each; a sixth review checks them (§9).

## 9. The sixth review

Fresh readers found every fifth-round finding resolved but one, and that one was this record's own: the wording that resolved D424 checked an internal transition's own interval, where the model checks only what a request changes (D428). The Jira reader found all twenty flows fully determined again, all eighteen declared remedies consistent with the sharpened definitions, and three descriptions that misled. The inventory reader found no object stuck and 80 of 85 transitions determined in outcome. It also asked whether the module was valid at all, since §4.11 ordered derived attributes without saying across which scope; a probe then showed that a cycle of derived attributes across types passed every step (D429). On verdicts, an invariant refusal's remedy had two readings (D430). `dependent` had two definitions, and two of the journey's remedies were false under the model's (D431).

Two of the inventory reader's remedy findings, `filled` and `Lease.out`, are classes that hold in the usual case and not at every edge. Each review had found a new such edge, and a builder carries the declared class as written, so ADR-0122 now says so and leaves the edges to the author (decision 34). Checking where the old invariant remedy was still written found the library API several decisions behind (D433). ADR-0122, amended with decisions 31 to 35, and corrections in place resolve each; a seventh review checks them (§10).

## 10. The seventh review

Fresh readers found every sixth-round finding resolved. The Jira reader found all twenty flows fully determined for the third round running, with no finding on an outcome or a verdict. The inventory reader found no finding on an outcome, 84 of 85 transitions fully determined and the last determined but for one refusal's remedy. Both readers reached the same rule from two sides: the remedy decision 33 gave an invariant refusal. Its order told a duplicate that another object must change, and it read an input a request left out two ways (D434). Two Jira names and descriptions misled (D435). ADR-0122, amended with decision 36, and corrections in place resolve each; an eighth review checks them (§11).

