# Restricting what a file field takes

**The question.** Can a flow say which media types a `file` attribute or input takes, so that a value of another type is refused and a form offers only what will be taken? Issue #44 (filed 2026-10-31) asks for it: a record keeps an attachment that is to be a photo or a PDF, never a video, and which media types it takes is a business rule of the record. Today the flow cannot state the rule, and no expression reads a file's media type or size (`eval.rs`, `member`, which answers no member of a file). This document records how form standards, description formats and a record platform restrict a file field, and what that suggests for ObjectFlow.

## 1. A form's list is a hint

**HTML, the `accept` attribute** (S1). "The accept attribute may be specified to provide user agents with a hint of what file types will be accepted." It takes three kinds of token: the strings `audio/*`, `video/*` and `image/*`; "A valid MIME type string with no parameters"; and a string starting with a full stop, which "Indicates that files with the specified file extension are accepted." The user agent narrows the choice: "User agents should prevent the user from selecting files that are not accepted by one (or more) of these tokens." The server is not to rely on it: "Authors are reminded that, as usual, data received from a client should be treated with caution, as it may not be in an expected format even if the user is not hostile and the user agent fully obeyed the accept attribute's requirements."

**Camunda Forms, the Filepicker** (S2, S3). Its property is "**Supported file formats**: Comma-separated list of supported file formats.", linked to HTML's file type specifiers. Its one validation is "**Required**: Filepicker must have a selected file." The form library renders the list as the native input's `accept` (`accept={evaluatedAccept || undefined}`). *Inferred:* the list is HTML's hint, passed through; no page read documents a check of it on the server.

## 2. Description formats state the list and enforce nothing

**OpenAPI 3.2.1, the Encoding Object's `contentType`** (S4). "The value is a comma-separated list, each element of which is either a specific media type (e.g. `image/png`) or a media type range (e.g. `image/*`)." The ranges are RFC 9110's. OpenAPI describes an interface; enforcing the description is each implementation's.

**JSON Schema 2020-12, `contentMediaType`** (S5). Its value "MUST be a media type, as defined by RFC 2046", one type and no list. The content keywords are annotations: "They do not function as validation assertions".

## 3. A record platform enforces it on the server

**Microsoft Dataverse** (S6 to S8). The list is the environment's, not a column's: "If `allowedmimetypes` is set, `blockedmimetypes` is ignored. Only the MIME types specified in `allowedmimetypes` are allowed." An upload of another type is refused, naming it: "Name: `MimeTypeNotInAllowedList`", "Message: `Operation not allowed as mime type image/svg+xml is not in the allow list.`" The size is each column's: "You can set the file size limit individually for each column of file type that you add to a table." The caller states the media type: committing an upload sends "The name of the file" and its "MIME type", and the sample derives it from the file's name, falling back to `application/octet-stream`. *Inferred:* the check compares the type the caller stated; no page read says the server inspects the content.

## 4. The stated type is the caller's claim

**OWASP, File Upload Cheat Sheet** (S9). "**Validate the file type, don't trust the Content-Type header as it can be spoofed**". The stated type still has a use: "The Content-Type for uploaded files is provided by the user, and as such cannot be trusted, as it is trivial to spoof. Although it should not be relied upon for security, it provides a quick check to prevent users from unintentionally uploading files with the incorrect type." It advises an allow-list, a check of the file's signature, and "**Set a file size limit**".

## 5. ObjectFlow beside them

- **The engine holds what the caller states, never the bytes.** A file value is a hash, a size and a media type, and ObjectFlow "never reads, streams, serves or deletes the bytes, which are the application's" (`DESIGN.md` §5.2). So a check in the engine is of the stated type, as Dataverse's is, by inference, and is OWASP's "quick check". The check of the content belongs where the bytes are: the application's blob store, which computes the hash.
- **What the flow states binds every writer.** A list in a consumer's own API binds only its API's callers: an import, an evaluator's service actor or another application writing the record is not held to it. A list in the declaration binds every request, effect, import and migration that writes the value, as a declared type does.
- **The format already narrows a type in its declaration.** `money(USD)` refuses an amount in another currency, `wrong_currency`, and `decimal(p,s)` refuses more digits than it holds (`errors.md`, input reasons).
- **An expression cannot state a range.** The language has no string operation beyond equality, membership and `length` (`DESIGN.md` §5.7), so a guard could list exact types but could not say `image/*`.
- **A file value carries no name** (ADR-0173 decision 11), so an extension, HTML's third token, has nothing to match.

## 6. What it suggests

These are recommendations for the author, not decisions.

1. **A declared list of the media types a file attribute or input takes**, each a type and subtype or a range of one type, `image/*`, as OpenAPI's and HTML's tokens are, and no extension, since a file value has no name.
2. **Enforced by the engine on every write**, where HTML, OpenAPI and JSON Schema enforce nothing: an input of another type refused as `wrong_currency` refuses money, and a write by an effect, an import or a migration refused as a value its attribute cannot hold.
3. **Answered by `declaration`**, so a form gives HTML's `accept` the same list.
4. **Said for what it is.** The engine checks the type the caller states; the blob store checks the bytes, as OWASP advises.
5. **A file's `media_type` and `size` read by expressions**, so a guard can bound a size or take a type in some states only. A declared size bound, which Dataverse sets per column, waits for a case that needs a form to read it.

## Sources

Each retrieved 2026-11-01 with `curl -sL` into its own directory, its markup stripped, and every quotation above found in the stripped text. A GitHub-hosted page's version is the latest commit to its file.

| | Source | Version |
|---|---|---|
| S1 | WHATWG, HTML Living Standard, the `accept` attribute: https://html.spec.whatwg.org/multipage/input.html#attr-input-accept | `whatwg/html` `source` at `404b3eb`, 2026-10-30 |
| S2 | Camunda 8 Docs, Filepicker: https://docs.camunda.io/docs/components/modeler/forms/form-element-library/forms-element-library-filepicker/ | `camunda/camunda-docs` at `9f00c6e`, 2026-08-26 |
| S3 | bpmn-io form-js, `FilePicker.js`: https://github.com/bpmn-io/form-js (`packages/form-js-viewer/src/render/components/form-fields/FilePicker.js`, branch `develop`) | `e7176a3`, 2026-10-28 |
| S4 | OpenAPI Specification 3.2.1, Encoding Object: https://spec.openapis.org/oas/v3.2.1.html | `OAI/OpenAPI-Specification` `versions/3.2.1.md` at `9f6c3fa`, 2026-09-10 |
| S5 | JSON Schema Validation, draft 2020-12, §8: https://json-schema.org/draft/2020-12/json-schema-validation | draft-bhutton-json-schema-validation-01 |
| S6 | Microsoft Learn, Files and images overview (Dataverse): https://learn.microsoft.com/en-us/power-apps/developer/data-platform/files-images-overview | `MicrosoftDocs/powerapps-docs` at `b9f6877`, 2026-01-10 |
| S7 | Microsoft Learn, Use file column data (Dataverse): https://learn.microsoft.com/en-us/power-apps/developer/data-platform/file-column-data | `MicrosoftDocs/powerapps-docs` at `52ecc39`, 2026-10-29 |
| S8 | Microsoft Learn, Column data types in Dataverse: https://learn.microsoft.com/en-us/power-apps/maker/data-platform/types-of-fields | `MicrosoftDocs/powerapps-docs` at `67c8475`, 2026-09-14 |
| S9 | OWASP Cheat Sheet Series, File Upload: https://cheatsheetseries.owasp.org/cheatsheets/File_Upload_Cheat_Sheet.html | `OWASP/CheatSheetSeries` at `89a49d9`, 2026-10-04 |

ServiceNow's documentation was not read: `docs.servicenow.com` answered a page that needs JavaScript, and `developer.servicenow.com` refused the client as an unsupported browser.
