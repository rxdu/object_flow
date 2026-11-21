# ADR-0291: An entry's faults, its list's default and its redaction on the wire

- **Status:** **Decided** 2026-11-21, accepted in advance under the author's direction of 2026-09-28 as confident recommendations, the build of ADR-0290 having found each point open in its text. Built the same day. Each may still change before 0.32.0 is cut, and is fixed once it is (ADR-0180 decision 2).
- **Date:** 2026-11-21
- **Refines:** ADR-0290 decisions 3, 4, 5 and 6
- **Relates to:** ADR-0122 (the order of refusals); `docs/guides/wire.md`, *What `/v1` promises*; `docs/guides/errors.md`, *Input reasons*; issue #102

## Context

ADR-0290 lets a `create` take `each: <name>`, so an act takes a list of entries and creates one object for each. Its text names the fields a refusal adds and the keys `declaration` gives the list. Building it found six points of the wire its text leaves open, each answerable two ways by two readers.

Three precedents bear on them:

- **A set's element.** A set holding an element of the wrong type is refused with that element's reason, and one repeating an element `malformed` (`core/crates/objectflow-core/src/wire.rs`, `decode`).
- **An input's `default`.** It is an expression: a string, or a YAML boolean or number (`flow.schema.json`, `definitions/expression`). A derived set's default is the text `"{}"` (ADR-0275).
- **A `redact` event's `redacted`.** It names the inputs and attributes whose values the erasure redacted, never a path within one.

## Decision

1. **`reasons` gives a list with an entry at fault its first fault's reason**, by entry and then by field, as a set takes its element's. `entries` gives every fault.
2. **An entry that is no object is named in `entries` as `{entry, input: null, reason: "wrong_type"}`.**
3. **`declaration` gives the list's `default` as the text `"[]"`**, as a derived set's is `"{}"`.
4. **A `redact` event on the act's object names the list in `redacted`** where it redacts a field of one of its entries, as it names an input whose value it redacted.
5. **`entries` and `entry` appear only where they apply**: `entries` on an `invalid_input` where an entry is at fault, and `entry` on a `call_refused` for a creation an `each` made.
6. **`over_limit` is refused once the inputs given are read as their types, and before what the transition decides as it is taken**: an input naming no object, a blank text, or one left out. A request filed for approval is refused in the same order as one executed (`DESIGN.md` §6, *The order of refusals*). A list over its limit is refused with its entries unread. A request with no list refuses as before.

## Alternatives rejected

- **A new reason for the list, `entries`.** It says only that `entries` holds the faults, and a set already takes its element's reason.
- **`malformed` for the list.** It names a value of the right type in the wrong form, and an entry left without a required field is not that.
- **No reason for the list.** `reasons` gives each name in `inputs` its reason (`errors.md`).
- **An entry that is no object marking the list `wrong_type` alone.** A form could then not mark the row at fault.
- **`declaration`'s `default` as the JSON array `[]`.** It gives `default` a type an expression never has. A `/v1` release never changes a field's type (`wire.md`).
- **A path in `redacted`, as `lines[1].note`.** It changes what a name there means, as ADR-0290 rejected a path in `invalid_input`'s `inputs`.
- **`entries` and `entry` on every refusal of their kind, as `[]` and `null`.** Every refusal without a list would carry a field that says nothing.
- **`over_limit` after every `invalid_input`.** A request executed decides an input naming no object, a blank text and an input left out as its transition is taken, beside the defaults it evaluates. The count is the request's own, checked with the inputs before any rule runs (ADR-0290 decision 4).
- **A request filed for approval keeping its own order.** One request would then meet `over_limit` executed and `invalid_input` filed.

## Consequences

- **The docs:** `errors.md` gives each refusal's fields and the list's reason; `wire.md` and `library-api.md` §6 give `declaration`'s default; `storage-schema.md` §9 the `redact` event; `DESIGN.md` §6 the order of refusals.
- **The core, `flowrun.py` and the Studio's sandbox** answer alike, held by the backends-agree tests and the core's on both backends.
