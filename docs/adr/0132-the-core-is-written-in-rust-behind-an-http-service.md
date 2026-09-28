# ADR-0132: The core is written in Rust, behind an HTTP service described by OpenAPI

- **Status:** Accepted by the author, 2026-09-28: "go with Rust, accept the rest", after asking "what do you recommend for the core's language?" and "how about Rust?". The author's reason, given the same day: "as this project is expected to be the driving engine for many applications, so I'd rather get it solid so that I can save time in the future for the applications". The recommendation first put was Python; re-examined at the author's question, it was withdrawn in favour of Rust, for the reasons below.
- **Date:** 2026-09-28
- **Refines:** ADR-0110 §5 (the transport and the core's language left open), ADR-0104 (upper-layer applications), ADR-0091 (a synchronous core driven by the harness)
- **Relates to:** PRD N1, N2, N3, N4, N7, C5, UC-20; `library-api.md` §1; `adversarial-harness.md`; `renderers.md` §3

## Context

ADR-0110 made the engine an internal service that every caller reaches through one interface, so no caller binds to the core, and left open its transport, HTTP with an OpenAPI description or gRPC, and the core's language, to be chosen "for correctness and maintainability alone". It also left the operations application's home to the author (`TODO.md`).

The recommendation first put for the language was Python: the design's executable parts, the flow checker, the syntax checker and the scenario checker, are Python, as is the first consumer. Asked "how about Rust?", the case was re-checked, and two of its reasons did not hold:
- the same recommendation required the engine to share no code with those checkers, so that a mistake in one is not passed as correct by the other, which makes the engine new code in any language;
- ADR-0110's own premise is that no caller binds to the core, so the first consumer's language does not reach it.

What remained was that the specification, not the language, has been the source of the record's defects, which holds for either language, and that this specification keeps changing: five decisions on 2026-09-28 alone. Keeping an implementation right through such change is where a compiler that checks exhaustiveness and absence earns most.

## Decision

1. **The core is written in Rust.**
   - The verdicts, remedy classes and syntax nodes are enumerations matched exhaustively, and optional and unknown values are types, not conventions.
   - The core is synchronous, as ADR-0091 drives it, and the service layer alone is asynchronous.
   - The adversarial harness is written in Rust against the core in process, keeping ADR-0091's deterministic driving.
2. **The Python checkers stay, independent of the engine.** The flow checker, the syntax checker and the scenario checker share no code with the core. The same flows and histories are run through both and their verdicts compared, so each is the other's oracle. A rule change lands in both, which is the price of that independence.
3. **Two things are checked before the first line of engine code.** Each is recorded here as a check to make, not a fact:
   - **a YAML parser that is maintained and strict.** It must read YAML 1.2, so that `NO` is not a boolean, as the flow checker's strict loader already requires; the long-standard `serde_yaml` is believed unmaintained since 2024;
   - **a decimal type wide enough.** `declaration-syntax.md` §8.3 makes `int × decimal(p,s)` a `decimal(p+19,s)`, up to 38 digits, and `rust_decimal` is believed to stop near 28, which would call for an arbitrary-precision crate for money and quotients.
4. **The service speaks HTTP, described by OpenAPI, carrying JSON.** The engine's tools for agents are already JSON Schema (`renderers.md` §3). The first consumer's backend is FastAPI. PRD N3 sizes the engine for operations rate, and `pull` returns cursor pages, so nothing needs streaming.
5. **The operations application lives in the operations platform's project**, as an upper-layer application (ADR-0104). UC-20 showed its questions answered by one query each and its actions taken as requests like anyone's, so ObjectFlow holds nothing of it.
6. **`library-api.md` stays the interface's executable specification.** It is written in Python, as `scripts/check-api-doc.py` checks it. The Rust core implements the same operations with the same meanings, and the HTTP service exposes them one to one.

## Alternatives rejected

- **Python.** It would give the fastest first release and one language across the author's projects. It was recommended first, and set aside once reuse was ruled out by the checkers' independence. Its remaining edge, speed of building, weighs less than correctness through years of specification change, which is the criterion ADR-0110 set, and less still for an engine the author expects many applications to be built on, each of which inherits its faults.
- **C# on .NET 8.** It is in the author's conventions, with a native decimal and pattern matching, but it would be a new stack beside the author's Python and C++ for no gain here.
- **Go.** It has no sum types, so the verdicts and the syntax tree would lose the exhaustive checking this core most needs.
- **C++.** It is the author's strength, but a transactional service over PostgreSQL and SQLite with JSON, YAML and decimals is costly to keep correct in it.
- **TypeScript.** Its numbers are floating point, which is wrong for money.
- **gRPC.** Typed stubs and streaming buy little at operations rate, and a binary protocol makes a refused request harder to read when debugging.
- **The operations application inside ObjectFlow.** It would put a scheduler and a notifier in the governed core, which ADR-0104 and PRD N6 keep out.

## Consequences

- `library-api.md` §1 says the core is Rust and the Python is the interface's executable specification.
- `TODO.md` holds the two checks of decision 3, the Rust conventions to write beside the author's `cpp-cmake.md`, the differential tests of decision 2, and moves the operations application to the platform's project.
