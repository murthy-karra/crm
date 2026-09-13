# Slice 010e2 — Execution brief

**APPROVED FOR IMPLEMENTATION — D-076, 2026-09-12.** Implements the accepted
[People-refresh specification](../specs/SLICE_010e2.md).
[Planning review](SLICE_010e2_REVIEW.md) owns review findings and readiness.
The current task implements this plan; it does not authorize live source/customer
operations, activation, Git publication or runtime deployment.

## Objective and bounded ownership

Provide previewed, confirmed updates to successfully imported existing People,
preserving local divergence, raw evidence, original parent/sibling plans and the
admin review hold. Only names, contacts, stage and assignee are in scope, through
existing confirmed mappings. Do not broaden into new People or other data families.
Mobile design cleanup and physical-phone testing are user-deferred; native
feature development continues through the separately proposed Mobile 002.
Follow the [coordinated launch](../plans/MOBILE_002_010e2_PARALLEL_LAUNCH.md)
to keep iOS, Android and migration running within three worktrees.

After approval, use one short-lived migration worktree and one primary writer
for both backend and Web. This replaces the original standalone two-lane plan:

| Lane | Branch / files | Required output |
|---|---|---|
| Migration backend and schema | `codex/migration-010e2`; new `crm-app` migration `people_refresh*` modules, routes, additive migration, focused DB/API tests | Frozen DTO/persistence/permit/locking contract, executor and recovery proof |
| Migration Web, same writer/worktree | Same branch; new Web API module, preview/progress/result component and tests | Admin preview/confirmation/recovery using the frozen backend DTOs; desktop/390px evidence |

The coordinator owns shared specs/status/decision records, module/router/worker
registration, workspace guards, release inventory/preflight, root scripts,
`.sqlx`, navigation integration and final integration. Backend proposes precise
shared guard/schema patches for serialized integration; only the backend lane
creates database migrations. Allocate a unique timestamp before writing one.
No concurrent edits to shared files. The other worktrees first supply the mobile
foundation, then the iOS and Android implementations; no fourth worktree opens.

## Execution sequence

1. Read AGENTS and the spec's decisions/contracts; inspect actual original
   manifest/provenance/contact mappings and guard/read/revision triggers. Freeze
   exact DTOs, engine semantics, scalar/contact diff paging, private permit,
   baseline identity, request receipts, monotonic boundary, locks, byte accounting
   and capability inventory in an owned contract checkpoint before implementation.
2. Implement backend then Web under the same writer, using the frozen DTO
   checkpoint and exact fixtures. Reuse existing migration UI and command/error
   patterns. Source content is escaped data, never trusted control. Mobile backend
   and later both native writers can proceed independently in their owned files.
3. Implement retained qualification and immutable preview first, then per-Person
   atomic updates with exact revalidation, baseline/ownership and receipt commit.
   The executor cannot reopen an old parent import, call FUB, manufacture a new
   source identity or bypass the workspace guard for ordinary callers.
4. Integrate the new capability/read/guard seams sequentially. Verify the known
   risk areas: all-report-row traversal, source reversion, removed/replaced contact
   identity, local edits, explicit clears versus incomplete evidence, and frozen
   previews raced by another authorized writer or membership change.
5. Exercise real synthetic API plus production Web, exact native/provenance
   reconciliation and process handoff/cancel/storage recovery. Record failed
   attempts honestly; no test duplicates business effects in shared development.
6. Run one independent bounded review and correct actionable findings. Complete
   required final gates once on the integrated source. A second review is scoped
   to concrete fixes; no third broad audit under D-050 without user direction.

## Resources and required checks

Use isolated synthetic database(s), API/Web ports and test identities. Discover
available ports before assigning them. Preserve the shared API3000/Web5173 and
native-demo API3101; never run destructive bootstrap or use shared tenant data.
Set separate `CARGO_TARGET_DIR` and explicit Vite output directories for all
verification. Worktrees do not isolate services. Serialize SQLx preparation and
`scripts/check-db` on this host; do not run overlapping global DB gates.

Required: spec §8 acceptance cases, formatting/Clippy/type checks, meaningful
interpreter/DB/API/Web and workspace/tenant tests, one D-050 representative plan
and paired-read pass, live SQLx preparation when SQL changes, `scripts/check`
and `scripts/check-db` on the final integrated tree. Attribute reusable evidence
and run affected checks again only for new failures or changes.

Completion records source/check commands, fixture scope, exact applied/held/
excluded totals, unchanged parent/sibling state, screenshots actually inspected,
failure/recovery evidence and unresolved limitations. No native physical testing,
live FUB qualification, restore exercise, distribution or deployment is implied.

## Following work

The next proposal after this rung should define new-Person admission and extending
dependent metadata/activity/history imports to those People. Later proposals own
new mappings/repair, per-family updates and deletion policy, remaining source
families, final reconciliation and activation. Keep each visible; do not report
this existing-People refresh as complete migration fidelity.
