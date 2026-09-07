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
