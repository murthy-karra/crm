# Mobile 005 / 010f3 — Planning review

**Independent review — READY, 2026-09-14, two rounds complete.** The user authorized independent
review under the D-081 follow-up. Reviewer `/root/mobile005_010f3_review` inspected
both complete specifications, briefs, coordinated plan, applicable decisions and
affected backend/schema/native code against published base
`e36c9b30bb7de6fcd73d20f8e32a7cd6fd42be18`. The reviewer was separate from the
author and did not edit files or execute runtime tests.

Round 1 verdict: **READY-WITH-FIXES** — two blockers and one nonblocking checkpoint
correction. The author applied all three; bounded round 2 returned **READY** for
Mobile005, 010f3 and the coordinated plan. All three findings are closed, with no
blocking or nonblocking review issues remaining. No implementation approval follows
from review. Both permitted planning review/fix rounds are complete; no third round
was performed or authorized.

| ID | Classification | Failure scenario / evidence | Correction and current disposition |
|---|---|---|---|
| F3-01 | P1 / TRUST / blocking | Original metadata worker writes identities directly after native creation (`metadata_worker.rs`, `write_identity`/`catalog_result`); backfill followed by capability activation at first admitted confirmation left an old-worker write interval | 010f3 §4/§6 now require an atomic readiness handover before admitted preview: fence old units before enumeration, serialize catch-up/claims/accounting/capability activation, reject late old commits, roll back partial charges. Added exact interval/rollback/rerun/cancel test requirements. Closed by reviewer in round 2 |
| M5-01 | P2 / CONTRACT / blocking | Rust `PersonChange` and mobile publication dispatch contain no details-edit variant; promising an existing event leaves an undeclared realtime contract change | Mobile005 declares `details_changed` in Slice003's v1 IDs-only envelope, Web type/Person/People/Today/list-count invalidation and broad fallback, no-op/replay/publish-failure test requirements; backend/coordinator ownership explicit. Closed by reviewer in round 2 |
| M5-02 | P2 / BOUNDARY / nonblocking | `capture::link_unmatched(add_contact_method)` locks Person then inserts contacts; mobile adapter already locks Person before dispatch, so an intake lock added only inside the new command would invert the intended order | Named capture in writer/trigger/tests; explicitly place new profile intake admission before the adapter's common Person lock. Closed by reviewer in round 2 |

No additional human decision was required for these corrections. The profile
input/add-remove/conflict policies and admitted metadata lifetime/source/global
catalog contracts still need their planned scope/implementation acceptance.

Review input fingerprints cover these five documents, in order: Mobile005 spec,
010f3 spec, Mobile005 brief, 010f3 brief, paired plan. SHA-256 input is UTF-8 path,
NUL, exact file bytes, NUL for each document:

- Round 1: `bcce8629955f0cb1e30bb19d0b1a3ce477a47422456a750eb420977a5d7ce36a`.
- Round 2: `1e29083be74d63b2a405fff57d9dcc9895180d0f699bac85f341febe57e89551`.

The reviewer verified the round-2 aggregate and found the handover consistent with
the existing original worker's Organization locking: earlier commits enter catch-up,
later incompatible commits fail, and claims cannot be trusted before readiness.
After that verdict, only readiness wording in the paired plan and review/status/
decision records was updated; no reviewed product or contract behavior changed.

Final documentation validation passed: `git diff --check`, all 226 local links
including explicit heading anchors, all 96 decision headings indexed without
duplicates, and ten Markdown-only changed/new files. No runtime tests,
schema application, services, native installation or implementation verification
were performed. Concrete schema/guard/lock fixtures and implementation checks
remain future execution requirements, not unresolved planning findings.

## Original author review — 2026-09-13

**2026-09-13 — author review, not independent review.** Inspected published
`e36c9b3` and the proposed documentation in this pass. No application code, schema,
service, app store or data changed; no runtime tests or independent READY claimed.

## Evidence and resulting design

| Existing source | Planning consequence |
|---|---|
| [People routes](../../backend/crates/crm-api/src/routes/people.rs) expose reads, assignment/stage/contact-attempt/tag/value actions; [command modules](../../backend/crates/crm-app/src/domain/commands/mod.rs) have no name/contact editor | Mobile005 must add a shared typed command, not claim to wrap an existing editor or add a client-only write path |
| [Contact normalizers/lookup](../../backend/crates/crm-app/src/domain/contact.rs) implement current email/phone rules and earliest-Person lookup; [intake](../../backend/crates/crm-app/src/domain/commands/receive_inquiry.rs) holds an Org intake lock | Preserve normalization/household semantics; serialize contact changes with intake before Person locks |
| [Person/contact schema](../../backend/crates/crm-api/migrations/20260821000002_person_contact_method.sql) has stable contact UUIDs and per-Person normalized uniqueness; [admitted refresh](../../backend/crates/crm-app/src/domain/migration/admitted_people_refresh_worker.rs) writes names and owned contacts | Explicit contact operations and all-writer profile revisions; stable UUIDs on edit, no replace-all, no migration baseline adoption |
| [Mobile generation code](../../backend/crates/crm-app/src/domain/mobile/generations.rs) pages contact UUID/kind/value under summary, ordered by UUID; display primary lookup uses import order/creation time/UUID | Add ordering fields and details revision; preserve paging and require complete sealed summary traversal before editing |
| [Mobile adapter](../../backend/crates/crm-app/src/domain/mobile/operations.rs) has explicit operation/resource dispatch; [bounds/bootstrap](../../backend/crates/crm-app/src/domain/mobile/mod.rs) use 128-KiB operations, 100-row/512-KiB components | Extend receipt/visibility/digest handling deliberately; retain old serialization and download/access budgets |
| [iOS store](../../ios/FieldCRM/LocalStore.swift) and [Android store](../../android/src/main/java/org/crm/field/FieldStore.kt) preserve stage drafts, immutable operations and typed receipt checks | Add profile drafts/overlays explicitly and prove upgrade of populated Mobile004 stores, not reinstall success |
| [Metadata worker](../../backend/crates/crm-app/src/domain/migration/metadata_worker.rs) resolves People through original import results and writes globally scoped catalog identities with original owner FKs | New admission resolver/root, plus additive shared claims with original-writer integration; no weakening old FKs or independent duplicate global registry |
| [010f1](../specs/SLICE_010f1.md) owns exact metadata rules, original snapshot and one-child lifetime | Reuse value/capacity rules but declare the new source/cohort/lifetime contracts; source qualification must independently prove custom-field completeness |
| [010e4](../specs/SLICE_010e4.md) separates successful terminal admission cohorts and exact-boundary remainder | Include committed cancelled admissions; first metadata coverage remains separate from repair or ongoing synchronization |

## Author review dispositions

- Corrected initial mobile wording to match the existing summary-section contact
  pages; new order fields and representation qualification are explicit contracts.
- No ordinary profile editor exists. Added proposed input bounds, last-identifier
  protection, explicit contact add/edit/remove and one aggregate revision rather
  than assuming existing user-facing validation or per-field merge policy.
- Catalog identity is global but current ownership FKs are original-only. Added
  shared claim backfill and original-writer compatibility; new child FKs alone
  would not prevent conflicting cross-import target choices.
- Kept source qualification independent of 010e1 result labels and selected one
  exact capture for People and definitions. 010f1's 4-MiB reader is not silently
  widened to another feature's larger raw limit.
- Scoped cancelled remainder to never-settled work on the same accepted boundary;
  this does not authorize another source, held-item repair or family updates.
- Sequenced shared backend integration before native writers, preserved a maximum
  of three worktrees, and assigned schema/shared-file ownership and isolated gates.

## Original review status and remaining checkpoints

No contradiction with accepted decisions was identified in this author pass.
New input/conflict/lifetime/catalog contracts are proposals, not accepted decisions.
The two complete specs and briefs are ready for scope review. Required independent
review remains before implementation; provide it these documents and the evidence
above, with D-050's two-round limit. No current agent delegation or implementation
launch is implied by a future parallel schedule.

After scope acceptance, owning implementation checkpoints must freeze concrete
DTOs, schema/FKs, global-claim source-key equality/erasability/accounting, all-writer
lock order and trigger/grant compatibility. A change to policy, authority, source
fidelity or cancellation semantics returns to the spec. Tests listed are future
acceptance requirements, not executed results.

Documentation checks executed for this pass: `git diff --check` passed; all 224
local Markdown link targets across the ten changed/new documents exist; all 96
decision/open-item headings are unique and present in the decision index. Only
Markdown files changed. Runtime/backend/native tests were not run for this
documentation-only change.
