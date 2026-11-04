# Declaring a value of several fields once

**The question.** Issue #65 (filed 2026-11-04) asked whether a flow may name a repeated part once. Its second case is a group of inputs that several transitions take, each transition declaring the group again. Its third is a group of attributes declared once with its rules, which a type holds twice, such as proposed dates beside the dates they would replace, the same conditions declared on each. The correction posted on the issue the same day put the two as one design, a value of several fields declared once with its rules, which the format lacks (`TODO.md`, the item after #65). The author directed the same day that it be researched first ("go with your recommendations"). This document asks how proven platforms declare such a value and its rules, hold it more than once, store it, and change it.

## 1. A modelling language: a value without identity, its rules its own

**UML 2.5.1** (S1). A data type is told apart from a class by identity: "DataType differs from Class in that instances of a DataType are identified only by their value." One with attributes is a structured data type: "If a DataType has attributes (i.e., Properties owned by it and in its namespace) it is called a structured DataType." Two such values are equal by their fields: "Instances of a structured DataType are considered to be equal if and only if the structure is the same and the values of the corresponding attributes are equal." A data type is a classifier, a classifier a namespace (§9.9.4), and a namespace owns rules: "ownedRule : Constraint [0..*]" … "Specifies a set of Constraints owned by this Namespace."

**Martin Fowler, *Value Object*** (S2). A compound of values is the common case: "A date range consists of start and end dates, which themselves can be compounds of year, month, and day." It is equal by its fields: "Objects that are equal due to the value of their properties, in this case their x and y coordinates, are called value objects." Domain-driven design draws the same line: "In Domain-Driven Design the Evans Classification contrasts value objects with entities."

## 2. A relational database: a composite type, its rules on a domain over it

**PostgreSQL 16.15** (S3 to S5). A composite type lists fields and their types: "A composite type represents the structure of a row or record; it is essentially just a list of field names and their data types." A column takes it as its type: "For example, a column of a table can be declared to be of a composite type." It takes no rules: "only field names and types can be specified; no constraints (such as NOT NULL ) can presently be included." The manual gives the rules to a domain over it: "(To work around this, create a domain over the composite type, and apply the desired constraints as CHECK constraints of the domain.)" A domain holds a rule for every column of its type: "Domains are useful for abstracting common constraints on fields into a single location for maintenance." A field is renamed in place: "RENAME ATTRIBUTE This form is only usable with composite types. It changes the name of an individual attribute of the type."

## 3. An object-relational mapping: an embeddable, its rules validated with its holder

**Jakarta Persistence 3.1** (S6). An embeddable has no identity of its own: "Instances of these classes, unlike entity instances, do not have persistent identity of their own. Instead, they exist only as part of the state of the entity to which they belong." It is not shared: "Embedded objects belong strictly to their owning entity, and are not sharable across persistent entities." Its fields are mapped into the holder's table, a field's column named again by an override: "The AttributeOverride annotation may be applied to an entity that extends a mapped superclass or to an embedded field or property to override a Basic mapping".

**Jakarta Validation 3.0** (S7). The rules declared on a value's class are checked with the object that holds it: "Consider the situation where bean X contains a field of type Y . By annotating field Y with the @Valid annotation, the Validator will validate Y (and its properties) when X is validated."

## 4. Data formats: a record as a field's type, its fields renamed by alias or number

**Apache Avro 1.12.0** (S8). A record lists fields, each with a type: "fields : a JSON array, listing fields (required)." A field takes other names, for a rename: "aliases : a JSON array of strings, providing alternate names for this field (optional)." A field added later takes a default, which reading an older record uses: "if the reader’s record schema has a field that contains a default value, and writer’s schema does not have a field with the same name, then the reader should use the default value from its field."

**Protocol Buffers** (S9). A message is a field's type: "You can use other message types as field types." A field is identified on the wire by its number, never reused: "Field numbers should never be reused."

## 5. ObjectFlow beside the sources

ObjectFlow's types are all objects with an identity. A part, the nearest construct, is an object of its own type: "the referenced objects are its parts" (`flow-format.md` §4.3). A family shares attributes, derived attributes and invariants among types, never within one (§4.14). A condition is declared on one type (§4.7). So a group a type holds twice, or one input several transitions take, has no form. Its rules are declared again for each group, and its inputs again on each transition.

The store gives each stored attribute one column (`storage-schema.md` §3), in the types of §3.1, and an event's payload maps each attribute name to its value as JSON (§4). A version renames an attribute by name (`flow-format.md` §4.16, `renamed_attributes`), and erasure redacts each attribute marked `personal` (`storage-schema.md` §9).

A request writes the object's new state and then runs its effect (`DESIGN.md` §6, step 5). UML runs a transition's effect after it exits the source state and before it enters the target (S1, §14.2.3.9.6). Probed on the published 0.15.0 with both checkers and both runners (`TODO.md`, #65): a transition into a final state cannot call an internal transition on its own object, which check 40 refuses at a final state.

*Inferred, which no source states:* every source separates a value, equal by its fields, from an object with an identity, and gives the value's rules to the value. UML's data type owns its constraints (S1). PostgreSQL puts them on a domain over the composite type (S3, S4). Jakarta Validation checks them wherever an embeddable is held (S7). Two ways to store such a value appear: its fields as columns of the holder's table (S6), or one column of a composite type (S3). Only the first fits both of ObjectFlow's backends, since SQLite has no composite type.

## 6. What it suggests

Declare a value of several fields once, as UML's structured data type is, with its own rules. An attribute or a declared input then names it as its type, any number of times, its rules applying to each, as Jakarta Validation applies an embeddable's. Store its fields as columns of the holder's table, as Jakarta Persistence maps an embeddable, so a filter and an index read a field as they read an attribute. Rename a field by name, as PostgreSQL's `RENAME ATTRIBUTE` and Avro's aliases do, and carry it into every holder's columns.

Questions for the ADR:

1. Whether a value's rules are invariants alone, as UML's constraints are, or also conditions a guard names. #65's rules are conditions.
2. Whether a field may be `personal`, erased by itself or with its value.
3. How the wire and the event payload answer a value: a nested object, or each field by a dotted name.
4. Whether a transition into a final state may run the shared steps a self-call names (§5), which check 40 now refuses.

The decision and its alternatives are the author's, in an ADR.

## Sources

Each retrieved 2026-11-04 with `curl -sL` into its own directory, its markup stripped (a PDF by `pdftotext`), and every quotation above found in the stripped text.

| | Source | Version |
|---|---|---|
| S1 | OMG, *Unified Modeling Language*, §10.2.3.1, §9.9.4, §7.8.10 and §14.2.3.9.6: https://www.omg.org/spec/UML/2.5.1/PDF | 2.5.1, December 2017 |
| S2 | Martin Fowler, *Value Object*: https://martinfowler.com/bliki/ValueObject.html | 14 November 2016 |
| S3 | PostgreSQL, *Composite Types*: https://www.postgresql.org/docs/16/rowtypes.html | 16.15 |
| S4 | PostgreSQL, *CREATE DOMAIN*: https://www.postgresql.org/docs/16/sql-createdomain.html | 16.15 |
| S5 | PostgreSQL, *ALTER TYPE*: https://www.postgresql.org/docs/16/sql-altertype.html | 16.15 |
| S6 | Jakarta Persistence, §2.6 and §11.1.4: https://jakarta.ee/specifications/persistence/3.1/jakarta-persistence-spec-3.1.html | 3.1, final release March 04, 2022 |
| S7 | Jakarta Bean Validation, §5.1.3: https://jakarta.ee/specifications/bean-validation/3.0/jakarta-bean-validation-spec-3.0.html | 3.0, final release 2020-07-04 |
| S8 | Apache Avro, *Specification*, records and schema resolution: https://avro.apache.org/docs/1.12.0/specification/ | 1.12.0 |
| S9 | Protocol Buffers, *Language Guide (proto 3)*: https://protobuf.dev/programming-guides/proto3/ | as retrieved; the page names no release |

Salesforce's compound fields were to be a source; its documentation pages returned no text to `curl`, and are left out.
