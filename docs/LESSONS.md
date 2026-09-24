# Lessons

Operational lessons from working on this project. See [`adr/`](adr/) for design decisions and [`../TODO.md`](../TODO.md) for open work.

### Design detail outran the stated goal

- **Pattern:** During initial ideation, design decisions were generated at a rate faster than the objective was established — actor models, delegation chains and proposal semantics were proposed before it was confirmed whether the project was internal tooling, a released library, or an open-source protocol. Those three imply different first versions, and the work rested on an unverified reading of the motivation. It took two explicit interventions ("I wonder if you're clear about what I want first") to surface it.
- **Correction:** In open-ended design, state the understood objective back explicitly before extending the model, and separate what was actually said from what was inferred. Structural decisions that follow from the stated model are safe to continue; decisions that depend on organisational context are not, and should be flagged as resting on assumptions rather than presented alongside the rest.
- **Context:** Early-stage design discussion, no code. Applies to any project where the framing is still being negotiated.
- **Recurrence, 2026-09-23:** at larger scale. Eighty decisions and two hundred findings were taken against a one-paragraph purpose, and the record never had a requirements document. When the author stated the fuller objective — a data-driven engine that records its own flow data, takes user datapoints and formulas, and helps flows converge — it reversed at least one explicit exclusion (analytics over the log) and conflicted with seven more positions (`PRD.md` §9). Each decision had been checked against the model and the case studies, and none against a stated requirement, because there was none to check against. Keep a PRD, and test each new decision against its use cases. The author made this a standing rule on 2026-09-23, on accepting ADR-0081 to ADR-0086: the PRD is the baseline, and a design choice is checked against it, never the reverse. `DESIGN.md` states it in its preamble.

### Idealization is not a slow start to implementation

- **Pattern:** Offering to scaffold a repository at the end of a conceptual discussion, when the request was to think about the idea.
- **Correction:** When told the work is at the idealization stage, treat that as the deliverable, not as a preamble. Capture decisions when asked, and otherwise state explicitly what is being held only in conversation so it is a visible risk rather than a silent one.
- **Context:** This project; applies generally to design-first work.

### The design record did not name its first consumer

- **Pattern:** DESIGN.md described its examples as a strawman and TODO.md listed domain questions as unanswered, while the real first consumer — a production system in a sibling repository — already answered several of them and contradicted others. A reviewer inferred the wrong first consumer from the strawman names and had to be corrected by the author.
- **Correction:** Name the first consumer in the design record, with the location of its repository, before listing any domain question as open, and check that repository for evidence before extending the model. A question whose answer sits in a repository the author already owns is not an open question.
- **Context:** This project; applies to any design whose first customer already exists as code.

### A boundary frozen on an atypical first consumer costs one deferral per later case

- **Pattern:** The expression language was fixed at "no arithmetic" (ADR-0021) because the first consumer serialises every unit and needed none. The next three case studies each needed arithmetic for a different reason, and each was deferred to the next before ADR-0032 admitted it.
- **Correction:** When the first consumer is known to be atypical along a dimension — here, unit-tracked rather than quantity-tracked — mark any boundary that rests on that dimension provisional, and decide it at the first structurally different case rather than after the third.
- **Context:** Design iteration against case studies; applies to any language or scope boundary set from one consumer's evidence.

### A consequence derived from the author's decision is not the author's decision

- **Pattern:** ADR-0018 records the author's decision that the store assigns object identity. Its consequences, written in the same session, added "ObjectKeeper offers no sequence primitive" — an inference, not something the author said — and the ticket case study withdrew it a few hours later (ADR-0029).
- **Correction:** In an ADR, keep the author's decision and the reviewer's derived consequences visibly separate, and mark derived consequences as provisional so a later withdrawal reads as a correction of the inference rather than a reversal of the author.
- **Context:** ADR writing when the decision is the author's and the consequences are drafted by an assistant.
- **Recurrence, 2026-09-23:** in a requirements document built to prevent exactly this. The first PRD labelled each requirement Said, Carried or Inferred, and four Said rows still carried an inference: a one-interface rule read out of "regardless of human or AI agents", a metric list read out of "metrics to evaluate the flows", a measure of "easily", and a porting clause nobody said. A fifth took the author's word "exceptions" in one sense when the author's own domain uses the other. The label was right at the level of the row and wrong inside it. Split a row until each part has one source, and check a word the author used against how the author's own documents use it before choosing its meaning.

### Scoping a fix to one syntactic form leaves the same defect everywhere else

- **Pattern:** Twice in one repair, a fix was applied to fenced declaration blocks and declared complete. The first time, five case studies were "re-expressed in the new grammar" and their mapping tables, guard tables, prose and "what held without change" sections kept the superseded notation and the superseded rules. The second time, a null-test fix corrected four lines in fenced blocks and left nine in tables and prose plus four in the ADRs, while its own ADR claimed a mechanical sweep had found no other violation.
- **Correction:** When a change invalidates a form of words, grep for the words across every file and every context, not for the construct in the shape you happen to be editing. A claim that a sweep was exhaustive must state what it swept; "every declaration block" is not "every occurrence".
- **Context:** Repairing a design record; applies to any rename or semantic change across prose and code alike.

### The sections most likely to go stale are the ones that summarise

- **Pattern:** After fifteen ADRs changed the model, the parts that still asserted the old model were the specification's structured summaries — its model block and glossary — and the case studies' "what held without change" and "still open" sections. The prose bodies had absorbed the repair. Two studies asserted that nothing changed while their own text three lines above described what had.
- **Correction:** After a substantial change, revisit the summarising artefacts first: glossaries, model blocks, status lines, "what held", "still open", and index tables. They are what a reader skims, they are written to be stable, and nothing in a diff points at them.
- **Context:** This project; applies to any document with a summary that is maintained separately from its body.

### An absolute claim in a design document is a defect waiting to be found

- **Pattern:** Three absolutes were written and two were false. "No state change bypassed the guards" was contradicted by the import path in the same document. "ObjectKeeper never touches the bytes" was contradicted by erasure. "The log is never pruned" survived only because erasure was carefully described as a rewrite rather than a prune.
- **Correction:** When a property has an exception, state the exception in the same sentence as the property. A guarantee with its exception named is weaker and true; without it the document is wrong and the exception is undesigned, which is how the assertion path came to exist for months with no declaration, no gate and no audit.
- **Context:** Design records making guarantees; the stronger the claim, the more it needs its carve-out written next to it.


### A specification's examples must be run against its own rules, mechanically

- **Pattern:** Three successive drafts of a declaration language shipped worked examples that violated checks the same document defined. Each time a reviewer found them by hand, and each time the next draft introduced fresh ones: a state reachable by nothing, an authority list naming the wrong parent, a transition reading an input it never declared, a required attribute no creation wrote. Hand review caught them, but only after a full review round, and never all of them.
- **Correction:** When a document defines rules and also contains examples, write the checker before the third draft and run it on every edit. `scripts/check-syntax-doc.py` parses the fenced examples and runs the mechanically decidable subset; it caught two real defects on its first run and three of four deliberately injected ones on its second, and the fourth exposed a bug in the checker rather than in the document. A checker that has never been shown to fail is not evidence.
- **Context:** Any specification carrying examples: a language, an API reference, a schema. The cost is an hour; the alternative is one review round per defect.

### A term the checks depend on but the grammar never defines

- **Pattern:** Check 17 of the declaration syntax rejected "a write whose target is not an attribute or **stored relationship end** of `this`". Nothing in sections 1 through 9 said which end of a relationship was the stored one. Every example declared both ends symmetrically, so the term the check rested on had no definition anywhere in the document, and nine iterations of review — including two that read the check list closely — did not notice, because each half reads correctly on its own.
- **Correction:** Extract the nouns a rules section uses and grep for each one's definition in the body. A term that appears only in the rules is either undefined or defined by the rule, and the second is almost always wrong: a check should test a property the language defines, not introduce it.
- **Recurrence, same day:** the fix above was applied to `stored`, the one term that had been found, and not to the class. The next review found eleven more of exactly the same shape, including one term used by a check and defined nowhere in the repository at all. Fixing the instance and not the class cost a full review round and thirty-four defects.
- **Context:** Any document with a normative body and a separate list of validations — a language spec, a lint ruleset, an API contract with a conformance suite. It generalises: when a defect is found by a mechanism, ask what else that mechanism would find, and run it, before declaring the defect closed.

### A rule and its check are two halves that drift apart

- **Pattern:** A specification stated its rules where a reader meets each construct and listed what publishing verifies in one section at the end. Four times across three iterations a rule was changed and its check was not, so the document recommended a construct the checker rejected. Each was found by a human reviewer one iteration late, and each fix was scoped to the instance. Twelve normative statements turned out to cite no check at all.
- **Correction:** Make the link mechanical and in the direction that fails loudly. Every sentence asserting that something is rejected must name the check enforcing it, and a checker rule fails the build when one does not. Reviewing for the link by attention is what had already been tried three times.
- **Context:** Any document pairing a normative body with a separate conformance list — a language spec, a lint ruleset, an API contract with a test suite. Related to "a term the checks depend on but the grammar never defines": both are the seam between the two halves, and both were first fixed one instance at a time.

### The newest mechanism is the least-checked thing in the document

- **Pattern:** Two instruments were added to stop a recurring defect — a rule that every normative statement cite the check enforcing it, and a swap test replacing a whitelist of invariant shapes. Both were right and both were wrong in their first version. The swap test, read literally, rejected the two shapes it claimed to accept. The citation rule used a proximity window that gave a live false pass, its detector recognised seven phrasings and missed seventeen statements in the very document that introduced it, and its stated claim to have caught the four defects motivating it turned out, when a reviewer ran it against them, to be one of four. Every one of those was found by a reviewer going straight at the newest thing.
- **Correction:** When a mechanism is added to prevent a class of error, test it against the instances it was built for and report the count, before claiming anything about it. Run it over the document that introduces it: if that document does not satisfy its own new rule, the rule is not yet real. Expect the repair for a defect class to contain a fresh instance of that class, because the repair is written under the same conditions that produced the original.
- **Context:** Any self-checking artefact — a spec with a conformance suite, a lint rule added after a bug, a test that asserts a policy. Related to "a rule and its check are two halves that drift apart".

### A renamed fragment needs a sweep with a saved command, not a grep at the time

- **Pattern:** One expression, `a.event`, was corrected to `a.at_event` three separate times over two days — first in the specification, then in the design document, then in two decision records and a case study. Each fix swept the file in front of it. The same shape recurred with a renamed concept, where replacing a word in one document left thirteen files using the old one, and with an outcome-grammar change whose superseded form survived inside the very decision record that introduced the grammar.
- **Correction:** When a name or a notation changes, write the sweep as a command that returns nothing and keep it — in the checker, in a script, or at minimum quoted in the commit message so the next person can rerun it. `scripts/check-corpus.py` is where this repository keeps them, alongside the cross-reference and amendment sweeps. A grep run once proves the file you were editing is clean and nothing else. Where the superseded form appears in a historical record that should not be rewritten, annotate it in place with what the current spelling is, which is what this repository does for decisions that were later amended.
- **Context:** Any corpus where one decision is described in several documents. Related to "scoping a fix to one syntactic form leaves the same defect everywhere else", of which this is the narrowest and most repeatable instance.
- **Recurrence, 2026-09-24:** the review against PRD revision 5 changed three rules stated in many places — "consumer" in three senses, "the attempt log holds no inputs", "a personal value may be counted" — and the repair corrected the sites the reviewers cited and grepped by eye for the rest. An independent verification then found twenty-two more sites across seven documents (D361, D368). The sweep that would have caught them is a search for the old phrase over the whole tree, read site by site, run before the repair is called done.

### A scripted edit keyed on a common pattern must assert where it matched

- **Pattern:** A script inserted a recommendation under each of thirteen numbered questions by matching `^<n>\. ` with a multiline regex. Four numbers matched an earlier list first: two design goals at the top of the document and, worse, the five-step name-resolution order that a publish check depends on. The document still parsed, every line was still valid, both checkers reported clean, and the corruption survived four commits until a reviewer reading the file noticed it. The `edit()` helper used everywhere else in this repository asserts the match count is exactly one; the script that did the damage did not.
- **Correction:** Assert where a scripted edit matched, or scope the search to the section first. A helper that refuses an ambiguous match is the cheap version and it already existed. Do not expect a post-hoc check to save you here: an inserted indented paragraph between two list items is structurally identical to a legitimate continuation, so nothing mechanical can tell them apart. What is checkable is *where a distinctive block is allowed to be*, which is what `scripts/check-corpus.py` now verifies for these.
- **Context:** Any scripted edit to prose. Related to "a renamed fragment needs a sweep with a saved command not a grep at the time"; both are about a change whose blast radius was wider than the place it was aimed at.

### Two documents describing one design need an owner per rule, written down

- **Pattern:** The model document and the language specification both stated about thirty-two rules. Six had drifted to the superseded version, including how invariant symmetry is decided, where a conditional may appear, whether a bound is optional, and whether a reference is an attribute type. It had happened before with the outcome grammar, which was restated in the model document and had gone stale in three of six lines. Each individual drift was cheap to fix and none was caught by either checker, because both statements were internally consistent and neither document knew the other existed.
- **Correction:** State the division of ownership in the documents themselves, at the top, where an editor will see it before adding a rule: this document says what a thing means, that one says how it is written, and when they disagree the one with a checker wins. Then delete rather than duplicate. A pointer that goes stale is a broken link, which is mechanical; a restatement that goes stale is a contradiction, which is not.
- **Context:** Any pair of documents describing one system at different levels — a model and a schema, an API guide and its reference, a protocol and its wire format.

### A verified list that names the DDL and not the behaviour

- **Pattern:** The storage schema stated what it verified — every SQLite DDL statement, a cascade round trip, a refused state value — and its one claim about runtime behaviour, that a sequence could be allocated on a second connection while the request's transaction was open, was reasoned from a mental model of SQLite and was false: SQLite has one writer per file, so the second connection blocks on the first as soon as the first has written. A ten-line two-connection probe found it (D187). The DDL executed cleanly because the DDL was never the claim.
- **Correction:** When a document lists what it verified, list what it did not, in the same paragraph. Any claim about locking, isolation, ordering under concurrency, or what a second connection can do gets a probe with two connections before it is written down as a decision, and the probe is kept beside the schema checker so the claim stays tested.
- **Recurrence, same day:** the same document decided against deferred constraints because "SQLite cannot defer to commit". It can — `DEFERRABLE INITIALLY DEFERRED` is honoured, and the same checker now runs it — and the decision built on the false premise could not have written a required reference at all (D191). Two claims about one database in one document, both reasoned, both wrong the same way.
- **Recurrence, 2026-09-23:** two more, this time in the concurrency story rather than the schema, and each in several documents at once. ADR-0039 and four documents after it said a row lock makes a contended row queue at serialisable isolation; on PostgreSQL 16.8 the waiter queues and is then aborted (D203). Three documents said SQLite "serialises writers already"; it serialises writes, and a deferred transaction that reads and then writes fails when another commits in between (D204). The correction above said to probe claims about locking and isolation before writing them down, and was applied to new claims only; the claims already in the record were never swept for it. When a lesson like this one is written, list the existing claims it covers and probe those too.
- **Context:** Storage and concurrency design; applies to any document whose "verified" paragraph could be read as covering claims it did not test.

### A new field on a record is checked against every path that reads the record

- **Pattern:** ADR-0088 added a read set to every event so that each decision could be re-evaluated (PRD L2), and checked it against erasure: it holds identities, not values, so erasure needs nothing from it. It was never checked against visibility. `history`, `pull` and `export` return an event under the visibility of the event's own object, so a reader who may see a delivery learned the ids of every object its guards read, including ones they may not see, which is what ADR-0030 refuses. The same day, a metric guard's value was specified to go "on the event" in the ADR and was carried into neither the schema nor the API. Both were found only when the traceability map asked, requirement by requirement, whether T5 and L2 were actually met (D216, D217).
- **Correction:** When a field is added to something the store records and returns, list every read path that returns it — each operation, each subscription, the export — and check the field against each cross-cutting rule in turn: visibility, erasure, personal-data taint. Then follow the field through to the schema column and the API shape in the same change, not in a later one. A requirement-by-requirement map against the PRD is what caught it, and it caught it because it forces the question for every requirement, including the ones nobody was working on.
- **Context:** Any design that records provenance or evidence alongside data: audit fields, read sets, cached values, as-of times.

### A fixture proves a check can fire, not that it catches the mistake

- **Pattern:** Checks 54 to 61 were each proven by a fixture, which is this repository's rule. A fixture is written by the same hand as the check, in the shape the check expects, so it shows that the check can fire and nothing more. A check whose pattern matched the fixture's spelling and missed the document's own would still have passed.
- **Correction:** After the fixture, mutate the specification's own examples one way per check — remove a mandatory clause, make a part singular, use an unlisted unit — and confirm that each mutation is caught by the check that claims it, and by nothing else. Keep the mutations in the checker so they run on every pass: `scripts/check-syntax-doc.py` applies fourteen of them to the specification since iteration 19, and a copy with check 59 disabled reports the one mutation it stopped catching.
- **Context:** Any checker validated by self-test fixtures, and any test suite whose tests were written alongside the code they test.

### Evidence cited in the record must be reproducible from the record

- **Pattern:** A latency probe for PRD C5 was run early in the session from a scratch file, and its numbers were carried forward in conversation as "evidence gathered". By the time the traceability map needed to cite it, the numbers existed in no committed document and the script in no committed path, so the claim could not be checked by anyone else.
- **Correction:** A measurement becomes evidence when the command that produced it is committed and the document citing it names the command, the conditions and the limits. Until then it is a note, and should be called one.
- **Context:** Any measurement taken during design: latency, rates, volumes, probes of database behaviour.

### A checker that verifies citations is not a verification of coverage

- **Pattern:** The traceability map reached "every requirement covered" with its checker passing: every requirement present, every cited section and decision existing. An independent reviewer then read each cited passage against each clause of the requirement it claimed, and tried to write the PRD's use cases in the grammar, and found twelve defects (D218 to D229): an observation's personal fields that could not be erased, a correction that could hide another job's results, a metric reading objects its reader could not see, a refusal that did not record what it was decided on, and more. Every one was in a row marked covered, and several were in constructs spelled the same hour.
- **Correction:** Treat a mechanical coverage check as a structure check. Before claiming coverage, have someone who did not write the design read the cited text against the full wording of each requirement — every clause, not the gist — and try to write each use case in the grammar, since a use case written out fails where a requirement read in summary passes. Then tighten the checker with whatever part of the reviewer's reasoning is mechanical: here, that a covered row must cite a document that owns an implementation.
- **Context:** Any requirements traceability, coverage matrix or compliance checklist whose checker can only see that a reference exists.
- **Recurrence, the same session:** a repair was itself checked against only the finding it answered. The second draft of ADR-0096 met C2 by withholding any metric group a reader could not wholly see, and the third review pass found it broke M2 — a reader seeing 99 of 100 objects got no standard metric at all — and leaked the dimension values of the groups it withheld. The correction is the same one, applied one level down: a repair to one requirement is checked against every requirement that touches the same mechanism (here C2, M2, T5 and UC-11), before it is written, not after.
- **Recurrence, the same day, and what finally found the rest:** after the single reviewer's three passes converged, five fresh reviewers, each given one slice — two groups of requirements, trust and the first consumer, the decision record, and the documents' agreement with each other — found twenty-five more defects (D236 to D260), most of them in things the earlier passes had marked sound. The most productive instruction was the same as before: *write the use case in the grammar and the API*; standard metrics that were only phrases, a rate that needed two datasets, an assignee that could never be removed, and a publish that wrote live rows without events all surfaced that way. One reviewer's repeated passes converge on its own blind spots; independent readers with distinct slices do not share them.

### Moving a transition behind `only via` strips the markings a request supplies

- **Pattern:** Writing the unit's journey from production, `receive` became `only via Shipment.receive_unit`, because production reaches it only through the shipment. Check 58 correctly forbids `backdatable` on an `only via` transition, so the marking the specification's unit carried was dropped, and nothing moved it to the parent. The module passed every check and broke UC-6: a receipt recorded the morning after would end the unit's time in `PROCUREMENT` when it was recorded (D282). The review against the PRD found it, not the checker.
- **Correction:** When a transition moves behind `only via`, move every request-level marking it carried — `backdatable`, `proposable` — to the transitions that are now requested, and re-read each use case that cites the transition before calling the restructure done. A checker validates the declaration's form; only the use cases say what the declaration was for.
- **Context:** Declaration modelling in this repository, and any refactor that routes a public operation through a new entry point: the entry point inherits the callers' obligations, not only their calls.

### A clause that says what does not count never gets a fixture

- **Pattern:** Check 15 says an `act` "does not count as outgoing" and an `assert` target "does not count as reached". The checker counted both, so a live state with only an action passed (D283). Every positive clause of the check had a fixture; the two negative ones had none, because a fixture is written to make a check fire, and these clauses exist to make it fire more often than a naive implementation would. It was found only when an exemption was being added to the same check and its code had to be read line by line against its text.
- **Correction:** For every "does not count", "is not", "except" in a check's text, add a fixture in which that exclusion is the only thing standing between the declaration and a finding. Whenever a check's clause is edited, read the implementation's sets against every sentence of the row, not only the one being changed.
- **Context:** Any checker or linter whose rules carry exclusions. Related to "a fixture proves a check can fire, not that it catches the mistake": the gap there was spelling, and here it is the negative space of the rule.

### Reading a claim's cited lines confirms the citation, not the claim

- **Pattern:** An audit agent reported that production shows its audit trail only to administrators, citing the permission that gates the `/audit-logs` routes. The lines said what the agent said, so the gap was recorded as "verified" and a rule was designed to close it: withhold who acted from every reader without `AUDIT_VIEW`. Production shows each record's activity, with usernames, to anyone who may view the record; the permission gates only the cross-record trail. The rule was stricter than the system it claimed to reproduce, and it broke the assignment metrics for ordinary readers (D285). An independent verification found it by reading the other endpoints.
- **Correction:** To verify a claim about what a system does, look for the counter-example, not the cited line: search for every other path that reads the same data before recording what "only" or "every" means. A cited line proves the citation exists; the claim needs the negative search.
- **Context:** Any audit or review that turns a sub-agent's or a reader's finding into a design decision, and above all a finding phrased with "only", "never" or "every".

### Before narrowing or widening a semantic rule, find what relied on the old one

- **Pattern:** To match one production rule — a slot with no role never blocks completion — ADR-0103's first draft made an element whose filter is unknown drop out of every aggregate. D20 had been closed on 2026-09-08 by making a guard over an erased value fail closed, and the change reopened it inside guards: `none(a in approvals where a.approver == actor.id)` would pass for an erased approver (D284). The production rule never needed it; a presence test expresses it.
- **Correction:** Before changing what an expression means, search the register and the decisions for every resolution that cites the rule being changed, and restate the change so that each still holds. Prefer expressing the new case in the existing language, and change the semantics only where that cannot be done.
- **Context:** Expression semantics, type rules and any shared definition in this repository, where a resolution is often a property of the semantics rather than a line anyone can point to.

### A behaviour promised in prose across documents needs its mechanism in the document that owns it

- **Pattern:** Four documents — `DESIGN.md` §11, `publish-and-import.md` §4, the cutover plan and ADR-0100 — promised that a metric over a gap in legacy history "counts it as unknown time and reports as incomplete". The document that defines what a metric returns, and the API shape it returns in, defined `complete` by visibility alone and had no notion of a gap. Every checker passed, since each sentence was consistent with its neighbours, and three independent reviewers found it only by reading the promise against the owning definition (D300).
- **Correction:** When a document promises that the engine reports, marks or says something, find the field or rule in the document that owns that output — the syntax, the schema or the API — and cite it. If none exists, the promise is either a design decision to take or a sentence to withdraw; do not let it stand in several places as though repetition made it true.
- **Context:** This repository's split between the model, the language, the schema and the API, and any design record where several documents describe one behaviour.

### A shell heredoc that is not quoted executes the backticks in the prose it carries

- **Pattern:** A Python script passed through an unquoted `<<EOF` heredoc inserted a provenance line containing `` `4109939` ``. Bash ran it as a command, printed "command not found", and the line was written with the commit missing. Markdown uses backticks everywhere, so any prose inserted this way is at risk.
- **Correction:** Always quote the heredoc delimiter (`<<'EOF'`) when the script carries document text, and pass values in through the environment or a file rather than by interpolation. After a scripted insert, grep for one distinctive fragment of what was meant to be written.
- **Context:** Scripted edits to the markdown of this repository.

### An example written to illustrate a decision must name something the record declares

- **Pattern:** Writing ADR-0105, an example named "a lease's `overrun`" as a time-driven transition, and a `DESIGN.md` edit gave `maintain` a report of pending deletions to return. Neither exists in the record: the first consumer's lease has a derived `overdue`, and `maintain` returns a count. Both were caught before commit, by re-reading the edit against the module and the API.
- **Correction:** Before an example names a transition, a field or a return value, find its declaration. An example that illustrates with an invented name is an unsourced claim, and a later reader will take it as a design fact.
- **Context:** Decision records and design prose in this repository, where examples are read as statements of what exists.

### A worked example is judged by what it reveals about the design, not by how faithfully it models the domain

- **Pattern:** Reviewing the design against PRD revision 5, part of the effort on the worked example went into making its transitions match production: a role check on the engineer-of-record, a serial rule on opening stock, a guard on a user leaving, version numbers in the flow review's variant module. The author clarified, not for the first time, that the example exists to verify the engine can meet the needs, and that a particular transition's logic matters much less than whether the example reveals design defects, shows the design's strengths and makes it more complete before building.
- **Correction:** Choose and judge each case of the example by the engine mechanism or PRD requirement it stresses, and whether it found a defect or rules one out. Use production's rules as evidence of what the engine must be able to express, not as a specification the example must reproduce. When a domain slip turns up, fix it cheaply and ask what general design question it points at: the opening-stock case matters because a creation into a mid-lifecycle state can bypass the guards of the path it skips without counting as an override, not because of the serial.
- **Context:** `unit-journey.md`, `flow-review.md` and the worked-example page, and any example written to test a design rather than to ship.
