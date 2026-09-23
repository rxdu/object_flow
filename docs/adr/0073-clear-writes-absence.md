# ADR-0073: `clear` writes absence, and is the only way to

- **Status:** **Accepted** — confirmed by the author 2026-09-08, having been found by re-expressing the case studies
- **Date:** 2026-09-08
- **Refined by:** ADR-0099 — `clear` also writes absence to an optional, singular, stored `ref` end.
- **Refines:** ADR-0052, ADR-0053

## Context

`docs/DESIGN.md` §5.4 has said since the repair that "clearing a value deliberately is a separate input or a separate action". The declaration syntax provides no spelling for it. An unsupplied optional input **skips** its write, which leaves the stored value alone; there is no literal `null` to assign, ADR-0053 having made a bare `null` in a comparison a publish error for good reasons; and iteration 15 removed a clause from check 8 precisely because it named a write the language could not express.

So the design document promised something the language did not have, and nobody noticed for six iterations because no example needed it.

Re-expressing the case studies in the current grammar found the case. `case-study-tickets.md` declares `Bug.reopen`, which must clear `resolution` and `resolved_at`: a bug moved back to in-progress that still reads `resolution = FIXED` is wrong in a way the whole model exists to prevent. It was written in the pre-syntax notation as `resolution := null`, which is why the gap survived — the notation was never checked against the language.

## Decision

An outcome step `clear <attribute>` writes absence to an optional attribute of `this`.

It is a write like any other: it stamps the attribute's last-written index, appears in the event, and invalidates an approval whose guard read that attribute. This matters, because the alternative reading — that clearing is a non-write — would let a reopened bug keep a standing approval that was granted against its resolution.

Publishing rejects a `clear` on a required attribute, on a counter, or on a relationship end (check 17). An optional relationship end is cleared by writing the reference itself, since a reference has a value to write and an attribute does not. *(Refined by ADR-0099 §6, repairing D249: there is no `null` to write and an unsupplied optional input skips its write, so an optional reference could never be cleared. `clear <reference>` now writes absence to an optional, singular, stored `ref` end, and is refused on a required end, a set, a `part` or an `owner` (check 17).)*

## Alternatives rejected

- **A `null` literal assignable on the right of a `set`.** Rejected: ADR-0053 made a bare `null` a publish error so that presence is always tested with `is null` and never compared, and admitting it on the write side would put the token back in the language for readers to confuse.
- **An optional input whose absence means clear.** Rejected because it collides head-on with ADR-0052: an unsupplied optional input already means "leave it alone", and one token cannot mean both. That collision is exactly what makes a partial edit safe today.
- **A separate transition per cleared field**, which is what "a separate action" in DESIGN.md could be read to mean. Rejected: it multiplies transitions for a reason that is not a lifecycle distinction, and `Bug.reopen` clears two fields at once.

## Consequences

- `docs/design/declaration-syntax.md` §5.2 gains the step, §9.5 the reserved word, and check 17 the rejection — **implemented**, with all five failure modes confirmed by probe: a required attribute, a counter, a relationship end, an undeclared name and a path. The clause was claimed and unenforced for the first hour of its existence, which is the defect class ADR-0062 is about, so it was checked before the ADR was confirmed rather than after.
- `docs/DESIGN.md` §5.4 stops promising a construct that did not exist and names this one.
- `case-study-tickets.md` is expressible, and it is the only document in the repository that needed it, which is a fair measure of how often clearing a value is the right answer.
