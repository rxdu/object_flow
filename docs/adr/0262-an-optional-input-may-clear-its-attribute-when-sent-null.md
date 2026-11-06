# ADR-0262: An optional input may clear its attribute when sent null

- **Status:** **Decided** 2026-11-06 under the author's direction ("a new issue was filed, read and address properly"), on the recommendation formed the same day for issue #71, accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-06
- **Refines:** ADR-0122 decision 10 (a null is absence; only `clear` removes a value)
- **Relates to:** ADR-0122 decision 11 (a step is never conditional on an input); ADR-0257 (a text never blank); ADR-0261 (an attribute's presence); ADR-0209 decision 5 (a plain read of an absent value clears); [`clearing-an-optional-value.md`](../research/clearing-an-optional-value.md); issue #71

## Context

Issue #71, filed 2026-11-06 at 0.18.0, reported that one transition cannot clear an optional attribute, so an edit form showing an optional field cannot say "no value now". An optional input sent as null is not supplied (ADR-0122 decision 10), and since ADR-0257 a text marked `not_blank` cannot stand in with a blank. Decision 11's answer is two transitions, one that sets and one that clears.

Reproduced 2026-11-06 on the debug service's sandbox, on a flow of the repository's own (the research note gives the table). `edit`, taking `note` and `marked` as optional inputs, was answered `satisfied` for `note: null` and `marked: null` and changed nothing. `marked: ""` was refused `malformed`. Only a transition whose effect is `clear: [note]` cleared it.

Two transitions cost each such field one more offer beside the edit, and two requests and two events for an edit that changes one field and clears another. The form must also know which field maps to another transition, which no offer says.

Established update interfaces tell "leave it" from "clear it" in one request ([`clearing-an-optional-value.md`](../research/clearing-an-optional-value.md)). In JSON Merge Patch a member given null removes the value and one left out is unchanged. GraphQL holds "a semantic difference between the explicitly provided value null versus having not provided a value". JSON:API keeps an attribute an update leaves out, and Google's update mask names the fields an update writes.

## Decision

1. **A transition from a state may list in `clearable_inputs` the optional inputs a null clears.** Given as null, each clears its attribute as `clear` does, a write its event records. Left out, the attribute keeps its value, and given a value, it is written as before.
2. **Each listed is one of `optional_inputs`, and its attribute one `clear` may clear**: an optional, singular attribute or stored reference that is no composite end, which no state the transition leaves the object in requires. A creation lists none, having nothing to clear. Both checkers refuse any other at step 3, `names`.
3. **Every other input keeps decision 10**: sent null, it is not supplied.
4. **A call, a creation or a cascade passes no absent value**, as before. So a request clears through a clearable input, and an effect clears by `clear`.
5. **`availability` names the clearable inputs** of each offer, as `clearable`. A transition's tool takes null for each, and `declaration` shows the list as the module writes it.
6. **Decision 11 holds.** The clear is the input's own write, as its value's would be, not a step conditional on the input.
7. **A clearable input may write its attribute absent** (ADR-0261), so a presence invariant over it is judged by what it reads.

## Alternatives rejected

- **Two transitions, as decision 11 says.** The issue lists what it costs a platform for each field: an offer beside the edit, two requests and two events for one edit, and a form that maps fields to another transition.
- **A null clearing every optional input**, as Merge Patch does for every member. It would change what every flow's transitions write: a null is not supplied today, and a writer sending null for an empty field would start clearing. The flow's author says which act may clear.
- **A list of attributes to clear in each request**, as Google's update mask names the fields an update writes. A caller could then clear what the transition was not declared to clear.
- **Marking the attribute clearable** for every transition taking it. What an act writes is the transition's to declare, as `optional_inputs` is.
- **A blank text standing for "none".** It needs the text unmarked, so every writer may write a blank, which `not_blank` exists to prevent.

## Consequences

- **The core.**
  - `model.rs`: `Transition::clearable_inputs`.
  - `request.rs`: a clearable null is kept as an absent input, and the attribute is removed where it is written. An absent value is not read as a reference.
  - `reads.rs`: `Offer::clearable`, and `may_write_absent` counts a clearable input.
  - `tools.rs`: a clearable input takes null.
  - The service renders `clearable` in each offer.
- **The checkers.** Both hold decision 2 at step 3. Both checkers' example checks take an input sent null as not supplied, where a null reference was read as an alias.
- **The reference runner** takes an input sent null as not supplied, a clearable one as a clear, as the core decodes it. The Flow Studio's sandbox over it names each offer's `clearable`, and the reference tools (`flowtools.py`) take null for each, so the Studio's tests find both backends alike. Before, it read a null reference as an alias and refused it `no_such_object`, where the core took it as not supplied. That divergence was never met, since no example sent a null input.
- `flow.schema.json`, `flow-format.md` §4.8 and §10, the format's card and `wire.md` state the key. The issues sample's `edit` clears an assignee, a due date and an epic, with two examples.
- **It is additive.** A module without the key behaves as before, and format 1's frozen corpus is unchanged.
- **Tested.**
  - `a_clearable_input_given_null_clears_its_attribute`, on the library, so on both backends:
    - a text, a text marked `not_blank` and a reference cleared, the event recording the write;
    - a value left out kept, and a blank still refused;
    - a transition not listing the input keeping the value given null;
    - a call passing an absent value clearing nothing.
  - `a_clearable_input_sent_null_clears_its_attribute`, on the service's sandbox, so on both backends:
    - each offer's `clearable`;
    - a clear recorded in the history;
    - a value left out kept;
    - a tool taking null for the clearable input alone;
    - a module listing a non-optional input refused at load.
  - The issues sample's two examples, run by the reference runner and by the core.
  - Five plants both checkers agree on.
  - Seventeen planted defects were each caught: every null kept, or none; a clear not written; a cleared reference judged as one; the offer naming none; the tool taking no null; the presence rule ignoring the input; each of the four checker rules in each checker; and the runner keeping no null, or writing no clear.
