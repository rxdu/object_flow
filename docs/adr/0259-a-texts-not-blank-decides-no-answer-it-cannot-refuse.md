# ADR-0259: A text's `not_blank` decides no answer it cannot refuse

- **Status:** **Decided** 2026-11-06 under the author's direction ("review and address issue #70"), on the recommendation formed the same day for issue #70, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-06
- **Refines:** ADR-0220 decision 1 (a type's invariants judged personal as a query's filter is); ADR-0206 decision 2 (`available`, `availability` and `check` refused `personal` where an invariant deciding the answer reads a personal member)
- **Refined by:** ADR-0261 — its reasoning extended to a declared invariant `<attribute> is not null` over a stored scalar, which no input writes absent; a creation's inputs counted as its caller's.
- **Relates to:** ADR-0257 decisions 2 and 3 (an input refused blank, and the invariant `<attribute>_not_blank`); ADR-0146 decision 1 (a publish leaves no violation nobody admitted); ADR-0054 (an admission lets a violation stand); ADR-0240 decision 5 (`availability` refused where any offer of the object is decided by a personal member); issue #70

## Context

Issue #70, filed 2026-11-06 at 0.17.0, reported that marking a personal `string` attribute `not_blank: true` makes `availability` refuse every object of its type without `reads_personal`. An application showing a record's offers to people who may read the record but not its personal texts then offers them nothing on it.

Reproduced 2026-11-06 on the core as 0.17.0 built it. A type whose personal text is taken by its own input, marked `not_blank`, had each of its transitions judged personal (`a_personal_texts_not_blank_decides_no_offer_it_cannot_refuse`, `Visit`). Its `availability` was refused `personal` (`a_personal_text_marked_not_blank_leaves_its_offers_unopted`). Both tests pass with this decision.

`transition_reads_personal` (`reads.rs`) counts every invariant of the type that reads a personal member (ADR-0220 decision 1), and `availability` asks it of each transition of the object's type and of each creation offered on it. The generated `<attribute>_not_blank`, `<attribute> is null or trim(<attribute>) != ""`, reads the text, so every transition of the type became personal.

That invariant decides nothing a value held can tell, where every write of the text is by an input refused blank:

- An invariant refuses a request only where it fails on an object the request writes, and an admission lets a failing one stand (ADR-0054).
- Every object a store holds satisfies `<attribute>_not_blank` or holds it admitted (ADR-0146 decision 1, ADR-0257 decision 3).
- It reads the text alone, so it fails only where the request writes a blank text.
- A text is written by its own type's transitions: by its input, or by an `assign` to it (`flow-format.md` §4.8). On its creation a default or an identifier may write it, which no object held decides.
- An input refused blank, its own or one marked `not_blank` (ADR-0257 decision 2), writes no blank text.

Where a write may be blank, the invariant's verdict tells a value held. Given a blank text from an input not refused blank, a request is refused unless an admission stands, which a blank text held alone allows. Given another value, the verdict tells whether that value is blank, and it may be read from a personal member.

## Decision

1. **The invariant a `not_blank` generates, `<attribute>_not_blank`, decides none of `available`, `availability` and `check`** where no transition of the type may write the text blank: each writes it by its input, or by an `assign` of an input, that is refused blank (ADR-0257 decision 2).
2. **Where one may, it is judged by what it reads, as before, for every transition of the type.** A transition may write it blank by its input or an `assign` of an input not refused blank, or by an `assign` of any other value, a `foreach`'s steps included.
3. **It is known by its name and its expression.** An invariant of its name that a subtype declares with another expression is judged as any invariant is.
4. **The guard stays the core's alone** (ADR-0220 decision 4).

## Alternatives rejected

- **Leaving every generated invariant out**, as the issue suggests. Where a transition writes the text from an input not refused blank, or from another value, the verdict tells a value held, as the context shows.
- **Judging the invariant by the transitions one request reaches**: the transition asked about, and each its calls, creations and cascades take on the type. It is more exact for `check` and `available`, but needs each request's reach. `availability`, the read the issue meets, asks of every transition of the type anyway.
- **Judging every invariant by whether a write can make it fail.** A declared invariant no write fails makes every offer personal as #70's did. The reimbursement sample's `Claim.has_title`, `title is not null`, does so: probed 2026-11-06, `available` on `Claim.withdraw` was refused `personal`, and answered with the invariant removed. The engine can show that no write fails an invariant only where a refusal at the input holds the invariant's own predicate, as the marking's does. An arbitrary invariant compares what a request writes with what an object holds, the inference the audit's R15 recovered by halving. `TODO.md` records the case.
- **Always sending `reads_personal`**, which the issue considered. It computes offers from personal values for readers not entitled to them, which ADR-0206 exists to prevent.

## Consequences

- `reads.rs`: `transition_reads_personal` leaves the invariant out by decision 1. `model.rs`: `Type::not_blank_of` knows it, by the expression the generator and it share. `request.rs`: `may_write_blank` judges the type's writes by `not_blank_input`, the rule the request refuses by.
- `wire.md` states the rule beside ADR-0220's.
- **It refuses nothing 0.17.0 answered.** A read 0.17.0 refused `personal` is answered where decision 1 holds.
- **Tested.** `a_personal_texts_not_blank_decides_no_offer_it_cannot_refuse`, on the library: a text written by its own input and by an input marked `not_blank` decides nothing, whatever else the type writes. A text written by an input not so refused, or by a value read in a `foreach`, makes every transition of its type personal. A text not personal decides nothing, as before. A subtype holding the marking it inherits decides nothing, and one declaring the invariant's name otherwise, or the text again unmarked, is personal. `a_personal_text_marked_not_blank_leaves_its_offers_unopted`, on the service's sandbox, so on both backends: `availability`, `check` and `available` answered without the opt-in; a blank text refused `invalid_input`, `malformed`, checked or requested, and nothing written; and a type whose transition writes the text from an input not refused blank refused `personal`, and answered with the opt-in. Six planted defects were each caught: every generated invariant left out, the invariant known by its name alone, an input of the text's name not judged, an assigned input taken as refused blank whatever its marking, a `foreach`'s steps not read, and an `assign` judged whatever it writes. The service's test fails on the judgement before, at `availability`.
- **Two defects found building it** are recorded in `TODO.md` for the author: a request's call into another type's transition is not judged by that transition's guards, and a subtype may replace the generated invariant.
