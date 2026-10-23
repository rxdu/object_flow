# Atomic batches of requests

**The question.** Should a caller be able to send several requests, on several objects, that apply together or not at all? Issue #30 (filed 2026-10-23) asks for this as `request_batch`. In the issue's case, a person reviews a set of changes and confirms them as one act, and a confirmed set of three that lands as two is not what was confirmed. ADR-0037 rejected an atomic batch on 2026-09-08: "it would let a caller compose multi-object transactions the declaration never described, which defeats the printable rule set. Anything that must be atomic is declared as a cascade." This document records how established systems answer the question (2026-10-23), for the author's decision. It recommends; it does not decide.

## 1. Record platforms let the caller compose an atomic set

**Amazon DynamoDB, `TransactWriteItems`** (S1). The operation "groups up to 100 action requests", and "no two actions can target the same item". "The actions are completed atomically so that either all of them succeed, or all of them fail." A refusal cancels the whole request: "TransactionCanceledException The entire transaction request was canceled", among other reasons when "A condition in one of the condition expressions is not met." The batch is idempotent by a token: "Providing a ClientRequestToken makes the call to TransactWriteItems idempotent, meaning that multiple identical calls have the same effect as one single call." The token holds for a window: "A client request token is valid for 10 minutes after the first request that uses it is completed." A changed batch under the same token is refused: "If you submit a request with the same client token but a change in other parameters within the 10-minute idempotency window, DynamoDB returns an IdempotentParameterMismatch exception."

**Salesforce REST API, Composite** (S2, S3). The resource "Executes a series of REST API requests in a single call. You can use the output of one request as the input to a subsequent request." Its `allOrNone` flag decides what a failure does: "If the value is true, the entire composite request is rolled back." The batch is bounded: "You can have up to 25 subrequests in a single call."

**JSON:API, the Atomic Operations extension** (S4). "This extension guarantees that those operations will be processed in order and will either completely succeed or fail together." In its normative words: "A server MUST perform all operations atomically, so that a failure to perform any operation MUST invalidate any effects of preceding operations." The answer is positional: "The results array MUST be the same length as the requested operations array and each result MUST correspond positionally to its associated operation." A later operation may name a resource an earlier one creates, by "a local identity (lid) in a prior operation object".

## 2. Domain-driven design keeps a transaction to one aggregate

**Vaughn Vernon, *Effective Aggregate Design*, Part I** (S5). "And a properly designed bounded context modifies only one aggregate instance per transaction in all cases." He grants the rule's strictness: "Limiting the modification of one aggregate instance per transaction may sound overly strict. However, it is a rule of thumb and should be the goal in most cases." What spans aggregates is not made atomic: "Often, in such cases, the business goal can be achieved with eventual consistency between aggregates." His reason is concurrency. His aggregates "employ optimistic concurrency to protect persistent objects from simultaneous overlapping modifications by different clients", and "Smaller aggregates not only perform and scale better, they are also biased toward transactional success, meaning that conflicts preventing a commit are rare." One of his rules is headed "Model True Invariants In Consistency Boundaries".

## 3. ObjectFlow beside them

- **ADR-0037 takes the second position.** Atomicity across objects is declared, as a cascade (ADR-0019), so the rule set names every multi-object act. A cascade reaches further than Vernon's one aggregate, but its boundary is declared, as his is.
- **The issue's objections to a declared alternative hold, each checked 2026-10-23.** A transition's input is a value of a declared type, a reference, or a set of either, so a list of (target, transition, inputs) has no form to arrive in. The language has no string operation beyond equality, membership and `length` (`DESIGN.md` §5.7). A cascaded transition runs as the requesting actor (ADR-0019), so an upper layer, which authorizes each request it sends (ADR-0114), cannot authorize the elements of an effect one by one.
- **Vernon's reason does not carry over.** A store has one writer (ADR-0178, ADR-0213 decision 4), so its transactions never conflict with one another. *Inferred:* the cost of a larger transaction here is the time it holds the writer, not a failed commit.
- **The mechanism exists.** The writer already commits up to 64 waiting requests in one transaction, each on a savepoint and decided against the state the requests before it left (ADR-0208). A batch differs in one rule: a refusal rolls back every request, not its own alone.
- **What a batch holds the writer for, derived.** ADR-0208 measured one request's phases at p50 on the reference machine (SQLite, a two-year store, 2026-10-14): deciding 1.4 ms, writing the rows 1.7 ms, and the commit's flush 7.2 ms. *Derived, not measured as a batch:* n requests hold the writer about 3.1 × n ms plus one flush, so about 85 ms at 25, 200 ms at 64 and 320 ms at 100. A request queued behind such a batch waits that long, against C5's 100 ms at the 95th percentile (PRD C5).

## 4. What it suggests

These are recommendations for the author, not decisions.

1. **The split is by what the system is.** Record platforms let callers compose an atomic set, bounded at 25 or 100, with an idempotency token. Domain-driven design declares consistency boundaries and leaves the rest eventual. ObjectFlow's rules are of the second kind, but its upper layers call it as a record platform's clients do, and since ADR-0114 they hold the authorization each request carries.
2. **A batch adds no rule and bypasses none.** Each request still meets every declared guard and invariant, against the state the requests before it left. What a batch adds is failure atomicity over requests the caller has already decided. ADR-0037's objection holds in its own terms: the rule set no longer names every act applied together. But the rule set never named which requests a caller sends in sequence either.
3. **Recommended:** supersede ADR-0037's rejection with a bounded, idempotent batch, much as the issue proposes. Its bound is set by measuring a batch's hold on the writer against C5, not taken from another platform. It keeps out what group commit already runs alone (a change to the flow), and, at first, requests that would consult an external evaluator, since the writer cannot be left mid-batch. It answers a changed batch under a used key as `request` does, as DynamoDB refuses a mismatched token.
4. **Local names for created objects wait for a case.** JSON:API's `lid` and Salesforce's references let a later request name what an earlier one created. The case at hand needs none; the request's form can take one later without a change to the rest.

## Sources

Each retrieved 2026-10-23 with `curl -sL` into its own directory, its markup stripped, and every quotation above found in the stripped text. Spaces that stripped markup left before a comma are removed in the quotations.

| | Source | Version |
|---|---|---|
| S1 | Amazon DynamoDB API Reference, TransactWriteItems: https://docs.aws.amazon.com/amazondynamodb/latest/APIReference/API_TransactWriteItems.html | the page gives none |
| S2 | Salesforce REST API Developer Guide, Composite: https://developer.salesforce.com/docs/atlas.en-us.api_rest.meta/api_rest/resources_composite_composite.htm | the page gives none |
| S3 | Salesforce REST API Developer Guide, Composite Request Body: https://developer.salesforce.com/docs/atlas.en-us.api_rest.meta/api_rest/requests_composite.htm | `allOrNone` described as of API version 48.0 and later |
| S4 | JSON:API, Atomic Operations extension: https://jsonapi.org/ext/atomic/ | the site's v1.1 |
| S5 | Vaughn Vernon, *Effective Aggregate Design*, Part I: Modeling a Single Aggregate, 2011: https://www.dddcommunity.org/wp-content/uploads/files/pdf_articles/Vernon_2011_1.pdf | the PDF, its text read with `pdftotext` |
