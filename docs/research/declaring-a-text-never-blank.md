# Declaring a text that is never blank

How established systems declare that a text field holds more than white space, once on the field, and whether they refuse such a value or change it. Asked by issue #69, filed 2026-11-04, which found that a required text input or attribute takes `""` and `"   "`, and that refusing them needs a guard on each act that takes the text. Researched 2026-11-04.

## Findings per source

### Jakarta Bean Validation: `@NotBlank`

A constraint annotation on a field or a property, checked wherever the bean is validated. Its definition: *"The annotated element must not be null and must contain at least one non-whitespace character. Accepts CharSequence."* It names `Character.isWhitespace(char)` as its definition of white space, under *See Also*, and dates the constraint *"Since: 2.0"*. It refuses the value and does not change it.

### Ruby on Rails: the `presence` validation

A validation declared on the model, run on every save through it. *"This validator validates that the specified attributes are not empty. It uses the Object#blank? method to check if the value is either nil or a blank string - that is, a string that is either empty or consists of whitespace."* It refuses the value and does not change it.

### Django: a model field's `blank`, and a form field's `strip`

A model field's `blank`: *"If True, the field is allowed to be blank. Default is False."* The page adds that *"null is purely database-related, whereas blank is validation-related"*, and that *"If a field has blank=False, the field will be required."* A form's `CharField` takes `strip`: *"If True (default), the value will be stripped of leading and trailing whitespace."* A required form field then refuses the empty result: *"By default, each Field class assumes the value is required, so if you pass an empty value – either None or the empty string ("") – then clean() will raise a ValidationError exception"*. So a form refuses white space alone by changing the value first, and keeps the stripped value.

### JSON Schema: `minLength` and `pattern`

*"A string instance is valid against this keyword if its length is greater than, or equal to, the value of this keyword."* A length counts every character, so `"   "` satisfies `minLength: 1`. Refusing white space alone takes a regular expression, `pattern`, whose dialect is ECMA-262's.

### Pydantic: `StringConstraints`

Its `strip_whitespace`: *"Whether to remove leading and trailing whitespace."* Beside it are `min_length`, `max_length` and `pattern`. With `strip_whitespace` and `min_length`, a value is stripped and then counted, and the stripped value is kept.

## What the sources share, and where they part

- **Observed:** each declares the rule once, on the field, not on each operation that writes it.
- **Observed:** two of them refuse the value as sent, Bean Validation and Rails. Two change it before counting it, Django's forms and Pydantic. JSON Schema counts characters and needs a pattern for white space.
- **Observed:** no two name white space alike. Bean Validation names Java's `Character.isWhitespace`, and Rails names Ruby's `blank?`.
- **Inferred:** a named marking, `@NotBlank` and `presence`, is the common way to say it, apart from a general length or pattern.

## ObjectFlow beside the sources

- A guard `trim(inputs.reason) != ""` refuses a blank text from every writer of the act that names it, since 0.15.0 (ADR-0249). It is a rule on each act, not on the text.
- ADR-0249 fixed what white space is: Unicode's `White_Space` property, the same 25 characters in both runners. It rejected trimming a value as it is written, since a request records what was sent.
- The engine refuses its own blank text already: a label's name empty once normalised is refused `invalid_input`, `malformed` (ADR-0161).
- A marking that generates an invariant holds for every write, as `taken_by` does (ADR-0176), and a publish leaves no violation of a new invariant that nobody admitted (ADR-0146).

## What it suggests

A marking on the text, `not_blank: true`, as Bean Validation and Rails name it: the value refused as sent, white space read as `trim` reads it, and an attribute's rule held by a generated invariant, so every write and every publish over held values is checked. Not a length, which counts white space, and not a trim, which changes what was written. Decided in [ADR-0257](../adr/0257-a-text-declared-not-blank-is-never-written-blank.md).

## Sources

Each retrieved 2026-11-04 with `curl -sL`, its markup stripped, and each quotation searched in the text.

| Source | URL | Version |
|---|---|---|
| Jakarta Bean Validation, `NotBlank` | https://jakarta.ee/specifications/bean-validation/3.0/apidocs/jakarta/validation/constraints/NotBlank.html | Jakarta Bean Validation API 3.0.0 |
| Rails Guides, Active Record Validations | https://guides.rubyonrails.org/active_record_validations.html | v8.1.4 |
| Django, Model field reference | https://docs.djangoproject.com/en/5.2/ref/models/fields/ | 5.2 |
| Django, Form fields | https://docs.djangoproject.com/en/5.2/ref/forms/fields/ | 5.2 |
| JSON Schema Validation, draft 2020-12 | https://json-schema.org/draft/2020-12/json-schema-validation | draft 2020-12 |
| Pydantic, `pydantic.types` | https://docs.pydantic.dev/latest/api/types/ (served from https://pydantic.dev/docs/validation/latest/api/pydantic/types/) | latest, as served |
