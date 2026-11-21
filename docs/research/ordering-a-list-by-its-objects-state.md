# Ordering a list by its objects' state

**The question.** Issue #101 (filed 2026-11-20) asked that `query` take `state` as an order key, ordering objects by their state's place in their type's declared states. A list showing each object's state beside its other fields cannot sort by it, since the engine pages the list and a caller sorts only the page it holds. The author directed on 2026-11-21 that the open issues be reviewed and addressed. This note asks how established systems order records by a field that holds one of a declared, ordered set of values, a lifecycle's state above all.

## Findings per source

### PostgreSQL: an enum orders as its values were listed

PostgreSQL's enumerated types are "data types that comprise a static, ordered set of values". Their order is the declaration's: "The ordering of the values in an enum type is the order in which the values were listed when the type was created. All standard comparison operators and related aggregate functions are supported for enums." Its example then selects `current_mood > 'sad'` and answers `happy` and `ok`, which were listed after `sad`, though both sort before it by name.

### Jira: a field orders by its own sorting order

Jira's `ORDER BY` is "Used to specify the fields by whose values the search results will be sorted", and "By default, the field's own sorting order will be used." Its example sorts "by CreationDate, then by Priority (highest to lowest)" with `order by created, priority desc`: a priority orders by its rank, not its name. *Inferred*, which the page does not state of status: the fetched pages say nothing of how `status` sorts.

### Linear: a list orders by status as near to done first

Linear orders a list by status otherwise: "When ordering by status, list views show issues from closest to done → farthest from done, followed by completed and canceled issues." It points elsewhere for the declared order: "If you prefer to order issues by status in your team’s workflow order, use board views instead." A project list cannot reverse that order: "You can also reverse the sort order of your projects, except when sorting manually or by status."

## What the sources share, and where they part

- **PostgreSQL and Jira order a declared set by its declaration**, an enum by its listing and a priority by its rank, never by name.
- **Linear orders by how near each status is to done**, a grouping by kind with the closed ones last, and keeps the declared order for its boards.

## ObjectFlow beside the sources

- **The engine already orders a state by its declared place, wherever an order meets one.** A metric's rows order states and enumeration members in declaration order (`library-api.md` §6). A loop over a collection of members runs in declaration order (`flow-format.md` §4.8). `query` orders an enumeration attribute by its members' declared order, through the comparison `rank` (`core/crates/objectflow-core/src/reads.rs`), which orders a state by its place in its type's declared states too.
- **What `query` lacks is a key yielding the object's own state.** Its keys are `created_at`, `entered_at(<tracked member>)` and an attribute holding one value (ADR-0127 decision 2, as ADR-0194 decision 7 amended it). `order: "state"` is refused `unknown_order_key`, reproduced on 0.31.0 at `f1d6841` (`TODO.md`, *Issues filed on GitHub*).
- **A family's types each declare their own states**, since a base declares none (`flow-format.md` §4.14), unless they bind one state machine (§4.10). Two types' declared orders need not agree, so a place in one type's states says nothing of a place in another's.
- **A state's category is declared in the module's `categories`**, which Linear's grouping by kind resembles. Within one category, a type's states would tie.

## What it suggests

*A recommendation for the author, not a decision.*

Take `state` as an order key, ordering objects by their state's place in their type's declared states, as PostgreSQL orders an enum by its listing and the engine already orders a state in a metric's rows. The object id breaks ties, as for every key, and ` desc` reverses it.

- On a family, take it where every type with objects declares the same states in the same order, as types binding one machine do, and refuse it otherwise, saying why: one type's place beside another's would read as an order and be none.
- Leave a category's order to a later key, `state.category`, should a case need Linear's grouping.

## Sources

Each retrieved 2026-11-21 with `curl -sL`, its markup stripped, and each quotation searched in the text.

| Source | URL | Version |
|---|---|---|
| PostgreSQL documentation, 8.7 Enumerated Types | https://www.postgresql.org/docs/18/datatype-enum.html | 18 |
| Jira Cloud, JQL keywords | https://support.atlassian.com/jira-software-cloud/docs/jql-keywords/ | as served |
| Linear, Display options | https://linear.app/docs/display-options | as served |
