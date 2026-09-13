# Slice 010e3 contract checkpoint

Frozen 2026-09-13 under D-078. This is additive: it admits qualified core-only
People from sealed retained evidence without reopening 010c or changing 010e2.

## HTTP

All routes are current-admin, organization-scoped, CSRF-protected and return
`Cache-Control: no-store` under `/api/migrations/fub/people-admissions`.
`POST /` accepts `{request_id,report_id}`. `GET /` requires
`parent_import_id` and supports opaque cursors and `limit<=20`; resource pages
are 128 KiB. Item and result pages use opaque endpoint-bound cursors and
`limit<=50`/256 KiB. Item contacts use the same bound. `POST /{id}/plans`
accepts `{request_id,expected_plan_revision}`. `POST /{id}/confirm` accepts
`request_id`, exact `plan_id`, decimal-string `plan_revision`, hex digest,
`eligible_count`, and affirmative acknowledgements for coverage, mappings,
distinct shared contacts and continuing review hold. `retry` and `cancel` take
request ID plus expected lifecycle revision. Exact request replays return the
saved receipt; changed bodies conflict. No eligible rows cannot confirm.

`GET /api/people/{id}/admission-provenance`, with bounded `/contacts` and
`/fields/{field}` pages, is an admin-only review read. The original
import-provenance route keeps its response and 404 behaviour.

## Persistence and authority

Migration `20260928000001_fub_people_admission.sql` defines
`migration_people_admission`, `_plan`, `_item`, `_contact`, `_result`,
`_receipt`, `_reservation`, plus `person_admission_provenance` and IDs-only
`person_admitted`. `migration_import_identity` remains the sole global key and
gets nullable `admission_id`/`admission_item_id`/`admission_result_id`; original manifest/mapping
columns remain populated only for original origins. Admission People identities
have `family='people'`, original import/plan parent references, null original
manifest/mapping, matching admission/item/result/target and no target cascade.

`crm.people_admission_permit` is a JSON `{lease,item}` private setting. The
private `crm_people_admission_mutation_allowed(uuid,text,text,text,jsonb)`
allows only the exact running, leased eligible item to insert its planned new
Person, owned contacts, initial stage/assignment facts and `person_admitted`.
It never grants UPDATE/DELETE. The workspace trigger calls it in addition to,
not in place of, existing import and refresh permits. Schema-owned person
revision trigger changes for the newly inserted Person remain permitted.

A run binds original completed parent/confirmed plan and workspace revision,
account, report/output revision, original and newer snapshots/sequences and
intervals, mapping choices and engine into encrypted plan inputs plus digest.
Plans seal immutably, expire at ten minutes and successors supersede only
unconfirmed previews. The confirmed admission boundary is admission-owned and
monotonic. One active run exists per parent.

## Qualification and result semantics

Each newer People group is accounted as `eligible`, `already_imported`,
`already_admitted`, `excluded_original`, `held_baseline_gap`,
`held_evidence_gap`, `held_mapping_gap`, `held_identity`, or `held_target`.
Qualification requires exact successful nontruncated retained observations,
positive decimal source IDs, duplicate-key-rejecting parse, semantic-HMAC
agreement, complete original/newer People streams and complete newer users and
stages. Absence is proven by a bounded qualified original identity index; report
labels, timestamps, display prefixes or contact matches cannot establish it.

Each settlement atomically writes a new Person, all owned contacts, initial
migration facts, encrypted provenance, immutable result and global identity.
Existing identities/tombstones win; an identity collision rolls native writes
back and is classified from the durable current row. The prospective UUID is
allocated in the item and an occupied UUID holds it. Equal contacts do not merge
People. Existing mapping choices are frozen and rechecked, and no new mapping,
stage or membership is created.

Only core Person/name/contact/stage/assignment and provenance/fact are covered.
No note/task/tag/custom-field/history/Inquiry or activation claim is implied.

## Review field and retained-byte units

Detail exposes `source_boundary.original` and `.newer` with snapshot UUID,
decimal-string sequence, started_at and completed_at; `coverage` names covered
and deferred families and the continuing review hold. Item detail exposes
`fields: Record<string,{prefix,total_bytes,truncated}>` and categorical
`held_reasons`. Field keys come from its sealed retained provenance plus normalized
first_name/last_name. Prefixes are at most 256 UTF-8 bytes; full-field pages are
at most 16,384 UTF-8 bytes. The 128 KiB limit bounds summary responses, not the
complete retained field content. Unknown field names return an opaque missing
resource response; prefixes are never write inputs.

The existing migration ledger measures retained payload bytes, not PostgreSQL
heap/index/WAL size. Admission charges the following persisted units exactly
once: encrypted plan inputs and its 32-byte rolling/final digest; item source key
and source ID plus both encrypted projection/provenance envelopes; encrypted
ordered contact envelopes; request action/digest/encrypted receipt; each copied
Person provenance envelope; result source ID and disposition; global identity
source ID and family; and the current UTF-8 preparation checkpoint. An encrypted
envelope includes nonce and ciphertext including authentication tag. The second
persisted provenance copy is a separate charged unit; shared raw source captures
retain their existing charge and are not charged again.

Fixed UUID, numeric, boolean and timestamp columns follow the existing ledger's
exclusion. IDs-only `person_admitted` introduces zero variable payload bytes;
native initial facts/Person/contact projections use the already charged retained
evidence, rather than a second native-row disk-size estimate. Identity's variable
overhead is its full source ID plus `people`. These logical units are frozen for
the independent `retained_byte_audit` test, which scans actual stored values;
production uses bounded incremental counters and admission-owned reservations.
