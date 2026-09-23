# The adversarial harness

Draft, 2026-09-09, amended 2026-09-23. The acceptance test, which PRD N4 names as the acceptance test. The question is not "can a consumer complete the workflow" — that tests the happy path, and the objective is not the happy path — but "can a fallible actor, given everything the store offers, reach a state the declaration says is impossible".

**Amended 2026-09-23** for ADR-0082 to ADR-0091:
- the claim is over **enforced** rules, and observing clauses are excluded;
- three failure conditions are new: intervals the log does not reproduce, a read set that does not re-evaluate to its verdict, and a personal value surviving an erasure;
- UC-19's three routes join the scripted behaviours;
- the racer interleaves requests through an injected connection source behind a statement barrier, the mechanism ADR-0091 decided, since the earlier "one process, no threads" description could not stop a synchronous request (D209).

The threat model is **mistakes, not malice** (ADR-0015): an actor that guesses, retries, skips steps, races itself and misreads a verdict. Not one with stolen credentials or database access. A hostile actor with a `psql` prompt defeats every design in this repository and is somebody else's problem.

## 1. The claim under test

`DESIGN.md` §13 states it: no state change bypasses the **enforced** guards except through a capability-gated, recorded assertion, either one the type declares or the built-in one import and migration use. An observing clause is on trial and guarantees nothing (ADR-0085), so the harness treats it as absent. It checks separately that such a clause records its would-be refusals and never refuses.

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
| an interval the log does not reproduce, or a legacy interval that overlaps another of its dimension or does not end where the object's first recorded interval begins | the interval index stopped being an index over the log, or the port left a gap or an overlap in history (ADR-0083, ADR-0096) |
| an event whose rules, re-evaluated over its read set, give a different verdict | the record no longer explains the decision (ADR-0088, PRD L2) |
| a personal value still readable after its subject's erasure — on an object, in an event, in a predecessor, in a caller's event, in a label note — or an erased object's shared file gone for another object | erasure missed something, or took too much (ADR-0087, UC-17) |
| any of the above reachable **without** a capability the declaration gates it on | the escape hatch is not the only escape |

Every one of these is checkable against the store from outside, by reading the declaration and the read surface. That is deliberate: the harness must not need privileged access to detect a failure, or it is testing something the consumer cannot.

## 2. The actor

Five behaviours, because they are the ways real callers actually break things.

**The guesser** calls transitions that `availability` did not offer, with inputs it invented, in states it did not check. It is testing that a refusal is a refusal, not a partial effect, and that the refusal reaches the attempt log with its clause and remedy (ADR-0083).

**The retrier** repeats a request after a timeout, without an idempotency key, then with a stale one, then with one belonging to a different request. It tests three things:
- ADR-0041's replay;
- ADR-0076's order: the identical retry replays whatever `expected_version` it carries;
- the key's scope, which is the actor's `id` (ADR-0077). A second actor sending the same key is a different request, and the same actor reusing a key with a different body is refused as `KeyReused`, neither replayed nor applied.

**The racer** issues concurrent requests against one object, against a pair with a shared invariant, and against a whole and its part at once. It is testing:
- serialisable isolation;
- the retry bound;
- that a contended row on PostgreSQL waits and then retries rather than corrupting anything (ADR-0090);
- whether a cascade that aborts leaves anything behind;
- that per-object order survives the settled cursor's (transaction, position) order (ADR-0089).

**The skipper** works from a stale read: it takes `availability`, waits, and acts on it after the world has moved. It is testing that `check` is advice and not a reservation, which the read surface says plainly and which every caller will forget.

**The route-tester** tries the routes PRD UC-19 names, each a way the data-driven features could open a door around the rules:
- it records an observation it is not permitted to record, so that a gate reading it would open. The `recorded by` guard must refuse it (ADR-0082, PRD D12);
- it backdates a transition beyond its declared bound, to shorten a measured duration. The bound must refuse it (ADR-0083, PRD D5);
- it completes a transition an observing clause would have refused, and then acts as though the rule were enforced. The request must proceed, the would-be refusal must be recorded, and the printed rule set must show the clause as not enforced (ADR-0085, PRD T1).

None of them is clever. Cleverness is the malice model, and this is not it.

## 3. The fixture is the first consumer's types

Not a synthetic type set. The first consumer's declaration is the fixture, because a harness against types invented for it tests the harness.

That gives, from the walkthrough and the syntax document:
- a unit with a nine-state lifecycle and an `only via` sale;
- a delivery with parts and cascades, and an approval invalidated by `changed_since`;
- an engagement with a one-open-per-unit invariant;
- a quantity-tracked stock with counters;
- a family sharing a machine;
- a service job with an engineer-of-record marked `assignee`, and inspection results as an observation kind.

Between them they exercise every mechanism the model has except erasure and supersession, which the payments and CRM studies supply. The CRM merge is the fixture for erasure across a supersession chain.

## 4. What a run produces

A run is a seed, a declaration version and a transcript, and it is reproducible from those three. The transcript is every request and its verdict, in order, with the events each produced. It is reproducible only because the store takes its id source, its clock and its connection source as injected dependencies (ADR-0077, ADR-0091). Ids are time-ordered and a cascade iterates in id order, so a run whose ids came from the wall clock would order its cascades differently each time, and the reduction below would be unsound.

When a run fails, the artefact is the **shortest prefix that still fails**, found by replaying the transcript with requests removed. A four-thousand-request transcript is not a bug report; eleven requests are.

The reduction is only sound if the run is deterministic given its seed, which is why the concurrency in §2 is scheduled by the harness rather than left to the operating system.

**How the racer schedules** (ADR-0091):
1. The store's core is synchronous: a request runs to completion on its caller's thread, so it cannot be paused between two statements within one thread.
2. The harness therefore runs each in-flight request on **its own thread**, and hands the store a connection source whose connections wait at a **barrier** before every statement.
3. The harness releases exactly one waiting statement at a time, in the order its seed chooses. It waits for that statement to return, or to block on a database lock, before choosing the next.
4. A statement that does not return within a short bound is recorded as blocked, and the harness releases its next choice. The lock's holder can then proceed — which is exactly the wait-then-retry interleaving the racer exists to produce on PostgreSQL (ADR-0090).

Only one thread is ever runnable, so the interleaving is a function of the seed and the transcript, and the reduction stays sound. The database is real, so the locks, the serialisation failures and the isolation are its own.

That is why the earlier framings were wrong. "A hook inside the store" is invasive, and it tests the harness's idea of where the steps are. "A single-threaded simulation of the isolation level" is unfaithful, because it tests the harness's idea of serialisability rather than the database's. And the first draft of this section, "one process, no threads", could not work at all: nothing can stop a synchronous call between two of its own statements on one thread (D209). A barrier behind an injected connection needs neither a hook nor a simulation, because a connection is already a thing the test can hold and withhold.

The interleaving points are statement boundaries, not arbitrary instruction boundaries, so the racer explores a coarser space than a thread scheduler would. That is the right space, since the guarantees are about transactions.

## 5. What a passing run does not prove

Worth stating plainly, because a green harness is exactly the artefact people over-read.

It does not prove the declaration says what the business meant. It cannot: it checks the store against the declaration, and a rule that was never written is not a rule the harness can miss. That is the residual risk `TODO.md` names, and legibility is its only mitigation — which is why the printable rule set exists, and why it is reviewed by someone who knows the process.

It does not prove the guarantee for declarations unlike the fixture. It samples.

It does not prove anything about an observing clause, which guarantees nothing by design, and says so on the rule set.

And it does not prove anything about an actor with database access, which is the model this design does not defend against and says so.

**Which backend it runs against.** Both, and they are not equivalent.
- **SQLite** begins every transition transaction `IMMEDIATE` (ADR-0090), so its writers are serial. The racer finds little there beyond the busy timeout, and a green run says little about concurrency.
- **PostgreSQL** pays for predicate tracking, and is where a serialisation failure, a retry and a lock wait actually happen (ADR-0039, ADR-0090).

PostgreSQL is therefore the reference for the racer. SQLite is the reference for the other four behaviours, being cheap enough to set up and tear down for the thousands of runs they want.

## 6. Still open

Two, and neither blocks building it.

- **How long a run should be.** Every transition and every guard clause are cheap and should be floors. Every pair of concurrent transitions on one type is quadratic, and that is where a budget has to be chosen. No argument settles it, only measurement.
- **Whether a fallible model is worth using in place of the five scripted behaviours.** It is closer to the real threat, and it makes reduction and reproducibility much harder. The scripted behaviours come first because they are reproducible.
