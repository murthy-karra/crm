# Slice 011b — Verification evidence

Status: **COMPLETE AND MERGED TO LOCAL MAIN — independent review READY / ACCEPTED.**
All 16 approved criteria are supported; no actionable finding remains. The user
approved local commit and merge on 2026-09-06 after this verification. Push and
deployment remain outside the approved scope.

## Target and runners

- Working branch: `codex/slice-011b-saved-lists`, based on `1635fc4`.
- [Exact changed-file inventory](SLICE_011b_CHANGED_FILES.txt), including new
  migration, SQLx metadata, tests and performance evidence.
- Terra / ultra owns implementation, automated checks and serialized database
  operations. Astra / ultra coordinates and independently reviews the result.
- The coordinator operated the real browser against Vite at
  `http://127.0.0.1:5174`, a loopback fault proxy at port 3101, and the current
  API at port 3100 (PID 34640). API binary SHA-256:
  `1677cdbc58d2927877df64a57c04fe8042711b9ae91cc5c27474147b66717349`.
- The walkthrough used only synthetic accounts and People in the generated
  database `crm_011b_qa_20260906200225_34458_11455`. Existing development
  services/data were not replaced. Web corrections were hot-reloaded, and the
  affected flows were checked again. Browser results below are observations,
  not claims about a deployed build.

## Live browser observations

| Scenario | Observed result |
|---|---|
| Shared list viewed by two agents | The same symbolic `me` definition returned Bea/Blake for Agent One and Casey/Cory for Agent Two. |
| Dynamic membership | Reassigning Casey through the Person profile reduced Agent Two's list from two matches to one on re-entry. |
| Shared reader preview | An agent could clear the local filter, preview seven People and save a personal copy; the shared definition still matched the viewer's assigned People. |
| Lost create response | The proxy completed the upstream create then dropped its response. Name editing was disabled; Retry opened one new personal list. Two POST requests produced one visible definition. |
| Duplicate semantics | A dirty original kept its edited name and two-Person preview while Duplicate created an independent seven-Person saved-version copy. A clean Duplicate opened its new copy. |
| Update conflict | Tab B saved; Tab A's Refresh preserved its draft. Tab A's first and second Saves both conflicted, leaving B's version and A's draft intact. Explicit Reload restored the saved version. |
| Lost update response | Fresh detail reconciliation recognized both a renamed definition and a later nonempty `me` filter edit, disabled Save and removed the unsaved label without issuing another write. |
| Deleted uncertain create | The proxy dropped the committed create response; another tab deleted that list. Retry became terminal with disabled controls. Enter sent no additional create, and the refreshed index contained no replacement. Two POSTs, one DELETE, no resurrection. |
| Delete conflict | A confirmation made stale by another tab did not delete automatically. The user-visible notice required opening a new confirmation, which named the latest version. |
| Delete retains People | Deleting copies left their original list and all seven People intact. Shared deletion explained its Organization-wide effect. |
| Bounded counts and row recovery | A warm index Refresh evaluated 25 displayed definitions with at most four requests in flight. One forced count failure left other rows usable; its Retry added one request and page two added only two. Total: 28 count requests, maximum four in flight, zero People/detail requests for this isolated measurement. |
| Private and foreign links | Another member and the admin could not open Agent Two's personal definition. Foreign-Organization links failed in both directions. Missing links showed the same unavailable state. No table or Save as action appeared; isolated private/foreign checks made zero People requests. |
| Invalid definitions | Unsupported content exposed no filter editor or Save as. A stale stage exposed an invalid chip to its writer; a name-only change issued no People request. Removing the missing stage enabled a valid preview and Save. |
| Outage recovery | A forced 503 during Refresh retained the unsaved name after connectivity and Retry were restored. |
| Scope and names | An admin's create dialog defaulted to personal; shared required explicit selection. An 80-Unicode-scalar name was accepted without UTF-16 truncation. |
| Ordinary People and URLs | Save as list on `/people` saved empty criteria and opened the new list. A named URL ignored an unrelated `?filter=` override; the same filter on `/people` produced zero matches. Clear all preserved the unrelated context parameter and hash. |
| Inspector and keyboard | At the narrow breakpoint (375px measured content width), the loaded inspector was readable. Escape closed it and restored focus to the selected Person link. The viewport override was reset. |

The in-app browser's native `window.confirm` interaction blocked its automation
interface during the dirty-copy-link check; Cancel/Discard was not reliably
observed there. The real-router regression supplies that evidence: Cancel
invokes the guard and retains the route/draft; confirming then opens the copy.
The retained draft and saved-versus-working copy contents were directly
observed before navigation.

## Review corrections and checks

Independent backend findings I1–I4 and the coordinator's metadata-only index
finding were corrected and rechecked in source. Terra reports the latest
saved-list DB rerun passed **16/16** tests; earlier environment-setup failures
were not counted as successful runs.

Web review addressed session lifetime, stale cache removal, count scheduling,
window focus, row retries, dirty-draft preservation and inaccessible-list
gating. Browser findings B1–B3 were fixed and rechecked: completed counts no
longer remain stuck loading, API object-key order does not mark an untouched
list dirty, and saved counts/descriptions are labeled during a local preview.
B4's disabled-query loading labels were changed to paused states and covered
by an automated test.

Terra reports the Web checkpoint passed type checking, lint and **373 tests**
before the final corrections. The final focused set passed **66 tests**, type
checking and lint. Independent findings AW4–AW7 and nested filter-object
equality are corrected, including authority refresh after mutation failures.
AW4, AW5 and nonempty-filter reconciliation were also rechecked in the browser
as recorded above. Independent source re-review found no remaining actionable
defect and verified the added focus/age-cutoff and dirty-Duplicate navigation
regressions. It also accepted the measured performance evidence after checking
all four plans against rendered SQL and current source hashes. After the final
repository gates, its disposition was **READY / ACCEPTED against all 16
criteria**, with no unresolved actionable finding. The reviewer did not run
tests; automated results are Terra's runner evidence. Ordinary People URL
serialization remains unchanged; canonicalization is only used for
saved-definition comparisons.

The coordinator independently compared both `list_summaries` and
`filtered_summaries` function bytes with HEAD: unchanged. All **147 existing
SQLx metadata files** were also byte-identical to HEAD; **13 new files** were
added for this slice.

## Acceptance coverage

The independent Astra reviewer mapped all 16 criteria in
[specification §9](../specs/SLICE_011b.md#9-acceptance-criteria-and-evidence).
References below use these test files:

- **DB:** [db_saved_lists.rs](../../backend/crates/crm-api/tests/db_saved_lists.rs).
- **HTTP:** [saved_lists.rs](../../backend/crates/crm-api/tests/saved_lists.rs).
- **Workspace:** [PeopleView.test.ts](../../web/src/views/PeopleView.test.ts).
- **Counts:** [savedListCounts.test.ts](../../web/src/api/savedListCounts.test.ts).
- **Cache:** [savedLists.test.ts](../../web/src/api/savedLists.test.ts).
- **Index:** [ListsView.test.ts](../../web/src/views/ListsView.test.ts).
- **Dialog:** [SavedListDialog.test.ts](../../web/src/components/SavedListDialog.test.ts).

| Criterion | Test and observed evidence |
|---|---|
| 1. Migration/grants | DB `saved_list_schema_enforces_live_tombstone_shape_and_app_grants`; migration constraints, composite membership references and grants reviewed. |
| 2. Creation/replay | DB `create_replay_edit_delete_and_direct_version_validation`, `create_retry_identity_is_scoped_to_actor_and_organization`, concurrent and quota tests; browser empty criteria, symbolic Me and lost create response. |
| 3. Retry after edit/delete | DB create/edit/delete replay and HTTP deletion-authority coverage; Workspace terminal-token regression; browser lost create → delete → terminal Retry with no replacement. |
| 4. Privacy/isolation | DB `visibility_and_current_membership_permissions_do_not_leak_personal_lists`, metadata index and identical-404 route tests; browser member/admin private links and cross-Organization links in both directions. |
| 5. Quotas | DB `saved_list_scope_quotas_are_separate_and_deletion_frees_a_slot`, `concurrent_create_and_revision_races_preserve_one_winner`; Dialog personal-50/shared-200 explanations. |
| 6. Revisions/authorization races | DB concurrent revision/update-delete checks and `membership_for_share_lock_rechecks_role_and_status_after_a_privilege_race`; direct permission/deactivation coverage. |
| 7. Validation/precedence | DB strict HTTP/error precedence, direct unsupported version, valid oversized body and downstream 503 tests; HTTP authentication and malformed-path tests. |
| 8. Invalid stored filters | DB independent deep JSON, v1 unknown-field/bad-shape and stale/cross-org/inactive-reference cases; Workspace invalid states; browser unsupported definition and explicit stale-stage repair. |
| 9. Membership parity | DB `saved_count_uses_same_capped_semantics_and_resolves_me_per_viewer`, `saved_count_matches_people_for_every_filter_axis_mixed_and_exact_caps`; predicate/binding review; browser two-viewer Me and assignment changes. |
| 10. Create/copy flows | Workspace retry identity, late completion and mixed-Me tests; Dialog quotas; filter deep-equality tests; browser explicit scope, empty-filter Save as, independent copies and frozen/terminal retries. |
| 11. Edit/delete conflicts | Workspace repeated-409 baseline, authority loss, actual router and dirty copy-link Cancel/Discard guards; browser two-tab Save/Delete conflicts, Reload, uncertain PUT reconciliation and reader controls. |
| 12. Existing workspace | Existing FilterBar/PeopleView/PersonPreview suites plus named empty/missing/invalid/cached-switch tests passed in the full gate; browser URL/hash/named override and narrow inspector/Escape. |
| 13. Counts/cache/recovery | Counts warm-25 invalidation, retries, reconnect and session tests; Cache eviction/ignored-abort tests; Index actual focus; Workspace warm-cache relative-age focus and late-create actor/unmount cases; DB publisher remains empty. Browser four-request bound and independent row Retry. |
| 14. Telemetry/performance | DB `saved_list_spans_record_static_kinds_without_names_filters_or_retry_tokens`; [measured 50k dense/sparse plans](../design/perf/slice-011b-2026-09-06/README.md) and browser scheduling bound. |
| 15. Full checks | SQLx preparation, `./scripts/check`, subsequent `./scripts/check-db` and final diff check passed; existing tracked summary SQL/cache unchanged. |
| 16. Live walkthrough | The synthetic two-agent/admin/second-Organization observations above. Native confirm automation limitation is recorded; actual router tests verify Cancel/Discard. |

## Automated checks

Terra ran the implementation checks; the coordinator ran the independent
byte comparisons and document checks described above.

| Command | Result |
|---|---|
| `./scripts/sqlx-prepare` | Passed on the current SQL. Thirteen new cache files; all 147 existing files preserved. The final DB gate independently checked the cache against fresh migrations. |
| `./scripts/check` | Passed in 30 seconds: Rust formatting, clippy, production compilation, crate boundaries, 651 service-free tests and 5 doctests; Web lint, type checking, 388 tests and build; 9 email-worker tests. |
| `./scripts/check-db` | Passed after `./scripts/check`: schema/cache preparation check, Centrifugo health and 379/379 database tests, 139 seconds total. An earlier passing run is superseded for the required final order. |
| `git diff --check` | Passed for implementer and independent reviewer; coordinator also checked the final documentation tree. |
| Documentation/evidence checks | Coordinator verified local Markdown links, the exact changed-file inventory and existing query/cache byte preservation. Copied performance harness passed `bash -n`; reviewer verified all four rendered SQL statements and source hashes. |

## Cleanup and delivery boundary

The coordinator closed the isolated browser tab and stopped its fault proxy.
Terra stopped the QA harness, API on 3100 and Vite on 5174. The harness's
cleanup left its generated QA database behind; Terra verified it was no longer
referenced, dropped that exact guarded database, then verified it was gone.
The generated performance database was also dropped after measurement. Original
development API 3000, Vite 5173 and shared development data were preserved.

The verified implementation was committed as `2af023c` on
`codex/slice-011b-saved-lists`, from base `1635fc4`, then merged to local `main`
after the user's explicit approval. Main had not diverged from that base;
the merge required no conflict resolution. Delivery-status documentation was
updated in the merge commit. Backend/Web files remain identical to the tested
implementation commit, so the recorded full checks apply without rerunning
the unchanged implementation. The coordinator checked the final diff and
document links and confirmed a clean working tree after the merge.

No push or deployment was performed. The shared development API was not
restarted with this build.
