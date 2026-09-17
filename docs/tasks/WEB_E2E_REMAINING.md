# Remaining catalog families: Web E2E verification

Implementation: correspondence (04), Operator (08), calls (09), and migration
(10). Subagents owned separate family files; shared orchestration and provider
wiring were coordinated in the primary lane. This expansion changes no production
application behavior, shared contract, or database migration.

## Reuse and provider boundaries

API, Web, browser, calls Web, and migration API images use content fingerprints
and process-locked build reuse. Optional targets build only for their selected
families. Every attempt still owns a fresh database, credentials, network and
services. Ordered steps share the family's state without resetting it.

Correspondence delivers synthetic RFC822 messages at the authenticated inbound
boundary. Operator uses controlled completion/tool-call responses through the real
inference HTTP adapter, then executes real authorization and commands. Calls uses
the real backend LiveKit HTTP adapter and a substituted external browser SDK;
it proves signaling/application state, not WebRTC, audio, actual microphone
permission or SIP/carrier transport. Migration uses the existing injected Reader
and synthetic release-readiness fixtures; normal routes, typed commands, workers,
PostgreSQL and review gates remain real. It does not prove production FUB HTTPS
behavior, source API fidelity or deployed-fleet compatibility.

## Verification

Run `18502a4cca9c` passed correspondence 9/9, Operator 8/8 and calls 9/9.
Migration initially failed a stale source selector; explicit UI refresh fixed it.
Run `ec2ae79007c7` passed the first eight migration steps, including actual People
import/provenance and review-hold denials, then failed a case-sensitive text
assertion. Neither run is claimed as a complete suite pass.

Run `3d28ec779f3d` passed all nine non-migration families, 87 steps, at concurrency
4. All 36 family pairs had disjoint networks, Organizations and People. Every
attempt cleaned up with `verified_empty`. Elapsed attempt window was 119.6 seconds
while the expanded migration image was also compiling.

Migration run `1fab01ab4411` passed all 16 steps with verified cleanup.
Authoring runs `2e6e2a31d0bb` and `db02d4a7c2ef` exposed test synchronization
errors: checkbox enumeration preceded rendered proposal/plan controls. Tests now
wait for visible acknowledgements and all selected ready-family controls; no
application changes or weakened assertions were needed. Both failed environments
were removed.

Final command: `./scripts/e2e --all --concurrency 4 --timeout 900`.
Run **`50e7de25c305` exited 0: all 10 families and all 103 steps passed**.
Four actual browser-family intervals overlapped. All five image targets were reused
without builds. All 45 family pairs had disjoint networks, Organizations and People;
project markers matched. Every attempt reports `verified_empty`. The attempt window
was 160.8 seconds; browser execution spanned 135.6 seconds.

| Family | Passed steps |
|---|---:|
| Workspace | 12 |
| Leads | 11 |
| Routing | 9 |
| Correspondence | 9 |
| Relationships | 10 |
| Tasks | 10 |
| Lists | 9 |
| Operator | 8 |
| Calls | 9 |
| Migration | 16 |

Evidence: `.e2e/runs/50e7de25c305/summary.json`, `isolation-proof.json`,
`browser-overlap.json`, `build-cache.json`, `source.json`, `runner-tests.log`
and `support-tests.log`. Each family directory contains `steps.json`, sanitized
HTTP/realtime/database/provider evidence, `html/index.html`, traces and videos.
All artifacts sanitized successfully; no external provider request occurred.

These are representative stateful paths through every catalog family, not every
branch in the catalog. In particular, standalone People admission/recovery and
admitted-cohort migration ladders remain outside this migration journey. The
individual records below enumerate residual branches.

Harness checks: 10 Python runner checks and 5 browser diagnostic regression checks
passed against the final tree/image. `git diff --check` passed. The migration
example's release build verifies the Rust fixture against
the actual application. Bounded reviews checked the calls boundary and migration
worker/Reader scope; missing standalone admission/recovery coverage is not claimed.

Detailed coverage and remaining branches: [correspondence](WEB_E2E_CORRESPONDENCE.md),
[Operator](WEB_E2E_OPERATOR.md), [calls](WEB_E2E_CALLS.md),
[migration](WEB_E2E_MIGRATION.md). Instructions: [harness README](../../e2e/README.md).
