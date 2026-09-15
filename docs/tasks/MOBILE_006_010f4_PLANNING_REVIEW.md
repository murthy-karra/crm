# Mobile006 / 010f4 — Author findings and review handoff

**INDEPENDENT ROUND 1 READY — 2026-09-14.** The user requested
“ok start planning” and selected “Offline tags and custom fields (Recommended)”.
D-083 records that authority; no proposed implementation contract is accepted.
Author inspection used main `a5cb24d` and the completed Mobile005/010f3 sources.

## Concrete evidence and planning consequences

| Inspected implementation | Consequence |
|---|---|
| [Mobile operations](../../backend/crates/crm-app/src/domain/mobile/operations.rs) checks receipts then acquires its common Person lock; commands are individually dispatched | New metadata admission must precede that lock; compose typed helpers and one atomic receipt, preserving exact old dispatch/digest bytes |
| [Generation summary](../../backend/crates/crm-app/src/domain/mobile/generations.rs) contains profile/stage/assignment and paged contacts, not tags/field values; stage catalog is independently versioned | Add explicit metadata representation/capabilities and shared catalog pages, with same-revision upgrade and old-client seal compatibility |
| [Tag commands](../../backend/crates/crm-app/src/domain/tag/commands.rs) and [field commands](../../backend/crates/crm-app/src/domain/custom_field/commands.rs) open their own transactions and lock Person/catalog rows | Extract narrow transaction-compatible typed helpers; inventory all catalog/cascade/import paths before choosing a composable barrier order |
| [Field validators](../../backend/crates/crm-app/src/domain/custom_field/model.rs) use exact number strings, bounded calendar dates and text; clear permits archived definitions | Retain native rules in both clients; explicit clear remains available for archived stored values; no float/timezone coercion |
| [Original activity worker](../../backend/crates/crm-app/src/domain/migration/activity_worker.rs) resolves original People/results and consults a globally scoped activity identity | Add an admission resolver and exclusive admitted-owner tuple to the existing registry; avoid a duplicate claim registry or invented original rows |
| [Activity source interpreter](../../backend/crates/crm-app/src/domain/migration/activity_source.rs), HTML/time helpers and [010f2](../specs/SLICE_010f2.md) preserve exact source/role/time rules | Reuse conversion/validation while separately qualifying a newer source, cohort, mappings and first-coverage lifetime |
| [Activity review](../../backend/crates/crm-app/src/domain/migration/activity_review.rs) already resolves successful admitted People but provenance joins target original activity identities | Extend precise owner/provenance joins and durable confirmed-boundary checks, avoiding duplicated native rows or legacy unbounded reads |
| [Prior release record](MOBILE_005_010f3_RELEASE.md) says IN PROGRESS despite older project-summary wording | Preserve actual release ownership/authority; planning does not claim deployment completion or start another rollout |

## Author dispositions

- Mobile uses existing catalog IDs only, one atomic patch and conservative aggregate
  Person metadata/catalog revisions. These are explicit proposals; offline catalog
  creation and per-field merge policies are excluded from this bounded slice.
- Archived fields are not hidden or silently dropped; explicit clearing preserves
  the existing command's semantics. No repeated target IDs or replace-all payload.
- Mobile metadata representation is opt-in; old cached summary absence is never an
  empty editable baseline. Catalog-only changes invalidate metadata completeness.
- A dedicated catalog revision read model avoids adding an Organization row-lock
  requirement to every ordinary metadata edit. All-writer lock inventory and exact
  barrier placement remain a required concrete checkpoint before implementation.
- The activity identity registry is already global. Extending its owner shape is
  smaller than copying the metadata slice's shared-claim handover, and preserves
  original immutable evidence. Legacy worker/reader readiness still needs proof.
- Activity roots cover terminal successful admissions, including partial cancelled
  results, from one qualified snapshot. Metadata completion is not an eligibility
  dependency; settled activity holds are not later remainder/repair candidates.
- Existing bounded review revisions sum original activity children only. The draft
  now explicitly includes admitted commit revisions and tests insertion between
  pages, so new activity cannot leave an old cursor apparently current.
- Planning describes the established three-worktree implementation sequence but
  launches no writers or reviewers. Required independent review is still pending.

## Independent review scope

Review both complete specifications, briefs and coordinated plan against the actual
code and D-050. Focus on the metadata catalog/value/old-writer lock graph and
generation completeness; original/admitted activity identity FKs and equality;
source completeness/timezone/content restrictions; zero-write reader boundaries;
atomic receipt/remainder/byte accounting; and disjoint file/database ownership.
Do not reopen unrelated product policy or repeat historical completed reviews.

Return stable findings with severity, location, failure scenario, evidence and
smallest correction. Independent review has not returned READY. After findings
are resolved, request acceptance of the concrete new contracts under AGENTS §11
before implementation. The user's scope selection is not that acceptance.

## Validation of this planning change

`git diff --check` passed. A Python check over all ten changed/new Markdown files
verified 248 local link targets, 98 unique indexed decision/open-item headings and
19 consecutively numbered acceptance criteria. New-file whitespace and final
links are rechecked before handoff. No application, database, native, browser or
performance tests have been run for these Markdown changes. Future acceptance
matrices are requirements, not claimed evidence.

## Independent planning review — round 1

Both independent reviewers inspected main `a5cb24d`, the complete respective
specifications/briefs, coordinated plan and relevant implementation. Both returned
**READY with zero blocking findings**. Reviewers were read-only; no test execution
or implementation verification was claimed. D-084 already authorizes implementation.

- Mobile reviewer `mobile006_plan_review`: lock graph, all-writer revisions, catalog
  representation completeness, atomic typed command reuse and tenant/privacy scope
  are adequately specified. Concrete checkpoint: validate final tag capacity, then
  apply removals before additions; retire overlays only with a covering metadata-
  qualified sealed representation. These implement existing requirements.
- Migration reviewer `activity_plan_review`: identity/source/lifetime/remainder and
  reader/recovery boundaries are adequately specified. Concrete checkpoint includes
  `crm_activity_measure_row`, which currently derives its ledger owner through
  original `import_id`; extend measurement for admitted ownership while retaining
  original charges and per-row accounting. This implements the existing counted-
  column requirement.

Author inspection and earlier pending-review wording above describe the planning
pass before these independent results. There is no repeat scope approval pending;
independent implementation reviews and required final checks remain.
