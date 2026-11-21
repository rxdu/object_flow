# Deriving an act's inputs from the record it makes

**The question.** Issue #88 (filed 2026-11-12) asked that a value hold references, sets, files and `personal` fields, so that a group of inputs about fifteen acts take could be declared once. Its case is a custody record, an observation kind of twelve fields, which each act that changes a unit's hands records from its effect (ADR-0270). Each act fills what it knows and declares the rest as inputs, about nine each. The author asked on 2026-11-12 for the design that serves ObjectFlow's long term, not only the mechanism asked for. This note asks how established systems let a request take the fields of a record it makes without declaring them again.

## Findings per source

### Django: a `ModelForm` derives its fields from a model

Django's model forms say why a form takes its fields from a model: "If you're building a database-driven app, chances are you'll have forms that map closely to Django models. [...] it would be redundant to define the field types in your form, because you've already defined the fields in your model." A form names its model, and its fields follow, all of them with `fields = "__all__"`, or all but some with `exclude`: "Set the exclude attribute of the ModelForm's inner Meta class to a list of fields to be excluded from the form."

A field the form leaves out is the code's to fill. "Django will prevent any attempt to save an incomplete model, so if the model does not allow the missing fields to be empty, and does not provide a default value for the missing fields, any attempt to save() a ModelForm with missing fields will fail." The code then sets each before saving: "you can use save(commit=False) and manually set any extra required fields". The model's own rules hold the form's input: "Model validation (Model.full_clean()) is triggered from within the form validation step, right after the form's clean() method is called."

### Ruby on Rails: nested attributes save a record through its parent

Rails lets one request carry an associated record's attributes inside the parent's: "Nested attributes allow you to save attributes on associated records through the parent. [...] When you enable nested attributes an attribute writer is defined on the model." The writer is named after the association, as `author_attributes=`, so the associated record's fields arrive nested under one key, and the associated model declares them.

### TypeScript: `Omit` derives one type from another

TypeScript derives a type from another's properties less some: `Omit<Type, Keys>` "Constructs a type by picking all properties from Type and then removing Keys (string literal or union of string literals)." Its example derives `TodoPreview` from `Todo` less `description`; *inferred*, which the page does not state, a property added to `Todo` reaches `TodoPreview` without editing it.

### GraphQL: an input object is declared apart from the output type

GraphQL declares a separate type for input: "A GraphQL Input Object defines a set of input fields; the input fields are either scalars, enums, or other input objects. This allows arguments to accept arbitrarily complex structs." It gives the reason it does not reuse an output type: "The GraphQL Object type (ObjectTypeDefinition) defined above is inappropriate for re-use here, because Object types can contain fields that define arguments or contain references to interfaces and unions, neither of which is appropriate for use as an input argument."

## What the sources share, and where they part

- **Django and TypeScript derive the input from the record's declaration**, so a field added to the record reaches every input derived from it. Django's form leaves out by name what the code fills, as TypeScript's `Omit` removes keys.
- **The record's rules hold the derived input**, in Django by the model's validation run from the form's.
- **The shape differs.** Django's form fields are flat, the form's own. Rails nests them under the association's name. GraphQL declares a separate input type, *inferred* from its stated reason to be because its output types hold what an input cannot: fields with arguments, interfaces and unions.

## ObjectFlow beside the sources

- **The record is already declared once.** An observation kind's fields may be references, `personal` texts, sets of references, files and sets of files, with the kind's `invariants`. A kind declaring each of these passed the reference checker clean on 2026-11-12 at `c4bb0af`. The engine stores them, holds references, and erases personal fields by the generated `forget` (ADR-0146, ADR-0147).
- **An act records it from its effect** since 0.23.0 (ADR-0270), naming each input of the kind's `record`. Step 3 refuses a `create` that leaves out a field the record requires, or passes one that does not fit (`effect_recording_plants`).
- **What #88 still costs**: each act declares again each field it does not fill, about nine on each of about fifteen acts, and passes each one into the `create`. An optional field added to the kind is passed by no act until each is edited.
- **A kind holds nothing an input cannot.** Its fields are attributes, with no arguments, interfaces or unions, so GraphQL's reason for a separate input type does not apply.
- **What values with references would take** (ADR-0254 decision 9's later slices): a reference, a set or a file kept in a holder's columns, with its integrity, its `referrers`, its index rows and its erasure, on both backends. No case yet needs a value held on a type with such a field.

## What it suggests

*A recommendation for the author, not a decision.*

Derive an act's inputs from the record it makes, as Django's `ModelForm` derives a form's fields from its model: a `create` names the inputs the act fills, and each other input of the target becomes an input of the act, of the same name, type, optionality and `personal` marking, passed to the target. The record stays the one declaration, and its rules hold the derived inputs as the model's hold the form's.

- Flat, as Django's form is, so the wire, the agent tools, an event's payload and the erasure of a personal input stay as they are. Each derived input carries a marker naming the record it comes from, so a form or a tool may show them as one group, as Rails' nesting does.
- `subject` and `corrects` are never derived: the act names the subject, and a correction stays a request of its own.
- A derived input's type is read in the target's module, so the act's module imports the target alone.
- Within a loop as well, since the derived inputs are the request's, the same for each element.
- A derived name another input of the act already takes is refused, the act filling that field itself.
- Values holding references, sets, files and `personal` fields, ADR-0254's later slices, stay unbuilt until a value held on a type needs them.

## Many records in one request (2026-11-21)

**The question.** Issue #102 (filed 2026-11-20) asked that one act take a list of entries, each naming an object with fields of its own, so that it creates several objects, each with its own inputs. Its case is an item admitted with its lines, each line naming a model, how many units, and whether the customer may decline it. The issue asked for a declared input holding a list of values whose fields may be references. The author directed on 2026-11-21 that the open issues be reviewed and addressed. This section asks how established systems take several related records in one request.

### Ruby on Rails: an array of hashes, one record each, bounded and atomic

Rails takes a collection's records as an array: "include the key :posts_attributes with an array of hashes of post attributes as a value. For each hash that does not have an id key a new record will be instantiated". Each hash holds the associated model's own attributes. The count is bounded, and past the bound nothing is taken: ":limit Allows you to specify the maximum number of associated records that can be processed with the nested attributes. [...] If the size of the nested attributes array exceeds the specified limit, NestedAttributes::TooManyRecords exception is raised." The records are saved with their parent, all or none: they "are saved and destroyed automatically and atomically when the parent model is saved. This happens inside the transaction initiated by the parent’s save method."

### Django: an inline formset, one model form per related object

Django builds the related objects' forms from the related model: "Inline formsets is a small abstraction layer on top of model formsets. These simplify the case of working with related objects via a foreign key." Its example, `inlineformset_factory(Author, Book, fields=["title"])`, names the related model and the fields each form takes. The number of forms is bounded: "The absolute_max parameter to formset_factory() allows limiting the number of forms that can be instantiated when supplying POST data. This protects against memory exhaustion attacks using forged POST requests". Past it, its example's formset is not valid, saying "Please submit at most 1000 forms."

### Odoo: a create command per related record

Odoo writes a record's related records by commands: "One2many and Many2many fields expect a special command to manipulate the relation they implement." Its `create(values)` command does this: "Create new records in the comodel using values, link the created records to self." The values are the related model's fields.

### GraphQL: a list of an input type declared apart

GraphQL's list is ordered: "Lists are ordered sequences of values wrapped in square-brackets [ ]." An input list takes each item by its item type: "When expected as an input, list values are accepted only when each item in the list can be accepted by the list’s item type." That item type is an input object, declared apart from any output type (above).

### What the sources share, and where they part

- **Rails, Django and Odoo take each entry as the related record's own fields**, declared once by its model, and make one record per entry.
- **Each source takes the entries as an ordered list.** Rails and Django bound how many, and refuse the whole past the bound rather than taking part of it.
- **Rails makes the records with their parent, in one transaction.**
- **GraphQL declares the entry's type apart**, for the reason given above, its output types holding what an input cannot.

### ObjectFlow beside the sources

- **A field cannot differ by entry unless a loop's collection holds it.** `rest: inputs` gives a creation the request's inputs, the same for each element of a loop, and a field that differs by element is named by the step (ADR-0271 decision 5). A loop's item is an element of a set of references, a stored set or a range, so a line's model may differ by element, but a quantity chosen in the request may not.
- **A set cannot carry the entries.** On the wire a set is an array with no element repeated (`guides/wire.md`), and a loop over one runs in its own order, not one the caller chose. Two sets, of models and of quantities, cannot be paired.
- **A value cannot carry them either.** A value's fields are built-in types and enumerations, references among ADR-0254 decision 9's later slices, and a declared input holds one value, never a set of them. Both were refused at the 0.31.0 release build, `f1d6841` (`TODO.md`, *Issues filed on GitHub*).
- **The line's fields are already declared once**: as the inputs of the line type's initial transition, with that type's guards and invariants holding them, as the related model declares and validates its fields in Rails, Django and Odoo.
- **A request is already all or nothing.** A creation an effect makes that is refused refuses the act whole, `call_refused` naming its step (`guides/errors.md`).

### What it suggests

*A recommendation for the author, not a decision.*

Take each entry as the rest of the target's inputs, as Django derives each inline form from the related model and Rails takes the related record's own attributes: a `create` step names the act's input that lists the entries, and makes one object per entry, in the order given, by the target transition. The step names the inputs that are the same for every entry, and each entry holds the rest, each declared as the target declares it.

- A required bound, the whole refused past it, as Rails' `:limit` and a loop's `limit` do.
- An entry may repeat another, each making its own object, since the list is not a set.
- One request, all or nothing, as Rails saves nested records in its parent's transaction and as an act's creations already are.
- A declared list of values whose fields may be references, GraphQL's form and the issue's, left until a case needs entries that make something other than one record each.

## Sources

Each retrieved 2026-11-12 with `curl -sL`, its markup stripped, and each quotation searched in the text, and again 2026-11-21 with the two sources added that day for the section on many records.

| Source | URL | Version |
|---|---|---|
| Django, Creating forms from models | https://docs.djangoproject.com/en/5.2/topics/forms/modelforms/ | 5.2 |
| Ruby on Rails API, `ActiveRecord::NestedAttributes::ClassMethods` | https://api.rubyonrails.org/classes/ActiveRecord/NestedAttributes/ClassMethods.html | v8.1.4 |
| TypeScript Handbook, Utility Types | https://www.typescriptlang.org/docs/handbook/utility-types.html | as served; `Omit` released in 3.5 |
| GraphQL Specification | https://spec.graphql.org/October2021/ | October 2021 Edition |
| Django, Formsets | https://docs.djangoproject.com/en/5.2/topics/forms/formsets/ | 5.2 |
| Odoo, ORM API | https://www.odoo.com/documentation/17.0/developer/reference/backend/orm.html | 17.0 |
