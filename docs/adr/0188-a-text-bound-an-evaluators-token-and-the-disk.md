# ADR-0188: A text's bound, an evaluator's token and the disk in health

- **Status:** **Decided by the author**, 2026-10-12 ("go with your recommendations on questions 9, 10 and 11"), on the recommendations `TODO.md` states with each, after the third production review (`design/production-review.md` §7); built the same day.
- **Date:** 2026-10-12
- **Refined by:** ADR-0191 — a token to be sent over `http://` beyond loopback said at start.
- **Refines:** ADR-0175 (the evaluators' transport), ADR-0183 decision 8 (`/health` and `/metrics`), ADR-0187 decision 2 (the service's one `unsafe` call)
- **Relates to:** `declaration-syntax.md` §4; `guides/wire.md`; `guides/integrate.md` §9; `core/README.md`

## Context

The second and third production reviews left three minors open that each needed the author.

- **Text size.** A text value had no bound but the body's 2 MiB: a reviewer stored a 1.9 MB title.
- **Evaluator credentials.** The service sent an evaluator no credential, so a deployment whose evaluator is an accounting system with authentication had to run an unauthenticated bridge in front of it.
- **Disk space.** `/health` answered `200` with the disk full; a write then failed with `disk I/O error`.

## Decision

1. **A string value holds at most 64 KiB.** The bound covers a value of type `string`, `text`, `identity` or `actor_kind`, counted in UTF-8 bytes. An input or an import's value past it is refused `out_of_range`, alike in the reference runner. The expression language has no text concatenation, so an effect can write only a value it read, which was bounded where it entered. A flow that needs a longer text stores a file and holds its reference.
2. **An evaluator may be given a bearer token.** `--evaluator-token <name>=<file>` reads a token from a file at start, and the service sends it to that evaluator as `Authorization: Bearer <token>` with every call.
   - **The file.** A file that others than its owner can read is refused at start, as is an empty one, and so is a name that no `--evaluator` gives.
   - **The log.** The token is never logged.
   - **Why a file.** A file is used rather than an environment variable, which child processes inherit and which shows in the process's environment.
3. **The store's volume is in `/health` and `/metrics`.** `/health` answers `disk_free_bytes`, the space an unprivileged writer has on the store's volume, and `/metrics` gives it as `objectflow_disk_free_bytes`. Below 64 MiB, the space the `-wal`'s limit needs for a checkpoint, `/health` answers `503` with the fault `disk_low`. The figure is read through `statvfs`, the service's second `unsafe` call, allowed where it stands beside its safety comment, as ADR-0187 allowed the first.

## Alternatives rejected

- **(9a) No bound beyond the body.** Rejected: one request could store 2 MiB in each of its text values, every one of which an event and an export then carry.
- **(9c) A per-attribute `max_length` in the format.** Rejected for now: it is a format addition every flow would have to think about. A store-wide bound closes the abuse, and a narrower bound, where a flow needs one, is a guard such as `length(title) <= 200`.
- **(10) A token in an environment variable, or in the `--evaluator` URL.** Rejected: an environment variable is inherited and shown, and a URL's credentials are logged wherever the URL is, which the transport's log does.
- **(11) A floor as a flag.** Rejected for now: 64 MiB is the space a checkpoint needs at the `-wal`'s limit, a property of the engine rather than of a deployment. An operator who wants an earlier warning alerts on `objectflow_disk_free_bytes`.

## Consequences

- `wire.md` says a string's bound and `/health`'s `disk_free_bytes` and `disk_low`. `integrate.md` §9 says the token and the disk. `core/README.md` lists the flag. The workspace's `unsafe` stays denied, with the service's two calls allowed.
- Held by these tests, each mutation-checked:
  - `test_a_string_holds_at_most_64_kib_alike`, in both backends, counted in bytes;
  - `an_evaluator_is_sent_its_token`, with its refusals at start;
  - `a_volume_is_low_below_what_a_checkpoint_needs` and `a_deployments_edges_are_said`.

  The `503` below the floor is reasoned and not run, since a full volume is not safely made in a test; the floor's comparison is tested at its boundary.
