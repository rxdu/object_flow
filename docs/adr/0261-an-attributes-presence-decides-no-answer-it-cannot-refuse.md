# ADR-0261: An attribute's presence decides no answer it cannot refuse

- **Status:** **Decided** 2026-11-06 under the author's direction ("address the two follow-ups"), on the follow-up `TODO.md` recorded building issue #70, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-06
- **Refines:** ADR-0259 (a text's `not_blank` decides no answer it cannot refuse), whose reasoning this extends to one shape of declared invariant; ADR-0220 decision 1 (a type's invariants judged personal as a query's filter is); ADR-0206 decision 2 (`available`, `availability` and `check` refused `personal` where an invariant deciding the answer reads a personal member)
- **Relates to:** ADR-0260 decision 1 (the invariants a request may evaluate); ADR-0054 (an admission lets a violation stand); ADR-0146 decision 1 (a publish leaves no violation nobody admitted); ADR-0209 decision 5 (a plain read of an absent value clears an optional attribute); `flow-format.md` §4.8 and §4.17

## Context

The reimbursement sample's `Claim.has_title`, `title is not null`, makes every offer of `Claim` personal, as #70's generated invariant did. `title` is personal, and optional only so that erasure can remove it; `draft` writes it by a required input, and no other transition writes it. Probed 2026-11-06 on the debug service's sandbox with the sample loaded: without the opt-in, `available` on `Claim.withdraw`, `Claim.submit` and `Claim.add_line` was refused `personal` under 0.18.0's judgement, and each is answered with this decision.

ADR-0259 rejected judging every invariant by whether a write can make it fail. An arbitrary invariant compares what a request writes with what an object holds. The engine can show that no write fails an invariant only where what the inputs can write holds the invariant's own predicate.

`<attribute> is not null`, over one stored scalar of its type, is such an invariant:

- An invariant refuses a request only where it fails on an object the request writes, and an admission lets a failing one stand (ADR-0054). Every object a store holds satisfies it or holds it admitted (ADR-0146 decision 1). An erasure admits the invariants it breaks (`DESIGN.md` §8), each reading what it erases (`erasure_broke`).
- It reads the attribute alone, so it fails only where the request leaves the attribute absent.
- A stored scalar is written by its own type's transitions alone: by its input, by an `assign` to it, or by a `clear` of it (§4.8). A reference may also be written through its opposite end, by another type's `add` or `remove`, so a reference is left out.
- An input never writes it absent. One sent null is not supplied (§4.8), and a call or a creation passes no absent value (`arguments`, `request.rs`). An update leaving it out keeps the value held.
- An `assign` writes it absent where the value assigned is absent: a plain read of an absent value clears an optional attribute (ADR-0209 decision 5). An input a transition always takes is never absent: a declared input neither optional nor defaulted is refused `missing`, and an attribute input it requires is refused by its `<attribute>_provided`, in every request, call, creation and cascade (`take_unwound`).
- A default is a literal, a qualified value or `now` (§4.17), never absent.
- A creation's object holds nothing before it, and its inputs are its caller's. A creation leaving the attribute out is refused by `<attribute>_provided` or by the invariant, whatever any object holds.

Where an update may write the attribute absent, the verdict tells whether an admission stands, which only an absent value held allows. Where a creation's `assign` reads another object, the verdict tells whether what it read is absent, and that may be a personal member.

## Decision

1. **An invariant `<attribute> is not null`, over one stored scalar attribute of its type, decides none of `available`, `availability` and `check`** where no transition of the type may write the attribute absent from a value an object holds:
   - by a `clear` of it;
   - on an update, by an `assign` to it of anything but an input the transition always takes;
   - on a creation, by an `assign` to it of anything but an input.

   A `foreach`'s steps count.
2. **Where one may, it is judged by what it reads, as before, for every transition of the type.**
3. **It is known by its expression alone, whatever its name.** Any other expression, `<attribute> is null` and a disjunction among them, is judged as any invariant is, and so is one over a reference or a set.
4. **The guard stays the core's alone** (ADR-0220 decision 4).

## Alternatives rejected

- **Judging every invariant by whether a write can make it fail.** ADR-0259's reason stands: an arbitrary invariant compares what a request writes with what an object holds, the inference the audit's R15 recovered by halving.
- **Leaving every such invariant out, whatever the writes.** Where an update may clear the attribute, the verdict tells whether an admission stands, as the context shows.
- **Counting a creation that may leave the attribute out**, as an update that may clear it is counted. `Claim.draft` takes the title as required, so the sample would be answered either way. But a creation taking it as optional, refused by the invariant where its caller leaves it out, would make every offer of its type personal by nothing an object holds.
- **Applying the creation's reasoning to ADR-0259's text as well.** A creation's `assign` of an input not refused blank would then decide nothing, as one of an input does here. No flow has met that case, and the marking's own input already covers a creation taking the text. ADR-0259's rule is left as decided.
- **Recognising more shapes**, such as `this.<attribute> is not null` or a conjunction of presences. Nothing needs them yet; each would be judged as any invariant is, as before.

## Consequences

- `model.rs`: `Type::presence_of` knows the invariant by its expression. `request.rs`: `may_write_absent` judges the type's writes, by `always_given` for an update's inputs. `reads.rs`: `transition_reads_personal` leaves the invariant out by decision 1, beside ADR-0259's.
- `wire.md` states the rule beside ADR-0259's.
- **It refuses nothing 0.18.0 answered.** A read 0.18.0 refused `personal` is answered where decision 1 holds, as the sample's `Claim` reads are.
- **A value an effect reads from a personal member**, passed to a call's or a creation's input, can decide an answer that no guard or invariant judged personal reads. That limit holds for this decision as for ADR-0259 and ADR-0260, and `TODO.md` records it for the author.
- **Tested.** `a_personal_attributes_presence_decides_no_offer_it_cannot_refuse`, on the library:
  - written by inputs alone, an `assign` of an input always given among them, and erased: no transition is personal;
  - a creation assigning an input it may leave out, and an update assigning an attribute input it requires: not personal;
  - a `clear`, an update's `assign` of an optional or a defaulted input, a creation's `assign` of another object's member, and a `foreach`'s `assign`: every transition of the type is personal;
  - `is null`, a disjunction and a reference: judged by what each reads;
  - an attribute not personal: not personal, as before;
  - a subtype holding the invariant it inherits: not personal.

  `a_personal_attributes_presence_leaves_its_offers_unopted`, on the service's sandbox, so on both backends:
  - `availability`, `check` and `available` answered without the opt-in;
  - a creation leaving the title out refused, checked or requested, by `title_provided` where it requires the title and by `has_title` where it takes it as optional, and nothing made;
  - an erasure removing the title satisfied, and the erased claim's `close` checked and taken;
  - and a type whose update may write the title absent refused `personal`, and answered with the opt-in.

  Twelve planted defects were each caught: the invariant judged as before; every such invariant left out; a reference taken as a stored scalar; `is null` taken as `is not null`; a `clear` not read; an `assign` not read; an update's every input taken as given; a creation's `assign` of any value taken as an input's; a `foreach`'s steps not read; a defaulted input taken as given; an optional attribute input taken as given; and a creation's inputs judged as an update's. The first fails the service's test too.
