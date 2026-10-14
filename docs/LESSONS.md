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

- **Pattern:** ADR-0018 records the author's decision that the store assigns object identity. Its consequences, written in the same session, added "ObjectFlow offers no sequence primitive" — an inference, not something the author said — and the ticket case study withdrew it a few hours later (ADR-0029).
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

- **Pattern:** Three absolutes were written and two were false. "No state change bypassed the guards" was contradicted by the import path in the same document. "ObjectFlow never touches the bytes" was contradicted by erasure. "The log is never pruned" survived only because erasure was carefully described as a rewrite rather than a prune.
- **Correction:** When a property has an exception, state the exception in the same sentence as the property. A guarantee with its exception named is weaker and true; without it the document is wrong and the exception is undesigned, which is how the assertion path came to exist for months with no declaration, no gate and no audit.
- **Context:** Design records making guarantees; the stronger the claim, the more it needs its carve-out written next to it.


### A specification's examples must be run against its own rules, mechanically

- **Pattern:** Three successive drafts of a declaration language shipped worked examples that violated checks the same document defined. Each time a reviewer found them by hand, and each time the next draft introduced fresh ones: a state reachable by nothing, an authority list naming the wrong parent, a transition reading an input it never declared, a required attribute no creation wrote. Hand review caught them, but only after a full review round, and never all of them.
- **Correction:** When a document defines rules and also contains examples, write the checker before the third draft and run it on every edit. `scripts/check-syntax-doc.py` parses the fenced examples and runs the mechanically decidable subset; it caught two real defects on its first run and three of four deliberately injected ones on its second, and the fourth exposed a bug in the checker rather than in the document. A checker that has never been shown to fail is not evidence.
- **Context:** Any specification carrying examples: a language, an API reference, a schema. The cost is an hour; the alternative is one review round per defect.
- **Recurrence, 2026-09-24:** the check that a quoted publish report matches the declarations (ADR-0109) reported clean on its first run because it verified nothing: a non-greedy regex over the document paired each block's closing fence with the next block's opening one, so the quoted block was never read. Planting a wrong line in the quote exposed it; fences are now paired line by line. Planting a misspelt state in the same change found that check 19 ignored a bare state (D380).

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
- **Recurrence, 2026-09-24:** writing the worked example's cases into a checked module, a new declaration checked clean; probing it with planted mistakes then showed that the checker ignored `use`, never resolved a state literal, and read only the first line of a wrapped guard, so a misspelt state and a wrong metric binding both passed (D374 to D376). A clean result on new declarations is evidence only after a planted mistake in them is caught.
- **Recurrence, 2026-09-29:** the reference runner's metric self-test passed "a dimension bound to an absent value is unknown", and mutating the rule to read the absent group instead still passed it: the job with no engineer had no inspections, and later none inside the window, so both readings gave unknown. Each rule of `flowrun.py`'s metrics is now mutated once (half-up rounding, an exclusive window, the absent group, the upper middle, attributes read as they are now, the filter ignored, two places for an average, the working store read) and each mutation fails the self-test. A check of a value is evidence only when its fixture gives a different value under the rival reading.

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

### A rename follows the convention the old name followed

- **Pattern:** Renaming the project to ObjectFlow (ADR-0107), the GitHub repository was renamed to `objectflow`. The old repository, `object_keeper`, was the snake_case form of the old name, and the author had to correct the new one to `object_flow`.
- **Correction:** Before renaming anything, read how the old name was formed in each place it appears — repository, directory, package, file names — and form the new one the same way. A second rename of something outward-facing costs a redirect and a correction in the record.
- **Context:** Renames of a project, a module or a package, and any outward-facing identifier.


### A requirement's reading is checked against the language before it says what a mechanism enables

- **Pattern:** The first-consumer check said that conditions against observed norms would give L5, rules that read metrics, "its first real use". The language forbids a metric anywhere but a `require` clause (check 56), so a condition cannot read one; the claim held only until ADR-0113 was written, and was retracted in the document and its answer table.
- **Correction:** Before a finding says which requirement or mechanism a need exercises, find the rule in `declaration-syntax.md` that would carry it, and its check. Where the language forbids the direct route, say what the route is instead (here, a norm held as data, which is L4).
- **Context:** ObjectFlow design documents: findings, checks against a case study, and traceability notes.

### An inferred clause that narrows the author's own words is a question, not a requirement

- **Pattern:** The author asked on 2026-09-25 to "leave the auth & authz for upper layers to decide". PRD revision 8 moved authentication and role mapping upstream but kept "what a capability permits" in the engine, marked Inferred, from other requirements. The same day that clause produced ADR-0112's metric audiences, and a walkthrough then showed the author a role mapping they judged untrackable; ADR-0114 superseded ADR-0112 within hours and reversed the clause.
- **Correction:** Where the design infers a clause that keeps back part of what the author asked for, put it to the author as a question with its consequence, before writing it into a requirement and building on it. Marking it Inferred records the doubt but does not resolve it.
- **Context:** ObjectFlow's PRD and every decision that narrows or widens the author's stated boundary.


### A cutoff that judges saved input is read from the clock, not estimated

- **Pattern:** The walkthrough page counts a reviewer's verdict only if it was saved after `REVISED`, the time its content last changed. Twice on 2026-09-25 the cutoff was set to a round time estimated ahead of the publish (08:05Z published at 08:01Z, then 09:00Z published at about 08:15Z), so any verdict saved between the publish and the cutoff would have been shown as belonging to an earlier version and not counted. Neither discarded a mark, because none was saved in the window; the second was found only when the next cutoff was about to be estimated the same way.
- **Correction:** Take the cutoff from `date -u` just before the build, and check the saved verdicts before publishing, so a mark made against the version being replaced is known.
- **Context:** Published pages that compare stored timestamps against a revision time: the walkthrough's verdicts, and anything else that marks input stale.

### Publish only what a build that succeeded produced

- **Pattern:** A page's build and its publication were sent as one parallel batch on 2026-09-26. The build failed, on a recursive schema definition the vocabulary walker followed forever, and the publication went ahead with the previous build's file, so a version went out that did not contain the change it was published for.
- **Correction:** Run the build and its checks first, and publish in a later step, only after they succeed. Anything that consumes a build's output waits for the build's result.
- **Context:** The walkthrough and specification pages, and any artifact generated from repository files.

### A quote is checked against the page's raw text, not a fetcher's summary

- **Pattern:** On 2026-09-26 two things read through a fetch tool that summarises a page with a model came back wrong. ADR-0118 and `flow-format.md` Appendix A.4 quoted Cube's query format as listing "dimensions to group by", and the page says "An array of dimensions". A summary of Jira's approval-step page said the step "does not require a Premium edition", which the page does not say and Atlassian's overview contradicts. Both read as quotations, and the first was committed as one.
- **Correction:** Before a quotation goes into a document, fetch the page's raw text (`curl`, strip the markup) and find the quoted words in it; use the summarising fetch to locate a page, never as the source of a quote. A researcher asked for quotes is told the same, and its quotes are sampled against the raw text.
- **Context:** Any document that cites external sources: ADRs, the format's Appendix A, case studies.

### A sentence that cites a check is proven by a probe before it is committed

- **Pattern:** Twice on 2026-09-26 a specification sentence named the step and check that enforce a rule without a probe having shown it. In construct 4, `flow-format.md` §4.10 said a machine's conditions naming an unrequired attribute were refused at step 3, and a probe found no step refused them. In construct 13, §4.19 said step 4 refuses a cycle among supersession targets, and the internal checker's check 26 has no such clause; that one was committed and then corrected in place.
- **Correction:** For every "(step N, code)" a new or edited sentence carries, run a mutation that breaks the rule and see that code fire, or read the implementing code; where neither shows it, say the rule is unchecked instead of citing a check.
- **Context:** `flow-format.md` and any document that says which check enforces a rule.


### Prose inside an inline YAML mapping is quoted

- **Pattern:** Twice on 2026-09-26 and 2026-09-27, writing flow modules by hand, a description with a comma inside an inline mapping, `{ category: todo, description: Applied, waiting to be screened. }`, was cut by YAML into a description and a second key, and step 2 reported only "Additional properties are not allowed ('waiting to be screened.' was unexpected)". `flow-format.md` §2 rule 4 already required the quoting; the author of the rule broke it, in the orders study and then in three modules of the Jira catalogue.
- **Correction:** Quote every prose value written inside `{ … }`, a description above all, or write it in the block form where YAML reads it whole. The checker now names a key that could not be a name as a value cut at a comma, citing the rule (D394).
- **Context:** YAML flow descriptions, and any YAML whose inline mappings hold prose.

### A section number is read from the file, not recalled

- **Pattern:** On 2026-09-27, writing register entry D424, `declaration-syntax.md` §4.4 and `DESIGN.md` §5.2 were cited from memory for text that sits under §4.2 and §5.4; the same session, an ADR's rejected alternative said an aggregate over an absent set is unknown, which no document states, and earlier a catalogue note cited `case-study-payments.md` §5 for an idiom that section contradicts. The first two were caught before the commit only because the sections were then looked up.
- **Correction:** Find the heading a cited line sits under with a command, such as `awk 'NR<=N && /^#+ /{h=$0} NR==N{print h}' file`, before writing the `§`; and hold a rejected alternative's reasons to the same standard as a decision's, since a reader re-evaluating the decision will rely on them.
- **Context:** Any document that cites another by section, and ADRs' rejected alternatives.

### A claim that a document says something is found in it before it is written, and its absence is shown by a search that cannot hide it

- **Pattern:** On 2026-09-27 ADR-0124's draft said "every standard combined metric groups by five dimensions", and `refusal_rate` is the only standard combined metric; a search caught it before the commit. The same hour, `metric-scenarios.md` §33 said `User.leave` has no guard "as the journey's own prose says", and `grep … | cut -c1-300` over `unit-journey.md` showed no such prose, so the claim was judged invented and rewritten, committed, and reported to the author as a caught mistake. The prose was there: the sentence sits past character 300 of a paragraph written on one line, as every paragraph here is. The retraction was the error, and was undone on 2026-09-28.
- **Correction:** For every "as X says", "X declares" or "every Y", search X or the list of Y before writing the sentence, and narrow a generalisation to the cases it was checked against. Before concluding that a document does *not* say something, search with a command that cannot hide a match: `grep -o '<phrase>'` or `grep -c`, never output cut to a width, since one-line paragraphs put most text past any cut.
- **Context:** Findings sections, ADRs and register entries, which readers take as sourced, and any search in this repository's Markdown.
- **Recurred** 2026-09-28: ADR-0134's draft decided a reused key's remedy as though none existed, and ADR-0105 §6 had decided it; a reader of the decision records caught it before the commit. A claim that something is undecided is a claim about every decision record, and is searched for across `docs/adr/` first.
- **Recurred** 2026-10-09: a design paragraph's draft said the publish report had carried three fields "since 2026-10-08", from memory; `git log -S` over `publish-and-import.md` dated them 2026-09-25, 2026-10-08 and 2026-10-09, and the clause was dropped before the commit. When something entered the record is read from `git log -S`, never recalled.

### A changed rule is searched for by its old wording across the record, not only edited where the decision names it

- **Pattern:** On 2026-09-28 ADR-0133 widened check 60 from "a type taking part in supersession" to "every type with a personal attribute". Its consequences named the check tables, and those were edited, but `declaration-syntax.md` §6.4 still called an erasure optional, and `flow-format.md` §4.19 and `DESIGN.md` §8 still scoped the rule to supersession. The coherence review found it the same day (D462), two readers independently.
- **Correction:** When a decision changes a rule, search the whole record for the old rule's distinctive words (here "supersession" beside "erase") before committing, and fix or mark every hit. A decision's list of consequences is where the author of the decision thought the rule lived, not where it lives.
- **Context:** Any ADR that changes a rule stated in more than one document, especially `DESIGN.md`, the syntax, the format and their check tables.

### A whole-record review is split so that every document has a reader, and the split is checked against the file list

- **Pattern:** On 2026-09-28 the coherence review was planned with a slice for the decision records from ADR-0078 on, and the briefs as sent gave that slice's reader the case studies instead. No reader had the records, and a sweep after the fixes found twelve of ADR-0081 to ADR-0113 mentioning capabilities or visibility without ADR-0114.
- **Correction:** Before launching the readers, list every file the review covers and tick each off against the briefs; a document no brief names is a gap in the review, not a document found clean.
- **Context:** Reviews delegated in slices, in this repository and elsewhere.

### An exit status is captured before anything else runs

- **Pattern:** On 2026-09-28 a loop ran every check as `python3 $s > out; echo "$(basename $s) exit=$?"`. The command substitution runs `basename` before `$?` expands, so every script reported `exit=0`, and a corpus check with two findings read as passing. It was caught only because a reader ran the check on its own.
- **Correction:** Capture the status on the next statement, `python3 $s > out; rc=$?`, and read each output's last lines as well as its status.
- **Context:** Shell loops over the check scripts, and any loop that reports a status.
- **Recurrence, 2026-09-29:** the scenarios and corpus checks ran in the background, and the command that printed their statuses also ran `git commit`, so the commit (`c98dfd4`) was made before `scenarios rc=1` and `corpus rc=1` were read. The new notice had changed the notices a quoted publish report lists. A status is read, and a failure handled, before the step that depends on it runs, never chained after it in one command.

### A check that claims to read a language parses it

- **Pattern:** Until 2026-09-28 both checkers read expressions with regular expressions: enough to find the names an expression uses, and nothing more. `issues.yaml` wrote a set in brackets, which the grammar does not have, and passed both; the parser written for the contradiction checker refused it on its first run (D478).
- **Correction:** Where a check's claim depends on what an expression means, or on its being well formed, parse it with the grammar the specification states, and prove the parser both ways: on every expression the record holds, and on malformed ones it must refuse.
- **Context:** The flow and syntax checkers, and any tool that reads the expression language.

### A sentinel compared by identity survives copying only if it says so

- **Pattern:** On 2026-09-29 the reference runner marked absence with one object, `ABSENT`, tested with `is`, and worked on a deep copy of the store for each request. While absence was never stored the copy did not matter; once spans and events held it, `copy.deepcopy` made new instances, `x is ABSENT` became false on every copied row, and a subtraction reached the absent value. The self-test found it on its first run.
- **Correction:** A sentinel compared by identity defines `__copy__` and `__deepcopy__` to return itself (and `__reduce__` for pickling), and a test asserts `copy.deepcopy([S])[0] is S`.
- **Context:** Python tools here that copy state holding a sentinel: `scripts/flowrun.py`, and any runner or harness after it.


### A new check's output is read whole, since a notice may say a check did not run

- **Pattern:** Writing the inventory's examples, the flow checker's output was filtered to its fatal findings. The example of a duplicate serial, which named its invariant's remedy, had become `notrun`, a notice, since the runner could not yet infer the remedy of a uniqueness failing on another object. The filter hid it, so the remedy looked verified, and a planted wrong remedy was refused by nothing.
- **Correction:** When a check is new, read its notices as well as its findings: a notice saying something was not run means that nothing was verified. Where a notice would mean a check stopped proving what it proves, make it fatal in that run. The flow checker's corpus run now fails on an example it does not run (ADR-0140).
- **Context:** Any checker that reports "not run", "skipped" or "not checked" as a notice; the flow checker's `notrun` and `coverage` notices.

### Resolving a defect from the reference alone contradicted a worked scenario
- **Pattern:** On 2026-09-30, resolving D487 (whose attempt-log row a flag raised by a called transition is), I generalised the answer to every row, a refusal's included, from `DESIGN.md` and the flow format, without reading `metric-scenarios.md`, whose §12 had already determined the opposite for a refused call. A cold reader found the contradiction hours later.
- **Correction:** Before resolving a defect about what the engine records or counts, grep the worked scenarios (`metric-scenarios.md`, the examples files) for the construct: a scenario marked "determined" is a decision already taken, and a resolution must agree with it or say why it overturns it.
- **Context:** ObjectFlow design documents; any rule about attempts, events, intervals or metric rows.

### A commit message claimed a plant that did not exist
- **Pattern:** On 2026-10-01 two commits (`a405582`, `4398230`) said ADR-0146's "a range is an int" check was "proven by a plant". No plant existed: the only `range` in the flow checker's plants was a literal-conversion fixture. When the plant was finally written it failed. The check made no finding on a set-typed range, since `value_kind` leaves sets untold, and it accepted `2.5`, since every number literal is typed `number`. The claim was written from the plan, not from a run.
- **Correction:** Claim "proven by a plant" or "caught" only for a plant whose output line was read in this run. Write the plant in the same edit as the check, both ways (refused, and a near miss accepted), and grep the run's output for its line before the commit message names it. A check that relies on a kind function must be planted with each kind that function leaves untold.
- **Context:** ObjectFlow's checkers and their plants; any commit message or defect resolution that cites a proof.

### A self-test's case can pass or fail through a rule it was not written for
- **Pattern:** On 2026-10-04, writing self-tests for ADR-0147's runner rules, three cases tested something other than their name. A request dated three days back was refused by the interval check, since the order's own state began later, so removing the bound left the test passing. A planted object that collided with the new one came first in id order and named its own `code_unique`, so the invariant-order case failed whatever the order was. A proposal step lacked the operator's alias, so it was refused as `actor_known` before reaching the rule. Each was found only by reverting the rule and seeing which test failed, or failed to.
- **Correction:** Build a self-test's case so that only the rule under test can decide it: keep other objects out of its way (no planted object that itself breaks an invariant step 7 checks across the store), date it so that no other check refuses it, and give every alias a step uses. Then revert the rule on a copy and confirm that this test, and not another, fails.
- **Context:** `scripts/flowrun.py` self-tests and the flow checker's plants; any test of one rule in an engine that checks many in sequence.

### A test of the logic under a page passed while the page did not work
- **Pattern:** The Flow Studio's example player passed every example in its test, which drives it directly, and the page built on it refused the second step of the first example run in a browser. "Run to the end" looped inside one event handler and read React state from the render it started in, so every pass took the first step again; no test exercised the component, only the player beneath it.
- **Correction:** A test of the logic a page calls is not a test of the page. Before calling a page done, use it: open it in a browser, the cached Playwright Chromium driven over the DevTools protocol where no Chrome is installed, and take the steps a user would. Inside an async loop in a handler, keep progress in a ref, never in render state.
- **Context:** React, TypeScript; the Flow Studio's front end, 2026-10-04.

### `pkill -f` and `pgrep -f` matched the shell that ran them
- **Pattern:** Stopping the Flow Studio's servers with `pkill -f "python -m flowstudio"`, and later with `pgrep -f "...|python -m flowstudio$"` inside a kill loop, the pattern also matched the Bash command line that carried it, so the shell killed itself and the command ended with status 144; it happened twice in one session.
- **Correction:** Stop a background process by the id recorded when it was started (`cmd & echo $!`). Where a pattern is unavoidable, bracket one character, `pgrep -f "[v]ite/bin/vite.js"`, so the pattern cannot match its own command line.
- **Recurred 2026-10-05, past the bracket.** `ps … | grep "[c]heck-" | xargs kill`, run in the same command as `for c in scripts/check-*.py; …`, killed that command's own shell: the bracket keeps `grep` from matching itself, but the shell's command line held the text `check-` from the loop it was about to run. So a bracket is no defence when the command that kills is also the command that does the work. Kill by the ids already read, and start the work in a separate command once they are gone.
- **Recurred again 2026-10-05, the lesson in hand.** A smoke test of the core's service ended with `pkill -f "objectflow-service --port 18731"` in the command that started it, and killed that shell (status 144). Knowing the lesson did not stop it, since the kill was an afterthought appended to a working command. Start a server in its own command, keep its id (`echo $! > pidfile`), and stop it by that id in a separate command, every time, so no command ever carries a pattern that names what it runs.
- **Recurred 2026-10-13.** A probe of the label request started a service in the background, asked it two requests and ended with `kill $(pgrep -f "lbl/s.sqlite")` in the same command; the pattern matched that command's own shell (status 144) and left the service running, found afterwards by `ps` with a bracketed pattern and stopped by its pid. A pid looked up by a pattern is a `pkill` in two steps: the rule against name patterns covers the lookup, not only the kill.
- **Context:** Shell, any session that starts and stops servers or background checks.

### A differential test covers only what its corpus contains
- **Pattern:** The Rust core's parser was held to Python's over all 280 expressions of the design, and a parser made right-associative for `-` still passed: no expression in the design chains `-`, `/`, `or` or `and`, so associativity was never compared. Only reverting a rule on a copy showed it.
- **Correction:** Beside the real corpus, give a differential test a hand-written set that reaches each construct the grammar defines, the rules a corpus happens not to use among them: chains of each left-associative operator, nesting, every literal, and inputs that must fail. Then break one rule at a time and see the test fail.
- **Context:** Any oracle comparison over a corpus; the core's `tests/parse_like_python.rs`, 2026-10-04.

### "None fails" cannot see a regression when "not run" is allowed
- **Pattern:** The Rust core's first slice was judged by the replay of every example, none allowed to fail and any allowed to be not run. A core with its state check broken still passed. The one example the check decided carried on past the state and reached an effect the slice does not run, so it turned from passed to not run, which the criterion allowed.
- **Correction:** Where an outcome may honestly be "not run", hold what passes with a ratchet: record each example that passes, and fail when one stops passing. Then break a rule on a copy and see the comparison fail.
- **Context:** Differential testing of a partial implementation against an oracle; the core's replay, `apps/flow-studio/server/tests/core_passing.txt`, 2026-10-04.

### A lenient oracle hides its callers' breaches of the wire
- **Pattern:** The Flow Studio's two players sent a decimal from an example's YAML as a JSON number, where ADR-0150's wire carries it as a string. The Python reference runner accepted the number, and every example that sends one passed against it. The Rust core decodes the wire strictly and refused the input as `invalid_input`, which made the replay look like a fault in the core.
- **Correction:** When a second implementation of a contract refuses what the first accepted, check the caller against the contract before either implementation. Decode the wire strictly in the reference too, or test the callers against the contract directly, so a breach shows where it is made.
- **Context:** Any wire with more than one server behind it; the studio's `tests/replay.py` and `web/src/session/player.ts`, found 2026-10-05.

### Search the deferred questions before recording a gap as new
- **Pattern:** Building the core's import, I listed four gaps in the interface as new register entries. Three of them, two channels for an import's admissions, a class's identity and an owned type in a batch, were already the third cold reading's open questions C31 to C33, which ADR-0148 deferred to whoever built the import. Entered again, each question would have had two records and two answers.
- **Pattern (recurred 2026-10-06):** Building read sets, I put three questions to the author as open, the third being how `export` pages a row that changes after it is written. ADR-0100 §6 had decided it on 2026-09-23 and `library-api.md` §6 stated it, one line from where I added ADR-0158's restatement; the author approved the restatement, which also keyed attempts by an event position a refused request never writes, and decision 3 was withdrawn the same day.
- **Correction:** Before recording a gap, search the register's open entries and the questions they defer (D516's list, the cold readings' sections) for it. A gap already recorded is answered where it is, in the rule's home citing the record; only a gap no record holds becomes a new entry. Before putting a question to the author, grep `library-api.md`, `DESIGN.md` and `docs/adr/` for the operation's name and read every hit; a question the record answers is not the author's. Grep case-insensitively and for the concept's other names too (`-i`, and the class a reference would be: `EvidenceRef`, not only `evidence`).
- **Pattern (recurred again 2026-10-06):** ADR-0159 claimed no document gave a cited reference's form; `library-api.md` §5 had defined `EvidenceRef` since 2026-09-23. A case-sensitive `grep evidence` missed the class, and the ADR wrote a diagnostics reference with a field `EvidenceRef` lacks; it was corrected to the earlier record the same day.
- **Pattern (recurred a third time 2026-10-06):** Of three questions put to the author, one was answered by `DESIGN.md` §7 and §8 and another's premise, that no document gave the rows' shape, was false (`declaration-syntax.md` §6.9). I had read `storage-schema.md` §9 for the erasure and not `DESIGN.md` §8's fuller text, and searched library-api for a class where the shape lived in a table of the syntax; the author approved "my recommendations" on questions I had not recommended on.
- **Correction (2026-10-06):** For each question, read the whole section of `DESIGN.md` that owns the feature, not grep hits alone, and grep every design document for the concept's nouns; and never put a question to the author without a recommendation, so an approval approves something stated.
- **Context:** ObjectFlow's defect register, cold readings and ADRs; ADR-0154, 2026-10-05; ADR-0158, ADR-0159 and ADR-0160, 2026-10-06.

### Validate the names a derived value is computed over
- **Pattern:** The core's `draft` took a source as a map from each module's name to its text and computed the digest over those names, without checking that each name was the one its file declares. A file under the wrong key was accepted, and its digest could never match `check-flows.py --digest` over the same files. A test of the next slice drafted a module under another module's key and passed, which is how it showed.
- **Correction:** Where a value is computed over labels a caller supplies, check each label against the content it labels at the boundary, and refuse a mismatch as invalid input; a label that only names is still part of the value.
- **Context:** Boundary validation in the Rust core's `change_source` (`src/request.rs`), ADR-0135 and ADR-0155; found 2026-10-05.

### A rule harmless at one version can become reachable at two
- **Pattern:** The core's erasure read only the installed version's `personal` marks, which matched ADR-0146's "personal under any version" while a store had one version. Building publishing made a second version possible, and with it an attribute dropped or unmarked before an erasure, which would have left its values behind with nothing reporting it.
- **Correction:** When a slice makes a new state of the store reachable, such as a second version, search the rules that quantify over it ("any version", "every event") for ones an earlier slice met only trivially, and either build them or answer the newly reachable case as not run.
- **Context:** The Rust core's publishing and erasure; ADR-0146 decision 2, ADR-0155 decision 6; found 2026-10-05.

### `$?` after a command substitution reports the substitution
- **Pattern:** A loop ran the ten design checks and printed each status as `echo "$(basename $c) rc=$?"`. The command substitution runs before `$?` expands, and resets it, so every check printed `rc=0` whatever it returned. The corpus check had failed on three missing back-links, its saved output said so, and the docs commit went in on the false report.
- **Correction:** Capture the status first, `rc=$?`, before any other expansion on the line, and print a name with no substitution in it. Read each check's verdict line from its output too, never the loop's summary alone.
- **Context:** Shell loops over `scripts/check-*.py` in ObjectFlow; commit `7ba2fcf` (ADR-0155's first docs), found and repaired 2026-10-05.
- **Recurred** 2026-10-09 and 2026-10-10, in the same form: `echo -n "$(basename $c .py)=$? "`, written ad hoc beside a gates script that captures `rc=$?` correctly. On 2026-10-10 the corpus check reported a finding while the loop printed `check-corpus=0`, which exposed it; rerun with the status captured first, all ten passed, so the commit of 2026-10-09 made on the bad loop's word was sound by luck. The correction stands; an ad-hoc loop is where it is forgotten, so write the loop from the gates script rather than from memory.
- **Recurred again, 2026-10-10, in the gates script itself**, which the note above wrongly says captured the status correctly: `line="$line $(basename $c .py) rc=$? ;"`. Every design check's status in the gate summaries read `rc=0` for nine commits, `9aa9eed` to `fb935d2`, while `check-flow-format-doc` failed with 11 and then 12 findings and `check-corpus`, at `fb935d2`, with three. Found when that check, run alone for a new edit, exited 1. The note's claim was not verified when written; it is corrected here. The gates script now captures `rc=$?` as the statement after the command and prints each check's own last line beside its status, so a `rc=0` beside "12 finding(s)" is a contradiction on one line. Separately, ADR-0176's docs commit ran one design check of the ten, `check-flow-docs`, and so missed the back-links `check-corpus` asks for: a docs-only commit runs all ten.

### Check that a decision's clauses can hold together at every step between
- **Pattern:** ADR-0156 decision 5 said both that each slice of the checker joins the dry run as it lands and that a sandbox runs the core's checks only when the last slice lands. Before the last slice the only dry runs are a sandbox's, so the two clauses could not both hold; it was found only when the first slice was wired in.
- **Correction:** When a decision sets a rule for the steps along the way and another for the end, read them together at an intermediate step before recording it; where they conflict, say which governs.
- **Context:** Sequencing clauses in ObjectFlow's ADRs; ADR-0156, 2026-10-05.

### Measure a self-test's reach before trusting it as an oracle
- **Pattern:** The core's port of the examples' checks agreed with the Python checker over every exported plant at the first run, and three of five mutations of the port went unnoticed: 30 of the 50 messages the reference's examples checks give were given by no plant, though its self-test's docstring said each rule was proven. An oracle of plants can only fail where a plant reaches.
- **Correction:** When a port is held to a reference's plants, mutate each rule of the port and run the oracle; for each mutation that passes, add a plant to the reference that breaks the rule, or an example the design lacks where the rule is about what a valid file may say. Count the rules no finding exercises, by their messages, before calling the port held.
- **Context:** The Rust core's checker against `scripts/check-flows.py` (ADR-0156 decision 4), `example_rule_plants`; found 2026-10-05, and again the same day porting the replay's matcher, where eight of thirteen mutations went unnoticed until nine plants were added, porting the metric reads, where six of thirteen did until six cases were added to both self-tests, and writing the reader's read, where three of eight did until three cases joined both.

### A change's source is the whole version, not a delta
- **Pattern:** The scenario replay drafted each published version with only the modules the version changed. The core installs a change's source as the version, so every module left out was retired, and requests on its types answered `unknown_transition` two versions later, which looked like a core bug until the declarations were read version by version.
- **Correction:** Draft a `DeclarationChange` with every module of the new version; when a publish's effect is surprising, read the installed declaration after each version before suspecting the engine.
- **Context:** ObjectFlow's `DeclarationChange` and `scripts/replay-scenarios.py`; ADR-0155, `declaration-syntax.md` §6.6; found 2026-10-06.

### Assert every scripted replacement, after rustfmt above all
- **Pattern:** Python edit scripts replaced text with a bare `str.replace`, which does nothing where the text is not found. Twice in one session the target had been reflowed by `cargo fmt` since it was read: a mutation ran unmutated and passed, and a test kept an expectation the script meant to change, costing a debugging round each time.
- **Correction:** In every edit script, assert the count of each target before replacing (`assert s.count(old) == 1`), and after `cargo fmt` re-read the region before scripting an edit to it; a mutation that passes is first checked for having applied at all.
- **Pattern (2026-10-06):** Striking a TODO entry by replacing a prefix of its line kept the line's tail, which a later edit then glued onto another entry; the assertion that the prefix occurred once passed, so it caught nothing.
- **Correction (2026-10-06):** In list documents, replace whole lines: find the entry's start and its line end, and replace from one to the other.
- **Context:** Scripted edits to the Rust core in ObjectFlow; found 2026-10-05 and again 2026-10-06.

### Run the gates in one call, never in parallel calls that share a shell
- **Pattern:** Three gate commands sent as parallel tool calls shared one shell: the first's `cd core` landed between the others' steps, so the studio's tests ran from the wrong directory and printed a stale output file's "28 passed", the checks' glob matched nothing, and the replay did not start. A passing line from a run that did not happen nearly stood as a result.
- **Correction:** Run gates sequentially, each from an absolute path, deleting each output file before its run so a stale one cannot be read as fresh; read the exit code of the run itself.
- **Context:** Claude Code's Bash tool, whose parallel calls share a working directory; ObjectFlow's gates, 2026-10-06.

### Restore a mutated file from a saved copy, never from git
- **Pattern:** Checking a mutation by hand, I restored the file with `git checkout --`, which put back the last commit and discarded the slice's uncommitted edits to it along with the mutation. Only a later grep showed the edits were gone.
- **Correction:** Before mutating a file, copy it aside and restore from the copy, as the mutation scripts do; and before any command that rewrites a tracked file from git, check `git diff --stat` for that file and treat uncommitted changes as work that command destroys.
- **Context:** Mutation checks during a slice, with uncommitted changes in the mutated file; ObjectFlow, 2026-10-06.

### A process a test or a server starts must stop when its parent does
- **Pattern:** The studio's server, started by the front end's tests and stopped with SIGTERM, started the core's service as its child and closed it in a `finally`; Python's default SIGTERM handling exits without running `finally`, so every test run left the core's services running under init. A Python test of my own then failed before its cleanup and left its server and core running too. Only listing the processes afterwards showed them.
- **Correction:** A server that starts a child turns SIGTERM into an ordinary exit (`signal.signal(SIGTERM, lambda *_: sys.exit(0))`) and closes the child on every path; a test that starts a process stops it in a `finally`; and after a run that starts processes, list them (`ps -eo pid,ppid,comm`) and confirm none is left, before and after a mutation as well.
- **Context:** The Flow Studio's core backend, its pytest and Vitest suites; ObjectFlow, 2026-10-06.

### A mutation that does not build tests nothing
- **Pattern:** Checking that a parity test catches a reordered type list, I mutated the Rust service with an expression that did not compile; the test run reported "2 failed, 5 errors in 0.52s", which reads as the mutation caught, though every test had failed at building the service.
- **Correction:** Build the mutated code first and confirm it builds, then confirm the failure is the assertion the mutation targets (print the `E ... assert` line), not an error before it; a run that ends far faster than the passing one is the sign.
- **Context:** Mutation checks across a language boundary, a Python test over a Rust service; ObjectFlow, 2026-10-06.

### A gate that prints nothing has not been shown to run
- **Pattern:** I listed "the flowrun self-test" among the gates and ran it as `python3 scripts/flowrun.py --self-test`; the script has no entry point, so the command imported the module, did nothing and exited 0, and the gate list reported it as passing for several commits. The runner's self-test does run, inside `check-flows.py`.
- **Correction:** A gate's evidence is its output, not its exit code: confirm each gate prints what it checked (a count, a summary line) the first time it joins the list, and read that line on every run; drop any that prints nothing.
- **Context:** ObjectFlow's gate list, `scripts/flowrun.py`, whose self-test `check-flows.py` calls; found 2026-10-06.

### Run clippy before the gates, not through them
- **Pattern:** Twice in one session a Rust change built and passed its tests, then failed the full gates on a clippy lint (`type_complexity`, `double_ended_iterator_last`), costing a gate run of over ten minutes each time.
- **Correction:** After any Rust change, run `cargo clippy --all-targets --all-features -- -D warnings` before starting the gates; the gates confirm, they are not where a lint is first seen.
- **Context:** ObjectFlow's core, whose `scripts/check` denies warnings; 2026-10-07.

- **Recurred 2026-10-14**, the lesson in hand: a function returning two vectors of pairs failed the gate on `type_complexity`, the change's own test suites having passed without clippy. Run it as part of the suites, not after them.
### Write a mutation's pattern from the formatted file
- **Pattern:** Twice in one session a mutation's search text was written from code as first typed, before `cargo fmt` reflowed it; the substitution matched nothing, the build and test ran on unchanged code, and the run's "ok" read for a moment as a result.
- **Correction:** Read the lines to mutate from the file as it stands after formatting, and treat a pattern that does not match as a failed mutation run, not as a pass; the helper should stop on it rather than go on to build.
- **Context:** Mutation testing in ObjectFlow's core with scripted substitutions; 2026-10-07.


### Wait on a run's own output, not a file a previous run wrote
- **Pattern:** A background gate run removed and rewrote a shared `gates.out` only after a first step, and the wait that polled that file for its last line found the previous run's complete file and showed its results as current; the stale counts were caught only by noticing they had not moved.
- **Correction:** Wait on the background task's own output file, or have the run write to a fresh file it names, and check a figure that must have changed, such as the test count after adding a test.
- **Context:** ObjectFlow's gates run in the background from the agent's shell; 2026-10-07.

### Before narrowing a bulk read, find what it checks over the whole
- **Pattern:** To save a read of a type's ids, the bulk read of objects' records was given only the ids it would fill; the read of the type's intervals refuses any interval whose object is not among the ids it is given, as a sign of a damaged store, so every other object's intervals were refused and the probe failed. The narrowing would have silently weakened that check had the reader skipped unknown ids instead.
- **Correction:** Before narrowing what a bulk read is given, read what the read validates against it; where the full set is the check's reference, keep it, and measure the cost instead of removing it.
- **Context:** ObjectFlow's core, `store::read_type_parts` and `spans`, building ADR-0165 decision 2; 2026-10-07.

### Test a lazy part with the object already held in its lighter form
- **Pattern:** Making an object's record lazy, a bulk read of every object kept the objects already held, so one read earlier by id, as the actor of a request is, came back without its record. It was missed twice in one session, in a publish's simulation and then in its install, because every publish test was approved by the built-in operator, whose type the install skips; the second was found only by the scenarios' replay, whose actor is a declared user.
- **Correction:** When a value becomes lazy, list every bulk read and test each after the same object has been read alone first, by a path a real caller takes (here, an actor of a declared type); keep the refusal of an unread value at the write, which turned both misses into errors rather than lost history.
- **Context:** ObjectFlow's core, `World::in_order` beside `World::get`, ADR-0165 decision 2; 2026-10-07.

### Never match a process by a pattern the command itself contains
- **Pattern:** `pkill -f` or `pgrep -f` with a pattern such as `vite` or `scripts/dev`, run from a shell whose own command line holds the same text, matched that shell and killed it, exit status 144, three times in one session; once it left a scratch file behind that the rest of the command was to delete.
- **Correction:** Stop a process by the pid its starter reported, or select by a pattern the command cannot contain, a character class such as `vit[e]`; and list what remains with the same kind of pattern before trusting that nothing does.
- **Context:** The agent's shell, stopping the Flow Studio's development servers and probes; 2026-10-07.
- **Recurred** 2026-10-10, through the correction itself: the loop used character classes for some alternatives (`[w]atch.sh`, `[p]ytest -q`) and left others literal (`cargo test --all-features`, `replay-scenarios`, `pnpm check`), so the second `ps | grep -E` matched the shell running it and killed it, exit 144, with an orphaned core service left behind. A class guards only the alternative it is in. Stop the gates by the pid the background task reports, never by a list of patterns, and when a list is unavoidable, print the matches and kill in a second command after reading them.

### A probe that starts a service must not end in a pipe it holds
- **Pattern:** A scratch script that started the core's service through the studio's `Core()` was piped into `tail`; when the script failed partway, the service it started outlived it holding the pipe's write end, so `tail` never saw the end of its input and the command hung, twice, while a mutation the command was to restore stayed in the source file.
- **Correction:** Write such a probe's output to a file, close what it starts in a `finally`, and restore a mutated file before anything that can hang, not after it; when a command hangs, look for a child still holding its output before blaming the code.
- **Context:** The agent's shell, probing the Flow Studio's backends; 2026-10-08.

### A rule's precondition read before calling a backend wrong
- **Pattern:** A design sentence (D528, "the attribute of the new type opens its own as a newly tracked member does") was read against the core's test, which asserts no interval for a retyped `size`, and the core was declared wrong in the conversation as a new defect. `size` was retyped from an enumeration to a string, which check 58 does not track at all, so the core was right; the claim was retracted.
- **Correction:** Before naming a defect against either backend, check every precondition the rule rests on in the format (what is tracked, what a check admits), and make the difference show on the wire with a probe before writing it down; a test assertion that contradicts a design sentence is evidence to examine, not a verdict.
- **Context:** ObjectFlow's publishing, the comparison of the core and the reference runner (ADR-0166); 2026-10-08.

### A module loaded by path has no name to look itself up by
- **Pattern:** `check-flows.py` passed itself to the runner as `sys.modules[__name__]`. Run as a script it is `__main__`, and imported it has its name, but the studio loads it by path with `spec_from_file_location("check_flows", …)` and never registers it, so every check that ran an example in the studio raised `KeyError: 'check_flows'`; the checker's own runs could not show it.
- **Correction:** A module that hands itself to another must not assume `sys.modules` holds it; fall back to its own namespace (`SimpleNamespace(**globals())`), and test the change through each way the module is loaded, here the studio's tests as well as the script.
- **Context:** Python, the flow checker loaded by the Flow Studio's server; 2026-10-08.

### A reused browser profile restores the last run's tabs
- **Pattern:** Driving the studio's page over the DevTools protocol, each run launched the cached Chromium on the same `--user-data-dir` and killed it at the end, so the next launch restored the tabs of every run before it. Four copies of the page then shared the one sandbox, each loading the first module and resetting it about once a second, and the run under test saw its objects vanish. It read as a bug of the page, and an idle page reproduced it, until the server's log showed `/studio/backend` asked eight times at startup, four pages under StrictMode.
- **Correction:** Give each driven run a fresh profile, `mkdtemp`, with `--no-restore-session-state`, and delete it after. Before calling behaviour a page's, check from the server's log how many clients are talking to it. Reading App's load effect while chasing this did find a real race, a superseded load still sent, now closed; but it was found by reading, not observed, and is reported so.
- **Context:** The Flow Studio's front end driven by hand through Chromium's DevTools protocol; 2026-10-08.

### "Not run" on both sides is agreement that compares nothing
- **Pattern:** The comparison of every example passed with the new `publish` examples in it, and a TODO note then claimed the comparison played their publish steps through both backends. It had not: the comparison gives its player a bundle, a player given one had no reader, and each publish step was not run on either side, which the comparison counts as alike. The stale declaration it would have found (D543) waited until the claim was checked.
- **Correction:** Where a comparison counts the same "not run" on both sides as agreement, a new case's passing proves nothing until its outcome is read: check that it ran, here by breaking the feature and seeing the comparison fail, before saying the comparison holds it.
- **Context:** The Flow Studio's backend comparison and any oracle test that tolerates unimplemented paths; 2026-10-08.

### Files the running checks import were edited under them
- **Pattern:** With every check running in the background for one decision (ADR-0167), the next decision's work began at once and edited `scripts/replay-scenarios.py` and the studio's sandbox, both of which the run imports in its last stages; the run then passed on a state no commit would hold, and could not stand as evidence for either.
- **Correction:** While a run of the checks is going, change nothing it imports: draft in the scratchpad, or commit the first change before starting the second. A run that overlapped an edit is rerun, and a change that went uncommitted meanwhile is split out and checked alone, here on a worktree of its own commit.
- **Context:** The repository's gate run (core, studio, web, the doc checkers and both replays); 2026-10-08.

### A question put to the author without a recommendation, three times
- **Pattern:** Two questions recorded for the author in one session, what the scenarios' port declared and whether check 44 should hold a mirror, listed options and asked "to decide" with no recommendation, against the standing rule that no question goes out without one. Each time the author answered "go with your recommendation", which did not exist, and the recommendation had to be formed after the fact and the ADR's status written to say so. It recurred a third time the same day in another shape: a question raised about check 30 was announced with a recommendation "to be recorded after" the commit in flight, and the author asked for the recommendation before it existed (ADR-0170).
- **Correction:** A question recorded for the author, in TODO, an ADR or a reply, carries the recommendation and its reason in the same entry; where the choice is genuinely open, say which way you lean and what would change it. Before ending a turn that hands decisions back, reread each one for its recommendation. Deferring the recommendation counts as omitting it: a question goes out only with its recommendation in the same message, and if the analysis is not done, say the question is not ready rather than announcing it.
- **Context:** Author decisions recorded in `TODO.md` and summarized in replies, ObjectFlow; 2026-10-08.

### A set comparison cannot see a count
- **Pattern:** The checker oracle held the core to the flow checker by collecting each side's findings into a set, so a finding made twice agreed with one made once; a mutation making the core report a cycle at every closing passed it, while the flow checker's own self-test, which compares lists, caught the same mutation on its side (D545).
- **Correction:** Where two implementations are held equal over outputs that can repeat, compare multisets (counts per key) or sorted lists, never sets; and mutation-test a duplicate as one of the mutations, since only a duplicate shows the difference.
- **Context:** Oracle and comparison tests between the core and the Python reference, ObjectFlow; 2026-10-08.

### A filtered run hid a crash
- **Pattern:** A self-test run was piped through `grep "check 10"` to show only the new cases' lines, and its output was read as all cases passing; the script had failed to load on a syntax error, the traceback had no matching line, and the empty result looked like success. The success was reported to the author before the exit status was looked at.
- **Correction:** A filtered view of a run is evidence only with the run's exit status beside it, and a count of the expected result lines: save the output, print the status, count the lines expected and show everything else. The same holds for a mutation runner, where a build failure must be told apart from a kill.
- **Context:** Every gate and self-test run through a pipe, ObjectFlow; 2026-10-08.

### A check enforced, its inventory row left behind
- **Pattern:** Five checks were enforced in one day (12, 32, 30, 10, 36), each recorded in `declaration-syntax.md` §10, `flow-format.md` §7 and `TODO.md`, while `core-checker.md` §2, the table saying which part of the reference enforces each check, kept saying "neither" for all five until building the sixth found it.
- **Correction:** Before recording a check's enforcement, search the design for every place that states its status, by its number (`grep -n "^| <n> |" docs/design/*.md`) as well as by its words, and change each; the inventory is the one a reader consults first.
- **Context:** `declaration-syntax.md` §10's checks and the documents that track them, ObjectFlow; 2026-10-08.

### A walk's expected results are read from a run of that walk
- **Pattern:** Writing the Flow Studio's manual test guide on 2026-10-09, walk 5 said the dry run of `service-v2.yaml` shows "two notices", read from a dry run made over an empty sandbox; the walk opens a job first, which adds a third, the backfill of `ServiceJob.site` reaching it. Three more steps (the clock's *Advance*, the diagram following the type tab, a migrated object's history) were written as expected without having been watched, and *Advance* turned out to show nothing anywhere on the page. A run of each walk exactly as written caught all four before the commit.
- **Correction:** Read each expected result in a walk-through from a run of exactly the steps it gives, in its order and from its starting state; drive any step not yet watched before writing what it shows, and give where the backends differ both readings.
- **Context:** Manual test guides and README walk-throughs, the Flow Studio; 2026-10-09.

### A test that a program refuses to start waits with a deadline
- **Pattern:** The deployment service's tests checked that a store of another release, and a retention given without a store, are refused, by running the service to its exit with `Command::output()`. Mutating either refusal away let the service start and serve, and the test waited on it for twenty minutes instead of failing, until its process was stopped by hand.
- **Correction:** A test that a server refuses to start spawns it and polls `try_wait` against a deadline, stopping the child and failing when the deadline passes; mutate the refusal to see the test fail, not hang, before trusting it.
- **Context:** Rust integration tests that run a service binary, `objectflow-service/tests/deployment.rs`, ObjectFlow; 2026-10-10.

### The gates run as a tracked background task, never behind a bare `&`
- **Pattern:** Three times on 2026-10-10 the full gates were started as `bash watch.sh > /dev/null 2>&1 &`: nothing announced their end, and the redirect threw away clippy's status line, which the gate script prints first; once it hid clippy's refusal of `expect()` in a new integration test until the core's check failed on it minutes later.
- **Correction:** Start the gates with the tool's background mode, whose end is announced and whose output is kept; where a run was already started bare, watch it by its pid in a background task that prints the result file and clippy's log when the pid ends. Run clippy over a new test file before the gates, not only its tests.
- **Context:** The full gates, `scratchpad/watch.sh`, ObjectFlow; 2026-10-10.
- **Recurrence, 2026-10-10:** a fourth bare `&` minutes after this entry was written, so the habit was not enough: the gate script now runs clippy itself and writes its status into the result file with every other gate's, so a run started any way keeps it.

### A new finding code is unheld until the oracle lists it
- **Pattern:** ADR-0176's `veto` notice was ported to the core's checker and its plants exported, and the oracle passed; a mutation that dropped every `veto` from the core still passed, because `checker_oracle.rs` compares only the codes in its `COVERED` list, and a new code is in none.
- **Correction:** A new code a check reports joins `COVERED` (or `COVERED_IN_EXAMPLES`) in the same change that ports it, and the port is mutated away once to see the oracle fail.
- **Context:** The Rust core's checker held to `scripts/check-flows.py` by exported plants, ObjectFlow; 2026-10-10.

### A test that follows a server's cursor bounds its loop
- **Pattern:** A paging test looped `while cursor is not null`; with the keyset mutated away, every page named a further cursor, and the test ran until the harness killed it, a hang rather than a failure that names the fault.
- **Correction:** A loop over pages, retries or any answer the server controls asserts a bound beyond which the server is wrong, and prints the last answer.
- **Context:** Tests of paged reads and other server-driven loops, ObjectFlow's deployment tests; 2026-10-11.

### Tests that run at once name their scratch files apart
- **Pattern:** Two deployment tests copied the same release fixture to `fresh_store("upgrade-reimbursement")`, a name the process id alone made unique; cargo runs a test binary's tests on threads of one process, so both wrote one file, and one test's upgrade met the other's half-copied or already-upgraded store. It passed in every gate run from the upgrade's commit until one run under load, where it failed as a 503 `engine_mismatch` that looked like an upgrade fault.
- **Correction:** A scratch path is unique per test, not per process: the test's own name or tag in it, never a name derived only from the fixture being copied.
- **Context:** Rust integration tests with temporary files, ObjectFlow's `deployment.rs`; 2026-10-11.

### A process started after `cd` in the background is a subshell's child
- **Pattern:** Probes started a service as `cd $P && service --port 0 & echo $! > svc.pid` and stopped it by that pid; `cd … && cmd &` backgrounds the whole list as a subshell, so `$!` named the subshell, the kill reached it, and five services stayed running until a `ps` before the next probe found them.
- **Correction:** Start a background process with no `cd` in its list (absolute paths, or `(cd "$P" && exec cmd) &`, whose `exec` makes the subshell the process), and after stopping it check `ps` for what the probe started.
- **Context:** Shell probes of the engine's service, ObjectFlow; 2026-10-11.
- **Recurred** 2026-10-13: a first-publish check started `D=… && rm … && service … &`, an `&&` list without `cd`, which backgrounds as a subshell all the same; its `$!` was the subshell's, the service ran on for 98 minutes, and `ps -p <that pid>` confirmed the wrong process stopped. A cold reviewer found it. Any `a && b &` is a subshell: start the process alone on its line, and check `ps` by what it runs (its store's path), never by the pid just killed.

### A feature's documentation commit searches every document for its absence
- **Pattern:** ADR-0179 built paging and its commit updated `wire.md` and `integrate.md`, which said "pages hold everything today"; the README's status line, "it does not page reads", stayed, and both third-review agents read it as current.
- **Correction:** When a change ends a stated limitation, search the whole repository for the limitation's wording and its negations (`not page`, `pages hold`, `not yet`) before committing, the README and the guides first.
- **Context:** Documentation of shipped features, ObjectFlow; 2026-10-11.

### A recommendation that reverses a design sentence is checked against the PRD first
- **Pattern:** Asked to recommend on the third review's questions, I recommended refusing an actor in a final state, reversing `DESIGN.md` §5.8, and the author accepted it; implementing it, the scenarios' replay failed on §40, which the PRD's UC-22 requires: "the departed user's request is recorded as theirs", with revision 10's rationale "a departed one is recorded". The recommendation contradicted the baseline, unchecked.
- **Correction:** Before recommending a decision that reverses or narrows a design statement, search the PRD for the requirement and the use case behind that statement, and put any conflict to the author with the recommendation, as a PRD revision to approve, not after the decision is taken.
- **Context:** Design recommendations in ObjectFlow, whose PRD is the baseline every design choice is checked against; 2026-10-11.


### Restricting who may call an operation is checked against the design's capability statements
- **Pattern:** Building the production review's MAINTAIN finding, I made `maintain` the operator's act, as `upgrade` is, the recommendation I had written into `TODO.md` myself; `DESIGN.md` §10 says "who may ... maintain ... is decided by upper-layer applications (ADR-0114)", withdrawing exactly such a capability. I had checked F3's engine-level refusal against the PRD the same hour and found T6 and N7 against it, but searched nothing for `maintain`; the same mistake as the lesson above, a second time in one session.
- **Correction:** Before any change that narrows who may call an operation or take a transition, grep `DESIGN.md`, the PRD and the ADRs for the operation's name and for "capabilit", "authoriz" and "upper layer"; fix a finding by validating what the store validates (the actor's identity and state), never by adding a role, unless the author decides a PRD or design revision.
- **Context:** ObjectFlow, whose engine authenticates and authorizes nobody by design (PRD N7, ADR-0114); reviewers' recommendations to add an authorization check are the case to watch; 2026-10-11.
- **Recurred 2026-10-13, as a restriction of when, not who.** Fixing the tenth review's PROPOSAL-RACE, I pinned a proposal to its object's version at filing, refusing any approval after the object moved; `DESIGN.md` §9 says an approval re-evaluates every guard "under the current declaration" and filing checks "short of the object's state and the guards", so a proposal may be filed before its object reaches the state its transition leaves, and an older test, a job's handover proposed while it was still `OPEN`, failed. The fix let the approver name what it read (`target_version`) instead. A change that narrows when a decision may be taken is checked against the design's statement of that decision as a change of who may take it is.

### A module constant appended to a test file can shadow one of the same name
- **Pattern:** Adding F5's comparison test, I appended a module `TALLY = """…"""` to `test_publishing.py`, which already defined `TALLY` 160 lines earlier; Python rebinds the name at import, so an unrelated test that read `TALLY` loaded my module and failed `unknown_transition`, while my own test, run alone with `-k`, passed.
- **Correction:** Before adding a module-level name to a test file, grep the file for it (`grep -n "^NAME = "`), and run the whole file, not only the new test, before calling it done. *Recurred 2026-10-13 with a name that is a value*: a new deployment test took `fresh_store("beside")`, an older test's store, keyed by name and process, so each deleted the other's store when the tests ran together, and the gates failed on a store read empty; a store's, a socket's or a directory's name in a test is such a name, and grepped as one (`grep -n 'fresh_store("<name>")'`).
- **Context:** Python test modules with flows as module constants, the Flow Studio's server tests above all; 2026-10-12.

### Read the gates' verdict before committing, never in the same command
- **Pattern:** I ran `grep` over the gates' summary and `git commit` in one chained command, so the commit ran whatever the summary said: the gates had reported `FAILED core` (a library test asserting the answer `no_retention` replaced), and `d813e62` was committed failing; the fix followed in its own commit.
- **Correction:** Read the gates' summary in one call; commit in the next, only on "every gate passed". Never chain `git commit` after a command whose output decides whether to commit.
- **Context:** ObjectFlow's `scripts/check-all` before each commit, run in the background; 2026-10-12.

### A mutation that matched nothing passed as a surviving mutant
- **Pattern:** A mutant applied with `sed` against a line `cargo fmt` had since rewrapped matched nothing, so the file was unchanged and its test passed, read at first as a mutant the test missed (2026-10-12, the fourth review's minors).
- **Correction:** Apply every mutant by a script that asserts its pattern matched once, as the Python edits do, and match across whitespace (`\s*`) where a formatter may have wrapped the line; a mutant that did not apply is reported, never run.
- **Context:** Mutation checks over Rust formatted by rustfmt; any formatter.

### A fix tested at its function, not along the finding's path, was claimed whole
- **Pattern:** The fourth review's refused-restore finding was "fixed" by giving the quick check a connection that carries no log in on close, and tested by calling that function; the start, the path the finding took, opened the file once before it on a plain connection, which still wrote the log in. The changelog, the guide and the message claimed it fixed; the fifth review measured it not (D609, 2026-10-12).
- **Correction:** Test a fix along the path the finding was reported on, end to end where the finding was end to end, here a refused start leaving the files byte for byte, and mutate the finding's own cause back in; claim the fix only from that test.
- **Context:** Any finding a reviewer reproduced through the service; ObjectFlow's production reviews.

### A notice's scope fixed from the decision's words, before it ran over the samples
- **Pattern:** ADR-0190's notice `unbounded_reach` was written to take every declared limit, as the decision read; only the scenarios' check, run late, showed it firing fourteen times on one version's ordinary loops, where the refusal past a limit is the author's own rule (2026-10-13).
- **Correction:** Run a new notice over every sample bundle and the scenarios' versions before fixing its scope in the ADR; a notice that fires where nothing is wrong teaches authors to ignore the one that matters.
- **Context:** ObjectFlow's checkers; any lint or warning added to a corpus.

### A form advised in a message written from memory, not from the language
- **Pattern:** The notice told authors to declare `count(<set>) <= n`, which is not the language's form for a set's size (`count(x in <set>)`); the samples bounded that way stopped their examples from running, and the checkers recognised only the wrong form (2026-10-13).
- **Correction:** Before a message advises a fix in the flow's language, write the fix into a sample and run it; check the form in `declaration-syntax.md` rather than recalling it.
- **Context:** Any checker message or doc that prescribes code.

### A test helper that panicked holding a child left a service running
- **Pattern:** The deployment tests' `start()` spawned the service and panicked when its first line was not the expected one, before wrapping the child in the guard that kills it on drop; a new warning line printed first, the test failed, and the orphaned service held the test command's pipe open, so the command hung for ten minutes (2026-10-13).
- **Correction:** Kill and reap a spawned child on every path out of the helper, or wrap it in its guard before anything that can fail; and when a test command hangs, list the processes it started before anything else.
- **Context:** Rust integration tests that spawn the service; any test harness that spawns processes.

### A remedy recommended without measuring what it costs the engine
- **Pattern:** The fifth review's ERASE-LIMIT fix had a notice recommend a bounding invariant, `count(x in lines) <= n`, and the samples declare it, trusting `DESIGN.md` §5.6 that such a traversal invariant is checked from the related objects; the core checked it on its whole type after every write, and a go-live of the shipped sample went from 13 ms a write to 2.8 s (sixth production review P8, 2026-10-13).
- **Correction:** Before a notice, a guide or a sample recommends a construct, measure a write with it on a store of realistic size; and hold each cost a design document states with a test that fails when the implementation does more, as the whole-type reads now counted do.
- **Context:** Any recommendation an engine makes to its authors; ObjectFlow's checker notices and samples.

### A decision written without reading the earlier one it reverses
- **Pattern:** ADR-0192 decision 7 said to refuse a second service on a store, to answer a reviewer's "ambiguous start line"; ADR-0178 decision 2 had decided that a second process may share a store, an upgrade's old copy among them, and two deployment tests held it. The tests caught it; the decision text had not looked (2026-10-13).
- **Correction:** Before deciding a behaviour, grep the ADRs for the area it touches and read the decisions there; where a finding asks only for a word, give the word rather than reverse a decision, and say in the new ADR which one it keeps.
- **Context:** ObjectFlow's ADRs, decided on a standing instruction; any project with a decision record.

### A mutation that turned a failing test into a hanging one
- **Pattern:** A test that started the service on a read-only store expected it to exit, and waited with `output()`; the mutant that let the service start made the test wait for good, and the mutation run with it, until the service was stopped by hand (2026-10-13).
- **Correction:** A test that runs a process expected to exit waits for it within a bound, then kills it and fails, so a regression fails the test rather than hanging it; a mutation check of such a test is what shows the difference.
- **Context:** Integration tests that spawn the service; mutation checks of them.

### A consumer the search listed, and nobody read
- **Pattern:** Before rewriting the guides, a search for what reads them listed `apps/flow-studio/server/tests/test_golive.py`, which takes `write-a-flow.md`'s purchasing module from the page, and `scripts/check-flow-docs.py` checks every document's YAML blocks; neither was read, a first line naming each block's file was added, and three gates failed (2026-10-13).
- **Correction:** Read every file a search lists as reading what is about to change, before changing it; a convention added to shared text, a comment line in a code block among them, is checked against each parser of that text, and taught to it where it is kept.
- **Context:** Documentation that tests and checkers read; any text with more than one reader.

### A fix that turned an indexed lookup into a sweep
- **Pattern:** Comparing identities without case (ADR-0192 decision 6) replaced the indexed lookup of an actor's identity with a sweep of every actor for each actor checked; every gate passed, and the seventh review measured a publish five times slower on two years and 70 s of approval with 3,600 actors, bisected to that commit (2026-10-13).
- **Correction:** A change that replaces a read by an index with a sweep, in a path a publish or a whole-store check runs per object, is held by a test counting what it enumerates (the world's `keyed`, `gathered` and `enumerated` counts) and measured on the review's store before it is committed.
- **Context:** ObjectFlow's core; any lookup inside a per-object loop.



### A decision written for one backend of two
- **Pattern:** ADR-0194 decision 6 said a time computed past the calendar would have no value in a filter, as a date's has none; building it found that the reference runner holds a time as an untyped count of seconds, so only the core could tell a time from a number, and the two backends would have answered a guard differently. The decision was moved to "unchanged" with that reason (2026-10-13).
- **Correction:** Before deciding a change to what an expression means, read how the reference runner evaluates the same expression; a semantic only one backend can carry is not a fix but a divergence.
- **Context:** ObjectFlow, where the Python runner is the oracle the core is held to.

### A changed rule's old tests are found by the crate's suite, not the gate
- **Pattern:** Building ADR-0195, an agent's approval refused where no `publishing` is declared and history moved to a pool of its own, each change was tested by its new test and that test's mutants, and the full `check-all` was the first run of the crates' whole suites; twice in one batch it failed on an older test that asserted the rule being changed (`a_version_is_approved_only_by_the_kinds_the_installed_one_declares`, `only_the_reads_that_grow_with_the_store_wait_their_turn`), each costing a twenty-minute gate run. A search for the old rule's wording had missed both, since each test phrased it its own way.
- **Correction:** After changing what a rule decides, run the changed crate's whole suite (`cargo test -p <crate>`, a minute or two) before the gate; read each failure as a test of the old rule to update, or a defect.
- **Context:** Rust core and service, any change of behaviour that older tests may pin; 2026-10-13.

### A stall's signal waited on the thing that stalled, and its test checked only that it came
- **Pattern:** ADR-0195 decision 6's log line for a write waiting on another process's lock was said on the log's cut's tick, which then takes the writer; a tick that fell within the write's first wait found no stall yet and waited out the whole stall, so the line came only once the write gave up. Its test asserted the line appeared eventually, with a 1 s busy timeout shorter than the tick, which never meets that ordering; both of the ninth review's reviewers found it.
- **Correction:** A watcher reads only what it watches and never takes the resource that stalls; test the signal while the stall lasts (the write not yet finished), with timings that put the watcher's tick inside the stall's first wait, and mutation-check by moving the watcher back.
- **Context:** The service's health and log signals, any watchdog; 2026-10-13.

### A superlinear measurement is a finding, not a slow benchmark
- **Pattern:** Measuring the eighth review's fix, a dry run of 3,000 types took 8.4 s, 4,500 took 20.3 s and 8,000 ran past eleven minutes; the run was stopped and the size lowered to fit the benchmark's time, and the growth itself was not looked into. The ninth review then found the dry run's slot held 229 s, and profiling showed the checks quadratic in the types.
- **Correction:** Where a cost grows faster than its input in a measurement, stop and find why before going on: compare call-count profiles at N and 2N (callgrind needs no privileges), and the function whose cost quadruples is the one.
- **Context:** Any benchmark of the engine; 2026-10-13.
- **Recurred 2026-10-14**, in concurrency. ADR-0200 recorded heavy reads at once costing 83 CPU-s against one's 3 with a suspect named, SQLite's memory-statistics mutex, and the cause left to measure later; the next review found it a major. A probe of one filter on one, two and four threads reproduced it in a minute, each thread's user and system time split pointed at a lock, and rebuilding SQLite one flag at a time found it, the suspect cleared. For a cost that grows with concurrency, compare each thread's CPU at one, two and four threads, and test each hypothesis by changing only it, before naming a cause.

### A bound on a walk's depth is no bound on its work
- **Pattern:** ADR-0197 bounded derived typing at 64 attributes deep, for a chain of 3,000 that overflowed the stack, and measured the chain linear. The eleventh review's adversary wrote a chain only 40 deep, each link reading the one before twice: 2^40 typings held the dry run's slot past six minutes. Measuring the fix found the same unbounded fan-out in four more walks over the same structure (the optional reads, the inliner, the logic solver, the run-time typing of a zero) and in evaluation itself.
- **Correction:** When a recursive walk is bounded, bound its total work, by a memo per node or a bound on what it unfolds to, not its depth alone. Find every walk over the same structure by searching for its accessor (`derived_attributes`, `t.derived`), and test each with the widest shape (a diamond), not only the deepest (a chain).
- **Context:** Checkers and evaluators over derived attributes, any recursive walk over a declared graph; 2026-10-13.
- **Recurred** 2026-10-13: the bound that fix added was counted within each module, so a chain split across a module and one importing it passed each and multiplied past it. A bound on a composed structure is computed over the composition: every unit a walk can reach, here every module the checked one imports.

### A command chain written across lines runs on after a failure
- **Pattern:** A backgrounded command joined its first steps with `&&` but put a mutation runner on later lines. When the first script failed an assertion, the later lines still ran, and a second mutation run started over the first. Two scripts mutated the same sources at once, one's "as built" test failed against the other's mutant, and either could have restored the other's mutant as the original.
- **Correction:** Join every step of a script that mutates sources with `&&`, or run it under `set -e`. Never start a mutation run while another may be running. After any interrupted run, search the sources for the mutants' marks before going on.
- **Context:** Mutation testing in this repository; 2026-10-13.

### A timing test passed on an input refused for another reason
- **Pattern:** The first test of the typing memo timed `checks_of` on a 22-link diamond and discarded its result. Its flow declared `derived_attributes` before `states`, an order error, so it timed whatever ran on an invalid module, and passed. Only rewriting it to assert the findings showed it.
- **Correction:** A test that bounds how long something takes also asserts what it produced: the findings expected, and no others. A refused or malformed input is fast for reasons of its own.
- **Context:** Scaling tests of the checkers and the engine; 2026-10-13.

### A decision claimed a bound wider than its code enforces
- **Pattern:** ADR-0198 decision 2 and the changelog said a dry run's checks are held to `--dry-run-timeout`. The deadline was read only where derived attributes unfold, the one place the review had found slow. The next review drafted 60,000 bare types, a shape no derived attribute is in, and the checks ran 374 s, past the deadline the documents promised.
- **Correction:** State a bound exactly where the code enforces it ("read while derived attributes unfold"), and test the claim with a shape outside the mechanism that enforces it, not only the shape that prompted it.
- **Context:** Deadlines and bounds in the checker and the service; ADR text and the changelog; 2026-10-14.

### A growth test over valid inputs misses what refused inputs cost
- **Pattern:** The checks' growth test timed 1,000 and 4,000 valid types, which produce no findings, so two quadratics that grow with the findings went unseen: each finding compared with every one before it, and each finding's line found by searching every key. 60,000 refused types took 374 s.
- **Correction:** A scaling test of a checker times the shapes it refuses too, where each element makes findings, and asserts the findings it made.
- **Context:** The checkers' and the publish path's scaling tests; 2026-10-14.

### A bundled dependency's build flags are part of the program
- **Pattern:** `libsqlite3-sys`'s bundled build sets `SQLITE_ENABLE_MEMORY_MANAGEMENT`, which puts every connection's page cache behind one mutex for the process. Nothing in the repository said so, and four heavy reads at once each took 11.2 s where one took 2.2 s, for two review rounds.
- **Correction:** When work at once does not scale, read the options a bundled C dependency is compiled with (`PRAGMA compile_options`, or its build script), set the ones that matter in the repository's build configuration, and hold each with a test that fails a build without it.
- **Context:** The core's SQLite, and any `-sys` crate built from source; 2026-10-14.

### A fix classified at the wrong grain
- **Pattern:** ADR-0200 sent a read of one object to a pool where any rule of its type grew with the store. A `get`, which evaluates no condition, then waited behind reports because a guard on one transition read a metric: 25.5 s, a major in the next review.
- **Correction:** Classify a request's cost by what that operation evaluates, and test each operation against a type where only another operation's rules are heavy.
- **Context:** The service's read pools; 2026-10-14.

### Test inputs in reading order leave a walk's other branch unexercised
- **Pattern:** The depth bound of the checkers' walk was tested on chains declared in reading order, so each link's reads were already computed when it was reached. The branch carrying depth up from a child pushed later never ran, and a mutant dropping its `+ 1` survived.
- **Correction:** For an iterative walk with a memo, give the tests an input whose reads point forward, as a type reading one declared after it, so both the memo's branch and the push-and-pop branch run, and mutate each.
- **Context:** The checkers' fan-out walk; any memoized iterative walk; 2026-10-14.

### Staging everything stages what a run left behind
- **Pattern:** Commit `24e7ff6` staged with `git add -A` and took `t40k.pid`, a process id a reviewer's run wrote at the repository's root.
- **Correction:** Before staging, read `git status --short` and every untracked path in it; stage only what the change made.
- **Context:** Commits in this repository; 2026-10-14.

### A release named that does not exist
- **Pattern:** Writing the guides for the masked idempotency digest, I wrote "a key a store recorded before release 0.3.0"; no release 0.3.0 is planned, the next being 1.0.0. Caught on rereading, before the commit.
- **Correction:** Name what distinguishes the data ("a digest made with personal values in it, as release 0.2.0 made them"), or a release that exists; never a version number not in `Cargo.toml` or the changelog.
- **Context:** The guides and the changelog; 2026-10-14.

### A check widened without reading how the engine applies what it judges
- **Pattern:** Extending `unbounded_reach` from erasures to every transition, I tested it on planted flows and counted it over the examples, without reading how the engine applies a loop's limit: to what its `where` keeps, not to the set. The gate's scenario module then showed six notices on filtered loops, each advising a bound on the whole set, which was wrong.
- **Correction:** Before widening a check, read how the engine applies the construct it judges, run it over the realistic modules (the scenarios, not only the examples), and read every new finding before recording an expectation.
- **Context:** The checkers' notices; 2026-10-14.

### A renamed code left behind in one guide
- **Pattern:** I changed an agent's personal-query refusal from `malformed_filter` to `personal` after writing the guides, and updated `wire.md` and `errors.md` but not `agents.md`; the next review found the guide naming the old code.
- **Correction:** After renaming a code, a flag or a message, search every document for the old one before committing.
- **Context:** The guides; 2026-10-14.

### A special case written on an unchecked belief
- **Pattern:** I exempted the operator from the personal guard, believing it was no stored object and so had no kind; it is a built-in human, so the exemption was dead, which only an equivalent mutant showed.
- **Correction:** Check how an actor or entity resolves before writing a case for it; and treat a surviving mutant as a question about the code, not only the test: an equivalent one marks a branch to remove.
- **Context:** The service's actor checks; 2026-10-14.

### A fix that lived in the build's environment
- **Pattern:** ADR-0201's SQLite flag sat in `core/.cargo/config.toml`, which cargo reads only for a build run from `core/`; a build from the repository's root brought the collapse back, and only a start-log line said so.
- **Correction:** Put build configuration where every build in the repository reads it, and make a build without a setting the program needs stop, rather than test for it after the fact.
- **Context:** The core's build; 2026-10-14.

### A value found by how it was written, where its type said where it was
- **Pattern:** ADR-0202 replaced an erased identity in the attributes an effect wrote from `actor.id`, the path the review had shown. The next review copied it once more, `requester: requested_by`, and it survived; the checkers already required every runnable copy to be of type `identity`.
- **Correction:** When a value must be found again, find it by what it is, its type, not by the one way it was seen to travel; and test with a copy made another way.
- **Context:** Erasure and any rewrite by value; 2026-10-14.

### A protection applied one level deep
- **Pattern:** Check 56 refused a metric reading a personal member of its item, not one reached through a reference or a derived attribute, so `d.owner.salary` and `d.owner_name` passed, while check 10 already followed both to find a personal value.
- **Correction:** Where a rule protects a class of value, apply it through every way the language reaches that value, reusing the reading that already follows them; and plant a case for each way.
- **Context:** The checkers' personal-data rules; 2026-10-14.

### A guard decided against a PRD row it never read
- **Pattern:** ADR-0201 decision 4 and ADR-0202 decision 3 made the engine refuse an agent's, then an unknown actor's, read by a personal member, reading the actor's kind. PRD T5, a Must, says "the engine filters no read by who is asking"; neither ADR checked it, and an independent critique of ADR-0204 found the conflict.
- **Correction:** Before deciding any behaviour about actors, visibility or storage, read the PRD's §6 rows that govern it, and cite them in the ADR; a decision that contradicts a Must is a PRD §12 revision for the author, never a quiet exception.
- **Context:** ADRs decided under a standing instruction; 2026-10-14.

### A reviewer's claim about a promise taken without checking the decision
- **Pattern:** The fifteenth operator wrote that `/metrics`' names are not promised by ADR-0180; I put that into `operate.md`, ADR-0202 and the register. ADR-0183 holds those names under ADR-0180's promise.
- **Correction:** A reviewer's statement about what the project promises is a claim to check against the ADRs before it changes a guide; cite the decision it rests on.
- **Context:** Folding review findings into the guides; 2026-10-14.


### One value standing for absent data and for a failed computation
- **Pattern:** The evaluator answered an arithmetic past its bound as absent, the same value as data nobody had supplied, and recorded the difference in a flag beside it that only the effect path read. A derivation answered null, a query's filter dropped the object from its matches and from their negation, an invariant held and admitted the write it should refuse, and a guard masked it with `is null`. Each place read the absence as absence.
- **Correction:** A failure is not a value. Make it an error the evaluator returns, distinct from the value domain, so every caller must handle it and none can read it as data; keep "unknown" for what the language defines as unknown. Exempt only where the answer stands without it, such as the side of an `or` that does not decide.
- **Context:** The core's evaluator and the reference runner (ADR-0205 decision 2); 2026-10-14.

### A decision worded wider than its evidence
- **Pattern:** I wrote ADR-0205 decision 2 as "Unknown is absent data alone". The audit's evidence and my recommendation were about arithmetic past its bound, and ADR-0047 keeps a division by zero unknown. Built as worded, it would have made `done / total` with `total` 0 refuse the read of every object it is derived on. It was found only while building.
- **Correction:** Before recording a decision, read each sentence against the decisions that define the same term, and word it at the scope of the evidence. A definition of a term ("unknown means …") is a claim about every existing use of it.
- **Context:** ADRs written from audit findings; 2026-10-14.

### A tee test that cannot reach a runtime refusal
- **Pattern:** The Studio's sandbox runs the checker on what it loads, so a tee test can hold only flows both checkers accept. Once the checkers refuse a construct at publish, the runtime refusal of the same construct, in the runner as in the core, is reachable only through an expression whose type the checkers cannot tell, and a mutant of it survives a tee test written with the plain construct.
- **Correction:** Hold a runtime refusal the checkers also make by a unit test of the runtime itself (the core's `Library::load` does not check; the runner's evaluator can be driven directly), and hold it through a checked flow by an expression the typing cannot tell.
- **Context:** The reference runner and the Studio's tests; 2026-10-14.

### State a publish builds in memory, lost on a restart
- **Pattern:** A publish dropping a type retired it in the library it installed, in memory. A store opened again loaded the latest version's source alone, so the retired type was gone, and every read of its objects answered `500`. No test restarted after a drop; the audit found the type's missing erasure, and only the test written for that fix restarted.
- **Correction:** Whatever an operation derives into the running state, the open path must derive again from what is stored. Test each such derivation across a restart, the way an operator meets it.
- **Context:** The store's library and every in-memory product of a publish; 2026-10-14.

### A flag read from a shared marker
- **Pattern:** An object counted as erased when any of its events had source `erased`. The `redact` events an erasure records on other objects carry that source too, so an object whose value another's erasure redacted could never have its personal attributes written again. It was found only when a new kind of `redact` event, on a label, showed the label as erased.
- **Correction:** Derive a state from the event that means it, by its transition, not from an attribute several kinds of event share; and when adding a new event of an existing source, check what reads that source.
- **Context:** The store's derivation of `erased`; 2026-10-14.

### A fix in both backends tested in one
- **Pattern:** The identity area's fixes were built in the core and in the reference runner, and tested by core tests alone. Mutation testing found seven of the runner's halves held by nothing: its case-folded rename, a dropped type's erasure, the shadowing refusal, a loop's `not_erased`, a label's `redact` event and a metric naming actors by their objects. Writing the missing test then found a real difference, the runner reporting `lost:` transitions for a retired type's objects, which the core does not.
- **Correction:** A fix made in both backends gets a test through both, a `tee` test ending in `alike`, written with the fix. A runner path the checker makes unreachable is removed and its comment says why, never kept as an untested copy of the core.
- **Context:** `scripts/flowrun.py`, `scripts/flowpublish.py` and the Flow Studio's tee tests; 2026-10-14.

### An oracle holding the rule a decision replaced
- **Pattern:** ADR-0205 decision 3 changed what a key answers after an erasure: refused, where it replayed. The core, its tests and the guides changed with it; the adversarial harness's key oracle did not, and failed the first full run, the core being right.
- **Correction:** When a decision changes a promise, search every oracle that states it, the harness's checks and the reference runner included, and change them in the same commit as the code.
- **Context:** `objectflow-harness`, ADR-0204 §3's promises; 2026-10-14.

### A start reordered, its announcement with it
- **Pattern:** Binding the service's addresses before the store was opened, to keep a refused start from touching the store's files, moved the "listening on" line ahead of the store's opening. The deployment tests, like an operator's script, read that line as "it serves", and six of them raced the store's creation.
- **Correction:** When reordering a start, keep what each line it says means. Here the bind moved and the line stayed where the service begins to serve.
- **Context:** `objectflow-service`'s start; 2026-10-14.

### Two tests on one store
- **Pattern:** A new deployment test named its store `restored`, as an older test already did. Tests run at once in one process and share the store's path, so the two collided and one failed now and then. It passed alone, which looked like a flaky test rather than a fault of the new one.
- **Correction:** Give each test's store a name of its own, and check for a repeated name when adding a test: `grep 'fresh_store("' … | sort | uniq -d`.
- **Context:** `objectflow-service/tests/deployment.rs`; 2026-10-14.

### A test rewritten by an unchecked replace
- **Pattern:** A probe was turned into the real test by string replacements on its text. One, its final `eprintln!` into an assertion, matched nothing, since the formatter had reflowed the line, and the replace raised nothing. The test asserted nothing and passed; only a mutant disabling the behaviour it was for showed it.
- **Correction:** Every scripted edit checks that its target occurs exactly once, inside helper loops as well as at the top; a new test is trusted once a mutant of what it holds has made it fail.
- **Context:** Python edit scripts over rustfmt-formatted Rust; `objectflow-service`'s unit tests; 2026-10-14.

### A scripted edit of several files makes every assertion before its first write
- **Pattern:** Moving two examples out of `service.examples.yaml` into a file of their own, a script wrote the trimmed file, then asserted the slice it would copy into the new one; the assertion failed, so the examples were gone from the first file and never written to the second. Git had the original, so nothing was lost, but only because the file was otherwise unmodified.
- **Correction:** Compute every new text and make every assertion first, then write all the files at the end, in one block with nothing that can fail between the writes. Where a file already has uncommitted changes, copy it to the scratchpad before the script runs.
- **Context:** Scripted edits that move text between files, in any repository; found 2026-10-14.

### Clippy runs as CI runs it, and is read before the gate starts
- **Pattern:** Clippy and `scripts/check-all` were started in one background command, clippy without `-D warnings`. Only the gate's verdict was read, so two `needless_borrow` warnings went unseen until the gate failed on them 20 minutes later.
- **Correction:** Run clippy on its own, with `-- -D warnings` as CI does, and read its output before starting the gate. A warning is an error the gate will find.
- **Context:** Rust work in this repository, `scripts/check-all`; found 2026-10-14.
