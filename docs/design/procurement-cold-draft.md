# Procurement on ObjectFlow: a cold draft, a cold integration, and what they found

Status: **repaired**, 2026-10-10: fourteen defects (D547 to D560), each resolved, and ten questions for the author (§5), decided as recommended by ADR-0173 and each built, the evaluators' transport with the client the author chose (ADR-0175); a second cold review, against the repaired state, is to follow. The author asked for a review of the design and the samples "as a potential developer who will use it to implement a procurement tracking system", for the sharp edges that make using it difficult or unpleasant. Two agents new to ObjectFlow did the two jobs such a developer has: one wrote a procurement flow, the other built an application against the engine. Each kept a log of what hurt; every claim framed as a defect was reproduced or corrected before it was classified (§3).

## 1. How it was run

**The brief**, given to both, was a mid-sized company's procurement in ten points: requisitions with lines and a cost centre, which an AI agent may draft from email; approval by amount at three levels in SGD, nobody approving their own; a budget per cost centre with a finance override; three quotes above SGD 10,000, a dearer choice justified; purchase orders to suppliers whose records the ERP owns, an amendment over 10% re-approved; partial receipts, 5% over allowed for raw materials; invoices matched three ways within 2%, exceptions to accounts payable, payment in Xero; cancellation before receipt, or of the remainder after; nine metrics, one per approver; and 300 open orders brought in from a spreadsheet at go-live.

**The drafter** was told to build the flow from the documentation and the samples, check it and run its examples on both backends. **The integrator** was told to build a client of the engine for a procurement application in twelve steps, from starting the engine to agent tool definitions, and to judge the integration surface itself. Both started from `README.md` at commit `9e28e8b`, could read any user-facing document, the samples, `scripts/`, `core/` and the Flow Studio, and were barred from the records of known findings (`defects.md`, the cold readings, the reimbursement draft, the recoverability review, `TODO.md`, `LESSONS.md`), so what they report is their own experience. Unlike the reimbursement draft (`reimbursement-cold-draft.md` §1), they could read the samples, since a developer would. The repository was read-only to both; each worked in a scratch directory, and `git status` was unchanged after each.

**What they built.** The drafter's `procurement` module has 16 types and 69 transitions, a nine-state `Requisition` with three approval levels, quotes, orders with lines, receipts, invoices with the 2% match, a stubbed Xero check and seven metrics; its 52 examples pass on the reference runner and on the core, and a go-live version with 6 examples imported 300 orders with their receipts, 1,144 objects, in about 3 seconds on the core and cut over in 4. The integrator's client ran all twelve steps against the core's service with `reimbursement.yaml` standing in for a requisition, after finding `approvals.yaml`'s `PurchaseRequest` unable to carry a purchase (D557); it wrote a mirror twin of the module to import 300 rows, a v2 to make an approval proposable, and an agent tool generator of its own.

**How each finding was checked.** The logs are model output and were treated as assertions. Each claim framed as a defect was reproduced by the reviewer's own probe against a fresh core service, the runner through the Flow Studio's server, or the flow checker, over minimal modules written for the purpose; the command and its output are in the defect's entry. One claim was wrong and is corrected below (I12). The probes lived in the session's scratch directory, which does not outlive it; each defect quotes enough to reproduce it.

## 2. The verdict, for a team choosing it today

**The flow language carried the whole brief.** Everything the drafter modelled checks, runs alike on both backends but for two places, an example's derived amount (D548) and a remedy the runner does not compute (D547), and enforces what the brief asked, separation of duties included. What it could not express is short and mostly by design: who may act, which is the application's (ADR-0114); one currency per amount (ADR-0068); the engine never initiating, so "automatic" matching is an application job. Both agents praised the checker's messages, the publish dry run, the import's mechanics and the verdicts as values.

**The engine is not yet adoptable, and says so only in places.** The only service is a sandbox's: an in-memory store, a clock that starts at `2026-01-01T00:00:00Z` and moves only when told, loopback only, one caller (`DESIGN.md` §2: "a deployment's service … is not yet built"). That is the project's stage, not a defect, but a developer meets it last: the README says there is "no runnable code yet" and, a page later, that the core is being built behind an HTTP service, and nothing says how to start it (D558).

**The sharpest edges, by how much they would hurt a team**, each with where it is taken up:

1. No deployable service: no persistent store, wall clock, configurable address or health route (the stage; `TODO.md`).
2. A deployment cannot answer an external evaluator, so a budget check or an ERP read-back cannot be connected to anything real; nothing in the design says how a deployment's service would (Q2).
3. Rules about who acts, "a person submits" and "nobody approves their own", can only be invariants over a recorded actor, whose refusals tell an agent the wrong next move, or none, and which `availability` does not evaluate (Q1, D547).
4. No integration guide and no wire reference: both agents read the service's source or the Flow Studio's code to learn the wire, the integrator most of its time (Q7).
5. Go-live into a live type needs a mirror twin of the module and a cutover, and closes the type to later imports (Q4).
6. The two backends judge an example's derived value differently, so no examples file asserting a derived amount passes both (D548).
7. Agent tooling the README describes in the present tense does not exist (D558, Q10), and the refusal fields an agent is told to act on, `objects` and `proposable`, are never computed (`TODO.md`).
8. Three checker defects refuse modules the reference documents as valid, or pass one it says is invalid (D549, D550, D551).
9. Multi-currency is a pattern each team invents (Q5).
10. The examples format shaped the drafter's model: `creates` names one type, and an import step cannot raise a sequence (Q6).

## 3. The findings, each checked

`P` is the drafter's log (§6), `I` the integrator's (§7), by entry. *By design* cites the decision; *stage* is a part of the design the core does not yet build, which the wire names in `not_computed` where it can.

| Entry | Finding | Checked by | Disposition |
|---|---|---|---|
| P1 | "The agent submits" and "no self-approval" can only be invariants over a recorded actor; the agent is offered `submit`, then refused with remedy `unreachable_from_here`; the self-approval refusal has remedy `null` | the checker's message the drafter quotes, `check 64  reads the actor outside an outcome value`, and `declaration-syntax.md` §3.2: an outcome is "the one place a declaration may read the actor (check 64)"; the drafter's `selfapprove_probe.py`, rerun: `{"kind": "invariant_violated", "remedy": null, …}`; `library-api.md` §4 types the remedy as a `Remedy`, inferred, never null | D547 for the null; the rest Q1 |
| P2 | An example's derived money value: `SGD 1250.00` passes the core and fails the runner, `1250.00` the reverse | the drafter's five examples; for a derived decimal, a probe, `price * price - price`: unquoted `0.75` and `0.7500` pass both, `"0.7500"` the core only, `"0.75"` neither | D548 |
| P3 | One currency per amount; FX by hand | `declaration-syntax.md` §8.3, ADR-0068, ADR-0145 | By design; Q5 |
| P4 | `creates` aliases one type, so an example cannot name a PO's lines | `flow-format.md` §11.3: "They MUST be of one type" | By design; Q6 |
| P5, I22 | Go-live needs a mirror twin of every family brought in, invented external ids and a cutover; an example's `import` cannot raise a sequence; a type takes no import after cutover | `publish-and-import.md` §4 to §7; `flow-format.md` §11.3 lists the step's keys, no `sequences`; the dry run's classes are `ImportReport.findings` in `library-api.md` §5, so the key the drafter guessed, `classes`, is documented under another name | By design; Q4, Q6 |
| P6, I1, I24 | No page says how to write, check and run a flow, or start the engine; the README contradicts itself; the studio cites a `core/README.md` that does not exist; the service ignores `--help` | `README.md` line 7; `apps/flow-studio/server/flowstudio/core.py:51`; `objectflow-service --help` and `--bogus` both print `listening on http://127.0.0.1:8712` | D558; Q7 |
| P7 | No command runs a module's examples on the core | the checker's step 6 runs the runner only; both agents borrowed the Flow Studio's `tests/replay.py` | `TODO.md` |
| P8 | `i.held(<member>)`, documented in `declaration-syntax.md` §6.9, is refused in a formula metric | a probe: both checkers answer `metric owner_wait reads 'owner'; a metric reads its rows through 'i'` | D550 |
| P9 | A `create` with `result:` whose target is only via its creator fails check 13 | a probe: removing only `result:` passes; both checkers refuse it, the core's dry run unaccepted | D551 |
| P10 | A value reached through required references is not "provably present" | `flow-format.md` §4.5: "any other expression is not counted" | By the rule; Q8 |
| P11 | No metric expectation in an example; a reference dimension reads as an id | `flow-format.md` §11.4 | By design; Q6 for the first |
| P12 | No list-of-records input and no atomic batch, so a document of N lines is N+2 requests | `DESIGN.md` §10: "There is no atomic batch" | By design |
| P13, I19 | Nothing generates the tool definitions or the rule set the README describes; an offer gives each input's type but not whether it is optional | `renderers.md` header: "nothing generates it yet"; `availability` gives types only, and `renderers.md` §5 sends a form to `declaration(type)` | D558 for the README's tense; Q10 |
| P14, I12 | `get` answers `null` for a requisition's lines | a probe: `"lines": null` with a line present; `flow-format.md` §4.3: a set "is never optional and never absent" | D552 |
| I12 | A requisition's lines cannot be queried by its id | **wrong as stated**: `claim.id == "claim-3"` finds the line, through `.id`, which every object has (`flow-format.md` §3; `declaration-syntax.md` §6.9 lists it among an object's members). What is right: `claim == "claim-3"` silently matches nothing, and `claim == claim-3` is `501 not run: Object("claim-3") - Int(3)` | D556 |
| I17 | The runner is not usable as an application's backend: no `query`, no idempotency key, values the core refuses accepted | the integrator's runs; `DESIGN.md` §2 makes the runner the oracle and the core the service | By design; one sentence in the guide (Q7) |
| P15, I9 | The runner's wire refuses an actor named by its identity, and a login in a reference input is a `500` | a probe through the studio's server: `actor_known` for `{"id": "alice@corp"}`, which the core accepts; `500 {"fault": "KeyError: 'alice@corp'"}`; `DESIGN.md` §5.8: the actor is "the object whose actor identity the request names" | D560 |
| P15 | `availability` takes `id`, `request` takes `object_id` | the probes | D555 |
| P16 | Re-selecting a quote without a justification cannot clear the old one in one transition | `flow-format.md` §4.8: a transition never both writes and clears one attribute | By design; two transitions |
| P17 | A finance override of one rule has no construct | `DESIGN.md` §8: an assertion carries none of the flow's guards | By design; the drafter's guard reading a recorded reason is the pattern |
| P18 | "Automatic" matching is an application job, with complementary guards written twice | `DESIGN.md` §12: scheduling and chasing belong to an upper-layer application | By design |
| P19 | An effect sees the object's new state, so a derived value read in the effect counts the object | `DESIGN.md` §6 step 5: "write its new state, then run its effect" | By design; worth a sentence in `authoring-flows.md` (Q7) |
| P20 | The coverage notice counts only requests under test | ruled by design in the reimbursement draft (entry 22), raised again | Q6 |
| P21 | Every type needs a final state; a personal input needs an erasure | checks 15 and 60 | By design |
| P22 | `+` of two decimal scales passes the checker | a probe: a direct sum of scales 2 and 4 is refused by check 24, but `price * price - price` passes both checkers, since a product of decimals is not typed | D549 |
| P23 | The diagram draws an only-via transition like any other; step 5 decides 2 of 18 guards over derived totals | the drafter's output | Cosmetic; step 5's reach is by design (`flow-format.md` §7.1) |
| I2 | No deployable service | `store.rs:1307`, `Connection::open_in_memory()`; `main.rs:1282`, bound to `127.0.0.1`; `DESIGN.md` §2 | Stage; `TODO.md` |
| I3 | The core's sandbox load runs no checks | ADR-0153 decision 5; the studio checks before it loads | By design for the slice |
| I4 | No wire reference; the wire differs from `library-api.md` (no `/v1/batch`, `not_computed`, shapes) | ADR-0150: "The OpenAPI description is written from these … as the studio's slices use them"; none exists | Q7; the missing operations are stage |
| I5 | `objects`, `proposable`, `unknown` and `consulted` are never computed on a refusal | the probes: every `unsatisfied` carries them in `not_computed` | Stage; `TODO.md`, first for agents |
| I6 | An agent refused by `confirmed_by_a_person` is told `self_serviceable`; `stale` shares that class with a mistyped input | the probe; `library-api.md` §4's inference rule gives exactly that; `DESIGN.md` §5.5 gives `stale` `self_serviceable` | The rule as written; Q1. `stale` by design |
| I7 | `invalid_input` names the input, never why | `library-api.md` §4, `InvalidInput.inputs` | By design; Q9 |
| I8 | An amount past the store's range is a `500` | the probe: `500 {"fault": "the store: a decimal past 64 bits"}` | D554 |
| I10 | Every user must exist in the store before acting | `DESIGN.md` §5.8, `actor_known` | By design; an identity-sync pattern belongs in the guide (Q7) |
| I11 | `availability` ignores the actor and lists built-ins; `check` of a creation names an id | ADR-0114; the probe: `check` named `claim-3`, and the next creation was `claim-3`, so it predicts rather than invents | By design |
| I13 | No paging; unknown request keys ignored | the probes: `"fitler"` and `"limit": 0` return everything | Paging stage (`TODO.md`); unknown keys D555 |
| I14 | No text search in a filter | the integrator's probe, `refused: malformed_filter` for `contains`; the integrator called search the application's "by design, `DESIGN.md` §12", but no document says whose search is | A gap in the documentation: the guide's table (Q7) |
| I15 | History carries a request's inputs, not its writes; some attributes are JSON inside strings | not reproduced | Reported; `TODO.md` |
| I16 | A deployment cannot answer an evaluator | `library-api.md` §3, `EvaluatorSource`: "Production passes the real integrations"; `DESIGN.md` §2: "No caller embeds the core"; the service routes only the sandbox's fixed answers | Q2 |
| I18 | Faults and not-found differ by route: plain-text `400`, empty `404`, `get` `200 null`, `history` `200` empty | the probes; ADR-0150 decision 1: "A fault is a `4xx` or `5xx` with a `fault` message" | D555 |
| I20 | Proposing a non-proposable transition is `not_requestable` with no parents on the core | the probe; the runner raises a fault; no document names the verdict | D553 |
| I21 | A publish report's sections listed in `not_computed`; anyone may publish | the report; ADR-0114 | Sections stage; who approves by design |
| I23 | Durations read `"15 days"`, `"1431 h"` | ADR-0150 decision 2 chose the model's notation over ISO 8601 | By design; the wire reference states it (Q7) |
| I25 | `approvals.yaml`'s `PurchaseRequest.item` can never be set | nothing in the module or its examples writes `item`, and no check notices an attribute nothing writes | D557 |
| I26 | A `file` attribute takes any string on the wire | the probe: `"receipt": "quote-4471.pdf"` accepted; `DESIGN.md` §5.2 promises a reference "with its hash, size, media type and provenance"; ADR-0150's notation names no form for it | D559 |

## 4. What was changed

Nothing in the design or the code, which the author asked to review rather than repair. The defects are in the register as D547 to D560, all open; the follow-ups and the questions are in `TODO.md`; this document records the run.

## 5. Questions for the author

Each with a recommendation. **Decided 2026-10-10 as recommended** ("now address the issues", ADR-0173), with three of the defects' own decisions beside them; each is built in its own commit, tracked in `TODO.md`.

1. **Rules about who acts.** Check 64 keeps the actor out of conditions (ADR-0114), and the sanctioned pattern (ADR-0145) records `actor.kind` or `actor.id` in an outcome and constrains it with an invariant. Both agents used it, and it enforces; but the remedy an invariant failure infers (`library-api.md` §4) can never be `delegable`, so an agent refused for being an agent is told to fix its input (I6) or that nothing will help (P1), and `availability`, which runs no effects and so no invariants, offers it what it will be refused. **Recommendation:** infer `delegable` for an invariant failure whose reads include a value the request's own effect wrote from `actor`, since another actor taking the same transition could pass it; this stays within ADR-0114, the engine still judging only the record. And mark an offer whose transition writes from `actor` into a value an invariant reads, so a worklist or an agent knows the offer depends on who takes it. If the author would rather revisit ADR-0114's rejected alternative, a declared actor kind per transition evaluated as a guard, that is the cleaner and bigger change.
2. **Evaluators in a deployment.** `EvaluatorSource` is injected in process, and no caller embeds the core, so nothing says how a deployment's service answers `xero.bill_paid` or a budget check; a team today would have to fork the service. **Recommendation:** an ADR before the deployment service: the service calls each evaluator namespace at an HTTP endpoint the deployment's configuration names, outside the transaction as §6 already orders, with a timeout whose expiry is the evaluator's `unknown` (remedy `temporal`), and a verdict-only answer (ADR-0008). Rejected: evaluator plug-ins compiled into the service, which make every integration a fork.
3. **Proposing a non-proposable transition** (D553). **Recommendation:** a distinct verdict, `not_proposable`, remedy `self_serviceable` (send it as a request, if the caller may), since `not_requestable` means only-via, and the runner's fault treats a well-formed request as malformed. Rejected: `not_requestable` with a field saying why, since a caller branching on `kind` would conflate the two.
4. **Go-live into owned types.** An import writes only mirrors (`flow-format.md` §4.15), so bringing in live orders is a mirror twin of each family and a cutover publish, after which the type takes no more imports. Both agents made it work, fast, and both called it their costliest step. **Recommendation:** keep the principle; generate the mirror twin from the module (each family marked `mirror`, an `external` identifier added, references to owned types dropped, erasures reaching the parts), and put a worked go-live example in `publish-and-import.md`. Import into an owned type, for phased onboarding, is best not reopened until a consumer needs a second batch.
5. **Multi-currency.** One currency per amount (ADR-0068, ADR-0145 decision 3), hit by both cold drafts now. **Recommendation:** keep it, and write the pattern the drafter reached into `authoring-flows.md`: a currency reference with a rate, transaction amounts as decimals, the home-currency amount computed and kept at entry, and the limit that nothing stops adding decimals across currencies.
6. **The examples format.** (a) An `import` step cannot raise a sequence, as `import_batch` can; (b) `creates` names one type; (c) no example can expect a metric's value; (d) the coverage notice counts only requests under test, ruled by design once and raised again. **Recommendation:** (a) a `sequences` key on the step, and (d) the notice reworded to say it counts the request under test, both small, now; (b) aliases keyed by type next; (c) when metrics need regression tests.
7. **Documentation for adopters.** Both agents spent their first hour finding out what runs and how, and the integrator most of its time learning the wire from the service's source. **Recommendation:** three pages: a quick start for an author (write, check, run on both backends), an integration guide for an application (start the service, the wire in JSON, an identity-sync pattern, and a table of what the engine does and what the application must), and a wire reference, written from the service now and replaced by OpenAPI with the deployment service; with the README corrected (D558).
8. **"Provably present"** (`flow-format.md` §4.5). **Recommendation:** count a path of non-optional references ending in a non-optional attribute, which always has a value, so a state can require what the engine derives (`cost_centre.fiscal_year`) without a hand-written invariant; leave values a guard tests out of it.
9. **Why an input is invalid.** **Recommendation:** a reason beside each input `invalid_input` names, from a closed set (`unknown_input`, `malformed`, `wrong_currency`, `no_such_object`, `out_of_range`), which the engine knows at step 4, so a form needs no validator of its own.
10. **Agent tools.** **Recommendation:** say "will be generated" in the README now (D558), and build `renderers.md` §3's tool schemas as an operation once the wire reference exists, filtering out for an actor kind the transitions whose invariants refuse that kind, which joins question 1.

## 6. The drafting agent's log, as it wrote it

The log is model output, kept as written; §3 is the checked reading of it, and the paths it quotes were in the session's scratch directory.

Evaluator: a developer new to ObjectFlow, building the procurement brief from the repository's own documentation and samples, on 2026-10-10. Repository at commit `9e28e8b` (read-only for me). Everything I wrote is in this directory.

### What I built, and what runs it

- `procurement.yaml`: module `procurement`, 16 types, 69 transitions, 7 declared metrics. People and reference data (`Employee`, `ProcurementAgent`, `Company`, `Department`, `Currency`, `CostCentre`, `Supplier` as an ERP mirror), the requisition (`Requisition` with 9 states, `RequisitionLine`, `Approval`, `Quote`), the order (`PurchaseOrder`, `POLine`), receiving (`Receipt`, `ReceiptLine`) and `Invoice`, plus a Xero evaluator.
- `procurement.examples.yaml`: 52 examples. All pass on the reference runner (the checker's step 6) and on the Rust core (via `run_on_core.py`).
- `procurement_golive.yaml` and `procurement_golive.examples.yaml`: the go-live version, in which the PO family arrives as mirrors, plus 6 examples that import a legacy order, cut over by publishing `procurement.yaml` and keep working on it. All pass on both backends. `procurement.yaml` checks clean against it with `--previous`.
- `golive_rehearsal.py`: 300 legacy POs (1,144 objects) dry-run, decided, imported and cut over on the core's sandbox. `read_metrics.py` reads every declared metric on both backends. `run_on_core.py` replays an examples file on the core. `agent_view.py`, `get_probe.py`, `selfapprove_probe.py` and `decl_probe.py` are probes whose output is quoted below.

Shorthand used below. From this directory:

```
CHECK="env PYTHONDONTWRITEBYTECODE=1 /home/rdu/RduWs/object_flow/scripts/.venv/bin/python /home/rdu/RduWs/object_flow/scripts/check-flows.py"
$CHECK procurement.yaml procurement.examples.yaml        # 0 fatal, 52 passed
python run_on_core.py                                    # {'passed': 52}
$CHECK procurement_golive.yaml procurement_golive.examples.yaml   # 6 passed
python run_on_core.py procurement_golive.yaml            # {'passed': 6}
```

How the brief maps:

| Brief | Modelled as | Status |
|---|---|---|
| 1 Requisition, agent drafts | `raise` by employee or agent; `add_line` with `suggested_category`; a person's `set_category`; invariants `category_set_by_a_person` and `submitted_by_a_person` over a recorded `actor.kind` | done; see 1 and 4 |
| 2 Approval by amount, no self-approval, reject and revise | states `PENDING_MANAGER`, `PENDING_DEPARTMENT_HEAD`, `PENDING_CFO`; two transitions per level (approve, or approve and escalate) guarded on the SGD total; `Approval` part recording `actor.id`; invariants `not_own_requisition` and `decided_by_expected_approver`; `reject` then `revise` | done; see 1 and 2 |
| 3 Budget and finance override | `within_budget` guard over a derived sum on `CostCentre`; `record_budget_override` writes a reason that the guard accepts | done, approximated; see 17 |
| 4 Three quotes above SGD 10,000, justify a dearer one | `Quote` parts; `enough_quotes` and `cheapest_or_justified` on `Requisition.order` | done; see 16 |
| 5 PO, ERP suppliers, amendment over 10% re-approved | `PurchaseOrder.issue` creates lines from the requisition; `Supplier` mirror; `amend_line` vs `amend_line_for_reapproval`; `reapproved_by_someone_else` | done |
| 6 Partial receipts, 5% over for raw materials | `Receipt` and `ReceiptLine`; invariant `POLine.not_over_received` over a derived limit | done |
| 7 Three-way match within 2%, AP exceptions, Xero payment | `match` and `route_to_exception` with complementary guards; `accept_exception`, `reject`; `record_payment` guarded by evaluator `xero.bill_paid` | done; matching is an application job requesting `match` (see 18) |
| 8 Cancellation before and after receipt | `cancel` guarded `nothing_received`; `cancel_remainder` sets each line's `cancelled_qty` | done |
| 9 Metrics | `requisition_to_po_time`, `approval_wait_by_level`, three per-approver turnaround metrics, `invoice_exception_share`, `spend_by_supplier_and_category` | computed, and equal on both backends; see 8 and 11 |
| 10 Go-live of 300 open POs | a mirror version, `import_batch`, a cutover publish | done in a rehearsal on the core; see 5 |

### Findings

#### 1. A rule about who acts can only be an invariant, so its refusal tells an agent the wrong thing, or nothing

- **What I was trying to do:** enforce "the agent drafts, a person submits" and "nobody approves their own requisition", and have the refusal tell the agent to ask a person, which the README promises ("A refusal is an instruction ... ask a person").
- **Where I looked:** README "Built for AI agents"; DESIGN.md §5.5 (Approval paragraph), §5.8; flow-format.md §5 (the `actor` bullets); the reimbursement sample's `confirmed_by_a_person` invariant; declaration-syntax.md §8.1.
- **What happened:** a condition may not read the actor. My probe condition `expression: actor.kind == human` gives `actor_guard.yaml:460  fatal   check 64  reads the actor outside an outcome value: require submitter_is_a_person: actor.kind == human because …`. So, following the reimbursement sample, I recorded `actor.kind` and `actor.id` in the effect and wrote invariants over the record. They are enforced, but:
  - The agent submitting is refused as `{"kind": "invariant_violated", "remedy": "unreachable_from_here", ... "invariant": "submitted_by_a_person"}` (core, `agent_view.py`). Its remedy is inferred and cannot be declared. "Take a different path" is wrong for an agent; the true answer is `delegable`.
  - The requester approving their own requisition is refused on the core as `{"kind": "invariant_violated", "remedy": null, "failures": [{"object_id": "approval-9", "type": "Approval", "invariant": "not_own_requisition", "remedy": null}], ...}` (`selfapprove_probe.py`). That is no remedy at all, and it names an object the rollback never created. DESIGN §5.5 says every refusal names a remedy class. The reference runner cannot compute it: with `remedy:` in the expectation the checker reports `notice  notrun  nobody_approves_their_own_requisition is not run: it reaches the remedy of an invariant that fails on an object other than the one requested, ...`.
  - `availability` runs no invariants, so the agent is offered `submit` as `"availability": "available"` on a categorised draft, then refused (`agent_view.py`).
- **Severity:** blocker, for the refusal's remedy and the offer (they can't be expressed). The rules themselves are enforced.
- **What I did instead:** invariants `submitted_by_a_person`, `category_set_by_a_person`, `Approval.not_own_requisition`, `Approval.decided_by_expected_approver`, `PurchaseOrder.reapproved_by_someone_else`. Examples assert only verdict and clause, never remedy.
- **What would have helped:** a sanctioned way to state "this transition is for people only" (the actor's kind is declared with its type, not claimed by the caller) as a guard with a declared remedy, which `availability` evaluates; and a defined remedy for an invariant that fails on a created part.

#### 2. The reference runner and the core disagree on how an example writes a derived money value; no spelling passes both

- **What I was trying to do:** assert a requisition's SGD total (`total_sgd`, a `sum` of `money(SGD)`) in an example.
- **Where I looked:** flow-format.md §11.3 ("Values ... as the attribute or input it fills declares"); the approvals sample, which writes stored money as `SGD 42.50`.
- **What happened:** with `values: { total_sgd: SGD 1250.00 }` the checker says `fatal   example   an_agent_suggests_a_category: expected total_sgd to hold SGD 1250.00, and it holds 1250`. With the bare `1250.00` the checker passes, but the core fails: `failed   an_agent_suggests_a_category: expected total_sgd to hold 1250.0, and it holds SGD 1250.00`. Measured on the five such examples: bare spelling gives runner 43/43 and core 38/43; currency spelling gives core 43/43 and runner 38/43. Stored money is accepted either way by the runner. On the wire, both backends return `SGD 10.00` for the same derived value (`get_probe.py`); only the checker's example comparison drops the currency.
- **Severity:** costly. A publish on the core runs the examples, so an examples file the checker passes would be refused there.
- **What I did instead:** no example asserts a derived money value. I assert the stored `amount_sgd` on a line, decimals (`total`, `approved_total`), or behaviour (a later budget refusal or acceptance).
- **What would have helped:** one value comparison shared by both backends, and the corpus covering a derived money expectation.

#### 3. Multi-currency: an amount's currency is fixed at declaration time, so foreign-currency arithmetic falls back to bare decimals

- **What I was trying to do:** Singapore and US offices: lines and orders in SGD or USD, thresholds and budgets in SGD.
- **Where I looked:** declaration-syntax.md §8.3 "Money" and §11 question 9 (ADR-0068, "a multi-currency model declares an account per currency"); the reimbursement sample ("Amounts are in one home currency, assumed USD here"; `original_amount` "recorded, and never computed with").
- **What happened:** `money(ccy)` holds one currency per attribute, and there is no FX construct. Following the docs literally, a USD price can be stored but not computed with.
- **Severity:** costly.
- **What I did instead:** a `Currency` type whose `rate_to_sgd` finance maintains; transaction-currency amounts are `decimal` (`unit_price`, `amount`, `total`); the SGD amount is computed and frozen when a line or quote is entered (`SGD 1.00 * quantity * unit_price * currency.rate_to_sgd`, which the checker accepted). The price of this: nothing stops adding a USD decimal to an SGD decimal; a PO is single-currency and every requisition line must already be in the selected quote's currency (`lines_in_quote_currency`); and an invoice in another currency than its PO cannot be expressed.
- **What would have helped:** a documented multi-currency pattern, or money whose currency is checked against a reference at runtime.

#### 4. The examples DSL shaped my model: `creates` can alias only one type that the requested transition creates directly

- **What I was trying to do:** let `Requisition.issue_po` create the order (which creates its lines), then receive and amend against specific PO lines in examples.
- **Where I looked:** flow-format.md §11.3 (`creates`: "They MUST be of one type, and the transition's effect MUST create that type").
- **What happened:** with the requisition creating the PO, an example can alias the PO but never its lines, so no example can name `pol_cw` for a receipt or an amendment.
- **Severity:** costly, because the model's shape was decided by the test language.
- **What I did instead:** inverted the design. The buyer requests `PurchaseOrder.issue(requisition)`, which creates the lines directly and calls `Requisition.order` (now `only_via`). The quote rules therefore refuse as `call_refused` with `clause: Requisition.enough_quotes` rather than on the requisition itself.
- **What would have helped:** aliases for any object a request creates, keyed by type (`creates: { POLine: [a, b] }`), or a lookup step.

#### 5. Go-live needs a second, mirror-only version of the module; worked, but it is a project of its own

- **What I was trying to do:** bring in 300 open spreadsheet POs with their receipts so far.
- **Where I looked:** DESIGN.md §11; publish-and-import.md §4 to §7; flow-format.md §4.15, §11.3 (`import`, `publish` steps); the servicedesk sample's `Employee` mirror; `core/crates/objectflow-service/src/main.rs` (`import_op`) for the wire shape.
- **What happened:** an import writes only `mirror` types, and "A stage is two publishes and the imports between them". A mirror "MUST NOT hold a reference to a type this store owns, one with no opposite, marked `stored`, or whose opposite is a set". So in the go-live version `Currency` had to be a mirror too (the PO references it), the PO's link to `Requisition` could only appear at cutover, and every mirror (down to `ReceiptLine`) needs an external identifier, so I invented `sheet_ref` attributes. In examples, an `import` step cannot carry a sequence's last value. A probe shows the first PO after cutover is minted `expected number to hold PO-00251, and it holds PO-00001`. In `golive_rehearsal.py`, with `"sequences": [{"sequence": "po_number", "last": 300}]` in the batch, the next order is `PO-00301`, as documented. The dry run returns the disposition classes under the key `findings`, while publish-and-import.md §6 calls them classes; my first script looked for `classes`, decided nothing, and the import was refused whole (`"its class POLine.not_over_received (does not hold) is undecided"`), which is the documented safe behaviour.
- **Severity:** costly, though it worked end to end. 1,144 objects: dry run 1.3 s, import 2.9 s, cutover publish 4.0 s. The admitted over-received row is listed by `exceptions(POLine)`. The publish report says `newly_enforced:PurchaseOrder.cancel.nothing_received ... affected 120`, which is exactly what an approver needs.
- **What I did instead:** `procurement_golive.yaml` (v1, mirrors) with `procurement.yaml` (v2) as its cutover; 6 examples and a 300-PO rehearsal.
- **What would have helped:** a worked go-live example in the docs (a family with parts and a reference to reference data), and a `sequences` key on the example `import` step.

#### 6. Onboarding: no single page tells a developer how to write, check and run a flow

- **What I was trying to do:** find out how to write a module and check it.
- **Where I looked, in order:** README.md (its example is in the text notation; it never mentions YAML, `flow-format.md` or how to check one module, only `check-corpus.py` for "the record"); DESIGN.md (opens with the ADR acceptance chronology; says the text notation "is no longer written by anyone"); `docs/design/authoring-flows.md` (not linked from the README; a decision record, not a how-to); flow-format.md (the spec; it names `scripts/check-flows.py` but never shows a command line or how an examples file is passed); finally `scripts/check-flows.py --help`, which is good: `usage: check-flows.py [--previous=<file>] <module.yaml> [<module.examples.yaml> ...]`.
- **What happened:** by my tally, about 2,400 lines of dense, ADR-cited documentation, plus 1,200 lines of samples, before I could write a line with confidence. The README also says "There is no runnable code yet", while a Rust core and a reference runner both ran everything I wrote.
- **Severity:** costly, in onboarding time.
- **What would have helped:** a one-page quick start: the minimal module (flow-format.md §8 is good), the exact check command with an examples file, how to run the examples on the core, and links to the three samples closest to common business flows.

#### 7. No documented way to run examples on the core

- **What I was trying to do:** run my examples on the Rust core as well as the reference runner.
- **Where I looked:** README.md (Rust core "being built"); `core/scripts/check` (the core's own gates); `core/crates/objectflow-service/src/main.rs` (routes; no examples operation); `apps/flow-studio/README.md` (the Studio edits only in `apps/flow-studio/flows/`, inside the repository, which I may not write to); `apps/flow-studio/server/tests/replay.py` and `test_core_replay.py`.
- **What happened:** no command runs a given module's examples on the core. The check in step 6 uses only the Python runner.
- **Severity:** costly.
- **What I did instead:** `run_on_core.py`, which imports the Studio's test helper `replay()` and starts the prebuilt `core/target/debug/objectflow-service --port 0` (current with its sources; I built nothing), then stops it by pid. To read metrics on the runner over HTTP, `read_metrics.py` runs the Studio's `serve("127.0.0.1", 0, <my dir>)` in-process.
- **What would have helped:** `check-flows.py --backend=core`, or a core command that takes a module and its examples.

#### 8. The documented `i.held(<member>)` cannot be written in a YAML formula metric (checker defect)

- **What I was trying to do:** approver turnaround by approver and level, in one metric over `intervals`.
- **Where I looked:** declaration-syntax.md §6.9 (`by engineer = i.held(engineer)`, and `.held(<member>)` in the interval row members); flow-format.md §6 (fixed measures convert to `i.held(<a>)`); `scripts/check-flows.py:2850`.
- **What happened:** `dimensions: { approver: i.held(pending_approver), ... }` gives `procurement.yaml:643  fatal   names     metric approver_turnaround reads 'pending_approver'; a metric reads its rows through 'i'`. The free-name scan treats the argument of `.held()` as an undeclared name.
- **Severity:** annoying.
- **What I did instead:** three fixed measures (`measure: median_time_in_state`, `group_by: [pending_approver, month]`), one per pending state, since a fixed measure's `group_by` converts to `.held()`. The core marks them in `person_dimensions` (`"Requisition.manager_turnaround.pending_approver"`), as the brief needs for a per-person figure.
- **What would have helped:** the scan exempting `.held(...)`'s argument, and a corpus sample using it.

#### 9. Check 13 refuses a `create` with `result:` whose target is `only_via` the creator (checker defect)

- **What I was trying to do:** `create: { type: PurchaseOrder, transition: issue, result: po, ... }` in `Requisition.issue_po`, with `PurchaseOrder.issue` declared `only_via: [Requisition.issue_po]`.
- **Where I looked:** flow-format.md §4.8 (`create ... result`); `scripts/check-syntax-doc.py:529-548`.
- **What happened:** `procurement.yaml:904  fatal   check 13  PurchaseOrder.issue names parent Requisition.issue_po, which never reaches it`. Removing only `result: po` made it pass (probe). The check's regex looks for `create PurchaseOrder.issue(` in the converted text, and `result` converts to `create po = PurchaseOrder.issue(...)`.
- **Severity:** annoying (I didn't need `result`, since `PurchaseOrder.requisition` is the stored end and `Requisition.purchase_order` its inverse; it would have been costly otherwise).
- **What would have helped:** check 13 accepting the `result =` form.

#### 10. "Provably present" ignores guards and required references, which pushes state requirements into hand-written invariants

- **What I was trying to do:** require `pending_approver` in the pending states, `budget_year` in `APPROVED` and `selected_quote` in `ORDERED`.
- **Where I looked:** flow-format.md §4.5 ("any other expression is not counted, even when it would have a value").
- **What happened:** for example, `procurement.yaml:367  fatal   required  Requisition.submit enters PENDING_MANAGER without setting 'pending_approver', which PENDING_MANAGER requires: make it a required input, assign it a value that is provably present, or require it in state DRAFT`. The assignment is `requester.manager`, behind the deny guard `requester_has_manager`. Likewise `budget_year := cost_centre.fiscal_year`, where `fiscal_year` is required and `cost_centre` is a required reference.
- **Severity:** annoying. The suggested fix ("make it a required input") would move data the engine can derive onto the caller.
- **What I did instead:** explicit invariants (`awaiting_someone`, `approved_in_a_budget_year`, `ordered_by_a_purchase_order`) that hold at runtime but lose the static check.
- **What would have helped:** counting a path of required references to a required attribute, and an attribute a deny guard of the same transition tests with `is not null`.

#### 11. Metrics cannot be asserted in examples; and the declared spend metric returns internal ids

- **What I was trying to do:** prove the nine metric questions are answered.
- **Where I looked:** flow-format.md §11.4 (`expect` keys: no metric); library-api.md (`metric`); the Studio's `tests/test_offer.py` for the wire shape.
- **What happened:** nothing in an examples file reads a metric. `read_metrics.py` shows both backends computing identical values, for example `Requisition.approval_wait_by_level: [[{"level": "PENDING_MANAGER", ...}, "4 h", null], [{"level": "PENDING_DEPARTMENT_HEAD", ...}, "2 days", null]]`. The spend metric's supplier dimension comes back as `"supplier-12"`, an internal object id, so a report must resolve ids itself.
- **Severity:** annoying.
- **What would have helped:** a `metric` expectation in examples; the option to show an identifying attribute for a reference dimension.

#### 12. Line items cannot be sent in one request, and there is no atomic batch

- **What I was trying to do:** the agent drafts a whole requisition from an email; a receipt records several PO lines.
- **Where I looked:** flow-format.md §4.8 (input types: values, sets of values, references); DESIGN.md §10 ("There is no atomic batch").
- **What happened:** an input cannot be a list of records, so each line is its own `add_line` request, and a receipt is `record_delivery`, then `add_line` once per PO line, then `post`.
- **Severity:** annoying. Harmless in `DRAFT` and `OPEN`, where a half-built object is visible as such, but it is N+2 calls per document for an agent.
- **What I did instead:** the multi-request pattern, as the reimbursement sample does.

#### 13. Agent tooling: no rule-set printer or tool-schema generator, and the live offer omits optionality and enumeration values

- **What I was trying to do:** give the drafting agent tool definitions "generated from the declaration" (README).
- **Where I looked:** renderers.md (header: "The rule set of §2 is prose and is not checked; nothing generates it yet"); `scripts/`; the core's routes.
- **What happened:** nothing generates either one. `availability` gives per-transition inputs as types only, for example `"inputs": {"description": "string", ..., "category": "SpendCategory", "suggested_category": "SpendCategory"}`, without saying which are optional or what `SpendCategory`'s members are.
- **Severity:** costly for the agent part of the brief.
- **What would have helped:** the tool-schema renderer of renderers.md §3 as a command or an operation.

#### 14. `get` returns `null` for a set-valued inverse end

- **What I was trying to do:** read a requisition's lines.
- **Where I looked:** DESIGN.md §10; flow-format.md §4.3 ("A set ... is never optional and never absent").
- **What happened:** on both backends, `get` of a requisition with one line gives `lines = None` (and `Department.members = None`), while its derived `total_sgd` is `SGD 10.00` (`get_probe.py`).
- **Severity:** annoying.
- **What I did instead:** `query` of `RequisitionLine`.

#### 15. Small wire inconsistencies between operations and between backends

- **What happened:** `availability` takes `"id"`, `check` and `request` take `"object_id"`: `the wire answered 400: a request names an object_id or, to create, a type`. The runner's HTTP adapter refuses an actor named by identity (`{"id": "ana"}` gives `unsatisfied ... actor_known`) and wants the object id (`employee-3`), while the core accepts both. DESIGN §5.8 says a request names the actor by identity.
- **Severity:** annoying.

#### 16. A choice that depends on whether an optional input is given needs two transitions

- **What I was trying to do:** `select_quote` with an optional justification.
- **Where I looked:** flow-format.md §4.8 (an optional input "is skipped when the input is absent"; "A transition MUST NOT both write and clear one attribute").
- **What happened:** with one transition, re-selecting without a justification would keep the previous selection's justification. I can't clear and conditionally write the same attribute.
- **Severity:** annoying.
- **What I did instead:** `select_quote`, which clears the justification, and `select_quote_with_justification`, which requires it; the check sits on `Requisition.order`.

#### 17. Budget: per-rule override and fiscal-year scoping by hand; "spent" approximated

- **What I was trying to do:** "committed plus spent must not exceed the budget unless finance records an override".
- **Where I looked:** DESIGN.md §8 (an assertion "carries its own guards ... and none of the flow's").
- **What happened:** an assertion would skip every guard of the approval and must name a target state, so it cannot override one rule.
- **Severity:** annoying.
- **What I did instead:** `record_budget_override` writes a reason, and `within_budget` reads `... or budget_override_reason is not null`. The commitment is the approved total, then the PO's total net of cancellations; the year is matched through `budget_year`. An invoice above the PO value does not add to consumption.

#### 18. The engine never initiates, so "automatic" matching is an application job, with complementary guards written twice

- **What happened:** `match` (guard `within_tolerance`) and `route_to_exception` (guard `outside_tolerance`) restate the same arithmetic negated. The job requests `match` and, on refusal, `route_to_exception`. This is by design (DESIGN §12), and it costs a duplicated expression that could drift.
- **Severity:** annoying.

#### 19. An effect sees the new state, so derived values read in the effect can count the object itself

- **What happened:** `matched_against := purchase_order.uninvoiced_received_value` in `match` produced `expected matched_against to hold 3000.0, and it holds -50`, because the invoice was already `MATCHED` and so counted as invoiced. DESIGN §6 step 5 says the state is written first; I had not taken it in.
- **Severity:** annoying (an example caught it).
- **What I did instead:** `+ amount`, with a comment.

#### 20. Coverage notice counts only the transition under test

- **What happened:** `notice  coverage  no example requests 47 transitions of procurement: ... Employee.onboard, ... POLine.add, ..., Requisition.order, ...`, although setups request `Employee.onboard` in every example and `cascaded:` asserts `POLine.add` and `Requisition.order`.
- **Severity:** annoying, since the useful signal is buried among false gaps.

#### 21. Ceremony the checker requires

- Every type needs a final state and every state an exit (`check 15  Company has no terminal state`, `Company.OPERATING is non-terminal with no outgoing transition`). I added `Company.dissolve` and `Currency.retire`, which nobody needs.
- One optional personal `note` input on `reject` requires an erasure on `Requisition` (`check 60`).
- Severity: cosmetic.

#### 22. The decimal-scale rule documented for `+` is not enforced

- **What happened:** declaration-syntax.md §8.3 says `+` takes "`decimal` of one scale". A probe derived attribute `quantity + line_total` (`decimal(10,2)` plus `decimal(22,4)`) checks with `0 fatal`, and so does `purchase_order.uninvoiced_received_value + amount` in my module. I did not test what each backend computes at runtime.
- **Severity:** cosmetic for me; a possible source of runner/core drift.

#### 23. Misc

- The diagram from `scripts/flow-diagram.py procurement.yaml Requisition` draws the `only_via` transition `order` like any requestable arrow. Cosmetic.
- Step 5 (the logic check) decided 2 of 18 `Requisition` guards, because the totals are derived from parts, so complementary threshold pairs (`within_manager_limit` and `above_manager_limit`) are not checked for gaps or overlap. Cosmetic.

### Not finished, or left to the application

- Who may act (is this person the manager, in finance, in accounts payable) is the application's by design. The engine refuses only what the record shows: self-approval, and a decision by anyone but the awaited approver.
- The real Xero integration (the evaluator is stubbed in examples), notifications and chasing are the application's.
- Spend by supplier and category is measured on ordered value, not paid value.
- No requisition-line amendment (remove and re-add instead).
- PO re-approval is "anyone but the amender", not the original approval chain.
- Not tested: performance beyond the 1,144-object import; erasure of `Employee`; local-time (Singapore vs US) date rules.
- Housekeeping: I did not build or run cargo. During my session (at 00:48:46) something rewrote the git-ignored `core/target/debug/objectflow-service.d`; the binary is unchanged (dated 2026-10-09 19:08). None of my scripts invokes cargo (the Studio's cargo build is in its `__main__`, which I did not run). `git status` is as I found it.

### What worked well

- **Checker messages** are precise, carry a line number, and usually say how to fix the problem: the YAML comma rule, the `required` message offering three fixes, `check 64`. A run takes about 1 s.
- **The `unknown` notice** caught a real bug: `r.budget_year` (optional) in a filter would have made `committed_sgd` unknown and the budget guard always refuse.
- **Runner and core agree.** All 58 final examples pass on both. The only disagreements I found are the derived-money spelling (finding 2, 5 examples) and the self-approval remedy (finding 1). Every metric value matched. 42 of 43 slice-2 examples passed on their first run.
- **The expression language held** money × decimal arithmetic, three-hop paths, `state in {…}`, conditionals in derived attributes, and invariants over the recorded `actor.id`.
- **Separation of duties** via recorded actor plus invariant, and the agent-suggests/person-confirms pattern from the reimbursement sample, enforce correctly (see 1 for the remedy problem).
- **Assignee plus standard metrics** gave per-approver time and work in progress with no extra code, marked as naming a person.
- **Go-live tooling:** dry run, disposition classes, import, cutover impact report, `exceptions()`, and the sequence raise all worked on the first real attempt, and fast.
- **Evaluator stubs** (`satisfied`, `unanswered` giving `temporal`) made the Xero rules testable.
- **`flow-diagram.py`** draws a readable state diagram from the module (`requisition-diagram.mmd`).

## 7. The integrating agent's log, as it wrote it

As §6; entry 12's claim that lines cannot be queried by their requisition's id is wrong as stated (§3, I12).

Evaluator: an application developer new to ObjectFlow, building a procurement web app and an AI agent on it. Date: 2026-10-10. Repository at commit `9e28e8b` (read-only).

### What I built and ran

- `ofwire.py`: a standard-library wire client (`POST /v1/<operation>`, `/sandbox/<control>`).
- `procurement_client.py`: starts the engine itself, runs the twelve steps, and writes every request and response to `transcript-core.md` (or `transcript-runner.md` with `--backend runner`). Run: `python3 procurement_client.py`. It stops the engine by pid on exit.
- `golive_import.py`: brings 300 open requisitions from a generated spreadsheet (`legacy_pos.csv`) into the engine through a mirror version, a disposition file (`disposition.json`) and a cutover publish. Step 10 of the client runs it.
- `modules/reimbursement-v2.yaml`: my v2 (cost centre, `approve` proposable, policy rule enforced at approval, `ReturnReason.OTHER` merged into `NEEDS_DETAIL`). `modules/golive/reimbursement.yaml`: the mirror twin needed for the import. `modules/golive/cutover-with-backfill.yaml`: the cutover version the import needed.
- `agent-tools.json`: 19 tool definitions my own generator wrote from `declaration()`.
- `run-core.out`, `run-runner.out`: console output of the two runs. `p*.py`: the exploratory probes the entries below quote; they expect a core service already listening on 127.0.0.1:8793 (`core/target/debug/objectflow-service --port 8793`), except `p15.py`, which expects the studio server with the runner backend on 8796.

**Stand-in module: `reimbursement.yaml`**, not `approvals.yaml`. Its `Claim` is a purchase requisition in all but name: a person's request to spend, made of lines with amounts in `money(USD)` and categories with a policy limit, reviewed, approved under a business function's authority, then recorded in the accounting system (Xero, through an external evaluator), which stands in for raising the PO in the ERP. It has an agent type, flags, invariants and four declared metrics. `approvals.yaml`'s `PurchaseRequest` cannot carry its item or any amount through a transition and has no actor type (entry 25). No sample covers everything: nothing in `reimbursement.yaml` is proposable and it has no mirror type, so I wrote a v2 and a go-live variant myself, which is what a real team would do anyway.

**Result in one line:** every step works end to end against the Rust core's sandbox service, with workarounds; nothing could go to production, because the only service that exists is an in-memory, single-caller sandbox with a frozen clock and no way to reach a real external system.

---

### Entries

#### 1. Nothing tells a developer how to start the engine

- **Trying to:** start the engine and load a module (step 1).
- **Where I looked:** `README.md` (line 7: "**Status: in design.** There is no runnable code yet."; line 132: "The engine's core is being built in Rust behind an HTTP service (ADR-0132, ADR-0150), slice by slice"); `docs/DESIGN.md` §2; `core/` (no README); `apps/flow-studio/README.md` (how the studio runs, not how an app runs the engine); `docs/adr/0153-the-cores-first-slice.md` (names the crate `objectflow-service`); finally `apps/flow-studio/server/flowstudio/core.py` and the doc comment of `core/crates/objectflow-service/src/main.rs`.
- **What happened:** the README contradicts itself (no runnable code / being built). No user-facing document gives the command. `core.py` raises `"no cargo at …; the core is built with the toolchain core/rust-toolchain.toml pins (core/README.md)"`, but `core/README.md` does not exist. The binary ignores `--help` and unknown arguments and silently binds a default port: `objectflow-service --help` → stderr `listening on http://127.0.0.1:8712`. The working command, learnt from `core.py`: `cargo build -q -p objectflow-service` in `core/`, then `core/target/debug/objectflow-service --port 0`, parsing `listening on http://127.0.0.1:<port>` from stderr.
- **Severity:** costly (the first hour went to finding out whether anything runs, and how).
- **Did instead:** copied the studio's launcher into `procurement_client.py` (`Engine.start`).
- **Would have helped:** a `core/README.md` (or an "Integrating an application" page) with the build and run command, the flags, the default port, and a correct status line in the README.

#### 2. There is no deployable service: in-memory only, frozen clock, loopback only

- **Trying to:** judge what deployment would look like (binary, persistence, PostgreSQL, configuration).
- **Where I looked:** `docs/DESIGN.md` §2 (line 101: "The core's service as built is a sandbox's, serving one caller … a deployment's service, opening a store of its own, is not yet built."); `core/crates/objectflow-core/src/store.rs:1307-1308` (`Store::sandbox` → `Connection::open_in_memory()`, the only constructor); `core/crates/objectflow-service/src/main.rs:1282` (`TcpListener::bind(("127.0.0.1", port))`); `Cargo.toml` (rusqlite only; no PostgreSQL driver).
- **What happened:** the only store is SQLite in memory, rebuilt on every `/sandbox/load` and lost when the process stops. Before a load every operation answers `409 {"fault":"no modules are loaded; load them first"}`. The clock starts at `2026-01-01T00:00:00Z` and moves only through `POST /sandbox/clock`, so every event my app recorded is stamped `2026-01-01…` whatever the wall time. The listen address is hard-coded to 127.0.0.1, so the engine cannot be reached from another container without a proxy. There is no configuration file, no health or version endpoint, no request logging (stderr carries one line), and no graceful shutdown (SIGTERM, exit −15). Every request runs under one global mutex ("a sandbox serves one caller").
- **Severity:** blocker for any deployment, even a pilot with real data.
- **Did instead:** ran everything in the sandbox and treated it as a functional prototype.
- **Would have helped:** a store opened from a configured SQLite file or PostgreSQL DSN, the wall clock, a configurable bind address, a health and version route, and a stated date or milestone for "a deployment's service".

#### 3. The documented bootstrap is not the real one, and a sandbox load runs no checks

- **Trying to:** load my module the documented way.
- **Where I looked:** `docs/design/publish-and-import.md` §1 (a store is created at version 0 with its operator; the first version is a `DeclarationChange` drafted, submitted and published); `docs/DESIGN.md` §2 (sandbox controls); `docs/adr/0153-the-cores-first-slice.md` decision 5 ("The core does not check a module before it loads it in this slice").
- **What happened:** with no store you cannot draft a change. The only entry is `POST /sandbox/load {"files":[{"name":"reimbursement.yaml","text":"…"}]}`, which installs version 1 directly, and `load` with no files is refused (`400 load takes the module files, in import order`). The core's load accepts a broken module: I changed `pay`'s `from: VERIFIED` to `from: NOPE`, and the core answered `{"loaded": true, …}` (`p2.py`). The same text through the studio server's runner backend is refused with findings: `Claim.pay names the state NOPE, which Claim does not declare` and five `required` findings. The core's publish does check: the same text as a `DeclarationChange` was refused at submit as `unsatisfied dry_run_clean`, with the same findings in the dry run (`p17.py`).
- **Severity:** annoying (the publish path is safe; only a direct load is unchecked).
- **Did instead:** run `scripts/check-flows.py` on every module before loading it.
- **Would have helped:** the core's load running the same checks as its publish, or a load that answers `loaded: false` with findings, as the studio's does.

#### 4. There is no wire specification, and the wire differs from the documented API in many places

- **Trying to:** write a client, typed if possible, from the documentation.
- **Where I looked:** `docs/design/library-api.md` (line 3: "Transport bindings come after and are not here"; line 722: "Twenty operations"); `docs/adr/0150-the-services-wire-and-the-flow-studios-stack.md` (line 11: "no OpenAPI description exists"; line 15: "The OpenAPI description is written from these, operation by operation, as the studio's slices use them"); a search for `*openapi*` (none); then `core/crates/objectflow-service/src/main.rs` in full, and `apps/flow-studio/web/src/session/player.ts` for publishing.
- **What happened:** the only complete description of the wire is the service's source. Differences from `library-api.md`, all observed:
  - `/v1/batch` does not exist: `POST /v1/batch {"requests":[]}` → `404` with an empty body.
  - Responses carry a `not_computed` list the documents never mention, and drop documented fields: `Unsatisfied` has no `unknown`, `objects`, `proposable` or `consulted`; a `Flag` has no `unknown` or `objects`; an `Event` has no `object_seq`, `source`, `taint_version`, `txn`, `retries` or `flags`; `InvariantViolated` has no `objects`.
  - Shapes differ from the Python signatures: `availability` returns a bare array; `check` returns `{verdict, unevaluated}`; `history` returns `{events, not_computed}`, with legacy entries of another shape in the same list; `get` adds `open`; `declaration` returns the module's YAML for the type, re-encoded as JSON, plus `enumerations`, `version`, `source_ref`, `source_digest` and `person_dimensions`, not "the rule set … rendered also as readable text and as agent tool schemas" (DESIGN.md line 632).
  - `follow` on `get` and `history` is accepted and ignored. Reads need no actor at all: `POST /v1/get {"id":"claim-1"}` → `200`.
  - How to draft a `DeclarationChange` (`type: "DeclarationChange"`, `transition: "draft"`, `inputs: {"source": {"<module>": "<yaml text>"}, "source_ref": "…"}`) and that the store's first actor is called `"operator"` I found only in the studio's `player.ts` (lines 28 and 290-300); the built-in type's declaration lists no inputs for `draft`.
- **Severity:** costly. Most of my time went here, and a TypeScript client written from `library-api.md` would have mistyped half the response shapes.
- **Did instead:** read the service's source and wrote the client to what it does.
- **Would have helped:** an OpenAPI document generated from the service, or at least a wire page listing each route, its request body and its response, with `not_computed` explained.

#### 5. The refusal fields the docs tell an agent to branch on are never computed

- **Trying to:** handle a refusal "the way the docs say a caller should" (step 4).
- **Where I looked:** `README.md` line 31 ("A refusal is an instruction"); `docs/design/renderers.md` §4, line 291 ("`objects` are the checklist items still unchecked, so an agent works on them rather than guessing; `proposable` says whether filing a proposal would be accepted"); `library-api.md` §4.
- **What happened:** `POST /v1/request {"actor":{"id":"rev@corp"},"transition":"complete_review","object_id":"claim-7"}` → `{"kind":"unsatisfied","remedy":"dependent","clause":"lines_classified","not_computed":["unknown","objects","proposable","consulted"]}`. Every `unsatisfied` verdict in both transcripts, and in every offer, has the same `not_computed`. A `dependent` refusal never says which objects must change, and a `delegable` one never says whether proposing would be accepted. The runner backend leaves them out too.
- **Severity:** costly for the agent: the headline "the remedy points at what to work on" does not exist on the wire.
- **Did instead:** the agent re-derives the objects with queries it has to hand-write per clause (`query ClaimLine filter claim.number == "…"`, then reads `nature` on each), and reads `proposable` from `declaration()`.
- **Would have helped:** `objects` and `proposable` computed, even before `unknown` and `consulted`.

#### 6. A remedy can send an agent the wrong way

- **Trying to:** drive the agent by the remedy class alone, as renderers.md §4 says ("`remedy` is what an agent should branch on rather than on the clause name").
- **Where I looked:** renderers.md §4; library-api.md §4 (`InvariantFailure.remedy` is inferred: `SELF_SERVICEABLE` where the transition takes an input that the invariant reads).
- **What happened:**
  - The agent confirming a line's nature: `{"kind":"invariant_violated","remedy":"self_serviceable","failures":[{"object_id":"claimline-8","type":"ClaimLine","invariant":"confirmed_by_a_person","remedy":"self_serviceable"}]}`. "Fix the request" is wrong: the invariant reads the actor's kind, so no input the agent can send will ever pass. A literal agent retries with other values. The truthful remedy is delegable (a person must confirm).
  - `submit` on a draft with no lines is `unsatisfied has_lines, remedy unreachable_from_here` ("stop"), though adding a line fixes it; under v2, approving an over-policy requisition is `unreachable_from_here`, though `return_for_changes` is the way forward. Both remedies are the module author's choice, and nothing checks them.
  - `stale` is `self_serviceable`, the same class as a mistyped input, so the remedy alone cannot tell "re-read and resend" from "correct your input"; the app has to branch on `kind` as well.
- **Severity:** costly for an autonomous agent; annoying for a UI.
- **Did instead:** `explain()` in the client branches on `kind` first and on remedy second; for invariant failures that read `actor.kind` the app has to special-case them.
- **Would have helped:** inferring `delegable` for an invariant that reads `actor` and not inputs, and a checker notice when a declared remedy contradicts an obvious way forward (`has_lines` on a state that offers `add_line`).

#### 7. `invalid_input` never says what is wrong or what notation is expected

- **Trying to:** show a person why their entry was refused.
- **Where I looked:** ADR-0150 decision 2 (the value notation: money `"USD 19.99"`, a date `"2026-10-04"`, a reference is the object's id); library-api.md §4 (`InvalidInput.inputs`: "every input at fault, by name").
- **What happened:** each of these answered exactly `{"kind":"invalid_input","remedy":"self_serviceable","inputs":["<name>"]}` and nothing else (`run-core.out`, `p5.py`, `p18.py`, `p19.py`): money as a JSON number `1500`; `"1500.00"`; `"SGD 500.00"` into `money(USD)`; `"USD 1.234"`; `"usd 5.00"`; a date given as `"2025-12-29T00:00:00Z"`; `"2025-02-30"`; a reference given as the user's login `"alice@corp"`; an input the transition does not take (`"colour"`); a title containing NUL; `inputs` sent as an array (named `claimant`!). A missing required input is instead `unsatisfied` with a generated clause, `approving_function_provided` or `title_provided`, which no `declaration()` lists, so the app must know the `<input>_provided` naming convention. The notation is lenient in places it does not document: `"USD  5.00"` (two spaces), `"USD 5"` and `"USD 5.5"` are accepted and normalised.
- **Severity:** annoying: every form must re-implement validation to give a useful message.
- **Did instead:** client-side validators per declared type, and a mapping of `<name>_provided` to "is required".
- **Would have helped:** a reason per input (`unknown_input`, `malformed`, `wrong_currency`, `no_such_object`) and the generated clauses listed in `declaration()`.

#### 8. An amount too large is a 500, not a refusal

- **Trying to:** make sure a user's typo cannot break the service.
- **Where I looked:** library-api.md §7 (the closed list of faults).
- **What happened:** `POST /v1/request {"actor":{"id":"operator"},"transition":"define","type":"ExpenseCategory","inputs":{"name":"x","policy_amount":"USD 99999999999999999999.00"}}` → `500 {"fault":"the store: a decimal past 64 bits"}`. The same for a line's `amount` `"USD 99999999999999999.00"`. The store kept answering afterwards.
- **Severity:** annoying (a client retrying 5xx as transient will retry a typo).
- **Did instead:** cap amounts in the app.
- **Would have helped:** `invalid_input` for a value outside the type's range.

#### 9. Actors and references are named differently, and the two backends disagree on it

- **Trying to:** name the acting user and the requester the way my auth system knows them (`alice@corp`).
- **Where I looked:** library-api.md §3 (`Actor.id`: the store "resolves `id` to the object holding that actor identity"); ADR-0150 ("a reference is the object's id").
- **What happened:** on the core, the actor may be the login (`{"id":"alice@corp"}`) or the object id (`employee-1`); a reference input must be the object id: `inputs: {"claimant": "alice@corp"}` → `invalid_input ["claimant"]`. On the runner backend, an actor named by its login is refused as `actor_known`, and a login in a reference input crashes the server: `500 {"fault":"KeyError: 'alice@corp'"}` (`run-runner.out`, step 2).
- **Severity:** annoying.
- **Did instead:** the app keeps a map from each user id to the engine's object id (`Run.actor()` in the client).
- **Would have helped:** references resolvable by a declared identity or unique key, as actors are; one rule for both backends.

#### 10. Every app user must first be created in the engine by the operator

- **Trying to:** let a signed-in user create a requisition.
- **Where I looked:** library-api.md §3; flow-format examples (`setups: company`).
- **What happened:** `{"actor":{"id":"alice@corp"},"transition":"draft",…}` before onboarding → `{"kind":"unsatisfied","remedy":"dependent","clause":"actor_known"}`. A fresh store knows only `operator`. Each person and each agent must exist as an object of a type marking an `identity` attribute (`Employee.onboard`, `AssistantAgent.register`), requested by an actor the store already knows.
- **Severity:** annoying (a user-directory sync job the app must own: onboarding, offboarding, renames).
- **Did instead:** the client onboards its three users and the agent as the operator at start-up.
- **Would have helped:** a documented pattern for syncing an identity provider, ideally a bulk path (the import cannot do it for an owned `Employee` type, entry 22).

#### 11. `availability` is actor-blind, lists built-ins, omits required-ness and creations, and `check` invents ids

- **Trying to:** render a worklist's action bar for a given user (step 3).
- **Where I looked:** DESIGN.md §10; renderers.md §4 and §5 ("`declaration(type)` gives the input schema … `availability(id)` gives the button's state").
- **What happened:** for the requester, the approver and the agent, the offers are byte-identical: `availability` never looks at the actor (ADR-0114), so every button that is "available" is offered to everyone, including `erase_personal_data` and the built-in label `record` (with `creates: "label"`), which appear on every object, the proposal included. `inputs` is `{"amount":"money(USD)","category":"ExpenseCategory",…}` with no required or optional marking, so building the form also needs `declaration()`. An initial transition (`draft`) is never offered (`reads.rs:588` skips initial transitions); `check` with a type covers it, but its answer `{"verdict":{"kind":"satisfied","object_id":"claim-11","events":[15],…}}` names an id and an event position that were never created, and the next real creation got `claim-11` and event 15.
- **Severity:** annoying.
- **Did instead:** `render_offers()` drops label recording and other-state transitions, takes required inputs from `declaration()`, and the app filters by its own role map.
- **Would have helped:** a required flag per input in the offer, and `check` of a creation answering without an object id.

#### 12. A requisition's lines cannot be listed from the requisition, or queried by its id

- **Trying to:** show a requisition with its lines, the core of a procurement screen.
- **Where I looked:** library-api.md §5 (`Object.attributes`); DESIGN.md §10 (`query` filter); declaration-syntax.md §8 literals (line 1313: no literal for an object id).
- **What happened:** `get` of a requisition with three lines returns `"lines": null` (and `delivery.yaml`'s `checklist_items` and `approvals` likewise), while its derived `total` counts the lines. `query ClaimLine filter 'claim == "claim-7"'` → `{"items":[],"refused":null}` (silently compares an id with a string). `filter 'claim == claim-7'` → `501 {"fault":"not run: Object(\"claim-7\") - Int(7)"}`: the id is parsed as a subtraction.
- **Severity:** costly (every parent/child screen needs a workaround).
- **Did instead:** `filter 'claim.number == "EXP-00001"'`, through the parent's unique business key (`Run.lines_of`), or the `events` of the `add_line` request (`events` resolves the positions a verdict returns, and the creation event names the line).
- **Would have helped:** `get` returning a set reference's members, and a way to compare a reference with an id in a filter.

#### 13. No paging anywhere, and unknown request keys are ignored

- **Trying to:** page a worklist (step 6).
- **Where I looked:** DESIGN.md §10 (`query(type, filter, order, cursor, fields)`); library-api.md §8 ("Cursors are opaque strings").
- **What happened:** `query`, `available` and `metric` return everything on one page with `"cursor": null`, and `export` returns every line with the cursor after the last. Sending a cursor to `query` or `metric` is a fault: `400 {"fault":"a query's page holds every object, so it has no cursor to continue from"}` (and the same for a metric's groups); `available` ignores one. There is no page size; `"limit": 1` is ignored and all three items come back. Any misspelt key is ignored too: `{"type":"ClaimLine","fitler":"amount > USD 1000.00"}` returns every line.
- **Severity:** annoying at 300 open POs, costly as history grows.
- **Did instead:** page client-side.
- **Would have helped:** a `limit` and a real cursor, and `400` on unknown keys.

#### 14. No text search in the filter language

- **Trying to:** let a buyer find a requisition by words in its title.
- **Where I looked:** declaration-syntax.md §8 (operators and literals).
- **What happened:** `filter 'title contains "Keyboard"'` → `{"items":[],"refused":"malformed_filter"}`. There is no `contains`, `like` or prefix match.
- **Severity:** annoying (search is app-side by design, DESIGN.md §12).
- **Did instead:** nothing; the app would feed `export` into its own index.
- **Would have helped:** an explicit statement in an integration page that search is the app's, with the recommended `export`/`pull` feed.

#### 15. History shows what was sent, not what changed, and some values are JSON inside strings

- **Trying to:** render a requisition's history as an audit view (step 6).
- **Where I looked:** library-api.md §5 (`Event.payload`); DESIGN.md §7.
- **What happened:** each event's `payload` is `{"inputs":{…}}` only. `submit` shows `inputs={}` although it set `submitted_at` and `first_submitted_at`, and `approve` does not show `approved_at`. Line events live on the lines' own histories. An imported event shows the legacy key (`"claimant":"E1"`), not the id it resolved to. `Proposal.inputs`, `DeclarationChange.source`, `report` and `evidence` are JSON documents inside string attributes (`"inputs":"{\"approving_function\":\"businessfunction-5\"}"`). The runner backend's history events have no `payload`, `cause`, `reads`, `context` or `principal` at all.
- **Severity:** annoying.
- **Did instead:** the audit view diffs consecutive `get`s, which `history` cannot give for a past version, and double-parses those attributes.
- **Would have helped:** the effect's writes in the event, and structured values for structured attributes.

#### 16. An external rule (the ERP check) cannot be connected to anything real

- **Trying to:** gate "record and verify the PO" on the ERP, as the sample does with Xero (step 7).
- **Where I looked:** library-api.md §3 (`EvaluatorSource` is injected in-process "Production passes the real integrations"); DESIGN.md §2; the service's source.
- **What happened:** the service has no evaluator mechanism other than the sandbox's fixed answers, one per function whatever its arguments: `POST /sandbox/evaluators {"answers":{"xero.bill_matches":"satisfied"}}`. With no answer set, the request is a fault and not a verdict: `501 {"fault":"not run: the evaluator function xero.bill_matches, which the sandbox gives no answer for"}`. There is no callback, webhook or plug-in through which a deployment could answer `bill_matches(bill_id, number, total)`, and an embedded Rust `EvaluatorSource` is excluded by design ("No caller embeds the core", DESIGN.md §2).
- **Severity:** blocker for any module with an external guard (budget checks, vendor status, ERP read-back), which a procurement flow needs.
- **Did instead:** set fixed answers in the sandbox.
- **Would have helped:** an evaluator protocol the service calls (an HTTP callback per evaluator, with a timeout and `fresh:`), or an "answer pending" verdict the app completes.

#### 17. The two backends are not interchangeable, and the runner is not usable by an app

- **Trying to:** find out from the docs which engine an app should use, and confirm my client works on both.
- **Where I looked:** `apps/flow-studio/README.md` ("so the backend behind it can change: the Python reference runner, or the Rust core"); `docs/design/flow-studio.md` §2 ("Two backends, one wire"); DESIGN.md §2 (the runner is the oracle, the core is the service).
- **What happened:** the runner has no service of its own; the only way to reach it over HTTP is the studio's server (`python -m flowstudio --backend runner --port 0`). Running my client against it (`run-runner.out`): `POST /v1/query` → `404 no operation /v1/query`; `idempotency_key` → `501 not run: a request's idempotency_key`; `import_batch` dry run → `501 not run: a dry run`; `subscribe` → `unknown_transition`; proposing a non-proposable transition → `400` fault (the core: `200 not_requestable`); money given as a JSON number (`1500`, `500.0`), with no currency (`"500"`) or in the wrong currency (`"SGD 5.00"` into `money(USD)`) is accepted (`p15.py`, `run-runner.out`; the core answers `invalid_input` to each); an actor named by login is refused; a login in a reference input is `500 KeyError`. The oracle accepts values the core refuses.
- **Severity:** annoying (the docs point at the core once read closely, but "the backend can change" is misleading for an app).
- **Did instead:** used the core only.
- **Would have helped:** one sentence: "Applications use the core's service; the runner is a test oracle reachable only through the studio."

#### 18. Error and not-found shapes are inconsistent

- **Trying to:** write one error handler for the client (step W).
- **Where I looked:** ADR-0150 decision 1 ("A fault is a `4xx` or `5xx` with a `fault` message: a body that is not the operation's, an operation that does not exist …"); library-api.md §7.
- **What happened** (`transcript-core.md`, step W): malformed JSON → `400` with plain text `Failed to parse the request body as JSON: key must be a string at line 1 column 2`; no content type → `415` plain text; unknown operation → `404` with an empty body; GET → `405` with an empty body. Not found is different on every route: `get` → `200 null`; `history` → `200 {"events":[]}` (cannot tell "no such object" from "no history"); `availability` → `404 {"fault":"no object claim-999"}`; `request` → `200 {"kind":"not_found"}`; `declaration` of an unknown type → `404` fault; `metric` → `200 refused:"unknown_metric"`; `query` → `200 refused:"unknown_type"`. An actor sent as a string (`"actor":"operator"`) → `400 {"fault":"a request names its transition and its actor"}`, which it did. The service down → connection refused, as expected.
- **Severity:** annoying.
- **Did instead:** `Wire.raw` normalises non-JSON bodies, and each call site handles its route's not-found.
- **Would have helped:** a JSON `fault` body on every 4xx/5xx and one not-found convention for reads.

#### 19. No agent tool generator, and nothing in a declaration stops an agent taking a person's transition

- **Trying to:** generate agent tools from the declaration and decide whether to hand them to an LLM (step 11).
- **Where I looked:** `README.md` line 32 ("Agent tool definitions are generated from the declaration"); DESIGN.md line 632 ("rendered also as … agent tool schemas"); `docs/design/renderers.md` §3 (hand-written examples; line 21: "nothing generates it yet" for the rule set); `core/`, `scripts/`, the studio (no generator: `grep -rln input_schema` finds only `reads.rs`, which builds offers, and a doc checker).
- **What happened:** I wrote a generator (`tools_for()` in the client) from `declaration()`; it produced 19 tools (`agent-tools.json`). The shapes are fine for an LLM. But the set includes `claim_approve`, `claim_pay`, `claim_erase_personal_data` and `claimline_confirm_nature`, and the engine accepts most of them from an agent: as `bot`, an `AssistantAgent`, I drafted a claim in Alice's name, submitted it, completed its review and approved it, then erased its personal data, all `satisfied` (`p20.py`). Only `confirm_nature` is refused, because that module's author wrote an invariant on `actor.kind`. "Agents draft, people approve" (README line 34) holds only where the module author writes it as a rule; segregation of duties (the requester approving her own requisition) is entirely the app's.
- **Severity:** costly.
- **Did instead:** would hand the agent a curated subset (draft, add_line, suggest_nature, and `approve` as a proposal only) and enforce roles at the app's edge.
- **Would have helped:** the generator the README promises, with a per-actor-kind filter, or a declarable "agents may only propose this" that the engine enforces.

#### 20. Proposals work, but the approver's inbox gets little help

- **Trying to:** let the agent propose an approval and a person approve it (steps 8 and 8b).
- **Where I looked:** DESIGN.md §9; flow-format.md (`proposable`); `delivery.yaml` (the only proposable sample).
- **What happened:** `reimbursement.yaml` has no proposable transition. Proposing one is refused as `{"kind":"not_requestable","remedy":"unreachable_from_here","parents":[]}`, the verdict for an only-via transition, which describes something else. Offers never say a transition is proposable (`proposable` is not computed, entry 5), so the agent must read `declaration()`. A pending proposal offers `approve` as `available` without evaluating the target's guards, so the inbox cannot preview whether approving will succeed; approving too soon returns the target's refusal and leaves the proposal `PENDING` with `last_verdict` (with `delivery.yaml`, `p11.py`: `unsatisfied signed_off, delegable`). A successful approval returns the target's `Satisfied` (`object_id: "claim-20"`), not the proposal's version or state. After my v2 made `approve` proposable, the round trip worked: proposal filed by `procure-bot` with a context, approved by `rev@corp`, executed, and the requisition's `approve` event records `rev@corp` with `cause` the proposal's approval and the agent's context.
- **Severity:** annoying.
- **Did instead:** read the proposal back after approving; parse `inputs` from its JSON string.
- **Would have helped:** a distinct verdict for "not proposable", and the target's availability evaluated on a proposal's `approve` offer.

#### 21. The publish report hides some of its sections, and nothing checks who approves

- **Trying to:** publish v2 over live requisitions and review its impact (step 9).
- **Where I looked:** publish-and-import.md §1 and §2.
- **What happened:** the flow is clean (entry "worked well"). But every report carries `"not_computed":["invariants","sweepable","unsweepable","queryable_derives","worst_case_fanout","observed_fanout","replaced_creations","creations_past_first_state","observing_clauses","flags"]`, so a reviewer does not see what §2 promises. The agent drafted and submitted the change, and the approver who installed it was not checked: any actor can publish, the drafter included (documented as the app's job).
- **Severity:** annoying.
- **Did instead:** read the dry run's findings, which carry what matters (below).
- **Would have helped:** the missing sections, or a note on the wire page saying which ones are not built.

#### 22. Importing open POs needs a mirror twin of the whole module, a cutover publish, and is impossible later

- **Trying to:** bring 300 open POs from a spreadsheet at go-live (step 10).
- **Where I looked:** publish-and-import.md §4 to §7; flow-format.md §4.15; library-api.md §6 (`import_batch`, `lookup`).
- **What happened:** importing into the live module's `Claim` is refused: `{"refused_types":["Claim"],"refusal":{"type":"Claim","legacy_key":"PO-LEGACY-1","reason":"Claim is owned by this store, not a mirror"}}`. The documented path is: a version in which the type and everything it references is a `mirror`; the import; then a cutover publish. A mirror may not reference an owned type, so `Employee`, `BusinessFunction`, `ExpenseCategory`, `Claim` and `ClaimLine` all had to become mirrors (`modules/golive/reimbursement.yaml`, 170 lines), each with an `external` identifier, and the checker required the mirror's erasure to call its parts' erasure. Then:
  - Dry run: two classes (`Claim.state` "is not declared", `Claim.claimant_provided` "is absent"). Excluding them created a third class, `ClaimLine.claim` "names an excluded object", so deciding is a loop of dry runs; the second round's findings silently dropped the excluded `claimant_provided` class while still listing `state`.
  - Import: 619 objects written in one transaction (20 employees, 1 function, 2 categories, 298 claims, 298 lines) in a few seconds.
  - Cutover publish of the real module: refused `decision_unresolved`, with the finding `invariant:Claim.approved_invariant x1 … ['claim-324']`, an invariant generated from `APPROVED`'s required attributes that `declaration()` does not list. I added a `migration: backfill:` to the cutover version; my first expression stamped `approved_at` on 149 claims, which the report's count exposed; the corrected one touched 1, and the publish applied.
  - `lookup` needs the qualified key: `lookup legacy "Claim:EXP-04001"` finds it, `"EXP-04001"` returns `null`.
  - After cutover, `Claim` is owned, so a second batch (a late spreadsheet, another business unit) can never be imported into it.
- **Severity:** costly (it works, and it is principled, but it is days of module authoring and rehearsal for a team, and closes the door on phased onboarding).
- **Did instead:** `golive_import.py`, which runs the whole sequence.
- **Would have helped:** an import into an owned type through the built-in assertion, with the same disposition file, or a generator for the mirror twin; generated invariants listed in `declaration()`.

#### 23. Values need a parser for the model's notation, which varies by magnitude

- **Trying to:** display metric values (step 7).
- **Where I looked:** ADR-0150 decision 2; declaration-syntax.md §8 ("Units": `s`, `min`, `h`, `day(s)`, `week(s)`).
- **What happened:** durations came back as `"15 days"`, `"3 weeks"`, `"1431 h"` and `"0 s"` for the same metric family; a month dimension is `"2026-01"`. A standard metric with a window was refused with a bare code: `metric Claim.time_in_state over "30 days"` → `{"rows":[],"refused":"no_window"}`.
- **Severity:** cosmetic.
- **Did instead:** a small parser for number plus unit.
- **Would have helped:** the notation's grammar in one place on a wire page, or a canonical single unit (seconds) beside the human string.

#### 24. No integration guide; the facts an app needs are spread over a dozen documents

- **Trying to:** learn the integration surface "the way a developer would", starting from the README.
- **Where I looked, in order:** README.md; DESIGN.md (152 KB; §2, §10, §12, §13); library-api.md; ADR-0150 (value notation); ADR-0153; publish-and-import.md; flow-format.md §4.15 and §11; renderers.md; flow-studio.md; then the service's source and the studio's TypeScript.
- **What happened:** each document is precise, but none is written for an application developer: they are design records, dense with ADR cross-references and amendment histories (library-api.md opens with ten "Amended …" paragraphs). The README's "What it looks like" is in a text notation that DESIGN.md says "is no longer written by anyone"; the format apps actually write is YAML.
- **Severity:** costly.
- **Did instead:** read the code.
- **Would have helped:** a twenty-minute "build an app on ObjectFlow" page: start the service, load a module, create, offer, refuse, publish, import, with the wire's JSON.

#### 25. `approvals.yaml`'s PurchaseRequest cannot carry a purchase

- **Trying to:** use the sample the brief named, which has a `PurchaseRequest`.
- **Where I looked:** `docs/design/flow-format/examples/approvals.yaml`.
- **What happened:** `start` takes no inputs, so the declared `item` attribute can never be set: `{"transition":"start","type":"PurchaseRequest","inputs":{"item":"Laptop"}}` → `{"kind":"invalid_input","inputs":["item"]}`. There is no amount and no actor type, so only `operator` can act (`alice` → `actor_known`). `approve`'s guard `submitted` can never fail, since `submit` sets `submitted_at`.
- **Severity:** annoying (only for choosing a sample).
- **Did instead:** used `reimbursement.yaml`.
- **Would have helped:** a procurement sample (requisition → PO → receipt) with an agent type and a proposable approval.

#### 26. Notifications, timers, files and permissions are all the app's, and only some of it is said plainly

- **Trying to:** list what the app must still build (step 12).
- **Where I looked:** README "What it is not"; DESIGN.md §2, §12, §13; DESIGN.md line 191 (files).
- **What happened:**
  - **Authentication and permissions:** clearly the app's (README, DESIGN §13: "A caller that can reach it can name any actor the store knows"). With the engine on a reachable port, any process can act as anyone.
  - **Notifications:** pull only. `subscribe`, then a poller calling `pull` and `acknowledge` with the settled cursor; worked first time. No push or long-poll.
  - **Timers and escalation** ("PO not acknowledged in five days"): an app scheduler calling `available` and requesting transitions, with backdating only where a transition is declared backdatable; the reimbursement module has none.
  - **Attachments:** DESIGN.md line 191 says a `file` attribute "holds a content-addressed reference to a blob … with its hash, size, media type and provenance". On the wire it is any string: `"receipt": "quote-4471.pdf"` is accepted, and a JSON object with a hash is `invalid_input`. Storage, hashing and erasure of the bytes are the app's.
  - **Reporting:** the declared metrics plus `export` (JSON lines, one page).
  - **Search:** the app's (entry 14).
- **Severity:** annoying (the split is defensible and mostly stated, but scattered, and the `file` claim does not match the wire).
- **Did instead:** noted each in the client's step 12.
- **Would have helped:** one table in the integration page: "the engine does / your app does", with the recommended pattern for each.

---

### What worked well

- **Verdicts as values.** `200` with `kind` on every refusal, an exhaustive set, a remedy on each. Optimistic concurrency works exactly as documented: `{"kind":"stale","cause":"expected_version","expected":6,"actual":7}`. An idempotent retry replays (`"replayed": true`), and a reused key for another request is a clear `409 key_reused`.
- **Exact values.** Money and decimals as strings (`"USD 4880.00"`), RFC 3339 UTC timestamps, enumeration members by bare name: no float ever touched an amount.
- **The publish dry run.** The report named, per object, what v2 would migrate (`mapping:removed_members:ReturnReason.OTHER`, `claim-24`) and what it would newly refuse (`newly_enforced:Claim.approve.lines_within_policy`, `claim-22`). At cutover it caught an unresolved invariant on one imported object, and its counts exposed my wrong backfill (149 objects instead of 1) before anything was installed. Approval at the version read, the `migrate` event on each object with the mapping it applied, and `declaration(type, version)` for both versions all worked.
- **The import mechanics.** Classes by rule and shape, the disposition file, one transaction for 619 objects, the sequence raised past the legacy numbers (the first new requisition was `EXP-04301`), `entered` times carried into `time_in_state`, and `lookup` by legacy key.
- **Proposals.** Filed by the agent with a context, approved by a person, executed under the rules in force, with the approver as the actor, the cause linking back, and `last_verdict` on a refused approval.
- **Metrics.** Declared metrics computed with their flags (`slow` on `submission_to_payment`), `gaps` and `history_complete`, and the person-naming dimensions marked in `declaration()`.
- **The flow checker.** Fast and specific: it caught `decimal(12,2)` unquoted in a flow mapping with an exact fix, and a mirror's erasure that did not reach its parts.
- **Startup.** The service builds with the pinned toolchain and starts in under a second; one binary, no runtime dependencies.
- **Flags.** An over-policy line is accepted and flagged (`flags: [{"clause":"ClaimLine.within_policy"}]`), offered in advance in `availability`, and counted by a metric: a good fit for procurement policies that warn rather than block.
