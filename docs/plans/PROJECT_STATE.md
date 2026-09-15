# Project state

Updated: 2026-09-15 — Mobile007 / 010d3 deployed to shared development.
Current status and live residuals only. [History](PROJECT_HISTORY.md) preserves
completed chronology. Decisions remain authoritative; use the
[reading index](../decisions/DECISION_INDEX.md).

## Current state

**Mobile006 / 010f4 are implemented, verified, merged to main, pushed and deployed
to shared development under D-084 and its release follow-up.** Both native clients
edit existing Person tags and custom fields offline with sealed catalogs, atomic
commands and explicit conflict/replay recovery. Administrators import qualified
retained notes/tasks for admitted People with exact cancellation remainders.
Imported workspaces retain the administrator review hold.

The [release record](../tasks/MOBILE_006_010f4_RELEASE.md) owns runtime identity,
74 schema checksums, backup/recovery, preservation, HTTP/mobile/browser proof and
180-second observation. API source `3c080b5` matches final production Rust;
Web source `a96b771` includes the verified narrow-label repair. Shared development
is Mac-hosted, not the planned production cluster. Dated evidence does not prove
future service health.

[Implementation status](../tasks/MOBILE_006_010f4_IMPLEMENTATION_STATUS.md) owns
scope/resources; [final verification](../tasks/MOBILE_006_010f4_FINAL_VERIFICATION.md)
owns acceptance. Both final implementation reviews are READY. Full DB (1,103),
native/browser acceptance, 18/26 realistic plans and paired Person/Today gates pass.
The final Web-only release correction passed all 1,298 Web tests again.

The live database already contained Mobile005/010f3 before this release. Its
older unfinished release record is superseded operationally by this verified
rollout; missing historical evidence was not inferred or recreated. Earlier
Mobile004/010e4 and preceding slices remain documented in history.

## Current branch and retained resources

`main` contains the Mobile007 / 010d3 release. Its runtime identity and smoke
checks are recorded in [the release record](../tasks/MOBILE_007_010d3_RELEASE.md).
Private verification services and build outputs remain retained.
Three previously completed writer worktrees and five milestone branches
were removed after preserving all lane histories and the exact dirty migration
patch in a verified private Git bundle. QA databases, installed native stores,
captures, verification artifacts and release recovery material remain. The owned
native API3106 is available; the isolated browser QA processes are stopped.

## Next family boundary

010d3 implementation and shared-development release are complete alongside
010e4 core refresh, 010f3 metadata and 010f4 activity. D-087 selects
[010e5 stage/agent mapping repair](SLICE_010e5_MAPPING_REPAIR.md) for
existing People first. Implementation and local verification are complete under
D-088; [evidence](../tasks/SLICE_010e5_VERIFICATION.md) records the gates. Commit `05abffa`
is merged, pushed and deployed to shared development;
[release evidence](../tasks/SLICE_010e5_RELEASE.md) owns current runtime identity. Never-imported People recovery follows separately.
Remaining family deltas, activation and complete migration fidelity retain their
own scope in the [family ladder](SLICE_010_ADMITTED_PEOPLE_LADDER.md).

## Last accepted decision

**D-088:** accepts 010e5 implementation and one reviewer for independent planning/
implementation review. Implementation is verified and merged into local `main` as `05abffa`; shared
development and release scopes remain unchanged.

**D-087:** authorizes planning stage/agent mapping repair and selects existing
People first. Proposed repair contracts require review and implementation acceptance.

**D-086:** accepts Mobile007 / 010d3 contracts and implementation with isolated
synthetic verification. The implementation, publication and shared-development
deployment are complete; customer-facing operational work remains separate.

**D-085:** authorizes planning admitted-People history with the user-selected
mobile companion, online Find and save People. Both detailed draft specifications,
execution briefs and coordinated plan are written. No new implementation or
shared-contract approval follows from this planning choice.

**D-084:** accepts the reviewed Mobile006/010f4 contracts and authorizes
implementation plus isolated synthetic verification, established writers and
local integration commits. Its release follow-up now authorizes main publication,
cleanup and shared-development deployment; other operational scope remains separate. D-083's planning boundary is superseded for this implementation.

**D-082:** implement both reviewed plans and isolated synthetic verification,
with Terra high writers and the coordinated three-worktree sequence. Local
integration checkpoints are allowed. Implementation and required verification are
complete; its subsequent release follow-up authorizes publication/deployment,
its deployed schema is now included in the verified D-084 release.

**D-081:** draft Mobile005 and 010f3 together, preserving parallel mobile/migration
development; 2026-09-14 follow-up authorized independent review, now READY.
D-082 subsequently accepted the reviewed contracts and implementation. D-081's
earlier planning-only boundary is superseded for this accepted implementation.

**D-080 plus its release follow-up:** accepted both implementations, then authorized
commit/push, cleanup and shared-development deployment. Those actions are complete.
Live customer/source work, activation and native distribution remain separate.

**D-079:** plan Mobile004 and 010e4; user selected smaller sequential migration
slices and kept calling afterwards. Proposed implementation contracts are not
accepted by planning authorization.

**D-078 plus its 2026-09-13 release follow-up:** accepted Mobile 003's reported
contact occurrence time and 010e3's core-only admission contracts, then authorized
the completed main merge, push and shared-development release. Full text and
prior approvals: [decision index](../decisions/DECISION_INDEX.md#accepted-decision-locations).
No repeat approval is pending for those completed actions.

## Operational entry points

| Need | Entry point / limit |
|---|---|
| Development | [README](../../README.md#development); Docker restart does not restart the API; dev-bootstrap wipes data |
| Boundaries | [Architecture map](../architecture/ARCHITECTURE_BASELINE.md); planned production differs from development |
| Release/recovery | [Current release](../tasks/MOBILE_006_010f4_RELEASE.md), [compatibility runbook](../tasks/SLICE_010c_RELEASE_PREPARATION.md), [release prompt](../prompts/07-deploy.md) |
| Real data/production | [Readiness](PRODUCTION_READINESS.md); its historical release inventory predates the current release |
| Prior checkpoints | [History/ledger](PROJECT_HISTORY.md#slice-ledger); detailed proof stays in per-slice records |

Import/refresh/admission confirmation needs fresh actual-workload inventory and
`CRM_MIGRATION_RELEASE_REPORT`. The final D-084 report expires at
`2026-09-15T08:01:58Z`; no automatic renewal exists. Ordinary CRM/retained reads
remain available after expiry. No live source operation or customer activation
was performed. Re-inventory actual workloads before later confirmations.

## Parked / queued tracks

- **Mobile design/system information and physical-phone/cellular testing:**
  explicitly user-deferred. Preserve essential saved-work/access/sync feedback.
  [Device follow-up](../tasks/MOBILE_001_PHYSICAL_DEVICE_FOLLOWUP.md) owns prerequisites.
- **Live FUB validation/customer processing:** user-deferred; dataset and V/C
  readiness gates apply before source operations. Synthetic proof is not live qualification.
- **Remote gates:** deferred; local gates were about three minutes at the recorded
  checkpoint. Preserve the prior vendor/runner survey in history; recheck before selection.
- **Deferred walkthroughs:** Slice 017 live sends; 009 steps 3–5 (reply-all,
  retroactive forwards, rotation). A held capture row was deliberately retained
  for the user's dismiss-path exercise. Earlier 011a filter walkthrough is complete.

## Live residuals and follow-ups

- **Customer readiness:** erasure runbook, key/retention/deletion policy, synthetic
  restore proof and responsible owners remain open. [Readiness](PRODUCTION_READINESS.md)
  also tracks core snapshot multi-worker handoff and SQLx cancellation notices;
  neither has a proven benign explanation or completed correction.
- **Telephony:** Telnyx SIP password/trunk rotation remains a user action;
  busy/ring-out live proof, placing-sweep versus slow microphone prompts and
  deactivated-call-owner handling remain open. Host: `livekit1.tarams.org` (D-055).
  Egress is dormant. Check instructions before live telephony operations.
- **Call-history ordering:** the 2026-09-12 audit reproduced timeline placement
  failure around `call_completed`, including in isolation; later complete run
  passed. Host/DB clock comparison is a supported mechanism, not proven ties.
  [Audit](../tasks/SLICE_010_COMPLETION_AUDIT_2026-09-12.md) preserves limits and
  the separate sleep/wake-correlated database warning episode.
- **Known product edges:** 007h1 plain-text forwarded signatures remain accepted;
  HTML gmail_quote separation is later. Manual explicit assignment lacks a
  membership-status filter (D-027/O-004); `set_local_password` and the actual
  `crm-admin` binary lack recorded test coverage. Revisit early-slice cookie,
  bind, request-ID, configuration and error-output minors when affected code changes.
- **Tooling:** fourth 501-INSERT fixture remains unbatched; killed runs may leave
  SQLx test databases; doctests previously dominated service-free checks. The lld
  experiment was reverted; retry only for a new toolchain reason. Never overlap
  DB-backed runs against shared test resources.
- **Environment:** dev ingress lives in the Cloudflare dashboard (D-025).
  Verify process identity and additive schema before release; old API processes
  can survive Docker restarts. Temporary backup artifacts are not a durable backup policy.
- **Standing review discipline:** verify reported changed files against actual
  tracked/untracked diffs. Prior unexplained document additions were retained by
  user choice; preserve attribution rather than guessing authorship.
- **Minor evidence pin:** 011a M7 positive `filter_kinds` span pin remains skipped;
  URL/filter regressions were resolved, and the unconstructible 20-clause ceiling
  was closed. Full residual wording remains in the archived snapshot.

## Backlog (deferred product tracks — full notes in the decision log)

Email sending/transactional/mailbox reconstruction (O-014/O-006), per-Person
crypto-shred and erasure (O-012/O-013), bulk-content storage/retention (D-062/O-015),
AI next-step suggestions (O-008), SMS consent (O-006) and recording consent (O-002)
retain their owning decisions. D-056's size cap is already implemented; raw MIME
still uses encrypted PostgreSQL storage. No new policy is chosen by this list.
Foundations F-01/F-02/F-03 remain proposals except where separately accepted.

## Latest verification

[Mobile006 / 010f4 evidence](../tasks/MOBILE_006_010f4_FINAL_VERIFICATION.md) records
all implementation gates passing and both final review verdicts READY. The
[release record](../tasks/MOBILE_006_010f4_RELEASE.md) adds exact old-field/new-default
preservation, five additive migrations, 75 HTTP checks, real desktop/narrow UI,
73 matching public assets, tunnel checks, cleanup and final stable observation.
Failed setup/check attempts and their corrections remain in the owning evidence.
Earlier milestone results remain in [history](PROJECT_HISTORY.md) and their
linked verification records; they are not claims about present runtime health.

## Next recommended action

010e5 is implemented, verified, pushed and deployed to shared development. Next migration planning: recover never-imported People, including
identity safety and follow-on family imports. See [010e5 status](../tasks/SLICE_010e5_IMPLEMENTATION_STATUS.md).

Use the [Mobile007 / 010d3 release record](../tasks/MOBILE_007_010d3_RELEASE.md)
for shared-development runtime identity and recovery. [Final evidence](../tasks/MOBILE_007_010d3_FINAL_VERIFICATION.md)
records the passing implementation gates and retained corrections.
The [family ladder](SLICE_010_ADMITTED_PEOPLE_LADDER.md) retains the sequential
migration scope. Native calling follows the agreed progression. Physical-phone/design/live-source work remains deferred;
complete customer erasure/restore readiness before real customer data.
No repeat implementation or release action is pending for Mobile006/010f4.

## Approval currently required

None for the completed D-084 implementation, publication, cleanup and shared-
development release. D-084 superseded D-083's planning-only boundary.
New shared contracts, native distribution, live FUB/customer processing, activation
and production deployment retain their own scope and gates. Recovery targets,
retention/erasure, support access, outbound/recording consent and agent-departure
policy remain open where the decision log says so.
