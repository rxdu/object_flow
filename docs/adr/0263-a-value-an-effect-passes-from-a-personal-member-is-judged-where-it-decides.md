# ADR-0263: A value an effect passes from a personal member is judged where it decides

- **Status:** **Decided** 2026-11-06 as recommended, under the author's direction ("go with your recommendation on the new finding"), on the recommendation put the same day in `TODO.md`. Decision 3, a call's target and a `foreach`'s array, was found while deciding it and is accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-06
- **Refines:** ADR-0260 decision 3 (an input a guard reads judged by the guard's own transition); ADR-0220 decision 2 (an input judged by its transition's declaration); ADR-0261 (a creation's inputs are its caller's)
- **Refined by:** ADR-0264 — decision 2 fails closed: an input passed a personal value decides nothing only where it is optional, not `not_blank`, and of a kind every value fits; an argument computed from one, and a write of one, make the answer personal.
- **Relates to:** ADR-0206 decision 2 (`available`, `availability` and `check` refused `personal` where a personal member decides the answer); ADR-0259 (a text's `not_blank`); the model's check 10 (a personal value reaches only a personal attribute)

## Context

ADR-0260 judges `available`, `availability` and `check` by every guard and invariant a request may evaluate. It judges an input a guard reads by the guard's own transition (decision 3), and it judges no input's own check. A value a caller passes in is therefore never judged by where it came from.

Probed 2026-11-06 on the debug service's sandbox, on 0.18.0's code. `Desk.pin`, `Desk.secret`, `Desk.buddy` and `Desk.nicks` are personal; `Staff`'s inputs are not. Without the opt-in:

- **A guard on a passed input.** `Desk.reward` calls `Staff.promote`, passing `pin` to its input `amount`, which the guard `inputs.amount < 100` reads. `check` of `Desk.reward` answered `satisfied` for a pin of 3 and `call_refused` for 500.
- **An input's own check.** `Desk.tell` calls `Staff.jot`, passing `secret` to its input `text`, marked `not_blank`. `check` answered `satisfied` for a secret `kept` and `call_refused` for a blank one.
- **A call's target.** `Desk.ping` calls `poke` on the personal `buddy`, refused by a guard on `Staff`'s own `busy`. `check` answered `call_refused` with a buddy and `satisfied` without one.
- **A `foreach`'s array.** `Desk.sweep` steps over the personal `nicks` with a limit of 2. `check` answered `satisfied` for two nicks and `over_limit` for three.

The model's check 10 follows a personal value through a call to the attribute it writes, and refuses one written into an attribute that is not personal. So an invariant over what the value writes reads a personal attribute, and ADR-0260 judges it. The value decides the answer unjudged only through an input a guard reads, an input's own check, or what a step reads to choose what it reaches. ADR-0259's `not_blank` and ADR-0261's presence each rest on an input's check, so each was open to the same leak through a creation or a call.

## Decision

1. **An input that a call, a creation or a cascade of the request passes a value read from a personal member is personal in the transition it takes, to the end.** The guards reading it are judged so, and an argument reading it passes it on. Each argument is judged as its own transition's expressions are (ADR-0220): a member by its type, an input by its declaration, and an input passed such a value is personal too.
2. **Such an input makes the answer personal where its own check may refuse its value.** That is:
   - its absence, where the transition always takes it;
   - a blank, where it is marked `not_blank`;
   - no object of its type, where it is a reference;
   - another media type, where it is a file with a list;
   - its digits, where it is a decimal or money.

   An input no check refuses by its value, such as an optional text, decides nothing.
3. **A call's target, a supersession's successor and a `foreach`'s array, range and filter, read from a personal member, make the answer personal.** Which object a call reaches, and which items a `foreach` takes, decide what is taken and refused.
4. **A creation the request takes is judged by an attribute's presence invariant** (ADR-0261) where it writes the attribute from an input passed a personal value. Its inputs are then not its caller's alone.
5. **The guard stays the core's alone** (ADR-0220 decision 4).

## Alternatives rejected

- **Every argument reading a personal member makes the answer personal.** It is simple and fails closed. But the reimbursement sample's `Claim.add_line` passes its personal input `purpose` on to the line it creates, so every offer of `Claim` would be personal again, as issue #70 reported of a marked text.
- **Leaving it, stated in `wire.md` as the guard's limit.** The four probes each recover a personal value from the answer, which ADR-0206 exists to prevent.
- **Judging the guards that read a passed input, and not its own checks.** `Desk.tell`'s leak is a `not_blank` refusal, which no guard reads.
- **Following a passed value into the attributes it writes.** Check 10 makes each of them personal, so an invariant over one is judged already.

## Consequences

- `reads.rs`: `reaches` walks what a transition's steps and cascades reach, which `taken_by` takes and `passed_personal` follows. `Personal` counts each input passed a personal value. `transition_reads_personal` judges decisions 1 to 4. `request.rs`: `refused_by_value` and `writes_from_input`.
- `wire.md` states the rule beside ADR-0260's.
- **It refuses what 0.18.0 answered without the opt-in**, a read decided by a value passed from a personal member in one of these ways. The Flow Studio's server sets the opt-in on what it forwards (ADR-0206), so it answers as before. The reimbursement sample's `Claim` reads are answered without the opt-in, as ADR-0261 made them: probed on the sandbox, `available` of `withdraw`, `submit`, `add_line`, `approve` and `erase_personal_data`.
- **Tested.** `a_value_passed_from_a_personal_member_is_judged_where_it_decides`, on the library:
  - each guard on a passed input, and each of the five checks, is personal;
  - two hops are personal, the second through a type the walk reads before its caller;
  - a call's target, a `foreach`'s array, a pass inside a `foreach`, a creation's presence and a cascade's argument are personal;
  - a personal value reaching an input nothing refuses, a value not personal, a `foreach` over a set not personal, and a creation passed a literal are not.

  `a_check_a_personal_value_passed_on_decides_is_opted_into`, on the service's sandbox, so on both backends: the four probes are refused `personal` without the opt-in and answered by the value with it, and a value passed that is not personal is answered without it. Fifteen planted defects were each caught, one for each part of decisions 1 to 4, and for every argument or every passed input taken as personal.
