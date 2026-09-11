# Slice 010c — Web execution boundary

This refines file ownership within D-065 and [the implementation brief](SLICE_010c_IMPL.md).
Begin implementation only after the first backend review/fix confirmation.
The [010c spec](../specs/SLICE_010c.md) and
[concrete backend contract](../specs/SLICE_010c_CONTRACT.md) remain authoritative.
Use [the accepted UI style](../design/UI_STYLE.md) and existing controls/query
patterns. Responsive Web here does not implement native clients.

## Ownership

The Web support writer owns only these new files and the adjacent API/main-panel,
mapping-panel and record-panel tests:

- `web/src/api/imports.ts`, `imports.test.ts`;
- `web/src/components/migration/PeopleImportPanel.vue`;
- `ImportMappingPanel.vue`, `ImportRecordPanel.vue`, `ImportFieldViewer.vue` and
  `PersonImportProvenance.vue` in that same directory.

The coordinator owns the new `ImportFieldViewer.test.ts` and
`PersonImportProvenance.test.ts` after the support writer's explicit handoff;
the support writer retains both production components. These two test-only files
verify source escaping/segments and stale identity/resource response rejection.

The primary retains every existing file, shared session/workspace lifecycle,
API types/client/query integration, router/shell, realtime/call/Operator ownership,
People detail/preview/history, MigrationView mounting, and necessary new workspace
waiting view/helper/tests. Primary retains backend/schema/generated metadata.
Root owns docs and private browser QA. No shared-file edits by the support writer.

## Component boundary

| Component | Inputs and responsibility |
|---|---|
| PeopleImportPanel | `refreshWorkspace: () => Promise<void>`; loads paged retained snapshots/previews and existing imports, owns choices/confirmation/run recovery, calls the parent callback after a confirmed receipt before further rendering |
| ImportMappingPanel | `importId`, `planId`, `revision`, `canReplan`; emits typed `apply` with stage/assignee patches totaling at most 50; parent owns request identity and expected revision |
| ImportRecordPanel | `importId`, `planId`, `mode: 'plan' \| 'results'`; bounded records/results and held reasons, scope changes discard pending reads |
| ImportFieldViewer | `request` as record-scope, mapping-scope or provenance-scope plus `title`; one bounded segment and cursor navigation, escaped text, no unbounded concatenation/download |
| PersonImportProvenance | `personId`; current-admin query only, escaped imported source metadata and full-field inspection, mounted by primary in Person detail |

The parent workspace callback immediately fences private UI/requests and verifies
`/me` through the shared session lifecycle. A confirmation receipt's mode fields
do not become client session authority. Primary adds required
`MeOrganization.workspace_mode` and string `workspace_revision` to the shared
type. The panel is independent of source credentials and CoreSnapshotPanel;
it must not trigger capture or assessment automatically.

Mapping field inspection uses the additive mapping-scoped endpoint in the
specification/contract, never a mapping UUID in a record-field URL. Shared API
401/403/workspace error handling belongs to the primary session coordinator.
Components discard stale scoped responses and clear their own reads on access
loss; the parent coalesces authoritative workspace refreshes. Do not issue
competing raw `/me` requests from child components.

Existing SnapshotBudgetPanel may be reused with a fetched CoreSnapshot. Its
refresh event updates allowance/status only; budget changes never retry import
work. Do not edit that shared component in the support lane.

## Scope, state and verification

Support owns local `importQueryKeys` with prefix
`['org', orgId, 'people-imports', actorId, sessionLifetime, workspaceRevision]`.
Every async read/mutation compares that scope and selected run/plan generation.
Reads receive AbortSignal; use `retry: false`. Poll only queued/running/building
work, backing failures from two seconds to thirty seconds; stop on ready, paused
or terminal state. Superseded responses cannot restore old choices, details or
People after a role/mode/session transition. Never retain credential input.

Keep source IDs, counts, bytes and revisions exact strings; use BigInt or existing
safe formatters. Show partial coverage, held reasons, the immutable plan revision,
required reservation and current allowance/ceiling clearly. Exact held-count,
review-only and remaining-data acknowledgments precede explicit confirmation.
Unknown network outcomes preserve the canonical request UUID/input for replay.
Cancellation explains that committed rows and the review hold remain. No activation,
repair, general stage CRUD, source writes or import Operator tools are added.

Use Card, FormField, ConfirmDialog, the existing unstyled PrimeVue conventions,
controls.ts and theme tokens. Source text is untrusted and escaped; source URLs
are not automatically fetched or made into operational actions. Long fields and
tables scroll within their containers at 390px. Keep the approved compact layout.

Focused tests follow CoreSnapshotPanel's real QueryClient/PrimeVue harness with
API fetch mocking. Cover current server DTOs, partial choice inheritance, explicit
stage creation/unassigned choices, stale actor/mode/plan responses, uncertain
request replay, required acknowledgments, paused/retry/cancel behavior and terminal
polling stops. Primary tests the shared waiting/admin read-only shell, navigation,
cache fencing and Operator/call cleanup. Root then verifies the combined UI
against the production build and real local synthetic API. Full final gates and
the second bounded review remain required before completion.
