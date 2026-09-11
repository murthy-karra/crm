# Slice 010b — Public source qualification for planning

Checked 2026-09-11 against official public pages and their embedded OpenAPI
`oasDefinition`. No FUB account, customer record or API credential was used.
This is public documentation evidence; live behavior remains user-deferred.
D-063 approves the [specification](../specs/SLICE_010b.md). Concrete defensive
parser/profile rules are application contracts, not vendor guarantees.

## Enumeration

FUB recommends opaque `next` pagination and limits pages to 100 records; deep
offset requests may require keyset pagination. Record which mechanism a family
actually supports and reconstruct a fixed-origin request rather than executing
`nextLink`. Missing or changing metadata cannot prove exhaustive capture.
[Pagination](https://docs.followupboss.com/reference/pagination).

Common filters apply to most, not necessarily every, endpoint. `updatedAfter`
does not make People a change feed for notes or other child records; parent
timestamps need not change when a related record changes.
[Common parameters](https://docs.followupboss.com/reference/common-filters).

## Core source contracts

| Source | Published evidence and planning implication |
|---|---|
| [People](https://docs.followupboss.com/reference/people-get) | `includeTrash` defaults false; `includeUnclaimed` is explicit. `fields=allFields` includes custom fields, but can produce large responses. Source documentation calls out relationships selection separately. Freeze and verify actual field coverage; preserve every returned field rather than dropping what the CRM cannot represent. |
| [Users](https://docs.followupboss.com/reference/users-get) | Public parameters include `includeDeleted`, fields, sort, limit and offset. Example users carry IDs, email, role/status and timezone. Preserve inactive/deleted identities for source attribution; never create CRM authentication or infer access solely from display names. |
| [Stages](https://docs.followupboss.com/reference/stages-get) | List supports limit/offset/sort; examples carry ID, name, orderWeight, isProtected and peopleCount. Counts and matching labels are evidence for preview, not authorization to create stages. |
| [Custom fields](https://docs.followupboss.com/reference/customfields-get) | `/customFields` returns collection `customfields`; retain source `name`, choices and recurring-date flag. Type-name similarity does not resolve target field limits or recurring-date semantics. |
| [Notes list](https://docs.followupboss.com/reference/notes-get) | The page's embedded OpenAPI exposes `GET /notes` with limit, offset and personId; examples use collection `notes` and include ID, personId, author names, body and isHtml. This page was retrievable directly even though the public llms index omitted its GET entry. Neither omission nor a search-tool error proves the endpoint is absent. |
| [Note detail](https://docs.followupboss.com/reference/notes-id-get) | `includeThreadedReplies` and `includeReactions` request additional content. Examples add author IDs, replies/reactions, and source-system/visibility metadata. A valid note can return 404 because of source/privacy restrictions; report unavailable content instead of deletion. Never infer author identity from names alone. |
| [Tasks](https://docs.followupboss.com/reference/tasks-get) | Explicit `isCompleted` supports partitioning open/completed work. Examples include `dueDate`, `dueDateTime`, completion and assignee fields; the example uses `AssignedTo` and numeric `isCompleted`. Preserve bytes and tolerate qualified source representations; do not reinterpret date-only deadlines as UTC instants. Common pagination still needs endpoint qualification. |

Several response examples contain ellipses/trailing commas and are not valid
JSON fixtures. Create minimal synthetic fixtures from the published shapes;
do not label repaired examples as captured live responses. Public schema
`info.version` is not a verified running server version.

### Interpretation for the approved specification

Notes list and enriched detail are separate representations of the same source
ID. Their expected additional fields are not evidence of a source edit; compare
content only under a qualified representation/field profile. The specification's
deterministic semantic HMAC is an application comparison rule, not a vendor
version identifier; original HTTP bytes remain preserved independently.

The documented restricted note-detail 404 supports an explicit item gap, not a
deletion claim. This evidence does not qualify arbitrary collection 403/404s as
complete enumeration or prove that credentials are still valid. The approved
specification pauses unqualified denials and retains successful list evidence. No new
live endpoint observation or permission was obtained during source qualification.

## Remaining coverage

Core-first was accepted as sequencing (D-061), not reduced migration fidelity.
Embedded tags/addresses/relationships and file references are preserved where
returned but do not prove their standalone collections were captured.
History, calls/texts, mail, media, appointments, deals and settings remain explicit
future snapshot work. Some calls/texts are not accessible through the public API;
complete enumeration of visible records cannot prove full historical coverage.
[Calls](https://docs.followupboss.com/reference/calls-get),
[texts](https://docs.followupboss.com/reference/textmessages-get).

## Local evidence and unresolved qualification

Public schema captures are retained at `/private/tmp/crm-010b-source/` and
identified by [the schema hash manifest](SLICE_010b_SOURCE_SCHEMA_SHA256.json). They are public
documentation captures, not a supported immutable vendor release. Refresh using
the source links when fixtures are finalized. No raw customer samples exist.

Before calling a connector live-validated: verify allFields/nested coverage,
deleted-user scope, task partition completeness, notes list/detail visibility,
per-endpoint continuation/exhaustion, source-user permissions and registered
identification. Do not close these questions using an index entry, example
payload or successful synthetic test alone.

## Implementation qualification refresh — 2026-09-11

All seven saved schema hashes matched the manifest. Current public HTML
confirmed People, users, tasks, stages and note detail; a repeat public notes-list
fetch returned 403, so its saved schema remains the evidence. No authenticated
API request was made and no approved scope was disproved.

- People, users, stages, custom fields and notes list explicitly document limit
  and offset. Tasks omits pagination query parameters but its response example
  contains offset/limit/total; global pagination guidance is supporting evidence,
  not live qualification of tasks. This gap remains explicit.
- Global documentation recommends `next` and shows People; the users example
  includes `next: null`. The endpoint schemas do not supply a complete per-endpoint
  terminal-page contract. Missing tokens and short pages alone cannot prove
  completion; require coherent collection/position/total evidence or pause.
- Custom fields' example represents total as a decimal string. Metadata parsing
  must tolerate qualified integer/string forms without floating-point loss.
- People `includeUnclaimed=true` covers leads offered to the current source user,
  not every unclaimed lead in the account. Users `fields=allFields` excludes
  `calling`; requesting that field requires `allFields,calling`. Freeze the
  actual selection and report omitted/unknown field coverage explicitly.
- Note detail must match the captured positive source ID. Its restricted 404
  supports only a committed item gap, never deletion or collection exhaustion.

Source links above and the global pagination/common-parameter references support
these distinctions. Lossless semantic JSON comparison and duplicate-key rejection
are application safeguards; source payload bytes remain independently preserved.
