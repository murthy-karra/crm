# Mobile006 / 010f4 implementation review

**IN PROGRESS.** D-050 independent implementation round 1 returned NOT READY
for both slices. Planning reviews are separate. One implementation review round
remains for each slice; no second round has been requested yet.

## Mobile006 — round 1

- **M6-R1-01, P1:** iOS reopened drafts ignored durable proposal actions. Fixed
  in `a8fb89c` (integrated `c50fd7f`); regression evidence is in the iOS lane.
- **M6-R1-02, P2:** Android reused Person metadata across catalog-only changes.
  Fixed in `e4d53af` (integrated `85d7bd0`); storage instrumentation 6/6 passed.
- **M6-R1-03, P2:** iOS conflict recovery lacked actual value comparison,
  qualified current catalog, and atomic revised proposal creation. First repair
  did not satisfy these requirements; `e2fd7c1` and `2cf2f00` add value comparison, sealed-catalog qualification, atomic replacement and explicit exclusion of invalid actions. ModelTests 20/20 pass; actual UI recovery verification remains in progress.
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
rollback and byte accounting passed there. The broader source/handover matrix passed 12/12 in `logs/integration/f4-review-matrix-db-2.log`. The realistic migration plan harness passed in `activity-hotplans-db-1.log` (227.05s; 25k People/50 members, no plan failures). Storage sizing is a separate pending report. Browser acceptance, mobile query plans, paired performance, SQLx and final repository gates remain open.
