# Following a referenced object's transitions

**The question.** Issue #103 (filed 2026-11-21) asked that an object follow a transition of the object it refers to, in the same request, declared by the referring type, so that a module the referred-to one may not import can still have its objects end or move with another's. The author directed on 2026-11-21 that the open issues be checked. This note asks where established systems declare what a referring record does when the record it refers to is deleted or changed: on the referencing side or on the referenced side. It also asks whether the referenced side knows of the rule, whether the action is atomic with the change, and whether the referring record's own rules can refuse it.

## Findings per source

### PostgreSQL: the referencing table declares the action

A foreign key is written on the referencing table, `REFERENCES reftable [ ( refcolumn ) ] [ MATCH matchtype ] [ ON DELETE referential_action ] [ ON UPDATE referential_action ]`, its actions `NO ACTION`, `RESTRICT`, `CASCADE`, `SET NULL` and `SET DEFAULT`. The page says: "In addition, when the data in the referenced columns is changed, certain actions are performed on the data in this table's columns. … Referential actions are executed as part of the data changing command, even if the constraint is deferred." The referencing table's own rules still bind: "Note that these do not excuse you from observing any constraints", and "The PostgreSQL behavior is for BEFORE DELETE to always fire before the delete action, even a cascading one."

### Django: the `ForeignKey` declares `on_delete`

"When an object referenced by a ForeignKey is deleted, the referring objects need updating. The on_delete argument specifies how this is done, and whether Django or your database makes the updates." A `ForeignKey` "Requires two positional arguments: the class to which the model is related and the on_delete option". For `CASCADE`, "Django emulates the behavior of the SQL constraint ON DELETE CASCADE and also deletes the object containing the ForeignKey. Model.delete() isn’t called on related models, but the pre_delete and post_delete signals are sent for all deleted objects." The documentation does not say whether this is atomic. Django's source runs the deletion inside `transaction.atomic`, so a `pre_delete` receiver that raises rolls the whole deletion back.

### Odoo: the `Many2one` declares `ondelete`

`odoo.fields.Many2one` takes "ondelete (str) – what to do when the referred record is deleted; possible values are: 'set null', 'restrict', 'cascade'". The referred-to model's `One2many` is optional and takes no such parameter. Odoo 20.0's source turns `ondelete` into a database foreign key, so PostgreSQL's rule above applies. Its source runs the deletion checks of the records deleted directly, before the foreign key cascades, so a cascade-deleted record's own checks do not run, which no page read says. The documentation of Odoo's development version says a later release will run them: "The methods decorated with ondelete() are then called on all the records to delete, including the cascade-deleted ones, before anything is removed from the database."

### Salesforce: the child's relationship field declares it

"If the lookup field is optional, you can specify one of three behaviors to occur if the lookup record is deleted": "Clear the value of this field" ("This is the default."), "Don’t allow deletion of the lookup record that’s part of a lookup relationship", and "Delete this record also". For a master-detail relationship, "you always create the relationship field on the detail object", and "deleting a master record also deletes related detail and subdetail records." The child's triggers do not run on a cascade: "Cascading delete operations. Only records that initiate a delete cause trigger evaluation." No page read says whether the cascade shares the master's transaction.

### Rails, the contrast: the parent declares `dependent`

`has_many :books, dependent: :destroy` is declared on the parent, and "Controls what happens to the associated object when its owner is destroyed". With `:destroy`, the children's callbacks run, and "The whole callback chain is wrapped in a transaction. If any callback raises an exception, the execution chain gets halted and a rollback is issued". With `:destroy_async`, "an ActiveRecord::DestroyAssociationAsyncJob job is enqueued", outside that transaction.

## What the sources share, and where they part

| System | Declared on | Referenced side declares it? | Atomic with the change? | Referring side's rules can refuse? |
|---|---|---|---|---|
| PostgreSQL | the referencing table | no | yes, in the same command | yes: its constraints and triggers |
| Django | the referencing `ForeignKey` | no | yes, by its source; its documentation is silent | partly: `pre_delete` receivers |
| Odoo 20.0 | the referencing `Many2one` | no | yes, as a database foreign key | no in 20.0; its development documentation says a later release will |
| Salesforce | the child's relationship field | no | not found in the pages read | no: a cascade fires no child trigger |
| Rails | the parent's `has_many` | yes | yes, but `:destroy_async` | yes, with `:destroy` |

- **Four of the five declare the rule on the referencing side**, and there the referenced side declares nothing of the referring one. Rails declares it on the parent, as a parent callback.
- **Where the rule is declared on the referencing side, its actions are fixed**: delete, clear, set a default, or refuse. None runs a transition of the referring record's own declaration.
- **Whether the referring side's own rules can refuse the change differs.** PostgreSQL and Rails let them, Salesforce and Odoo 20.0 do not, and Odoo's development documentation moves to letting them.

## ObjectFlow beside the sources

- **A cascade is declared on the whole's composite end** (`flow-format.md` §4.3), the placement Rails uses, and needs the whole to name its parts. Both ends of a relationship are in one module (§4.3), so a module cannot declare an end on a type it imports.
- **A part's guards can refuse its whole's transition** (ADR-0019), as PostgreSQL's constraints and Rails' callbacks can. ADR-0019 rejected reacting to events after the fact, since that is neither atomic nor able to refuse.
- **A guard reading `referrers` refuses a transition while live references stand** (ADR-0024 decision 2), which is SQL's `RESTRICT` written on the referenced side.

## What it suggests

*A recommendation for the author, not a decision.*

Let the referring reference declare which transitions of the referred-to object it follows, as four of the five sources declare the rule on the referencing side, so the referred-to module need not know the referring one. Run each follower's own declared transition, not a fixed action. The follower's guards then refuse the whole change as a cascaded part's do. That is how PostgreSQL's constraints and Rails' `:destroy` callbacks refuse it, and how ADR-0019 requires a cascade to.

## Sources

Each retrieved 2026-11-21. Salesforce's help and developer pages served only a script to a plain fetch and were rendered in a headless browser. Each quotation was compared with the fetched text.

| Source | URL | Version |
|---|---|---|
| PostgreSQL, `CREATE TABLE` | https://www.postgresql.org/docs/current/sql-createtable.html | 19 |
| PostgreSQL, constraints | https://www.postgresql.org/docs/current/ddl-constraints.html | 19 |
| PostgreSQL, `CREATE TRIGGER` | https://www.postgresql.org/docs/current/sql-createtrigger.html | 19 |
| Django, model field reference | https://docs.djangoproject.com/en/stable/ref/models/fields/ | 6.1 |
| Django, `deletion.py` | https://github.com/django/django/blob/stable/6.1.x/django/db/models/deletion.py | 6.1 |
| Odoo, ORM API | https://www.odoo.com/documentation/20.0/developer/reference/backend/orm.html | 20.0, and the development version |
| Salesforce, object relationships | https://help.salesforce.com/s/articleView?id=platform.overview_of_custom_object_relationships.htm&type=5&language=en_US | Winter '27 |
| Salesforce, data modeling | https://trailhead.salesforce.com/content/learn/modules/data_modeling/object_relationships | Trailhead |
| Salesforce, Apex triggers | https://developer.salesforce.com/docs/atlas.en-us.apexcode.meta/apexcode/apex_triggers_ignoring_operations.htm | API 68.0 |
| Rails, associations | https://guides.rubyonrails.org/association_basics.html | 8.1.4 |
| Rails, callbacks | https://guides.rubyonrails.org/active_record_callbacks.html | 8.1.4 |
