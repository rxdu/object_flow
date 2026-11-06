# ADR-0264: A personal value passed on decides nothing only where nothing can refuse it

- **Status:** **Decided** 2026-11-06 under the author's direction ("go with your recommendation on the new finding"), whose option 2 judges an input passed a personal value "by its own checks": this makes that judgement fail closed. Found the same day by an automated security review of `041da36`, and accepted in advance as the author directed for a confident recommendation. Built the same day.
- **Date:** 2026-11-06
- **Refines:** ADR-0263 decision 2 (an input passed a personal value, judged by its own checks)
- **Relates to:** ADR-0260 decision 2 (a call's target the expression does not tell, taken fail closed); ADR-0209 decision 5 (a plain read of an absent value clears); the model's check 10

## Context

ADR-0263 decision 2 counts an input passed a personal value where its own check may refuse that value. `refused_by_value`, as `041da36` built it, named five checks: absence, `not_blank`, a reference's object, a file's media type and a decimal's digits. An automated security review of that commit found that a value may be refused in other ways, so an answer can still tell it:

- **A text past its bound.** The wire refuses a string past 64 KiB `out_of_range` (`wire.md`). The review raised a concatenation of personal texts passing the bound in an effect; whether the effect refuses it was not probed, and decision 2 counts such an argument either way.
- **An argument's arithmetic.** An argument is evaluated in the caller's effect, and arithmetic past a type's bound is refused `value_in_range` there (`wire.md`, Values), whatever the input it is passed to.
- **A write of one.** An `assign`, an `add` or a `remove` of a value read from a personal member is refused `value_in_range` where its arithmetic passes a bound, where it is absent and its attribute is not optional, or where it does not fit the attribute, by its digits, its calendar or its kind. Check 10 makes the attribute personal, but an invariant over it is not what refuses.

Naming each check that may refuse leaves the next one out. ADR-0260 decision 2 takes a call's target fail closed where the expression does not tell its type, for the same reason.

Building it, the full gate found that a write rule judging inputs by their declaration took ADR-0259's case back: `Visit.annotate` assigns `aside` from its own input `why`, marked `not_blank`. ADR-0220 decision 2 judges `why` personal, since it is written into a personal attribute, and `aside`'s `not_blank` refuses a blank. So `annotate` was personal again (`a_personal_texts_not_blank_decides_no_offer_it_cannot_refuse`). The value is the requester's own, so decision 4 judges such an input personal only where the request passes it a personal value.

Measured 2026-11-06 on the debug service's sandbox, every sample module of `docs/design/flow-format/examples/` loaded with what it imports: 13 modules and 195 transitions, each asked of `available` without the opt-in. None was refused `personal` under `041da36`, and none is under this decision.

## Decision

1. **An input passed a personal value decides nothing only where nothing can refuse it.** It is optional, not marked `not_blank`, and of a kind every value fits: one string, integer, boolean, date, timestamp or enumeration member. The checker holds the passed value's kind to the input's (`argument_type_errors`). Every other input passed a personal value makes the answer personal, a reference, a file, a decimal, money, a value of several fields and a set among them.
2. **An argument computed from a personal member makes the answer personal.** Only a plain read, a name or a path of them, evaluates without refusing, since an absent one is not passed.
3. **A write of a value read from a personal member makes the answer personal**, by an `assign`, an `add` or a `remove`. The exception is an `assign` of a plain read to an optional attribute, not marked `not_blank`, of a kind every value fits: an absent value clears it (ADR-0209 decision 5), and any other is written whole.
4. **In decisions 2 and 3, an input is personal only where the request passes it a personal value** (ADR-0263 decision 1). The request's own inputs are its caller's, and a refusal by one tells the caller nothing held. ADR-0220 decision 2 still judges an input a guard reads, or one passed on, by its declaration.

## Alternatives rejected

- **Naming each check that may refuse**, as `041da36` did. Its five checks missed a text's bound, an argument's arithmetic and a write's fit, so each check would have to be found before it leaked.
- **Judging the passed value's source kind beside the input's.** The checker already refuses a value of another kind where both kinds can be told. A plain read of the input's kind is what this decision lets through, so the source's kind adds nothing.
- **Every effect expression reading a personal member makes the answer personal**, ADR-0263's first option. The reimbursement sample's `Claim.add_line` passes a personal text plainly to an optional text, which nothing refuses, so every offer of `Claim` would be personal, as issue #70 reported of a marked text.

## Consequences

- `request.rs`: `refused_by_value` fails closed by `takes_every`, and `takes_every_read` judges an `assign`'s attribute. `reads.rs`: each `Reach` carries an effect's write; `transition_reads_personal` judges decisions 2 and 3, by `plain`, and decision 4 by `Personal::passed_only`.
- `wire.md` states the rule beside ADR-0263's.
- **It refuses what `041da36` answered without the opt-in**, which no release carried: a read decided by a personal value passed to an input that may refuse it, computed in an argument, or written. The sample modules' 195 transitions are answered as before.
- **Tested.** `a_value_passed_from_a_personal_member_is_judged_where_it_decides`, on the library:
  - an argument computed from a personal value into an input taking every integer, a doubled one assigned, one assigned to a decimal of fewer digits, one added to a set, and a write computed from an input the request passes one are personal;
  - a plain read into that input, one assigned to an optional text, and a write computed from the request's own input are not.

  `a_personal_texts_not_blank_decides_no_offer_it_cannot_refuse` holds `Visit.annotate` unopted, as ADR-0259 left it.

  `a_check_a_personal_value_passed_on_decides_is_opted_into`, on the service's sandbox, so on both backends: a pin multiplied past the integer's bound, satisfied for 3 and refused `value_in_range` for 500, is refused `personal` without the opt-in, and a plain read into the same input is answered. Nineteen planted defects were each caught, ADR-0263's and these: each kind taken as fitting, every passed input taken as refused, a computed argument not judged, a write not judged, every personal write taken as refused, and a write's inputs judged by their declaration.
