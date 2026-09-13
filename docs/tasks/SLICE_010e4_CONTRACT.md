# Slice 010e4 contract checkpoint

Frozen 2026-09-13 under D-080. This adds a separately scoped, retained-evidence
refresh for People successfully admitted by one terminal 010e3 admission. It
does not change 010c import, 010e2 original-People refresh, or 010e3 admission
contracts.

## Cohort, source and baseline

`POST /api/migrations/fub/admitted-people-refreshes` accepts only
`{request_id,admission_id,report_id}`. The server resolves all other bindings.
`admission_id` must name the same-Organization terminal (`completed` or
`cancelled`) admission with at least one successful settled result. The cohort
is exactly those settled results from that one admission, never a query over
admitted People and never a combination of admissions.

The report must be completed/sealed, have the same original import/confirmed
plan/source account/workspace revision, and have a newer capture that begins
strictly after both the cohort admission source capture and the latest confirmed
admitted-refresh capture. A same-boundary remainder is allowed only after a
cancelled/completed attempt and only against that exact already-confirmed
report/snapshot/sequence/interpretation. The cohort has its own monotonic
watermark; 010e2 and 010e3 watermarks are untouched.

Initial baseline B is reconstructed only from the successful
`migration_people_admission_result`, its exact `migration_people_admission_item`
encrypted native projection, exact `migration_people_admission_contact` UUIDs,
immutable global identity and qualifying mappings. It is never reconstructed
from live `person` or `contact_method`. Later B comes only from an immutable
settled result/baseline pointer owned by this feature. The worker requires exact
admission/result/identity/target/baseline proof before reading or changing a
Person. Current C is live native state and N is the complete, qualified newer
instruction over B using the 010e2 missing-versus-explicit-clear rules.

Only `C=B && N!=B` is eligible. `C=B && N=B` is `already_current`. Any
divergence, including `C=N`, contact replacement/unowned contact, absent target,
identity/mapping disagreement, erased/tombstoned target or evidence gap holds
the entire Person. A source Person missing from newer evidence is
`not_seen_again`, never deletion. Original/new report disposition labels do not
choose a candidate.

## HTTP

All routes are current-admin, active-Organization scoped, CSRF-protected for
mutations, and return `Cache-Control: no-store`. Foreign resources are
non-disclosing and unknown request keys are rejected. Resource pages are at
most 128 KiB; item/contact/result pages are at most 256 KiB. All cursors are
authenticated and bind actor, Organization, endpoint, resource, plan identity,
revision/digest where applicable, filter, limit and a frozen upper key.

| Route | Request / bound |
| --- | --- |
| `GET /availability` | Required `admission_id` and `report_id`; returns the server-resolved admission/report/parent/plan/account/workspace bindings, `available`, and a closed reason code; foreign or missing bindings are opaque `404` |
| `POST /` | `{request_id,admission_id,report_id}`; returns `201` preparation receipt |
| `GET /` | required `admission_id`, opaque cursor, `limit<=20` |
| `GET /{id}` | lifecycle, exact admission origin, source intervals, availability/counts/actions |
| `GET /{id}/items`, `/results` | opaque cursor, `limit<=50`, frozen traversal upper key |
| `GET /{id}/items/{item}` | bounded metadata, separate clear/removal counts and truncated field metadata |
| `GET /{id}/items/{item}/contacts` | opaque cursor, `limit<=50`, frozen owned contact side/identity/order/change |
| `GET /{id}/items/{item}/fields/{side}/{field}` | side `baseline/current/proposed`; only `first_name`/`last_name`; UTF-8 fragment limit 4..16384 |
| `POST /{id}/plans` | `{request_id,expected_plan_revision}` |
| `POST /{id}/confirm` | exact plan id/revision/digest, `acknowledged_eligible_count` (integer, exact plan count) and coverage/exclusion/name-clear/assignment-clear/contact-removal acknowledgements |
| `POST /{id}/retry`, `/cancel` | `{request_id,expected_lifecycle_revision}` |

`plan_revision`, result counts and byte offsets serialize as decimal strings.
Field prefixes are display-only and confirmation identifies the full immutable
plan. Confirm requires at least one eligible business update; all-held or
all-current previews remain readable but cannot queue work. Exact request replay
is bound to actor/Organization/action/resource/normalized body; a changed body
conflicts. Only the initiator can confirm/retry; each current administrator can
read/cancel.

## Persistence and private authority

Migration `20260930000001_fub_admitted_people_refresh.sql` adds only these
Organization-scoped tables:

`migration_admitted_people_refresh`, `_plan`, `_item`, `_contact`, `_result`,
`_baseline`, `_receipt`, `_reservation`, and the append-only encrypted
`person_admitted_refresh_provenance`.

The resource stores immutable admission/result source FKs, original parent/plan,
account, report/snapshot/sequence/intervals, workspace revision and its own
lifecycle/lease/watermark. Each item stores immutable exact admission item/result,
global identity and target references, encrypted baseline/current/proposed/
instructions envelopes, source observation keys and a disposition. Contacts
record side (`baseline`, `current`, `proposed`) and the exact owned committed
contact UUID/order. Result/provenance snapshots are append-only. A baseline
stores the result pointer/version and encrypted native projection; transferring
that ownership precedes replacing the pointer.

The item-detail projections retain exact stage/assignee IDs and may also return
`current_stage_label` / `current_assignee_label`: current same-Organization catalog
display hints, bounded to 256 characters each. The UI labels them as current,
keeps identifiers inspectable, and never submits these hints as source evidence
or substitutes them for the frozen mapping checks.

The capability is `fub-admitted-people-refresh-v1`. Release readiness names
`ADMITTED_PEOPLE_REFRESH_CAPABILITY`, `admitted_people_refresh_ready`, and
`require_admitted_people_refresh`; the schema inventory requires all nine
tables including `person_admitted_refresh_provenance` and
`crm_admitted_people_refresh_mutation_allowed(uuid,text,text,text,jsonb,jsonb)`.
Old readers/workers fail closed at this capability boundary.

The only review-workspace mutation authority is the transaction-local
`crm.admitted_people_refresh_permit` JSON `{lease,item}`. Its six-argument
predicate receives independent OLD and NEW JSON images and resolves one live
running refresh lease, one unsettled eligible item, the exact admission/result/
identity/live same-Organization target, exact baseline pointer/version, matching
workspace parent/plan/revision and an active initiating administrator.

It permits:

* `person UPDATE` only when OLD/NEW ID and Organization equal the resolved
  target; the business delta is exactly `first_name`, `last_name`, `stage_id`,
  `assigned_user_id`; `updated_at` and schema-derived `stage_revision` are the
  only non-business differences. Schema-derived `mobile_revision` is also
  permitted only when the owned Person/contact delta invokes its trigger.
* `contact_method INSERT` only for a proposed owned contact of the target;
  `UPDATE` only for a baseline/current owned contact with fixed ID/Organization/
  Person/kind and a delta limited to `value`, `normalized_value`, `import_order`;
  `DELETE` only for a baseline/current owned contact.
* `stage_changed` and `assignment_changed INSERT` only for the target with
  `actor_kind=system`, `origin=migration`, `reason=migration_refresh` and the
  refresh initiator as `on_behalf_of_user_id`.

It permits no other table, operation or column, no target substitution, no old
010e2 permit, and no `crm.import_token` broadening. It does not permit inquiry,
tag, custom-field, note, task, identity, source-attribution, activation or
history mutation. The mobile-derived stage revision trigger remains schema-owned
and advances only for a real stage ID transition.

## Settlement, accounting and recovery

One bounded transaction takes the existing parent serialization then owned
refresh/baseline/Person/contact locks. It revalidates authority, review binding,
lease fence, report boundary/manifest, source/mapping evidence, baseline pointer,
target fingerprint and permit before changing all core fields/contacts. It
appends immutable encrypted before/after provenance and settlement, advances the
feature baseline, records migration stage/assignment facts only when changed,
advances progress, and settles its reservation atomically. Stale change yields
`held_stale` without a partial write.

States are `preparing`, `ready`, `queued`, `running`, `paused`, `completed`,
`cancelled`; ready plans are immutable and expire after ten minutes. Exactly one
active refresh exists per admission cohort. Cancellation fences leases but retains
settled data/baselines. Closed outcome values are `eligible`, `already_current`,
`held_local_change`, `held_evidence_gap`, `held_mapping_gap`,
`held_target_missing`, `held_original_hold`, `excluded_source_only`,
`not_seen_again`, `settled`, `settled_noop`, `held_stale`, and `cancelled`.
Recovery pause reasons are `initiator_not_authorized`, `storage_budget_exhausted`,
`release_not_ready`, `retained_integrity_failed`, `retained_evidence_invalid`,
`source_binding_changed`, `work_unit_timed_out`, and `lease_expired`.

The existing snapshot/Organization ledger is used with feature-owned
reservations, a cancellation allowance, 16 MiB raw read and 64 MiB retained work
unit limits. Charges once: plan/item/contact/result/baseline/request envelope
nonces+ciphertexts, digest, variable source keys/IDs, checkpoint and provenance;
fixed UUID/numeric/boolean/timestamp columns and native projections are not a
second charge. Shared raw/admission evidence keeps its original charge. The
C1/C2 erasure inventory includes all nine tables without claiming a new
retention or per-Person crypto-shred policy.

## Web boundary

The panel is a separate `AdmittedPeopleRefreshPanel` on admission review. It
selects only terminal admissions with successful results and a later qualified
report; labels the admission origin; shows coverage, all held/excluded/missing
items, exact clears/removals, before/current/proposed bounded detail and lifecycle
recovery. It requests the bounded availability result only for its selected
admission/report pair, keys that result to the active Organization/identity and
pair, displays its closed reason, and enables preparation only when the server
returns `available: true`. It does not reuse, relabel or
hide the old original-People refresh/family controls. It never executes a field
prefix or browser-generated identity as source evidence.
