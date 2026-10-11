# Hiring on ObjectFlow: the third cold review

Status: **repaired**, 2026-10-11: fifteen defects (D578 to D592), each resolved the same day, five findings decided by ADR-0181, and six questions for the author, added to `before-1.0.md` §4. The first two reviews built procurement (`procurement-cold-draft.md`, `procurement-second-review.md`); this one built a different system, so what it found is not what two rounds of procurement repairs had already shaped.

## 1. How it was run

As the second (`procurement-second-review.md` §1), at commit `874457c`, but for three things. **The brief** was a hiring system for a company of 600 in ten points: openings approved by HR with a salary band and a headcount; applications screened by an AI agent, decided only by a person, at most one live per opening; stages from screening to hire, rejection and withdrawal, reconsideration within 30 days; interview loops of two to five, each interviewer's feedback once, nobody interviewing a candidate they referred; offers within the band approved by the hiring manager, above it by the HR director too, expiring after seven days, a counter-offer approved again; a background check by an external provider; headcount never exceeded; personal data erased 180 days after a rejection unless kept by consent, and on request; the metrics a hiring manager and HR want, two of them per interviewer; and 500 open applications brought in from the old system's CSV export. **Both agents were also asked** whether what the project promises about later releases is enough to build on. **The barred records** were the earlier reviews' and the author's: `defects.md`, `before-1.0.md`, any file named for a cold reading or a review, ADR-0173 and ADR-0176, `TODO.md` and `LESSONS.md`.

**What they built.** The drafter's `hiring` module: 9 types, 35 states, 50 transitions, 29 conditions, 10 invariants and 11 metrics; 67 examples passing on the reference runner and on the core; a go-live version whose 6 examples pass on both; a batch of 1,620 objects for 500 legacy applications, 1,619 written on a deployment's store once its five planted defects were each found as a class and decided, and cut over. The integrator ran its twelve steps against a deployment's store, 2,388 exchanges in its transcripts: an 8-thread race with no lost update, 20 refusal cases, worklists over 300 applications in pages, a background-check provider over HTTP, subscriptions, a second version, a go-live of 1,440 objects, the metrics, and an agent proposing through the generated tools what a person decided; and, beyond the brief, a `kill -9` with nothing lost, two processes on one store, and an erasure's bytes on disk.

**How each finding was checked.** As before: each claim framed as a defect was reproduced by a probe of the reviewer's own against a fresh service, the flow checker or both backends, over minimal modules or the agents' own artifacts, before it was classified; two claims were wrong as stated and are corrected below (D580, E4).

## 2. The verdict

**Both agents: a team could build this hiring system on ObjectFlow and pilot it today; it is not ready for production.** Every point of the brief was expressible, with the timers and the roles the application's, as the design says; the examples passed on both implementations at the first run; the go-live found every planted defect as a decidable class; and both named the checker, the refusals with their remedies, `availability` and the dry run as the strongest parts.

**What stood in the way, most serious first**, as they found it: what a release promises was proposed, not binding, with no support window; one SQLite file and no authentication; agents allowed whatever a flow does not forbid, the erasure of a person among it (D579); erased data left in the store's files (D584); a refusal pointing at a record created later (D581); every time rule copied into the scheduler (D591); a resumed import refused (D585); and a flow the checker passed that the core could not store (D578).

## 3. The findings, each checked

The drafter's entries are `P3-<n>`, its summary's numbering, and the integrator's `E<n>`; §6 and §7 hold the logs as written.

| Finding | Claim | How it was checked | Disposition |
|---|---|---|---|
| P3-6 | an attribute `interviewer_id` beside the reference breaks the core | a minimal module: the checker clean, the core's load `500 duplicate column name` | D578 |
| P3-7, E30 | `taken_by` refused on an erasure; agents get it | a minimal module on both checkers; the agent's tools | D579; agents denied by default, to the author |
| P3-8, E13 | a timestamp written as text in a filter matches nothing silently | **wrong as stated**: the core answers `501 not run: comparing Timestamp with Text`, which the drafter's client read as an empty page; the silent case is beside it, `name == 3` and `state == "OPEN"` answering nothing | D580 |
| P3-9 | a filter on a non-indexed derived attribute works, though the documents say only indexed ones may | read: `DESIGN.md` §5.2 (what is indexed) and §13 (a known limit); `reads.rs`, which answers every filter over the type | to the author |
| P3-10, E21 | `mirror-version.py` writes a version the checker refuses | the script over both agents' modules | D586 |
| P3-15, E33 | the README says reads do not page | read | D588 |
| P3-14 | a return to the stage left needs a transition per stage | read: the state is written before the effect (`DESIGN.md` §6) | `write-a-flow.md` §5's pattern |
| P3-13, E27 | durations read back in odd units | read: `wire.md` §2 writes each in the unit that reads best | as designed |
| P3-5, E1 | step 5's `undecided` notices cannot be acted on | not reproduced; the notice is accurate | `TODO.md` |
| P3-4, E2 | some inline-mapping comma mistakes give a schema message, not the friendly one | not reproduced | `TODO.md` |
| P3-11 | an import's refusal cannot be an example's expectation | read: `flow-format.md` §11.3, a refused `import` step fails its example, which no expectation names | `TODO.md` |
| P3-12, E12 | an import of 1,620 objects takes 37 s, a write 80 ms | the agents' measurements, on debug builds with durable commits | ADR-0178's measurement stands; a release build's is `TODO.md`'s |
| E1 | the card writes `unique`'s `with` and `where` as one form | the checker | D587 |
| E4 | a change cannot carry its examples | **wrong as stated**: a probe drafted a change with `<module>.examples`, whose dry run ran them, a failing one refusing the submit `dry_run_clean` | D592, said in `integrate.md` §7 |
| E5, E8 | a refusal names a creation's would-be id, which a later object takes | a probe on both backends | D581 |
| E6, E31 | a person who left and a retired agent still act | read: `DESIGN.md` §5.8, by design | D589; to the author |
| E7, E14, E16, E17, E18, E20, E25, E26, E28 | the wire leaves unsaid what a client relies on | read against `request.rs`, `model.rs`, ADR-0127, ADR-0175 and `DESIGN.md` §7 | D592 |
| E9 | a `taken_by` refusal does not tell `proposable` | read: ADR-0179 told it on `unsatisfied` only | D582 |
| E10 | generated clauses have no description | `flow-format.md` §9 question 2 | already the author's |
| E11 | guards refuse one at a time | as designed: the first refusal applies (`DESIGN.md` §6) | — |
| E15 | a condition's description reads as a fact in a refusal | not a defect of the engine; the descriptions are the author's | — |
| E19 | every time rule is copied into the scheduler | read | D591; engine timers to the author |
| E22 | a connected model becomes mirrors whole at go-live | as designed (D575, `publish-and-import.md` §7) | — |
| E23 | a batch resent unchanged is refused | a probe with the integrator's own batch on a deployment's store | D585 |
| E24 | a disposition decides a whole class | as designed (ADR-0147) | per-object exclusion, `TODO.md` |
| E29 | `query_objects` cannot page | read both renderers | D583 |
| E32 | proposals are not de-duplicated | read | `TODO.md` |
| E34, E36 | nothing on running a deployment | read | D590 |
| E35 | an erasure leaves its values in the files | a probe on a deployment's store, before and after | D584 |
| E3 | a request before any publish answers `unknown_transition`, `self_serviceable` | not checked | `TODO.md` |
| both | the release promise is proposed only; one SQLite file; no authentication | known (ADR-0180, ADR-0178, ADR-0114) | to the author |

## 4. What was changed

D578 to D586 in the code, each with a test that fails without it: both checkers, both backends where both answer, the core's store and import, and the go-live script; D587 to D592 in the guides, the card and the README. ADR-0181 records the five decisions, and amends ADR-0180 to promise a sandbox's controls and the tools' names.

## 5. Questions for the author

Added to `before-1.0.md` §4, each with a recommendation: agents denied by default; an actor in a final state refused; filters on members no index serves; support and deprecation windows and a security-fix policy; JSON held as text in two built-in attributes; and engine-owned timers.

## 6. The drafting agent's log, as it wrote it

The log is model output, kept as written; §3 is the checked reading of it, and the paths it quotes were in the session's scratch directory. Its entry 8 is wrong as stated (§3, D580).

### Hiring flow on ObjectFlow — drafter's log

Started 2026-10-11 12:28 (+08). Repository /home/rdu/RduWs/object_flow at 874457c, read-only. Everything I write is under this directory.

#### Timeline / entries

##### 12:28 Read README.md
Clear pitch. Points me to docs/guides/write-a-flow.md and flow-format-card.md for writing a flow, core/README.md for running the engine. README says "not yet for production: writes run one at a time on one SQLite file, and it does not page reads" while the last commit message says "reads come in pages (ADR-0179)" — a first hint the README status line may lag the code. To check.

##### 12:28–12:31 Reading the authoring docs
Read docs/guides/write-a-flow.md (244 lines), docs/guides/flow-format-card.md (296 lines), the samples people.yaml, reimbursement.yaml, customers.yaml(+examples), flow-format.md §4.1–4.19, §11, declaration-syntax.md §8, publish-and-import.md §4–§7, integrate.md, core/README.md. The core built in 3 s (already cached in core/target).

**Worked well:** write-a-flow.md is a genuinely good on-ramp: a 2-type module, its examples, the two commands to run, then a "patterns for common needs" list that maps almost one-to-one onto my brief (approval by amount, a transition a person must take, rules about who acted, existing records at go-live, an external system's check). flow-format-card.md is the right "one page" reference. The README's "What it is not" (no timers, no permissions, no UI) set my expectations correctly before I modelled anything.

###### Entry 1 — the guides send me to ADRs I was told not to read, and the reasoning lives there
- **What I was trying to do:** understand why "a level that either completes or escalates is two transitions" and how multi-currency works.
- **Where I looked:** write-a-flow.md §5 cites "ADR-0176 decision 6", §6 cites "ADR-0173 decision 5"; flow-format.md cites ADR-0173 decision 8 and ADR-0176 for `taken_by`.
- **What happened:** the user-facing guides lean on ADR numbers for the *why*; a newcomer reads the guide, then has to dive into a 180-ADR directory. (I was barred from 0173/0176 by my brief, but a normal developer would have to read them, which is the point: the guides are not self-contained.)
- **What I did instead:** took the guide's statement at face value.
- **What would have helped:** one sentence of rationale inline instead of an ADR pointer.
- **Severity:** minor.

###### Entry 2 — the normative spec is very dense prose
- **What I was trying to do:** learn the rules for erasure, uniqueness, cascades.
- **Where I looked:** flow-format.md §4.3, §4.12, §4.13 (975 lines; single paragraphs of 300+ words with 4–6 ADR citations each, plus "Corrected 2026-09-27 …" notes inline).
- **What happened:** the information is there, but e.g. §4.13's erasure paragraph packs seven rules into one paragraph; the "corrected on date X" asides are history a reader of a normative spec does not need.
- **What I did instead:** read slowly, relied on the card and the samples.
- **What would have helped:** bullet-listed rules, with history moved to a changelog.
- **Severity:** minor.

###### Entry 3 — how is a text literal written in an examples file?
- **Where I looked:** flow-format-card.md's loans example writes `inputs: { user_id: '"ana@lab"', name: '"Ana"' }` (quoted inside the YAML string); customers.examples.yaml writes `inputs: { name: Dana Lee }` bare; flow-format.md §11.3 says "anything else as its literal" and an identity "by the identity itself, such as `ana`".
- **What happened:** two styles in the two places a newcomer copies from. I will try the bare form and see.
- **Severity:** minor.

##### 12:31–12:34 First draft of hiring.yaml (8 types) and the checker
Wrote the whole module in one go from the card and the samples: Staff (human actor), ScreeningAgent (agent), CheckProvider (service), Opening, Candidate, Application, Interview, Offer, BackgroundCheck. The checker runs in 0.35 s.

**Worked well:** the checker. First run: 9 fatals, all mine (YAML commas inside `{ … }` descriptions), each message saying exactly what to do: `'which the screening agent screens against.' is not a key but the rest of a value cut at a comma: inside { … }, a value containing a comma is quoted … (§2 rule 4)`. Second run: 2 fatals, `check 15 Candidate has no terminal state` — a fair modelling point (I added ARCHIVED). Third run: 0 fatal. Everything else I wrote on first reading — `taken_by`, composite parts with cascades on reject/withdraw, the two-transition "approve / approve_and_escalate" pattern, invariants over a copied identity — was accepted as written. That is a strong sign the guides and card are accurate.

###### Entry 4 — inline-map commas: the trap is documented but still the first thing everyone will hit
- **What I was trying to do:** write one-line attributes `{ type: string, description: What a candidate must have, which … }`.
- **What happened:** 9 fatals. Some came out as `Additional properties are not allowed ("waiting for HR's approval." was unexpected)` rather than the friendly message (lines 79, 87, 664) — the friendly message only fires sometimes.
- **What I did instead:** quoted every description containing a comma.
- **What would have helped:** the same friendly message for every variant (the schema message is cryptic); or samples that always use block style for descriptions.
- **Severity:** minor.

###### Entry 5 — "undecided" notices I cannot act on
- **What happened:** `notice undecided the invariants of Application in SCREENING expand to more than 4096 cases, so whether they can hold together is not decided` — seven times, one per state. My Application has two invariants of its own plus generated ones (required attributes, `taken_by` ×8, uniqueness).
- **What I did instead:** nothing; it is a notice. But it is noise that will train a team to ignore notices, and I do not know whether it hides a real contradiction.
- **What would have helped:** say which invariants blow up the case count, or report it once per type.
- **Severity:** minor.

##### 12:34–12:37 Examples: 67 examples, 13 setups — all pass on the reference runner first time
`check-flows.py hiring.yaml hiring.examples.yaml` → `0 fatal · 10 notices`, `examples: 67 run: 67 passed, 0 failed, 0 not run` in 4.4 s. Because "everything passed first time" is suspicious, I mutated two expectations (a metric row and a derived value): both failed with precise messages (`expected the metric Interview.feedback_turnaround to read a row {'interviewer': 'alice', 'value': '21 h'}, and it reads [...]`). So the runner really checks.

**Worked well:** the examples format. Setups chained with `given`, `creates: [iv1]` to alias parts made by a whole's transition, `after: 181 days` for time, `metrics:` expectations, `warned: [screened]`, generated clause names (`approve_taken_by`, `cv_provided`) — all as documented. Generated guard `cv_provided` made "CV required but erasable" work: an optional personal attribute in `required_inputs`.

###### Entry 6 — the core cannot load a module the checker accepts: `duplicate column name: interviewer_id`
- **What I was trying to do:** run the same examples on the Rust core, as write-a-flow.md §3 says.
- **Where I looked:** `apps/flow-studio/scripts/examples-on-core hiring.yaml hiring.examples.yaml`.
- **What happened:** a 30-line Python traceback ending `replay.Failed: the wire answered 500: the store: duplicate column name: interviewer_id`. My `Interview` has a reference `interviewer: { reference: Staff }` and an attribute `interviewer_id: { type: identity }` (the guide's own pattern: "copy the fact onto the record"). The core's SQL mapping stores the reference in a column named `interviewer_id`, which collides with my attribute. The checker (0 fatal) did not catch it; the service answered 500, a fault, on `/sandbox/load`.
- **What I did instead:** renamed `interviewer_id` → `interviewer_login` and `hiring_manager_id` → `hiring_manager_login` (Opening has a `hiring_manager` reference too).
- **What would have helped:** the checker refusing (or the core quoting/suffixing) a name that collides with a reference's storage column; `<reference>_id` is the most natural name for "the referenced person's id, copied". And the script printing one line instead of a traceback.
- **Severity:** major (a valid flow cannot be loaded by the engine; found only by running the second implementation; in production you would only find it at publish time).
After the rename: `examples-on-core hiring.yaml hiring.examples.yaml` → **67 passed** (6.6 s). Reference runner: 67 passed. Both implementations agree on all 67.

##### 12:37–12:39 Probing the core's sandbox by hand (tools/probe_sandbox.py, tools/probe_query.py)
I wrote a 40-line wire client (tools/ofclient.py) and drove a scenario: open → approve → apply → screen → interviews → feedback → forget; then reads, tools, metrics.

**Worked well:**
- **Erasure is thorough.** After `Candidate.forget`, the candidate's name/email, the application's `cv` and the agent's rationale, and the interview's `notes` are null in the objects *and* in the event payloads (`apply {"inputs": {"cv": null, …}}`, `submit_feedback {"inputs": {"notes": null, "score": 3}}`), each object getting a `forget {"erased": [...]}` event. That is exactly what a PDPA/GDPR auditor would ask for, and I got it by marking attributes `personal: true`.
- **Agent safety by declaration works.** `availability` for the screening agent shows `advance_to_phone_screen unavailable invariant_violated`, and `tools` for `agent` leaves out every `advance_*`, `reject`, `hire*`, `approve*`, `submit_feedback`, `report_*`.
- Metrics read as declared (time_in_state per stage, by source, interviewer load, feedback turnaround, pass rate) with nothing to instrument.

###### Entry 7 — an agent may erase a candidate, and the flow cannot say otherwise
- **What I was trying to do:** keep the screening agent away from irreversible actions.
- **Where I looked:** `tools` for `actor_kind: agent` returned 32 tools including `candidate_forget`, `offer_accept`, `offer_send`, `opening_open`, `staff_onboard`, `staff_leave`. integrate.md §6 warns: "Nothing an agent may take is forbidden unless the flow says so." So I tried `taken_by: [human, service]` on the erasure.
- **What happened:** `fatal names Candidate.forget is an erasure transition, which taken_by does not mark; it marks an initial, external or internal one (ADR-0176)`.
- **What I did instead:** nothing in the flow; my application would have to keep an allowlist of the agent's tools (as integrate.md suggests), i.e. the one irreversible operation is the one the flow cannot guard.
- **What would have helped:** `taken_by` on an erasure (and on an assertion). For a hiring system the erasure is the operation I most want "humans and the retention service only".
- **Severity:** major (agent-safety is the product's headline; the most destructive transition is outside it).

###### Entry 8 — a query filter with a quoted timestamp silently matches nothing
- **What I was trying to do:** the retention scheduler's query: rejected applications at or before a cut-off.
- **What happened (core, sandbox, clock moved 181 days):**
  - `rejected_at <= 2026-01-01T00:00:00Z` → `refused: malformed_filter` (fine, but no hint how to write it)
  - `rejected_at <= "2026-01-01T00:00:00Z"` → **`refused: None`, items `[]`** although both applications were rejected at exactly 2026-01-01T00:00:00Z. A timestamp compared with a string is accepted and is never true.
  - `rejected_at <= now - 180 days` → the two applications. `erasure_due` (a non-indexed derived attribute reading `now`) as a filter → the right candidate.
- **What I did instead:** wrote the cut-off relative to `now`. wire.md shows filter examples for money, states and references but not for a time; declaration-syntax §8.1's literal list has no timestamp literal, which I only found by reading the expression spec.
- **What would have helped:** refusing a timestamp-vs-string comparison as `malformed_filter` (as wire.md says it does for reference-vs-string), and one timestamp example in wire.md's query section.
- **Severity:** major. A retention job that silently finds nothing is a privacy breach nobody notices.

###### Entry 9 — doc says only `indexed` derived attributes are query filters; the core filters on a non-indexed one
- flow-format-card.md: "`indexed` makes it a query filter"; flow-format.md §4.11 an indexed derived "reads only what the store holds on the object itself … MUST NOT read now". My `Candidate.erasure_due` (reads `now` and a set) is not indexed, yet `query … filter: "erasure_due"` answered correctly. Good for me, but I do not know whether it is a supported behaviour or a full scan that will be refused later. **Severity:** minor (uncertainty about what is promised).

##### 12:39–12:45 Point 10, the go-live, end to end on a deployment's store
1. `scripts/mirror-version.py hiring.yaml Application > hiring-golive.yaml` — it said on stderr that BackgroundCheck, Candidate, Interview, Offer, Opening and Staff become mirrors too (every type an application references or composes). So the "staged cutover" is, for hiring, all-or-nothing: during the go-live window nothing but the import can write any of the seven types.
2. Go-live examples (hiring-golive.examples.yaml, 6 examples: import a posted opening, staff, a candidate, an application in INTERVIEWS with one interview done and one scheduled; `publish: { source: [hiring.yaml] }`; then act): **6 passed on the reference runner, 6 passed on the core.**
3. A synthetic CSV export of the old ATS (tools/gen_legacy.py → legacy/*.csv: 60 staff, 40 openings, 500 open applications, 520 interviews) with five planted defects; tools/csv2batch.py maps it to a 1,620-object batch; tools/golive.py runs the procedure of publish-and-import.md §7 on `--store store/hiring.sqlite`.

Results (debug build of the core):
- go-live publish 0.24 s; **dry run 12.6 s found exactly the five planted defects**, each as a class with a sample legacy key: `Application.state: is not declared` (the "On Hold" stage), `Candidate.email_unique: does not hold` (sample C-A4311, C-A4312), `Application.referral_names_its_referrer`, `Interview.feedback_from_its_interviewer`, `Interview.score_from_one_to_four`.
- decided in legacy/disposition.json (one exclude, four admits with reasons); **import 24.9 s, written 1,619**; cutover publish 5.8 s; `exceptions` lists the admitted objects afterwards; a third candidate with the duplicated email is refused `invariant_violated` naming both holders.
- after cutover: the first new application is `APP-04801` (the legacy sequence raised past 4800); 140 applications in INTERVIEWS; `work_in_progress` 200/120/140/40; time-in-stage reaches back to the legacy `entered` times; a late import of an Interview is refused `refused_types: ['Interview']`.

**Worked well:** this is the best part of the system. The dry run → disposition file → import → cutover loop is exactly what I would want for a migration review, the findings were precise, nothing was half-written, and every rule of the flow (uniqueness, who gave feedback, score range) was applied to the legacy data as invariants.

###### Entry 10 — mirror-version.py writes a module the checker refuses
- **What happened:** `check-flows.py hiring-golive.yaml` → `fatal names metric offers_accepted counts 'accept', which is not a transition of Offer` (and `offers_declined`). The script drops every transition of a mirror but keeps `measure: transition_count` metrics that name them.
- **What I did instead:** removed Offer's two metrics from the go-live version by hand (noted in its header).
- **What would have helped:** the script dropping (or warning about) metrics over dropped transitions.
- **Severity:** minor (easy to work around, but the documented path breaks on a plain module).

###### Entry 11 — import findings can only be tested on a live store, not in an examples file
- An examples-file `import` step "admits nothing … an object that … violates an invariant refuses the step", and an import is never the request under test, so I cannot write "the legacy row with a 5 on the old scale is caught" as an example. I proved it with the CSV run instead. **Severity:** minor.

###### Entry 12 — legacy import throughput
- 1,620 objects: dry run 12.6 s, import 24.9 s on the debug build (I did not build release). Fine for 500 applications; for a larger ATS (tens of thousands) it would need batching and the release build, and the whole batch is one transaction. **Severity:** minor (note for planning).

###### Entry 13 — a duration read back as "30782631 s"
- `Application.time_in_state` answered `"value": "30782631 s"` (356 days). wire.md says a duration comes back "in whichever unit reads best"; a dashboard has to parse and re-format anyway. **Severity:** minor.

##### 12:45–12:47 A third version over the live store: HR asks for "On Hold" (v3/hiring.yaml, tools/publish_v3.py)
The old ATS had an "On Hold" stage (the row I excluded at go-live). A realistic first change request.

###### Entry 14 — "return to the stage it came from" costs 8 transitions and 4 conditions
- **What I was trying to do:** one `put_on_hold` from any live stage that records where it came from, and one `resume` back.
- **What happened:** a transition has one target, so `resume` cannot go "back". My first attempt recorded the source stage in the effect: `assign: { location: held_from, expr: "if state == SCREENING then Stage.SCREENING else …" }` — but write-a-flow.md §5 says "the state is written first, then the effect runs", so `state` in an effect is already ON_HOLD; the checker also refused it (`required … enters ON_HOLD without setting 'held_from'`). There is no "previous state" to read.
- **What I did instead:** `hold_from_<stage>` ×4 each assigning a literal `Stage.X`, and `resume_<stage>` ×4 each guarded `held_from == Stage.X`: 58 transitions instead of 52. `availability` then offers exactly the right `resume_*` (verified on the store), so a screen copes, but the flow reads worse and the same pattern recurs for the brief's "reconsider within 30 days" (I simplified it to "returns to screening", see coverage table).
- **What would have helped:** a way to read the state being left in an effect (e.g. `this_event.from_state`), or a documented idiom for "return to where it was".
- **Severity:** minor-to-major (modelling cost, not a blocker).

**Worked well:** publishing v3 over the go-live store (1,620 objects): submit 9.5 s (it runs the examples), dry run 9.4 s, publish 9.4 s; the dry-run report lists, per new transition, which live objects gain it (`gained:Application.hold_from_offer … affected 40`), and every live application then had its `hold_from_*`. v3's 67 examples also pass on both implementations. Note: a dry run's answer has `"verdict": null` and only the report (my script assumed a verdict; minor).

###### Entry 15 — README status contradicts the guides
- README: "not yet for production: its writes run one at a time on one SQLite file, and it does not page reads." integrate.md §5 and wire.md: "Reads come in pages (ADR-0179)", and the last commit is "reads come in pages". A team deciding whether to adopt reads the README first. **Severity:** minor.

###### Entry 16 — things the application must build that a hiring team will expect "the system" to do
Not defects (README "What it is not" is honest about each), but they are real work for my team:
- **Timers.** Offer expiry (7 days) and the 180-day erasure are scheduler jobs that poll (`available` for `Offer.expire`; `query Candidate filter "erasure_due"`) and request the transition. The flow only refuses an early expiry (`seven_days_passed`, `temporal`).
- **Roles.** "HR approves", "the HR director approves", "the hiring manager approves" — the flow can only say *the recorded approver equals the opening's copied hiring-manager identity* and *the director is not the manager*; who is HR/director is my app's permission table.
- **Files.** The CV's bytes, virus scanning and deletion on erasure are the app's; the store keeps `{hash,size,media_type}`.
- **Authentication.** The service "authenticates nobody"; it must sit behind our app on loopback.
- **Agent tool allowlist** (Entry 7).
- **Search** (no text search) and names in metrics (metrics give `staff-4`, the app maps ids to names).

##### 12:48 Concurrency and rate (tools/probe_concurrency.py, fresh deployment store, debug build)
**Worked well:** 20 concurrent `apply` requests of one candidate to one opening → exactly 1 `satisfied`, 19 `invariant_violated one_live_application_an_opening`. Writes are serialised, so a count-based invariant is race-free. 200 sequential creations 3.7 s (≈54/s, debug build); a 201-row query 35 ms. A 600-person company's hiring volume is far below this.

#### What I built

| File | What |
|---|---|
| `hiring.yaml` | the flow: 9 types (Staff, ScreeningAgent, CheckProvider, Opening, Candidate, Application, Interview, Offer, BackgroundCheck), 35 states, **50 transitions**, 29 conditions, 10 invariants, 11 declared metrics (+ every type's standard metrics) |
| `hiring.examples.yaml` | 13 setups, **67 examples**: reference runner 67/67 passed; Rust core 67/67 passed |
| `hiring-golive.yaml` | the go-live version from `mirror-version.py hiring.yaml Application` (7 of 9 types mirrors), hand-fixed (Entry 10) |
| `hiring-golive.examples.yaml` | 2 setups, **6 examples** (import → publish hiring.yaml → act): runner 6/6, core 6/6 |
| `v3/hiring.yaml` | a third version adding On Hold (58 transitions); its 67 examples pass on both; published over the go-live store |
| `legacy/*.csv`, `legacy/batch.json`, `legacy/disposition.json`, `legacy/dryrun.json` | a synthetic old-ATS export (500 open applications, 520 interviews, 40 openings, 60 staff, 5 planted defects), its batch, the dry run and the decisions |
| `tools/` | `ofclient.py` (wire client), `gen_legacy.py`, `csv2batch.py`, `golive.py`, `probe_*.py`, `publish_v3.py` |
| `store/` | the deployment stores the runs left (hiring.sqlite after go-live; hiring-v3.sqlite; concurrency.sqlite) |

`check-flows.py hiring.yaml hiring.examples.yaml` → `0 fatal · 10 notices` (1 `warn` by design, 2 `coverage`, 7 `undecided`). `check-flows.py --previous=hiring-golive.yaml hiring.yaml` → 0 fatal.

#### The brief, point by point

| # | Point | How | Expressed in the flow? |
|---|---|---|---|
| 1 | Openings, HR approval, close on fill or cancel | `Opening` DRAFT→AWAITING_APPROVAL→POSTED→FILLED/CANCELLED; `approve` `taken_by: [human]`, records `approved_by`; invariant opener ≠ approver; band and headcount guards; `cancel` refused while any application is live (`dependent`) | Yes. *Which* people are HR is the app's |
| 2 | Applications, one live per opening, agent screens, only a person rejects/advances | `apply` requires CV (optional personal attribute + generated `cv_provided`); invariant `one_live_application_an_opening` (count over the candidate's applications); `screen` `taken_by: [agent]`; every advance/reject/hire `taken_by: [human]`; `screened` a `warn` guard | Yes |
| 3 | Stages; reject with reason / withdraw before hire; reconsider within 30 days | 4 live states + HIRED/REJECTED/WITHDRAWN; `reject` requires `rejection_reason`; `reconsider` guarded `now <= rejected_at + 30 days` | Partly: reconsider returns to SCREENING, not to the stage it was rejected at (one target per transition; doing it properly is N transitions, Entry 14) |
| 4 | Loop of 2–5, feedback once after, by the interviewer, offer only when all in, no interviewing a referral | `Interview` part of `Application`; `loop_has_room` (<5), `loop_of_two`, `all_feedback_in`; `submit_feedback` SCHEDULED→DONE (once by construction), `interview_happened` (`temporal`), score 1–4 guard + invariant, `feedback_from_its_interviewer` invariant over a copied identity; `interviewer_did_not_refer` | Yes |
| 5 | Offer approval by band, director above band, 7-day expiry, counter-offer re-approval | `approve` (within band) / `approve_and_escalate` (above) / `approve_as_director`; invariants: manager approval = opening's hiring manager, director ≠ manager, above band never APPROVED/SENT/ACCEPTED without director; `accept` refused after 7 days; `expire` refused before 7 days; `counter` clears approvals, bumps `revision` | Rules yes; the expiry *happening* is the app's scheduler |
| 6 | Background check before hire | `BackgroundCheck` part; `report_clear`/`report_adverse` `taken_by: [service]` (the provider's integration is a service actor); `hire` guarded `cleared_by_provider` | Yes (an evaluator would also work) |
| 7 | Never over headcount, last hire closes | `hires` derived count; invariant `hires <= headcount`; `hire` / `hire_last` with complementary guards, `hire_last` calls `Opening.fill`; `set_headcount` never below hires | Yes, but as two transitions the app chooses between by `availability` |
| 8 | Erase 180 days after rejection unless consent; forget on request | personal: candidate name/email, CV, agent rationale, interview notes; `Candidate.forget` erasure → `Application.forget` → `Interview.forget`; erases objects **and** event payloads; `Candidate.erasure_due` derived for the scheduler | Erasure yes; the 180-day *trigger* is the app's scheduler. The agent cannot be barred from erasing (Entry 7) |
| 9 | Metrics | `time_to_hire` (per opening), standard `time_in_state` (per stage), `screening/phone_screen/interview/offer_pass_rate` (from a `stage_rank` I maintain by hand in each advance), `offers_accepted`/`offers_declined`, `interviewer_load`, `feedback_turnaround` (person-naming, marked), `applications_by_source`, `Opening.time_to_fill` | Yes; pass rates need a hand-kept `stage_rank` |
| 10 | Go-live of 500 applications from CSV | go-live version + 6 examples; a 1,620-object batch from CSV dry-run, decided, imported, cut over on a deployment store | Yes; the CSV→batch mapping is code my team writes |

#### Verdict

**Could my team build this hiring system on ObjectFlow?** Yes. In about 20 minutes of wall-clock work I had a 9-type, 50-transition flow covering all ten points, 67 examples passing identically on both implementations, and a 500-application go-live run end to end on a deployment store. The language expressed every *rule* of the brief; what it cannot do — start timers, decide roles, store files — it says up front it will not do. The checker, the examples format and the import/disposition loop are the strongest parts, better than what we would build ourselves in a conventional ATS backend.

**Could we run it in production?** Not yet; I would pilot it. What stands in the way, most serious first:
1. **No promise binds yet.** ADR-0180 is *proposed* and binds only from 1.0.0, which has no date; we are on 0.2.0 "in intent". The project itself says "in design, under review by its author", one author.
2. **Operational base.** One SQLite file on one machine, no PostgreSQL yet, no replication/HA story beyond "copy the file", and the service authenticates nobody. Fine for a pilot, not for a system HR depends on daily.
3. **A valid flow the engine could not load** (Entry 6: `duplicate column name: interviewer_id`, a 500 on load). The two implementations agreeing on examples does not yet mean the checker catches everything the store refuses.
4. **Agent safety stops short of erasure** (Entry 7): the most destructive transition cannot be `taken_by`-restricted; we would keep our own allowlist.
5. **Silent wrong answers at the read edge** (Entry 8): a mistyped timestamp in a query filter matches nothing instead of being refused — dangerous for exactly the retention job privacy law cares about.
6. Modelling costs we can live with: one target per transition (hire/hire_last, per-stage resume), hand-kept stage rank for funnel metrics, tooling rough edges (mirror-version.py output refused by the checker, the examples-on-core traceback).

**Is what the project promises about later releases enough to build on?** The *content* of ADR-0180 is the right shape — additive-only `/v1`, flows valid under one release stay valid within a major, new checks arrive as notices first, stores upgraded never stranded, held by a frozen format-1 corpus and a wire baseline test. That is enough to start writing code against. It is not enough to commit a production system, because it is not yet in force and says nothing about: **when 1.0.0 lands and how long a major is supported**; a security-fix policy; whether moving a deployment from SQLite to PostgreSQL is an "upgrade" the promise covers (ADR-0177 covers releases, not backends); whether the *import tooling* (`mirror-version.py`, the batch/disposition shapes beyond the wire keys) and the generated agent tools' names are promised; and whether behaviour I relied on but the docs do not state (filtering on a non-indexed derived attribute, Entry 9) will keep working. I would need those answers, plus a named maintainer and release cadence, before a production commitment.

#### Findings, most serious first

| # | Finding | Severity |
|---|---|---|
| 6 | The core refuses a checker-valid flow: attribute `interviewer_id` collides with reference `interviewer`'s storage column → `500 duplicate column name` on load; the examples-on-core script prints a traceback | major |
| 7 | `taken_by` is refused on an erasure; an agent's tools include `candidate_forget` and the flow cannot prevent it | major |
| 8 | Query filter `rejected_at <= "2026-01-01T00:00:00Z"` is accepted and silently matches nothing; no timestamp literal is documented for filters | major |
| — | Release promise (ADR-0180) proposed, binds only from an unscheduled 1.0.0; no support window, security or backend-migration promise | major (for production) |
| — | Deployment is one SQLite file, no auth, no HA; PostgreSQL not built | major (for production) |
| 14 | "Return to the previous stage" (On Hold, reconsider) needs one transition per stage; an effect cannot read the state being left | minor-to-major |
| 10 | `mirror-version.py` emits a go-live module the checker refuses (keeps transition_count metrics over dropped transitions) | minor |
| 9 | Docs say only `indexed` derived attributes are query filters; a non-indexed one works — unclear what is promised | minor |
| 5 | Seven `undecided … more than 4096 cases` notices I cannot act on | minor |
| 4 | Inline-map comma trap; some variants give a cryptic schema message instead of the friendly one | minor |
| 11 | Import refusals cannot be tested in an examples file (only on a store) | minor |
| 12 | Import throughput: 1,620 objects dry-run 12.6 s + import 24.9 s (debug build), one transaction | minor |
| 13 | Durations read back in odd units (`"30782631 s"`) | minor |
| 15 | README says reads are not paged; guides say they are | minor |
| 1, 2, 3 | Guides rely on ADR citations for rationale; normative spec is dense; two styles of text literal in examples | minor |

**What worked well:** write-a-flow.md and the card were accurate enough that a 9-type flow checked clean after fixing only YAML quoting and one terminal state; 67 examples passed first time on the runner and on the core; the checker's messages say what to do; `taken_by`, generated clauses (`cv_provided`, `approve_taken_by`), `warned`, `metrics` expectations and `creates` aliases all behaved as documented; erasure reaches objects and event payloads; `availability` shows exactly the one of a complementary transition pair that applies; the go-live dry run found every planted defect as a decidable class; a v3 publish over live data reports which objects gain what; the one-live-application invariant held under 20 concurrent writers.

## 7. The integrating agent's log, as it wrote it

As §6; its E4 is wrong as stated (§3).

### Integrator log: a hiring (ATS) application on ObjectFlow

Reviewer: a developer new to ObjectFlow. Repository at commit 874457c, read-only. Started 2026-10-11 12:28 +08.

Entries are written as I go. Format per entry: what I was trying to do / where I looked / what happened / what I did instead / what would have helped / severity.

#### Timeline

- 12:28 Read README.md. Clear pitch. Notes: README says "not yet for production: its writes run one at a time on one SQLite file, and it does not page reads", but the most recent commit (35347c1) says "reads come in pages (ADR-0179)". Need to check which is true.
- 12:29 Read integrate.md, wire.md, core/README.md, write-a-flow.md, flow-format-card.md, ADR-0180, publish-and-import.md §4-7, flow-format.md §4.3-4.18. Built the service in release (26 s, clean).
- 12:32 Wrote flow/hiring.yaml (7 types, ~650 lines). The checker's messages are excellent (line numbers, what to do). Three rounds to 0 fatal.
- 12:38 Steps 1-4 run against a deployment's store (transcripts/step01..04). Wall-clock times in this log are from `date`.
- 12:54 All twelve steps and the production probes run. Non-ASCII names (`陈伟 (Chen Wei) – Ñúñez`) store and filter fine; U+0000 is refused `malformed`. No process left running (checked with `ps`).
- 12:28-12:54 is wall-clock time from `date`; the work went quickly because the guides were enough for most steps.

#### Entries

##### E1. "At most one live application per opening" cannot be a `unique`
- **Trying to do**: declare that a candidate has at most one live application per opening.
- **Where I looked**: flow-format-card.md "An attribute" (`unique`: `true`, `in_scope`, or `{ with: [...], where: ... }`), flow-format.md §4.12.
- **What happened**: the card writes `{ with: [<attribute>], where: <expression> }` as ONE form, which is exactly what I needed (`unique: { with: [opening], where: open }`). The checker: `fatal schema unique is true, in_scope, { with: [<attribute>, …] } or { where: <expression> }, one of them`. flow-format.md §4.12's table lists them as four separate forms. The card is wrong (or misleadingly compressed).
- **What I did instead**: a traversal invariant, `not open or count(o in candidate.applications where o.open and o.opening == opening) <= 1`. It made the checker's step 5 give up ("expand to more than 4096 cases ... not decided", 7 notices).
- **What would have helped**: `with` and `where` combinable (a partial composite unique index is ordinary SQL), or the card not suggesting it.
- **Severity**: minor.

##### E2. Commas inside flow-style YAML descriptions
- **What happened**: 7 fatals, `'which screening reads.' is not a key but the rest of a value cut at a comma`. Clear message, quick fix. Worked well: the checker explains YAML's own trap.
- **Severity**: minor (good message).

##### W1 (worked well). Flow check, examples on both implementations, and publishing to a deployment's store
- 4 examples written for the riskiest rules (referrer never interviews; no offer before all feedback; last hire fills the opening; above band needs the HR director) passed on the reference runner (`check-flows.py`) and on the core (`examples-on-core`, 0.7 s). `examples-on-core` is a good tool.
- Step 1: `objectflow-service --store store/hiring.sqlite --port 0 --evaluator bgcheck=http://127.0.0.1:<p>/evaluate --evaluator-timeout "2 s"` started first time; `GET /health` said `shape: deployment, version: 0`; draft → submit → publish of a `DeclarationChange` installed version 1. The procedure matches integrate.md §2/§7 and test_golive.py. Transcript: transcripts/step01_publish.md.
- A trick I was unsure would be allowed did work on both: "the last hire closes the opening" as `foreach: { range: opening.hires - opening.headcount + 1, limit: 1, steps: [call fill] }` (a range that is <= 0 runs nothing). There is no `if` effect step; this is the only conditional effect I could find. Would have liked a documented pattern for "do X when this was the last one".

##### E3. A request before any flow is published says `unknown_transition`, remedy `self_serviceable`
- **What happened**: `request onboard` on a version-0 store → `{"kind":"unknown_transition","remedy":"self_serviceable","transition":"onboard"}`. The remedy tells a client to fix its request; the real cause is "no flow published". `/health` says `version: 0`, so a client can tell, if it thinks to look.
- **Severity**: minor.

##### E4. The publish report's `examples` is empty; the examples file cannot travel with the change (that I could find)
- **Trying to do**: have the publish run my examples, as write-a-flow.md says "the engine runs those examples every time the flow is published".
- **Where I looked**: integrate.md §7, wire.md "Changing the declaration", publish-and-import.md §1-2.
- **What happened**: the report has `"examples": {}`. No document says how to include `hiring.examples.yaml` in a `DeclarationChange`'s `source` (whether as another key, and under what name).
- **What I did instead**: ran the examples locally before publishing (check-flows.py, examples-on-core).
- **What would have helped**: one sentence in integrate.md §7 on whether a deployment's publish runs examples, and how they are sent.
- **Severity**: minor.

##### E5. A refusal of a duplicate names an object that was never created
- **Trying to do**: show "which other records" a refusal points at (step 2, a second onboarding of an existing identity).
- **What happened**: `{"kind":"invariant_violated","remedy":"self_serviceable","failures":[{"object_id":"staff-2",...,"invariant":"user_id_unique"},{"object_id":"staff-21",...}],"objects":["staff-2","staff-21"]}`. `staff-21` is the id the refused creation would have had: `get staff-21` → `null`. A screen that links every id in `objects` links to nothing; the id is also burnt.
- **What I did instead**: my explainer drops ids that `get` answers `null` for (an extra read per refusal).
- **What would have helped**: `objects` naming only the existing holders (wire.md §3 says "the objects in conflict, such as the other holder of a unique value"), and the new object reported as "the object being created".
- **Severity**: minor.

##### E6. A user who has LEFT, or an agent RETIRED, still acts
- **Trying to do**: offboard a user (step 2) by `Staff.leave`.
- **What happened**: `iv8@corp` after `leave` (state LEFT, final) registered a candidate: `satisfied`. The store knows the actor is gone and still accepts its requests. integrate.md §3 only says an *unknown* actor is refused.
- **What I did instead**: my application must block departed users itself (it would anyway, at its login), but a long-running agent or a service key revoked in the store is not stopped by the store.
- **What would have helped**: either a documented sentence ("the store never reads an actor's state; revoke at your gateway") or an opt-in rule on the actor type (a state category that refuses its requests).
- **Severity**: major (a revoked agent key keeps working unless the gateway also revokes it; easy to get wrong because the store has a `REVOKED`-style state that looks authoritative).

##### W2 (worked well). Step 2-3: actors, idempotency, versions, concurrency
- Mirroring 15 staff, an agent and 3 services, each under an idempotency key `sync-staff-<user_id>`: a re-run of the sync replays (`"replayed": true`). Acting by identity (`rec.alice@corp`) and by object id (`staff-2`) both record `actor_id: rec.alice@corp`; the agent's kind is recorded `agent` from its type.
- Step 3: create (`register` + `apply` with a CV as `{hash,size,media_type}`, bytes kept in my blobs/ dir), retry under the same key → same answer `replayed: true`; a different body under the same key → `409 key_reused`; edit with `expected_version` → `stale` with `expected`/`actual` for the second recruiter, who re-reads and succeeds. 8 threads x 10 rounds on one record at the version each read: 10 satisfied, 70 `stale`, no lost update, no 5xx. Two recruiters applying the same candidate to the same opening at the same instant: exactly one succeeds, the other is refused by the invariant. Transcript: transcripts/step03_edit.md.

##### E7. An idempotency key is scoped to the actor, which the wire does not say
- **What happened**: the same key and body from another recruiter is not a replay: it is a new request (here refused by my one-live-application invariant). The 409's text says "this actor applied the idempotency key …"; wire.md §4 says only "the same request under the same key answers its first verdict".
- **What would have helped**: one sentence: keys are per actor. Matters for a server-side retry queue that re-sends under a service identity.
- **Severity**: minor.

##### E8. A refusal can name an object id that is then given to a different object (raised from E5)
- **Trying to do**: step 4, show "which other records" each refusal points at.
- **What happened** (transcripts/step04_refusals.md): scheduling the referrer as interviewer → `invariant_violated`, failures `[{"object_id":"interview-31",...,"invariant":"not_interviewed_by_referrer"}]`. `interview-31` did not exist; the next interview I scheduled (a different interviewer) became `interview-31`. Same with Opening: the band-out-of-order refusal named `opening-30`; the next opening created *is* `opening-30`, and the very next refusal (`apply` to a DRAFT opening) names the real `opening-30`. Ids are `<type>-<n>` with one counter, and a refused creation's number is reused. So a refusal saved in a log, an attempt row, or shown in a notification later points at an unrelated record.
- **What I did instead**: my explainer calls `get` on every id a refusal names, and labels missing ones "does not exist" — correct only at that instant.
- **What would have helped**: the refused creation reported as "the object being created" (no id), or numbers never reused.
- **Severity**: major (a refusal's pointer is wrong exactly in the case it is meant to help: a person clicking through to "the record in the way" can land on another candidate's record).

##### E9. A `taken_by` refusal does not say it is proposable
- **Trying to do**: step 4/12, the agent asks to advance an application (a transition marked `taken_by: [human]` and `proposable: true`).
- **What happened**: `{"kind":"invariant_violated","remedy":"delegable","failures":[{..."invariant":"advance_to_phone_screen_taken_by"...}],"objects":[]}`. No `proposable` field (wire.md lists `proposable` only on `unsatisfied`). integrate.md §4 tells an agent to file a proposal "where the refusal says `proposable: true`", so an agent following the guide hands the decision to a person instead of proposing.
- **What I did instead**: read the transition's `proposable` from `declaration`, and the agent's tool list (step 12).
- **Severity**: minor.

##### E10. Generated clauses have no description to show
- **What happened**: `notes_provided`, `advance_to_phone_screen_taken_by`, `reject_taken_by`, `user_id_unique` are not in `declaration`'s `conditions`/`invariants`, so a screen has only the raw name. My own conditions come back with their `description`, which is excellent for "why".
- **What I did instead**: a small table of hand-written texts for the generated families (`*_provided`, `*_taken_by`, `*_unique`, `*_required` …).
- **What would have helped**: the declaration listing generated clauses with a generated description.
- **Severity**: minor.

##### E11. Validation reports every bad input at once, but guards one at a time
- **What happened**: three bad inputs → one `invalid_input` naming all three (`band_min: wrong_currency`, `headcount: malformed`, `level: wrong_type`) - good for a form. But an unknown input key (`colour`) hides all the others (only `colour: unknown_input` is reported). And a request failing a guard and an invariant reports only the first (`check` returned only `headcount_positive` while the band was also out of order), so a form fixes errors one round-trip at a time.
- **Severity**: minor.

##### W3 (worked well). Refusals are actionable
- `unsatisfied` with `objects` is exactly what a screen needs: "move to offer" refused `loop_complete`, `dependent`, `objects: [interview-31, interview-32, interview-33]`; the referrer refusal names the referrer (`staff-10`) and the application. `temporal` for feedback before the interview; `delegable` for feedback by the wrong interviewer and for the agent's advance; `unavailable` with the state for a second feedback. The remedy-based fallback (five remedies) made a generic explainer easy (app/ofclient.py `explain`, 50 lines). Catalogue of 20 cases: refusals.json.

##### W4 (worked well). Step 5: worklists and paging
- Filters through references and on derived attributes work: `opening.department == "Robotics" and open`, `state == Application.INTERVIEWS and awaiting_feedback > 0`, `opening.hiring_manager.user_id == "hm.evan@corp"`, `scheduled_at < now - 1 h`; `fields` picks the columns; `order: "entered_at(state)"` gives "oldest in stage first". Every worklist over 300 applications / 600 interviews answered in 1-14 ms. Paging by cursor (limit 50/100) met an application created between pages exactly once, no duplicates. `available` gives "what can be done to what" (92 applications for advance_to_phone_screen). Transcript: transcripts/step05_worklists.md.

##### E12. Writes are slow: ~80 ms each, ~12/s from one client, 24/s from eight
- **What happened**: 20 sequential `set_consent` writes: 80 ms each; 20 creations: 85 ms each (`get` 0.7 ms). Seeding 300 applications (1220 writes) from 8 threads took 51 s; the median request waited 284 ms, p95 585 ms. The store was 3 MB.
- **What I did instead**: nothing; for 600 staff (a few thousand writes a day) it is enough. But a bulk job (re-scoring, a mass rejection when an opening is cancelled, the nightly erasure sweep) is minutes, and every user waits behind it, since writes are one at a time.
- **What would have helped**: a stated write throughput figure in core/README.md, and a batch request.
- **Severity**: minor for this brief (major for anything larger).

##### E13. No timestamp literal in a filter; a string compared to a timestamp is a 501
- **Trying to do**: "interviews held before this morning" for an interviewer's worklist.
- **What happened**: `scheduled_at < now` and `scheduled_at < now - 1 h` work. `scheduled_at < 2026-10-11T00:00:00Z` → `refused: malformed_filter`; `scheduled_at < "2026-10-11T00:00:00Z"` → **HTTP 501** `not run: comparing Timestamp(1791686260) with Text("2026-10-11T00:00:00Z")`; `timestamp("…")` → 501 `not run: the function timestamp`. wire.md says a malformed filter is `refused`, never a fault. No document shows how to write a fixed instant in a filter.
- **What I did instead**: express every date window relative to `now`, computing the offset in the application.
- **Severity**: minor.

##### E14. Query `order` grammar is only in ADR-0127
- wire.md lists `order` as a key with no form; `entered_at(state)`, `"<key> desc"` and "indexed attributes only" are in ADR-0127. Found by grep. **Severity**: minor.

##### W5 (worked well). Step 6: buttons and forms from `availability` + `declaration`
- One generic renderer (app/step06_actions.py, ~100 lines) turns each offer into a button (available, no input), a form (inputs typed `money(SGD)`, an enumeration, `timestamp`, `file`, a reference type → picker filled by `query`), a disabled button with the reason and remedy, or nothing (not from this state). Required vs optional comes from `declaration`'s `required_inputs`/`optional_inputs`/`inputs[].optional`. Output: actions.html. A transition taking no input is decided for the asker: the offer's `approve_as_hiring_manager` is a button for its hiring manager and disabled for another ("The hiring manager's approval is the hiring manager's", delegable).
- `check` gives a form's pre-flight: the HR director approval naming a non-director → `unsatisfied approver_is_hr_director, delegable, objects: [staff-6]`.

##### E15. A form's reason text reads as if the condition already holds
- **What happened**: a condition's description is "stated as what holds when it is satisfied" (flow-format.md §4.7), so a disabled `send` shows "The hiring manager approved the offer, and the HR director too where it is above band." - which reads as a statement of fact, the opposite of the situation.
- **What I did instead**: prefix "Needs: " in the screen.
- **What would have helped**: integrate.md saying how to phrase a refusal from a description.
- **Severity**: minor.

##### E16. Effects that create return the parent's id, not the created object's
- **Trying to do**: `draft_offer` creates an Offer; the screen should open the new offer.
- **What happened**: the verdict is `{"object_id":"application-27","events":[1579,1580]}`; the offer's id is nowhere in it. I re-read the application and took the last of `offers` (racy if two people draft at once), or could read `events` by position.
- **What would have helped**: the verdict listing objects it created (`created: ["offer-902"]`).
- **Severity**: minor.

##### E17. Undocumented `corrects` input on the built-in `record` (label) offer
- wire.md says a label form takes `name` and an optional `note`; the offer also lists `corrects` of type `label`. **Severity**: minor.

##### W6 (worked well). Step 7: the background check through a real HTTP endpoint
- A 100-line provider stand-in (app/bgcheck_provider.py) plus the deployment's `--evaluator bgcheck=<url>/evaluate`: the engine posts `{"evaluator":"bgcheck","function":"clear","arguments":["BG-ANA-0001"]}`. Hire while pending → `unsatisfied background_clear, temporal`, with `consulted: {"bgcheck.clear(BG-ANA-0001)": "unsatisfied"}`; after the provider's clear result → `satisfied`, and the hire's event keeps `consulted` and `as_of` (an audit trail of the check, which HR would want). A 500 or a timeout → `temporal`, `unknown: true`, `"unanswered"`, and a line on the service's stderr (`answered 500`, `was not reached: timeout: global`). `availability` lists the guard in `unevaluated` rather than calling out. Transcript: transcripts/step07_bgcheck.md, step07b_bgcheck_faults.md.

##### E18. An evaluator answers yes/no, so "pending" and "adverse" look the same
- **What happened**: an adverse result and a pending one both give `unsatisfied ... remedy temporal` ("try later"), since the remedy is the condition's. A hiring manager is told to wait forever on a failed check.
- **What I did instead** (would do): a second function `bgcheck.adverse(ref)` guarding with remedy `unreachable_from_here`, or have the bridge record the result on the application as a service transition (and then the evaluator is not needed).
- **What would have helped**: a pattern for a three-valued external answer in write-a-flow.md §5.
- **Severity**: minor.

##### W7 (worked well). Step 8: subscriptions, an event reactor and timers
- app/reactor.py (~150 lines): three subscriptions (`Application` with `changes_state_only`, `Offer`, `Interview` filtered to `submit_feedback`), each drained page by page with `pull` and `acknowledge`d at `settled`; handlers make notifications, and an accepted offer automatically opens a background check at the provider and records its reference as the bridge's service actor (verified: `bg_check_ref: BG-application-196`). Nothing scans the store.
- On a sandbox (same reactor code; the clock moved with `/sandbox/clock`): the offer expired on day 8 and not day 6 (`expire` on day 6 → `unsatisfied seven_days_passed, temporal`); accepting after expiry → `unavailable EXPIRED`; reconsidering on day 31 → `unsatisfied within_reconsideration_window`; at day 181 the rejected candidate without consent was erased (`forget` → name, email, CV gone; the rejection event's personal `note` input redacted to null; a later write refused `not_erased`), the one with consent kept. A candidate's "forget me" works at any time. Transcripts: step08a_events_deployment.md, step08b_events_sandbox.md.

##### E19. Every time rule is written twice: in the flow and in the application's timer table
- **Trying to do**: expire an offer seven days after it is sent; erase 180 days after rejection.
- **Where I looked**: integrate.md §8 ("schedules: timers, reminders and escalations request transitions or read `available`"), README ("Not a workflow orchestrator").
- **What happened**: the engine knows the rule (`seven_days_passed: now >= sent_at + 7 days`) but cannot say when it will become true, nor wake anyone. My reactor recomputes `recorded_at + 7 days` itself, a copy of the rule that will drift when version 2 changes it (step 9 does). The alternative the guide names, sweeping `available(Offer, expire)` on a schedule, is polling.
- **What I did instead**: a timer table in the application, rebuilt from events, re-checked by the engine at fire time (the request is refused `temporal` if early, so the copy can only be late or early, never wrong).
- **What would have helped**: an offer or event saying when a `temporal` guard next changes (`ready_at`), so a scheduler needs no copy of the rule; or a documented recipe for a scheduler (persisting timers, recovering after a crash, catching up after downtime).
- **Severity**: major (it is the main thing an application must build that the engine does not provide, and the place the "declared once" promise leaks).

##### E20. A subscription starts empty: no backlog
- **What happened**: subscriptions created after 1600 events delivered 0 events. Correct for a feed, but the reactor's timers for offers already out and rejections already made (and for the 500 imported at go-live) must come from a one-off query, which no guide mentions.
- **Severity**: minor.

##### W8 (worked well). Step 9: version 2 and its impact on live records
- v2 adds a rule (`move_to_offer` needs the loop to average at least 2.5), an attribute and a transition, and shortens the offer window from 7 to 5 days. `check-flows.py --previous=hiring.yaml hiring-v2.yaml` → 0 fatal. The `DeclarationChange`'s dry run report said exactly what my own count said: `lost:Application.move_to_offer` for the 8 live applications whose loop averages below 2.5 (I computed the same 8 independently), and `newly_enforced:...loop_passed` for 78 objects. Publishing at the version read installed version 2; the low-scoring application asking to move is refused `loop_passed`, pointing at its two interviews; `declaration` with `version: 1` still reads the old rule. This is the strongest part of the product. Transcript: transcripts/step09_version2.md.
- Two small notes: the report's `not_computed` lists ten fields (`invariants`, `flags`, `creations_past_first_state`, …) whose absence I cannot judge; and the change to the offer window does not appear anywhere in the report (a guard's expression changed; nothing lost or newly enforced on live objects today), though it changes when every outstanding offer expires, and my reactor's copy of "7 days" (E19) is now wrong without anything telling me.

##### E21. `mirror-version.py` writes a go-live version the checker refuses
- **Trying to do**: step 10, the go-live version, as integrate.md §7 and publish-and-import.md §7 say: `scripts/mirror-version.py hiring.yaml Application > hiring-golive.yaml`.
- **What happened**: it made Candidate, Interview, Offer, Opening and Staff mirrors too (said on stderr; expected, since everything an application touches is joined to it) and dropped every transition but erasures, but kept `Opening.openings_posted`, a `transition_count` of `approve`: `check-flows.py hiring-golive.yaml` → `fatal names metric openings_posted counts 'approve', which is not a transition of Opening`.
- **What I did instead**: deleted that metric from the go-live version by hand.
- **What would have helped**: the tool dropping (or reporting) metrics that count a dropped transition, and running the checker on its own output.
- **Severity**: minor (obvious message, small fix), but it is the documented happy path of go-live.

##### E22. Go-live turns my whole model into mirrors, and the store must be empty of owned data first
- **What happened**: an application references its opening, its candidate, staff (hiring manager, referrer, interviewer), and owns interviews and offers, so every type with business data becomes a mirror; Staff too, which means during the go-live window no user can be onboarded by `onboard` (only imported). And since only a mirror is imported, the import must happen on a store whose version 1 is the go-live version: a store already running the real flow (my steps 1-9) cannot take a late batch of legacy applications (checked below). publish-and-import.md §7 says "the go-live shapes the model"; it does, completely, for a model like this one.
- **Severity**: major for planning (a staged cutover by type, which publish-and-import.md §7 describes, is impossible for an ATS: everything is one strongly connected stage), though a big-bang go-live works (below).

##### W9 (worked well). Step 10: go-live of 500 applications from a CSV
- legacy_ats_export.csv (501 rows: 25 requisitions, 450 candidates, interviews and offers denormalised, times without a zone, dirty rows planted) → app/legacy_import.py (a 120-line mapping) → one batch of 1446 objects (28 staff, 25 openings, 450 candidates, 501 applications, 369 interviews, 73 offers) with each application's current stage `entered` and its earlier stages as `legacy_intervals`, and the two sequences raised.
- On a fresh store: go-live version published; dry run 1.6 s; it found every planted problem as a class: `Application.state: is not declared` (2 "On Hold"), `Opening.hiring_manager_provided: is absent` (REQ-105), `Interview.not_interviewed_by_referrer`, `Interview.score_in_range` (a 5 on the old 1-5 scale), `Application.one_live_application_per_opening` (the duplicate). Decisions written to golive-disposition.json with who and why; the import wrote 1440 objects in 4.9 s in one transaction; the cutover publish's report listed what the objects gain (`gained:Application.advance_to_phone_screen` 221 …). Afterwards: `lookup legacy Application:ATS-00007` finds it, 498 open applications by stage, the admitted interview listed by `exceptions`, the next opening `OPN-0125` and application `APP-00501` follow the legacy numbers, an imported offer sent in July expires at once, and a late batch is refused `refused_types: ["Application"]`. Transcript: transcripts/step10_golive.md.

##### E23. Re-sending the same import batch is refused
- **Trying to do**: resume a go-live import after (say) a timeout, as publish-and-import.md §4 says: "a resumed run sends the batch again", and a refresh that changes nothing writes nothing.
- **What happened**: the identical batch and disposition sent a second time → `written: 0`, `refusal: {"type":"Application","legacy_key":"ATS-00001","reason":"an intermediate value of state entered before the latest interval the store holds"}`. Nothing is double-written (good), but the second run fails, so an operator who resends after a timeout sees a refusal and must work out that the first run succeeded.
- **What I did instead**: check with `lookup` whether the first run landed before resending.
- **What would have helped**: an identical batch replaying as a no-op (as an idempotency key does for a request), or an import idempotency key; at least the doc stating that a batch with `legacy_intervals` cannot be resent.
- **Severity**: major (go-live runbook; contradicts the design doc's resume promise).

##### E24. A disposition decides a whole class, and excluding uncovers new classes one round at a time
- **What happened**: the duplicate-application class held both applications (`sample: ["ATS-00041","ATS-00999"]`); `exclude` would have dropped the genuine one too, so I cleaned it upstream (dropped ATS-00999 in the mapping). Excluding the two On Hold applications then produced new classes (`Interview.application: names an excluded object`, `Offer.application: …`) visible only in the next dry run: three dry-run rounds to converge (my importer loops until no class is undecided).
- **What would have helped**: the dry run reporting the consequences of an `exclude` in the same round, and a per-object exclude list.
- **Severity**: minor.

##### E25. A DeclarationChange's `report` attribute comes back as a JSON string inside the object
- `get declarationchange-…` → `attributes.report` is a string holding JSON, while `publish` answers `report` as an object. **Severity**: minor.

##### W10 (worked well). Step 11: metrics with nothing instrumented
- Every brief metric came from a declared metric or a standard one, on both stores: applications by source (`keep: [source]`, `over: "30 days"`), time to hire by opening, median time in each stage, applications reaching each stage by opening (shares computed by the app: on the live store 304 → 151 → 91 → 5 → 1 hired), offers by outcome, interviewer load and feedback turnaround (marked in `person_dimensions`), refusals by clause and actor kind (the agent's `taken_by` refusal shows as `actor_kind: agent`), work in progress. On the go-live store the imported legacy intervals feed time-in-stage ("SCREENING 11 days") and load by month back to July. `bind` narrows to one opening. Transcripts: step11_metrics_live.md, step11_metrics_golive.md.

##### E26. The per-assignee standard metrics are named `<Type>.<metric>.<reference>`, which no guide says
- **What happened**: `Interview.open_work` → `refused: unknown_metric`; declaration-syntax.md §6.11 calls it `open_work`, and `person_dimensions` lists `Interview.open_work.interviewer.assignee`. By trial: `Interview.open_work.interviewer` works ("open interviews per interviewer", the brief's load metric for free).
- **Severity**: minor.

##### E27. Durations come back in whatever unit "reads best"
- **What happened**: one metric's rows: `"11 days"`, `"132 h"`, `"1972150 s"`, `"4744150 s"`. wire.md §2 says so and says to parse. A chart or a CSV export must normalise every value; "1972150 s" reads best to nobody.
- **What would have helped**: an option for a fixed unit (or seconds as a number).
- **Severity**: minor.

##### E28. A read names an actor the store does not know and is answered anyway
- A metric naming people (`Interview.interviewer_load`) read as `nobody@nowhere` → 7 rows. Only requests check `actor_known`. Harmless given that the application decides who reads, but the `actor` on reads then records nothing checkable. **Severity**: minor.

##### W11 (worked well). Step 12: the agent screens through generated tools; a person decides
- `tools` for `actor_kind: agent` gave 42 JSON-Schema tools rendered from the flow; `application_advance_to_phone_screen` and `application_reject` come "for proposing only" (`propose: {"const": true}`, "a person approves the proposal") because they are `taken_by: [human]` + `proposable`. My host filters them by an allowlist (7 tools incl. my own `read_cv`, since the engine holds only a CV's hash), validates every call against its `input_schema` with jsonschema (a recommendation `MAYBE` is caught before the wire), and maps `<type>_<transition>` back to a request in the agent's name.
- A deterministic stand-in for the model (no model API here) screened 12 applications against each opening's must-haves using only the tools: `record_screening` with a rationale (recorded as `actor_kind: agent`), and 5 proposals (3 advance, 2 reject). The recruiter listed `Proposal` objects in `PENDING`, approved 5 and overruled one rejection; each approval took the transition as the recruiter (`actor_kind: human`), the target's event `cause` pointing at the approval, the proposal keeping `proposer: agent.screener-1`. `Application.transition_counts` by `actor_kind` shows record_screening by agents and every advance/reject by humans. Transcript: transcripts/step12_agent.md; every tool call: agent-tool-log.json.

##### E29. `query_objects` (the agent's tool) hands back a cursor it cannot send
- **What happened**: the tool's schema has `type`, `filter`, `order`, `fields` only (`additionalProperties: false`); a call with `limit` fails validation. Its answer over 243 open applications carries 100 items and `cursor: "aa3ebaed60f6.application-373"`, which the agent has no way to pass back. An agent can see only the first 100 of anything.
- **What I did instead**: worklist filters that drain (screened applications leave the filter), so the agent never needs page 2.
- **Severity**: minor here (major for an agent asked to report over all records).

##### E30. Agents are allowed everything the flow does not forbid
- **What happened**: of the flow's 39 transition tools the agent was offered 35 that my application must forbid: `application_withdraw`, `candidate_forget`, `offer_send`, `offer_accept`, `staff_onboard`, `staff_set_hr_director` (the HR-director flag!) … integrate.md §6 says this plainly ("Nothing an agent may take is forbidden unless the flow says so … keep an allowlist"). For a product whose pitch is "agent-safe", default-allow means safety depends on every flow author remembering `taken_by` on every sensitive transition, or on the application's allowlist.
- **What would have helped**: a module-level default (`agents: propose_only` / deny unless listed), or `tools` taking an allowlist.
- **Severity**: major.

##### E31. The retired agent keeps acting (confirms E6 for agents)
- `ScreeningAgent.retire` → state RETIRED; the agent's next `record_screening` → `satisfied`. Revoking an agent in the store does not stop it. **Severity**: major (see E6).

##### E32. Proposals are not de-duplicated, and `Proposal.inputs` is a JSON string
- The agent filed a second pending `advance_to_phone_screen` proposal for an application it had recommended HOLD (allowed: proposals are the agent's, recommendations its own record); nothing prevents many pending proposals for the same target and transition, so a reviewer's queue can fill with duplicates. `attributes.inputs` comes back as `"{}"`, a string holding JSON (as E25). **Severity**: minor.

##### E33. The README's status and diagram disagree with the guides
- README: "not yet for production: its writes run one at a time on one SQLite file, and it does not page reads"; integrate.md §5 and my step 5: reads page (ADR-0179). The README's diagram draws the service over PostgreSQL; the core runs SQLite only, PostgreSQL "not yet built" (integrate.md §1). A reader deciding from the README alone gets two wrong facts. **Severity**: minor.

##### E34. Operations: no online backup, restore, monitoring or auth guidance
- The only backup instruction is for upgrades: stop the service and copy the file (integrate.md §2). Nothing on backing up a running store, point-in-time restore, health beyond `GET /health`, metrics/logs to watch, or what to do when the disk fills. The service authenticates nobody (core/README.md) and must sit behind the application on loopback, which is fine, but every operator must design that gateway. **Severity**: major for production (fine for a pilot).

##### W12 (worked well). Production probes (app/step13_production_probes.py, transcripts/step13_*.md)
- A counter-offer above band clears both approvals (`counter` → DRAFTED, `hm_approved_by`/`hrd_approved_by` null); `send` is refused `approved, delegable` until the hiring manager and then the HR director approve again.
- `kill -9` of the deployment's service during a stream of creations (17 acknowledged in 1.5 s): after restart none of the 17 was lost, the store opened at its version, and the request in flight at the kill, resent under its idempotency key, applied once.
- A second service process on the same store file by mistake: both served 15 concurrent creations each, all satisfied (the SQLite lock and the busy timeout of ADR-0178 hold).
- `POST /v1/upgrade` dry run on a current store answers `upgraded: false, steps: []`.

##### E35. Erased personal data stays in the store's files
- **Trying to do**: a candidate's "forget me" (brief §8; Singapore's PDPA in practice).
- **What happened**: on a deployment store, `register` "Zebulon Quaxworth", `reject` with a personal note, `forget`. `get` shows name and email null and the event's note redacted, but after stopping the service the `-wal` file still held the name 6 times and the note once; after a checkpoint (on a copy) the main file still held the email in the free space of page 775 of index `t_candidate_email_key_ix` (`integrity_check: ok`, no live row or index entry matches: these are deleted bytes not overwritten). So erased data is recoverable from the file, and from every backup copy of it. Nothing in the docs says how erasure relates to the bytes on disk.
- **What would have helped**: `secure_delete` on the engine's connection and a checkpoint after an erasure, or a documented statement plus a recipe (encryption at rest, backup expiry, `VACUUM`).
- **Severity**: major for production privacy compliance.

##### E36. `maintain`'s task shape is undocumented, and `archive_events` is a 501
- wire.md: "`prune_attempts`, `prune_idempotency` or `archive_events` before a time". The shape is `{"task": {"name": "prune_attempts", "before": "<timestamp>"}}` (found from the 400's text after three tries); a prune inside the retention answers `refused: inside_retention`; `archive_events` → `501 not run: archiving events, which a single SQLite file has no tier for`. So the event log of a deployment's store only grows. For this brief's volume (4 MB for 1,500 imported objects) that is years of headroom. **Severity**: minor.


---

#### What I built and ran

All under `r3-integrator/`. Every request and answer of every step is in `transcripts/<step>.jsonl` (2,388 exchanges, the bulk seeding and import bodies included), with a readable `transcripts/<step>.md` beside each.

| Step | Files | Result |
|---|---|---|
| Flow | `flow/hiring.yaml` (7 types: Staff, ScreeningAgent, AppService, Opening, Candidate, Application, Interview, Offer), `flow/hiring.examples.yaml` (4 examples), `flow/hiring-v2.yaml`, `flow/hiring-golive.yaml` | 0 fatal; examples pass on the reference runner and on the core |
| 1 deploy + publish | `app/env.py`, `app/step01_publish.py` | worked: `--store` SQLite deployment, evaluator endpoint, DeclarationChange draft → submit → publish |
| 2 actors | `app/step02_actors.py` | worked (staff, agent, services mirrored idempotently); E5, E6 |
| 3 create/edit/concurrency | `app/step03_edit.py` | worked: idempotent retry, 409 on key reuse, `stale` on version, 8-thread race with no lost update; E7 |
| 4 refusals | `app/step04_refusals.py`, `app/ofclient.py` (`explain`), `refusals.json` | worked: 20 cases across every verdict kind and remedy; E8-E11 |
| 5 worklists | `app/step05_worklists.py` | worked: 300 seeded applications, 10 worklists, paging; E12-E14 |
| 6 buttons/forms | `app/step06_actions.py`, `actions.html` | worked; E15-E17 |
| 7 background check | `app/bgcheck_provider.py`, `app/step07_bgcheck.py`, `app/step07b_bgcheck_faults.py` | worked, including 500 and timeout; E18 |
| 8 events | `app/reactor.py`, `app/step08_events.py` | worked on the deployment (subscriptions, automation) and, for timers, on a sandbox with a moved clock; E19, E20 |
| 9 version 2 | `app/step09_version2.py` | worked; the impact report matched my independent count |
| 10 go-live | `app/gen_legacy_csv.py`, `legacy_ats_export.csv`, `app/legacy_import.py`, `app/step10_golive.py`, `golive-batch.json`, `golive-disposition.json` | worked on a fresh store (1,440 objects in 4.9 s); E21-E25 |
| 11 metrics | `app/step11_metrics.py` | worked on both stores; E26-E28 |
| 12 agent | `app/step12_agent.py`, `agent-tool-log.json` | worked with a deterministic stand-in for the model (no model API available here); E29-E32 |
| extra | `app/step13_production_probes.py` | counter-offer, kill -9, two processes, upgrade, maintain, erasure on disk; E35, E36 |

Every rule of the brief is enforced by the engine from the flow, except: who holds which role (recruiter, HR, hiring manager as a person) and who may see person-naming metrics, which are the application's by design; the 180-day erasure *schedule* and the 7-day expiry *trigger*, which my reactor runs (the engine refuses either if early); and "pending" versus "adverse" background checks (E18).

#### Verdict

**Could my team build this hiring system on ObjectFlow?** Yes. Every one of the twelve steps worked against the real service, most at the first or second attempt, and the hard rules of the brief (HR approval before posting, one live application per opening, the agent recommending but never deciding, the 2-5 interview loop with feedback once and by the interviewer after the interview, no referrer interviewing, offers within/above band needing the HM / HM+HR director, re-approval on a counter-offer, the seven-day window, the background check before hire, never more hires than headcount with the last hire closing the opening, the 30-day reconsideration window, erasure with consent) live in one 650-line declaration that two implementations agree on. The refusal model (kind + remedy + the records in the way), `availability` for buttons, the publish impact report, the import's dry run and disposition file, the subscriptions, the evaluator endpoint and the generated agent tools are each better than what we would build ourselves. Building the application side took one person well under a day for a working skeleton.

**Could we run it in production?** Not yet, for a pilot only. What stands in the way, most serious first:

1. **The security boundary is entirely ours, and the store's own state does not back it up** (E6, E30, E31, E34). The service authenticates nobody; a user who has LEFT or an agent RETIRED in the store still acts; and an agent is offered every transition the flow does not mark `taken_by` (35 of 39 here, including withdraw, forget, send offer and set the HR-director flag). Workable with a gateway and an allowlist, which we would build anyway, but nothing in the engine catches a mistake in either.
2. **Erasure does not remove the bytes** (E35). A forgotten candidate's email and the rejection note survive in the WAL and in the free space of the store's pages, hence in every backup. Under Singapore's PDPA we need either the engine to scrub, or a documented position and a recipe (encryption at rest, backup expiry).
3. **Operations for one SQLite file are undocumented** (E34, E36, E12): no online backup or restore, no monitoring beyond `/health`, events never archived (`archive_events` is a 501), one writer at ~12 writes/s (fine for 600 staff; a mass action holds everyone for minutes). Durability itself held up (kill -9 lost nothing; a second process on the same file was safe).
4. **Every time rule is written twice** (E19, E20): the engine knows "seven days after sending" but cannot say when it becomes true; our scheduler carries a copy, which version 2 silently made wrong. The timer table, its persistence and its catch-up after downtime are ours to build.
5. **Go-live sharp edges** (E21-E24): the documented tool writes a go-live version the checker refuses; re-sending the same batch (the documented resume) is refused; the whole model becomes mirrors, so cutover is necessarily big-bang and must precede any live use.
6. **A refusal can point at the wrong record** (E8): ids of refused creations are reused by later objects of the same type.
7. Many small documentation gaps (E1, E3, E4, E7, E9-E11, E13-E17, E25-E29, E32, E33), each costing minutes, together a day.

**Is what the project promises about later releases enough to build on?** The shape is right and better than most young projects: ADR-0180 promises additive-only `/v1` (unknown fields ignored, unknown refusal kinds handled by their five remedies), the flow format stable within a major version with new checks arriving as notices first, and stores upgraded rather than stranded; and it is held by checks (`wire-v1.json` and a test, a frozen format-1 corpus, upgrade fixtures). My client was written to it (it ignores unknown fields; `explain` falls back on `remedy`). But it is not yet enough to commit a production system to:
- it is *Proposed*, and binds only from 1.0.0, which has no date; 0.2.0 keeps it "in intent";
- there is no support window: how long `/v1` is served after `/v2`, how long a 1.x line gets fixes, whether security fixes are back-ported;
- it promises the request keys (`wire-v1.json`), but answers are described in prose only; things our client depends on are not clearly inside it: the standard metric names (`Interview.open_work.interviewer`, E26), the duration formatting (E27), JSON-in-a-string attributes (E25, E32), the `<type>-<n>` id format and its reuse (E8), `not_computed` fields that may start carrying values, and today's 501 "not run" answers to valid inputs (E13, E36);
- the sandbox controls are explicitly not promised, yet they are the only way to test our time rules (E19) and our examples' evaluators; a team's CI depends on them;
- the go-live tooling (`mirror-version.py`) is a script outside the promise.

What we would still need to know before committing: the 1.0.0 date and what remains before it; the support and deprecation windows; whether the SQLite store can be upgraded to the PostgreSQL store the README's diagram shows, and when; and whether the engine will own timers or at least announce when a temporal guard next changes.

#### Findings by severity

**Major**
- E6/E31: actors whose state is LEFT/RETIRED still act; revoking in the store revokes nothing.
- E30: agents are offered every transition not marked `taken_by` (default-allow).
- E35: erased personal data remains in the WAL and in free page space of the store file.
- E34: no guidance for online backup, restore, monitoring; no authentication in the service.
- E19: time rules duplicated in the application's scheduler; no "ready at" from the engine.
- E23: re-sending the same import batch is refused (contradicts the documented resume).
- E22: an ATS model makes every business type a mirror; cutover must be big-bang, before live use.
- E8: refusals can name ids that are later given to different objects.

**Minor**
- E12: ~80 ms per write, ~12 writes/s single client, one writer.
- E21: `mirror-version.py` output refused by the checker (keeps a metric of a dropped transition).
- E1: the format card shows a `unique` form (`with` + `where`) the checker refuses.
- E3: before any publish, requests answer `unknown_transition`, `self_serviceable`.
- E4: how (or whether) a deployment's publish runs examples is undocumented.
- E5: duplicate refusals name the refused creation's would-be id.
- E7: idempotency keys are per actor, undocumented.
- E9: a `taken_by` refusal does not say `proposable`.
- E10: generated clauses (`*_taken_by`, `*_provided`, `*_unique`) have no description.
- E11: guards report one failure at a time; an unknown key hides other input errors.
- E13: no timestamp literal in filters; a string-vs-timestamp filter is a 501 instead of `malformed_filter`.
- E14: `order` grammar only in ADR-0127.
- E15: condition descriptions read as facts when shown as refusal reasons.
- E16: creating effects return the parent's id, not the created object's.
- E17: undocumented `corrects` input on the label offer.
- E18: evaluators are yes/no; "pending" and "adverse" look the same.
- E20: subscriptions have no backlog; first sweep undocumented.
- E24: dispositions decide a whole class; excludes uncover new classes one round at a time.
- E25/E32: `report` and `Proposal.inputs` returned as JSON strings; proposals not de-duplicated.
- E26: per-assignee standard metric names (`<Type>.<metric>.<ref>`) undocumented.
- E27: durations in mixed units (`"1972150 s"`).
- E28: reads accept unknown actors.
- E29: the agent's `query_objects` tool cannot page.
- E33: README status/diagram contradict the guides (paging; PostgreSQL).
- E36: `maintain` task shape undocumented; `archive_events` 501.

**Worked well**: W1-W12 above: the checker and its messages, examples on both implementations, refusals with remedies and the records in the way, idempotency and versions under real concurrency, worklists and paging, availability-driven forms, the evaluator endpoint, subscriptions, the publish impact report, the go-live dry run/disposition/import/cutover, declared and standard metrics including legacy history, generated agent tools with proposals, and crash durability.

