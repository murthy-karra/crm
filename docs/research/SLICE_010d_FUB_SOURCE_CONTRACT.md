# Slice 010d — Public FUB history source qualification

Checked **2026-09-12** against official public reference pages, their embedded
OpenAPI `oasDefinition`, and the official export help article. This is planning
research, not an approved implementation contract or live account qualification.
No authenticated FUB request, registration, webhook, export, message, media fetch
or customer-data operation was performed.

Read with [the decision log](../decisions/DECISION_LOG.md), especially
D-012/D-015/D-042/D-050/D-059–068, and
[010b source qualification](SLICE_010b_FUB_SOURCE_CONTRACT.md). D-061 leaves
history outside the delivered core snapshot. D-062 separately governs email
body storage; D-064's review hold remains in force.

## Planning conclusion

Public GET contracts exist for **lead events, calls and text messages**, including
individual-record reads. Each collection warns that some source data is
unavailable through the API. Exhausting accessible pages cannot establish complete
account history. Ordinary email retrieval was **not qualified** by this research;
marketing-event and campaign endpoints are different products.

The accepted planning direction splits **010d1 retained history capture and
coverage review** from **010d2 bounded timeline import**, specified later. The
first is the smallest useful deliverable supported by this research. Capture the
accessible event/call/text representations, source IDs and exact source time
fields without turning them into native Inquiry/contact-attempt/call facts.
Native projection needs separately reviewed time, actor, direction, outcome and
group-conversation policies. This research does not itself approve the resulting
specification, reduce eventual migration fidelity or authorize cutover.

## 1. Authentication, audience and system restrictions

API keys use Basic authentication and act with their user's access. OAuth uses
Bearer authentication. An owner, admin, agent and lender do not have identical
source visibility; account expiry can leave an API key valid while most requests
return 403. A successful identity probe therefore proves neither complete
history access nor continuing account availability.
[Authentication and authorization](https://docs.followupboss.com/reference/authentication).

The vendor requires system registration when providing services to a FUB customer
and asks for `X-System` and `X-System-Key` on API requests. Its definition of
`system` differs from a lead's marketing `source`. Registration does not itself
override record privacy or guarantee full-account communications access.
[Identification](https://docs.followupboss.com/reference/identification).

The inspected GET pages do **not** define an exhaustive owner/admin/system-origin
access matrix. Do not invent a rule that only our system's records are readable,
or its opposite. In particular, the registered-system restriction on creating
text-message logs does not establish the GET visibility rule. The POST endpoint
records a log rather than sending a text; it was inspected only for semantics.
[Text log creation documentation](https://docs.followupboss.com/reference/textmessages-post).

## 2. Lead events and historical inquiries

`GET /v1/events` explicitly accepts `limit` (default 10, maximum 100), `next`,
`offset`, `personId`, comma-separated `type`, `hasProperty` and `propertyAddress`.
The collection key is `events`; its example metadata includes
`collection/offset/limit/total/next/nextLink`. Types span registrations, inquiries,
property/web activity, incoming calls and unsubscribe activity. Some events remain
available only in the FUB interface.
[Event collection](https://docs.followupboss.com/reference/events-get).

The collection example/schema supplies `id`, `created`, `updated`, `personId`,
`message`, `description`, `noteId`, `source`, `type`, `property`, `propertySearch`,
`pageTitle`, `pageUrl` and `pageDuration`. `GET /v1/events/{id}` is documented;
its example omits the page fields. Property/search objects contain potentially
sensitive addresses, URLs, numeric/string values and search criteria. Preserve
both representations rather than treating a smaller detail example as proof of
complete replacement.
[Event collection schema](https://docs.followupboss.com/reference/events-get),
[event detail schema](https://docs.followupboss.com/reference/events-id-get).

Neither inspected GET schema supplies `occurredAt`, `userId`, `createdById` or
`system`. The POST contract separately accepts historical `occurredAt`, and
explains that its response describes the associated Person. It also describes
`Inquiry` as shorthand for property/general inquiry. Those write-side facts do
not establish how historical occurrence time is returned by GET.
[Event creation semantics](https://docs.followupboss.com/reference/events-post).

Consequences for planning:

- Preserve source `created` and `updated` under their own labels. Do not assert
  that either equals the original inquiry's occurrence time.
- Never manufacture an Inquiry from `Person.created`, reconstruct a missing
  event from Person-level lead-source fields, or replace original attribution
  with a later event's `source`.
- Preserve `noteId` as a cross-reference. It does not authorize merging a source
  event with the note already delivered by 010f2.
- `Incoming Call` and `Unsubscribed` are source event classifications; they do
  not establish telephony completion/outcome or communication consent policy.

## 3. Calls

`GET /v1/calls` supports `limit` (10 by default, maximum 100), `offset`, `personId`,
`phone`, `toNumber` and `fromNumber`. The example has a `calls` collection and
offset/limit/total metadata. A `personId` of zero represents an unknown caller,
so enumerating only imported Person IDs would omit some account-level records.
Some call data is inaccessible through the API.
[Call collection](https://docs.followupboss.com/reference/calls-get).

`GET /v1/calls/{id}` exposes example fields `id`, `created`, `updated`,
`createdById`, `updatedById`, `phone`, `personId`, `userId`, `userName`, `note`,
`outcome`, `isIncoming`, `duration`, `ringDuration` and `recordingUrl`. There is no
separate call-start/end/occurrence timestamp in this GET schema. Recording
content is replaced by privacy text in the example. The collection example also
misspells its recording key as `recordingUrl:` and contains an ellipsis: it is
not a runnable JSON fixture.
[Call detail](https://docs.followupboss.com/reference/calls-id-get),
[call collection example](https://docs.followupboss.com/reference/calls-get).

POST documentation defines `userId` as the user making/receiving the call and
allows administrators to set it; creation/editor IDs remain distinct source
provenance. Its outcome vocabulary is `Interested`, `Not Interested`,
`Left Message`, `No Answer`, `Busy`, `Bad Number`; duration is in seconds.
This is not a guaranteed complete GET vocabulary or an accepted mapping to our
native outcomes. Do not derive call time by subtracting duration from `created`,
use an editor as the caller, or infer consent from a recording reference.
[Call write-field semantics](https://docs.followupboss.com/reference/calls-post).

## 4. Text messages

`GET /v1/textMessages` explicitly lists `personId`, `toNumber` and `fromNumber`
filters. None is marked required in the schema, but the description emphasizes
Person/phone retrieval; account-wide unfiltered behavior still needs
qualification. Pagination parameters are absent from this endpoint's parameter
list, although its `textmessages` response contains offset/limit/total and
`next/nextLink` metadata. Some message data is API-inaccessible.
[Text collection](https://docs.followupboss.com/reference/textmessages-get).

The list schema includes IDs for record/Person/creator/editor/user, `created`,
`updated`, `sent`, `status`, `message`, `isIncoming`, phone endpoints, display
names, `read`, `archived`, shared-inbox/action-plan/group references, external
flags/links/labels, `media`, `participants` and `systemName`. Group-only
participants can carry Person/relationship IDs, phones and visibility/reply
flags. Preserve that structure; a group message is not safely reduced to one
Person's outbound attempt.
[Text collection schema](https://docs.followupboss.com/reference/textmessages-get).

`GET /v1/textMessages/{id}` exists. Its schema adds `systemId`, omits the list's
participants field, and supplies only an empty-object media item schema. List
and detail therefore need separate capture profiles. `Sent` and `Received` are
examples, not a closed delivery-status enum; booleans have inconsistent example
and default values. No received/delivered timestamp semantics are established.
[Text detail](https://docs.followupboss.com/reference/textmessages-id-get).

Both examples replace message content with privacy text. This does not establish
a reliable runtime redaction sentinel, body completeness, attachment availability
or permission to follow returned URLs. Preserve returned bytes and explicitly
separate captured metadata/body text from uncollected media. `sent`, `created`,
`updated`, creator/editor and attributed user stay separate until a reviewed
mapping establishes their meaning. Do not infer delivery, outreach credit or
consent merely from existence of a source log.

## 5. Ordinary email, marketing events and exports

Ordinary `/emails` collection/detail GET contracts were **not found** in the
retrieved official reference sidebar or endpoint schemas. Public documentation
pages `/reference/emails-get` and `/reference/emails-id-get` returned 404.
This establishes an unqualified documentation surface, not an authenticated API
404 or proof that no private/partner API exists. No request was sent to
`api.followupboss.com` to test an undocumented route.

The official webhook guide lists email create/update/delete notifications, but
that does not define a backfill enumeration or email-response schema. Webhooks
require owner privileges; subscriptions were not created. Deleting a Person
deletes related records without individual child-delete events, so even a future
webhook implementation cannot ignore Person deletion.
[Webhook guide](https://docs.followupboss.com/reference/webhooks-guide).

`GET /v1/emEvents` is **marketing activity**, not mailbox correspondence. Its
explicit filters are `type`, `personId`, `updatedAfter`, `limit` (10/100) and
`offset`. The example collection is `emEvents`; rows contain
`count/type/personId/campaignId/campaignName/created/updated`, with no event ID.
The aggregated `count` prevents treating each row as a uniquely identified
individual send/open. Occurrence time, stable identity and aggregation semantics
need further qualification before immutable native projection.
[Marketing events](https://docs.followupboss.com/reference/emevents-get).

`GET /v1/emCampaigns` filters `origin` and `originId`; examples contain
`id/origin/originId/name/subject/bodyHtml`. The documented endpoint includes full
HTML by default and can fail with large responses; the vendor recommends starting
with a limit of ten. Campaign content is not a captured per-recipient RFC message
or proof of all connected-mailbox history. Collecting this content requires the
separately specified D-062 bulk-storage path; do not silently strip it or add it
to the PostgreSQL history capture.
[Marketing campaigns](https://docs.followupboss.com/reference/emcampaigns-get).

The standard contact CSV export includes at most the **latest 50 calls, 50 texts
and 50 notes per contact**. FUB's help article says email and call recordings
cannot be exported, and points to connected email accounts for mail access.
That export is therefore a possible supplemental source, never automatic proof
of full history. Mailbox access would need its own authorization, source
contract, identity/deduplication and D-062 storage work.
[Export limitations](https://help.followupboss.com/hc/en-us/articles/360015269133-Export-Contacts-to-a-Spreadsheet).

## 6. Pagination, deltas and completeness

General guidance recommends opaque `next` over offsets, with default limit ten
and maximum 100; some deep pagination requires keyset continuation. It supplies
no stable point-in-time snapshot guarantee. Qualify each family separately,
including its final-page signal, changing totals, deletion/visibility changes and
duplicate/reordered pages. Reconstruct requests at the approved API origin; do
not execute `nextLink`. The events example itself contains an older
`api.reclients.com` link.
[Pagination](https://docs.followupboss.com/reference/pagination).

Common `createdAfter/Before`, `updatedAfter/Before`, `ids`, ID bounds and sorting
apply to most endpoints, not a guaranteed universal contract. Events' example
URL includes `updatedAfter`, although its explicit query list does not. Calls
and texts have no explicit per-endpoint delta parameters in the inspected
schemas. Parent `updated` does not necessarily change when a related record
changes. Do not use People timestamps as a communications change feed or claim
deletion capture from updated-only polling.
[Common filters](https://docs.followupboss.com/reference/common-filters).

Rate limits use a ten-second sliding window and response headers. The published
GET-events context is 20 requests per window with a valid system key, ten without;
the global context is separately limited. Honor actual headers and `Retry-After`,
including a 429 received while reported quota remains. These public values are
not an accepted multi-process pacing implementation.
[Rate limiting](https://docs.followupboss.com/reference/rate-limiting).

Keep separate report categories for accessible records captured, known
list/detail gaps, privacy-restricted content, uncollected media, unavailable email
retrieval, unsupported native mappings and unqualified account-wide coverage.
Zero returned rows, a completed pagination loop or an admin credential cannot
erase the vendor's restricted-results caveat.

### 6.1 Candidate request allowlist for 010d1

The following is a **proposed application profile**, with its public evidence
limits stated explicitly. The slice specification owns the final exact parser
and continuation rules. Use the existing approved API origin
`https://api.followupboss.com/v1`; freeze this profile, account authority, schema
digest and capture epoch before execution. None of these requests was executed
against an account during this research.

| Family | Proposed initial request | Proposed continuation and its evidence | Documented detail, excluded from 010d1 requests |
|---|---|---|---|
| Events | `GET /events?limit=100&offset=0` | `limit=100&next=<opaque token>` is explicit in the endpoint contract. Offset is also explicit, but changing pagination mode mid-capture needs a specified rule. | `GET /events/{id}`; numeric path ID, no added query parameters. |
| Calls | `GET /calls?limit=100&offset=0` | Offset-only with `limit=100` is explicit. `next` is supported by general guidance but absent from this endpoint's parameter list/example; a nonempty token or deep-offset rejection pauses this bounded profile. | `GET /calls/{id}`; numeric path ID, no added query parameters. |
| Text messages | `GET /textMessages?limit=100&offset=0` | This limit/offset profile relies on general pagination guidance; the endpoint-specific query list omits both. Conditional adoption of a valid nonempty `next` with coherent metadata is defensible as an explicitly synthetic profile. Unfiltered enumeration and continuation remain live-unqualified. | `GET /textMessages/{id}`; numeric path ID, no added query parameters. |

Evidence: [events](https://docs.followupboss.com/reference/events-get),
[calls](https://docs.followupboss.com/reference/calls-get),
[texts](https://docs.followupboss.com/reference/textmessages-get),
[general pagination](https://docs.followupboss.com/reference/pagination).

For this candidate profile, allow no `fields`, `sort`, time/ID bounds, Person,
type or phone filter. Those would create a separately named capture scope;
Person-filtered enumeration especially cannot establish account-wide coverage.
Retain every returned field, including unknown fields, in the encrypted raw
representation. Absence of `fields` asks for the vendor's default representation,
not a guarantee of all available fields or unredacted content. **010d1 captures
collections only**; documented detail variants remain explicit coverage gaps.
If a later approved slice collects detail bytes, keep them separate rather than
replacing a list representation that has extra fields.
Media/external/page/recording URLs remain inert source values.

The documented record identity is an integer `id`, with integer detail path
parameters. Use account + family + exact source ID as the identity candidate;
preserve full integer precision. The docs do not establish lifetime uniqueness,
ID reuse policy or cross-account uniqueness. A positive-ID validation rule is an
application contract to test, not proof from the generated schema's zero
defaults. Call `personId=0` is a documented association case, distinct from a
record ID. Retain source Person/user/system IDs without treating them as trusted
local identities, authenticated actors or a proven stable binding across epochs.

### 6.2 Continuation, terminal evidence and qualification gate

Public documentation is sufficient to design the bounded adapter and synthetic
fixtures. It does **not** qualify a fully exact live capture/exhaustion contract
for all three families. Before implementation is declared ready, the slice must
freeze and test the following defensive application rules; do not describe those
tests as validation of actual vendor behavior:

1. Accept only the named collection (`events`, `calls`, `textmessages`) and a
   qualified metadata shape. Validate integer offset/limit/total where required,
   page bounds and record IDs without coercing schema defaults into missing data.
   Persist the complete response and its request profile before advancing.
2. Treat `next` as opaque data, percent-encode it into a reconstructed request at
   the fixed origin, and preserve the frozen query scope. Never follow
   `nextLink`, an HTTP redirect or a record URL to expand the allowlist. Repeated
   tokens/pages, conflicting duplicate IDs and lack of progress must pause with
   evidence rather than loop, discard bytes or declare completion.
3. Under an explicitly accepted offset profile, advance by the qualified page
   width and verify the returned offset; require full nonterminal pages. The
   general guide illustrates offsets increasing by the requested limit. A short
   page, changed total or server demand for keyset pagination needs a specified
   consistency rule; switching mode or assuming a short page is final is unsafe.
4. Define terminal evidence separately for each mode. A candidate offset rule
   checks coherent final offset + returned row count against the qualified total;
   a candidate keyset rule also requires an explicit terminal token state and
   reconciles the unique captured IDs against the qualified count. Public docs
   do not define every terminal/null/absent/count combination. Unrecognized
   combinations, missing required metadata and early empty pages fail closed.
   Even a passing rule means only that this accessible capture scope exhausted
   under that profile, not a source snapshot or full-account completeness.
5. Do not expand 010d1 to detail requests to compensate for a list field gap.
   Record omitted detail variants, inaccessible/UI-only history and unknown body
   availability separately from successful collection enumeration. HTTP 403/404
   cannot establish deletion or collection exhaustion. Exhausted request, page,
   byte or time budgets leave resumable incomplete work.

At minimum, implementation fixtures must cover ordinary and empty terminal
pages, exact-multiple page counts, keyset/offset variants, duplicates/reordering,
changing totals, invalid/unknown metadata, redirects, 429/retry, collection access
failure, unknown Person bindings and preserved list-field differences. Fixture qualification
establishes the application's bounded behavior. Explicitly authorized source
qualification must later establish which live variants are accepted; until then,
report that qualification as deferred and fail closed on unsupported responses.
No live probe, registration or customer capture is authorized by this document.

## 7. Proposed execution boundaries and unresolved decisions

For a first capture rung, freeze account/credential authority, request profile,
record IDs and complete raw representations; budget and encrypt the retained
evidence; expose bounded administrator review. Preserve current People import
bindings and sibling results. A capture taken after the core snapshot is a
separate capture epoch, not evidence that both represent the same source instant.
Source records whose Person mapping is missing or ambiguous remain reported.

Before native historical facts, specify and review:

1. Which event types become Inquiries, and a qualified occurrence-time rule that
   preserves unknown time instead of defaulting from Person creation.
2. Call direction/outcome/time mappings, historical actor handling, corrections
   and whether the native model can represent source facts without inventing a
   LiveKit call or changing Today.
3. Text direction/status/time and group-participant semantics; explicit source
   privacy and eventual audience rules before release of the review hold.
4. Account-qualified identity across snapshots, per-family deltas/deletions,
   local changes, replay and reconciliation; no current child-lifetime rule
   implicitly authorizes repairs or replacement imports.
5. Email source access, D-062 storage/recovery, and recording/media
   authorization and consent. These are separate work, not URL-following details.

Synthetic fixtures can exercise the proposed defensive parser and worker
contracts. Before declaring real-source qualification, verify endpoint access,
pagination/exhaustion, body visibility, source-user/system scope, time/status
semantics and known deleted/unknown-Person cases against an explicitly authorized
dataset. Live validation remains user-deferred; D-015/O-012/O-013 customer-data
prerequisites remain open. A restricted item 403/404 is not deletion or successful
collection exhaustion without an accepted qualification rule.

## 8. Reproducible public evidence

Public HTML and extracted documentation/schema JSON are retained locally at
`/private/tmp/crm-010d-public-source-a6t4eqop/`; they contain vendor examples, not
customer captures. `capture-index.json` records requested documentation URLs,
HTTP results and extracted operations. The schema serialization is Python
`json.dumps(oasDefinition, indent=2) + "\n"`, UTF-8, default ASCII escaping.
All eleven schemas declare OpenAPI 3.1.0 and documentation `info.version=1.0`;
neither identifies a verified running API build.

| Public reference slug / saved `-oas.json` | Bytes | SHA-256 |
|---|---:|---|
| `events-get` | 16,377 | `9fc1cb73f55c0c5c43dcbfa6374fec69d91f7b7283594f8d2a40106182ec80c7` |
| `events-id-get` | 10,771 | `500c120de1591238e416145491cc4790802d08af07b07f3071b8764c0be8c4e2` |
| `events-post` | 22,510 | `cb98aae734ab5df4de0c3d16d050c2e52a252701003145d0022047e8016b3e99` |
| `calls-get` | 3,830 | `f117894a97f12c23cf3fda1e4227ff5441ddd0b5dfe14436c66716195a12017c` |
| `calls-id-get` | 4,800 | `9cc64a61b1bdfce66915c75fd551d7118c4b4489ad9bfc4be35ccd08eb4c20fb` |
| `calls-post` | 4,381 | `0b4131b0e73da183901c1fc4950bec218ba9e605e4ede97b4b66feaa09b84a01` |
| `textmessages-get` | 12,749 | `3f0c8562c98b3b649a27c714dabf83d41f24b9ba76e3b4bc02b280850274502b` |
| `textmessages-id-get` | 6,785 | `af526bbc0dccca7119e4029d17baa6bc1c86c195aa49e2156ac4d88151b8c7b5` |
| `textmessages-post` | 8,930 | `5e70b54e648e571e642856b4c6d1fd5033696f98ac79c1105b6b7eafec8d7e01` |
| `emevents-get` | 3,039 | `b8ba143ca4eb46f1bb029d519892ebef5386323e7e07db2cfb78b75fee1e1daf` |
| `emcampaigns-get` | 2,415 | `d8cb75070cae79e9661b4fd7d9f99790f9561fd1056f58b947cf6ca1c6bdba05` |

The initial default-client `llms.txt` request returned 403 and browsing-tool
Markdown opens failed. Browser-user-agent public HTML requests supplied the
successful evidence above. Later documentation requests returned 429; fetching
stopped. These are documentation-host observations, not API behavior. The
ordinary-email 404 observations and identification page were retrieved in a
subsequent read batch and are not entries in the initial capture index.

Several examples are incomplete JSON, privacy substitutions or mechanically
generated schemas with weak/null types and contradictory defaults. Preserve
these source limitations; do not silently repair examples and label the result
as a real response. Refresh official sources when fixtures/contracts are frozen.
Public evidence is useful for bounded planning, not a substitute for live
qualification or approval of the resulting specification.
