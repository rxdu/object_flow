# Case study: a payments ledger

Written 2026-09-08, after the model. The declaration below is written in the flow description format since 2026-09-26 (ADR-0116), and `scripts/check-corpus.py` checks it on every run with every step of the flow checker (`scripts/check-flow-docs.py`, ADR-0121).

> **Amended 2026-09-25 for ADR-0114.** ObjectFlow records who acted and never evaluates it. The declarations below lost every clause that read who is asking or declared who may do or see something — capabilities, actor guards, visibility — which the upper layer now decides. The narrative records the model as the study found it, before ADR-0114.

*Vocabulary, noted 2026-09-24:* written before PRD revision 5, this document says "consumer" for an application built on the store, which PRD §5 now calls an upper-layer application, and sometimes for the deployment or a reader; "the first consumer" keeps its meaning (D368).

## 1. Why this case

The five earlier cases are the same shape twice removed from this one. The first consumer and the CRM have few, long-lived, richly related objects. Issue tracking adds families. Orders at volume adds throughput and short lifetimes but keeps money as a number. None of them is a **ledger**, and a ledger stresses four things nothing else does.

**A record is meaningless alone.** A ledger entry is one half of something; entries arrive in balanced sets and the property worth enforcing spans several objects created in one request. Every other case study's invariants are about one object or one relationship.

**Money must not be wrong.** Currency, scale, rounding on a fee split, partial capture, partial refund. The orders study said "money is everywhere" and typed it as a number.

**The authority is outside.** A card network approves or declines, and its answer is the fact the transition turns on — including the negative answer, which is a decline.

**Nothing is edited.** A posted entry is never corrected in place; a correction is a new compensating entry. That is immutability as a domain rule rather than as a storage choice.

It also has time with legal force, a chargeback window measured from settlement, and idempotency, since the same request arriving twice must not move money twice.

## 2. Mapping

| Payments concept | Declared as |
|---|---|
| Account, ledger entry, posting | three types, none of them a physical thing, so all three are `tracking record` (ADR-0067) |
| Debits equal credits | the invariant `balanced`, `sum(e in entries: e.signed) == USD 0.00`, a traversal invariant over the posting's parts |
| An entry is never alone | the invariant `double_entry`, `count(e in entries) >= 2` |
| Debit versus credit | one `signed` derivation, not two transitions. See §3 |
| A balance | an ordinary attribute maintained by an action the entry calls, **not** a counter. See §3 |
| Idempotency per caller | `idempotency_key: { type: string, indexed: true, unique: { with: [merchant_id] } }` (ADR-0072 recorded the compound form; ADR-0061 decided it) |
| The network approved | the condition `authorised`, `card_network.authorisation_stands(this.id)` |
| The network refused | the condition `refused`, `not card_network.authorisation_stands(this.id)` |
| Money movement is all-or-nothing | a posting's entries are created in one outcome, and a failing guard on any of them aborts the request (ADR-0038) |
| An entry outlives its posting | `POSTED` is reached only by a creation, so the posting has no terminal transition and the coverage rule asks nothing (ADR-0061 decision 5) |
| The chargeback window | the condition `in_window`, `settled_at is not null and now <= settled_at + 120 days`, with the remedy `unreachable_from_here` |
| A settled payment still accepts a chargeback | `SETTLED` is `category: closed` and **not** `final`, with `ARCHIVED` after it |

## 3. The ledger, declared

```yaml
module: payments
categories: [pending, active, closed]

enumerations:
  AccountKind: [ASSET, LIABILITY, REVENUE, FEE_EXPENSE]
  DeclineReason: [INSUFFICIENT_FUNDS, DO_NOT_HONOUR, EXPIRED_CARD]

sequences:
  account_code:
    description: Numbers the accounts, in one series.
  payment_ref:
    description: Numbers the payments, in one series.

evaluators:
  card_network:
    description: The card network, whose answer is the fact a capture or a decline turns on.
    functions:
      authorisation_stands:
        description: Whether the network's authorisation of this payment stands.
        arguments:
          payment: { type: identity }
        fresh: 30 s

types:
  Account:
    description: An account of the ledger, whose balance moves only by the entries recorded against it.
    tracking: record

    attributes:
      code:
        type: string
        identifier: { sequence: account_code, format: "ACC-{n}" }
        indexed: true
        unique: true
      kind:    { type: AccountKind, indexed: true }
      balance: { type: money(USD), default: USD 0.00, indexed: true }
      entries: { reference: "LedgerEntry[]", opposite: account }

    states:
      OPEN:   { category: active }
      CLOSED: { category: closed, final: true }

    summary: [code, kind, balance, state]

    conditions:
      zeroed:
        description: The account's balance is zero.
        expression: balance == USD 0.00
        remedy: dependent

    transitions:
      open_account:
        kind: initial
        to: OPEN
        required_inputs: [kind]
      apply:
        kind: internal
        from: OPEN
        only_via: [LedgerEntry.record]
        inputs:
          delta: { type: money(USD) }
        effect:
          - assign: { location: balance, expr: balance + inputs.delta }
      close:
        kind: external
        from: OPEN
        to: CLOSED
        guards:
          zeroed: deny

  LedgerEntry:
    description: One leg of a posting, a debit or a credit of one account, never edited once recorded.
    tracking: record

    attributes:
      posting:  { reference: Posting, opposite: entries }
      account:  { reference: Account, opposite: entries }
      amount:   { type: money(USD), indexed: true }
      is_debit: { type: bool, indexed: true }

    states:
      RECORDED: { category: closed, final: true }

    derived_attributes:
      signed:
        description: The amount as it moves the account, positive for a debit and negative for a credit.
        expression: if is_debit then amount else USD 0.00 - amount

    summary: [amount, is_debit, state]

    conditions:
      positive:
        description: The amount recorded is more than zero; the sign is the entry's side.
        expression: inputs.amount > USD 0.00
        remedy: self_serviceable

    transitions:
      record:
        kind: initial
        to: RECORDED
        only_via: [Posting.post]
        required_inputs: [account, amount, is_debit]
        inputs:
          into: { reference: Posting }
        guards:
          positive: deny
        effect:
          - assign: { location: posting, expr: inputs.into }
          - call: { target: inputs.account, transition: apply, inputs: { delta: signed } }

  Posting:
    description: A balanced set of entries, created together in one request.
    tracking: record

    attributes:
      payment:   { reference: Payment, optional: true }
      posted_at: { type: timestamp, indexed: true }
      entries:
        reference: "LedgerEntry[]"
        aggregation: composite
        opposite: posting

    states:
      POSTED: { category: closed, final: true }

    summary: [posted_at, state]

    invariants:
      balanced:
        description: The posting's debits equal its credits.
        expression: "sum(e in entries: e.signed) == USD 0.00"
      double_entry:
        description: A posting has at least two entries, since an entry is never alone.
        expression: count(e in entries) >= 2

    transitions:
      post:
        kind: initial
        to: POSTED
        only_via: [Payment.capture]
        inputs:
          for_payment: { reference: Payment }
          debit:       { reference: Account }
          credit:      { reference: Account }
          gross:       { type: money(USD) }
        effect:
          - assign: { location: payment, expr: inputs.for_payment }
          - assign: { location: posted_at, expr: now }
          - create: { type: LedgerEntry, transition: record, inputs: { into: this, account: inputs.debit, amount: inputs.gross, is_debit: true } }
          - create: { type: LedgerEntry, transition: record, inputs: { into: this, account: inputs.credit, amount: inputs.gross, is_debit: false } }

  Payment:
    description: A card payment, from its request through the network's answer to settlement and archiving.
    tracking: record

    attributes:
      reference:
        type: string
        identifier: { sequence: payment_ref, format: "PAY-{n}" }
        indexed: true
      merchant_id:      { type: string, indexed: true }
      idempotency_key:  { type: string, indexed: true, unique: { with: [merchant_id] } }
      amount:           { type: money(USD) }
      settled_at:       { type: timestamp, optional: true, indexed: true }
      decline_reason:   { type: DeclineReason, optional: true }
      merchant_account: { reference: Account }
      clearing_account: { reference: Account }

    states:
      REQUESTED: { category: pending }
      CAPTURED:  { category: active }
      SETTLED:   { category: closed }
      DECLINED:  { category: closed, final: true }
      ARCHIVED:  { category: closed, final: true }

    summary: [reference, amount, state]

    conditions:
      refused:
        description: The network has not authorised the payment.
        expression: not card_network.authorisation_stands(this.id)
        remedy: dependent
      authorised:
        description: The network's authorisation of the payment stands.
        expression: card_network.authorisation_stands(this.id)
        remedy: dependent
      in_window:
        description: The payment settled no more than 120 days ago, the chargeback window.
        expression: settled_at is not null and now <= settled_at + 120 days
        remedy: unreachable_from_here

    transitions:
      request:
        kind: initial
        to: REQUESTED
        required_inputs: [amount, merchant_id, idempotency_key, merchant_account, clearing_account]
      decline:
        kind: external
        from: REQUESTED
        to: DECLINED
        optional_inputs: [decline_reason]
        guards:
          refused: deny
      capture:
        kind: external
        from: REQUESTED
        to: CAPTURED
        guards:
          authorised: deny
        effect:
          - create: { type: Posting, transition: post, inputs: { for_payment: this, debit: clearing_account, credit: merchant_account, gross: amount } }
      settle:
        kind: external
        from: CAPTURED
        to: SETTLED
        effect:
          - assign: { location: settled_at, expr: now }
      receive_chargeback:
        kind: internal
        from: SETTLED
        inputs:
          case_id: { type: string }
        guards:
          in_window: deny
      archive:
        kind: external
        from: SETTLED
        to: ARCHIVED
```

## 4. What the model could not say, and what was decided

Five things. Four became open questions the author has since answered; the fifth was a defect.

**Uniqueness over two attributes could not be written at all.** A type-scan invariant matched the object against itself, so no payment could ever be created, and the clause that repaired it was not a symmetric shape. **ADR-0061**: a type-scan ranges over every *other* object, and `unique with` gives the compound key directly. This is the case that produced both.

**The refusal could not be written.** The specification prescribes expressing a conditional external check as two transitions with opposite guards, and a verdict was usable only as a whole guard clause, so the second transition did not type. A payment could be declined by anyone holding the create authority, with nothing tying the decline to what the network said. **ADR-0061** allows a negated evaluator call. Without it the model has an authority hole exactly where its integrity lives.

**Money had no scale and no rounding.** The specification stated that integer division truncates *because* money is the trap that would create, and then left money's own arithmetic undefined: nothing said `money(JPY)` has no minor unit, or what a fee split does with a fractional cent. **ADR-0061**: the scale is the currency's minor unit and scalar arithmetic rounds half to even, stated rather than deferred to a backend, because a rounding difference surfaces as a reconciliation break months later rather than as an error.

**A multi-currency ledger cannot be typed.** `money(ccy)` fixes its currency at declaration, so this study is dollars throughout. **ADR-0068** keeps it that way: a mismatch is a publish error rather than a production one, and a model holding several currencies declares an account per currency, which is what double-entry systems do regardless. Recorded as a limit in `edge-cases.md`.

**A balance cannot be a counter.** A counter is an `int` — and, under the rule in force when this was written, one that could not go negative — and a balance is money and routinely negative, so the design's own remedy for an invariant whose scan is too slow — declare a counter and the cascade that maintains it — is closed to the one quantity that most wants it. The balance above is an ordinary attribute with nothing tying it to the entries that produced it. **ADR-0072** defers this, and says why: the first consumer stores no stock level at all and computes availability on demand, so nothing in the record exercises a maintained counter. *(Corrected 2026-09-24: it keeps a stock level for bulk accessories, which a quantity lot with a counter would model, D279.)* What it does change is that non-negativity moves out of `counter` and into an invariant, which is where a fact about stock belongs.

## 5. What held without change

**The debit-and-credit split was a modelling error, not a language gap.** The first version of this model had `Account.debit` and `Account.credit`, then `LedgerEntry.record_debit` and `record_credit` to call them, then four-entry `only via` lists on each. The repair is one derivation, `signed`, and one sign-agnostic `apply`. Conditional *values* were always available in a derived attribute; what the straight-line rule forbids is a conditional *step*. Four transitions and eight authority entries became two and four. This is recorded because the reviewer who reported it as a language defect withdrew it, and the withdrawal is the useful part.

**`only via` with a mandatory list.** The property this domain most needs — every write to the ledger arrives through a posting, and nothing can call `Account.apply` directly — is one clause per transition.

**Terminal versus closed.** The specification anticipates the mistake and names it. A settled payment must still accept a chargeback, so `SETTLED` is closed and not terminal. Getting that wrong late costs a lifecycle restructure and a fresh set of cascade clauses.

**Three-valued evaluation.** The window guard reads an optional `settled_at`, and the presence test went in on the first attempt because §8.2 shows the failure in the shape it arrives in rather than stating a rule.

**The remedy classes earn their keep here.** An expired chargeback window is `unreachable_from_here`, not `temporal`: waiting is exactly what will not help. Declared correctly, the publish report leaves the transition off the list a scheduler polls (ADR-0071).

## 6. Rare cases, recorded in `edge-cases.md`

- A multi-currency balance in one attribute. Not covered, by decision (ADR-0068).
- A posting whose legs each carry their own account, amount and sign, supplied by the request. An input may be a set of references, so a selection can be passed; a set of anonymous structures cannot, since every input is typed. Each posting shape is therefore its own creation.
- Exact allocation of an amount into parts that must sum back. Not expressible, and deliberately: who absorbs the remainder is a decision, and it belongs to the consumer.
- A compensating entry as a *correction*. `corrects` rewrites an attribute in place, which this domain forbids; a compensating entry is an ordinary new posting and the link between them is a reference.
- A maintained counter over money. Deferred (ADR-0072).
