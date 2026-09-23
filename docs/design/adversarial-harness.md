# The adversarial harness

Draft, 2026-09-09. The acceptance test. Not "can a consumer complete the workflow" — that tests the happy path and the objective is not the happy path — but "can a fallible actor, given everything the store offers, reach a state the declaration says is impossible".

**Amendment pending.** ADR-0081 to ADR-0086, accepted 2026-09-23, change this document: intervals rebuilt from the log become a ninth failure condition, observing clauses are excluded from the guarantee under test, and UC-19's three routes join the scripted behaviours. Until it is amended, where it disagrees with those ADRs or with `DESIGN.md`, they win (`TODO.md`).

The threat model is **mistakes, not malice** (ADR-0015): an actor that guesses, retries, skips steps, races itself and misreads a verdict. Not one with stolen credentials or database access. A hostile actor with a `psql` prompt defeats every design in this repository and is somebody else's problem.

## 1. The claim under test

`DESIGN.md` §13 states it: no state change bypasses the guards except through a capability-gated, recorded assertion, either one the type declares or the built-in one import and migration use.

That is falsifiable, which is what makes it worth a harness. A run **fails** if it produces any of:

| Failure | What it means |
|---|---|
| an object in a state no transition reaches | the machine was not the only way in |
| an invariant false, with no admission recorded | enforcement was skipped or lost a race |
| a required attribute absent | a creation wrote less than it declared |
| a part alive under a whole in a terminal state | a cascade was skipped or a coverage rule was wrong |
| a stored relationship end whose far side disagrees | something wrote the derived end |
| an event whose replay does not reproduce the row | the log stopped being the history |
| a verdict of `satisfied` with no event | a write that was not recorded |
| any of the above reachable **without** a capability the declaration gates it on | the escape hatch is not the only escape |

Every one of these is checkable against the store from outside, by reading the declaration and the read surface. That is deliberate: the harness must not need privileged access to detect a failure, or it is testing something the consumer cannot.

## 2. The actor

Four behaviours, because they are the four ways real callers actually break things.

**The guesser** calls transitions that `availability` did not offer, with inputs it invented, in states it did not check. It is testing that a refusal is a refusal and not a partial effect.

**The retrier** repeats a request after a timeout, without an idempotency key, then with a stale one, then with one belonging to a different request. It is testing ADR-0041's replay, ADR-0076's order — the identical retry replays whatever `expected_version` it carries — and the key's scope, which is the actor's `id` (ADR-0077): a second actor sending the same key is a different request, and the same actor reusing a key with a different body is refused as `KeyReused` rather than replayed or applied.

**The racer** issues concurrent requests against one object, against a pair with a shared invariant, and against a whole and its part at once. It is testing serialisable isolation, the retry bound, and whether a cascade that aborts leaves anything behind.

**The skipper** works from a stale read: it takes `availability`, waits, and acts on it after the world has moved. It is testing that `check` is advice and not a reservation, which the read surface says plainly and which every caller will forget.

None of them is clever. Cleverness is the malice model, and this is not it.

## 3. The fixture is the first consumer's types

Not a synthetic type set. The first consumer's declaration is the fixture, because a harness against types invented for it tests the harness.

That gives, from the walkthrough and the syntax document: a unit with a nine-state lifecycle and an `only via` sale, a delivery with parts and cascades and an approval invalidated by `changed_since`, an engagement with a one-open-per-unit invariant, a quantity-tracked stock with counters, and a family sharing a machine. Between them they exercise every mechanism the model has except erasure and supersession, which the payments and CRM studies supply.

## 4. What a run produces

A run is a seed, a declaration version and a transcript, and it is reproducible from those three. The transcript is every request and its verdict, in order, with the events each produced. Reproducible only because the store takes its id source and its clock as injected dependencies (ADR-0077): ids are time-ordered and a cascade iterates in id order, so a run whose ids came from the wall clock would order its cascades differently each time, and the reduction below would be unsound.

When a run fails, the artefact is the **shortest prefix that still fails**, found by replaying the transcript with requests removed. A four-thousand-request transcript is not a bug report; eleven requests are.

The reduction is only sound if the run is deterministic given its seed, which is why the concurrency in §2 is scheduled by the harness rather than left to the operating system.

**How the racer schedules.** The harness holds several database connections in **one** process and issues statements on them in an order the seed chooses. Connection A opens a transaction and runs its guard reads; the harness then makes connection B do the same before letting A write. No thread races anything: the test process decides who acts next, and the database still provides the real isolation, the real locks and the real serialisation failures.

That is what makes the run both faithful and reproducible, and it is why the earlier framing of this as a choice between "a hook inside the store" and "a single-threaded simulation of the isolation level" was wrong. The hook is invasive and the simulation is unfaithful — it would be testing the harness's idea of serialisability rather than the database's. Driving real connections in a chosen order needs neither, because a connection is already a thing the test can hold and withhold.

Two things follow. The interleaving points are statement boundaries, not arbitrary instruction boundaries, so the racer explores a coarser space than a thread scheduler would — which is the right space, since the guarantees are about transactions. And the harness needs the store's API to be synchronous, or to expose one, since it must be able to stop between statements.

## 5. What a passing run does not prove

Worth stating plainly, because a green harness is exactly the artefact people over-read.

It does not prove the declaration says what the business meant. It cannot: it checks the store against the declaration, and a rule that was never written is not a rule the harness can miss. That is the residual risk `TODO.md` names, and legibility is its only mitigation — which is why the printable rule set exists and why it is reviewed by someone who knows the process.

It does not prove the guarantee for declarations unlike the fixture. It samples.

And it does not prove anything about an actor with database access, which is the model this design does not defend against and says so.

**Which backend it runs against.** Both, and they are not equivalent. SQLite serialises writers already, so the racer finds almost nothing there and a green run says little about concurrency; PostgreSQL pays for predicate tracking and is where a serialisation failure, a retry and a lock wait actually happen (ADR-0039). PostgreSQL is therefore the reference for the racer, and SQLite is the reference for the other three behaviours, being cheap enough to set up and tear down for the thousands of runs they want.

## 6. Still open

Two, and neither blocks building it.

- **How long a run should be.** Every transition and every guard clause are cheap and should be floors. Every pair of concurrent transitions on one type is quadratic, and that is where a budget has to be chosen — no argument settles it, only measurement.
- **Whether a fallible model is worth using in place of the four scripted behaviours.** It is closer to the real threat and it makes reduction and reproducibility much harder. The scripted behaviours come first because they are reproducible.
