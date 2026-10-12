# ADR-0175: The evaluators' HTTP transport

- **Status:** **Accepted** by the author, 2026-10-11, by ADR-0182 ("go with your recommendations on the fifteen questions"). Before: Built at the author's direction, 2026-10-10 ("go with ureq for the evaluator transport", on `TODO.md`'s recommendation), for the author's review.
- **Date:** 2026-10-10
- **Refined by:** ADR-0178 — a check's calls are asked from a read of its own, a request's from the writer between attempts. ADR-0188 — an evaluator's bearer token, `--evaluator-token`, read from a file its owner alone reads.
- **Builds:** ADR-0173 decision 2, ADR-0174 decision 7
- **Relates to:** `DESIGN.md` §2, §6; ADR-0008, ADR-0049, ADR-0092, ADR-0158; `design/procurement-cold-draft.md` I16; `core/README.md`; `TODO.md`

## Context

ADR-0173 decision 2 decided that a deployment's service calls each evaluator namespace at an endpoint its configuration names, outside the transaction, with the function and its arguments, and takes a verdict and never a value, a timeout, a fault or an unreachable endpoint being the evaluator's unknown, refused `temporal`. ADR-0174 built a deployment's service without it, answering every evaluator unknown, since the transport needs an HTTP client with TLS, a new dependency; the author chose `ureq` on 2026-10-10. What remained undecided is how the service learns which calls a request makes, which `DESIGN.md` §6 orders "first, outside the transaction", what it sends an endpoint, and how it is configured.

## Decision

1. **The client is `ureq` 3, with rustls and the platform's trust store**, so an endpoint behind a private certificate authority the host trusts is reached as a public one is; no gzip, and no other feature. It adds 16 crates to the service's 77, by `cargo tree` on 2026-10-10: `ureq`, `ureq-proto`, `rustls` and its `ring`, `rustls-webpki`, `rustls-pki-types`, `rustls-platform-verifier`, `rustls-native-certs`, `webpki-roots`, `openssl-probe`, `getrandom`, `subtle`, `untrusted`, `zeroize`, `base64` and `utf8-zero`.
2. **A request learns its calls by attempting.** A call the request's answers do not hold stops it, naming the evaluator, the function and its arguments, as the core's value notation writes them; the transaction it was in is rolled back, the service asks the endpoint outside it, adds the verdict to the request's answers, and attempts the request again, as `DESIGN.md` §6 orders a request whose evaluator's arguments moved to consult again within its retry bound. At most sixteen calls are asked for one request, after which a call is unknown. A verdict is the request's alone: the next request asks again, since an external system's answer may change.
3. **An answer is held by the call, then by the function.** The core looks up `<evaluator>.<function>(<arguments>)` first and `<evaluator>.<function>` second, so a sandbox's fixed answer for a function still answers every call, and a call's own verdict answers it alone.
4. **The endpoint is posted the call as JSON**, `{"evaluator": "xero", "function": "bill_paid", "arguments": ["BILL-0042"]}`, each argument in the wire's notation, and answers `{"verdict": "satisfied"}` or `{"verdict": "unsatisfied"}`. Any other answer, a status other than `200`, a body that is not that, a timeout or a connection refused, is the evaluator's unknown, and the service writes why to standard error, so a failing integration is seen and never answers a rule silently.
5. **Endpoints are flags.** `--evaluator <name>=<url>` names a namespace's endpoint, once for each, and `--evaluator-timeout <duration>` the time an answer is waited for, five seconds unless given. An evaluator the modules declare and no flag names is answered unknown, as ADR-0174 answered every one, and the service names each at its start.
6. **`check` consults as a request does**, since it answers what the request would receive; an offer asks no evaluator, as before (ADR-0048).

## Alternatives rejected

- **A callback into the core, asked during the request's transaction.** It holds the store's write lock across a network call, which `DESIGN.md` §6 forbids, and threads a client through every evaluation.
- **A dry run first to find every call.** A first refusal stops the dry run before later guards' calls, so it would need the same repetition as attempting, and runs every rule once more than attempting does.
- **The verdict cached across requests.** An evaluator answers about another system's present state, and a cache would answer with a past one; one is added only where a measurement shows an endpoint too slow.

## Consequences

- A deployment answers an evaluator through its endpoint; ADR-0174 decision 7 is replaced, and `DESIGN.md` §2, `core/README.md` and the integration guide say so.
- The core's evaluation has a new way to stop, naming the call it lacks; `NotRun` carries it, and the sandbox, which answers by function, answers as before.
- `objectflow-service/tests/deployment.rs` holds it: a bank endpoint in the test clears one transfer and not another, and a fault, an answer slower than the timeout and one with no verdict are each refused `temporal`; the call posted carries its argument, and the next request asks again. Mutating away the status check, the verdict's reading, the timeout, the request-scoped verdicts or the call's own answer each fails it.
