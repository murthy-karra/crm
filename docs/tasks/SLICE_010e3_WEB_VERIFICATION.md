# Slice 010e3 Web verification

Branch: `codex/migration-010e3-web`. The Web admission surface uses the isolated
Web build output in this worktree. It does not start a shared development server
or exercise the reserved API3103/Web5174 runtime; root owns that browser run.

## Implemented review flow

`PeopleAdmissionPanel` uses only relative `/migrations/fub/people-admissions`
paths, which resolve beneath the client API base exactly once. It loads completed
original imports and sealed core-change reports with bounded cursor paging, starts
an admission preview with a stable request body, polls active preparation and
settlement, pages disposition-filtered items, contact rows, UTF-8 field fragments
and fixed result traversals. It keeps source IDs, counts and revisions as decimal
strings through POST bodies, including the frozen confirmation envelope.

The panel requires acknowledgements for core-only coverage, inherited mappings,
distinct People for shared contacts, and the continuing review hold. Expiry enables
re-preview; retry/cancel carry their current string lifecycle revision. Ambiguous
writes retain their exact serialized body and request ID for replay. Query keys and
late reads are fenced by the existing admin/session/workspace access scope and are
removed on access changes or unmount.

`PersonAdmissionProvenance` provides an admin-only provenance card with bounded
contacts and complete UTF-8 field fragments. It renders server strings as Vue text,
without HTML injection. It preserves the existing original-import provenance 404
fallback because `PersonImportProvenance` only mounts this card after its original
route returns 404.

## Focused checks run

```sh
cd web
pnpm typecheck
pnpm test -- peopleAdmissions.test.ts
pnpm lint
pnpm build
```

Results on 2026-09-13:

- `vue-tsc`: passed.
- Vitest: 89 files / 1205 tests passed, including the new admission API contract
  checks for relative paths, cursor encoding, and unsafe decimal-string retention.
- ESLint: passed with zero warnings.
- Vite production build: passed.

The Vite build reported the repository's existing large-chunk advisory; it did not
fail the build. Actual API3103/Web5174 desktop and 390px acceptance remains for
root after the backend runtime is ready.

## Review round 1 component behavior

`PeopleAdmissionPanel.test.ts` now exercises the component, rather than only API
path construction. It verifies a dropped confirmation response replays the exact
serialized request body, authority re-verification preserves that uncertain
request while an identity change purges it, a late prepare receipt cannot apply
after selection changes, plan replacement clears all acknowledgements, expiry is
reactive, and opaque item cursors retain their endpoint-bound value on the next
page. The final focused run reported 90 files / 1210 tests passing.

## Final UI review additions

The completed-report selector now exposes **More completed reports** and carries
the returned opaque `reportCursor` to the next bounded request. Admission detail
now renders the server-provided original/newer snapshot IDs, decimal sequences,
started/completed boundaries, exact covered/deferred families, and the review
hold. Item detail renders every published provenance field summary and its UTF-8
byte count; every truncated published field has a complete-fragment control, so
retained properties such as `sourceUrl` are reviewable before confirmation.
Categorical held reasons are displayed as returned, without inferring missing
source fields.

The final focused component run was:

```sh
cd web
pnpm test -- PeopleAdmissionPanel.test.ts
```

It passed: 90 test files / 1212 tests. This includes opaque completed-report
cursor paging and an arbitrary published `sourceUrl` full-field request. The
Web DTO types match backend commit `2434584`; no backend files were changed in
this worktree.

## Browser-review template correction

A live browser review confirmed the item template must show the sealed intended
CRM projection independently from raw retained source provenance. The panel now
shows normalized first and last name, frozen target stage, and target assignee
with **Unassigned** explicitly rendered for a null assignee. Retained provenance
field summaries remain below that separate block. Reload only refetches selected
and enabled resources; preparation no longer triggers a settled-results request.

Focused component coverage includes these intended-value and active-preparation
reload paths:

```sh
cd web
pnpm exec vitest run src/components/migration/PeopleAdmissionPanel.test.ts --reporter=verbose
pnpm typecheck
pnpm lint
pnpm build
```

Results: the isolated component file passed 9/9 tests; typecheck and ESLint
passed; Vite production build passed. The build emitted the repository's existing
large-chunk advisory only. The live fixture/browser continuation is owned by root.


## Coordinator final browser and Web closeout

The actual browser workflow, including full source/provenance traversal,
cancellation/remainder, live settlement status, original/new Person profiles,
desktop and 390px layout and logout, passed as recorded in
[the migration verification](SLICE_010e3_VERIFICATION.md#actual-browser-acceptance).
The final full Web test run after the live-progress correction passed 90 files /
1215 tests (`pnpm exec vitest run`, 2026-09-13). The changed component passed 10/10
focused tests; typecheck, lint and production build also passed. A prior broad
run had an unrelated PersonDetailView timing failure; the final full run passed.
API3103/Web5174 were stopped and this short-lived Web worktree was closed after
its changes were integrated into the milestone.
