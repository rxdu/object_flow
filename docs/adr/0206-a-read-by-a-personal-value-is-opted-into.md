# ADR-0206: A read by a personal value is opted into

- **Status:** **Decided** 2026-10-14 on the author's standing instruction ("go with your recommendations"), the design ADR-0204 decision 9.3 asked for, within ADR-0205 decision 11's scope
- **Date:** 2026-10-14
- **Supersedes:** ADR-0201 decision 4 and ADR-0202 decision 3 (read guards by the actor's kind)
- **Refined by:** ADR-0218 — decision 2's guard parses a query's filter with its parameters, so a parameter on a personal member is personal. ADR-0219 — decision 1's members judged, for a query, as the type each belongs to declares it, by name only where a filter does not tell its type. ADR-0220 — decision 2's `available`, `availability` and `check` judged likewise, an input by its own transition.
- **Refines:** ADR-0204 decision 9.3; ADR-0205 decision 11
- **Relates to:** PRD T5, N7; `design/audit/personal-data.md` §2.3 and §2.6; `guides/wire.md`, `guides/errors.md`, `guides/integrate.md`, `guides/agents.md`

## Context

PRD T5 says the engine filters no read by who is asking, and ADR-0204 decision 9.3 found that ADR-0201 decision 4 and ADR-0202 decision 3 broke it: they refused an agent's, then an unknown actor's, query by a personal member, reading the actor's kind. A person's own query, a service's and the operator's were answered, so an application forwarding a person's filter left the same inference open (the audit's personal data R3).

The audit found the inference open on every read that selects by a value:
- a query's filter and order (R3);
- a member of type `identity` holding a personal identity, which no guard counted (R4);
- `available`, selecting objects by a guard that reads a personal member (R5);
- `lookup` by a personal external identifier (R6);
- a metric read's `bind` on a person dimension (R7);
- `check` and `availability`, whose verdicts are decided by a personal member (R15, R17);
- a request's refusal naming the holder of a unique personal value (R16).

Decision 9.3 replaces the kind-based guards with one that reads no actor and fails closed: such a read is refused `personal` unless the request carries an opt-in the gateway sets. Rules may read request inputs (N7), so the guard is within T5.

## Decision

1. **A personal member is one of these:**
   - an attribute marked `personal`;
   - an input marked `personal`, or one its transition writes into a personal attribute;
   - a derived attribute reading one, through references as well;
   - where any actor type's identity attribute is marked `personal`, every member of type `identity`, since a copy of an identity is the identity (ADR-0203 decision 1).
2. **A read that selects by a personal member is refused `personal`, unless its body carries `"reads_personal": true`.** The reads are:
   - `query`, by its filter or its order;
   - `available`, where the transition's guards read one;
   - `lookup`, by an external identifier whose attribute is personal;
   - `metric`, by its filter, or by a `bind` on a dimension the declaration marks as naming a person;
   - `check` and `availability`, where a guard or an invariant the answer is decided by reads one.

   The guard runs before anything is evaluated, so no refusal's reason can quote a stored value.
3. **A request's refusal names no holder of a personal value without the opt-in.** A uniqueness over a personal attribute, and `identity_taken` where identities are personal, are refused as before. Their `objects`, and each failure's `object_id`, are left out unless the request carries `"reads_personal": true`.
4. **The gateway sets the opt-in.** It sets it where the person or agent it acts for may read the personal values the read selects by, as it decides what any caller may see (T5, N7). A caller that sends it unchecked opts every caller in, which `integrate.md` says.
5. **Answers are unchanged.** `get`, `history`, `events`, `pull` and `export` answer the values they hold, `consulted` and `actor_id` included, for the gateway to filter as T5 says. A read that answers a value lets a reader learn no more than the value; one that selects by it lets a reader learn values it never answers, which is what the guard is for.
6. **ADR-0201 decision 4 and ADR-0202 decision 3 are superseded.** No guard reads the actor's kind.

## Alternatives rejected

- **The guard by the actor's kind** (ADR-0201, ADR-0202). It contradicts T5, and leaves a person's forwarded filter open.
- **Refusing every read by a personal member.** A person's own record and an HR search by name are legitimate, and the gateway can tell them apart where the engine cannot.
- **Answering selections with the personal values masked.** A selection's membership is the inference, whatever its values say.
- **An opt-in per member.** It names personal members in every request and gives the gateway nothing it cannot say with one flag, since it decides by the reader and not the member.

## Consequences

- Reads 0.2.0 answered are refused `personal` without the opt-in, for every actor kind. `CHANGELOG.md` lists them; the guides say when to set it.
- The Flow Studio's server reads as the flow's author, so it sets the opt-in on the reads it forwards to the core unless the caller sends its own, and its two backends answer alike. The reference runner's sandbox takes the key and has no guard, being the author's alone. *(Corrected 2026-10-14, while building it: this said the sandbox held the same guard.)*
