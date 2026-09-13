# Mobile 003 / 010e3 — Shared-development release

**DEPLOYED / VERIFIED — 2026-09-13.** The user requested “do the main merge,
push and deployment” after the completed implementation summary. D-078's release
follow-up authorizes this release to existing Mac-hosted shared development at
[app.tarams.org](https://app.tarams.org/manage/migration).

Runtime source is `b37a480c5bc4cd5c711dceba287cc5bed70a71d0`, including verified
implementation `faee455`. Subsequent closeout commits change documentation and
evidence only. Both native source trees are published; native distribution and
physical-phone/cellular testing remain separate. The original native-demo API
and installed stores were preserved.

The [implementation record](MOBILE_003_010e3_IMPLEMENTATION_STATUS.md) owns the
981 Rust, 1,215 final Web and 1,009 regular database case evidence, SQLx/Clippy,
native upgrade/offline/replay checks, populated migration workflows, paired
Today/Person reads and 25k-Person query-plan proof. Release reused those checks;
no broad suite or new implementation review was repeated. No application-source
correction was required during release.

## Executed release

| Step | Actual result |
|---|---|
| Git | Fetched origin; main had no divergent changes. Scanned 97 publication files against configured credentials with zero matches. Committed release authorization, fast-forwarded main and pushed `b37a480`. All six merged milestone branches were removed; only main and its worktree remain. |
| Builds | Locked API/admin/migrator build passed in 53.113s; production Web in 1.857s. Explicit isolated Cargo/Vite outputs preserved all 77 existing service artifacts during build. All 1,100 recorded source files remained unchanged. Three binary hashes and 73 Web hashes retained. |
| Backup | Custom crm_dev backup: 112,994,843 bytes, SHA256 `1c710fe8f117bd734e7654bea899992ef4055fc7da845c7b65a7b4793f8b7c5b`. Catalog validated 1,543 entries. Restore was not exercised. |
| Schema | Verified migrator applied `20260927000001` and `20260928000001` once. All 43 prior checksums matched before migration; all 45 applied SHA384 checksums match published SQL afterward. No reset or destructive rollback. |
| Preservation | All 142 prior tenant rowsets preserve every old column. The three new admission identity columns are separately verified null on old rows. All 45 business counts, workspace bindings/admissions and 100,077 history-review rows remain unchanged. After test cleanup, all 151 complete upgraded rowsets match; all nine new admission/provenance/fact tables are empty. |
| Runtime/configuration | API PID93752/port3000 and Web PID93773/port5173 run the verified artifacts. Only the release-report path changed; the entire mobile receipt key ring and all other configuration remain unchanged. Native demo API PID71121/port3101 remains on crm_mobile_001. |
| Compatibility | Actual API/in-process workers, admin/migrator launchers, containers and scheduled paths were inventoried. Workspace readiness and all seven migration feature capabilities, including fub-people-admission-v1, passed. Private demo exclusion is backed by its actual process environment/database target. |
| HTTP/assets/tunnel | 74 checks / 83 requests passed. Admission unknown-ID reads, including contacts, full fields and Person provenance, returned closed no-store 401/403/404 responses for anonymous/member/admin/other-org actors. Ordinary People/Today reads passed. Public entry/Migration assets match; all 73 locally served Web files match their build hashes. `scripts/check-tunnel` passed app/API/realtime 200/200/101. |
| Browser | Actual desktop 1280px and 390×844 screenshots inspected. New admission panel, disabled preview with no eligible parent, reload and ordinary Today navigation passed. Narrow panel clientWidth=scrollWidth=277. No console warnings/errors. Existing authenticated session retained; temporary viewport reset and created tab closed. No Operator or migration action submitted. |
| Mobile | 43 public HTTP requests passed. Bootstrap advertises log_contact_attempt alongside prior capabilities, preserves the exact 604,800-second lease and installation/context binding. Four People downloaded through one manifest and 12 component pages, then sealed. Current-task fields/revision, actor/Org/context denials and non-renewing reads passed. No operation POST or business write. |
| Cleanup/observation | All six temporary HTTP/mobile sessions revoked with original-cookie 401 proof. Only the owned context/admission metadata was removed after receipt and identity checks; the worker had reclaimed the generation. Artifacts/source/listeners remained stable across 226.762s; zero API errors, 12 WARNs matched deliberate mobile denial requests. Isolated build cache removed; protected backup, configuration and recovery binaries retained. |

## Limits and recovery

The deployed browser check covers an empty migration workspace. Populated
admission, cancellation/remainder, exact provenance and preservation are attributed
to implementation evidence. The released mobile check covers capabilities and
read/sync compatibility; actual contact creation, replay and offline restarts
remain attributed to the real-API native/backend implementation checks. No native
QA app was installed over the user's demo.

Rust build emitted no warnings. Vite retained its existing large-chunk and
external-output-directory advisories. An initial mobile helper invocation omitted
its explicit rollout flag and stopped before any request; its unstarted record is
preserved separately from the successful authorized invocation.

Actual-inventory confirmation evidence expires at **2026-09-13T14:43:44Z**.
There is no automatic renewal. Ordinary CRM and retained reads continue after
expiry; a later import/refresh/admission confirmation requires fresh operator
preflight. No live FUB/customer processing, workspace activation or
production-cluster release occurred.

Protected backup, old configuration/artifacts, exact new forward-recovery
binaries, private helpers and logs remain at
`/private/tmp/crm-mobile010e3-release-ksz6jcio`. Preserve receipt keys and migration
evidence for retries; do not reset the database or remove workspace bindings to
run older artifacts.

[Sanitized release evidence](../design/qa/mobile003-010e3-2026-09-13-release/README.md)
records artifact/schema/source hashes, preservation and executed check results.
