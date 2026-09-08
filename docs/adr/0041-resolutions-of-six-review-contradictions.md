# ADR-0041: Resolutions of six contradictions found in review

- **Status:** Accepted — repair of D05, D06, D27, D28, D31, D36, 2026-09-08; pending author review
- **Date:** 2026-09-08
- **Refined by:** ADR-0071 — mandatory bounds and guard names stay; the reported worst-case product goes.

## Context

The readiness review found six places where two accepted documents stated incompatible things. In each the correct answer was already decided somewhere and the other statement was stale or loose drafting. They are recorded together because each is a one-line decision, and a decision that lives only in an edit is not recoverable later.

## Decisions

### 1. Only-via transitions are not listed, but are named when refused (D05)

ADR-0020 wins over ADR-0037 and DESIGN.md §10. An only-via transition **does not appear** in an `availability` listing. The `not requestable` verdict is what a caller receives when it names one explicitly in a request or a `check`, and that verdict names the parent transitions that reach it.

Listing it would defeat the reason ADR-0020 exists, which it states plainly: an agent will call whatever is listed. Returning a verdict when asked directly costs nothing and helps a caller that guessed.

### 2. An applied idempotency key replays; it does not refuse (D06)

DESIGN.md §6 wins over ADR-0014's "refuses a second attempt". A repeated request carrying an applied key returns the original result, with an indication that it was replayed.

Refusal would make a retry after a network timeout indistinguishable from a genuine duplicate, which is the failure the key exists to prevent. ADR-0022's periodic scheduler sweep also depends on a repeat being harmless. ADR-0014's own warning against conflating request replay with transition deduplication stands: the key deduplicates the transition, and the replayed result is the transition's result, not a stored HTTP response.

### 3. An action's outcome is not restricted (D27)

ADR-0016 wins over DESIGN.md §5.4 and its terminology entry. An action is a transition whose from-state equals its to-state, and its outcome is unrestricted: it may write attributes, cascade transitions and create objects like any other.

The restricted reading was the shape of the option ADR-0016 explicitly rejected, and it would break approvals, since an approval is an object created by an `approve` action, and the CRM merge, which cascades from an action.

### 4. ObjectKeeper never reads or serves file bytes, and deletes them only under erasure (D28)

ADR-0031 wins over DESIGN.md's absolute "never touches the bytes". The correct statement: ObjectKeeper never reads, streams or serves blob content, and the only byte-level operation it performs is deletion during erasure, which a legal erasure requires.

This is a real capability with a real consequence: a deployment gives ObjectKeeper a blob-store credential that can delete. That is worth stating rather than hiding behind an absolute that was false.

### 5. Constraint compilation is an optimisation, is backend-dependent, and is checked at publish (D31)

The three-shape list of compiled invariants was attributed to ADR-0023, which names only uniqueness and partial unique indexes. Corrected: **which invariant shapes compile to database constraints is a property of the storage backend, not of the model.** PostgreSQL compiles uniqueness, uniqueness per external source, and interval exclusion per key; SQLite has no exclusion constraint and compiles fewer.

Since ADR-0039 makes serialisable isolation the default, compilation is now an **optimisation and a second line of defence**, never the only protection. Correctness no longer varies by backend. What does vary is cost, so publishing a declaration reports which of its invariants the configured backend can compile and which will be enforced by serialisation alone.

### 6. A cascade that exceeds its declared fan-out cap gets its own verdict (D36)

`self_serviceable` was wrong: it means satisfiable by a transition argument, and no argument makes a cascade smaller. A request exceeding a declared fan-out cap is refused with the verdict **`over-limit`**, naming the relationship and the cap.

It is not a guard failure. Nothing about the object is wrong; the request is too large for the declaration's stated bound. The verdict taxonomy becomes: satisfied; unsatisfied with a remedy class; `stale`; `not found`; `not requestable`; `over-limit`.

## Consequences

- DESIGN.md is corrected in §5.4, §5.5, §5.2, §5.6, §6 and §10, and the terminology table gains `over-limit`.
- ADR-0014's "refuses" and ADR-0037's "every transition" are annotated rather than deleted, so the record shows what changed.
- The publish report gains a compiled-invariant section (ADR-0027).
- D05, D06, D27, D28, D31 and D36 are resolved.
