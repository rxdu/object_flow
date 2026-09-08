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

