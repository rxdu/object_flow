# Before 1.0.0: the author's review

Status: **prepared 2026-10-11** for the author, at their direction ("go ahead and address all of them", on a plan naming "a review packet for the flow format and ADRs 0174 to 0179"). It holds what the author reviews before the engine's release is numbered 1.0.0 and ADR-0180's promise binds: six ADRs built at the author's direction and not yet reviewed, and the flow format as format 1 would freeze it. Each item says what was decided, how it is held, what would be hard to change once promised, and a question with a recommendation; §4 gathers the questions.

## 1. What to read, in order

1. **ADR-0180**, what a release promises: the frame for everything else here, since it says which decisions below become hard to change.
2. **ADR-0179**, a refusal's detail and pages: the most recent change to what an application reads.
3. **ADR-0174** and **ADR-0175**, a deployment's service and its evaluators' transport: what a team deploys.
4. **ADR-0177** and **ADR-0178**, an engine release's upgrade and requests at once: what keeps a deployment running.
5. **The flow format** (§3): `flow-format.md`, or the one-page card, [`guides/flow-format-card.md`](../guides/flow-format-card.md), and §3 below for what format 1 would freeze.

## 2. The ADRs awaiting review

### ADR-0180: what a release promises

- **Decides**: three promised surfaces, the wire `/v1`, the flow format and the store; within `/v1` and format 1 a release only adds, and a check that would refuse a description it accepted arrives as a notice first; semantic versioning, the promise binding from 1.0.0.
- **Held by**: format 1's corpus, the samples frozen with their digests and checked on both backends (`scripts/check-compat.py`, `objectflow-core`'s `format_1` test); the wire's baseline (`guides/wire-v1.json`), which a test of the service holds the keys to; each release's store fixtures (ADR-0177).
- **Hard to change once promised**: all of it, which is its point.
- **Question 1**: accept the promise, and number the release 1.0.0 once this review is done and the third cold review finds nothing that would break? **Recommendation: yes.** The rules cost little to keep, and a developer deciding whether to build on the engine asks what a release may break before anything else.

### ADR-0179: a refusal's detail, and pages

- **Decides**: a refusal tells its `objects`, `unknown`, `proposable` and `consulted`, a flag its `unknown` and `objects`, on both backends; the objects a clause names are settled by a rule (decision 2); `query`, `available`, `pull` and `export` answer pages of 100 by default, at most 1000, by an opaque keyset cursor; a metric and `exceptions` stay one page.
- **Held by**: the Flow Studio's test of both backends alike (`test_a_refusal_tells_what_it_names_alike`), two deployment tests of pages, the scenarios' replay reading every page three at a time; each mutation-checked.
- **Hard to change once promised**: the default page size, since a client that ignores cursors reads exactly one page; which objects a clause names, since an application links to them.
- **Question 2**: accept as built, the default page 100? **Recommendation: yes.** A client that ignores cursors then reads a short list whole and visibly misses the rest of a long one in a pilot, rather than in production; a page of at most 1000 bounds what one read costs the service. The rule for named objects follows `DESIGN.md` §5.5 where it was explicit, and settles only what it left open.

### ADR-0174: a deployment's service, its first slice

- **Decides**: one binary serving a sandbox or, with `--store`, a deployment's SQLite store in WAL mode with full synchronisation; the wall clock never moving backwards; loopback by default and no authentication, an application in front of it authenticating (ADR-0114); `GET /health`; retention flags. Decision 7 was replaced by ADR-0175, and decision 3 by ADR-0177's upgrade.
- **Held by**: `deployment.rs`, a store surviving restarts on the wall clock, and the second procurement integrator's pilot, five versions, three restarts and a go-live on one store.
- **Hard to change once promised**: the flags' names, which a deployment's unit files carry, and the `/health` answer, which monitoring reads; ADR-0180 decision 2 promises both.
- **Question 3**: accept as built? **Recommendation: yes.** Authentication stays the application's (ADR-0114), which the integration guide says first.

### ADR-0175: the evaluators' HTTP transport

- **Decides**: `ureq` 3 with rustls and the platform's trust store, the dependency the author approved; a request learning its calls by attempting, asking each endpoint outside the transaction; the endpoint's JSON call and answer; `--evaluator <name>=<url>` and a timeout; `check` consulting as a request does.
- **Held by**: deployment tests against a stub endpoint, an unanswering endpoint answered unknown, and the second integrator's Xero bridge.
- **Hard to change once promised**: the endpoint's JSON, which every evaluator an application writes answers; ADR-0180 decision 2 promises it with the wire.
- **Question 4**: accept as built? **Recommendation: yes.**

### ADR-0177: an engine release's upgrade, as built

- **Decides**: a release is the workspace version, moving exactly when what a store holds or means changes; a registry of releases with migrations; every operation reads the store's release in its transaction; a service on an older store serves only `POST /v1/upgrade` until it runs; a dry run rolls back; re-recording flows under new built-ins is not built until a release changes them.
- **Held by**: fixtures made by the 0.1.0 binary, upgraded and read alike; a store of an unknown release refused.
- **Question 5**: accept as built? **Recommendation: yes.** Its one gap, decision 6, refuses rather than proceeds, which is the safe side.

### ADR-0178: more than one request at a time

- **Decides**: one writer behind a lock, `BEGIN IMMEDIATE` with a busy timeout and bounded retries, then `stale` with cause `retries_exhausted`; every read on a pooled connection in its own read transaction, under the rules its snapshot was written under; PostgreSQL left for a consumer that outgrows one file.
- **Held by**: tests of reads beside a held writer, concurrent writers losing no update, a read seeing one committed state, a foreign lock past its timeout; a load probe, measured on one machine and stated as such.
- **Question 6**: accept as built, writes one at a time? **Recommendation: yes.** The measured 94 writes a second beside reads, on a debug build with durable commits, is a pilot's need; a team that needs parallel writes needs PostgreSQL, which `TODO.md` keeps.

## 3. The flow format, as format 1 would freeze it

`flow-format.md` is "draft 2, adopted" (2026-09-26), grown since by additions the reviews asked for: an example's `creates`, an import's `sequences` and an expectation of `metrics` (ADR-0173 decision 6), and `taken_by` and the `veto` notice (ADR-0176). Under ADR-0180 every description valid today stays valid and means the same; what format 1 freezes is therefore everything the checker accepts today, and what is open is what it refuses, which a later release may accept.

- **Open questions in §9**: whether generated guards and invariants carry descriptions (question 2), and whether a transition whose source state does not require an attribute its target requires is refused even where the attribute is kept (question 3). Answering either later adds or accepts, and breaks nothing. **Question 7**: leave both open past 1.0.0? **Recommendation: yes.**
- **What the cold reviews found hardest**, all of it now a notice or a rule the guide states: a guard on a creation reads `inputs.<name>` (`before`); an invariant reading through references vetoes later changes (`veto`); a personal attribute is optional and needs an erasure; sections in one order. None would change under the promise; each is in the card.
- **The name**: the schema calls itself draft 2 and its `$id` is relative. **Question 8**: at 1.0.0, rename draft 2 to format 1 in `flow-format.md`'s status, the schema's `title` and its `$id`? **Recommendation: yes**, the `$id` staying relative until the schema is published at an address, which is the author's to choose.

## 4. The questions

1. Accept ADR-0180's promise, and number the release 1.0.0 once this review is done and the third cold review finds nothing that would break? *Recommended: yes.*
2. Accept ADR-0179 as built, the default page 100? *Recommended: yes.*
3. Accept ADR-0174 as built? *Recommended: yes.*
4. Accept ADR-0175 as built? *Recommended: yes.*
5. Accept ADR-0177 as built? *Recommended: yes.*
6. Accept ADR-0178 as built, writes one at a time? *Recommended: yes.*
7. Leave `flow-format.md` §9's questions 2 and 3 open past 1.0.0? *Recommended: yes.*
8. Rename draft 2 to format 1 at 1.0.0? *Recommended: yes.*
