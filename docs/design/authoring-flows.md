# Authoring flows: where they are written, in what form, and how they are checked

Status: **discussion, open**, 2026-09-25. Nothing here is decided; question 3 is answered in part and on trial (§3). It records a design conversation with the author so that it survives across machines: what the author asked, what the design has today, the options with their trade-offs, a recommendation for each, and the questions only the author can answer. The PRD changes only once the author answers, and each accepted decision then becomes an ADR.

## What the author asked

In the author's words, in order:

1. "I'd like to make the way the states/transitions/... are specified clearly described and version controlled, it should be structured such that even manual editing is possible but should be clearly enough that an AI agent can be the default drafter".
2. "it should be possible to vadlidate [validate] the description automatically".
3. "there should be a way to validate if the specs provided by the user are semantically correct without logical errors (such as conflicting conditions)".

**Against the PRD.** None of this is a requirement yet:
- F1 says "a person or an agent may write a declaration", and that authorship clause is marked Inferred (from G1);
- F6, agents drafting changes to a published flow, is a Should and Inferred;
- nothing asks for the text to be version-controlled outside the engine, editable by hand, described in a way that can be validated, or checked for logical consistency.

An agent as the *default* drafter is stronger than F1 or F6 says. So these statements would become PRD requirements in the author's words, and F1's authorship clause would become Said. No wording is proposed here until the questions of §7 are answered.

## 1. What the design has today

Each item was checked against the document cited.

- **One text language.** A flow is written in `.of` files, one module per file, with `use` naming what it imports; a publish takes a module and the closure of its imports (`declaration-syntax.md` §1). The language is specified in `declaration-syntax.md`, 1,640 lines that carry the rules together with the history of twenty-five iterations.
- **No decision chose that form.** ADR-0010 decided that the declaration is data, inspectable at runtime. No ADR weighed a text language against a structured format such as YAML or JSON; the written form grew through the syntax iterations. The one mention of YAML in the record is ADR-0107's note that another project named `flowengine` is YAML-driven.
- **The store keeps every published version.** `of_declaration` holds each version's source text and parsed form, and is never deleted; every object and event records the declaration version in force (`storage-schema.md` §6, DESIGN §5.9).
- **A flow changes only through a `DeclarationChange`**: drafted, submitted with an impact report, and published by an approver who is not the drafter, a person where an agent drafted it. A change drafted against anything but the installed version is refused (DESIGN §9, ADR-0085, ADR-0097). The change holds the proposed text whole, in one column (`t_declaration_change.source`).
- **Nothing links a published version to a source revision** outside the store, and nothing says where a draft is written, reviewed as a diff, branched or blamed.
- **Hand-written version numbers.** Every enum, sequence, evaluator, machine, type, observation kind and metric carries `version <n>` in the text. Check 22 refuses a version that did not advance while its content changed, and a dependent that did not advance with what it depends on. Since ADR-0077, what an object and an event record is the store's declaration version, which fixes every type's version at that instant (DESIGN §5.9).
- **Line continuation is by indentation** (§9.1, check 51).
- **No formatter.** Nothing in the record defines a canonical layout for the text.
- **No descriptions.** `#` begins a comment (§1), and check 22 counts a change of comments as no change of content. Nothing in the language carries a description of what a state or a transition means into the rule set or the agent tools. The rule set renders each guard from its expression, "mechanical and lossy on purpose" (`renderers.md` §2).
- **Findings already carry a location.** A publish `Finding` has a severity, a code, a message, `where` as file and line, and the check number (`publish-and-import.md` §2). It carries no suggested fix.
- **Checking a draft needs a change in the store.** `publish(dry_run)` takes the id of a `DeclarationChange` (`library-api.md` §6), so there is no way to check a text without first creating one.
- **The checks are structural, not semantic.** The sixty-four checks cover names that resolve, markings in the right places, cycles among derivations, cascades, supersession and base types (checks 3, 12, 26, 43), the state graph (check 15: a non-terminal state with no exit, a state nothing reaches) and the swap test (check 6). None asks whether a condition can be true. A search of the record for satisfiability, contradiction or a solver finds nothing.
- **What DESIGN §13 leaves open.** The engine "cannot guarantee that **the guards say what was meant**". Its mitigations are the printable rule set and the adversarial harness.

## 2. Where a flow is written

**The need.** A flow's text should be drafted, diffed, reviewed and blamed with ordinary tools, and every version the store installs should be traceable to the revision it came from.

**Options.**
- **(a) The store only.** Drafts are `DeclarationChange` objects and the store is the version history. One source of truth, and governed, but no branches, no review of a diff in the tools people use, and editing by hand means pulling text out through the API and pushing it back.
- **(b) Written in a repository, installed in the store.** Flows live as files in a git repository, where they are drafted, diffed and reviewed; an agent drafts by opening a pull request. The store stays the authority on what is *in force*: a publish still goes through a `DeclarationChange`, with its impact report and F7's approval, and records the commit it was drafted from. The existing refusal of a change not drafted against the installed version guards against a repository that has drifted from the store.
- **(c) The repository is the authority.** A merge to the main branch publishes. This moves F7's approval into pull-request review, which the engine cannot enforce: that the approver is not the drafter, and that a person approves what an agent drafted. It also puts the impact report, which reads live objects, outside the place that holds them.

**Recommendation: (b).** It fits ADR-0104 and ADR-0110: a repository and its checks are an upper layer, and the engine keeps governance. What the engine would need:
- the commit, or another source reference, recorded on the `DeclarationChange` and on the version it installs;
- a way to check a text without creating a change (§3).

(c) is rejected for the reasons above.

## 3. The written form

**Options.**
- **Keep the text language as the only written form.** It is the most readable, it diffs well, and its checks exist. Its risk is that it is a new language, of which a model has no prior experience, specified in a long document.
- **A structured format, such as YAML or JSON with a schema.** A model produces valid structure reliably against a schema. But guards, invariants, derivations and metrics would stay strings in the expression language, so the hardest part to write is unchanged; review reads worse; and the language's design and checks would be redone.
- **Two written forms**, the text and a JSON form, converted both ways. The parsed form already exists in `of_declaration`. Accepting both as input creates a round-trip obligation, comments and layout included, and raises which one the repository holds.

**Recommendation** *(overturned by the author's answer below)*. Keep the text as the only form anyone writes, and expose the parsed form read-only for tools. Then make the text easy to edit by hand and easy to draft for an agent:
- **a canonical formatter**, so the same change made by a person and by an agent gives the same diff;
- **version numbers computed at publish rather than written**, since they are the bookkeeping most likely to be got wrong by hand or by an agent. *Not yet verified*: what else reads the per-declaration numbers, beyond check 22 and ADR-0027, ADR-0056 and ADR-0077. That is the first thing to check before proposing it;
- **a compact language reference for drafters**, separate from the design history and held against the specification by the checker;
- **an offline check**: the checks a publish runs that need no live data, run from a library or a command line with no store, returning the existing `Finding` shape with a suggested fix added. A refusal while drafting is then an instruction, as a refusal at runtime is (PRD F4);
- whether continuation by indentation (check 51) stays, or a formatter makes it moot, is a question for the language reference.

Whether an agent drafts this language well is empirical. No measurement exists, and none is assumed here (§6).

**The author's answer, 2026-09-25.** "I want the specifications to be more structured so that I don't have to try to understand by reading it line by line"; then, of a YAML form with fixed sections, "yes, try YAML on the service flow first". This overturns the recommendation above: the author weighs a structure a reader can scan above the text's compactness.

**The trial**, in [`yaml-trial/`](yaml-trial/), writes three modules as YAML: `people`; `inventory`, one robot from being added to stock until it is retired; and `service`, which adds records and an assignee. The walkthrough uses `inventory` at the author's request. It was first cut to available, sold and development, then judged over-simplified and extended: every unit leaves stock through a reservation, a sale can be reopened or the unit returned, and a unit can come back from development. The two moves back into `RESERVED` are the author's, since production sends both to `AVAILABLE`, and the author confirmed their meanings on 2026-09-25: a reopened sale keeps the unit for the same buyer, and a unit in development can be reserved for a customer. The return to stock, production's `accept_return`, was added at the same time. The author then set the meaning of the two closed states: `SOLD` and `DEVELOPMENT` are both delivered, externally to a customer and internally for development, and in both the item is no longer free or available; the transitions into `SOLD` and `DEVELOPMENT` are therefore `deliver_externally` and `deliver_internally`, named as a pair at the author's request (production's `sell` and `deliver_internal`), and either delivery is undone by `cancel_delivery`, which reserves the item again for the same recipient. *Superseded meaning:* the move from `DEVELOPMENT` to `RESERVED` was first confirmed as reserving a unit in development for a customer, as `reserve` from a second source state; on 2026-09-26 the author made it the cancellation of the internal delivery, so `cancel_delivery` now has two source states and `reserve` one. Because a transition has one outcome whichever state it starts from, the party an item is reserved for or delivered to is now one attribute, `recipient`, which `RESERVED`, `SOLD` and `DEVELOPMENT` all require; it replaced `reserved_for` and `sold_to`, which only a per-source outcome could have kept apart. The author also asked that the transitions map closely to the lifecycle diagram: every arrow of the diagram is one pair of a transition's source and target states, named as the transition is, so a transition with two source states is two arrows and an action is a loop on each state it acts in.

**The shape, as the author set it on 2026-09-25.** The author chose one block per transition ("yes, go ahead with the new shape"), then asked that the wording be formal and unambiguous ("`says` should be `description`, `moves` to `transitions`, review all the wording, all must be unambiguous and sound professional") and that every name be defined before it is used. A module declares, in this order, its `module` name, its `imports`, its state `categories`, its `enumerations` and its `types`; each type declares, in this order:
- **`description`** and **`tracking`**, both required;
- **`attributes`**, each `{ type: … }` or `{ reference: … }`, with `optional`, `unique`, `actor_kind` or `assignee` written out where they apply;
- **`observations`**, recorded datapoints, which the conditions may read;
- **`states`**, each with a `category`, `terminal` where no transition leaves it, and the `required_attributes` that have a value whenever an object is in it;
- **`invariants`** beyond those, if any;
- **`conditions`**, each with a `description`, an `expression` in the expression language and a `remedy`;
- **`transitions`**, each with a `kind` (`creation`, `state_change` or `action`), `from` and `to` as the kind requires, a `description`, the `required_inputs` and `optional_inputs` it writes to the attributes of the same names, its `guards` in evaluation order, each `enforced`, `observed` or `flagged`, and its `outcome`, an ordered list of `copy` and `clear` steps;
- **`metrics`**, each a `median_time_in_state` of a state or a `transition_count` of a transition, with `group_by` and `flag_when`.

Nothing has a default, and an enumeration value is written qualified, as `Condition.DAMAGED`, as the text language requires (§9.2 of the syntax). The converter expands each form into the text language, as listed in the header of `scripts/check-yaml-trial.py`: a required input on an optional attribute generates a guard `<attribute>_provided`, a state's `required_attributes` generate its invariant `<state>_attributes_present`, and a `transition_count` becomes the transitions along that transition's states. The trial still assumes, as questions 4 and 6 note, that version numbers are computed.

**Two earlier shapes, rejected by the author the same day.**
- *The first* wrote each transition as one line of inline YAML with `from`, `to`, `accepts`, `requires`, `on_trial` and `then`, and every condition as a named rule, five of the eight only saying that an input was given ("the spec description still doesn't look as clear"). The arrow was buried among what the transition needed, outcomes were strings in a second syntax, invariants were written as logic where they meant "this state requires these attributes", and a metric written as a state test had to be corrected when a second transition left the same state.
- *The second* fixed the structure with one block per transition whose first line was an arrow (`A, B -> C`, `stays in A, B`), but its words were informal or ambiguous: `says`, `moves`, `takes` and `may take`, `trial`, `copies` whose direction a reader had to infer, `holds`, `one of` with values written bare, keys containing spaces, and defaults for the remedy and the tracking mode. Its sections followed the diagram, so transitions named attributes declared below them.

**The specification**, [`flow-format.md`](flow-format.md) (draft 1, 2026-09-26), states the format normatively, marks which words are reserved and which the author names, and cites for each rule the step that enforces it; `scripts/check-flow-format-doc.py` holds it to the schema and the checker. `scripts/check-yaml-trial.py` checks a module in four steps, and reports every finding at its YAML line:
1. a strict loader, under which only `true` and `false` are booleans, since YAML 1.1 reads `NO` as false, and a key written twice in one mapping is an error, since a YAML loader otherwise keeps the last silently; a file YAML cannot parse is a finding at its line, not a crash;
2. the JSON Schema `flow.schema.json`, which an editor or an agent can validate against, and which holds each transition's kind to the `from` and `to` it requires;
3. what the schema cannot see: sections are in order, every state's category is declared, and a type another type references is declared before it or imported; every state, condition and attribute a transition names exists; every condition is used; every value an expression names is a state or an enumeration value; and every transition into a state sets each attribute the state requires, by requiring it as an input, copying it from an attribute every source state requires, or finding it required in every source state;
4. conversion to the text language, run through every implemented publish check.

All five modules pass. The self-test plants eleven mistakes and each is caught by its own step: a misspelt key, a guard on no declared condition, a guard listed twice, an enforcement that is not one of the three, a creation that names a source state, a condition that reads who is asking, attributes declared after the states that use them, a misspelt enumeration value, a transition into a state that does not set what the state requires, a `?` inside an inline mapping, and an enumeration value `NO`.

**What YAML itself constrains.** Inside an inline list or mapping, `?` marks a key and a comma separates items, so a type such as `decimal(10,3)` is quoted there, and optionality is the key `optional` rather than a `?` suffix.

**Found by the trial:** step 3's check on values closes, for YAML, a gap the text checker still has: check 19 requires an enumeration member to resolve and is enforced only in part, so `Condition.DAMAGD` passes it (`TODO.md`).

**Still open after the trial:**
- whether YAML becomes the written form of every flow, which replaces the text language's surface (its clause and indentation rules and the checks on them) and every example;
- whether every condition must carry a `description` (question 6 below); the generated guards and invariants carry none, and the rule set renders theirs from the expression;
- that version numbers are computed (question 4), which the trial assumes;
- whether step 3's conservative half should refuse: a transition whose source state does not require an attribute the target requires is refused even when the attribute happens to be kept, so the author declares it or the transition copies it;
- how a pair of types that reference each other is ordered, since defining every name before its use forbids the cycle; no trial module has one.

## 4. Descriptions that can be validated

The author asked that a description be validated automatically. Free prose cannot be validated deterministically, so a description is split into three kinds of content, each checked in the way it can be.

**Names it mentions.** A description that names attributes, states or observation kinds refers to them through placeholders, and every reference is resolved at publish, as check 19 resolves names in rules. A rename then breaks the description rather than leaving it wrong. This catches a stale description, not a wrong one.

**Claims about behaviour, written as examples the engine runs.** A claim such as "a job whose inspection failed cannot be finished until it is re-checked" becomes a small scenario: a setup, an actor, a request, and the expected outcome, a verdict with its clause and remedy class, or the state reached. The checker runs every example at publish against an empty in-memory store, so it tests the rules and not the live data; the run is deterministic, since the clock and the id source are injected (`library-api.md` §3). A shape, for discussion only, not a proposed syntax:

```
do finish WORKING -> DONE accepts photo {
  describe "The engineer closes the job once no inspection has failed."
  require none_failed: none(r in inspections where r.outcome == CheckOutcome.FAIL …) because dependent
  …
}
example a_failed_check_blocks_finish {
  given   a started job whose battery check failed and was not re-checked
  when    its engineer requests finish
  expect  refused by none_failed, dependent, naming the failed result
}
```

This is the first thing in the design that states what was meant and checks the guards against it, which is the risk DESIGN §13 leaves open. It also gives an agent a drafting loop: write the intent as examples, write the rules, run both. A later flow change that breaks an example fails on the change, as a broken test does.

**Rationale: why the rule exists.** This cannot be validated deterministically. A model could compare the rationale with the rule and flag a contradiction, but its verdict depends on the model and may differ between runs, which conflicts with the language's second goal, that everything promised to be checked is decidable from the text (`declaration-syntax.md`, preamble). So a model's review would be advisory, run outside the engine, for example as a comment on the pull request or the change; it would never block a publish, and the rule set would print the rationale marked as unverified.

**Recommendation.** A claim about behaviour must be an example; names are resolved; prose is rationale only, and labelled so. The printed rule set then shows three layers a reader can tell apart: the guard rendered from its expression, the examples verified at every publish, and the rationale, unverified.

## 5. Checking a flow for logical errors

**What does not count.** In most state machines, two transitions from one state with overlapping conditions are ambiguous. Here a request names the transition it wants (DESIGN §14), so overlap is harmless. The exception is two sweepable transitions from one state, which an upper-layer scheduler would have to choose between.

**What does.**
1. **A transition that can never fire**: its guards contradict one another, or the state it leaves.
2. **A state no object can validly be in**: its invariants contradict one another.
3. **A transition that always fails**: its outcome necessarily breaks an invariant of the state it enters. For example, `pay` clears `payment_ref` while an invariant requires `payment_ref` in PAID.
4. **A cascade that always aborts its parent**: the cascaded transition's guards cannot hold in the state the parent produces.
5. **A state that can be reached but not left**, once the satisfiability of guards is counted, not only whether an arrow exists (check 15 counts only the arrows).
6. **A guard that is always true**, likely a mistake, and a guard implied by the others, harmless but worth a notice.
7. **Objects waiting on each other**: a transition on A waits for B's state while B's waits for A's.

**Where the trial starts.** Finding 3 has a case that needs no solver, and the YAML trial checks it (§3, step 3): a state declares its `required_attributes`, and a transition into it that clears one, or sets it by no route, is refused.

**How.** The language keeps a flow's conditions small: paths of at most two hops, a mandatory bound on every loop and cascade (checks 21, 47), and no cycles among derivations or cascades (checks 3, 12). Most conditions therefore fall in a fragment a constraint solver decides exactly: arithmetic comparisons, enums, booleans and attribute paths. The analysis must model absence, since a guard over a missing value is unknown and refuses (`declaration-syntax.md` §8.2). What falls outside the fragment — a metric's value, an external evaluator's verdict, a count over every object of a type — is reported as **not decided**, never passed as consistent.

**Every finding carries a witness**, such as "with `amount = 750`, neither `approve` nor `reject` can be taken from SUBMITTED". A witness that a transition can fire is an example that can be generated, and a counterexample is a failing example, so this joins §4.

**The limit.** This finds a flow that contradicts itself. It cannot find one that is consistent and says the wrong thing; that is what the examples of §4 are for. DESIGN §13's open risk needs both.

**Recommendation.** Refuse a publish on a definite contradiction, findings 1 to 4; report the rest, 5 to 7 and anything not decided. Run the analysis when checking and publishing only, never on the request path, so it bears on neither request latency nor the choice of the core's language.

**A dependency to decide.** An off-the-shelf solver, such as Z3, is the obvious engine. It is a new external dependency, which needs the author's approval. The alternative is a solver written for this fragment: less to trust from outside, more to write and to prove.

## 6. How it would be tested

The reimbursement worked example already on `TODO.md` is the test: an agent drafts it from the language reference alone, and the result is judged by the design defects its findings and iterations reveal in the language, the formatter, the offline check, the examples and the analysis. No figure for how well an agent drafts is claimed until that is done.

## 7. Questions for the author

Answer any subset. The recommendations above are the defaults these would confirm or overturn.

1. **Version control.** Do you mean flows as files in a git repository, with the store recording which commit each version came from (§2, option b)? Or the store's own history?
2. **The default drafter.** Does it mean a person normally reviews and approves but rarely writes? If so, F1's authorship clause becomes yours, and F6 may move from Should to Must.
3. **The written form.** *Answered in part, 2026-09-25:* more structured, and YAML is on trial (§3), in the third shape the author set: one block per transition, formal and unambiguous keys, every name defined before its use. Whether YAML becomes the written form waits on the author's judgement of the trial.
4. **Version numbers.** Are you open to the language dropping hand-written version numbers, subject to the check in §3?
5. **Automatic validation.** Does it mean deterministic checks that can refuse a publish, with a model's review of prose only advisory (§4)?
6. **Claims as examples.** Must every claim about behaviour be an example, with prose limited to rationale?
7. **Example coverage.** Should every transition, or only some, carry at least one example, as a publish check or as a figure in the publish report?
8. **A broken example.** When a flow change breaks an existing example, is the publish refused until the example is updated in the same change, or is it reported like an impact?
9. **Logical errors.** Should a definite contradiction refuse the publish (§5)?
10. **The solver.** Is an off-the-shelf solver such as Z3 acceptable when checking, or should the fragment's solver be written here?
11. **Scope.** Should the first version include waits across objects (§5, finding 7), or start with contradictions within one type?

## 8. What an answer would change

For orientation only; nothing here is amended yet.
- **PRD:** F1's authorship clause, F6's priority, and new requirements for the written form, validated descriptions and logical checks.
- **ADRs:** the written form and the drafting home, with the options of §2 and §3 as their rejected alternatives; descriptions and examples; the logical analysis.
- **`declaration-syntax.md`:** descriptions, examples, computed versions if accepted, and the checks that validate them.
- **`publish-and-import.md` and `library-api.md`:** the source reference on a change, the offline check, and a suggested fix on a `Finding`.
- **`renderers.md`:** the three layers of a printed rule.
- **DESIGN §13:** the open risk narrowed by examples and the analysis.
