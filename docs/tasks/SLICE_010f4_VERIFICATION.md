# Slice 010f4 — implementation verification

**COMPLETE — READY in final implementation round 2.** Round 1 returned NOT READY;
the fixes and review budget are tracked in
[the paired review record](MOBILE_006_010f4_IMPLEMENTATION_REVIEW.md).
This record contains executed synthetic evidence. Full DB and paired performance
now pass; final verdicts are in the paired review record.
The real desktop/390px browser acceptance and rowset/accounting reconciliation passed.
The standalone SQLx preparation check passed; its cache is unchanged.

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
| F4-01 terminal cohort and identity ownership | `freezes_terminal_cohort_full_source_and_global_owner`; exact original/admission comparisons in the fixture | Focused DB and browser rowset inventory pass |
| F4-02 source qualification | `source_binding_requires_all_six_streams_and_frozen_output`, `selected_people_missing_and_conflicting_hold_related_units`; additional corrupt-People test | Focused DB pass |
| F4-03 fidelity, roles and times | Shared `activity_source` / `activity_html` interpreters run by `scripts/check`; retained full-source admitted fixtures | Focused/inherited checks pass |
| F4-04 global equality, local edits and deletion | `existing_equality_local_edits_tombstones_and_missing_identity_target`, global-owner and two-worker cases | Focused DB pass |
| F4-05 atomicity, retry and remainder | `result_failure_rolls_back_native_identity_checkpoint_and_bytes`, `authority_loss_two_workers_and_exact_remainder_replay`, `partial_remainder_copy_cancel_preserves_complete_source`, unconfirmed cancellation/reprepare | Focused DB pass |
| F4-06 tenant/admin/workspace authority | `http_tenants_private_permit_and_native_review_cursor_fences`, authority-loss case | Focused DB pass |
| F4-07 old readers/workers and readiness | `old_reader_and_worker_barrier_fails_after_confirmation`, `readiness_rejects_incomplete_schema_grants_guards_and_owner` | Focused DB pass; 13 DDL/grant/guard negative variants |
| F4-08 bounded review/provenance and page invalidation | HTTP/private-permit/native-cursor case, shared cursor unit tests, excluded coverage | Focused DB and browser source/provenance inspection pass |
| F4-09 real desktop/390px acceptance | Owned API3107 + Web5177; real desktop/390px confirmation, two partial cancellations/remainders and account switch | Pass; evidence below |
| F4-10 realistic plans and paired reads | 26 final exact hot-statement EXPLAIN probes at 25k People/50 members | Plans and paired Person/Today comparison pass |

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

## Integrated repository checks

[Integrated final verification](MOBILE_006_010f4_FINAL_VERIFICATION.md) owns shared
repository, SQLx, full DB and paired performance results.

## Real desktop and 390px browser acceptance

Executed through the real Vue UI on isolated Web5177/API3107, with retained
synthetic Organization `ce19b8f8-7983-4d14-9bac-c75905ed5023`. The source reader
was empty/disabled; controlled worker statistics ended with **zero source calls**.
One shared root exercises both responsive layouts; duplicate seed lifecycles were
not required by the reviewer.

- Prepared the qualified six-stream report for 60 terminal admitted People. Plan1
  held one note and 60 tasks pending four explicit role mappings.
- Inspected mapping evidence and CRM user pickers at desktop/390px. Mapped note
  author and task creator to the synthetic admin, assignee to the active synthetic
  member, and task kind to Call. Plan2 showed one eligible note, 60 eligible tasks,
  zero held, and one retained source-only component. Tasks are explicitly undated.
- Inspected full native preview, complementary list/detail observations and exact
  retained capture text. Reviewed the counted confirmation at both widths and
  confirmed at 390px. No ordinary workspace activation occurred.
- Desktop: settled one note/four tasks, permanently cancelled, then continued the
  exact 56-task remainder. At 390px: settled five more tasks, cancelled, and
  continued the exact remaining 51. The last successor completed all 51.
- Reloaded and selected predecessor/successor attempts with the labeled selector.
  Original cancelled results remained inspectable; the completed successor showed
  zero pending records. All three roots and the Organization released reservations.
- Person review showed the full imported note, creator/assignee, explicit undated
  task, original admission facts and administrator review hold. Actual note/task
  source inspection exposed the remaining F4-I4 reader-family defect; the client
  now selects the admitted reader only from the exact scoped server provenance
  path, never by fetching arbitrary URLs. Both note and task source reads then
  passed in the browser; the note source was inspected at both widths.
- Logged out and signed in as the synthetic member. Direct migration navigation
  showed only workspace review status at both widths, with prior source/body data
  absent. The browser was then paused at `about:blank` for serialized DB gates.

| Attempt | Final state | Native rows settled | Exact remainder |
|---|---|---|---|
| `12a42158-bded-41a5-999f-9fc9487df3d2` | Cancelled | 1 note + 4 tasks | 56 tasks |
| `a3144ff6-7104-468f-8b80-96aadbad9c01` | Cancelled | 5 tasks | 51 tasks |
| `5ae347d5-6bf5-48e1-bd11-5ca43c33be8a` | Completed | 51 tasks | 0 |

`integration/activity-ui/before-browser.json`, `after-browser.json` and
`browser-reconciliation.json` record **127 unchanged table rowsets**, including
original/admission/capture evidence and Person core/metadata projections. Person
`mobile_revision`/`updated_at`, derived history-review revisions and storage
ledgers are explicitly mutable. Exactly one note, 60 tasks, 61 global identities
and 61 committed results were added. Recomputed retained root bytes are 437,903,
193,994 and 239,619; their **871,516-byte** sum exactly matches the Organization
ledger increase. Every root and Organization reservation is zero. The final
inventory ran in `logs/activity-ui/inventory-after.log` (12.968s).

Screenshots under `integration/activity-ui/` include `desktop-confirmation.png`,
`390-author-picker.png`, `390-source-inspection.png`, `390-confirmation.png`,
`desktop-remainder-started.png`, `390-partial-cancel.png`,
`390-completed-remainder.png`, `desktop-person-source-fixed.png`,
`390-person-source-fixed.png`, and both `*-account-switch-fenced.png` files.

The native provenance repair passed 41 focused tests in
`logs/integration/web-native-admitted-source-r2.log`; original/admitted family,
unrelated URL and late account-change responses are covered. Typecheck passed in
`web-native-admitted-source-typecheck.log`. Final full Web/repository gate passed after this production change.

## Final remainder performance repair

The first 22-probe run (`activity-hotplans-db-r2.log`, 262.692s) failed the
broad-scan guard on the availability EXISTS. Its early-match execution examined
five rows, but review correctly identified the unbounded all-settled/late-match
case. The failure remains retained in `integration/activity-hotplans-3.json`.
The gate was not weakened.

`22c6c06` uses the canonical cancelled source's atomically committed pending
counts for both view and command availability. A cancelled partial copy uses its
complete source's counts. Manifest copying now fetches one indexed source row and
point-checks its result; already-settled rows advance exactly one checkpoint per
worker unit. New plans include late/empty source traversal and present/absent
result lookups. All 14 focused migration DB tests passed on `22c6c06` in 138.094s
(`activity-bounded-remainder-db-r2.log`), including no-remainder, partial-copy
and settled-row checkpoint cases. The current full repository gate also passed.
The final 26-probe run passed in 208.328s with zero failures:
`logs/integration/activity-hotplans-db-r3.log` and
`integration/activity-hotplans-4.json`. It uses the frozen `22c6c06` backend.
Full DB/paired gates pass; the previous failed plan is retained.

The final backend was also restarted without reseeding on API3107. Actual browser
reload and attempt selection showed the completed successor with 51 applied tasks,
zero pending records and zero reserved bytes. Evidence:
`integration/activity-ui/desktop-final-api-completed.png` and
`integration/activity-ui/final-api-reconciliation.txt`.
