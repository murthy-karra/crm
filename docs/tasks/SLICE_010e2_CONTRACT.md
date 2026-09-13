# Slice 010e2 contract checkpoint

Frozen on 2026-09-12 from D-076 and `SLICE_010e2.md`. This checkpoint owns
the additive 010e2 DTO and persistence vocabulary; it does not alter existing
010c import or 010e1 report contracts.

## Route and request contract

All endpoints are rooted at `/api/migrations/fub/people-refreshes`, require a
current organization administrator and emit `Cache-Control: no-store`.

| Endpoint | Request | Success |
| --- | --- | --- |
| `POST /` | `{ "request_id": UUID, "report_id": UUID }` | `201`, immutable refresh resource |
| `GET /` | `parent_import_id`, optional opaque `cursor`, optional `limit<=20` | bounded resource page |
| `GET /{id}` | none | resource, plan revision/digest/counts/actions |
| `GET /{id}/items` | optional closed `disposition`, opaque cursor, `limit<=50` | bounded item page |
| `GET /{id}/items/{item_id}` | none | bounded current/baseline/proposed scalar preview |
| `GET /{id}/items/{item_id}/contacts` | opaque cursor, `limit<=50` | bounded owned-contact diff page |
| `GET /{id}/items/{item_id}/fields/{side}/{field}` | opaque cursor, byte `limit` 4–16384 (default 16384); side `baseline/current/proposed`, field `first_name/last_name` only | complete retained scalar in UTF-8-safe pages, bounded 128 KiB response |
| `POST /{id}/plans` | `{ "request_id": UUID, "expected_plan_revision": integer }` | explicit new immutable preview revision |
| `POST /{id}/confirm` | `{ "request_id": UUID, "plan_id": UUID, "plan_revision": integer, "plan_digest": string, "acknowledged_coverage": boolean, "acknowledged_exclusions": boolean, "acknowledged_name_clears": integer, "acknowledged_assignment_clears": integer, "acknowledged_contact_removals": integer }` | `202`, queued resource/receipt |
| `POST /{id}/retry` | `{ "request_id": UUID, "expected_lifecycle_revision": integer }` | `202`, queued resource/receipt |
| `POST /{id}/cancel` | `{ "request_id": UUID, "expected_lifecycle_revision": integer }` | `200`, cancelled resource/receipt |
| `GET /{id}/results` | opaque cursor, `limit<=50` | bounded immutable settlement page |

`request_id` is bound to organization, actor, action, normalized body and
resource. Exact replay returns the saved receipt; changed input conflicts.
Unknown request keys are rejected. Foreign resources are non-disclosing.

Item pages carry metadata only, including nullable `source_id`, disposition,
separate clear/removal counts and closed `no_instruction` component names. Scalar
detail includes `contact_counts` and `truncated_fields`; a name longer than
1024 UTF-8 bytes is shown as an explicitly marked prefix. The field endpoint
returns `plan_id`, decimal-string `plan_revision`, `side`, `field`, decimal-string
byte `offset` and `total_bytes`, `fragment`, and opaque `next_cursor`. Fragments
are read-only; confirmation always names the full immutable plan and never
submits a displayed prefix. Contacts return their frozen side, exact contact
identity/order, owned value/normalization and `add/remove/retain/current` label.

All cursor tokens are authenticated and bind the trusted Organization/actor,
resource, endpoint, plan ID/revision/digest where applicable, filter, and page
limit. Run and settlement traversals freeze their upper key; a new traversal
is explicit. Preview dispositions and values remain frozen after settlement;
the results endpoint carries the resulting outcome. Old sealed previews remain
inspectable while a successor prepares; partially built previews are unavailable.

## Persistence and private authority

Migration `20260926000001_fub_people_refresh.sql` adds these additive,
organization-scoped tables: `migration_people_refresh`,
`migration_people_refresh_plan`, `migration_people_refresh_item`,
`migration_people_refresh_contact`, `migration_people_refresh_result`,
`migration_people_refresh_baseline`, `migration_people_refresh_receipt`, and
`migration_people_refresh_reservation`.

The immutable original baseline is reconstructed only from a successful
`migration_import_result`, its `migration_import_contact` rows, original plan
source/mapping evidence, and the current same-organization Person. Refresh
never changes any 010c row. Later baselines point at immutable 010e2 results.

Only the refresh worker may set transaction-local `crm.people_refresh_permit`.
The database workspace trigger admits that permit only for the exact refresh
tables plus `person`, `contact_method`, `assignment_changed`, and
`stage_changed`, and only while the workspace remains bound to the original
confirmed parent plan in `migration_review`. Ordinary writers retain the
existing review-mode denial.

## Eligibility and execution

Preparation walks every retained `people` report row, including `unchanged`.
It accepts only original `imported` People with a matching immutable source
identity and qualified newer evidence. It freezes report output revision,
parent/plan/account, source boundary, original mappings, exact projection,
and a SHA-256/HMAC digest before publishing a ready plan.

The executor processes one Person under parent then Person locks. It rechecks
actor membership, review workspace binding, fencing lease, frozen baseline and
destination fingerprint. `C != B` holds the whole Person even when `C == N`.
Missing, unowned, replaced, malformed, partial, ambiguous, tombstoned or
unmapped evidence holds the whole Person. Missing source fields are
`no_instruction`; only a present qualified null/blank (or `[]` for a complete
contact collection) is a clear/removal. All names, contact rows/order,
stage/assignment, immutable result, baseline pointer, receipt/progress and
byte settlement commit atomically.

Refreshes serialize per parent. A confirmed source boundary is monotonic; an
older or overlapping report cannot be confirmed. Cancellation fences later
writes but retains settled results and baseline pointers. The initiator alone
confirms/retries; any current administrator may read/cancel. Preparation and
each person unit use bounded keyset pages (50), raw input (16 MiB) and total
unit retention (64 MiB).

## Closed outcomes

Items and results use only: `eligible`, `already_current`,
`held_local_change`, `held_evidence_gap`, `held_mapping_gap`,
`held_target_missing`, `held_original_hold`, `excluded_source_only`,
`not_seen_again`, `settled`, `settled_noop`, `held_stale`, and `cancelled`.
The UI may group these but must never expose raw payloads or submit clipped
values as executable input. Confirmation compares exact plan counts for name
clears, assignment clears, and contact removals independently.
