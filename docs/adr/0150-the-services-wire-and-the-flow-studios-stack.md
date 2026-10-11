# ADR-0150: The service's wire, and the Flow Studio's stack

- **Status:** Accepted, 2026-10-04. Decisions 1, 3, 4 and 5 by the author ("go with your recommedtations"), answering `flow-studio.md` §9 and approving its §8. Decision 2 at the author's direction, as what decision 1 needs to carry the library's values, and open to the author's revision until the core implements it. The two findings of §Consequences are the author's to decide.
- **Date:** 2026-10-04
- **Refined by:** ADR-0151 — both findings decided as recommended: an `events` operation, and an invariant failure naming its object's type. ADR-0153 — the core's service serves the same wire. ADR-0154 — `import_batch` and `lookup` on the wire, so no example waits. ADR-0173 — a file's wire form, a read of an absent object answering `null`, a JSON fault on every error, a fault for a key an operation does not take, and an actor named by identity or id. ADR-0179 — a refusal's detail and pages on the wire: `limit`, a keyset `cursor`, `malformed_cursor`. ADR-0180 — what `/v1` promises: a release only adds, held by `guides/wire-v1.json`.
- **Refines:** ADR-0132 (the service's HTTP, which it left without a shape), ADR-0149 (the studio's stack and the PRD's proposals)
- **Relates to:** `design/flow-studio.md`; `design/library-api.md` §2 to §6; PRD §12

## Context

ADR-0132 decided that the engine's service speaks HTTP, described by OpenAPI and carrying JSON, and exposes the library's operations one to one. It gave the wire no shape, and no OpenAPI description exists. The Flow Studio is the first caller (ADR-0149), so its first slice fixes the shape for the service as well as for itself. `flow-studio.md` §9 asked the author three questions, and the author took the recommendation on each.

## Decision

1. **One `POST /v1/<operation>` per library operation.** The body is a JSON object of the operation's arguments under the library's names, the actor among them, and the result is the library's shape as JSON. A verdict is an object whose `kind` names its shape in snake case, `satisfied` to `call_refused`, with the shape's fields beside it; `kind` and not `verdict`, since `CallRefused` has a field of that name, the refused call's own verdict. A refusal is a `200`, since a refusal is a value and not an error (`library-api.md` §2). A fault is a `4xx` or `5xx` with a `fault` message: a body that is not the operation's, an operation that does not exist, a store that cannot answer. The OpenAPI description is written from these, operation by operation, as the studio's slices use them.

2. **A value on the wire is JSON's own where JSON keeps it exactly, and otherwise a string in the model's notation for it.**
   - **JSON's own:** absence is `null`; a `bool` is a boolean; an `int` is a number within ±(2^53 − 1), the range every JSON reader keeps exact; `text` and an identity are strings; a reference is the object's id; a set is an array.
   - **A string in the model's notation** (`declaration-syntax.md` §8.1):
     - a decimal, `"19.99"`, and money, `"USD 19.99"`;
     - a duration, `"30 min"`, and a date, `"2026-10-04"`;
     - a timestamp in RFC 3339 at UTC, `"2026-10-04T09:30:00Z"`;
     - a state or an enumeration member by its name;
     - an `int` outside the safe range, in digits.
   - A reader knows each value's type from the declaration, so no value carries its type. One notation serves descriptions, examples and the wire, and a decimal never passes through a binary float.

3. **Sandbox control is `POST /sandbox/<control>`**, outside `/v1/`, so a deployment's service, which offers none of it (PRD §12, F13), routes nothing there. The controls:
   - `load` builds a sandbox store from the modules given;
   - `reset` empties it;
   - `clock` sets or advances its time;
   - `evaluators` sets the answers its external evaluators give.

   The library already builds a store from an injected `Clock` and `EvaluatorSource` (`library-api.md` §3). So a sandbox is a store whose clock and evaluators the caller holds, and the controls drive those two. The studio's own tooling is served under `/studio/`, which is the studio's server and no service's.

4. **The studio's stack:**
   - **Front end:** React for the interface, TypeScript strict, Vite, Vitest, ESLint, Prettier, Tailwind, Mermaid for the diagram, and, from slice 3, CodeMirror.
   - **pnpm**, pinned through the `packageManager` field and run by Node's `corepack`, since it is not installed globally.
   - **Server:** Python's standard library over the repository's scripts.
   - **pytest** for the server's tests, pinned, in a virtual environment that sees the system interpreter's packages. The checkers run with that interpreter's PyYAML and jsonschema, and the server must load flows exactly as they do.

5. **PRD revision 21 stands as drafted, and F13 and UC-25 are accepted.** Following F10 to F12, they are written as rows in §6 and §7 when their design is, the sandbox's in `DESIGN.md` and the library's documents, and until then §12 records them as accepted.

## Alternatives rejected

- **For decision 1:** resource paths, `GET /objects/{id}` and `POST /objects/{id}/transitions/{name}`, a second naming of the same operations to keep in step with the library; a refusal as a `409` or `422`, which would make a caller treat an ordinary outcome as an error, the thing `library-api.md` §2 refuses.
- **For decision 2:** every value as its JSON type, which loses a decimal's exactness to a binary float and a timestamp's meaning to a number of seconds; a typed envelope, `{"type": "decimal", "value": "19.99"}`, on every value, which repeats the declaration in every payload; ISO 8601 durations, `PT30M`, a third notation beside the model's.
- **For decision 3:** sandbox controls as library operations, which would put a clock any caller can move into the interface of every deployment.
- **For decision 4:** plain TypeScript, which the author declined for React; installing pnpm globally, which a fresh machine would also have to do.

## Consequences

**Two findings against the interface, for the author**, from the studio's first slice, decided as recommended by ADR-0151.

**1. A caller cannot read the events its request appended.** The studio's first slice needs to know which objects a request created, and which transitions it caused, since 42 of the 163 examples name the one or expect the other. `Satisfied.events` gives the positions of every event the request appended, and `Event.cause` links each to the one that caused it. But no operation reads an event by its position: `history` reads by object, and `pull` through a subscription. So a caller is handed positions it cannot resolve.

Three ways to close it:
- **a twentieth operation, `events(actor, positions)`,** which resolves any position the store hands out, a verdict's or an event's cause. I recommend this one.
- **`Satisfied` naming the objects its events are on,** which answers this case and leaves `cause` unresolvable;
- **the caller subscribing and pulling,** which needs no change, but makes every application keep a subscription to learn what its own request did.

**2. An invariant failure on an object the refused request would have created names an object no read resolves.** `InvariantFailure` names the object by its id alone (`library-api.md` §4). When the object is one the request was creating, nothing of it was committed, so `get` finds nothing and the caller cannot tell its type. The type is what an expectation's clause is qualified by, and what a message to a person names. One example meets it: `servicedesk.examples.yaml`'s `a_rating_is_from_one_to_five`, whose rating of six fails `in_range` on the observation being recorded. I recommend that **`InvariantFailure` carry the object's type** beside its id.

**Where slice 1 stands.** Of the 163 examples, the replay through the wire passes 118 with the reference runner's verdict, and fails none. 45 wait, each for a reason the test names:
- 42 on finding 1, 32 of them for `creates` and 10 for `cascaded`;
- 1 on finding 2;
- 2 on an import step, which reaches the port, `import_batch`, a later slice of the studio.

None is passed by guessing.
