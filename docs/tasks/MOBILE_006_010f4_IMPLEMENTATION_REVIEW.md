# Mobile006 / 010f4 implementation review

**READY — both slices, final D-050 implementation round 2, 2026-09-15.**
Independent evidence closure reports no actionable findings. Planning reviews are
separate. This closes the existing second/final implementation review round;
it does not by itself claim publication or deployment.

## Mobile006 — round 1

- **M6-R1-01, P1:** iOS reopened drafts ignored durable proposal actions. Fixed
  in `a8fb89c` (integrated `c50fd7f`); regression evidence is in the iOS lane.
- **M6-R1-02, P2:** Android reused Person metadata across catalog-only changes.
  Fixed in `e4d53af` (integrated `85d7bd0`); storage instrumentation 6/6 passed.
- **M6-R1-03, P2:** iOS conflict recovery lacked actual value comparison,
  qualified current catalog, and atomic revised proposal creation. First repair
  did not satisfy these requirements; `e2fd7c1` and `2cf2f00` add value comparison, sealed-catalog qualification, atomic replacement and explicit exclusion of invalid actions. ModelTests20/20, current StorageTests33/33 and the actual simulator conflict UI pass; the iOS verification record owns logs.
- **M6-R1-04, P2:** iOS resumed completed catalog sections from their beginning.
  Fixed in `a8fb89c`; completed sections now skip on resume.

Both initial live offline/restart/upload journeys and actual populated installed
Mobile005 upgrades passed before these review fixes. Those results do not replace
verification of changed conflict/reconciliation code or final integrated gates.

## Slice 010f4 — round 1

- **F4-I1, P1:** selected snapshot People evidence was not qualified. Fixed in root `3c01177`: the bounded capture/index pass includes People and holds related native units for missing, unsupported or conflicting observations.
- **F4-I2, P2:** out-of-cohort records and invalid occurrences inflated primary
  totals. Fixed in root `3c01177`: inspectable excluded coverage is separate from native planned/pending/held totals and preserved through remainders.
- **F4-I3, P1:** new Rust checks did not fence old binaries. Fixed in root `3c01177`: the existing shared workspace barrier requires a transaction-local compatibility stamp after confirmed handover, preserving lock and authorization behavior. Old worker/read entry tests fail closed and current stamped paths recover.
- **F4-I4, P1:** admitted Web children called original activity endpoints. Fixed
  with a typed family provider and distinct access/cache scope. Focused original,
  admitted and evidence component suites passed 53/53; the added unconfirmed
  cancellation/reprepare case brings the passing focused count to 54.
- **F4-I5, P2:** unconfirmed source replacement was unreachable. The UI now
  exposes explicit cancellation of the unconfirmed plan and preparation from the
  selected source, including when a cancelled root remains selected. Backend
  cancel/reprepare uses existing typed commands and durable request receipts.

Earlier expanded database matrix: seven passed; the tombstone fixture and outer
no-store failure were corrected and their two selectors passed separately.
Bounded remainder copying, authority loss, two workers, exact receipt replay,
rollback and byte accounting passed there. The broader source/handover matrix passed 12/12 in `logs/integration/f4-review-matrix-db-2.log`. The realistic migration plan harness passed in `activity-hotplans-db-1.log` (227.05s; 25k People/50 members, no plan failures). Allocated relation sizes are recorded separately, without a capacity claim. Browser acceptance, both realistic query-plan sets, SQLx and the final repository gate now pass. Full DB and the completed paired measurement now pass.

## Round 2 remediation checkpoint

- Mobile M6-R2-01: Android current-read catalog could outrun its installed sealed
  catalog during conflict replacement. Fixed in `6ae1d27`, integrated `26d0d00`;
  the final catalog-integrity repair additionally requires exact cached catalog row completeness. Ten current storage tests pass; final live UI submission and retained exact-envelope replay receipt now pass.
- Mobile M6-R2-02: iOS staging generation/shared qualification marker could hide
  an intact sealed metadata baseline during an ordinary outage. Explicit generation
  qualification fixes this; 33/33 current storage tests pass. Reviewer accepted
  the code and actual new regression evidence in this same round.
- Migration: excluded-only coverage could advertise a remainder that cannot be
  created. The view now uses the same exclusion predicate as the command; the
  native-settled-before-terminal-cancel regression passed in 6.75s.
- Migration: reload could select a predecessor without a way to reach its
  successor. A labeled, bounded attempt selector and cursor controls now expose
  prior/successor attempts; 26 focused Web tests and typecheck passed.

- Migration: actual native Person source inspection exposed another original/admitted
  reader-family use. `05faec2` qualifies the family by the exact server provenance
  path; both note/task browser reads and 41 focused tests pass.
- Migration: the 22-probe performance run exposed unbounded remainder availability
  and settled-tail traversal. `22c6c06` uses canonical committed pending counts and
  one indexed manifest/result checkpoint per worker unit. The reviewer verified
  all 26 final plans, including late/empty and present/absent cases, with no failures.

## Final verdicts

- **Mobile006: READY.** All static findings are closed. Native iOS/Android
  storage, actual conflict replacement, exact replay and installed upgrade evidence
  pass, alongside the shared final gates.
- **Slice 010f4: READY.** Source/authority/remainder fixes, exact browser
  reconciliation, 14 focused DB cases and 26 realistic plan probes pass.
- Shared execution evidence: final repository and SQLx gates pass; all 1,103
  DB tests pass. Mobile plans pass 18/18. The paired Person and Today comparisons
  each have 40 equal measured responses per arm and pass their p95 limits.
- The reviewer accepted `d6d77e4`'s exact revision expectations/batched large
  fixture and `4bf6053`'s shared authentication manifest correction. Failed and
  interrupted attempts remain retained; no third review round was opened.

[Final verification](MOBILE_006_010f4_FINAL_VERIFICATION.md) owns executed commands,
results, source attribution and remaining scope limits.
