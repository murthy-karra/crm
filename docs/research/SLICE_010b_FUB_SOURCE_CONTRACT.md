# Slice 010b — Public source qualification for planning

Checked 2026-09-11 against official public pages and their embedded OpenAPI
`oasDefinition`. No FUB account, customer record or API credential was used.
This is documentation evidence for a draft; live behavior remains user-deferred.
The [specification](../specs/SLICE_010b.md) marks proposed profiles explicitly.

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
