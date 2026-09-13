# Mobile 002 / 010e2 — Implementation status

**Final integration verification — D-076, 2026-09-13.** Both approved contracts
are implemented. Terra high owned the mobile backend, iOS, Android and migration
backend lanes; the coordinator owned protected-read/Web integration and final
verification. Physical phones/cellular and broad mobile redesign remain deferred.
No new implementation approval is pending.

The local integration branch is `codex/mobile002-010e2-integration`. The combined
candidate is `07a0fbf` on the remaining migration worktree. Main/origin and shared
development are still the previous released milestone. This implementation does
not authorize publication, shared deployment, native distribution, live FUB or
customer processing, or activation.

| Component | Verified implementation |
|---|---|
| Mobile backend | `2fc753a`, integrated at `2fd9a9d`: all-writer note revisions, typed edit_note/update_task commands, exact durable receipts and bounded current-record reads |
| iOS | `5ffa962`: encrypted in-place upgrade, native offline edits, restart/replay, explicit conflict/revised receipt, protected ambiguity/permission transitions |
| Android | `b8a7f95`: encrypted in-place upgrade, native offline edits, process-death replay, completed-task field preservation, conflict/revised accepted receipt |
| Migration | `a0eb294`: qualified retained evidence, immutable bounded plans/confirmation, per-Person atomic updates, exact ownership/accounting, cancellation/recovery and protected paged Web review |

Mobile foundation and both native worktrees/branches are merged locally and
closed. Only `codex/migration-010e2` remains for the combined repository/DB gate.
No shared source output, API3000/Web5173, demoAPI3101 or original mobile demo
bundle/store was used for these mutations. The native QA apps use distinct
identities and protected storage namespaces.

## Actual verification

- Mobile foundation: focused edit tests, all 12 mobile DB cases, 19 note cases
  and 34 task cases passed. Its isolated SQLx preparation passed with clean
  `.sqlx`. Existing ordinary command/tenant/revision behavior is included.
- Combined `scripts/check` passed on `07a0fbf`: **975 Rust tests, five
  compile-fail doctests, 1,203 Web tests and 11 email-worker tests**, plus
  formatting, Clippy, production compilation, dependency fences, lint,
  typecheck and production Web build. Log:
  `/private/tmp/crm-010e2-qa/check-integrated.log`.
- Full `scripts/check-db`, including live SQLx metadata validation and all
  ignored ordinary DB cases, is the final gate still running. Do not treat
  earlier focused checks as a complete DB gate.
- [iOS evidence](MOBILE_002_IOS_VERIFICATION.md): 17 protected-store tests;
  real-API lost-response/conflict; actual installed old-store upgrade with
  exact encrypted-file/envelope preservation; native offline save/terminate/
  relaunch/sync; final native conflict/revised receipt (`qa-ui-conflict-v18`)
  and two protected-model fault tests (`qa-model-fault-v2`). The coordinator
  independently read both final xcresult summaries: 1+2 passed, no skips/failures.
- [Android evidence](MOBILE_002_ANDROID_VERIFICATION.md): populated encrypted
  Room upgrade, storage tests, JVM tests, no-bypass build/lint, and actual API3102
  instrumentation. Exact queued note bytes survived force-stop/relaunch and
  received acceptance. An existing completed task changed title/kind/due date
  without changing completion/ownership. Both actors' conflict/current state
  appeared in native Saved work; a new revised operation received an accepted
  revision-4 receipt and the authoritative note matched. The coordinator read
  all final pass transcripts and verified all four added dependency hashes
  directly against official Maven Central artifacts.
- [Migration Web/read evidence](SLICE_010e2_WEB_READ_VERIFICATION.md): bounded
  Unicode/full-contact paging, admin/tenant/cursor/authority fences, exact byte
  accounting/replay and controlled recovery cases. The real desktop/390px Web
  preview, re-preview, cancellation, exact confirmation and reload were tested.
  Its synthetic run settled six outcomes: **one update, one verified no-op,
  three held/retained, one excluded**. SQL reconciliation proved the other
  People, original parent/results/report, four-Person total and review hold
  unchanged; intended contact identities were preserved.
- [D-050 evidence](../design/perf/slice-010e2-2026-09-13/README.md): one 25k
  plan collection found an unbounded report-group anti-join. The fix reads 50
  raw descriptors first, followed by exact indexed ownership lookups. The two
  changed statements passed on the same retained fixture, without spills.
  The same-build Today pair had equal DTOs and current p95 48.633834ms versus
  baseline 35.023667ms, within its 25ms permitted increase.

## Evidence limits and resources

The 25k cardinality fixture uses explicitly inert ciphertext clones for SQL
plans; fidelity and accounting use separately qualified retained evidence.
The paired Today fixture has 100 People and a fixed clock. Neither is production
capacity evidence. An old Person-detail performance harness rejected a stale
source hash before measurement; its manifest was not weakened. Initial failing
Web fixture assumptions, missing receipt charges, local-conflict totals,
absent-record labels and native QA setup issues were corrected and their
passing follow-ups are recorded in the linked evidence.

Private mode-0600 configs and logs are under `/private/tmp/crm-mobile002-qa/`
and `/private/tmp/crm-010e2-qa/`. Synthetic databases are `crm_mobile_002` and
`crm_010e2_qa`; native API3102 and browser API3103/Web5174 were isolated.
Sensitive keys are not in these records. The original native demo and shared
runtime remain intact. Implementation completion does not establish cutover
readiness or migration fidelity beyond existing-People core fields.
