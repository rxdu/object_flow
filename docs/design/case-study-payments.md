# Case study: a payments ledger

Written 2026-09-08, after the model. The declaration below is checked on every run by `scripts/check-syntax-doc.py`, which reads this file as it reads the specification.

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
| Debits equal credits | `invariant balanced: sum(e in entries: e.signed) == USD 0.00`, a traversal invariant over the posting's parts |
| An entry is never alone | `invariant double_entry: count(e in entries) >= 2` |
| Debit versus credit | one `signed` derivation, not two transitions. See §3 |
| A balance | an ordinary attribute maintained by an action the entry calls, **not** a counter. See §3 |
| Idempotency per caller | `attr idempotency_key string indexed unique with merchant_id` (ADR-0072 recorded the compound form; ADR-0061 decided it) |
| The network approved | `require authorised: card_network.authorisation_stands(this.id) deferred` |
| The network refused | `require refused: not card_network.authorisation_stands(this.id) deferred` |
| Money movement is all-or-nothing | a posting's entries are created in one outcome, and a failing guard on any of them aborts the request (ADR-0038) |
| An entry outlives its posting | `POSTED` is reached only by a creation, so the posting has no terminal transition and the coverage rule asks nothing (ADR-0061 decision 5) |
| The chargeback window | `require in_window: settled_at is not null and now <= settled_at + 120 days because unreachable_from_here` |
| A settled payment still accepts a chargeback | `SETTLED` is `category closed` and **not** `terminal`, with `ARCHIVED` after it |

## 3. The ledger, declared

```text
module payments

capability PAYMENT_CREATE, PAYMENT_CAPTURE, CHARGEBACK_RAISE, ACCOUNT_ADMIN
category   pending, active, closed

enum AccountKind   version 1 { ASSET, LIABILITY, REVENUE, FEE_EXPENSE }
enum DeclineReason version 1 { INSUFFICIENT_FUNDS, DO_NOT_HONOUR, EXPIRED_CARD }
sequence account_code version 1
sequence payment_ref  version 1

evaluator card_network version 1 {
  fn authorisation_stands(payment : identity) fresh 30 s
}

type Account version 1 {
  tracking record
  states   OPEN category active, CLOSED category closed terminal
  summary  code, kind, balance, state

  attr code    string identifier from account_code format "ACC-{n}" indexed unique
  attr kind    AccountKind indexed
  attr balance money(USD) default USD 0.00 indexed

  ref  entries : LedgerEntry[] inverse account

  create open_account -> OPEN accepts kind {
    require may: actor.has(ACCOUNT_ADMIN) because delegable
  }
  act apply at OPEN only via LedgerEntry.record {
    input delta : money(USD)
    set balance := balance + inputs.delta
  }
  do close OPEN -> CLOSED {
    require may:    actor.has(ACCOUNT_ADMIN) because delegable
    require zeroed: balance == USD 0.00 because dependent
  }
}

type LedgerEntry version 1 {
  tracking record
  states   RECORDED category closed terminal
  summary  amount, is_debit, state

  owner posting : Posting inverse entries
  ref   account : Account  inverse entries
  attr  amount   money(USD) indexed
  attr  is_debit bool       indexed

  derive signed = if is_debit then amount else USD 0.00 - amount

  create record -> RECORDED only via Posting.post {
    input into     : Posting
    input account  : Account
    input amount   : money(USD)
    input is_debit : bool
    require positive: inputs.amount > USD 0.00 because self_serviceable
    set posting  := inputs.into
    set account  := inputs.account
    set amount   := inputs.amount
    set is_debit := inputs.is_debit
    call inputs.account.apply(delta := signed)
  }
}

type Posting version 1 {
  tracking record
  states   POSTED category closed terminal
  summary  posted_at, state

  ref  payment : Payment?
  attr posted_at timestamp indexed

  part entries : LedgerEntry[] inverse posting

  invariant balanced:     sum(e in entries: e.signed) == USD 0.00
  invariant double_entry: count(e in entries) >= 2

  create post -> POSTED only via Payment.capture {
    input for_payment : Payment
    input debit       : Account
    input credit      : Account
    input gross       : money(USD)
    set payment   := inputs.for_payment
    set posted_at := now
    create LedgerEntry.record(into := this, account := inputs.debit,
                              amount := inputs.gross, is_debit := true)
    create LedgerEntry.record(into := this, account := inputs.credit,
                              amount := inputs.gross, is_debit := false)
  }
}

type Payment version 1 {
  tracking record
  states   REQUESTED category pending, CAPTURED category active,
           SETTLED category closed, DECLINED category closed terminal,
           ARCHIVED category closed terminal
  summary  reference, amount, state

  attr reference       string identifier from payment_ref format "PAY-{n}" indexed
  attr merchant_id     string indexed
  attr idempotency_key string indexed unique with merchant_id
  attr amount          money(USD)
  attr settled_at      timestamp? indexed
  attr decline_reason  DeclineReason?

  ref merchant_account : Account
  ref clearing_account : Account

  create request -> REQUESTED accepts amount, merchant_id, idempotency_key,
                                      merchant_account, clearing_account {
    require may: actor.has(PAYMENT_CREATE) because delegable
  }
  do decline REQUESTED -> DECLINED accepts decline_reason {
    require may:     actor.has(PAYMENT_CREATE) because delegable
    require refused: not card_network.authorisation_stands(this.id)
                     deferred because dependent
  }
  do capture REQUESTED -> CAPTURED {
    require may:        actor.has(PAYMENT_CAPTURE) because delegable
    require authorised: card_network.authorisation_stands(this.id)
                        deferred because dependent
    create Posting.post(for_payment := this, debit := clearing_account,
                        credit := merchant_account, gross := amount)
  }
  do settle CAPTURED -> SETTLED {
    require may: actor.has(PAYMENT_CAPTURE) because delegable
    set settled_at := now
  }
  act receive_chargeback at SETTLED {
    input case_id : string
    require may:       actor.has(CHARGEBACK_RAISE) because delegable
    require in_window: settled_at is not null and now <= settled_at + 120 days
                       because unreachable_from_here
  }
  do archive SETTLED -> ARCHIVED {
    require may: actor.has(PAYMENT_CAPTURE) because delegable
  }
}
```

## 4. What the model could not say, and what was decided

Five things. Four became open questions the author has since answered; the fifth was a defect.

**Uniqueness over two attributes could not be written at all.** A type-scan invariant matched the object against itself, so no payment could ever be created, and the clause that repaired it was not a symmetric shape. **ADR-0061**: a type-scan ranges over every *other* object, and `unique with` gives the compound key directly. This is the case that produced both.

**The refusal could not be written.** The specification prescribes expressing a conditional external check as two transitions with opposite guards, and a verdict was usable only as a whole guard clause, so the second transition did not type. A payment could be declined by anyone holding the create authority, with nothing tying the decline to what the network said. **ADR-0061** allows a negated evaluator call. Without it the model has an authority hole exactly where its integrity lives.

**Money had no scale and no rounding.** The specification stated that integer division truncates *because* money is the trap that would create, and then left money's own arithmetic undefined: nothing said `money(JPY)` has no minor unit, or what a fee split does with a fractional cent. **ADR-0061**: the scale is the currency's minor unit and scalar arithmetic rounds half to even, stated rather than deferred to a backend, because a rounding difference surfaces as a reconciliation break months later rather than as an error.

**A multi-currency ledger cannot be typed.** `money(ccy)` fixes its currency at declaration, so this study is dollars throughout. **ADR-0068** keeps it that way: a mismatch is a publish error rather than a production one, and a model holding several currencies declares an account per currency, which is what double-entry systems do regardless. Recorded as a limit in `edge-cases.md`.

**A balance cannot be a counter.** A counter is a non-negative integer, and a balance is money and routinely negative, so the design's own remedy for an invariant whose scan is too slow — declare a counter and the cascade that maintains it — is closed to the one quantity that most wants it. The balance above is an ordinary attribute with nothing tying it to the entries that produced it. **ADR-0072** defers this, and says why: the first consumer stores no stock level at all and computes availability on demand, so nothing in the record exercises a maintained counter. What it does change is that non-negativity moves out of `counter` and into an invariant, which is where a fact about stock belongs.

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
