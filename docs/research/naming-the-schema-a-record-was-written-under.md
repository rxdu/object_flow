# Naming the schema a record was written under

**The question.** Issue #61 (filed 2026-11-03) found that a read answers a moved object's current type beside the version its values were written under. A version that moves a type (`moved_types`, ADR-0229) leaves that pair naming no declaration, or, after a merge into a type the earlier version declared, another type's. A gateway filters personal values by the declaration the pair names (ADR-0206 decision 5). The author decided the same day that every read answering a `declaration_version` also answers the type the object was of under it, and asked for the practice of other platforms to be researched with its ADR. This document asks how proven platforms tell a reader the exact schema a stored record was written under, when names may change after the record was written.

## 1. A schema registry stores the writer's schema identity with each message

**Confluent Schema Registry** (S1, S2). Each serialised message carries the identity of the schema it was written with, a "4-byte schema ID as returned by Schema Registry." The ID names one schema: "A schema ID is the globally unique identifier for a schema definition." A name and a version are a different key: "The same schema ID can appear as version 1 of one subject and version 3 of another." The page warns against using one for the other: "Use “subject version” or “schema ID” when precision matters." A reader configured to read with the latest schema still resolves each message by the identity it carries: "this setting does not override the embedded schema ID in existing messages".

## 2. A data format keeps the writer's schema, and resolves renames when it reads

**Apache Avro 1.12.0** (S3). An object container file's header holds the writer's schema: "avro.schema contains the schema of objects stored in the file, as JSON data (required)." The specification makes this a rule for storage: "Therefore, files or systems that store Avro data should always include the writer’s schema for that data." A renamed type is resolved when it is read, never rewritten in storage: "Aliases function by re-writing the writer’s schema using aliases from the reader’s schema."

**Apache Iceberg** (S8). Each manifest records the schema it was written with, the "ID of the schema used to write the manifest". A column is followed through a rename by an identity that does not change: "The table schema's column names and order may change after a data file is written, and projection must be done using field ids." The rename keeps it: "Renaming an existing field must change the name, but not the field ID."

## 3. Event stores record each event's type as it was written

**Axon Framework** (S4, S5). Each stored event keeps its type name and its revision: "the Event Store stores a revision number as well as the fully qualified name of the Event." A later rename maps the stored identity to the new one when events are read: "A rename may change the qualified name, the version, or both." A reader of the new name reads the old events too: "A filter on CoursePublished also matches events stored as CourseOffered".

**Marten** (S6). Each event's row holds its type's name: "the event type name is stored in the type column of the mt_events table". Mapping two classes onto one stored name loses which class wrote an event: "In this case, old OrderStatusChanged and new ConfirmedOrderStatusChanged event type names will be stored with the same order_status_changed event type."

**KurrentDB** (S7). An event is appended with its type: "Each event should be supplied with an event type." The documentation advises against tying that stored name to the code's type: "However, we recommend against this as it couples the storage to the type and will make it more difficult if you need to version the event at a later date."

## 4. ObjectFlow beside the sources

ObjectFlow stores each event's `declaration_version`, and an object's directory row its current type (`storage-schema.md` §2). A move records the type it left on its own event, `payload.moved.from`, and the core resolves the type an object was of under a version from those events (`request.rs`, `type_then`, ADR-0229 decision 3). No read answered that type. Reproduced 2026-11-03 on the published 0.12.0 (`TODO.md`, issue #61): `get` answered `Category` at version 1, and `declaration {Category, 1}` answered `404`, or, after a merge into a `Category` version 1 declared, Category's own declaration.

*Inferred, which no source states:* the pair a read answered, the current type and an earlier version, is the kind of key S2 calls imprecise, a name with a version where the name has since changed. The sources all keep, beside each record, a key naming exactly the schema that wrote it, and resolve a rename when they read. S1 and S8 store a schema identity, S3 the writer's schema itself, and S4, S6 and S7 the type name as written. ObjectFlow can compute such a key from what it stores already, the object's own move events, without a stored column.

## 5. What it suggests

Answer, beside each `declaration_version`, the type the object was of under that version, so the pair names exactly the declaration that wrote the values, as each source's stored key does. Resolve it from the object's own move events when it is read, as Avro and Axon resolve a rename when they read, rather than rewriting stored events. The decision and its alternatives are ADR-0244.

## Sources

Each retrieved 2026-11-03 with `curl -sL` into its own directory, its markup stripped, and every quotation above found in the stripped text.

| | Source | Version |
|---|---|---|
| S1 | Confluent, *Formats, Serializers, and Deserializers for Schema Registry on Confluent Platform*: https://docs.confluent.io/platform/current/schema-registry/fundamentals/serdes-develop/index.html | the "current" documentation; its footer's date, Apr 11, 2023, predates the releases the page names |
| S2 | Confluent, *Schema Registry Concepts for Confluent Platform*: https://docs.confluent.io/platform/current/schema-registry/fundamentals/index.html | as S1 |
| S3 | Apache Avro, *Specification*: https://avro.apache.org/docs/1.12.0/specification/ | 1.12.0 |
| S4 | AxonIQ, *Event Versioning*, Axon Framework reference: https://docs.axoniq.io/axon-framework-reference/4.13/events/event-versioning/ | 4.13; the 5.4 page, which its canonical link names, lacks the quotation |
| S5 | AxonIQ, *Defining Transformations*, Axon Framework reference: https://docs.axoniq.io/axon-framework-reference/5.4/message-transformation/defining-transformations/ | 5.4 |
| S6 | Marten, *Events Versioning*: https://martendb.io/events/versioning.html | the page says it was last updated 2026-11-02 |
| S7 | Kurrent, *Appending events*, .NET client: https://docs.kurrent.io/clients/dotnet/v1.0/appending-events | client v1.0 |
| S8 | Apache Iceberg, *Spec*: https://iceberg.apache.org/spec/ | format versions 1 to 4; the page gives no date |
