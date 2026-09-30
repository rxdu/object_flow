# The reimbursement worked example: a cold draft, and what it found

Status: **done**, 2026-09-30. The first consumer's first workflow, drafted in the flow format by an agent that had read only the language reference, as `authoring-flows.md` §6 prescribes, and judged by the design defects its draft revealed (`TODO.md`, "Write a reimbursement worked example"). The module and its examples are [`flow-format/examples/reimbursement.yaml`](flow-format/examples/reimbursement.yaml) and [`reimbursement.examples.yaml`](flow-format/examples/reimbursement.examples.yaml), kept as the agent wrote them.

## 1. How it was run

**The brief.** The operations platform's PRD (revision 17, a Claude document) gives the workflow in one row: "A structured claim with line items. It is reviewed, approved by a function, recorded in Xero by a person, read back to confirm, then paid. An agent suggests whether each item is an expense or a purchase." Its stage table gives why it goes first: "Nothing to migrate, no time slots to fight over; proves requests, lines, lifecycles and a verified Xero write". Two of its open questions were given to the drafter as undecided: whether a reimbursement is entered in Xero as a bill, a spend-money transaction or an expense claim, which decides whether "paid" is a step of its own; and which function does which step. The brief added PRD UC-23, an over-policy line seen and not refused, and UC-24, per-person figures behind the application's door. Reading further into the ops PRD was stopped by a permission check over personal data in its examples, so nothing else was taken from it.

**The drafter** was a fresh agent told to read `flow-format.md`, its two schemas and only the sections of other documents it cites, and not the example modules, `DESIGN.md`, the ADRs or the register, since copying an example would hide the gaps the test looks for. It could run the flow checker, and kept a log of every place the reference failed it. It changed nothing in the repository.

## 2. What it showed

The module, six types around a ten-state `Claim`, passed the flow checker on its first run, with twenty examples on seven setups that the reference runner runs, all passing. **That first pass is itself a finding**: several of the drafter's guesses passed without the reference confirming them (entries 17, 19 and 20), and in three places the checker accepted, or said, what the design does not (entries 18 to 20). Its log has twenty-three entries (§6). Each was checked against the design before it was classified:

| Entry | What the log says | Checked by | Disposition |
|---|---|---|---|
| 1 | The expression language is only in `declaration-syntax.md` §8 | `flow-format.md` §5 cites it by design | Kept: the format does not restate the model's expression language |
| 2 | What categories other than `closed` mean | `flow-format.md` §4.1 | D492, resolved in place |
| 3 | No date type; a day is bucketed by its UTC month | `declaration-syntax.md` §3.1, §6.9 | Question 2 for the author (§5) |
| 4 | One currency per attribute | ADR-0068, the author's ruling | Question 3 for the author |
| 5 | Free text is personal, so a required title is an optional attribute and an invariant | `flow-format.md` §4.13 | The pattern holds: erasure admits the invariant it breaks |
| 6 | Approval by a function cannot be enforced | ADR-0114 | By design: who may act is the upper layer's |
| 7 | A proposal cannot be written in an example | `flow-format.md` §11 | `TODO.md` |
| 8 | "The agent never commits a financial write" cannot be declared | ADR-0114's rejected alternatives name this very rule | By design |
| 9, 10, 11 | `warn` and `audit` read as disjoint; `.flagged` undefined; where a called transition's flag lands; a `warn` guard's remedy | `DESIGN.md` §5.5, §6 | D487, resolved in place |
| 12 | A guard reads the value from before the request | `flow-format.md` §7, notice `before` | By design; the drafter's full replacement is sound |
| 13 | Which figures name a person is only in the model | ADR-0114 decision 4 | D487, resolved in place |
| 14 | The quoting rule read literally forbids a cascade's inline list | `flow-format.md` §2.4 | D492 |
| 15 | The checker does not say whether examples ran | a run of the checker | D492 |
| 16 | Step 5 decides nothing about the ten-state `Claim` | the notice `undecided`, reproduced | Resolved the same day: step 5 sets aside what a state already settles, and decides all ten states (`flow-format.md` §7.1) |
| 17 | How an `identity` is written in an example | `flow-format.md` §11.3 | D492 |
| 18 | An unknown `warn` guard: the text says it refuses, the runner raises it | `DESIGN.md` §5.5; the drafter's probe, rerun | D488, resolved in place |
| 19 | A misspelt transition name in a filter counts nothing, silently | a probe: `t.transition == "confirm_natur"` passed | D490, resolved in place |
| 20 | `actor.kind` written to a `string`, and an invariant over it | probes: an enumeration's member written to a `string` passed too | D491 for the type check; question 1 for the author |
| 21 | `declaration-syntax.md` §8.2 shows a guard reading `actor` | the text | D489, resolved in place |
| 22 | Coverage counts only an example's request, not its setup | `flow-format.md` §11.5 says "requests" | By design |
| 23 | `--help` runs the self-tests | a run of the checker | D492 |

## 3. A finding the log does not have

`ClaimLine.over_policy` compares a line's amount with its category's `policy_amount` as it is now, so when finance changes a policy every past line's flag is recomputed, and `claimant_over_policy_lines` rewrites history, while the `warn` failures in the attempt log keep what was true when each line was entered. Two counts of the same thing then disagree. A derived attribute reading another object's current value is the model working as stated (`flow-format.md` §4.11); the lesson is for authors, that a figure which must not move with later edits is read from the attempt log or recorded on the object at the time, not derived.

## 4. What was changed

`flow-format.md` §2.4, §4.1, §4.8, §4.9, §5 and §11.3, and `declaration-syntax.md` §6.9 and §8.2, now say what the drafter could not find; the flow checker refuses a misspelt transition name in a metric and an assignment whose value does not fit its target, words the notice `unknown` by the guard's enforcement, prints how many examples ran, and answers `--help`. Each is recorded in `defects.md` as D487 to D492.

## 5. Questions for the author

1. **What type holds `actor.kind`, and may an invariant constrain it?** ADR-0114 lets an outcome write `actor.kind` into an attribute, but `declaration-syntax.md` §8.1 types it as "a fixed enum" with no name, so no attribute can be declared to hold it, and the drafter's `string` works only because the checker does not type it. With a type, an invariant over the recorded kind, "a line's nature is confirmed by a human", enforces the business's "the agent never commits a financial write" in the declaration. That has the shape of the rule ADR-0114 kept, one approval per person as an invariant over the recorded approver, and also of the rule its alternatives rejected. Recommendation: give the kind a built-in type, and allow the invariant, since it constrains what the record holds and the upper layer still decides who may ask.
2. **Should the model have a `date` type?** An expense has a calendar date, not an instant. Written as its UTC midnight it is bucketed into the right month only while every writer keeps that convention; an application that stores a local midnight as the instant it is, as Singapore's 1 March is 28 February 16:00Z, puts it in the month before. Recommendation: a `date` type, bucketed by its own calendar, with `timestamp` unchanged.
3. **Does ADR-0068 answer a claim line in any currency?** It declares one currency per attribute, and an account per currency as ledgers do; a claim line may be in whichever currency was spent. Recommendation: keep ADR-0068, and have a line carry the amount in the home currency, which is what is reimbursed, with the original amount and currency as recorded values the engine does not compute with.

## 6. The drafting agent's log, as it wrote it

Written by a first-time author who had read only `docs/design/flow-format.md`, the two schemas, and the sections of `declaration-syntax.md` that `flow-format.md` cites (§8 and §6.9, each logged below). No example module, ADR, `DESIGN.md` or checker source was opened.

What I built: six types. `Claim` has a 10-state lifecycle (DRAFT, SUBMITTED, RETURNED, REVIEWED, APPROVED, RECORDED, VERIFIED, PAID, REJECTED, WITHDRAWN). `ClaimLine` is a composite part of the claim. `ExpenseCategory` holds the policy amount. `BusinessFunction` is the approving authority. `Employee` and `AssistantAgent` are the human and agent actors. There is one evaluator (`xero`, used for the read-back and for payment) and six metrics.

Additions beyond the brief, with the reason for each:
- `merchant` and `purpose` on each line: a reviewer needs to know what the money was for. `purpose` is personal because it is free text.
- A separate `suggested_nature` (written by the agent) and `nature` (confirmed by a person): the suggestion then commits nothing.
- `return_reason` and `rejection_reason` as enumerations, so that returns and rejections can be counted.
- A `correct_xero_bill_id` correction, for when the read-back shows the wrong bill id was typed in.

Decisions on the brief's open questions:
- **Xero entries:** bills, paid separately (as instructed).
- **Home currency:** assumed USD. See entry 4.
- **Send back:** allowed from SUBMITTED and REVIEWED. The claimant then amends and resubmits.
- **Reject:** allowed from SUBMITTED and REVIEWED.
- **Withdraw:** allowed up to and including REVIEWED. After approval, withdrawing would need a void in Xero, which is finance's job, not the claimant's.

---

### 1. The expression language is entirely outside the reference
- **Kind:** left-the-reference
- **Where:** §5: "An `expression` is written in the expression language of `declaration-syntax.md` §8, which this format does not change."
- **What I needed / what happened:** Before writing the first condition I needed:
  - the aggregate form `agg(x in coll where f: body)`, which I could only guess from one fragment in §2.4;
  - whether `timestamp - timestamp` is allowed (for time-to-payment);
  - whether `in {A, B}` exists;
  - the literal forms.
  
  `flow-format.md` shows expressions only by example and has no grammar summary. I read `declaration-syntax.md` §8.1–§8.3.
- **What I did:** Wrote the expressions from §8.3. The checker accepted all of them on the first run. Confidence: high.

### 2. What categories other than `closed` are for
- **Kind:** silent
- **Where:** §4.1: "`categories` | the list of state categories the module's states use. `closed` is in every vocabulary whether or not it is listed, and has the meaning `declaration-syntax.md` §1 gives it".
- **What I needed / what happened:** The reference defines only `closed`. It does not say:
  - what other categories mean, or whether anything reads them;
  - whether listing `closed` explicitly is allowed or redundant.
- **What I did:** Guessed Jira-like categories (`to_do`, `in_progress`, `in_service`, `closed`). RETURNED shares `to_do` with DRAFT, so a reader can tell "waiting on the claimant" from "waiting on the company". I listed `closed` explicitly, and the checker accepted it. Confidence that the categories are used as intended: low.

### 3. No date type: an expense date is a UTC timestamp, bucketed by UTC month
- **Kind:** inexpressible
- **Where:** §4.3: "`type` | a built-in type (`string`, `bool`, `int`, `decimal(p,s)`, `money(ccy)`, `timestamp`, `duration`, `identity`, `file`, `event`)". The cited `declaration-syntax.md` §6.9 says time buckets are "in UTC calendar time whatever a session's time zone".
- **What I needed / what happened:** The business asked for "the date of the expense". That is a calendar date, not an instant. Stored as a timestamp, a local date east of UTC (for example 1 March in Singapore) becomes 28 February 16:00Z. Finance's "amount claimed per category per month" (`month(l.expense_date)`) then counts it in the wrong month.
- **What I did:** Typed `expense_date` as `timestamp`, documented it as "its midnight in UTC", and wrote example dates as `…T00:00Z`. The application has to convert. Confidence it is what the reference intends: medium. It is a real correctness gap for month-end figures.

### 4. One currency per attribute: foreign-currency lines cannot be represented
- **Kind:** inexpressible; also left-the-reference to find out why
- **Where:** §4.3 gives only `money(ccy)`. The reason (the currency is fixed at declaration, ADR-0068) is in `declaration-syntax.md` §8.3 "Money".
- **What I needed / what happened:** A robotics company's travel claims include lines in other currencies. With `amount: money(USD)` a line cannot carry its original currency.
- **What I did:** Assumed the claimant enters the home-currency amount as charged, and wrote this assumption into the module header. Another option was to add `original_amount: decimal` and `original_currency: string`, but I did not want untyped currency codes in the model. Confidence: medium.

### 5. Free text is personal, so a claim's title cannot be required
- **Kind:** contradictory (between the business need and the format), unclear
- **Where:**
  - §4.13: "Because an erasure writes absence, a `personal` attribute or field MUST be optional".
  - §4.13 (assertions): "an explanation in words is a further input, optional and `personal`, since free text may name someone".
- **What I needed / what happened:** The reference's own convention is that free text may name someone, so the title ("Dinner with Jane from Acme") is personal. A personal attribute must be optional, but every claim needs a title.
- **What I did:** Declared `title` as `optional: true, personal: true`, required it as an input of `draft`, and added an invariant `has_title: title is not null`. §4.13 says an erasure "admits every invariant that reads what it erases", so I expect erasure to pass. The example `erase_paid_claim` applies. The same pattern forced `Claim` and `ClaimLine` to carry an erasure transition, with the claim's erasure looping over its lines.

  Separately, §4.13 does not say whether a *declared input* may be personal and required. I made `return_for_changes.note` required and personal (a send-back without an explanation is useless), and the checker accepted it. Confidence that this is the intended pattern: medium.

### 6. "Approved by a function": the format has no roles or teams
- **Kind:** silent
- **Where:**
  - §4.3: "`assignee` | `true` on a reference that names who is responsible for the object".
  - §4.8 (proposable): "When to propose, and who may approve, are the application's (ADR-0114)".
  - §5: "who may act is decided outside the flow".
- **What I needed / what happened:** The business says approval is by a function (a role or team), not a named person, and which function reviews, records and pays is undecided. The reference says nothing about roles or functions. `assignee` points to `declaration-syntax.md` §6.10, which I did not follow.
- **What I did:** Declared a `BusinessFunction` type. `approve` requires an `approving_function` input, and a guard checks the function is active. Nothing can check that the person approving belongs to that function, because that is "who may act". Reviewing, recording and paying record no function at all. The event log records the actor, and the application decides who may take each step. Confidence: medium. This records the business's language but does not enforce it.

### 7. `proposable` looked like the construct for "the agent suggests, a person confirms", but cannot be shown or measured
- **Kind:** unclear
- **Where:** §4.8: "`proposable` | `true` if a request for the transition may be filed as a proposal instead, executed as its approver when approved (ADR-0036)…". The example keys in §10 have no proposal step.
- **What I needed / what happened:** Marking `confirm_nature` as proposable would let the agent file the classification and a person approve it. But:
  - the examples format cannot file or approve a proposal, so I could not write the "agent's suggestion" example that way;
  - a proposal's content is not stored on the object, so no metric could compare suggestions with confirmations.
- **What I did:** Stored the suggestion as data instead: `suggested_nature` is written by the agent through `suggest_nature`, and `nature` is written by a person through `confirm_nature`. Confidence that `proposable` was not the intended answer: low to medium.

### 8. "The agent never commits a financial write" cannot be written, and neither can "a claimant does not approve their own claim"
- **Kind:** inexpressible (by the format's stated design)
- **Where:** §5: "only an effect's value MAY read `actor` … a condition, an invariant, a derived attribute, a default, a metric and every other expression MUST NOT, since who may act is decided outside the flow (ADR-0114; step 4, `check 64`)."
- **What I needed / what happened:** The business rule is a who-may-act rule. I probed the runner (the probe examples were in a scratch copy, not the deliverable). Every one of these applies:
  - `actor: classifier` (the agent) requesting `confirm_nature`;
  - the agent requesting `Claim.approve`;
  - `actor: alice` approving her own claim.
- **What I did:** The flow separates suggesting (`suggest_nature`, which writes only `suggested_nature`) from committing (`confirm_nature`, `approve`, `record_in_xero`, `pay`). The example `refuse_suggestion_after_confirmation` shows the agent cannot overwrite a person's decision. Preventing the agent from taking the committing transitions is left to the application's authorization, and the module header says so. A workaround exists and the checker accepts it (entry 20), but I kept it out. Confidence that this is what the reference intends: high. The business requirement itself is not met by the declaration.

### 9. `warn` and `audit` read as disjoint in the reference; only the cited source says a raised flag is recorded
- **Kind:** contradictory; also left-the-reference
- **Where:**
  - §4.8: "`audit`: the condition is evaluated and its failures are recorded, and it refuses nothing"; "`warn`: a failure is reported to the caller with the result, and the transition goes through".
  - §4.9: "`attempts`, each request that did not apply; `attempt_counts`, their daily counts".
  - Checker notice: "within_policy warns: a failure is reported with the result and refuses nothing".
- **What I needed / what happened:** The business wants a rule that never refuses, tells the caller, and is recorded and counted per rule. Read literally, §4.8 gives "recorded" only to `audit` and "caller told" only to `warn`. A guard mapping takes one enforcement per condition. §4.9's `attempts` ("each request that did not apply") would not contain a warned request, because a warned request applied. So by the reference alone the requirement looked unmeetable.

  Only `declaration-syntax.md` §6.9 says an attempt row is a "request that did not apply, an observing clause's would-be refusal, or a raised flag", with members `.enforced` and `.flagged`. Neither member is defined anywhere I was allowed to read. I cannot tell whether an `audit` failure is `.flagged`, or neither.
- **What I did:** Used `warn` on `ClaimLine.within_policy` (at `add` and `amend`) and on `Claim.lines_within_policy` (at `approve`). Counted them with metrics over `attempt_counts`, filtered on `a.flagged`, with the dimension `rule: a.clause`. Confidence that `warn` is "the construct for exactly this": medium to high. Confidence that `a.flagged` selects exactly the warn failures: low.

### 10. "Counted per rule" cannot be verified, and it is unclear where a caused transition's flag is logged
- **Kind:** silent
- **Where:** §11.4 has `warned` for the caller-facing half. Nothing covers the attempt log. §11.5: "an example that reaches a construct it does not yet execute, such as a metric a guard reads, is reported as the notice `notrun`".
- **What I needed / what happened:**
  - The warning from `ClaimLine.add` fires inside `Claim.add_line`'s create step. The reference does not say whether the flag is recorded in `ClaimLine`'s attempt log or `Claim`'s.
  - The examples runner never evaluates metrics, and there is no expectation key for "an attempt row was written".
  
  So the "recorded and counted" half of the requirement is declared but untested.
- **What I did:** Put a flag-counting metric on both types (`ClaimLine.line_policy_flags`, `Claim.approval_policy_flags`). The examples verify only the caller-facing half: `warned: [ClaimLine.within_policy]` and `warned: [lines_within_policy]`. Confidence: low.

### 11. A `warn` guard must declare a remedy, but nothing fails for the caller to remedy
- **Kind:** unclear
- **Where:** §4.7: "`remedy` | yes | what a caller can do when it fails". §2.6: "every required key is written, including … every condition's `remedy`".
- **What I needed / what happened:** For a `warn` guard the request goes through, so it is unclear what the remedy means, or where it appears (the warning? the attempt row's `.remedy`?).
- **What I did:** Chose `self_serviceable` for the line rule (enter a lower amount) and `unreachable_from_here` for the approval rule (the claim must be returned and amended first). These are guesses about meaning; the checker accepts any value.

### 12. Guards see the object before the request's writes, so a partial edit cannot easily validate its result
- **Kind:** unclear, close to inexpressible
- **Where:**
  - §7 notice `before`: "a guard runs before the transition's writes, so it reads the value from before the request".
  - §5: "a guard that is unknown refuses".
  - `declaration-syntax.md` §8.3: `if` "is not allowed in a guard".
- **What I needed / what happened:** Amending a line should re-check the new amount against the new category's policy. With optional inputs, the guard must case-split every combination of which inputs were given, using `is null`. That is four disjuncts for amount and category alone. The reference gives no pattern for this.
- **What I did:** Made `amend` a full replacement: all four facts are `required_inputs`, and the guards read `inputs.*`. Amending also clears the agent's suggestion and the person's confirmation, since the facts changed. Confidence: high that it is correct. It is less friendly than a partial edit.

### 13. How the declaration says which figures name a person is only in the cited source, and the checker shows nothing
- **Kind:** silent in `flow-format.md`; left-the-reference
- **Where:** §4.9 says only that "no personal member is read anywhere in a metric". The answer is in `declaration-syntax.md` §6.9: "A dimension whose value names an actor — an object of a type that marks an `actor` identity, or an `.actor_id` — is **marked** in the declaration a reader reads".
- **What I needed / what happened:** Finance's per-claimant figures must be distinguishable. My first idea was to mark the `claimant` reference `personal`, but that is wrong: a metric could then not read it at all, and it would have to be optional. `flow-format.md` never mentions the automatic marking. The checker's output never says which metrics are marked, so I cannot confirm my two per-claimant metrics (`claimant_amount_claimed`, `claimant_over_policy_lines`) are. §6.9 also says any metric narrowed by a filter to one person "gives a person's figure" (D451), so the marking does not cover everything.
- **What I did:** Made `claimant` a reference to `Employee`, whose `user_id` has `actor_kind: human`, so by §6.9 the dimension is marked. As a belt-and-braces measure, per-person metrics are prefixed `claimant_` and their descriptions say "it names a person". Confidence that the marking happens: medium. It is unverified.

### 14. The quoting rule, read literally, forbids the reference's own cascade example
- **Kind:** unclear
- **Where:** §2.4: "Inside an inline list `[…]` or an inline mapping `{…}`, a value containing a comma, a question mark, a colon followed by a space, a bracket, a brace or `#` MUST be quoted".
- **What I needed / what happened:** `- { on: [pay, reject, withdraw], transition: close, limit: 200 }` contains a value with brackets and commas inside an inline mapping. Quoting it would make it a string. The rule means scalars, but does not say so. §4.14 and §6 write it unquoted.
- **What I did:** Followed the examples. The checker accepted it. Confidence: high.

### 15. The checker's output does not say whether any example ran
- **Kind:** checker-mismatch (the output does not show what §7 step 6 did)
- **Where:** On the first full run the output was only the notices and "0 fatal · 14 notices", with nothing about examples run or passed.
- **What I needed / what happened:** With everything passing at once, I could not tell whether step 6 had executed 18 examples or none.
- **What I did:** Planted wrong expectations in a copy:
  - A wrong remedy was caught at step 3 ("Claim.has_lines declares the remedy unreachable_from_here, not dependent"), which stopped the check before step 6.
  - A wrong `warned` list and a wrong `values` were caught at step 6 ("over_policy_line_is_flagged: expected the request to warn of nothing, and it warns of ['ClaimLine.within_policy']").
  
  So the runner does run. A count of examples passed would have saved these runs.

### 16. Step 5 silently stops checking the main type once six or more states carry required attributes
- **Kind:** silent
- **Where:**
  - §7.1: "A condition that expands to more than 4096 cases is reported as `undecided` rather than decided."
  - Checker: "reimbursement.yaml:198 notice undecided the invariants of Claim in DRAFT expand to more than 4096 cases, so whether they can hold together is not decided". There are ten of these, one per Claim state.
- **What I needed / what happened:** The central type of the workflow gets no logic check of its state invariants. I probed to find the cause:
  - Trimming each state to require only its newly set attributes still gave 10 `undecided`.
  - Keeping `required_attributes` on the first 5 states gave 0 `undecided`; on the first 6 or more, 10 `undecided`.
  
  So an ordinary linear lifecycle (draft → … → paid) with cumulative state invariants exceeds the bound. The reference gives the bound but no way to write around it.
- **What I did:** Kept the invariants. They are correct and are enforced at runtime; step 5 only proves less. The notices are not fatal.

### 17. How to write an `identity` value in an example
- **Kind:** silent
- **Where:** §11.3 "Values": "anything else as its literal".
- **What I needed / what happened:** `Employee.onboard` needs a `user_id` of type `identity`. The reference does not say what an identity looks like (an email address? an opaque id?), or whether it must be unique across humans and agents.
- **What I did:** Wrote bare words (`user_id: alice`, `agent_id: classifier`). The checker and runner accepted them. Confidence: medium.

### 18. The checker says an unknown `warn` guard refuses; the runner lets it through
- **Kind:** checker-mismatch; unclear in the reference
- **Where:**
  - §5: "a comparison with an absent value is unknown, and a guard that is unknown refuses". This is stated with no exception for `warn` or `audit`.
  - Checker, on the same guard: "small warns: a failure is reported with the result and refuses nothing" **and** "small can be unknown when finish is requested, through amount … which it never tests with `is null`; a guard that is unknown refuses".
- **What I needed / what happened:** "Never refuses" is a hard business requirement. I built a separate probe (`probes/warnprobe.yaml`): a `warn` guard over an optional amount left absent. The runner **applied** the request and listed the guard in `warned`. So in the runner an unknown `warn` does not refuse, but the checker's notice and the reference say it does.
- **What I did:** Made `within_policy` read only required values (`inputs.amount`, `inputs.category.policy_amount`, and `policy_amount` is required), so it is never unknown. Confidence that a `warn` never refuses: medium. The runner says it doesn't; the prose says otherwise.

### 19. Counting one specific internal transition: the fixed measure refuses, and the fallback is an unchecked string
- **Kind:** inexpressible (by the fixed measure); silent (literal forms)
- **Where:**
  - §4.9: "A `transition_count` … MUST NOT name a transition that shares a source and target with another".
  - Checker: "metric confirmations counts confirm_nature, which moves between the same states as amend, suggest_nature; a count by states cannot tell them apart".
- **What I needed / what happened:** I wanted a detective control: confirmations of a line's nature, split by actor kind, flagged if an agent made any. Every internal transition on `LISTED` shares LISTED→LISTED, so no internal transition of a type with more than one on a state can be counted by `transition_count`. The formula fallback over `transitions` needs `t.transition == "confirm_nature"`. The reference never says how a transition name is written in an expression. The checker accepted `t.transition == "no_such_transition"` with no finding, so a typo silently counts zero. I also found by probing that `agent` is a bare expression literal: `who == agent` was accepted, while `who == zebra_crossing` was refused as "neither value nor a dimension". The reference lists `agent` only as a reserved format value (§10), not as an expression literal.
- **What I did:** Left the detective metric out of the deliverable because its filter cannot be checked. Confidence: high.

### 20. The checker accepts laundering `actor.kind` into a `string` attribute and constraining it with an invariant
- **Kind:** checker-mismatch (probably accepts what the model forbids)
- **Where:** §5: "`actor.kind` is that type's actor kind". The cited `declaration-syntax.md` §8.1 says "`.kind` of a fixed enum", and §8.3 says "Comparison needs both sides of one type".
- **What I needed / what happened:** A probe copy added:
  - `confirmed_by_kind: { type: string, optional: true }`;
  - `assign: { location: confirmed_by_kind, expr: actor.kind }` on `confirm_nature`;
  - an invariant `confirmed_by_kind is null or confirmed_by_kind != "agent"`.
  
  The checker raised no finding. It stored an enum into a `string` and compared it with the text `"agent"`. The runner then refused the agent's confirmation as `invariant_violated confirmed_by_a_person unreachable_from_here`. This enforces entry 8's rule, but only by getting around §5's principle that "who may act is decided outside the flow". The reference never names the type of `actor.kind`, so I could not declare a correctly typed attribute.
- **What I did:** Kept it out of the deliverable. If the authors consider it legitimate, it is the only way I found to meet "the agent never commits a financial write" in the declaration. Confidence that the type check is missing: medium.

### 21. The cited section contradicts itself about `actor` in guards
- **Kind:** contradictory (in the cited `declaration-syntax.md` §8)
- **Where:**
  - §8.1: "A guard, an invariant, a derivation, a filter or a metric reading `actor` is a publish error (check 64)".
  - §8.2, a few lines later: "a guard over it fails closed and an erased approver cannot satisfy `none(a in approvals where a.approver == actor.id)` (ADR-0047)".
- **What I needed / what happened:** When checking whether a guard could compare the approver with the claimant, the one example I found does exactly what §8.1 and `flow-format.md` §5 forbid.
- **What I did:** Trusted `flow-format.md` §5 and check 64. I did not test it.

### 22. The coverage notice ignores transitions exercised only in setups
- **Kind:** unclear
- **Where:** §11.5: "It then reports the notice `coverage`, naming the examined module's transitions that no example requests". The checker reported "no example requests 19 transitions of reimbursement: … Claim.draft, … ClaimLine.add, … Employee.onboard …".
- **What I needed / what happened:** `Claim.draft`, `ClaimLine.add` and `Employee.onboard` run in every example's setup. "Requests" means only an example's `request`, not its steps. That is a defensible reading, but the notice then reports a flow as far less covered than it is.
- **What I did:** Nothing. Coverage is a notice, and adding an example per setup step would only satisfy the counter.

### 23. The checker has no usage text; `--help` runs its self-test over the example modules
- **Kind:** checker-mismatch (usability)
- **Where:** `python3 -B scripts/check-flows.py --help` printed "people.yaml: clean, 0 notices", "inventory.yaml: …" and about thirty "planted a … : caught by …" lines, each quoting a line of the example modules.
- **What I needed / what happened:** I wanted to know whether the checker could report which metrics carry the person marking (entry 13). There is no help text. The self-test output showed me file names and fragments of the example modules I was told not to open. I did not open them.
- **What I did:** Stopped there. The reference documents `--digest` and `--previous`, and nothing else.

---

### What the reference made straightforward

- **The Xero read-back:** an evaluator guard (§4.18), with a stubbed answer in the examples (§11.3).
- **Composition and cascades:** §4.3 and §4.14.
- **Identifiers:** §4.12.
- **Refusal verdicts:** the order and meaning of verdicts (§4.8 "Refusals", §11.4).

The module passed with 0 fatal on its first run. That is itself a caution: several of my guesses, listed in entries 17, 19 and 20, passed without the reference confirming them.

### Top issues

1. **Who may act cannot be declared** (entries 8 and 20). "The agent never commits a financial write" and "no self-approval" are outside the format by design. The runner lets the agent approve a claim. The only enforcement I found gets around §5, and the checker accepts it with an enum-to-string mismatch.
2. **`warn`, `audit` and the attempt log are described inconsistently** (entries 9 and 10). `flow-format.md` gives "recorded" only to `audit` and defines `attempts` as requests that did not apply. Only the cited §6.9 says flags are rows, and `.flagged` is undefined. "Counted per rule" cannot be tested by the runner.
3. **An unknown `warn` guard: refuses or not?** (entry 18). §5 and the checker's `unknown` notice say it refuses. The runner lets it through. This sits exactly on the business's "never refuses".
4. **Step 5 does not scale to a real lifecycle** (entry 16). The workflow's central type, with 10 states, gets no logic check of its invariants, and there is no guidance on writing within the 4096-case bound.
5. **"Which figures name a person" is invisible** (entry 13). It is answered only in `declaration-syntax.md` §6.9 (automatic marking of actor-typed dimensions), and nothing in the checker's output shows it.
6. **Counting a specific internal transition is impossible by the fixed measure, and unchecked by the formula** (entry 19). A misspelt transition name in a filter passes silently. Actor-kind literals are undocumented.
7. **The expression language and metric row members live outside the reference** (entries 1 and 9). The first condition could not be written without leaving it.
8. **No date type and one currency per attribute** (entries 3 and 4). Expense dates are bucketed by UTC month, and foreign-currency lines cannot be represented.

### Final checker result

Command: `cd /home/rdu/RduWs/object_flow && python3 -B scripts/check-flows.py <out>/reimbursement.yaml <out>/reimbursement.examples.yaml`

Summary lines, as printed on the final run:

```
0 fatal · 14 notices
reimbursement: step 5 decided 10 of 22 guard conditions in full; the rest read something outside the type, such as another object, a metric or an evaluator, which it leaves open
```

Exit status: 0. The 14 notices are:
- 10 × `undecided` (entry 16);
- 3 × `warn` (the intended flags);
- 1 × `coverage` (entry 22).

The probe for entry 18 is kept in `probes/warnprobe.yaml` and `probes/warnprobe.examples.yaml`. The other probes were temporary copies of the module, described in their entries.
