# Mobile 007 — Find and save People

**IMPLEMENTATION ACCEPTED — D-086, 2026-09-15.** The user accepted both plans
and declared contracts for implementation and isolated synthetic verification.
Both independent planning reviews are READY. Compatible review
corrections are owned work; materially different policy and publication/deployment
remain separate. [Current implementation status](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md)
owns progress and evidence; planning-era wording below does not limit D-086.

[Brief](../tasks/MOBILE_007_IMPL.md), [paired plan](../plans/MOBILE_007_010d3_PLAN.md).

## 1. Outcome

An active member on iOS or Android searches their operational Organization while
online, identifies a Person, and requests an offline download. Once the existing
complete reconciliation is sealed and committed locally, that Person supports
all already-accepted Mobile001–006 offline workflows. Assignment does not restrict
search visibility. A search hit is not a downloaded or editable Person bundle.

Keep two visibly distinct scopes: **On this device** (existing local name search)
and **Search Organization** (online). An explicit Search action submits the online
term; cancel/replace previous requests and fence late results by search epoch.
Results show name, stage, responsible member and primary email/phone to distinguish
similar names. Selecting a result offers **Save offline**; an already downloaded
Person offers **Open** and its existing freshness information. Do not merge People
with identical names or contact methods.

Use existing search meaning: literal case-insensitive name substring, or exact
normalized email/phone. No fuzzy matching, new search service, advanced filters,
new People, reassignment, history download, communications, calling or redesign.
Organization search is unavailable offline; local search remains usable within
the current seven-day authorization window. A network failure is not “no matches.”

## 2. Inspected baseline and shared-contract declaration

At main `d8367a7`, both People screens filter downloaded display names and expose
manual Person-ID pinning. iOS `FieldModel.pin` and Android `FieldRepository.pin`
feed the existing `pinned_person_ids` reconciliation request. The backend selects
Today + assigned + pinned People and validates requested IDs. No online mobile
People-search route exists. `person::queries::search_summaries` supplies existing
Operator matching semantics but also computes inquiry aggregates unnecessary for
mobile discovery; do not call complete Person/detail or full-book readers.

| Current | Proposed change and reason | Affected components / compatibility |
|---|---|---|
| No mobile search route/capability | Add context-bound read-only `POST /api/mobile/v1/people/search`; advertise `people_search` | Rust mobile/query/API and both apps; old clients/routes/generations/receipts stay valid |
| Discovery requires a known UUID | Search result selection requests the existing local pin and sealed reconciliation | iOS/Android UI, state and protected pin persistence; no new business command or server pin table |
| Search summary query includes inquiry aggregates | Add a narrow typed Person discovery query sharing existing matching/normalization meaning | Person query layer and focused DB tests; existing Operator/Web responses unchanged |
| Native UI has no search/download state distinction | Explicit transient results, persisted requested pin, pending/downloaded/needs-attention states | Both apps and fixtures; search data never qualifies an editable baseline |

This spec owns the proposed extension to Mobile001 §§1/4 and the mobile API.
No realtime vocabulary, Operator tool, Person revision, receipt kind or server
schema change is required by the proposed design. Add a measured narrow index
only if the query plan demonstrates a need; declare it at the backend checkpoint.

## 3. Proposed request and result

Use POST so search text does not appear in URLs/access logs. Require normal session,
current active membership, operational workspace and `x-mobile-context` binding
on the actual transaction. Context is not a credential. Use PersonVisibilityScope,
never a client Organization or inferred assignee scope. Do not renew offline access
through search; normal explicit bootstrap remains the renewal mechanism.

Strict JSON body `{ "term": "..." }`, at most 4 KiB. Trim outer whitespace; require
1–200 Unicode scalar values and at most 800 UTF-8 bytes after trimming. Empty,
unknown fields and overlong input fail explicitly. Search wildcards are escaped;
email/phone normalization uses existing validators without a new matching policy.

Return `{context_id, items, has_more}`. Each item has Person ID, display name,
nullable stage/assigned-user summaries, nullable primary email/phone. Preserve
existing ordering `(last_name NULLS LAST, first_name NULLS LAST, id)` and primary
contact ordering. Limit to 25 rows plus one lookahead. `has_more=true` means
**More matches — refine your search**; no misleading total, exhaustive result
claim, automatic page walk or whole-Organization download. A fresh query observes
current data; results are not a long-lived consistency snapshot.

Cap each row at 4 KiB and response at 128 KiB. Oversized/corrupt projections fail
with an explicit safe error rather than silent clipping or dropped matches.
One request has bounded DB work at the D-050 book size; LIMIT alone is not evidence.
Freeze exact names/null/error fixtures at the contract checkpoint before clients.
Use existing mobile 400/401/403/404/409/422/503 precedence: malformed JSON, wrong
JSON shape, unknown body fields and query parameters are 400 `malformed_request`;
a structurally valid body with an invalid term is 422 `invalid_input`,
unsupported protocol/context is the existing mobile code, unavailable is 503.
Every response, including middleware rejection, is no-store. Do not log terms,
contact values, names, result bodies or raw SQL parameters. Content-free outcome,
latency, returned-count and truncation metrics are sufficient.

## 4. Download and recovery state

1. Online search results live only in transient account/context-bound memory.
   They do not enter the sealed cache, renew access, replace a summary or alter
   Today. Clear them on sign-out, Organization/context change, access expiry or
   authorization rejection; fence responses from old epochs.
2. Save offline atomically persists the Person pin in the existing encrypted
   actor/Organization store. Show **Requested for download** only after commit.
   Disk/key/transaction failures cannot report success. Repeated taps reuse the
   same pin; do not enqueue duplicate reconciliations or business operations.
3. Run the existing synchronization coordinator, with the latest accepted complete
   metadata-capable representation. A pin change during a generation schedules
   another generation; it must not pretend the old manifest includes the new pin.
   A queued request survives restart and continues when authorized connectivity
   returns. Existing concurrency, storage and selection bounds remain unchanged.
4. Keep **Downloading** or **Waiting for connection** until every required component,
   catalog and seal is accepted and the complete bundle is committed locally.
   Only then show **Available offline** and enable existing editors. Never promote
   an online search row or partial download. Preserve the earlier complete cache
   and pending work on any failed replacement.
5. A Person deleted or inaccessible after search may make the existing all-pins
   generation fail. Show a recoverable request failure, preserve other pins/work,
   and let the user cancel that requested pin before retrying. Do not silently
   clear all pins or repeatedly retry a permanently invalid pin without feedback.
   Server responses must not disclose whether a foreign Person exists.
6. Cancel request removes only explicit pin intent through protected local commit;
   it does not delete drafts/outbox/receipts or revoke Today/assigned selection.
   Explain if the Person remains selected for another reason. Physical cache
   eviction follows existing reconciliation/protected-work rules, not this button.
7. An already downloaded Person opens the existing sealed bundle and freshness
   labels. Successful upload overlays remain until causally covered as before.
   Search refresh never rolls them back or overwrites unsynced edits.

Unknown capability hides online search with an update-required explanation while
preserving supported local work. Search uses the trusted configured API origin and
existing credentials; never fetch a URL or execute text found in a result.
No local search-history retention, automatic backup or new body access is added.

## 4a. Reviewed native intent and selection contract (round 1 correction)

Persist a minimal encrypted pin-intent record per Person: UUID, desired boolean,
monotonic local intent revision, nullable generic failure class. Persist one
monotonic account-local pin-set revision and its last sealed/admitted revision.
No search term/result/name/contact body is stored in this model. A no-op repeat tap
does not advance the revision. Actual add/cancel commits intent and pin-set revision
together. Cancelled intent may be removed after a covering successful generation;
never remove associated drafts, operations or receipts.

Freeze a generation's exact desired UUID set and local pin-set revision before
requesting it; persist that revision with its staging checkpoint. After success or
recoverable failure, compare current versus staged/admitted revision and schedule
one coalesced follow-up for the newest set, honoring existing pause/auth/backoff
and attempt limits. A restart makes the same comparison. Do not busy-loop on a
permanently invalid unchanged pin set, mark a newer intent fulfilled by an older
seal, or lose a cancellation/addition while sync is active. Previously complete
bundles remain usable under their existing access/freshness rules independently
of whether a new pin intent is still pending.

Preserve the existing manifest's closed `today|assigned|pinned` reasons in encrypted
staging and promote them atomically with the successful seal. Display remaining
selection reasons after cancellation only from the latest sealed generation,
clearly labeled as last synced. Missing reasons in a pre-Mobile007 installed
bundle mean unknown until a fresh seal; never guess membership from a search hit.

For generic all-pins `not_found`, mark the requested set as needing review without
claiming which ID was deleted or foreign. List only this account's own requested
UUIDs (with a short ID label when no cached/transient authorized name exists),
allow cancellation of any selected intent, and retry the remaining set explicitly.
No new endpoint identifying a foreign/missing pin is added. Retryable network
errors remain waiting/retry states and do not imply absence.

Both native schemas upgrade additively from populated Mobile006 storage: seed
existing pin UUIDs as desired intents, initialize local revisions deterministically,
preserve all old cache/drafts/outbox/receipts/keys and classify legacy staging
pin-revision/reasons as unknown requiring a later refreshed selection. The old
pin storage may remain for compatibility but must not be an independently mutated
second authority. Exact schema versions/SQL are native-owned checkpoint detail.

## 5. Acceptance

| ID | Required evidence |
|---|---|
| M7-01 | iOS + Android find an unassigned/non-downloaded Person by name and exact email/phone, distinguish same-name results, request/save, restart offline, and use accepted editors after complete seal |
| M7-02 | Empty/overlong/Unicode/wildcard terms, 0/25/26 matches, null contacts/names, deterministic ties, oversized rows and exact matching behavior; no full-book fetch or inquiry aggregation |
| M7-03 | Anonymous, platform-only, revoked member, held-workspace admin, foreign/mismatched/expired context rejected; no-store even on outer errors; logs contain no search PII |
| M7-04 | Late query/account/context responses, offline search, dropped response and repeated tap never expose another scope or promote an incomplete bundle |
| M7-05 | Failed pin commit, interruption during components/seal/local promotion, pin added during sync and app restart preserve precise requested/downloaded states |
| M7-06 | Deleted/foreign pin failure can be resolved explicitly without discarding other work; cancelling a pin preserves drafts/receipts and other selection reasons |
| M7-07 | Populated Mobile006 store upgrades in place without uninstall/key reset; old protocol/capability fallback, pending edits/overlays and seven-day locks preserved |
| M7-08 | Actual simulator/emulator + isolated API UI journeys, accessibility/keyboard states; query EXPLAIN and paired existing Person/Today regression under D-050 |

Run implementation-required backend/native/final integrated gates from the paired
plan. Planning inspection is not passing test evidence. Physical-phone/cellular,
app distribution and customer-data readiness remain separate.
