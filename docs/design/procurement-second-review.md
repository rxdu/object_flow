# Procurement on ObjectFlow: the second cold review

Status: **repaired**, 2026-10-10: seventeen defects (D561 to D577), each resolved the same day, and seven questions for the author (§5). The first review (`procurement-cold-draft.md`) found fourteen defects and asked ten questions; once those were repaired and built, the author asked for the review to be run again ("go ahead with the cold re-review"), to see whether the integration case is ready.

## 1. How it was run

As the first (`procurement-cold-draft.md` §1), but for three things. **The brief** was given again in ten points, written afresh from the first review's summary of it, so its wording differs and its substance does not: requisitions with lines against a cost centre, drafted by an agent and submitted by a person; approval by amount at three levels in SGD, nobody approving their own; a budget per cost centre with a finance override; three quotes above SGD 10,000; orders to suppliers the ERP owns, an amendment over 10% re-approved; partial receipts, 5% over for raw materials; invoices matched three ways within 2%, payment in Xero; cancellation before receipt or of the remainder after; the metrics; and 300 open orders from a spreadsheet at go-live. **The repository** was at commit `c494d5c`, after D547 to D560 and every question of ADR-0173 were built, the deployment's service and its evaluator transport among them. **The integrator** was asked to deploy the engine as a team would, if it could, and to connect an evaluator to something it ran. Both agents were barred from the defect register, both procurement reviews' records, ADR-0173, `TODO.md` and `LESSONS.md`, worked in their own scratch directories with the repository read-only, and left nothing running and the repository unchanged, as `git status` showed after each. The drafter took 33 minutes, the integrator 26.

**What they built.** The drafter's `procurement` module has 16 types, 65 transitions, 31 conditions, 10 invariants, 24 derived attributes and 7 metrics, with 90 examples passing on the reference runner and on the core, and a go-live version written by `scripts/mirror-version.py` whose 6 examples pass on both; on a deployment's store it imported 1,472 objects of 300 legacy orders, cut over, and operated them, an HTTP stub answering Xero. The integrator ran its twelve steps against a deployment's store throughout: five published versions, three restarts, an ERP and Xero bridge it wrote answering the engine's evaluator calls, 297 legacy orders and 318 receipts imported with three excluded by recorded decisions, and an agent acting through the generated tools with a person approving its proposals.

**How each finding was checked.** As in the first review: each claim framed as a defect was reproduced by the reviewer's own probe against a fresh service, the flow checker or both backends through the Flow Studio's server, over minimal modules written for it, before it was classified; one claim was wrong as stated and is corrected below (D562).

## 2. The verdict

**Both agents: a team could build this procurement system and pilot it on one machine today; it is not ready for production.** Every point of the brief was expressible and enforced, the same examples pass on both implementations, the go-live worked first time on a deployment's store, and the wire's answers explain themselves; both praised the flow checker, and the integrator called it the best part. The first review's sharpest edges, no deployable service, no evaluator transport, rules about who acts refused with the wrong remedy, no integration guide or wire reference, a go-live without tooling, the backends disagreeing on derived values, agent tooling that did not exist, are each gone.

**What stands in the way of production** is the stage the project is at, each tracked in `TODO.md`: a store cannot yet be carried to a newer engine release, so the first upgrade would strand its data; the service serves one request at a time, so a publish or a large import holds every user; reads are not paged. Beside those, the review's own findings, most serious first:

1. A slow external system held every user, since the service asked an endpoint under its one lock (D563).
2. Agent safety fails open: the tools hand an agent every transition no rule refuses it, approvals and payments among them, and a flow must state "a person takes this" on each (§5, question 1); the tools could not read (D564).
3. The guide's pattern for who acted, written through references, vetoes later changes to what it reads, an HR change of manager refused after an approval (D574).
4. A per-person metric recorded as the guide recommends was not marked as naming a person (D565).
5. The import batch's JSON and the evaluator endpoint's call were documented only in a test and an ADR (D572, D573), and the go-live's constraints on the model were found only by trying (D575).

## 3. The findings, each checked

The drafter's entries are `P2-E<n>` and the integrator's `I2-<step>`; §6 and §7 hold the logs as written.

| Finding | Claim | How it was checked | Disposition |
|---|---|---|---|
| I2-reading | README says the deployment's service is not built | read: the status line, against `core/README.md` | D561 |
| I2-7b | events do not record what the evaluator answered | **wrong as stated**: a probe found the verdict in `payload.consulted`, not where `library-api.md` §5 puts it | D562 |
| I2-7a | a slow evaluator stalls every user | measured by the integrator; read: `consulting` held the lock | D563 |
| I2-12a | the tools cannot read | `/v1/tools` | D564 |
| P2-E13, I2-11 | a per-approver metric is not marked as naming a person | a probe on both backends | D565 |
| P2-E2, I2-flow | `actor.id` does not set what a state requires | the flow checker over a minimal module | D566 |
| P2-E15 | the coverage notice lists only-via transitions | the flow checker over `reimbursement.yaml` | D567 |
| P2-E4 | a metric expectation reads an identity as an alias | a one-example module on the runner and the core | D568 |
| I2-11b | a standard metric's `filter` silently returns nothing | probes on both backends; also a `501` on the core | D569 |
| I2-4b | `/v1/get {}` answers `200 null` | a probe | D570 |
| I2-12c | the tools' money pattern accepts three places | read: both renderers | D571 |
| P2-E9, I2-reading | the batch's JSON is only in a test | read | D572 |
| I2-4a | the evaluator call's shape is only in an ADR; a failure is invisible to the application | read; the verdict's `unknown` is not computed | D573; `unknown` in `TODO.md` |
| P2-E6 | an invariant through references vetoes others' changes | the drafter's kept module; by design (`flow-format.md` §4.6) | D574; question 5 |
| P2-E8, P2-E11, I2-flow, I2-10c | the go-live shapes the model; guards are not checked on import | by design (`flow-format.md` §4.15, `publish-and-import.md` §4) | D575; question 7 |
| I2-6b | the label offer is undocumented in the guides | read | D576 |
| P2-E5 | a missing input arrives in two shapes | by design (`flow-format.md` §6) | D577 |
| P2-E12, I2-12b | agents are handed every transition no rule refuses | by design (ADR-0114); both reviewers wrote the rule on every transition | question 1 |
| I2-6a | offers and `available` ignore rules about who acts | by design (ADR-0152); `actor_dependent` marks them | question 2 |
| P2-E10, I2-10a | an unresolvable legacy reference stops the dry run, undecidably | by design (`publish-and-import.md` §4) | question 3 |
| P2-E7 | an example cannot refresh a mirror | the format: an import's alias is given once | question 4 |
| P2-E16, I2-flow, I2-4c | amount routing needs complementary transition pairs, a wrong pick refused `unreachable_from_here` | the format: a transition has one target | question 6 |
| P2-E3 | a presence guard does not set what a state requires | by design (`flow-format.md` §4.5) | `TODO.md`, a suggestion |
| P2-E1, I2-reading | the specification is hard to read | read | `TODO.md`, a reference card |
| I2-1 | the publish report is mostly `not_computed` | known | `TODO.md` |
| I2-1, I2-2 | the operator is a human | by design (ADR-0126) | none |
| I2-2 | a duplicate onboarding names an object that does not exist | the refused creation's would-be id | `TODO.md`, a suggestion |
| I2-3 | a cascade's created id only through `/v1/events`; a part's edit leaves the whole's version | by design (ADR-0151; versions are per object) | none |
| I2-3 | form errors come in rounds; a guard names no field | by design (`DESIGN.md` §6's refusal order) | none |
| I2-4 | a reference of the wrong type is `no_such_object` | by design (ADR-0173 decision 9's amendment) | none |
| I2-5 | no paging; ordering narrow; query fields do not follow references | known | `TODO.md` |
| I2-7c | `check` answers would-be positions | by design: the verdict the request would have | none |
| I2-8 | any actor may pull any subscription | by design (ADR-0114) | none |
| I2-9, I2-10 | publishing and importing hold the service | known: one request at a time | `TODO.md` |
| I2-11 | standard metrics have no window; dimensions are ids | by design: no time dimension; a reader resolves ids | none |
| I2-12c | duplicate proposals; a Proposal's inputs are JSON in a string; an approval answers its target | by design but the second | `TODO.md`, the second |
| — | production: no upgrade between releases, one request at a time, no paging, no authentication | known; authentication by design (ADR-0114) | `TODO.md` |

## 4. What was changed

Each defect's entry in the register says what fixed it and the test that holds it. In short: the README's status (D561); an event's `consulted` and `as_of` at its top level (D562); the service's lock let go while an endpoint answers (D563); three read tools, and every tool's description naming `object_availability` (D564); an identity dimension marked as naming a person (D565); `actor.id` and `actor.kind` provably present (D566); only-via transitions left out of the coverage notice's second part (D567); a metric expectation's text read as written or as an alias (D568); a metric filter reading what its rows do not bind refused `malformed_filter` (D569); `get` and `history` with no id a `400` (D570); money patterns bounded by the minor unit (D571); and the guides: the batch, the endpoint's call, an event, the label offer, the two shapes of a missing input, checking who acted against a fact as it stood, and how the go-live shapes the model (D572 to D577).

## 5. Questions for the author

Each changes an accepted decision or adds a construct, so each is the author's; each carries a recommendation.

1. **A transition a person must take, declared.** Both agents wrote "a person, not an agent" as a record and an invariant on every such transition, and the drafter's first version let the agent approve at the CFO level; nothing warns. ADR-0173's rejected alternative was a declared actor kind evaluated as a guard. **Recommendation:** a marking `taken_by: [human]` on a transition, sugar that generates the attribute recording `actor.kind` and the invariant over it, so the engine still judges only the record (ADR-0114): the refusal stays `delegable`, `tools` leaves the transition out for other kinds, and its offers are `actor_dependent`.
2. **Offers evaluated for who asks.** `availability` and `available` run no invariant (ADR-0152), so an agent's "what can I do" lists approvals it will be refused, and the application makes a `check` per row. **Recommendation:** an offer whose transition takes no input is simulated whole for the asking actor, as `check` does, so a rule over who acts decides it; one awaiting inputs stays `available_with_input` and `actor_dependent`; `available` marks its rows `actor_dependent` rather than simulating each.
3. **An unresolvable legacy reference as a class.** A reference to a key the batch and the store lack stops the dry run at its first row, decided outside the disposition file. **Recommendation:** a `REFERENCE` class, `names nothing`, with every sample, decidable by `EXCLUDE`, the dry run going on.
4. **An example's import refreshing a mirror.** An `import` step's alias is given once, so "the ERP blocks a supplier after it quoted" cannot be an example. **Recommendation:** an `import` naming an existing alias refreshes that object, as `import_batch` does a known legacy key.
5. **A notice for an invariant that can only veto others.** D574's case is an invariant of a record born in a final state reading through references, which no transition of its own can break. **Recommendation:** the checker's notice for it, naming the references and the fix, copying the fact at the decision.
6. **A transition whose target its guards choose.** Routing by amount is a pair of transitions with opposite guards per level, a wrong pick refused `unreachable_from_here`. **Recommendation:** not now: the pair is explicit, `availability` tells a screen which to show, and the guide states the pattern; reconsider if a third review meets it.
7. **An optional reference from a mirror to an owned type.** It would let a legacy order name a new requisition, and break the rule that a type migrates after every type referencing it (`publish-and-import.md` §7). **Recommendation:** no; the guide now says to keep such a link on the new side (D575).

## 6. The drafting agent's log, as it wrote it

The log is model output, kept as written; §3 is the checked reading of it, and the paths it quotes were in the session's scratch directory.

### Procurement on ObjectFlow: evaluation log

Evaluator: a developer new to ObjectFlow. Repository at c494d5c, read-only. Started 2026-10-10 19:18 (+08).

#### Timeline

- 19:18 Read README.md. Clear pitch; points to docs/guides/write-a-flow.md for authoring, integrate.md for apps, core/README.md for the engine.

- 19:18 Read docs/guides/write-a-flow.md (242 lines). Good: one page from empty file to "examples on both implementations", two commands, a table of samples, a "patterns for common needs" list that names almost every pattern this brief needs (lines of a document, approval by amount, "a person confirms", "nobody approves their own", external check, go-live). That list is the most useful page in the repo for this job.
- 19:18 Built the core (`cargo build -p objectflow-service`): already built, 0.05 s. scripts/.venv and the studio server venv already exist on this machine, so setup cost me nothing (a fresh machine would differ; not measured).
- 19:19-19:22 Read flow-format.md (972 lines) §4, §5, §7, §11; declaration-syntax.md §6.9, §8; integrate.md; wire.md; publish-and-import.md §4-§7; test_golive.py; mirror-version.py; reimbursement.yaml + examples (the closest sample).

#### Entries and progress notes, in the order they happened

##### E1. The specification is very hard to read as a newcomer
- **What I was trying to do**: learn the format well enough to write ~10 related types.
- **Where I looked**: docs/design/flow-format.md, declaration-syntax.md §6.9 and §8.
- **What happened**: the normative text is long single paragraphs, sentences carrying several ADR citations, corrections-in-place ("*(Corrected 2026-09-27: ...)*") and step/check codes. Measured: flow-format.md §7's step 3 `names` row (line 676) is one table cell of 1,062 words, 6,083 characters on one line. The rules are there but finding "can a guard read an optional input inside an `or`?" means reading whole paragraphs about effects to infer that the restriction is about effect *steps* only.
- **What I did instead**: worked from reimbursement.yaml and the guide's §5 pattern list, and used the checker as the real documentation (write, check, read the finding).
- **What would have helped**: a reference card (one table per construct: key, allowed values, one-line rule, link) separate from the rationale/history; the corrections and ADR trail moved to footnotes.
- **Severity**: minor (costly in reading time, not blocking, because the samples and checker compensate).


##### E2. A value recorded from `actor` cannot satisfy a state's `required_attributes`
- **What I was trying to do**: require `submitted_by_kind` (assigned `actor.kind` on submit) in every awaiting-approval state, and `amended_by` (assigned `actor.id`) in AWAITING_REAPPROVAL.
- **Where I looked**: flow-format.md §4.5 (the list of values "provably present" is `now`, a literal, an enum value, a non-optional input, a required attribute, or a path of required references).
- **What happened**: `check-flows.py` → `fatal required Requisition.submit enters AWAITING_MANAGER without setting 'submitted_by_kind' ... assign it a value that is provably present`. `actor.kind` and `actor.id` are always present on a request, but are not in the list.
- **What I did instead**: dropped them from `required_attributes`; the "a person submits" rule still holds through the invariant.
- **What would have helped**: count `actor.id` / `actor.kind` as provably present (they cannot be absent on an applied request), or say in §4.5 why not.
- **Severity**: minor.

##### E3. A guard that proves presence does not count for `required_attributes`
- **What I was trying to do**: require `chosen_quote` in ORDERED; `raise_order` is guarded by `quote_chosen: chosen_quote is not null` (deny).
- **Where I looked**: flow-format.md §4.5, the checker's message.
- **What happened**: `fatal required Requisition.raise_order enters ORDERED without setting 'chosen_quote'`, and a separate notice that the supplier guard can be unknown through `chosen_quote`.
- **What I did instead**: dropped `chosen_quote` from ORDERED's required attributes; wrote `chosen_quote is not null and ...` in the supplier guard. The ORDERED state's invariant is now weaker than the business rule.
- **What would have helped**: let a `deny` guard of the form `x is not null` count as setting the attribute for §4.5 (the checker already reasons about guards in step 5).
- **Severity**: minor.

##### W1 (worked well). The checker's findings are precise and say how to fix
- First run: 3 fatals for unquoted commas inside `{ ... }`, each naming line, cause and the fix (`inside { … }, a value containing a comma is quoted`). Second run: 5 fatals, each with line, the rule, and options. Third run: `0 fatal · 0 notices` for a ~1,000-line, 14-type module written in one sitting. Cross-type derived attributes (Budget.committed reads Requisition.committed_amount reads PurchaseOrder.committed_value reads OrderLine), `if … then … else` in a derived attribute, `count(distinct …)`, `min(…)` and invariants through three references all passed without fuss.


- 19:25 Wrote procurement.examples.yaml (12 setups, 66 examples) in one go. `check-flows.py procurement.yaml procurement.examples.yaml` in 4.5 s: `examples: 66 run: 62 passed, 4 failed`. Fixed the 4 (below), then 66/66 on the runner, and `examples-on-core` 66/66 on the core in 8 s.

##### W2 (worked well). The examples caught a real bug in my flow
- `cancelling_releases_the_budget` failed: `expected committed to hold SGD 0.00, and it holds SGD 2500`. My `OrderLine.cancel` moved the line to CANCELLED but did not set `cancelled_quantity`, so a cancelled order still committed its whole value against the budget. One-line fix (`assign: { location: cancelled_quantity, expr: quantity }`). This is exactly what an examples file is for, and the failure message said which value differed.

##### E4. An expected metric dimension that is an identity is read as an object alias when the strings coincide
- **What I was trying to do**: assert `Approval.approver_turnaround` reads `[{ approver: mark, level: MANAGER, value: 2 h }]`. In my setup the alias of Mark's Employee object is `mark` and his `user_id` (an `identity`) was also `mark`, as reimbursement.examples.yaml does (`as: alice, inputs: { user_id: alice }`).
- **Where I looked**: flow-format.md §11.4 `metrics` row ("an object by its alias").
- **What happened**: `expected the metric Approval.approver_turnaround to read a row {'approver': 'mark', 'level': 'ApprovalLevel.MANAGER', 'value': '2 h'}, and it reads [{'approver': 'mark', 'level': 'MANAGER', 'value': '7200'}]`. The row looks equal to the eye; the message prints the raw row (duration as `7200`, enum unqualified) next to the expected form, which hides the real difference. Meanwhile `values: { budget_overridden_by: fiona }` on an identity attribute passed, because there the attribute's type says it is an identity.
- **What I did instead**: made every identity differ from its alias (`user_id: mark@corp`). Then it passed (so the duration and enum forms were never the problem).
- **What would have helped**: the metric comparison should type the expected value by the dimension's declared type (it is known: `a.approver` is an `identity`), or the checker should warn when an alias equals an identity value; and the failure message should print both sides in the same notation.
- **Severity**: minor (cost ~5 minutes; would cost more to someone who did not suspect the alias).

##### E5. A missing required input is `unsatisfied <attr>_provided`, not `invalid_input`, when the attribute is optional
- **What I was trying to do**: prove a rejection without a reason is refused (`reject` has `required_inputs: [rejection_reason]`, and `rejection_reason` is an optional attribute since a draft has none).
- **Where I looked**: the checker's message, then flow-format.md §6 (the `<attr>_provided` row) and §11.4's last paragraph.
- **What happened**: `expected invalid_input, and it is refused as unsatisfied rejection_reason_provided self_serviceable`. This is documented (flow-format.md §6 table, §11.4 last paragraph), I had not absorbed it. The same request for a non-optional attribute would be `invalid_input`, so the verdict of "you left out a required input" depends on the attribute's optionality elsewhere in the type.
- **What I did instead**: expected the generated clause.
- **What would have helped**: nothing much; the checker's message told me the exact expectation to write. A client author (integrate.md §4) should be told that a missing input arrives in two shapes.
- **Severity**: minor.


- 19:30 Added 12 examples for edge cases (rejection at the wrong level, budget "what is left" with two requisitions, withdrawal releasing budget, amendment declined, amend below received, receiving another order's line, invoice rejected, HR changing a reporting line after an approval). Runner 78/78, core 78/78 after the two fixes below.

##### E6. An invariant that reads through references blocks later, unrelated changes to the referenced objects (the documented "record who acted, constrain it with an invariant" pattern)
- **What I was trying to do**: "Nobody approves their own requisition" and "the manager level is approved by the requester's manager". Following write-a-flow.md §5 ("record it in the outcome ... and constrain the record with an invariant"), each decision is an `Approval` part with `approver: actor.id`, and invariants `approver != requisition.requester.user_id` and `level != MANAGER or approver == requisition.requester.manager.user_id`.
- **Where I looked**: write-a-flow.md §5, integrate.md §6, flow-format.md §4.6 ("An invariant is a condition that holds for every object of the type at all times").
- **What happened**: all 66 examples passed. Then I added an example where HR moves Alice to a new manager *after* Mark approved her requisition: the HR sync's `Employee.change_reporting_line` is refused, `invariant_violated Approval.manager_level_by_the_manager dependent`. A historical approval record now vetoes an HR change, and tells the HR sync its remedy is `dependent` (change another object first), which nothing can satisfy short of rewriting history. In production this would jam the HR sync the first time anyone with an approved requisition changed manager.
- **What I did instead**: snapshot the facts at decision time onto the Approval (`requester_id`, `manager_id`, `head_id` passed by the `create` from `requester.manager.user_id` etc.) and write the invariants over the Approval's own attributes only. Kept the first version as `procurement.v0-invariant-through-references.yaml` for reference.
- **What would have helped**: (a) the guide's pattern should say "an invariant that reads another object is re-checked whenever that object changes; to check who acted against a fact at the time, copy the fact onto the record"; (b) a checker notice for an invariant on a type with only final states (a born-final record) that reads through a reference, since it can only ever veto other objects' changes; (c) a way to state "checked when this object is written, not when others are" (a guard that may read `actor`, or an invariant scoped to its own transitions).
- **Severity**: major (silent until production data has the shape; the fix changes the data model).

##### E7. An examples file cannot refresh a mirror, so "the ERP blocked a supplier after it quoted" cannot be an example
- **What I was trying to do**: prove point 5's supplier rule: the ERP blocks Acme after its quote is chosen; the next supplier import says so; the order is refused.
- **Where I looked**: flow-format.md §11.3 (`import` step: "its legacy key the alias").
- **What happened**: a second `import: { type: Supplier, as: acme, state: BLOCKED, ... }` → `fatal names 'acme' is already an alias`. Since an import's legacy key *is* its alias, an example can create a mirror object but never refresh one.
- **What I did instead**: the guard is covered by `a_blocked_supplier_cannot_quote` (imported blocked from the start); the refresh itself is exercised on the core's service below (go-live run).
- **What would have helped**: allow an `import` step to name an existing alias (a refresh), as `import_batch` does for a known legacy key.
- **Severity**: minor.


- 19:31 Go-live. `scripts/mirror-version.py procurement.yaml PurchaseOrder > procurement-golive.yaml`: 0 fatal; module against it with `--previous`: 0 fatal.
- 19:32 Wrote golive/make_legacy_export.py (stand-in for the old system's export: 300 open POs, 760 lines, 419 receipts, 12 ERP suppliers, four dirty rows on purpose) and golive/to_batch.py (the mapping, CSV to import batches). Started the core on a deployment store (`--store golive/store.sqlite --port 0`, pid recorded in golive/service.pid) and published the go-live version as version 1 through DeclarationChange draft/submit/publish. Imported the 12 suppliers (the ERP sync's batch): written 12.
- 19:33-19:35 Orders batch dry runs: (1) refused at the first PO naming supplier S-099; (2) after holding that PO back, classes `OrderLine.category: is not declared` + `OrderLine.category_provided: is absent` (same row, "Raw Mat."), `OrderLine.unit_price_provided: is absent`; (3) after mapping "Raw Mat.", excluding the priceless line surfaced `Receipt.line: names an excluded object`; decided that too. The disposition loop is good: each decision surfaces its consequence as a new class with a sample legacy key.

##### E8. Making one type a mirror makes the whole module a mirror; go-live is one big stage
- **What I was trying to do**: bring in 300 open purchase orders (with lines and receipts) at go-live, while requisitions, budgets and people start fresh in ObjectFlow.
- **Where I looked**: write-a-flow.md §5 last bullet, publish-and-import.md §7, `scripts/mirror-version.py`.
- **What happened**: `mirror-version.py procurement.yaml PurchaseOrder` printed 14 lines, `Invoice is a mirror too, joined to PurchaseOrder`, `Requisition is a mirror too`, then `Employee`, `Budget`, `Department`, `CostCentre`, `Approval`, `Quote`, ... Because a PO references its (optional) requisition, and a mirror may not reference an owned type, every type but `Agent` becomes a mirror. During the go-live window nobody can raise a requisition, onboard an employee or set up a budget; all of it has to be imported or wait for the cutover publish.
- **What I did instead**: accepted it (a weekend cutover), imported only suppliers and orders, and left employees, budgets and requisitions to be created after the cutover.
- **What would have helped**: the guide saying up front that the reference graph decides the stage, and that a reference from the legacy type to anything new (here `PurchaseOrder.requisition`) drags the new world into the mirror version; or allowing an optional reference from a mirror to an owned type that the import leaves empty.
- **Severity**: minor for this brief (one weekend), major for a staged migration of a larger system.

##### E9. The batch's object format is not in the wire reference
- **What I was trying to do**: write the mapping from the spreadsheet to `import_batch`, including each PO's creation date and when it entered its state.
- **Where I looked**: wire.md (lists `import_batch` keys `actor, batch, disposition, dry_run` only and points to publish-and-import.md), publish-and-import.md §4 (describes `created`, `entered`, legacy entries, silent spans in prose, no field names), test_golive.py (shows `type`, `legacy_key`, `state`, `attributes`, `sequences`).
- **What happened**: no page gives the JSON keys for a creation time or a state's entry time. Found them by reading `core/crates/objectflow-service/src/main.rs` `imported_object()`: `created_at`, `created_by_kind`, `entered: {<dimension>: {at, by, by_kind}}`, `occurred_at`, `legacy_entries`, `legacy_intervals`, `silent_spans`. Also not stated anywhere I found: whether `legacy_key` must be repeated inside `attributes` (test_golive.py does; mirror-version.py adds a `legacy_key` external attribute to every mirror).
- **What I did instead**: copied test_golive.py's shape and the Rust field names.
- **What would have helped**: a "Batch" section in wire.md with one complete object and every key.
- **Severity**: minor (cost ~10 minutes and a source read).

##### E10. A reference to a legacy key nobody has refuses the whole dry run, one row at a time, and cannot be decided
- **What I was trying to do**: dry-run 300 POs, one of which names a supplier code (S-099) that the ERP extract lacks.
- **Where I looked**: the dry run's answer; publish-and-import.md §4 (Assign ids) and §6.
- **What happened**: `"refusal": {"type": "PurchaseOrder", "legacy_key": "PO-03150", "reason": "supplier names Supplier:S-099, which the batch and the store lack"}`, `findings: []`. The dry run stops at the first such row and reports none of the other classes; a second bad supplier code would need a second run. publish-and-import.md §4 says this is by design ("a key naming nothing in the batch or the store refuses the batch"), and no disposition can exclude the row.
- **What I did instead**: the mapping holds back POs whose supplier is not in the ERP extract and writes them to `held_back.json` (the mapping deciding, outside the disposition file the design wants every decision in).
- **What would have helped**: report unresolved references as a class (`REFERENCE: names nothing`) with all samples, decidable by `EXCLUDE`, so one dry run lists everything and the decision is in the disposition file.
- **Severity**: minor.

##### E11. A rule written as a guard is not applied to imported data
- **What I was trying to do**: see whether the legacy over-receipt (an office line received 5 more than ordered) is caught at import.
- **Where I looked**: the dry run's findings; publish-and-import.md §4 (Import: the built-in assertion "evaluates no generated guard"; only a mirror's invariants are checked).
- **What happened**: the dry run raised no class for it. The rule (receive no more than ordered, 5% for raw materials) is a guard on `PurchaseOrder.receive`, and the import writes through the built-in assertion, which evaluates no guard; only invariants are checked at import. So the over-received line was imported as is (outstanding is now negative; `fully_received` reads true).
- **What I did instead**: noted it. To have the import check it, the rule would need to be stated again as an invariant on OrderLine (`received <= ...`), i.e. written twice, which is what the README says ObjectFlow exists to stop.
- **What would have helped**: the guide's go-live bullet saying "only invariants are checked on import; a rule you want enforced on legacy data must be an invariant", or a dry-run option that evaluates each imported object against the guards of the transitions that would have produced it.
- **Severity**: minor (for this brief the dry run is still useful; a team must know the gap).


- 19:36 Import of 1,472 objects (299 POs, 759 lines, 414 receipts) on the deployment store: `written: 1472`, 75.3 s on the debug build (the dry runs that stopped early took under 0.1 s; the clean dry run plus the import took about 4.5 min together). Spot check `lookup legacy PurchaseOrder:PO-03102`: 4 lines, 2 receipts, received value and totals derived correctly.
- 19:38 Cutover: published procurement.yaml as version 2 in 11 s. The dry-run report is useful: `gained:PurchaseOrder.complete` 16 orders (fully received already), `newly_enforced:PurchaseOrder.cancel.nothing_received` 230, `newly_enforced:...cancel_remainder.something_received` 69, each with samples.
- 19:38 After the cutover (golive/after_cutover.py): people, department, cost centre and budget created by requests; on imported PO-03102 an over-receipt was refused (`receipt_within_order`), the outstanding 1,000 received, a whole cancel refused (`nothing_received`), the remainder cancelled (SHORT_CLOSED, committed value down to the received value); an untouched imported PO cancelled; a new requisition drafted by the agent, its submission by the agent refused (`invariant_violated submitted_by_a_person delegable`), submitted by Alice, her own approval refused (both `not_own_requisition` and `manager_level_by_the_manager` listed), approved by Mark, committed, quoted; then an ERP sync batch re-imported Bolt as BLOCKED (`written: 1`) and raising the order with Bolt's quote was refused (`chosen_supplier_active dependent`); Acme chosen with a justification and the order raised as **PO-03401**, the number after the legacy system's last. A legacy batch naming PurchaseOrder after the cutover: `refused_types: ["PurchaseOrder"]`. Metrics read on imported + new data: spend by supplier and category 60 rows, by category 5 rows. `tools` for an agent: 49 tools, `requisition_submit` not among them.
- 19:39 Restarted the service on the same store with `--evaluator xero=<stub url>` (a 20-line Python stub): the store survived the restart (version 2, PO-03401 ISSUED); `record_payment` applied for a bill the stub said was paid and was refused `bill_paid_in_xero delegable` for one it said was unpaid; the stub logged `{"evaluator": "xero", "function": "bill_paid", "arguments": ["BILL-2"]}`. Stopped the service and stub by pid.

##### W3 (worked well). The deployment store, the import and the cutover behave as documented
- Publish by DeclarationChange, dry-run import with a disposition file, import, cutover publish with an impact report, a sequence raised past the legacy numbers, mirrors refreshed by a later batch, a late legacy batch refused, the store surviving a restart, and an evaluator asked over HTTP: every step did what publish-and-import.md §7 and core/README.md say, first time, with nothing but the module, a mapping script and ~150 lines of client code.

##### E12. The agent is offered, and allowed, every approval and finance step
- **What I was trying to do**: check what the mail agent may do, from `tools` for actor kind `agent`.
- **Where I looked**: `POST /v1/tools {"actor_kind": "agent"}` on the core; integrate.md §6; renderers.md was not needed.
- **What happened**: 49 tools; `requisition_submit` is correctly absent (the "a person submits" invariant), but `requisition_approve_as_cfo`, `requisition_override_budget`, `requisition_commit_budget`, `requisition_raise_order`, `purchase_order_reapprove` are offered. Nothing in my flow stopped an agent approving at the CFO level (the manager and head levels are bound to named people; the CFO level is not), or overriding the budget. My mistake, but the shape of the mistake is general: "agents draft, people approve" must be restated, as an `actor.kind` record plus an invariant, on every transition a person must take; there is no per-transition or per-type "human only" marking, and nothing warns.
- **What I did instead**: added `decided_by_kind` + invariant `decided_by_a_person` on Approval and `budget_overridden_by_kind` + invariant on the override (and the same on PO re-approval), with examples.
- **What would have helped**: a declared marking on a transition, "taken by a person", that generates the record and the invariant (and removes the tool), or a checker notice listing transitions an agent can take that write money or approvals.
- **Severity**: minor (once known; the language can say it), but easy to miss.

##### E13. A metric that names a person through a recorded identity is not marked as naming one
- **What I was trying to do**: per-approver turnaround (brief point 9), which names people and should be behind the application's door.
- **Where I looked**: `/v1/declaration` for Approval, the cutover publish report, declaration-syntax.md §6.9 (Dimensions: what is marked).
- **What happened**: `/v1/declaration {"type": "Approval"}` → `person_dimensions: []`, and the publish report's `person_dimensions: []`, although `approver_turnaround`'s dimension `a.approver` holds `actor.id` values (`mark@corp`). The marking only follows references to actor types and `.actor_id` (declaration-syntax.md §6.9). An application relying on `person_dimensions` to decide who may read per-person figures would publish this one to everyone.
- **What I did instead**: said so in the metric's description; an application would have to keep its own list.
- **What would have helped**: a way to mark an `identity` attribute as naming an actor (it is written from `actor.id`, which the checker can see), so the dimension is marked.
- **Severity**: minor (privacy-relevant).

##### E14. My first three-way match let the same goods be invoiced twice (my bug; the language handled the fix)
- Two invoices of 6 units each against a line with 6 received both matched on the core. Fixed by giving OrderLine the set of its invoice lines and comparing the quantity invoiced on all invoices not rejected with the quantity received. Noted because a newcomer's flow will have holes like this that pass every example they thought to write; the examples only prove what the author imagined.


- 19:41 Applied E12/E14 fixes; wrote procurement-golive.examples.yaml (2 setups: legacy order imported as mirrors with a sequence raised, then a `publish` step of procurement.yaml as the cutover; 6 examples on the imported order after the cutover). Runner 6/6, core 6/6.
- 19:42 Added coverage and boundary examples (exactly SGD 10,000.00 one quote; SGD 10,000.01 three; exactly SGD 50,000.00 head completes). 89/89 on both.
- 19:43 Release build (`cargo build --release -p objectflow-service`, 30 s) and the go-live timed on it, fresh store: publish go-live 0.19 s; dry run of the 1,472-object batch 7.7 s; import 10.4 s; cutover publish 1.8 s; spend metric over 759 lines 0.02 s; a request on an imported PO 0.01 s. (Debug build: import 75 s.) Fine for 300 orders.
- 19:44 Bound the CFO level to the company's CFO the same way (a `Company` with `cfo`, snapshotted on the Approval through `budget.cost_centre.department.company.cfo.user_id`), with an example. 90/90 on both; go-live regenerated, checked against, 6/6 on both.
- `scripts/flow-diagram.py procurement.yaml Requisition` renders a correct Mermaid diagram (diagram-*.mmd); useful for review with the business.

##### E15. The coverage notice asks for examples that cannot be written
- **What I was trying to do**: use the `coverage` notice to find untested transitions.
- **Where I looked**: the checker's two `coverage` notices; flow-format.md §11.5.
- **What happened**: the second `coverage` notice lists 21 transitions "no example requests as the request under test", 13 of them `only_via` transitions (`Approval.record`, `OrderLine.add`, `Receipt.record`, `RequisitionLine.close`, ...) that no caller may request. The first notice also listed `Quote.receive`, `OrderLine.amend` and `InvoiceLine.add` as untaken although examples took them, until I wrote `cascaded:` expectations naming them.
- **What I did instead**: added examples whose `cascaded` names the three creations; ignored the rest of the list.
- **What would have helped**: leave `only_via` transitions out of the "as the request under test" list, and say in the first notice that an untaken creation counts once an example's `cascaded` names it.
- **Severity**: minor (noise that trains you to ignore the notice).

##### E16. Approval by amount is one transition per (level, outcome) pair, and the caller must pick the right one
- **What I was trying to do**: "up to 5,000 the manager; up to 50,000 the head as well; above that the CFO as well".
- **Where I looked**: write-a-flow.md §5 ("guards over the derived total on transitions to each level").
- **What happened**: a transition has one target state and no conditional target, so the manager's approval is two transitions (`approve_as_manager` to APPROVED guarded `total <= 5000`, `approve_as_manager_and_escalate` to AWAITING_HEAD guarded `total > 5000`), and the head's two more, plus the CFO's: five approve transitions, four complementary conditions. It works and the diagram reads clearly; but the approver's screen must ask `availability` which of the two is open, and a wrong pick is refused `unreachable_from_here`. The same shape repeats for invoice match/exception, amendment reissue/re-approval, commit/override.
- **What I did instead**: as above.
- **What would have helped**: a transition whose `to` is chosen by guards (a choice pseudostate, UML's own construct), or the guide stating the complementary-pair pattern and that `availability` is how the UI picks.
- **Severity**: minor.

##### W4 (worked well). The record pattern is expressive enough to bind each approval level to the right person
- Manager, department head and CFO levels are each bound to a named person (the requester's manager, their department's head, the company's CFO), snapshotted from five-hop reference paths inside a `create`, with "nobody approves their own" and "a person, not an agent, decides" on the same record. Every refusal comes back `invariant_violated` with the invariant named and `remedy: delegable`, listing every invariant that failed (Alice approving her own: both `not_own_requisition` and `manager_level_by_the_manager`).


- 19:46 Final runs (golive/final-runs.txt): runner 90/90 and core 90/90 on procurement; runner 6/6 and core 6/6 on the go-live version; module against the go-live version `0 fatal · 0 notices`.
- 19:48 Reran the whole go-live end to end on the final module with the release build (golive/run_golive.sh, output golive/run-output.txt): go-live publish 0.21 s, suppliers written 12, dry run 8.5 s with no findings left, import written 1,472 in 11.5 s, cutover publish 2.0 s, then after_cutover.py as at 19:38 (PO-03401 minted after the legacy PO-03400; agent `tools` now 50, still no submit). Service stopped by pid; `ps` shows no objectflow-service or stub left; `git status` of the repository unchanged from the start.
- Did not use the Flow Studio's web front end (it needs pnpm/Node set up); its server-side pieces (examples-on-core, flow-diagram.py) were enough.

#### What I built

All in this directory:

| File | What |
|---|---|
| `procurement.yaml` | the module: 16 types (Employee, Agent, Company, Department, CostCentre, Budget, Supplier [ERP mirror], Requisition, RequisitionLine, Approval, Quote, PurchaseOrder, OrderLine, Receipt, Invoice, InvoiceLine), 65 transitions, 31 conditions, 10 invariants, 24 derived attributes, 7 metrics, 1 evaluator (Xero); 1,201 lines incl. a header mapping each brief point |
| `procurement.examples.yaml` | 12 setups, 90 examples, grouped by brief point 1-9 plus edge cases |
| `procurement-golive.yaml` | go-live version, written by `scripts/mirror-version.py procurement.yaml PurchaseOrder` (15 of 16 types mirrored, 2 transitions left) |
| `procurement-golive.examples.yaml` | 2 setups (legacy order imported, then the cutover `publish`), 6 examples on the imported order |
| `golive/` | make_legacy_export.py (stand-in spreadsheet: 300 POs, 760 lines, 419 receipts, 12 suppliers, 4 dirty rows), to_batch.py (the mapping), disposition.json (the decided classes), held_back.json, of.py (wire client), run_golive.sh + after_cutover.py (whole go-live on a deployment store), xero_stub.py, final-runs.txt, run-output.txt |
| `procurement.v0-invariant-through-references.yaml` | the first version, kept to show E6 |
| `diagram-*.mmd` | Mermaid state diagrams from scripts/flow-diagram.py |

Pass results: reference runner 90/90 + 6/6; Rust core 90/90 + 6/6. Legacy import on the core: 299 of 300 POs (1 held back for an unknown supplier, 1 line and its receipt excluded by decision), 1,472 objects, then cut over and operated.

Where the language could not express a point, or I left it to the application: who is in finance or accounts payable (the flow binds the manager, department head and CFO levels to named people through the record; finance/AP membership is the application's permission layer, by design, ADR-0114); sending the PO to the supplier, writing bills and payments in Xero, the ERP supplier sync job, and reminders (the application, via subscriptions, import_batch and the evaluator endpoint); the budget year chosen for a requisition (the application). Everything else in the ten points is in the module and proven by examples.

#### Verdict

**Yes for a pilot, not yet for production.** A team could build this procurement system on ObjectFlow today and run it on one machine: every rule in the brief was expressible, the checker is fast and precise, the same 96 examples pass on the reference runner and the Rust core, and the go-live (mirror version, dry run with a disposition file, import, cutover, then normal operation with an HTTP evaluator) worked first time on the deployment store. By the timestamps above, this session (an AI agent, so not a measure of a person's time) went from first reading at 19:18 to 66 examples passing on both implementations at 19:25 and to the end at 19:50; what that says is that the samples, the guide's pattern list and the checker's messages carry a newcomer a long way without the specification.

What stands in the way, most serious first:

1. **Engine maturity for production (from the docs, not hit in this session).** core/README.md and integrate.md §1: a deployment's store "refuses a store another engine release created, since an upgrade between releases is not yet built"; it "serves one request at a time"; it "authenticates nobody"; queries and metrics return everything on one page. The README calls the project "in design". For a system of record holding a company's purchasing, the missing store upgrade path is the blocker: the first engine upgrade would strand the data.
2. **E6 (major): invariants that read through references fire on other objects' changes.** The documented "record who acted, constrain it with an invariant" pattern, applied naturally to "approved by the requester's manager", jammed the HR sync as soon as a reporting line changed. Fixable by snapshotting, but nothing warns, and the refusal's remedy (`dependent`) misleads.
3. **E8 (minor here, major for a larger migration): one reference from a legacy type to a new one makes the whole module a mirror.** The go-live is necessarily a single big-bang stage for this module.
4. **The application layer is all yours (by design).** Authentication, roles (finance, AP, buyers), the ERP sync, the Xero bridge, PO dispatch, reminders, screens. The engine gives good building blocks (availability with `actor_dependent`, refusals with remedies, generated agent tools, subscriptions), but none of that work is in the box.
5. **Smaller things a team will trip over** (all minor): E12 human-only steps must be restated per transition and nothing warns (an agent was offered CFO approval); E13 a per-approver metric is not marked as naming a person; E11 guards are not applied to imported data, only invariants; E10 an unknown legacy reference aborts the dry run one row at a time; E9 the import batch's object format is only in Rust source and one test; E16 approval by amount is pairs of complementary transitions; E7 examples cannot refresh a mirror; E4 alias/identity ambiguity in metric expectations; E2/E3 `actor.*` and guards do not count for `required_attributes`; E5 a missing required input has two verdict shapes; E15 coverage notice lists transitions that cannot be requested; E1 the normative spec is hard to read.

#### Findings by severity

- **Blocker**: none for building and piloting. (For production: the store cannot be carried across engine releases; documented, not exercised.)
- **Major**: E6 invariant through references vetoes other objects' later changes (HR sync jammed).
- **Minor**: E8 go-live is one stage for the whole module (major for staged migrations); E12 no human-only marking, agent offered approvals; E13 person-naming metric unmarked; E11 imported data skips guards; E10 unresolved legacy reference aborts the dry run, undecidable; E9 batch object format undocumented; E16 complementary transition pairs for amount routing; E7 examples cannot refresh a mirror; E4 alias vs identity in metric expectations; E2 `actor.*` not provably present; E3 a presence guard does not satisfy `required_attributes`; E5 missing input refused in two shapes; E15 coverage notice noise; E1 spec readability.
- **Worked well**: W1 checker findings precise with fixes; W2 examples caught a real budget bug; W3 deployment store, import, cutover, restart and HTTP evaluator as documented; W4 record pattern binds each approval level to the right person. Also: wire answers are self-describing (`invalid_input` with per-field reasons such as `wrong_currency`; `availability` marks `actor_dependent`; `check` previews an invariant refusal); `mirror-version.py` and `flow-diagram.py` save real work; release-build import of 1,472 objects in about 11 s.

## 7. The integrating agent's log, as it wrote it

As §6; its claim that the record omits an evaluator's verdict is wrong as stated (§3, D562).

### Integration log: a procurement application on ObjectFlow (commit c494d5c)

Started 2026-10-10 19:18 (+08). Written as I work. Work directory: this folder; transcripts in `transcripts/`.

#### Reading in (19:18 to 19:20)

Read `README.md`, `docs/guides/integrate.md`, `docs/guides/wire.md`, `docs/guides/write-a-flow.md`, `core/README.md`, the `reimbursement.yaml` and `people.yaml` samples, `publish-and-import.md` §1, §4 to §7, `flow-format.md` §4.3 to §4.10 and §4.15, `scripts/mirror-version.py`, `apps/flow-studio/server/tests/test_golive.py`.

##### Worked well: the three guides are short and point at each other
- **What I was trying to do**: find out how to talk to the engine.
- **Where I looked**: README "Getting started" -> `integrate.md` -> `wire.md`.
- **What happened**: `integrate.md` (114 lines) is a real walkthrough; `wire.md` lists every operation with its keys and a captured exchange; the "what the engine does / what your application does" table (integrate.md §8) told me up front what I would have to build (auth, permissions, notifications delivery, search, schedules). `write-a-flow.md` §5 has a "patterns for common needs" list that names requisitions, approval by amount, "nobody approves their own" and a purchasing module in SGD. That is unusually on-target.
- **Severity**: (worked well)

##### README contradicts the core's README about the deployment store
- **What I was trying to do**: decide whether I can deploy as a team would, or only use the sandbox.
- **Where I looked**: `README.md` status box; `core/README.md` "What runs today"; `integrate.md` §1; `objectflow-service --help`.
- **What happened**: README.md: "a deployment's service, whose store survives a restart, is not yet built". `core/README.md`, `integrate.md` §1 and `--help` all describe `--store <file>` (SQLite, ADR-0174) and `--evaluator name=url` (ADR-0175) as running today.
- **What I did instead**: trusted `--help` and the binary; will verify by running it.
- **What would have helped**: the top-level README status line kept in step with the core.
- **Severity**: minor

##### The import batch's shape is not in the wire reference
- **What I was trying to do**: learn what `import_batch`'s `batch` and `disposition` look like, for step 10.
- **Where I looked**: `wire.md` §4 "Changing the declaration, and importing" (gives only the keys `actor, batch, disposition, dry_run` and the answer keys); `integrate.md` §7 (points to `publish-and-import.md`); `publish-and-import.md` §4/§6 (a design doc in Python dataclasses, ADR-laden prose, no JSON).
- **What happened**: the only place the JSON shape of a batch appears (`{"objects": [{"type", "legacy_key", "state", "attributes": {..., "legacy_key"}}], "sequences": [{"sequence", "last"}]}`, and the disposition as the dry run's `findings` with `disposition` set) is a test, `apps/flow-studio/server/tests/test_golive.py`.
- **What I did instead**: copied the shape from the test.
- **What would have helped**: a `wire.md` subsection with the batch, a finding and a disposition entry in JSON, like the other operations have.
- **Severity**: major (an integrator has to read the engine's test suite to do a go-live)

##### Design docs are dense; the guides are the only readable layer
- **What I was trying to do**: understand publish and import well enough to plan the go-live.
- **Where I looked**: `publish-and-import.md` §1 and §4.
- **What happened**: single paragraphs of 400+ words, each clause citing ADRs (`ADR-0146, ADR-0147, D531`). Precise, but not something a team new to the engine reads to learn a procedure. The guides stop where the design docs start (integrate.md §7 is one paragraph and hands off to the design doc).
- **What would have helped**: a guide-level "go-live" page: the five steps of publish-and-import §7, with the JSON at each step.
- **Severity**: minor

#### Writing the procurement module (19:20 to 19:24 wall clock)

No sample stands in well for a requisition with approval levels, a budget and orders, so I wrote `flows/procurement.yaml` (Employee, ProcurementAgent, CostCentre with a derived `remaining`, Supplier as an ERP-owned mirror, Requisition with lines, quotes and three approval levels, PurchaseOrder with partial receipts and cancellation, Receipt). `derive.py` derives the three versions I publish: v1 (go-live: PurchaseOrder/Receipt/Supplier mirrors, no budget rule), v2 (adds the budget hold and finance's override: the step 9 change), v3 (the full module: the cutover, ordering with quotes).

##### Worked well: the checker's messages say exactly what to do
- **What happened**: `scripts/check-flows.py` runs in 0.3 s and every fatal named the line and the fix: `'to revise and resubmit.' is not a key but the rest of a value cut at a comma: inside { … }, a value containing a comma is quoted`; `Requisition.resubmit enters SUBMITTED without setting 'first_submitted_at' … require it in state REJECTED`; `Requisition.APPROVED is non-terminal with no outgoing transition`. Six rounds to a clean module, each a minute.
- **Severity**: (worked well)

##### `actor.id` is not "provably present", so an approver cannot be a state's required attribute
- **What I was trying to do**: require `manager_approved_by` (assigned `actor.id` by the approving transition) in WITH_HEAD and APPROVED.
- **Where I looked**: checker output; `flow-format.md` §4.5 (the list of provably present values: `now`, literals, required inputs, … ; `actor.id` is not on it).
- **What happened**: `fatal required Requisition.approve_as_manager enters APPROVED without setting 'manager_approved_by'`, although every request has an actor.
- **What I did instead**: required the `*_approved_at` timestamps instead and left the approver ids optional.
- **What would have helped**: `actor.id` and `actor.kind` counted as provably present.
- **Severity**: minor

##### No partial edit: an optional input on a required attribute is refused
- **What I was trying to do**: one `edit` transition taking `title` and/or `cost_centre`, the usual "edit form".
- **What happened**: `fatal names Requisition.edit lists 'title' as an optional input, but the attribute is neither optional nor defaulted, so the input is required`.
- **What I did instead**: one transition per editable field (`retitle`, `change_cost_centre`). An edit form with N fields becomes N transitions, or one transition that resends every field.
- **What would have helped**: a guide note on how an "edit" screen maps onto transitions.
- **Severity**: minor

##### Routing by amount means two transitions per approval level
- **What I was trying to do**: "manager approves; above 5,000 it goes on to the head".
- **Where I looked**: `write-a-flow.md` §5 "Approval by amount: guards over the derived total on transitions to each level".
- **What happened**: a transition has one target state, so each level is two transitions with opposite guards (`approve_as_manager` to APPROVED if total <= 5,000, `refer_to_head` to WITH_HEAD if above). The application must show the approver one "Approve" button and pick whichever of the two is available. Six transitions and four conditions for three levels.
- **What would have helped**: nothing in the engine is wrong; a worked example in the guide would have saved the design time.
- **Severity**: minor

##### A remedy for "take another transition on this object first" is missing
- **What I was trying to do**: give `enough_quotes` (three quotes before ordering) and `has_lines` a remedy an agent can act on.
- **Where I looked**: `flow-format.md` §4.7; `reimbursement.yaml` (`has_lines` uses `unreachable_from_here`).
- **What happened**: the five remedies are self_serviceable (an input of this request), delegable, temporal, dependent (another object), unreachable_from_here ("stop"). "Add a quote, then retry" is none of them; the sample uses `unreachable_from_here`, which `wire.md` §3 tells an agent means "stop".
- **What I did instead**: `unreachable_from_here`, as the sample does; the application must special-case the clause.
- **Severity**: minor

##### Key order in a type is a fatal check, which bit my own YAML tooling
- **What happened**: round-tripping the module through PyYAML with default `sort_keys=True` gave 40 fatals (`in RequisitionLine, 'derived_attributes' comes after 'conditions'`). My bug, but an application that generates or edits flows must preserve key order.
- **Severity**: minor

##### The go-live version cannot simply be mirror-version's output
- **What I was trying to do**: a v1 whose PurchaseOrder family is a mirror (so the 300 legacy orders can be imported) while requisitions are live.
- **Where I looked**: `scripts/mirror-version.py` docstring; `flow-format.md` §4.15; `publish-and-import.md` §7.
- **What happened**: mirror-version made Supplier and Receipt mirrors too (said on stderr, good) but `Requisition.place_order` creates a PurchaseOrder, which check 53 refuses into a mirror. Removing it leaves `Requisition.APPROVED is non-terminal with no outgoing transition`. PurchaseOrder also cannot reference Requisition at all (a mirror may not reference an owned type), so an order can never point back to its requisition in any version that keeps orders importable.
- **What I did instead**: `derive.py` strips ordering from v1/v2 and adds an interim `record_legacy_order` transition (requisitions are ordered in the old system until cutover).
- **What would have helped**: `publish-and-import.md` §7 saying that the go-live order of operations decides the model: import into mirrors first, then cut over, and that a requisition-to-order link must live on the owned side.
- **Severity**: major (it shapes the data model and the go-live plan; discovered only by trying)

#### Step 1: start the engine as a deployment, publish v1 (19:25)

`services.py start --fresh` starts my ERP stand-in (`erp_bridge.py`) and `objectflow-service --store store/procurement.sqlite --port 0 --evaluator erp=<bridge>/evaluate --evaluator-timeout "2 s"`. `step01_publish.py` drafts a `DeclarationChange`, submits it, reads the dry run, approves it. Transcript: `transcripts/step01_publish.md`.

##### Worked well: a deployment's store, published through the wire like any later version
- **What happened**: `GET /health` → `{"shape":"deployment","version":0}`; `/sandbox/load` → `404 {"fault":"no operation /sandbox/load"}`; draft → submit → `publish` with `expected_version` → health `version: 1`. Exactly as `integrate.md` §2 says. The report has `could_violate` per invariant (which transitions could break "nobody approves their own"), which is useful in review.
- **Severity**: (worked well)

##### The publish report is mostly "not computed"
- **What happened**: the report's `not_computed` lists `invariants, sweepable, unsweepable, queryable_derives, worst_case_fanout, observed_fanout, replaced_creations, creations_past_first_state, observing_clauses, flags`. `person_dimensions: []` although my `manager_turnaround` metric is split by `manager_approved_by`, an identity written from `actor.id`, i.e. a person (see step 11).
- **What would have helped**: the guide saying which report sections a reviewer can rely on today.
- **Severity**: minor

##### The operator is a "human"
- **What happened**: the `DeclarationChange` the built-in `operator` drafted reads `"created_by_kind": "human"`. My identity sync and go-live import run as `operator`, so every Employee and cost centre is recorded as created by a human, and "split by kind of actor" metrics will count the sync as a person. `integrate.md` §3 says to onboard users "as operator" without saying what kind it is, or how to register a service actor of one's own (people.yaml has a `Service` type, my module does not).
- **What I did instead**: kept `operator`; would add a `Service` actor type before production.
- **Severity**: minor

#### Step 2: actors (19:26)

`step02_actors.py`: seven staff onboarded as `operator` with `idempotency_key: idp-onboard-<user_id>`, the agent `agent:mailbot` registered as a `ProcurementAgent`, three cost centres opened.

##### Worked well: an identity sync is safe to re-run, an unknown actor is refused with a reason
- Same key → `"replayed": true` with the first answer. An unknown actor (`mallory@acme.sg`) → `{"kind":"unsatisfied","remedy":"dependent","clause":"actor_known"}`. An actor may be named by identity or by object id.

##### RETRACTED: "Every write takes about 330 ms on a deployment's store"
- I first wrote here that every satisfied write took 318 to 348 ms. **That was wrong**: I read the `ms` column of `step01_publish.jsonl` (the submit, dry run and publish of a `DeclarationChange`, which do take about 330 ms each) and attributed it to step 2. Measured from the transcripts afterwards: satisfied writes have a median of 12 ms (step 2), 13 ms (step 3), 16 ms over the 86 writes of `seed.py`; reads 0.5 to 3 ms (debug build, SQLite file in /tmp). A publish's dry run and approval take about a third of a second each with an empty store.
- **Severity**: none (my measurement error)

##### A duplicate onboarding is refused naming an object that does not exist
- **What happened**: onboarding `alice@acme.sg` again under a new key → `invariant_violated`, `failures: [{object_id: "employee-2", invariant: "user_id_unique"}, {object_id: "employee-9", …}]`. `employee-9` is the would-be new object, which never existed (the next object created took id 9 under another type: `procurementagent-9`). The remedy is `self_serviceable`, which is right for a form but not what a sync wants to know ("already there: it is employee-2").
- **What I did instead**: the sync keys by the IdP id; to find an existing person it would `query` `user_id == "…"`.
- **Severity**: minor

#### Step 3: create and edit a requisition (19:27)

`step03_requisition.py`, `step03b_form_errors.py`.

##### Worked well: idempotency and the version read behave exactly as documented
- Retry under the same key → `"replayed": true`, same object and event. Same key, different request → `409 {"fault":"key_reused: this actor applied the idempotency key ui-draft-7d1e to a different request"}`. Tab A sending version 3 after tab B moved it to 4 → `{"kind":"stale","cause":"expected_version","expected":3,"actual":4}`. Derived `total` is on every `get`. History shows who, what was sent, and the cascade (`cause`).

##### A request's answer does not name the object it created through a cascade
- **What I was trying to do**: after `add_line`, show the new line (to let the user amend it).
- **What happened**: the verdict's `object_id` is the requisition; the line is only found by reading the events at the positions the verdict lists (`/v1/events`, the second event's `object_id: requisitionline-15`), or by diffing `lines` before and after.
- **What I did instead**: read `/v1/events` for the positions.
- **Severity**: minor

##### A part's edit does not move the whole's version
- **What happened**: amending a line (line version 1 → 2) changed the requisition's derived `total` but not the requisition's `version`. A screen that holds "requisition at version 5, total X" can be acted on after the total became Y and pass `expected_version: 5`. My flow only lets lines change in DRAFT/REJECTED, so approvals are safe, but the guarantee "send the version you read" does not cover derived values over parts.
- **What would have helped**: `integrate.md` §4 saying so, and pointing at `changed_since` (which the delivery sample uses for this).
- **Severity**: minor

##### Form errors come back in rounds, and guard refusals do not name the field
- **What happened**: inputs with an unknown key, a malformed quantity, a USD price and a missing category: 1st answer names only `colour: unknown_input`; without it, `quantity: malformed, unit_price: wrong_currency` (category's absence not reported); then `category: missing`; then guards one at a time (`RequisitionLine.quantity_positive`, then `price_positive`). Up to four round trips to clear one form. A guard refusal (`call_refused` → `unsatisfied`, clause `RequisitionLine.quantity_positive`) does not say which input it is about; the application keeps a clause→field map. An enumeration value not declared reads `wrong_type`, not "not a member".
- **What I did instead**: `explain()` in `ofclient.py` maps reasons to text; a real form would validate types client-side from `declaration`.
- **Severity**: minor

#### Step 4: every refusal (19:27 to 19:29)

`step04_refusals.py` (transcript `step04_refusals.md`; the first, broken run kept as `step04_refusals.first-run-bridge-bug.md`). `ofclient.explain()` turns each verdict into a sentence from the condition's or invariant's `description`, read from `/v1/declaration`, plus advice by `remedy`.

##### The evaluator call's `arguments` is a positional array, which only an ADR says
- **What I was trying to do**: answer `erp.cost_centre_open(code)` from my ERP stand-in.
- **Where I looked**: `core/README.md` ("posts each evaluator's call … `{"evaluator", "function", "arguments"}`"), `integrate.md` §8, `wire.md` §4 sandbox section. None gives an example body. Found it in `docs/adr/0175-the-evaluators-http-transport.md` decision 4: `{"evaluator": "xero", "function": "bill_paid", "arguments": ["BILL-0042"]}`.
- **What happened**: I had assumed an object keyed by argument name (the declaration names them: `code`). My bridge crashed on `args.get`, the engine logged `erp.cost_centre_open(CC-FIN) is unknown: … was not reached: io: Peer disconnected` on stderr, and every submit on every cost centre was refused `temporal` for the whole first run. From the application's side nothing distinguishes "ERP down" from "my bridge is broken": both are `unsatisfied`, clause, `remedy: temporal`; the reason is only on the engine's stderr.
- **What I did instead**: read the ADR, took `arguments[0]`, restarted both processes.
- **What would have helped**: the call and answer shown in `wire.md` with an example; names as well as positions in the call (the declaration names each argument, the call drops the names, so the endpoint depends on declaration order); the verdict saying the evaluator was not reached (`not_computed` lists `unknown` and `consulted`, which would have said it).
- **Severity**: major (an integration's failure is invisible to the application, and the contract is undocumented in the guides)

##### Worked well: the refusals are specific and the remedies right
- ERP says closed → `unsatisfied`, clause `cost_centre_open_in_erp`, `remedy: delegable` (the condition's); ERP down → same clause, `remedy: temporal`; ERP back → satisfied (no stale cache: ADR-0175 says every request asks again).
- Agent submits → `invariant_violated`, `submitted_by_a_person`, `remedy: delegable`; Alice approving her own as manager → `invariant_violated`, `manager_is_not_requester`, `delegable`. "Agents draft, people approve" and "nobody approves their own" both work as `integrate.md` §6 says.
- `unavailable` names the state; `not_requestable` names the parent transition; `unknown_transition`, `not_found`, `not_proposable` all as `wire.md` §3 lists them. Faults are `{"fault": "<plain sentence>"}`: `/v1/get takes no key folow`, `no operation /v1/approve`, `an operation is a POST`.
- The store survived a restart of the service (`services.py stop/start`): version 1, every object.

##### `/v1/get {}` answers `200 null`, not the documented 400
- **What happened**: `wire.md` §1 lists "an argument it needs left out" as a 400; `POST /v1/get {}` answers `null` with 200, the same as an object that does not exist.
- **Severity**: minor

##### A reference of the wrong type reads "no_such_object"
- **What happened**: `requester: "costcentre-10"` (an existing object, but not an Employee) → `reasons: {requester: no_such_object}`. A form says "does not exist" about something that does.
- **Severity**: minor

##### Routing twins are refused `unreachable_from_here`
- **What happened**: Bob pressing `approve_as_manager` on a SGD 6,415 requisition → `unsatisfied`, `within_manager_limit`, `unreachable_from_here`. Correct, but the application must know that `refer_to_head` is the button to show instead; the verdict does not point at the transition that is available. The answer is in `availability` (step 6).
- **Severity**: minor

#### Step 5: a worklist (19:30 to 19:33)

`seed.py` (24 requisitions in every state, 88 requests), `step05_worklist.py`, `step05b_order.py`.

##### Worked well: query filters are expressive
- `state in {Requisition.SUBMITTED, Requisition.WITH_HEAD, Requisition.WITH_CFO}`, `state.category == waiting`, `requester.id in {"employee-2", "employee-7"}`, `cost_centre.code == "CC-ENG"`, `total > SGD 5000.00` (a derived attribute), `entered_at(state) + 1 h < now` all work, and `fields` may name `entered_at(state)`. `available` lists the objects a transition is available to (11 SUBMITTED for `approve_as_manager`).

##### No paging, and ordering is narrow and undocumented
- **What I was trying to do**: a worklist sorted by amount, 10 per page.
- **Where I looked**: `wire.md` §4 (keys `order`, `cursor`; no syntax for `order`); `library-api.md` §6 (`order: str | Sequence[str]`).
- **What happened**: `order: "total"` (derived), `"state"`, `"cost_centre.code"` → `"refused": "unknown_order_key"`, with no reason. Descending is `"submitted_at desc"` (found by trying `-submitted_at`, `DESC`, `["submitted_at","desc"]`, all refused). A `cursor` sent to `query` is a 400. Every page holds every row (`wire.md` says so: "Page in the client").
- **What I did instead**: fetched all open requisitions in one query, sorted and paged in the app (`step05_worklist.py`).
- **What would have helped**: the order grammar and which keys are orderable in `wire.md`; real paging before a team has thousands of objects.
- **Severity**: major for 400 staff over years (every worklist read returns every matching object); minor today

##### "Waiting on whom" is entirely the application's
- **What happened**: as designed (ADR-0114), the engine knows the state (SUBMITTED = the manager's) but not who the manager is. The app maps state → role → person through its own org chart and cost-centre heads (`MANAGER_OF`, `HEAD_OF` in `step05_worklist.py`). `fields` cannot follow a reference (`requester.name` → `unknown_field`), so a list with names is N extra `get`s or a client-side cache.
- **Severity**: minor (expected from the docs; the reference-field limit is the cost)

#### Step 6: actions as buttons and forms (19:33 to 19:35)

`step06_actions.py` renders every offer for six (object, actor) pairs to text and to `out/actions.html`; `step06b_actor_dependent.py`.

##### Worked well: availability plus declaration is enough to build forms
- Each offer has `availability` (available / available_with_input / unavailable with the verdict), its inputs' types (`money(SGD)`, `int`, `Category`, `CostCentre`); `/v1/declaration` adds descriptions, `required_inputs`/`optional_inputs`, and enumeration members for selects. `unevaluated` marks that `submit` will ask the ERP when pressed; `unavailable` offers with `kind: unavailable` or `unreachable_from_here` are hidden, the rest shown greyed with the reason. About 60 lines of code.

##### Offers and `available` do not apply rules about who acts, though they are told the actor
- **What I was trying to do**: a "you can approve these" list and an agent's planning view.
- **What happened**: `available` for `approve_as_manager` as Alice lists her own two requisitions; `availability` offers her `approve_as_manager` on her own as `available`, `actor_dependent: true`; `check` of the same answers `invariant_violated`, `manager_is_not_requester`. As the agent, `available submit` lists five drafts the agent can never submit. Both requests name the actor, so the engine knows `actor.id` and `actor.kind`, but by design (`wire.md` §4: "which an offer cannot tell") it does not evaluate the invariants with them.
- **What I did instead**: the app filters by its org chart; for certainty, one `check` per row (N+1 requests).
- **What would have helped**: offers evaluated with the asking actor's id and kind (the engine already records both on every event).
- **Severity**: major for an agent (its own "what can I do" answer is wrong for every rule about who acts); minor for the people's UI

##### The built-in label transition appears on every object
- **What happened**: every offer list ends with `record` (`creates: "label"`, inputs `name`, `corrects: label`, `note`), which neither guide mentions; my renderer showed it as a form until I recognised it from `flow-format.md` ("Labels").
- **Severity**: minor

#### Step 7: an evaluator answered by a process I run (19:29 to 19:32)

`erp_bridge.py` is a small HTTP service holding the ERP's view (`data/erp.json`, re-read per call, every call logged to `transcripts/erp_bridge.jsonl`); the engine runs with `--evaluator erp=http://127.0.0.1:<port>/evaluate --evaluator-timeout "2 s"`. Step 4 exercised closed / down / back; `step07_evaluator.py` adds the rest.

##### Worked well: the transport is simple and failures are logged
- One POST per call, `{"evaluator":"erp","function":"cost_centre_open","arguments":["CC-ENG"]}` → `{"verdict":"satisfied"}`. Unsatisfied carries the condition's own remedy (`delegable`); a 503, a dropped connection or a timeout is `temporal`, and the engine writes why on stderr (`… answered 503`, `… was not reached: timeout: global`). No answer is cached across requests, so a reopened cost centre is seen at once. `check` does not call the ERP and lists the clause under `unevaluated`.

##### A slow external system stalls every user of the service
- **What I was trying to do**: see what a requester and everyone else experience when the ERP is slow.
- **What happened**: ERP answering in 3 s, timeout 2 s: the submit took 2.05 s and was refused `temporal`; a plain `get` sent 0.3 s after it took 1.75 s, i.e. it waited for the submit. The service serves one request at a time (core/README says so), and the evaluator call is made inside that one request's turn. Up to sixteen calls per request (ADR-0175), so one request can hold everyone for up to 16 × the timeout.
- **What would have helped**: concurrency for reads at least, or evaluator calls made without holding the service.
- **Severity**: major (with 400 staff, one ERP hiccup freezes every screen)

##### The event does not record what the evaluator answered
- **What I was trying to do**: show an auditor that the ERP said the cost centre was open when the requisition was submitted.
- **Where I looked**: the submit event's `reads` (`/v1/events`); `library-api.md` §5: "per evaluator or metric guard, the verdict or value it was given and that value's as-of time".
- **What happened**: `reads` holds `objects`, `scans`, `now`; nothing about `erp.cost_centre_open`. The event's `not_computed` (`object_seq, source, taint_version, txn, retries, flags`) does not admit the omission either.
- **What I did instead**: my bridge's own log is the only record of the ERP's answers.
- **Severity**: major (the record is the product's promise; an external check that gated an approval leaves no trace in it)

##### A `check` answers the event positions and version a write would have had
- **What happened**: `check` of submit → `{"verdict": {"kind":"satisfied","object_id":"requisition-74","version":3,"events":[155],"replayed":false}, "unevaluated":["cost_centre_open_in_erp"]}`; the real submit then answered the same `version: 3, events: [155]`. Harmless, but a dry run that reports positions in the log reads as if it wrote.
- **Severity**: minor

#### Step 8: events through a subscription (19:32)

`step08_events.py`: the notifier (as `operator`) subscribes to `Requisition` with `changes_state_only`, pulls, sends "emails" by state (to the manager on SUBMITTED, the head on WITH_HEAD, the requester on REJECTED), acknowledges.

##### Worked well: pull and acknowledge are simple and at-least-once
- A subscription starts at its own position (no backlog); four state changes came back with `to_state`, `actor_id`, positions; a pull without acknowledging gives the same events again (a crashed notifier loses nothing); after `acknowledge` with `settled`, nothing. The Subscription object reports `lag`, `lagging`, `dead`, which a health check can watch. Filters by `transitions` and `target_type` work.

##### Notifications need one `get` per event, and a subscription is anyone's
- **What happened**: an event names the object and states, not its attributes, so routing (who is the requester? which cost centre?) is a `get` per event. Alice could `pull` the notifier's subscription (`refused: null`); she could acknowledge it too and the notifier would miss events. By design the engine authorises nothing, so the service must never be reachable by anything but the application's back end.
- **Severity**: minor

#### Step 9: change the flow: v2 with the budget hold (19:32 to 19:33)

`step09_change.py`: before v2, CC-FIN (budget SGD 8,000) has a SGD 5,000 approved requisition and a SGD 4,000 submitted one. Draft v2 → submit → read the report → (meanwhile Bob approves the SGD 4,000 one under v1) → approve → refused → `refresh` → approve.

##### Worked well: the impact report names the live objects a change affects, and an approval is of the impact shown
- The dry run: `newly_enforced:Requisition.approve_as_manager.within_budget` → `["requisition-85"]` (the SGD 4,000 one), `lost:Requisition.approve_as_cfo` → the two SGD 63,000 requisitions on CC-ENG (budget 60,000), each with a message. When the world moved before approval, the approval was refused `{"kind":"unsatisfied","clause":"impact_unchanged"}`; after `refresh` the report named a different object (`requisition-22`, now over budget because of Bob's approval) and the approval went through (version 2). After it: a new SGD 4,000 requisition on CC-FIN → Bob refused `within_budget`, `delegable`; Erin (finance) is offered `override_budget` with its form; after her override Bob's approval applies. The report is also kept on the `DeclarationChange` with `drafted_by`/`approved_by` and their kinds.

##### `impact_unchanged` is refused `unreachable_from_here`
- **What happened**: the fix is `refresh`, re-read, approve again, but the remedy says "stop". An application must special-case the clause. Same vocabulary gap as step 1's "add a quote first".
- **Severity**: minor

##### A publish takes about 0.4 s and blocks the service meanwhile
- **What happened**: submit, dry run and approval each 0.33 to 0.40 s with about 90 requisitions. Fine now; it reads every object of each affected type, so it will grow with the store, and nothing else is served meanwhile.
- **Severity**: minor (to measure at scale)

#### Step 10: go-live of 300 open orders and their receipts (19:34 to 19:36)

`make_legacy.py` writes the old system's export (`data/legacy_open_pos.csv`, 300 orders; `data/legacy_receipts.csv`, 324 receipts; `data/erp_suppliers.csv`, 8 suppliers) with five planted defects: a category label instead of its code, a price with three decimals, a blank quantity, a supplier the ERP export lacks, an order received twice over. `legacy_map.py` is the hand-written mapping. `step10_golive.py` (owned type refused; first dry run), `step10b_golive.py` (dry runs until the batch places), `step10c_golive.py` (decide, import, re-import, check, cut over to v3), `step10d_after_cutover.py` (order, receive, cancel). The decided classes are kept in `data/disposition.json`.

##### Worked well: the import is one transaction, resumable, and the cutover keeps ids
- Import into an owned type → `refused_types: ["Requisition"]`, reason `Requisition is owned by this store, not a mirror`. After deciding, 623 objects written in 3.3 s in one transaction; sending the same batch again wrote 0. `lookup` by `PurchaseOrder:PO-04334` finds the order with its receipts and derived `received_quantity`. The order sequence was raised: the first order raised after cutover is `PO-04631`, after the legacy `PO-04630`. The cutover report listed what imported orders gain (`cancel` for 112 untouched, `cancel_remainder` for 185 partly received). After cutover: partial receipt, over-receipt refused (`within_receivable`), 5 % over accepted for raw materials (210 of 200, 211 refused), cancel refused after a partial receipt and `cancel_remainder` allowed, the ERP blocking a supplier refuses `place_order_justified` (`call_refused` → `PurchaseOrder.supplier_active_in_erp`, `delegable`), three quotes enforced above SGD 10,000, a dearer quote refused without a justification. A batch naming an order after cutover is refused.

##### A reference to a missing row refuses the whole dry run, one row per round
- **What I was trying to do**: see every problem of the export in one dry run, to send one list back to the old system's owner.
- **What happened**: round 1: `refusal: {"legacy_key":"PO-04481","reason":"supplier names Supplier:S999, which the batch and the store lack"}` and `findings: []`: nothing else was evaluated. Only after removing that row did the dry run report the classes. With 10 such rows that is 10 rounds. (Documented in `publish-and-import.md` §4, but a dry run is exactly where one wants all of them.)
- **What I did instead**: a loop that removes the refused row (and its receipts) and dry-runs again, recording each removal (`run/golive_removed.json`).
- **Severity**: minor

##### One bad value is reported as two classes; a class covers every object in it
- **What happened**: `unit_price: "SGD 12.345"` → `PurchaseOrder.unit_price: is not of its type` and `PurchaseOrder.unit_price_provided: is absent`, both sample `PO-04373`; same for the category label. Excluding an order then made its receipts a new class (`Receipt.order: names an excluded object`), found only by a second dry run with the disposition. Expected from §6, workable.
- **Severity**: minor

##### Rules written as guards are not checked at import
- **What happened**: PO-04531, received 20 of 10 in the old system, imported without a finding, because "receiving more than ordered is refused" is a guard on `receive`, and a mirror keeps only invariants. After cutover it shows `received_quantity: 20` and is offered `complete`.
- **What would have helped**: the go-live guide saying that only invariants are checked on imported data, so a rule that legacy data may already break needs an invariant too.
- **Severity**: minor

##### Imported objects are "created by a human" now, or "unknown" at an unknown time
- **What happened**: an imported order's `created_at` is the import time, `created_dated: false`, `created_by_kind: "unknown"`; its import event's actor is `operator`, kind `human`. The batch can carry the legacy creation time and changer (`publish-and-import.md` §4 "Created", "Intervals") but no example shows the JSON keys, so I could not supply the order date as the creation time.
- **Severity**: minor (covered by the batch-shape gap above)

##### The import blocks the service for its whole duration
- **What happened**: 3.3 s for 623 objects (debug build), during which no other request is served. A full go-live is a maintenance window.
- **Severity**: minor

#### Step 11: a manager's metrics (19:37 to 19:39)

`step11_metrics.py` (v4 published first, adding `submission_to_order`; transcript of that part `step11a_v4_and_activity.md`). Deployment clock is the wall's, so durations are seconds.

##### Worked well: most of the brief's figures exist from the first request, with nothing instrumented
- Time from requisition to order: `Requisition.submission_to_order` (declared in v4, and it read the history recorded under v1 to v3: `7 s` for October). Waiting at each level: the standard `Requisition.time_in_state` keyed by state (SUBMITTED 1 s, WITH_HEAD 0 s, WITH_CFO 457 s) or my declared `wait_with_*`. Approver turnaround: my `manager_turnaround` keyed by `approver` → `bob@acme.sg`. Spend by supplier and category: `PurchaseOrder.spend_by_supplier_and_category` over the 300 imported orders too (40 rows; `keep: ["category"]` aggregates). And for free: `Requisition.refusals` by clause and kind of actor (`cost_centre_open_in_erp` refused the agent once and people 7 times), `transition_counts` split human/agent, `work_in_progress`, `oldest_open`. `export` of `Requisition.intervals` pages with a cursor (the only paged read).

##### A per-person metric is not marked as naming a person
- **What I was trying to do**: put per-approver figures behind the app's own door, as README promises ("Every figure that names a person says so").
- **Where I looked**: `/v1/declaration` `person_dimensions` for Requisition (`[]`); the v1 publish report (`person_dimensions: []`); `flow-format.md` §4.9 (marked when the value is an object of an actor type, a row's `.actor_id`, or "a path ending in an attribute marked `actor`").
- **What happened**: `manager_turnaround` keyed by `approver: r.manager_approved_by` returns `{"approver": "bob@acme.sg"}` and is not marked. The approver was recorded exactly as `write-a-flow.md` §5 recommends (`assign … expr: actor.id` into an `identity` attribute). I found no way in the format to mark such an attribute as naming an actor.
- **What I did instead**: the app hard-codes which metrics are per-person.
- **Severity**: major (the privacy marking fails silently for the recommended modelling)

##### Windows and filters on the standard metrics
- **What happened**: `Requisition.time_in_state` with `over: "1 h"` → `refused: "no_window"` (the standard metric has no time dimension), so "waiting at each level over the last 30 days" needs a declared metric. With `filter: "state == Requisition.WITH_HEAD"` → 0 rows and no refusal (`bind: {"state": "SUBMITTED"}` works). Dimension values are ids (`supplier-90`), so a report maps them to names itself.
- **Severity**: minor

##### The brief's invoice exception share came later
- Invoices were not in my module at this point. I added them in v5 (step 12): `Invoice.exception_share` is a declared metric over a `was_exception` flag, and read `0.667` there.

#### Step 12: an AI agent with tools, a person approving (19:40 to 19:43)

`step12a_publish_v5.py` publishes v5 (`add_quote` made `proposable`; an `Invoice` type with a three-way match within 2 %, an exception path for accounts payable, payment gated on `xero.bill_paid`, and `exception_share`); the engine was restarted with `--evaluator xero=…` (same bridge). `step12_agent.py` is the app's tool loop; `agent/model_turns.json` holds the model's tool calls. **No LLM API is reachable from this sandbox, so I (Claude) played the model: I read the tool definitions the app hands it (`run/agent_tools_handed.json`) and the two emails, and wrote the calls; the loop validates each against its JSON Schema and sends it in the agent's name, as an SDK tool loop would.** `step12b_invoices.py`, `step12c_agent_approves.py`.

##### Worked well: tools from the flow, refusals the agent can act on, proposals a person approves
- `/v1/tools {"actor_kind":"agent"}` gave 29 (v4) / 34 (v5) tools, each a JSON Schema with enums for enumerations, a pattern for money, `object_id`, `expected_version`, `idempotency_key`, and `propose` on the proposable one. `requisition_submit` and `requisition_resubmit` are left out for an agent (the `submitted_by_a_person` invariant), as `integrate.md` §6 says; a person's tool list has them.
- Run: the schema caught `category: "FURNITURE"` client-side; the engine refused `unit_price: "SGD 95.005"` (`invalid_input`, `malformed`) and the model corrected it; a retry under the same key replayed; asking for `requisition_submit` got "not one of your tools", and the model handed the draft to Alice (`ask_person`). Alice saw `created_by_kind: "agent"` and the agent's three events in history, and submitted. A supplier's quote email → `requisition_add_quote` with `propose: true` → `proposal-749` PENDING with `proposer: agent:mailbot`; Erin (buyer) was offered approve/reject; approving executed `add_quote` as Erin, the quote's event naming Erin with `cause` the approval, the Proposal `EXECUTED` and `created_by_kind: agent`. Who drafted and who approved are both on the record.
- Invoices (brief item 7): within 1.5 % → `match`; 5 % over → `price_within_tolerance` (delegable) → `raise_exception` → `resolve` with a reason; more than received → `quantity_received` (dependent); `record_payment` refused `temporal` until Xero (the bridge) shows the bill paid. `Invoice.exception_share` = `0.667`.

##### The engine's tools are write-only; the agent cannot read
- **What happened**: 28 of the 34 tools (every one acting on an existing object) say in their description "Call availability(object_id) first", but no `availability`, `get` or `query` tool is generated (the only read is `read_metric`). An agent cannot find Alice's employee id, the cost centre, a supplier, or check what it may do.
- **What I did instead**: the app adds `find_employee`, `find_cost_centre`, `find_supplier`, `get_object`, `availability` (`step12_agent.py`, `APP_TOOLS`).
- **What would have helped**: read tools generated beside the write tools (the engine already has the operations).
- **Severity**: major

##### Nothing stops an agent approving unless the flow says so, and the tools hand it every approval
- **What happened**: the tools for an agent include `requisition_approve_as_manager/head/cfo`, `override_budget`, `place_order`, `employee_onboard`, `cost_centre_set_budget`, `invoice_resolve`, `invoice_record_payment`. `check` of `approve_as_manager` by `agent:mailbot` on a SGD 2,000 CC-OPS requisition → `satisfied`. My flow stated "a person submits" but not "a person approves", so the agent may approve. `integrate.md` §6 does say this is the flow's to state, but a missed invariant fails open and silently; and `propose` is optional even on a `proposable` transition (the agent's direct `add_quote` would apply; my app forced `propose: true`).
- **What I did instead**: an allowlist of seven engine tools in the app, and the app forces `propose` on quotes.
- **What would have helped**: a declaration-level default ("agents may only propose", or a per-type list of transitions an agent may take), or the publish checker noticing transitions an agent kind can take that write money or approvals.
- **Severity**: major

##### Small tool and proposal rough edges
- The money pattern `^SGD -?[0-9]+(\.[0-9]+)?$` accepts three decimals and negatives, so a schema-valid call is refused `malformed` by the engine.
- The same quote proposed twice gives two PENDING proposals (no dedupe; the app must).
- A Proposal's `inputs` is a JSON string inside a string attribute (`"{\"amount\":\"SGD 2350.00\",…}"`), so the buyer's screen parses JSON out of a field.
- Approving a proposal answers with the target requisition's id and version, not the proposal's.
- **Severity**: minor

#### Wrap-up (19:43)

All processes I started (engine, ERP/Xero bridge, stderr forwarder) were stopped by pid; nothing of mine is running. (A `cargo build --release -p objectflow-service` seen in `ps` at the end is not mine; I left it alone.) The repository is unchanged (`git status` shows only the `docs/design/jira-flows.html` that was untracked before I started). Wall-clock time: 19:18 to 19:43.

##### What ran, against which engine shape

Every step ran against **the deployment shape**: `objectflow-service --store store/procurement.sqlite` (debug build, already up to date for c494d5c), with `--evaluator erp=…` and later `xero=…` pointing at my own bridge. The sandbox was not used; `/sandbox/load` was shown to be refused (404) on the deployment. The store went through five published versions (v1 go-live with orders as mirrors, v2 budget hold, v3 cutover with ordering and quotes, v4 a new metric, v5 proposable quotes and invoices) and three restarts of the service.

| Step | Script(s) | Result |
|---|---|---|
| 1 start, publish | `services.py`, `step01_publish.py` | worked |
| 2 actors | `step02_actors.py` | worked |
| 3 create/edit, retries, versions | `step03_requisition.py`, `step03b_form_errors.py` | worked |
| 4 refusals | `step04_refusals.py` | worked (after fixing my bridge for the undocumented argument shape) |
| 5 worklist | `seed.py`, `step05_worklist.py`, `step05b_order.py` | worked, paging and most sorting done in the app |
| 6 buttons and forms | `step06_actions.py` (→ `out/actions.html`), `step06b_actor_dependent.py` | worked; offers ignore rules about who acts |
| 7 evaluator | `erp_bridge.py`, `step07_evaluator.py` | worked; a slow ERP stalls everyone; answers not in the record |
| 8 events | `step08_events.py` | worked |
| 9 flow change | `step09_change.py` | worked, including the impact report and `impact_unchanged` |
| 10 go-live 300 orders | `make_legacy.py`, `legacy_map.py`, `step10_golive.py`, `step10b_golive.py`, `step10c_golive.py`, `step10d_after_cutover.py` | worked: 297 orders and 318 receipts imported, 3 orders excluded with recorded reasons, cutover kept ids and numbering |
| 11 metrics | `step11_metrics.py` | worked, but the per-approver metric is not marked as naming a person |
| 12 agent | `step12a_publish_v5.py`, `step12_agent.py` (+ `agent/model_turns.json`), `step12b_invoices.py`, `step12c_agent_approves.py` | worked with app-supplied read tools and an allowlist; I played the model (no LLM API in the sandbox) |

Brief coverage in the flow: items 1, 2, 3, 4, 6, 7, 8, 9, 10 modelled and exercised; item 5 partly (supplier checked in the ERP at ordering; the "amendment over 10 % is re-approved" rule was not modelled, for lack of time, not because of a known obstacle); item 9's invoice exception share exists from v5. Simplifications: one item per purchase order; one currency (SGD).

##### Verdict

**A pilot: yes, today.** A small team could build this procurement system on ObjectFlow and run it on one machine: every one of the twelve integration steps worked against the deployment store, the wire is small and consistent, refusals are specific and actionable, idempotency and the version read work as documented, the go-live import and cutover work, and most of the manager's figures exist without instrumentation. The flow checker is the best part of the experience.

**Production for 400 staff: not yet.** What stands in the way, most serious first:

1. **No upgrade path for a deployment's store** (stated in `integrate.md` §1 and `core/README.md`; not tested by me): a new engine release refuses a store an older one created, so any engine fix strands the data. Blocker for production, not for a pilot.
2. **One request at a time, with evaluator calls inside the request**: an ERP or Xero answering slowly freezes every user (measured: a plain `get` waited 1.75 s behind a submit whose ERP call timed out). Major.
3. **The record does not keep what an external system answered**: a submit gated on the ERP leaves no trace of the ERP's verdict in the event, though the design says it should. Major for an audited approval process.
4. **Agent safety fails open**: the generated tools give an agent every transition the flow does not explicitly forbid (approvals, budget overrides, payments), `propose` is optional even on proposable transitions, and offers/`available` ignore rules about who acts, so the agent's own "what can I do" is wrong. The app must keep an allowlist and force proposals. Major.
5. **The agent's tools are write-only**: no generated `get`/`query`/`availability` tools, though 28 tool descriptions tell the agent to call `availability` first. Major.
6. **Undocumented integration contracts**: the import batch's JSON (only in a test) and the evaluator call's positional `arguments` (only in an ADR); an evaluator integration bug shows to the application only as `temporal`. Major.
7. **Per-person metrics are not marked** when the person is recorded as `actor.id` (the guide's own pattern), so the README's privacy promise fails silently. Major.
8. **Go-live staging shapes the data model** (a mirror may not reference an owned type; an owned type may not create into a mirror), discovered only by trying. Major.
9. **No server-side paging, narrow and undocumented ordering** (`total`, `state`, paths refused; descending is `"<key> desc"`). Minor today, major at scale.

##### What the application must build that the engine does not provide

Authentication and the org chart (who is whose manager, which head and CFO), so "waiting on whom" and every permission; paging, sorting by amount, and id→name resolution for lists and metric dimensions; a clause→field map and refusal texts for forms (refusals come in rounds); notification delivery (the engine gives events, the app sends mail); the ERP and Xero bridges (HTTP endpoints answering verdicts); the agent's read tools, its tool allowlist, forced proposals and proposal dedupe; the import mapping, the dry-run loop that removes unresolvable rows, and the disposition file; gating of per-person metrics.

##### Findings, by severity

**Blocker (production only)**: no engine upgrade for an existing deployment store (documented, not tested).

**Major**
- A slow external system stalls every user (one request at a time, evaluator calls inside it). Step 7.
- Events do not record evaluator answers. Step 7.
- Agent safety fails open: tools include every unforbidden transition; `check` let the agent approve; `propose` optional. Step 12.
- Generated agent tools have no reads, though their descriptions require `availability`. Step 12.
- Offers and `available` ignore rules about who acts (own requisitions listed as approvable; agent's `available submit` lists drafts it can never submit). Step 6.
- Per-person metric not marked in `person_dimensions`. Step 11.
- Import batch shape only in a test. Reading-in.
- Evaluator call shape only in an ADR; integration failure invisible to the app. Step 4.
- Go-live mirror rules shape the model and the go-live order. Flow writing.
- No paging (major at scale; minor today). Step 5.

**Minor**
- README says the deployment store is not built; it is. Reading-in.
- Design docs dense; no guide-level go-live page. Reading-in.
- `actor.id` not "provably present"; optional inputs on required attributes refused (no partial edit); routing by amount needs twin transitions; no remedy for "take another transition on this object first" (`enough_quotes`, `has_lines`, `impact_unchanged` all say "stop"); key order fatal. Flow writing, steps 4, 9.
- Publish report mostly `not_computed`; the operator is recorded as a human (sync, import, publish). Step 1.
- Duplicate onboarding names a nonexistent object. Step 2.
- A cascaded creation's id only via `/v1/events`; a part's edit does not move the whole's version; form errors come in rounds and guard refusals do not name the field; undeclared enum member reads `wrong_type`. Step 3.
- `/v1/get {}` is 200 `null` not 400; a reference of the wrong type reads `no_such_object`; routing twins refused `unreachable_from_here`. Step 4.
- Query fields cannot follow references. Step 5.
- The built-in label `record` transition on every object, undocumented in the guides. Step 6.
- `check` reports event positions and a version as if it wrote. Step 7.
- Subscriptions are pullable and acknowledgeable by any actor; notifications need a `get` per event. Step 8.
- Publish and import block the service (0.4 s per publish step; 3.3 s for 623 objects). Steps 9, 10.
- Import dry run stops at the first unresolvable reference; one bad value is two classes; guards are not checked on imported data; imported objects' creation time/kind not suppliable without the undocumented keys. Step 10.
- Standard metrics cannot be windowed; `filter` silently returns nothing; dimension values are ids. Step 11.
- Tool money pattern accepts three decimals; duplicate proposals; Proposal `inputs` is JSON in a string; approval answers with the target's id. Step 12.

**Retracted**: my early "330 ms per write" was a misreading of the publish transcript; writes are 12 to 16 ms (see the retraction under step 2).

**Worked well**: the three guides and the "engine vs application" table; the flow checker's messages; publishing through the wire; idempotency, versions and stale refusals; specific refusals with sensible remedies; evaluator transport and its stderr diagnostics; subscriptions; the change impact report and `impact_unchanged`; the import's one-transaction, resumable batches and an id-preserving cutover with sequences raised; standard metrics (refusals by clause and actor kind, time in state, work in progress) with nothing to instrument; tool schemas with enums; proposals recording who drafted and who approved.
