# Mobile 007 / 010d3 — Find People and preserve their history

**IMPLEMENTATION ACCEPTED — D-086, 2026-09-15.** The user accepted both plans
and declared contracts for implementation and isolated synthetic verification.
Both independent planning reviews are READY. Compatible review
corrections are owned work; materially different policy and publication/deployment
remain separate. [Current implementation status](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md)
owns progress and evidence; planning-era wording below does not limit D-086.

Originally planned under D-085; D-086 authorizes the reviewed implementation.

## Goal and deliverables

| Track | What the user gains | Detailed documents |
|---|---|---|
| Mobile 007 | Search the operational Organization online, save a Person, then use the existing offline tools after a complete download | [Specification](../specs/MOBILE_007_FIND_AND_SAVE_PEOPLE.md), [briefs](../tasks/MOBILE_007_IMPL.md) |
| Migration 010d3 | Import earlier events, calls and text metadata into the exact People added by later admissions, with visible gaps and safe recovery | [Specification](../specs/SLICE_010d3.md), [brief](../tasks/SLICE_010d3_IMPL.md) |

User selected Find and save People after requesting the history plan and asking
about continued mobile work. Both tracks keep moving independently: mobile uses
an operational synthetic Organization; migration stays in an administrator-review
workspace. They do not depend on migration activation or native history downloads.

## What was inspected

Baseline main `d8367a7`; current release evidence belongs to PROJECT_STATE and
MOBILE_006_010f4_RELEASE. No service identity/health claim follows from source.

- iOS `FieldCRMApp.swift` PeopleView and `FieldModel.pin`; Android `MainActivity.kt`
  PeopleScreen and `FieldRepository.pin`: downloaded-name search + manual UUID pin.
- `domain/mobile/generations.rs`: selected set is Today + assigned + explicit pins;
  invalid pins fail validation and only a complete seal qualifies the generation.
- `domain/person/queries.rs::search_summaries`: established literal name substring
  and exact normalized contact matching; contains inquiry aggregates to omit from
  the narrow new discovery projection.
- `history_import_source.rs`, `history_import_worker.rs` and migration
  `20260922000001_fub_history_timeline.sql`: existing stable global HMAC and
  original-only owner/provenance constraints.
- `history_review.rs`: admitted Person binding exists; current owner/provenance
  and immutable revision/readiness paths need deliberate extensions.

D-015/D-050, D-059–065, D-069–074 and D-079–084; O-012/O-013/O-015;
architecture baseline, Mobile001/006, 010d1/010d2/010f4 and the family ladder informed
these drafts. Earlier state banners in accepted specs remain historical; current
code/release evidence owns implementation status. No authoritative policy conflict
was found. Proposed contract changes are declared rather than applied.

## Recommended sequence after contract review and implementation acceptance

1. Freeze narrow mobile search fixtures and history source/owner/reader contracts.
2. Mobile backend and migration backend/Web can run concurrently under exclusive
   ownership. No client work against moving search DTOs.
3. Integrate the verified mobile backend, close that writer worktree, then run iOS
   and Android while migration continues. Mobile does not wait for history.
4. Integrate and verify both tracks. Deployment/publication remains its own later
   release step with fresh actual inventory and preservation evidence.

This is a future execution recommendation; no agents or worktrees have launched.
Keep the established Terra high preference for substantive implementation, per
MODEL_ROUTING. Model selection alone never authorizes a runner or delegation.
At most three short-lived worktrees: backend + migration first, then iOS + Android
+ migration. Each has one writer/branch/brief and explicit required checks.

## Ownership

| Owner | Exclusive files | Shared handoff |
|---|---|---|
| Mobile backend | Mobile domain/search route; narrow Person discovery query; mobile tests/contracts | Coordinator applies common registrations, optional measured index allocation and `.sqlx` |
| Migration | New admitted-history domain/API/Web; original history ownership/provenance compatibility; assigned additive schema/tests | Coordinator applies common workspace guard/grants, worker wiring, preflight, navigation and `.sqlx` |
| iOS / Android | Respective native tree and own evidence | Both consume one integrated frozen mobile contract |
| Coordinator | Decisions/specs/status, shared files, integration and serialized checks | New shared-file discoveries are explicitly reassigned before editing |

No migration version is reserved by this plan. Only assigned database owners may
create their allocated additive migrations. Never modify applied SQL or runtime
artifacts. Preserve installed native stores, source evidence, private QA/recovery
material and current shared services. Inspect actual ports/workloads before launch.

## Risks and the proposed treatment

- **Search result versus offline truth:** transient results cannot overwrite cache
  or enable editors. Show availability only after existing seal + local commit.
- **Failed requested pin blocks download:** offer explicit pin cancellation/retry
  without losing other selections or unsynced work. A cancelled pin is not erasure.
- **History previously called unlinked:** authenticate raw records and resolve exact
  admission identity in the new plan. Do not rewrite old capture evidence.
- **Identity/provenance compatibility:** one global registry, exclusive owner shapes,
  shared serialization and actual old-worker/read barriers before first write.
- **Unknown source coverage:** one qualified later capture, explicit unknown/missing/
  held outcomes. Completion does not assert a complete account or enable activation.
- **Scope:** history is metadata-only; native calling, reassignment, creation, broad
  design, physical-phone/cellular testing, live FUB and customer work remain separate.

## Required verification when implemented

Every M7 and H3 acceptance ID in the specs is required. Store expanded commands,
source/contract revision, runner/API/schema/device, outcomes and original failures
in the lane evidence. Planning is not proof of passing runtime checks.

Use a newly allocated private `CRM_PAIR_QA_ROOT`, isolated Cargo/Web/native build
outputs and an owned synthetic check environment. Never run dev-bootstrap against
shared development. Serialize all DB-backed checks, SQLx preparation and performance
against shared test resources. Final integrated command shapes:

```sh
export CARGO_TARGET_DIR="$CRM_PAIR_QA_ROOT/cargo"
export CRM_CHECK_WEB_OUT_DIR="$CRM_PAIR_QA_ROOT/web-dist"
export CRM_CHECK_ENV_FILE="$CRM_PAIR_QA_ROOT/check.env"
./scripts/check
./scripts/sqlx-prepare
./scripts/check-db
python3 -B -m unittest discover -s scripts/tests -p test_migration_release_preflight.py
git diff --check
```

During implementation use focused Rust fmt/clippy/nextest/DB cases and Web
lint/typecheck/tests/build. Freeze current iOS xcodebuild scheme/device/derived-data
and Android Gradle variant/emulator/API/build paths from their READMEs at launch;
run native persistence/UI tests and real simulator/emulator + isolated API journeys.
Prove populated-store upgrade without uninstall; do not use a clearing connected
harness as evidence that user storage survived. Native calling/physical devices
are not implied by simulator proof. Web must exercise actual desktop and 390px
preview/confirmation/cancel/remainder/recovery behavior.

D-050: one coordinated paired Person/Today regression and plan-shape run for each
new/changed hot query at a realistic 25,000-Person/50-member book and <=5 Today
loads; request p95 within max(25 ms, 10%) of paired baseline. Inspect actual work,
not LIMIT alone. Report laptop absolute timings without capacity claims. At most
two review/fix rounds per slice; no third round without explicit approval. Repeat
only changed/failed/stale checks after final gates.

## Planning completion and next gate

Both specifications, briefs and contracts completed independent planning review
and were accepted under D-086. A cream Lavish companion shows the two workflows.
The [implementation status](../tasks/MOBILE_007_010d3_IMPLEMENTATION_STATUS.md) and
[final verification record](../tasks/MOBILE_007_010d3_FINAL_VERIFICATION.md) own the
current implementation and acceptance evidence. Publication and deployment remain
a separate step.
