# Mobile 005 / migration 010f3 — Coordinated plan

**APPROVED FOR IMPLEMENTATION — 2026-09-14, D-082.** Published baseline `e36c9b3`
plus accepted planning documents supplies the implementation base. Mobile004/010e4
release stays preserved. [Planning review](../tasks/MOBILE_005_010f3_PLANNING_REVIEW.md),
[implementation status](../tasks/MOBILE_005_010f3_IMPLEMENTATION_STATUS.md).

## Outcomes

| Track | Draft specification / brief | Exit |
|---|---|---|
| Mobile shared backend | [Mobile005](../specs/MOBILE_005_OFFLINE_PERSON_DETAILS.md), [A](../tasks/MOBILE_005_IMPL.md#a--shared-backend-and-frozen-contract) | Typed profile command, all-writer revision, complete editable baseline and durable receipts |
| iOS | [B](../tasks/MOBILE_005_IMPL.md#b--ios) | Offline names/contact edits, explicit conflicts, installed-store preservation |
| Android | [C](../tasks/MOBILE_005_IMPL.md#c--android) | Same behavior against frozen shared contract |
| Migration backend then Web | [010f3](../specs/SLICE_010f3.md), [brief](../tasks/SLICE_010f3_IMPL.md) | Tags/custom fields for one admission cohort, safe global catalog reuse and exact remainder |

Use existing Terra high preference for substantive implementation. Apply
[model routing](../prompts/MODEL_ROUTING.md) to bounded design/review work; model
labels do not authorize agents or switch the current runner. Required independent
review is separate from writer self-check. Planning creates no runtime resources.

## Dependency schedule and ownership

| Stage after scope/contract acceptance | Worktree 1 | Worktree 2 | Worktree 3 |
|---|---|---|---|
| Shared foundation | Mobile backend | 010f3 backend then Web | Unused |
| Mobile contract verified/integrated; its tree closed | iOS | 010f3 continues if needed | Android |
| Combined acceptance | Close completed lanes; coordinator integrates and runs final gates | No new scope to fill a slot | At most three implementation trees |

Native implementation need not wait for migration. Backend wire/schema/lock
fixtures must be frozen before either native lane consumes them; migration wire
fixtures precede Web implementation. Root coordinates shared integration, not a
fourth independent code writer. Reassign ownership explicitly before shared fixes.

| Proposed branch | Exclusive primary ownership | Coordinator integration |
|---|---|---|
| `codex/mobile-005-backend` | New profile command, contact validation, mobile reads/operations, Rust realtime variant, Mobile005 migrations and fixtures | All-writer edits outside owned modules (including capture), shared guard/grants/router, Web realtime types/invalidation fixtures, `.sqlx` |
| `codex/mobile-005-ios` | `ios/`, platform tests and assigned evidence | Shared backend/contracts/docs |
| `codex/mobile-005-android` | `android/`, platform tests and assigned evidence | Shared backend/contracts/docs |
| `codex/migration-010f3` | New admitted metadata domain, original metadata claim integration, 010f3 schema, tests and new Web modules | Shared guards/worker/router/preflight, navigation, `.sqlx` |

Only mobile backend creates its revision/receipt migrations; only migration
creates metadata/claim migrations. Allocate disjoint actual paths/versions at
launch; mobile integrates first. Applied files are immutable. Coordinator owns
decisions/specs/status, root manifests/scripts/locks, module registration and SQLx
cache regeneration. Serialize patches to any file discovered to be shared.

## Cross-slice risks and integration tests

- Profile revision covers names/contact identity/value/order across existing
  intake and migration writers. Metadata changes must not advance it, although
  legitimate broad mobile invalidations remain. Neither feature changes stage
  revision semantics or original source/core-refresh baselines.
- Contact edits share intake's Organization lock before Person/contact locks;
  the new adapter admission precedes its existing common Person lock. Capture's
  optional contact insertion belongs in revision/lock compatibility tests.
  Catalog claims share metadata namespace ordering before claim/target/Person
  locks. Freeze one graph including workspace/lease/receipt/ledger and triggers;
  investigate inverse paths before parallel launch, not during final gate failures.
- Mobile operational permissions and metadata's review-only private permits stay
  distinct. Test HTTP outer denials and direct typed paths, including new derived
  revision trigger updates under existing original/admitted permits.
- Shared metadata claims require original-writer integration, bounded backfill,
  collision handling and an atomic readiness handover before admitted preview.
  Fence old units before enumeration and serialize catch-up/accounting/activation
  with original catalog commits; enforce rejection for already-running old workers
  as well as new launches. Failed handover leaves no partial claim/owner charge.
  Preserve old original
  child lifecycle/FKs/results; do not pass compatibility merely by naming a flag.
- Existing mobile generations/outbox/receipts and populated stores survive upgrade.
  Contact ordering fields are additive; old UUID pagination and legacy payloads
  stay stable apart from explicitly declared fields. New editing requires a
  qualified complete representation, not a primary-contact display summary.
- `details_changed` explicitly extends the Slice003 realtime vocabulary. Mobile
  backend owns Rust publication; coordinator integrates Web type/invalidation
  fixtures before the native handoff. Preserve the IDs-only v1 envelope and broad
  old-client fallback; no duplicate/no-op publication or post-commit failure result.

## Verification and resource discipline

Use separate synthetic operational mobile and migration-review databases, plus
foreign Org/actor fixtures. Inventory actual listeners at launch; preserve shared
API3000/Web5173, demo API3101, retained QA stores/databases and recovery artifacts.
Do not preassign an occupied port or overwrite a demo app identity. Isolate
`CARGO_TARGET_DIR`, Web output, Xcode derived data and Gradle outputs.

Each lane records focused checks and actual API/browser/simulator/emulator flows
specified in its brief. Coordinator runs `scripts/check`, `scripts/sqlx-prepare`
and `scripts/check-db` sequentially on integrated source, plus cross-slice tests
and relevant D-050 paired/hot-query gates. Attribute reusable evidence to runner,
revision and fixture; do not repeat unaffected gates without a reason. Keep full
logs in evidence files and summarize failures/counts. At most two review/fix rounds
per slice; unresolved trust or required-check gaps cannot be marked passed.

## Review, acceptance and following work

Independent review returned **READY on 2026-09-14** for both specs/briefs and this
plan after two rounds; all findings are closed in the linked review record.
Implementation/test success is not claimed. D-082 now supplies
scope/contract acceptance under AGENTS §11. Concrete compatible schema/DTO/guard/
lock detail then belongs to the assigned owners.

After verification, publication/cleanup/shared-development deployment have their
own release scope. Notes/tasks and history follow 010f3 as smaller family steps.
Native calling follows the agreed progression; neither this pair nor the ladder
resumes broad redesign, physical phones/cellular, live FUB/customer processing,
activation or distribution. Customer erasure/restore prerequisites remain open.
