# Slice 010f4 — implementation verification

**IN PROGRESS.** Independent implementation review round 1 returned NOT READY;
the fixes and review budget are tracked in
[the paired review record](MOBILE_006_010f4_IMPLEMENTATION_REVIEW.md).
This record contains executed synthetic evidence. Browser acceptance, full DB/
SQLx gates, paired performance and review round 2 remain open.

## Runner and source

- Integration branch: `codex/mobile006-010f4-integration`.
- Production source qualification, bounded remainder and old-binary handover:
  `3c01177`; subsequent Rust style changes through `109cd58` preserve behavior.
- Private evidence root: `/private/tmp/crm-mobile006-010f4-thyhauvv`.
- Root serializes DB tests, native API work, browser work and performance. Cargo,
  Web and SQLx outputs are isolated from shared API3000/Web5173 artifacts.
- No live FUB/customer, activation, distribution or deployment evidence is claimed.

## Acceptance matrix

The `db_admitted_activity` selectors below ran through the `all` integration
target with `--ignored --exact` or the documented module filter, one test thread.
`logs/integration/f4-review-matrix-db-2.log` contains 12/12 passes (97.29s).

| Requirement | Executed evidence | Status / remaining work |
|---|---|---|
| F4-01 terminal cohort and identity ownership | `freezes_terminal_cohort_full_source_and_global_owner`; exact original/admission comparisons in the fixture | Focused DB pass; browser rowset inventory pending |
| F4-02 source qualification | `source_binding_requires_all_six_streams_and_frozen_output`, `selected_people_missing_and_conflicting_hold_related_units`; additional corrupt-People test | Focused DB pass |
| F4-03 fidelity, roles and times | Shared `activity_source` / `activity_html` interpreters run by `scripts/check`; retained full-source admitted fixtures | Focused/inherited checks pass |
| F4-04 global equality, local edits and deletion | `existing_equality_local_edits_tombstones_and_missing_identity_target`, global-owner and two-worker cases | Focused DB pass |
| F4-05 atomicity, retry and remainder | `result_failure_rolls_back_native_identity_checkpoint_and_bytes`, `authority_loss_two_workers_and_exact_remainder_replay`, `partial_remainder_copy_cancel_preserves_complete_source`, unconfirmed cancellation/reprepare | Focused DB pass |
| F4-06 tenant/admin/workspace authority | `http_tenants_private_permit_and_native_review_cursor_fences`, authority-loss case | Focused DB pass |
| F4-07 old readers/workers and readiness | `old_reader_and_worker_barrier_fails_after_confirmation`, `readiness_rejects_incomplete_schema_grants_guards_and_owner` | Focused DB pass; 13 DDL/grant/guard negative variants |
| F4-08 bounded review/provenance and page invalidation | HTTP/private-permit/native-cursor case, shared cursor unit tests, excluded coverage | Focused DB pass; browser inspection pending |
| F4-09 real desktop/390px acceptance | Owned API3107 + Web5177 fixture and controlled worker are prepared | Pending |
| F4-10 realistic plans and paired reads | 21 exact hot-statement EXPLAIN probes at 25k People/50 members | Plan pass; paired Person/Today run pending |

Selector names in the table have the common `admitted_activity_` prefix. The
additional corrupt-People test passed in
`logs/integration/activity-corrupt-people-db-2.log` (6.84s), at `fcf6e42`.
It proves a corrupt selected capture pauses before native/identity results;
explicit cancellation then releases the retained retry reservation and settles
byte accounting. Its first attempt incorrectly expected a paused reservation to
be zero; that failed test assertion is retained in the `-1.log` file.

## Query-plan and storage evidence

- `integration/activity-hotplans-1.json`: 21 probes, zero failures; first actual
  run passed in 227.05s.
- `integration/activity-hotplans-2.json`: same 21 probes, zero failures, with
  separately recorded `pg_total_relation_size` values; passed in 253.70s.
- Fixture: 25,000 People, 50 members, 25,001 notes, 50,001 tasks, 25,010 indexed
  source observations, 25,004 manifests, 12,502 results and 25,002 identities.
- Evidence retains exact SQL, source/SQL hashes, bind values, output bounds and
  `EXPLAIN (ANALYZE, BUFFERS)` plans. Large point reads must avoid full relation
  scans; bounded outputs and temporary/hash spill checks are enforced.
- Bulk scale rows use explicitly inert copied ciphertext after the small real
  worker fixture completes. They establish cardinality and plan shape, not
  decryption, migration fidelity or production storage capacity. Allocated relation
  sizes include indexes and PostgreSQL update/dead-tuple overhead; they are not
  estimates of live customer storage.

## Integrated repository check

`logs/integration/final-local-check-3.log` passed in 361.765s: 51 release-preflight
tests, Rust formatting/Clippy/production compilation and dependency fences,
993 Rust tests, five doctests, Web lint/typecheck, 1,293 Web tests, isolated Web
build and 11 email-worker tests. The preceding two attempts retain a stale
readiness test initializer and Clippy failures; those were repaired.

The full database suite, regenerated SQLx cache, paired performance and final
independent review are separate gates and are not implied by this check.
