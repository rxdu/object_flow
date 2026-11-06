# ADR-0260: A transition's answer is judged by all its request may evaluate

- **Status:** **Decided** 2026-11-06 as recommended, under the author's direction ("go with your recommendations"), on the recommendation put the same day in `TODO.md` once building #70 found the defect. Decision 1's third part, an invariant of another type, was found while building the recommendation and is accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-06
- **Refines:** ADR-0220 (a transition's guards and its type's invariants judged personal by type); ADR-0206 decision 2 (`available`, `availability` and `check` refused `personal` where a guard or an invariant deciding the answer reads a personal member)
- **Refined by:** ADR-0263 — decision 3's input is personal too where the request passes it a personal value, and a call's target, a supersession's successor and a `foreach`'s array read from one make the answer personal.
- **Relates to:** ADR-0259 (the invariant a `not_blank` generates); `design/audit/personal-data.md` R15 and R17; `DESIGN.md` §5.6 (the invariants a request checks)

## Context

ADR-0206 decision 2 refuses `check` and `availability`, without `reads_personal`, where a guard or an invariant deciding the answer reads a personal member. `transition_reads_personal` (`reads.rs`) judged the transition's own guards and its own type's invariants alone. A request evaluates more:

- the guards of each transition a `call` or a `create` in its effect takes, and of each its type's cascades take, to the end;
- the invariants of each object those write;
- and the invariants of each object of another type that reaches what they write, which `affected_by_writes` finds along the relationship ends the invariant reads (`DESIGN.md` §5.6).

Probed 2026-11-06 on the debug service's sandbox, on the code of 0.17.0 with ADR-0259:

- **A call.** `Desk.reward` calls `promote` on the desk's owner, a `Staff` whose `promote` is guarded by `salary < 100`, `salary` personal. Without the opt-in, `check` of `Staff.promote` was refused `personal`. `check` of `Desk.reward` was answered: `satisfied` for an owner paid 50, `call_refused` for one paid 500.
- **Another type's invariant.** A `Team` holds `sum(m in members: m.salary) <= budget`, its budget 1000, and two members are paid 700 and 100. Without the opt-in, `check` of `Member.raise` for the second was `satisfied` up to a raise to 300 and `invariant_violated` from 301. Halving the input recovers the first member's salary, the inference the audit's R15 found.

`availability` and `available` ask the same function. `availability` simulates a request where an offer is decided for who asks, which takes its calls (`main.rs`).

## Decision

1. **`available`, `availability` and `check` judge a transition by every guard and invariant its request may evaluate:**
   - each guard of each transition it may take: itself and, to the end, each a `create` or a `call` of an effect takes, a `foreach`'s steps included, and each a cascade of a composite end on it takes;
   - each invariant of each type those transitions write, the invariant a `not_blank` generates judged as ADR-0259 says;
   - each invariant of another type that reaches an object of those types and does not read its own object alone. It reaches a type the request updates along any chain of relationship ends it reads. It reaches a type the request only creates along an end whose opposite the new object holds, since no other end refers to a new object. It reaches any type declaring a reference that may hold the referred object along a referrer's end. An invariant reading what no chain tells, such as a type scan, reaches every type.
2. **A call's target is taken by the type its expression tells**, a reference's or a `foreach` item's, as its family's transition of the name. A target whose type the expression does not tell, such as an input, is taken as every type's transition of the name (fail closed), as `check_batch` judges a target the store does not hold.
3. **An input a guard reads is judged by the guard's own transition**, and one an invariant reads by the transition requested (ADR-0220 decision 2).
4. **A uniqueness's key is not counted.** Its refusal names no holder without the opt-in (ADR-0206 decision 3).
5. **The guard stays the core's alone** (ADR-0220 decision 4).

## Alternatives rejected

- **Following a call by the type of the object it reaches at run time.** The guard runs before anything is evaluated (ADR-0206 decision 2), so it reads no object to decide.
- **Counting every invariant of every type a written object reaches, its own-object invariants included.** On an object the request does not write, such an invariant holds or stands admitted, so it decides nothing. Counting it would refuse every transition of a type another type's invariant reaches.
- **Judging another type's invariant by whether the request writes a member it reads.** It is more exact, but needs each write's members through every call, creation and cascade. ADR-0259 rejected judging every invariant by what a write can make fail, for the same reason.
- **A plain reference reaching an object the request creates.** No object refers to a new one but through an end the new one holds, so a creation is reached through an opposite end alone. This keeps ADR-0220's `Contact.add` answered, where a locker's invariant reads its contact's personal name.

## Consequences

- `reads.rs`: `transition_reads_personal` takes each transition `taken_by` finds and judges their guards and their types' invariants. `request.rs`: `invariants_reach` and `reads_its_object_alone`, over the reach `affected_by_writes` reads.
- `wire.md` states the rule beside ADR-0220's.
- **It refuses what 0.17.0 answered without the opt-in**: a read of a transition whose request may evaluate a personal member through a call, a creation, a cascade or another type's invariant. The Flow Studio's server sets the opt-in on what it forwards (ADR-0206), so it answers as before.
- **Tested.** `a_transitions_answer_is_judged_by_all_its_request_may_evaluate`, on the library:
  - a call on a reference's family, in a `foreach` and on an input's target is personal, and a call into a guard on no personal member is not;
  - a creation of a type whose invariant reads one, and a cascade into a part's guard reading one, are personal;
  - another type's invariant over a personal member is personal where it reaches a member updated, and one created holding the end's opposite;
  - a plain reference reaches a guest updated, not one created;
  - an invariant reaching what is written and reading nothing personal, beside one of its type reading its own object alone, decides nothing;
  - a referrer's end reaches a type holding a reference to the referred type, and no other.

  `a_check_is_judged_by_all_its_request_may_evaluate`, on the service's sandbox, so on both backends: the two probes refused `personal` without the opt-in and answered as before with it; `availability` of the desk refused; and a check deciding by no personal member answered. Twelve planted defects were each caught, one for each part of decisions 1 and 2.
