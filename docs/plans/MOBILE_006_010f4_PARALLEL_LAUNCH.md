# Mobile 006 / migration 010f4 — Coordinated implementation plan

**ACCEPTED — D-084, 2026-09-14.** The user approved implementation and isolated
synthetic verification. Both independent planning reviews returned READY. Current
implementation and acceptance evidence are recorded in
[implementation status](../tasks/MOBILE_006_010f4_IMPLEMENTATION_STATUS.md).
Publication/deployment and materially different policy remain separate.

D-083 records the initial planning choice; D-084 accepts the numerical assignment,
contracts and coordinated implementation. Retain the declared ownership and
verification limits below.

## Outcomes and current baseline

| Track | Specification / brief | User-visible outcome |
|---|---|---|
| Mobile backend, then iOS and Android | [Mobile006](../specs/MOBILE_006_OFFLINE_METADATA.md), [briefs](../tasks/MOBILE_006_IMPL.md) | Durable offline edits to existing tags/field values, with explicit metadata/catalog conflicts |
| Migration backend, then Web | [010f4](../specs/SLICE_010f4.md), [brief](../tasks/SLICE_010f4_IMPL.md) | Qualified first notes/tasks coverage for successful admitted People, with exact results/remainder |

Inspected source is `a5cb24d` on main, including verified Mobile005/010f3 code
`ab4a362`. The [prior release](../tasks/MOBILE_005_010f3_RELEASE.md) is separately
authorized and still recorded IN PROGRESS at planning inspection. Do not infer
runtime identity from main or this plan. Recheck the release record and actual
source/owned resources before an implementation launch; do not run a second release
or refresh shared services as a planning step.

## Execution schedule after review and implementation approval

Use Terra high for substantive writers under [model routing](../prompts/MODEL_ROUTING.md).
Sol high is the normal independent design-review profile; Astra high is appropriate
for a named difficult lock/identity finding. Model labels do not switch a runner or
authorize agents. Planning author review is not independent review.

| Stage | Worktree 1 | Worktree 2 | Worktree 3 |
|---|---|---|---|
| Shared foundation | Mobile backend | 010f4 backend then Web | Unused |
| Mobile foundation verified and integrated; backend worktree closed | iOS | 010f4 continues if needed | Android |
| Combined acceptance | Close completed writer lanes; coordinator integrates and runs final gates | No extra scope to fill slots | No fourth writer |

Native work does not wait for migration once the shared mobile contract is frozen.
Migration Web does wait for its own DTO fixtures. Keep one writer/branch/brief per
worktree, at most three. The coordinator owns decisions/specs/status, root manifests/
locks/scripts, registration, shared guard/grants, common Web navigation and `.sqlx`.
Both backend owners submit shared-file patches for serialized integration.

## File and schema ownership

| Owner | Exclusive primary paths | Integration boundary |
|---|---|---|
| Mobile backend | `domain/mobile/*`, new metadata composer, tag/custom-field command helpers, owned revision/snapshot/receipt migrations, mobile API/tests/contracts | Coordinator applies original/admitted **metadata** worker barrier patches, shared guard/grants and Web event tests |
| Migration | New `domain/migration/admitted_activity*`, original `activity*` identity/provenance adaptations, owned 010f4 migrations, API/tests and new Web modules | Coordinator integrates readiness/preflight, common worker/router/guard/navigation paths |
| iOS | `ios/` and own evidence | Frozen backend contract only |
| Android | `android/` and own evidence | Frozen backend contract only |

Paths under `domain/` above are within `backend/crates/crm-app/src/`. Mobile and
migration schema versions are allocated disjointly from the actual inventory at
launch; only the assigned database owner creates its migrations. Never modify an
already-applied migration. Generated SQLx cache is integrated once, serially.
Discovery of another shared file requires explicit reassignment before edits.

## Cross-slice risks and exit checkpoints

1. **Mobile catalog/value lock order.** Complete the ordinary tag/field and original/
   admitted metadata writer inventory before adding triggers. Put barrier admission
   before the mobile adapter's existing Person lock. Prove cascade deletion, imported
   value writes and catalog renames cannot invert the graph or bypass review holds.
2. **Activity identity ownership.** The existing registry's PK is already global;
   extend its owner tuple with exact conditional FKs, rather than duplicate it.
   Original row bytes/ownership and reader semantics stay intact. Confirm old worker/
   reader rejection against actual capability/schema predicates, not a flag alone.
3. **Revisions and authority.** Notes/tasks may advance broad mobile revision but
   must not advance metadata/details/stage tokens. Metadata updates do not manufacture
   activity/contact credit. Migration-review workspaces deny every new ordinary
   mobile path; private activity permits can insert only their planned note/task.
4. **Catalog versus Person reconciliation.** Catalog-only changes must invalidate
   an editable native metadata baseline even with unchanged Person revisions; old
   generation/receipt support and populated-store upgrades require dedicated proof.
5. **Preservation and release.** Both lanes use isolated synthetic data. Keep shared
   API3000/Web5173, native demo API3101, prior QA APIs/stores, keys, source evidence,
   backups and recovery artifacts. Actual occupancy is inspected at launch; these
   numbers reserve nothing. No application reinstall or cleanup of retained stores.

## Required checks and isolation

All M6/F4 acceptance rows are mandatory. Record actual commands, runner, tested
revision/fixtures, failures and evidence paths. A listed check is not passing
evidence. Use relevant focused gates during development and final integrated gates
once on the final tree; repeat only for changed/failed/stale evidence.

From the root, final sequential gates (set paths to newly owned directories and an
owned synthetic check environment before execution; do not source credentials into
tool output):

```sh
export CARGO_TARGET_DIR="$CRM_PAIR_QA_ROOT/cargo"
export CRM_CHECK_WEB_OUT_DIR="$CRM_PAIR_QA_ROOT/web-dist"
export CRM_CHECK_ENV_FILE="$CRM_PAIR_QA_ROOT/check.env"
./scripts/check
./scripts/sqlx-prepare
./scripts/check-db
git diff --check
```

`CRM_PAIR_QA_ROOT` must be a newly allocated absolute private QA directory, not an
existing runtime artifact directory. The check environment points to the documented
test services/roles; it is not a request to run destructive dev-bootstrap. DB-backed
tests, SQLx preparation and performance runs are serialized across the pair.

Focused backend commands run from `backend/` with the isolated Cargo directory:
`cargo fmt --all --check`, `cargo clippy --workspace --all-targets --locked -- -D warnings`,
and `cargo nextest run --workspace --locked` with exact test filters pinned by the
contract owner. DB cases run through the documented check-db environment/feature
setup; verify selectors actually ran tests. Web commands from `web/`:
`pnpm run lint`, `pnpm run typecheck`, `pnpm run test`, and
`pnpm run build --outDir "$CRM_CHECK_WEB_OUT_DIR"`.
Preflight tests: `python3 -B -m unittest discover -s scripts/tests -p test_migration_release_preflight.py`.

Native baseline commands, using owned QA scheme/variant/device substitutions frozen
at lane launch and the current [iOS](../../ios/README.md)/[Android](../../android/README.md)
instructions:

```sh
DEVELOPER_DIR=/Applications/Xcode.app/Contents/Developer xcodebuild \
  -project ios/FieldCRM.xcodeproj -scheme "$CRM_IOS_QA_SCHEME" \
  -destination "id=$CRM_IOS_QA_DEVICE" \
  -derivedDataPath "$CRM_PAIR_QA_ROOT/ios" test CODE_SIGN_IDENTITY=-
```

Android, from `android/`: `./gradlew --no-daemon assembleDebug testDebugUnitTest lintDebug`
and `./gradlew --no-daemon connectedDebugAndroidTest` are the existing command
shapes. Substitute the owned QA variant, API origin, isolated build directory,
Gradle user home and selected emulator at the checkpoint. Keep the pinned JDK/SDK;
do not upgrade dependencies to perform these checks. Run the populated-store
upgrade as a separately controlled installed-app proof; Gradle connected tests may
remove their app and therefore cannot be that proof on the retained demo identity.
Freeze expanded commands and actual device/variant IDs in each verification record.

Both apps need actual offline/restart/upload/conflict/storage-failure and upgrade
journeys. Web needs real desktop/390px source/mapping/confirm/cancel/remainder
journeys. Builds and unit tests alone are insufficient. D-050 gates one paired
Person/Today regression on the final integrated baseline and indexed bounded plans
for all new/changed hot queries. Coordinate one benchmark run; do not duplicate
per-lane capacity exercises or treat laptop absolute p95 as a production gate.

## Planning exit and remaining authority

The specifications declare the accepted product/contract changes and acceptance
criteria; the briefs assign implementation checkpoints rather than permission to
invent policy. [Author findings](../tasks/MOBILE_006_010f4_PLANNING_REVIEW.md) record
inspection and independent planning reviews, both READY. D-084 accepts these
contracts and authorizes implementation and isolated synthetic verification. At most two review/fix
rounds per slice under D-050; a third requires explicit approval.

The previous Mobile005 release keeps its existing authorization and is recorded
separately; this implementation does not claim its rollout is complete.
Publication/deployment of this pair, app distribution, physical phones/cellular,
live-source/customer processing, erasure policy, activation and calling remain
separately scoped. Subsequent history work stays a just-in-time following plan.
