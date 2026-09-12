# Slice 010f1 acceptance evidence map

**All A1–A12 criteria synthetically verified, 2026-09-11.**
This maps the [approved specification's criteria][spec] to inspected assertions
and actual execution. The final prescribed sequence passed 900 Rust, 1,054 Web
and **881 DB tests**, including all 36 metadata DB cases. Query collection passed
90 plans / 246 checks. Five final browser phases passed 51 checkpoints, and all
18 screenshots were inspected. See the [verification chronology][verification]
and [QA evidence index][qa] for exact scope, failures/corrections and artifacts.

Both additional test files are [registered][test-registration]. The inspected
private `backend-acceptance-1.log` records all eight new acceptance/concurrency
cases plus six adjacent cases passing: **14/14, zero failures**, 40.24 seconds of
test execution after a 30.63-second build. Its SHA-256 is
`e56b47afed13b0b2911bf20b2fa0707609bf7d0abbd204c0ce92de779467bbb8`.
The exact new tests and their proof are listed below. See the
[QA evidence index][qa] for final artifacts.

## Acceptance matrix

| ID | Acceptance | Existing tests and actual seams | Status / remaining proof |
|---|---|---|---|
| A1 | Completed bound parent, same frozen snapshot, exhausted custom-fields stream, no source calls | [metadata_http_rejects_uncompleted_parent_and_incomplete_custom_fields][http]; [metadata_http_creation_is_admin_scoped_strict_and_exactly_idempotent][http]. Tests reject an unconfirmed parent, damaged stream exhaustion, foreign access and forged body context. Source-reader counters remain unchanged through child preparation and execution helpers. [metadata_acceptance_cancelled_and_running_parents_are_ineligible][acceptance] adds both explicit parent states reached by typed commands. | Passed in final gates and browser/native reconciliation; zero post-capture source calls. |
| A2 | Exact raw qualification, all variants, corrupt/unsupported evidence held | [metadata_source_requalifies_people_and_definitions_after_completed_parent][source] alters ordinals, semantic HMAC, capture/record representation, HTTP status, acceptance, truncation, raw length and raw evidence. [metadata_source_all_variants_and_exact_raw_identity_override_preview][source] checks semantic variants, exact large numeric IDs and deliberately false display projections. Pure source and crypto tests below cover parsing and scope binding. | Passed in the final source/HTTP/library suites. |
| A3 | Honest embedded-tag, machine-key, choice, null/empty and unsupported-type distinctions; exact provenance | [metadata_source_large_and_nul_keys_can_map_without_native_key_coercion][source]; [metadata_source_native_values_preserve_precision_absence_literals_and_recurrence_holds][source]; [metadata_source_full_multimegabyte_evidence_survives_execution_without_truncation][source]; [metadata_http_full_evidence_segments_bind_person_field_and_revision][http]. Source unit tests distinguish malformed/absent/null/empty collections and preserve literal declared `None`, `N/A` and `null` choices. | Passed in final suites and real >2-MiB/128-digit browser inspection. |
| A4 | Explicit catalog choices, unchanged quotas, archive/source bindings, no many-field merge | [metadata_source_capacity_archival_and_local_conflicts_preserve_native_rows][source] exercises 200 tags/Org, 50 live fields and archived targets. [metadata_source_shared_tags_do_not_merge_people_and_overflow_holds_the_whole_link_set][source] holds the entire overflowing new-link set at 20 tags/Person. [metadata_source_machine_key_collisions_hold_while_equal_labels_map_explicitly][source], [metadata_r1_native_choice_case_collision_holds_dependencies_before_ready][r1] and [metadata_r1_qualified_oversized_machine_key_does_not_offer_creation][r1] cover collisions, dependencies and create-vs-map eligibility. The two new acceptance tests below directly exercise same-target and existing source-binding conflicts. | Passed in final source/gate/acceptance suites and explicit browser mappings. |
| A5 | Only committed parent People receive links/absent values; equal values idempotent, differing values held | [metadata_source_shared_tags_do_not_merge_people_and_overflow_holds_the_whole_link_set][source] keeps overlapping People separate. [metadata_source_capacity_archival_and_local_conflicts_preserve_native_rows][source] uses explicitly labelled migrator-only negative fixtures for preexisting equal/different local values and asserts native rows. [metadata_acceptance_tombstoned_parent_person_cannot_regain_metadata_or_history][acceptance] adds direct tombstone exclusion and complete native Person/contact/Inquiry/history plus parent/workspace comparisons. | Passed in final suites and exact per-Person native API reconciliation. |
| A6 | Revision/confirmation/expiry fencing and exact lost-response replay | [metadata_http_frozen_choices_enforce_patch_bounds_and_cursor_scope][http]; [metadata_http_confirmation_replays_and_preserves_parent_and_review_hold][http]; [metadata_http_committed_receipt_survives_expired_and_missing_readiness][http]. Includes simultaneous identical confirmations, stale revisions, changed bodies, member rejection and exact receipt replay after readiness expires/disappears. Query unit tests bind cursor owners and filters. [metadata_acceptance_expired_ten_minute_plan_requires_explicit_replan][acceptance] directly proves the separate plan-expiry rule. | Passed in final HTTP/acceptance suites and identical-byte lost-response browser replay. |
| A7 | Atomic catalog/Person units; crash, retry and cancel cannot duplicate or commit late | [metadata_gate_exact_inventory_concurrent_workers_and_duplicate_recovery][gate]; [metadata_gate_result_failure_rolls_back_native_identity_cursor_and_bytes][gate]; [metadata_gate_expired_lease_reclaim_and_partial_cancel_preserve_units][gate]. The three new [concurrency tests][concurrency] add a confirm/cancel start barrier, an observed database barrier after both Person native writes, and failure injection at that same result boundary. | Passed in final DB race/barrier/rollback suites and actual partial cancellation. |
| A8 | Review hold, narrow child permission, current roles and old-token fencing | [metadata_gate_private_permit_has_no_unrelated_insert_update_delete_or_parent_bypass][gate]; [metadata_r1_executor_demotion_requires_explicit_current_admin_adoption][r1]; [metadata_http_confirmation_replays_and_preserves_parent_and_review_hold][http]. The permit test covers incorrect units/targets, old parent tokens, expired leases and post-completion updates/deletes. Demotion requires explicit adoption by a current admin. Existing workspace suites cover ordinary mutation, Today, Operator and background paths. | Passed in final DB/HTTP suites and browser administrator demotion/member hold. |
| A9 | Exact accounting, fan-out/control receipts, current ceilings and exhausted cancellation | [metadata_gate_exact_inventory_concurrent_workers_and_duplicate_recovery][gate]; [metadata_gate_current_ceiling_pause_explicit_retry_and_cancel_at_exhaustion][gate]; [metadata_gate_catalog_continues_in_fifty_descriptor_units][gate]; [metadata_r1_result_counts_match_operations_including_held_null_and_absent][r1]. The gate `inventory` helper sums persisted variable columns and compares reservations. Large evidence and 121-tag/121-choice fixtures exercise fan-out. | Passed in final accounting suites and actual lower-ceiling pause, no auto-resume, explicit resume and reservation release. |
| A10 | Indexed/bounded hot queries at D-050 scale, including rare/empty filters | [metadata_hot_queries_at_d050][plans] extracts actual application SQL and checks `EXPLAIN (ANALYZE, BUFFERS)` output for claim/source/dependency/manifest/result/field and related paths. The synthetic scale fixture is distinct from executable capture/accounting fixtures. | Passed: 90 actual plans / 59 SQL hashes / 246 unchanged bounds. Initial failed fixture and correction retained in PERFORMANCE.md. |
| A11 | Dirty choices, uncertainty, progress, authority changes and long evidence | [MetadataMappingPanel tests][mapping-web], [MetadataImportPanel tests][panel-web], [MetadataFieldViewer tests][field-web], [MetadataRecordPanel tests][record-web], [PersonMetadataProvenance tests][provenance-web] and [metadataImports API tests][api-web]. Focused cases are listed below. | Passed: 1,054 Web tests, 51 final browser checkpoints, 18 inspected desktop/390px screenshots, zero page errors. |
| A12 | Existing 010a/b/c/native CRUD responses and workspace isolation remain compatible | Existing [import HTTP tests][parent-http], [import gate tests][parent-gate], [migration tests][migration-tests], [workspace HTTP tests][workspace-http] and [workspace background tests][workspace-background]; [release-preflight Python tests][preflight-tests]; [scripts/check][check] and [scripts/check-db][check-db]. Startup/preflight and synthetic workflow artifacts belong to the root verification record. | Passed: final SQLx/check/check-db sequence, synthetic startup and actual-DB metadata-capability preflight; contracts and hashes recorded. |

## Focused pure Rust and Web evidence

The [source qualification unit tests][source-unit] include these exact cases:

- `canonical_evidence_keeps_exact_numbers_unknown_fields_and_marker_objects`
- `duplicate_keys_and_invalid_shape_fail_closed`
- `page_bounds_apply_before_deriving_metadata`
- `tag_collection_absence_null_empty_and_bad_shape_remain_distinct`
- `tag_items_preserve_ordinals_duplicates_and_independent_holds`
- `declared_null_like_choices_are_literals_and_require_exact_matching`
- `numbers_preserve_exact_value_and_reject_rounding_and_exponent_blowup`
- `dates_require_exact_calendar_shape_and_native_range`
- `recurrence_is_held_or_explicitly_omitted_without_importing_behavior`
- `text_trimming_is_disclosed_without_converting_missing_null_or_empty`
- `source_holds_and_unsupported_definition_shapes_do_not_become_values`

[Crypto tests][crypto-unit] include
`snapshot_payloads_and_lookup_keys_bind_tenant_run_row_and_purpose`.
[Query tests][query-unit] include
`page_cursor_binds_endpoint_plan_filter_entity_limit_and_tenant`,
`page_shrinks_before_its_byte_ceiling_and_advances_from_last_visible_item`, and
`field_segments_use_exact_offsets_and_do_not_reuse_other_evidence_cursors`.

| Web suite | Representative exact test names / assertions |
|---|---|
| [MetadataMappingPanel][mapping-web] | `keeps explicit drafts across pages and kinds without choosing suggestions`; `caps drafts at 50 across pages and frees capacity when an inherited choice is restored`; `paginates existing targets and requires an explicit compatible choice`; `waits for an applied parent field mapping before looking up its existing options`; `refreshes progress on the current kind and page without losing its mapping draft`. Additional cases fence late reads and preserve existing mapping when native creation is unavailable. |
| [MetadataImportPanel][panel-web] | `freezes exact confirmation only after every acknowledgment, without activating the workspace`; `replays the exact lost confirmation receipt even after completion and readback`; `clears frozen requests and private summaries when the actor loses admin access`; `makes resume and terminal cancellation explicit and never starts work on allowance refresh`; `refreshes visible results as progress changes and stops after completion`. |
| [MetadataFieldViewer][field-web] / [PersonMetadataProvenance][provenance-web] | `escapes retained text and shows one UTF-8 segment with accurate byte positions`; `removes displayed fields and pending reads immediately on admin role loss`; `shows source and applied values separately and reads the exact committed result field`; `discards prior Person evidence when navigating or losing tenant authority`. |
| [MetadataRecordPanel][record-web] / [metadataImports][api-web] | `clears incompatible filters when switching between planned and committed views`; `keeps exact 128-digit source IDs and opaque continuation cursors from alias evidence`; `keeps all four segmented evidence owners distinct, including committed results`; `passes uncertain mutation failures to the caller without an automatic retry or new request ID`. |

## Additional direct proof: focused run passed

Every test below appears as `ok` in `backend-acceptance-1.log`. Both files are
registered in the consolidated DB test target. The focused result is 14/14 because
the filter also ran six adjacent tests; it is not a 14-case new-test suite or a
replacement for the complete 881-case final gate, which also passed.

| Criterion | Exact new test | Observed proof | Focused result |
|---|---|---|---|
| A1 | [metadata_acceptance_cancelled_and_running_parents_are_ineligible][acceptance] | Reaches cancelled and running parent states through typed commands, rejects child creation, preserves parent detail, creates no child and makes no source calls. | Passed in final gates and browser/native reconciliation; zero post-capture source calls. |
| A4 | [metadata_acceptance_two_valid_fields_cannot_merge_into_one_native_target][acceptance] | Both otherwise qualified source fields targeting one native field become held with `mapping_target_collision`; their dependent values remain held while an independent value applies. Native target and complete business/parent history remain unchanged. | Passed in final source/gate/acceptance suites and explicit browser mappings. |
| A4 | [metadata_acceptance_existing_different_fub_binding_cannot_be_reassigned][acceptance] | An explicitly labelled migrator-only existing FUB binding fixture rejects reassignment without advancing the plan; independent work applies and the original complete bound field row remains unchanged. | Passed in final source/gate/acceptance suites and explicit browser mappings. |
| A5 | [metadata_acceptance_tombstoned_parent_person_cannot_regain_metadata_or_history][acceptance] | Erases one synthetic native Person while retaining parent result/identity tombstones. Child manifest/result hold that target with no operations; metadata reaches only the live Person. Complete Person/contact/Inquiry/history and parent/workspace rows remain equal before/after; erased-target provenance returns not found. | Passed in final suites and exact per-Person native API reconciliation. |
| A6 | [metadata_acceptance_expired_ten_minute_plan_requires_explicit_replan][acceptance] | Asserts a 600-second ready-plan lifetime, rejects an explicitly expired fixture without confirmation receipt/native writes, proves no automatic replan, then explicitly prepares a new revision/digest and executes it. | Passed in final HTTP/acceptance suites and identical-byte lost-response browser replay. |
| A6–A7 | [metadata_concurrency_confirm_cancel_race_has_one_terminal_receipt_outcome][concurrency] | Starts real confirm/cancel commands at a shared barrier; either serialized confirmation outcome replays consistently, cancellation is terminal and replayable, and draining creates no native metadata/results/identities or remaining reservations. | Passed. |
| A7 | [metadata_concurrency_cancel_waits_for_complete_inflight_person_unit][concurrency] | A test trigger proves both native writes occurred inside the transaction before blocking its Person-result insertion. External reads see no partial writes/ledger changes; observed blocking proves cancel waits for that worker. Releasing the barrier commits exactly one complete Person result before terminal cancellation; replay/drain cannot change it. | Passed in final DB race/barrier/rollback suites and actual partial cancellation. |
| A7, A9 | [metadata_concurrency_person_failure_rolls_back_all_cells_result_cursor_and_bytes][concurrency] | Injects failure after both native writes, proves links/values and complete ledger/result/identity/receipt/checkpoint state roll back, requires explicit retry, and commits exactly one Person result with no duplicate writes or leftover reservations. | Passed. |

The two new source files at this focused checkpoint hash to:

- Acceptance: `e91d7712ac6efaab52f8072f9e287cf84945dfc44310fc32e0c70182a974e8cb`.
- Concurrency: `8a49482f2c9f23eb0c6a75552995d41c724900832be3695882286f46262dd00c`.

The earlier normal gates remain distinct from this focused run. The final repeat,
actual performance collection and browser/native reconciliation subsequently passed.

## Scope of the acceptance result

Evidence is synthetic and retained-source only. Reader-counter assertions prove
the child does not make new source calls; they do not validate live FUB behavior.
Activation, ordinary workspace operation, notes/tasks, repair/delta import and
customer-data readiness remain outside this slice. The browser does not seed
differing Person values through a review-hold bypass: that conflict is an explicit
database negative fixture. Preserve the original parent and admin review hold.

[spec]: ../../../specs/SLICE_010f1.md#9-acceptance-and-verification
[verification]: ../../../tasks/SLICE_010f1_VERIFICATION.md
[qa]: README.md
[http]: ../../../../backend/crates/crm-api/tests/db_metadata_import_http.rs
[source]: ../../../../backend/crates/crm-api/tests/db_metadata_import_source.rs
[gate]: ../../../../backend/crates/crm-api/tests/db_metadata_import_gate.rs
[r1]: ../../../../backend/crates/crm-api/tests/db_metadata_import_r1.rs
[acceptance]: ../../../../backend/crates/crm-api/tests/db_metadata_import_acceptance.rs
[concurrency]: ../../../../backend/crates/crm-api/tests/db_metadata_import_concurrency.rs
[test-registration]: ../../../../backend/crates/crm-api/tests/all.rs
[plans]: ../../../../backend/crates/crm-api/tests/db_metadata_import_plans.rs
[source-unit]: ../../../../backend/crates/crm-app/src/domain/migration/metadata_source.rs
[crypto-unit]: ../../../../backend/crates/crm-app/src/domain/migration/crypto.rs
[query-unit]: ../../../../backend/crates/crm-app/src/domain/migration/metadata_queries.rs
[mapping-web]: ../../../../web/src/components/migration/MetadataMappingPanel.test.ts
[panel-web]: ../../../../web/src/components/migration/MetadataImportPanel.test.ts
[field-web]: ../../../../web/src/components/migration/MetadataFieldViewer.test.ts
[record-web]: ../../../../web/src/components/migration/MetadataRecordPanel.test.ts
[provenance-web]: ../../../../web/src/components/migration/PersonMetadataProvenance.test.ts
[api-web]: ../../../../web/src/api/metadataImports.test.ts
[parent-http]: ../../../../backend/crates/crm-api/tests/db_import_http.rs
[parent-gate]: ../../../../backend/crates/crm-api/tests/db_import_gate.rs
[migration-tests]: ../../../../backend/crates/crm-api/tests/db_migration.rs
[workspace-http]: ../../../../backend/crates/crm-api/tests/db_workspace_http.rs
[workspace-background]: ../../../../backend/crates/crm-api/tests/db_workspace_background.rs
[preflight-tests]: ../../../../scripts/tests/test_migration_release_preflight.py
[check]: ../../../../scripts/check
[check-db]: ../../../../scripts/check-db
