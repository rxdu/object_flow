# The adversarial harness

Draft, 2026-09-09. The acceptance test. Not "can a consumer complete the workflow" — that tests the happy path and the objective is not the happy path — but "can a fallible actor, given everything the store offers, reach a state the declaration says is impossible".

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

**The retrier** repeats a request after a timeout, without an idempotency key, then with a stale one, then with one belonging to a different request. It is testing ADR-0041's replay and the boundary of the key's scope, which the record leaves undecided — so the harness is where that decision gets its evidence.

**The racer** issues concurrent requests against one object, against a pair with a shared invariant, and against a whole and its part at once. It is testing serialisable isolation, the retry bound, and whether a cascade that aborts leaves anything behind.

**The skipper** works from a stale read: it takes `availability`, waits, and acts on it after the world has moved. It is testing that `check` is advice and not a reservation, which the read surface says plainly and which every caller will forget.

None of them is clever. Cleverness is the malice model, and this is not it.

## 3. The fixture is the first consumer's types

Not a synthetic type set. The first consumer's declaration is the fixture, because a harness against types invented for it tests the harness.

That gives, from the walkthrough and the syntax document: a unit with a nine-state lifecycle and an `only via` sale, a delivery with parts and cascades and an approval invalidated by `changed_since`, an engagement with a one-open-per-unit invariant, a quantity-tracked stock with counters, and a family sharing a machine. Between them they exercise every mechanism the model has except erasure and supersession, which the payments and CRM studies supply.

## 4. What a run produces

A run is a seed, a declaration version and a transcript, and it is reproducible from those three. The transcript is every request and its verdict, in order, with the events each produced.

When a run fails, the artefact is the **shortest prefix that still fails**, found by replaying the transcript with requests removed. A four-thousand-request transcript is not a bug report; eleven requests are.

The reduction is only sound if the run is deterministic given its seed, which is why the concurrency in §2 is scheduled by the harness rather than left to the operating system: the racer's interleavings are chosen from the seed and replayed exactly.

## 5. What a passing run does not prove

Worth stating plainly, because a green harness is exactly the artefact people over-read.

It does not prove the declaration says what the business meant. It cannot: it checks the store against the declaration, and a rule that was never written is not a rule the harness can miss. That is the residual risk `TODO.md` names, and legibility is its only mitigation — which is why the printable rule set exists and why it is reviewed by someone who knows the process.

It does not prove the guarantee for declarations unlike the fixture. It samples.

And it does not prove anything about an actor with database access, which is the model this design does not defend against and says so.

## 6. What this leaves open

- **How the racer schedules.** Deterministic concurrency needs either a hook in the store to release transactions at chosen points, or a single-threaded simulation of the isolation level. The first is invasive and exact; the second is neither. This is the harness's one hard design problem.
- **How long a run should be.** Coverage of what? Every transition, every guard clause, every pair of concurrent transitions on one type — the third is quadratic and the first is too weak.
- **Whether the harness runs against SQLite, PostgreSQL or both.** They differ in exactly the place the racer probes, since one serialises writers already, so a green run on SQLite says less.
- **Whether a fallible *model* is worth using in place of the four scripted behaviours.** It is closer to the real threat, and it makes reduction and reproducibility much harder. The scripted behaviours come first because they are reproducible.
