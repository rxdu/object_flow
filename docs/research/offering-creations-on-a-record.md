# Offering, on a record, the creation of a related record

**The question.** When an act on a record is to create another record that references it, how does a platform decide that the act is offered on this record, and that it is open now? Issue #54 (filed 2026-11-01) asks for it: `availability` of an object offers its own type's transitions and `record` for each observation kind on it (ADR-0152 decision 5), and no creation of another type that takes the object as an input. So a consumer decides whether such a creation is open by reading the object's state, restating the guards the creation already declares. This document records how record platforms, case management and hypermedia formats place such an act, and what that suggests for ObjectFlow.

## 1. Record platforms declare the act on the parent

**Salesforce, object-specific "Create a Record" actions** (S1). An admin declares the action on the parent object: "From the management settings for the object for which you want to create an action, go to Buttons, Links, and Actions." and "For a Create a Record action, select the type of object to create." The relationship is filled by the platform: "For example, you add an object-specific action on the Account object that creates contacts. If a user creates a contact with that action on the detail page for the Acme account, that new contact is associated with Acme." Where the child references the parent more than once, the admin chooses: "If the object for which you're creating the action has multiple relationships with the target object, select the field to populate when a record's created." The parent's id wins over a value set beside it: "If you set a predefined value for the lookup field that's used as the action's relationship field, the action ignores the predefined value and uses the parent record ID instead." The child's own rules still decide the save, "because create actions respect validation rules and field requiredness".

**Salesforce, Dynamic Actions** (S2). Whether the action shows is a filter on the parent page's placement of it: "Click any action and click Add Filter to assign visibility rules for the action. You could add a rule that only displays the action if a user has a specific profile, or if the record data meets a specific criteria". *Inferred:* a condition the child's validation rules already state is written again as the filter, as issue #54's consumer restates a guard.

**Microsoft Dataverse, subgrids and commands** (S3 to S5). A maker places a subgrid of related rows on the parent's form: "From the subgrid, app users can create new rows or open an existing row to edit." The new row takes the parent: "Column mapping lets you set default values for a row that is created in the context of another row." A command's visibility is a formula of its own, "Defines logic for hiding or showing the button when running the app.", and the documented example of a command creating a related row is `Patch(Tasks,Defaults(Tasks),{Regarding:Self.Selected.Item},{Subject:"Subject of the Task"})`. *Inferred:* the visibility formula, like Salesforce's filter, is a second statement of when the act is open.

## 2. Case management lets the engine decide, per case

**OMG CMMN 1.1, planning tables** (S6). A model declares which items may be planned in a context: "A PlanningTable defines the scope of planning, in terms of identifying a sub-set of PlanItemDefinitions that can be considered for planning in a certain context." Each carries a rule the engine evaluates over that case's data: "ApplicabilityRules are used to specify whether a TableItem is "applicable" ("eligible," "available") for planning, based conditions that are evaluated over information in the CaseFile." What is offered is what the rule allows: "During planning the only DiscretionaryItems that MUST be shown to the Case Worker (in the authorizedRoleRef) are those, for which the ApplicabilityRule evaluates to TRUE." and "If no ApplicabilityRule is associated with a TableItem, its applicability is considered TRUE."

**Flowable, manual activation** (S7). A plan item whose manual activation rule holds "becomes *ENABLED* instead of *ACTIVE*", waiting for a person to start it, and the engine answers which are enabled: "A typical use case is showing a list of buttons of potential plan item instances that can currently be started by the end user."

## 3. Hypermedia lets the server list the acts

**Siren** (S8). "Actions show available behaviors an entity exposes." An action may create elsewhere: its example on an order posts to `http://api.x.io/orders/42/items`, carrying the order's number as a hidden field. The README states no rule for leaving out an action that is not open now.

## 4. ObjectFlow beside them

- **No source infers the act from the child's schema alone.** Salesforce's action and Dataverse's subgrid are placed by an admin or a maker, CMMN's items are declared in a planning table, and Siren's actions are the server's choice. Each takes the reference from the schema and fills it with the parent.
- **ObjectFlow's rule lives once, on the creation.** A creation's guards already say when it is open, as a Salesforce child's validation rules do at save. Salesforce and Dataverse state the condition again on the parent's side; CMMN and Flowable evaluate the one rule in the engine, per case. ObjectFlow's offer is of the second kind (ADR-0152), and F3 has an actor discover through the engine what it may do now.
- **The declaration cannot sit on the parent's type.** A module imports the modules below it, and the created type's module imports the object's, so the object's type cannot name a creation above it (issue #54's option 2). A mark on the creation, naming the input that takes the object, points the same way the imports do.
- **The implied rule would offer more than an author placed.** In the shipped sample flows (`flow-format/examples`), counting each requestable initial transition's required single references by a script, without the effects that assign them: each `User` would be offered `Space.create`, as the space's lead, and `WorkItem.create`, as its reporter; each `Space`, five creations; each `Employee`, `Claim.draft`. Every deployment's offers would grow without a change to its flows.
- **Today's clients read `creates` as a recording.** The Flow Studio fills the input `subject` with the object for any offer whose `creates` is set (`Offers.tsx`, `take`), as `wire.md` documents for `record`. An offer of another creation needs to say which input it fills.

## 5. What it suggests

These are recommendations for the author, not decisions.

1. **A creation says where it is offered**, naming the required single-reference input that takes the object, as a Salesforce admin chooses the relationship field and a CMMN modeller declares a planning table. Nothing is offered that its author did not place.
2. **The engine decides the offer from the creation's own guards**, evaluated per object as CMMN's applicability rule is per case, with the named input bound to the object. Nothing restates the rule.
3. **The offer names the input it binds**, so a client requests the creation with that input set, as Salesforce fills the relationship field with the parent's id.

## Sources

Each retrieved 2026-11-01 with `curl -sL` into its own directory, its markup stripped or a PDF's text read with `pdftotext`, and every quotation above found in the text. A GitHub-hosted page's version is the latest commit to its file.

| | Source | Version |
|---|---|---|
| S1 | Salesforce, Quick Actions Implementation Guide (PDF): https://resources.docs.salesforce.com/latest/latest/en-us/sfdc/pdf/actions_impl_guide.pdf | Winter '27, last updated July 22, 2026; the server sends no intermediate certificate, so it was fetched with DigiCert's published intermediate added, verification on |
| S2 | Salesforce Admins blog, *Try Dynamic Actions in App Builder with the Summer '20 Release*: https://admin.salesforce.com/blog/2020/try-dynamic-actions-in-app-builder-with-the-summer-20-release | 2020-06-16 |
| S3 | Microsoft Learn, Add and configure a subgrid component on a form: https://learn.microsoft.com/en-us/power-apps/maker/model-driven-apps/form-designer-add-configure-subgrid | `MicrosoftDocs/powerapps-docs` at `2b07328`, ms.date 2026-08-10 |
| S4 | Microsoft Learn, Map table columns: https://learn.microsoft.com/en-us/power-apps/maker/data-platform/map-entity-fields | `MicrosoftDocs/powerapps-docs` at `9d5121c`, ms.date 2025-08-07 |
| S5 | Microsoft Learn, Use Power Fx with commands: https://learn.microsoft.com/en-us/power-apps/maker/model-driven-apps/commanding-use-powerfx | `MicrosoftDocs/powerapps-docs` at `c1037a1`, ms.date 2026-08-06 |
| S6 | OMG, Case Management Model and Notation 1.1: https://www.omg.org/spec/CMMN/1.1/PDF | formal/16-12-01, December 2016 |
| S7 | Flowable, CMMN chapter: https://github.com/flowable/flowable-engine/blob/main/docs/docusaurus/docs/cmmn/ch06-cmmn.md | `0faddc1`, 2022-02-09 |
| S8 | Siren, README: https://github.com/kevinswiber/siren | `4504e80`, 2017-08-11 |
