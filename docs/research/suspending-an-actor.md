# Suspending an actor, so that it may act again

**The question.** When a person's account is switched off in a way that can be undone, how does a platform mark that the account may not act, apart from removing it? Issue #55 (filed 2026-11-01) asks for it. ObjectFlow refuses an actor only while its object is in a final state, `actor_active` (ADR-0182 decision 6). A state a transition can leave cannot be final, so an account deactivated until its person returns acts as an active one does. This document records how identity platforms mark such an account, and what that suggests for ObjectFlow.

## 1. Identity platforms keep a reversible status apart from removal

**SCIM 2.0, RFC 7643** (S1). The core User schema has an attribute for it, `active`, §4.1.1: "A Boolean value indicating the user's administrative status. The definitive meaning of this attribute is determined by the service provider. As a typical example, a value of true implies that the user is able to log in, while a value of false implies that the user's account has been suspended."

**Microsoft Entra ID, the Graph `user` resource** (S2). `accountEnabled`: "`true` if the account is enabled; otherwise, `false`."

**Okta, user account statuses** (S3, S4). Two of the statuses an admin sets refuse the user. "Accounts have a suspended status when an admin explicitly suspends them. The user can't access apps, the Admin Console, or the Okta End-User Dashboard. App assignments are unaffected and the user profile can be updated." And "Accounts have a deprovisioned status when an admin explicitly deactivates or deprovisions them. All app assignments are removed and the password is permanently deleted." A deactivation can be undone: "When you reactivate a previously deactivated user account, you might need to reassign some apps to users." A deletion cannot: "When you delete a user account, you can't undo the deletion."

**GitHub Enterprise Server, suspended users** (S5). A suspension is for people who leave, and keeps their work: "If employees leave the company, you can suspend their {% data variables.product.prodname_ghe_server %} accounts to open up user licenses in your {% data variables.product.prodname_enterprise %} license while preserving the issues, comments, repositories, gists, and other data they created. Suspended users cannot sign into your instance, nor can they push or pull code." It is undone the same way: "As when suspending a user, unsuspending a user takes effect immediately."

## 2. ObjectFlow beside them

- **Each platform marks explicitly that an account may not act.** SCIM and Entra hold it as a boolean of its own. Okta names it in a status, and has two an admin sets, each with its own consequences. *Inferred:* none of the pages read derives it from a broader classification of the account.
- **Each keeps it reversible, and apart from removal.** Okta reactivates a deactivated account and cannot undo a deletion. GitHub unsuspends, and keeps a suspended person's work. In ObjectFlow, a final state is the irreversible end: no transition leaves it (`flow-format.md` §4.5).
- **ObjectFlow's actor is an object with a lifecycle.** An actor type declares its states (`flow-format.md` §4.5), and the engine reads the actor's state already, refusing a final one (ADR-0182 decision 6). A deactivated account is a state of that lifecycle, as the issue's `DEACTIVATED` is. *Inferred:* the state is where a mark belongs, as Okta's statuses carry theirs, rather than an attribute that transitions would have to keep in step with the state.
- **The flow cannot say it today.** A condition may not read `actor` (`flow-format.md` §5, check 64, ADR-0114), so no guard can refuse a deactivated actor, and one on every transition would restate the rule.
- **`closed` already has a meaning.** Work is open until it enters a `closed` or final state, which the standard metrics read (`flow-format.md` §4.1). Reading `closed` as refusing the actor would give the category a second meaning. It would also refuse, with no change to their flows, the actors of every deployment whose actor types have a closed state that is not final. The shipped samples have none (`flow-format/examples`, every type with an `actor_kind` identity, read by a script).

## 3. What it suggests

These are recommendations for the author, not decisions.

1. **An actor type marks the states its actor does not act in**, beyond its final ones, as Okta lists the statuses that refuse a user. A flow that marks none behaves as today.
2. **A marked state refuses wherever a final state does**, so the store holds the rule for every writer, as SCIM's `active` and Entra's `accountEnabled` hold it for every application that signs in through them.
3. **A marked state stays leavable**, so reactivation leaves it, as Okta reactivates and GitHub unsuspends, and the person's history stays one actor's.

## Sources

Each retrieved 2026-11-02 with `curl -sL` into its own directory, its markup stripped, and every quotation above found in the stripped text. A GitHub-hosted page's version is the latest commit to its file. S5's quotations keep the page's template variables as its source has them.

| | Source | Version |
|---|---|---|
| S1 | IETF, RFC 7643, System for Cross-domain Identity Management: Core Schema: https://www.rfc-editor.org/rfc/rfc7643.txt | Standards Track, September 2015 |
| S2 | Microsoft Learn, user resource type (Microsoft Graph v1.0): https://raw.githubusercontent.com/microsoftgraph/microsoft-graph-docs-contrib/main/api-reference/v1.0/resources/user.md | `microsoftgraph/microsoft-graph-docs-contrib` at `5a88c6f`, ms.date 2025-01-10 |
| S3 | Okta Help, User account status: https://help.okta.com/en-us/content/topics/users-groups-profiles/usgp-end-user-states.htm | Classic Engine publication; the page gives no version |
| S4 | Okta Help, Deactivate and delete user accounts: https://help.okta.com/en-us/content/topics/users-groups-profiles/usgp-deactivate-user-account.htm | Classic Engine publication; the page gives no version |
| S5 | GitHub Docs, Suspending and unsuspending users (Enterprise Server): https://raw.githubusercontent.com/github/docs/main/content/admin/managing-accounts-and-repositories/managing-users-in-your-enterprise/suspending-and-unsuspending-users.md | `github/docs` at `1811c7b`, 2026-07-15 |
