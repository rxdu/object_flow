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

## Sources

Each retrieved 2026-11-12 with `curl -sL`, its markup stripped, and each quotation searched in the text.

| Source | URL | Version |
|---|---|---|
| Django, Creating forms from models | https://docs.djangoproject.com/en/5.2/topics/forms/modelforms/ | 5.2 |
| Ruby on Rails API, `ActiveRecord::NestedAttributes::ClassMethods` | https://api.rubyonrails.org/classes/ActiveRecord/NestedAttributes/ClassMethods.html | v8.1.4 |
| TypeScript Handbook, Utility Types | https://www.typescriptlang.org/docs/handbook/utility-types.html | as served; `Omit` released in 3.5 |
| GraphQL Specification | https://spec.graphql.org/October2021/ | October 2021 Edition |
