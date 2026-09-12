# Slice 010d1 concrete backend contract

Implementation contract under D-070 and [the approved specification](../specs/SLICE_010d1.md).
This adds retained capture only; native tables, Person readers, Today, core profiles
and existing command bodies are unchanged. Public schema qualification is frozen in
`docs/research/SLICE_010d1_SOURCE_SCHEMA_SHA256.json`; fixtures are constructed.

## HTTP and command identity

Namespace `/api/migrations/fub/history-captures`. Every route requires current
server-derived Org/admin and returns `Cache-Control: no-store`, including errors.
Bodies reject unknown fields and are at most 8 KiB. Mutation receipts contain
only `{capture_id: UUID, revision: decimal, state: closed-state}`; clients refetch
detail after any mutation, including replay. This bounds receipts independently
of capture size. Receipts bind Org, actor, request UUID, operation, run and exact
typed request. Reuse of a request UUID with another actor/body/operation conflicts.

| Request | Body / query | Response |
|---|---|---|
| POST collection | `request_id,parent_import_id,connection_id,expected_revision` (connection revision decimal string) | 201 compact receipt |
| GET collection | `parent_import_id?,cursor?,limit?` | `{captures:[run-summary],next_cursor}` |
| GET `/{id}` | none | run detail described below |
| POST `/{id}/confirm` | `request_id,expected_run_revision,acknowledgements:{api_visible_account_scope:true,coverage_gaps:true,retained_not_imported:true,source_user_evidence_revision:decimal,source_user_difference:boolean}` | 202 compact receipt; difference must equal displayed difference |
| POST `/{id}/retry` | `request_id,expected_run_revision` | 202 compact receipt |
| POST `/{id}/cancel` | `request_id,expected_run_revision` | 200 compact receipt |
| POST `/{id}/budget` | `request_id,expected_run_revision,expected_run_budget_revision,expected_org_budget_revision,expected_policy_revision,run_byte_limit,org_byte_limit` | 200 compact receipt |
| GET `/{id}/records` | `family?,disposition?,record_id?,cursor?,limit?` | `{records,next_cursor,capture_sequence,counter_basis:"current_run",counts}` |
| GET `/{id}/records/{record_id}` | none | one observation envelope |

All counts, bytes, source numeric IDs and revisions are decimal strings; UUIDs
are local resource strings; optional/unknown values are null. `limit` is an
integer 1–50, default 50. Family is `events|calls|text_messages`. Disposition is
`linked|parent_excluded|no_parent_identity|invalid_person_reference|conflicting_reference`.
Invalid source IDs remain separate pageable observations. A `record_id` filter
selects same-source-identity observations in the same run; invalid-ID selects self.

Run detail includes `id,parent_import_id,snapshot_id,parent_capture_sequence,
connection_id,connection_revision,source_account_id,source_user_id,
parent_source_user_id,source_user_difference,source_user_evidence_revision,
profile_version,schema_version,parser_version,initiated_by_user_id,state,revision,
pause_reason,created_at,proposal_expires_at,started_at,completed_at,
capture_sequence,raw_bytes,retained_bytes,reserved_bytes,run_byte_limit,
org_byte_limit,org_retained_bytes,org_reserved_bytes,run_budget_revision,
org_budget_revision,policy_revision,run_ceiling_bytes,org_ceiling_bytes,
required_reservation_bytes,release_ready,actions,streams`.
`actions` has booleans `confirm,retry,cancel,increase_budget`; source actions are
initiator-only. Each stream has state, checkpoint, frozen reported total,
advancing occurrences, valid/invalid occurrences, unique IDs, equal repeats,
conflicting variants and disjoint link counts. API-inaccessible count is null;
content flags and coverage reasons are explicit, never inferred zeroes.
`conflicting_variants` counts additional distinct semantic versions after the
first version of a source identity; an observation's `variant_count` includes
that first version. Equal repeated observations do not increase either count.

Observation envelope includes local `id,capture_id,capture_sequence,ordinal,
family,source_id,source_person_id,source_user_ids,source_created,source_updated,
source_kind,preview_truncated,relationship_uncertain,content_availability,
disposition,person_id,reference_available,variant_count,observation_count,
representation,profile_version,parser_version,schema_version,captured_at`.
Source strings are plain rendered text, bounded by the source parser (256 UTF-8 bytes per label; at most 6KiB serialized projection). No body,
subject, addresses, telephone numbers, HTML, message content or URL is returned.
The detail adds integrity status from successful AEAD open and retained reference;
it does not return capture bytes. Historical source dates are emitted only when syntactically qualified RFC3339 and remain labeled source values; unqualified dates stay raw with `source_timestamp_uncertain`. No native occurrence timestamp is invented. `content_availability` is only `returned_in_raw` (known non-null field present, without a completeness claim) or `not_returned`; `showContent`, placeholders and flags never establish source visibility.

Errors use existing JSON API envelopes: 401 anonymous,403 current nonadmin,
404 foreign/missing resource,400 malformed input,409 closed migration conflict/
storage/release/parent eligibility errors,503 unavailable key/service. Failure
reasons in run state are closed codes, never upstream text. Invalid/cross-scope
cursors are malformed. Current authorization is checked before receipt replay. Proposals expire for confirmation after 10 minutes, following 010b: they remain `proposed` with `actions.confirm=false`, can be cancelled, and require a new proposal to capture; confirm returns `migration_conflict`. No `expired` state is added.

## Persistence and crypto

One additive migration creates independent `migration_history_*` tables:
`capture_run`, `stream`, `capture`, `observation`, `identity`, `person_link`,
`seen`, `reservation`, `receipt`. Core jobs cannot claim these tables. Run/Org
composite FKs bind completed People import, its confirmed plan/original snapshot
and fixed capture sequence; writes revalidate current review binding and account.
Run state is `proposed|queued|running|waiting_retry|paused|completed_with_gaps|cancelled`.
Terminal evidence is retained. `confirmed_at IS NOT NULL` is the durable release
capability requirement, including cancelled and zero-collection runs.
Confirm and Resume also require this artifact's exact profile, parser and schema
manifest versions before changing durable state. Incompatible proposals remain
startup-exempt; they cannot create a confirmed binding through an older artifact.

Raw capture is immutable and labeled `advancing|diagnostic|identity`, with exact
response bytes, HTTP status, representation and truncation. Observation rows are
immutable capture+ordinal projections; IDs and semantic content are indexed with
tenant/run-purpose HMACs. Different variants remain separate. Person references
use the completed parent identity/result only, never contact matching. The
linkage inventory retains every parsed Person reference, bounded to 4096 per page.
Any derivation overflow retains only a diagnostic capture and pauses.

Run `schema_version` is `sha256:d20b212ecf0db3e6fbbf467b272501dd0796d9dc4af7e58fd50a075ae4a8841b`, the exact source-manifest file digest including newline. Current, older supporting and live identity bytes all pass history's strict duplicate-key/lossless bounded parser before the existing identity shape validator.

History AEAD binds Org, run, row and one closed purpose (`capture`,`projection`,
`checkpoint`,`receipt`,`cursor`). HMAC purpose additionally distinguishes source
identity, primary Person reference, semantic observation, page, token and request.
No canonical duplicate raw copy is stored. All plaintext source material is
transient; logs contain only safe IDs/closed codes/counters.

## Budget inventory and reservation proof

History run and the existing `migration_snapshot_storage` Org ledger count exact
octet lengths of every retained variable byte: ciphertext plus 24-byte nonce,
32-byte HMAC/index keys, request representation/profile/parser/schema strings,
source-version metadata, encrypted checkpoints, receipt ciphertext/digest/op and
Person-link keys. Fixed UUIDs, integer counters, booleans, closed enums and
timestamps are bounded operational metadata. No bodies are exempt from charging.

The 16-MiB request reservation safely contains the following maxima:

| Charged data per response | Upper bound |
|---|---:|
| Exact raw bytes plus AEAD | 4,194,344 |
| 100 projections, each ≤16KiB plus AEAD | 1,642,400 |
| Observation/source/semantic/primary keys, identity index and variant metadata, ≤512 bytes ×100 | 51,200 |
| Person linkage HMACs, ≤64 bytes ×4096 | 262,144 |
| Page/token digests and encrypted checkpoint, request/version strings | 16,384 |
| Total conservative retained bound | 6,166,472 <16,777,216 |

Actual settlement computes every retained field and checkpoint delta before
commit; `actual > reservation` fails closed. Replaced checkpoint bytes are removed
from usage exactly once. A reservation is consumed only by its token/run/Org
owner; expired/crashed owners are released under the Org lock before takeover.

At proposal reserve 8192 bytes for terminal cancellation. Compact receipt
plaintext is ≤4096 bytes; AEAD adds 40, digest32, operation≤32: ≤4200 bytes.
Proposal/confirm/resume/budget receipts admit exact bytes separately. Cancellation
consumes its dedicated allowance even when data budget is full, then releases
unused allowance. Natural completion releases the unused cancellation allowance.
Repeated receipt replay has no write/charge. Budget-increase receipt is admitted
against new approved ceilings atomically. No receipt silently disappears.

## Worker, enumeration and reads

The history worker uses the shared source permit and fixed history reader method;
identity is first on confirmation/restart/resume, pinned to acknowledged account
and source user. It releases the DB transaction before any request and rechecks
current admin, credential revision, parent binding and lease on commit. Fresh observed readiness is required at confirmation/explicit Resume and worker initial/recovery identity boundaries; ordinary admitted pages do not expire after five minutes. Each worker keeps one DB-clock startup boundary; the run stores the DB timestamp of its last successful identity verification. A missing/older verification or expired lease forces identity recovery, including a restart between queued pages. Compatible live workers reuse verification newer than their own startup boundaries, so alternating claims advance collections. Collection pages never renew this timestamp. Separately, each worker must admit its own initial source work with fresh observed readiness before reusing any shared verification; an idle clock initialization never grants that capability. This initial boundary is rechecked at settlement. API scheduling reloads observed evidence for these boundaries.
Three-attempt cycles and Retry-After use existing source transport behavior.
Every admission/disconnect path includes assessment,core and history symmetrically.

Parser rules are exactly spec §3. Terminal candidate pages are retained/advanced
once; distinct valid advancing IDs must equal the frozen total with no repeated
or invalid IDs. Failure pauses `enumeration_identity_uncertain` and saves a
terminal-pending marker. Resume revalidates retained counters without refetching
that terminal page or rewinding. Only three enumerated streams produce
`completed_with_gaps`. Diagnostic refetches never fill distinct-ID coverage.

Encrypted local cursors bind endpoint/Org/run/filter/record/page size and committed
capture sequence plus `(sequence,ordinal,UUID)` keyset. Later commits cannot enter
the series. Indexed keyset SQL fetches at most 51 observation rows before
decrypting at most 50 envelopes, each ≤8KiB and the response ≤512KiB.
Disposition predicates can scan additional indexed entries; this is not a
constant bound on physical scan work. Variant/reference annotations query only evidence
at/before that same boundary. Current Person existence is rechecked and no erased
name is returned. Capture readers never query native history or call the source.

## Compatibility and verification

`fub-history-capture-v1` is independent of metadata/activity/native-reader
capabilities. Fresh confirmation/resume and worker recovery require observed
current readiness. Preflight publishes `history_capture_schema_present`, `history_capture_binding_count`, `history_capture_unsupported_count`, `history_capture_confirmation_reasons` and `history_capture_confirmation_ready`; confirmed runs reject incompatible
recovery candidates. Existing proposed-only data does not grant readiness.

Focused source/DB/HTTP/preflight tests map to A1–A9/A11/A12; final gates and plan
collector are coordinator-owned. This contract is implementation scope, not a
test-pass or live-source qualification claim. All new raw/Person-link/receipt/
checkpoint/cursor stores extend the erasure inventory without closing readiness.
