# ADR-0017: File attachments are content-addressed references; the bytes are out of scope

- **Status:** **Accepted** — confirmed by the author 2026-09-08, after the readiness review
- **Date:** 2026-09-07

## Context

The first consumer stores photos (robot, delivery, service and intake-confirmation photos), packing-list PDFs and label templates, and they are not decoration. Intake photos are the labelling-evidence trail; a per-model flag `label_photo_required` makes "every item whose model requires a photo has at least one" a guard on the intake batch's `LABELS_PRINTED → LABELLING_CONFIRMED` transition (`wr:docs/design/state-transition-system.md`). Pre-delivery check records carry optional photo evidence. The files are production data, and ADR-0015 requires them preserved.

The author's stated preference is links to file objects in an S3-like store or on a filesystem. The open question is where ObjectKeeper's responsibility ends.

## Decision

ObjectKeeper defines a **`file` attribute type** (single or list): a reference to a blob held in external storage, carrying the storage key, a content hash, size, media type, and provenance — who attached it, when, and through which transition. It is a controlled attribute: written only as the outcome of a transition or action whose inputs include the reference, so guards can read it (presence, count, media type) and history records it.

The bytes are out of scope. The consumer uploads to the store, then submits the reference. ObjectKeeper never reads or writes blob content, never streams, and never serves files.

Keys are **content-addressed**, derived from the hash, so a reference can be verified against the bytes by anyone holding both, duplicates collapse, and a legacy photo ports to the key it would have had if uploaded through the new path.

The storage backend — an S3-compatible object store or a filesystem — is a deployment choice behind one small interface (put, get-URL, exists, delete), the same way PostgreSQL or SQLite is (ADR-0001).

## Alternatives

### A. Entirely out of scope: the attribute is a plain string

The consumer stores a URL or path in an ordinary attribute and manages the store itself.

Pros: nothing to build; ObjectKeeper stays exactly a layer over a database.

Cons: a guard can check only that a string is non-empty, not that it names a file, what type it is, or who attached it. The evidence trail — the reason the first consumer keeps photos at all — has no provenance. Dangling references are invisible. The port has to invent a convention per consumer for what the string means.

### B. Content-addressed reference, bytes external (chosen)

Pros: guards see presence, count, type and provenance; history records who attached what; the blob store is pluggable and can be the deployment's existing bucket; the guarantee is stated honestly as covering the reference and its provenance, not the bytes; content addressing makes references verifiable and the port deterministic.

Cons: the upload-then-reference two-step is not atomic, so a failed transition can leave an orphan blob — harmless, and reclaimable by a sweep over unreferenced keys. ObjectKeeper cannot itself assert that the blob exists; a guard that needs that is an ADR-0008 external evaluator, and whether it is eager or deferred is that ADR's question. Access control on the bytes belongs to the store, not to ObjectKeeper: a reference visible in ObjectKeeper does not by itself grant a read on the blob, and a deployment must decide whether that is a feature or a gap.

### C. ObjectKeeper owns the blobs

A built-in file object type with its own lifecycle (uploaded → attached → archived), with ObjectKeeper storing or fronting the bytes.

Pros: one trust boundary; existence is guaranteed by the store; deletion and archival cascade naturally; access is mediated end to end.

Cons: ObjectKeeper becomes a file server. Bytes flow through a write path designed for small transactional records; photos and PDFs bring streaming, resumable upload, size limits and CDN concerns that are a different engineering problem. ADR-0001 positions the project as a layer over a database, not a database; this would make it a layer over a database *and* an object store. The additional guarantee — that the bytes exist — is available more cheaply as an external evaluator under B.

## Consequences

- The model block in DESIGN.md gains `file` as an attribute type with the fields above.
- ADR-0015's open item on what "preserved" covers is closed: files migrate to the deployment's blob store under content-addressed keys, and each legacy photo row becomes a `file` reference on its object with provenance `asserted` from the legacy row.
- A storage-backend interface is required, with S3-compatible and filesystem implementations. It should be small.
- A sweep for unreferenced keys is an operational tool, not a runtime feature.
- Whether "the blob exists" is ever a guard, and whether it is eager or deferred, is decided per declaration under ADR-0008.

## Questions closed since

- **Free versus controlled** is moot: ADR-0042 removed the free class, so every file attribute is controlled and every attachment is written by a declared transition with a guard, an actor and an event. That is the stricter of the two readings this ADR offered, and the right one, since a file is potential evidence.
- **Erasure deletes bytes.** ADR-0031 requires it and ADR-0041 states it plainly: ObjectKeeper never reads, streams or serves blob content, and deleting during erasure is its only byte-level operation. The absolute "never touches the bytes" that once appeared in DESIGN.md was false.
- **Retention.** The log is permanent (ADR-0033) and a reference outlives any expiry policy, so **the blob store's own lifecycle expiry must be disabled for an ObjectKeeper artifact store**. Deletion happens only through erasure. A deployment that leaves expiry enabled will produce references whose content is gone; that is an operational misconfiguration, and a periodic reference-versus-store audit is the control that catches it. *(Derived 2026-09-08 from ADR-0033's permanence; derived, not part of what the author confirmed.)*
