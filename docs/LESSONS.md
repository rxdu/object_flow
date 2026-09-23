# Lessons

Operational lessons from working on this project. See [`adr/`](adr/) for design decisions and [`../TODO.md`](../TODO.md) for open work.

### Design detail outran the stated goal

- **Pattern:** During initial ideation, design decisions were generated at a rate faster than the objective was established — actor models, delegation chains and proposal semantics were proposed before it was confirmed whether the project was internal tooling, a released library, or an open-source protocol. Those three imply different first versions, and the work rested on an unverified reading of the motivation. It took two explicit interventions ("I wonder if you're clear about what I want first") to surface it.
- **Correction:** In open-ended design, state the understood objective back explicitly before extending the model, and separate what was actually said from what was inferred. Structural decisions that follow from the stated model are safe to continue; decisions that depend on organisational context are not, and should be flagged as resting on assumptions rather than presented alongside the rest.
- **Context:** Early-stage design discussion, no code. Applies to any project where the framing is still being negotiated.

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
