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
  did not satisfy these requirements; further repair remains in progress.
- **M6-R1-04, P2:** iOS resumed completed catalog sections from their beginning.
  Fixed in `a8fb89c`; completed sections now skip on resume.

Both initial live offline/restart/upload journeys and actual populated installed
Mobile005 upgrades passed before these review fixes. Those results do not replace
verification of changed conflict/reconciliation code or final integrated gates.

## Slice 010f4 — round 1

- **F4-I1, P1:** selected snapshot People evidence was not qualified. Root is
  extending the bounded capture/index pass to People and holding related native
  units for missing, unsupported or conflicting observations.
- **F4-I2, P2:** out-of-cohort records and invalid occurrences inflated primary
  totals. Root is retaining inspectable excluded coverage separately from native
  planned/pending/held totals and preserving that coverage through remainders.
- **F4-I3, P1:** new Rust checks did not fence old binaries. Root is extending
  the existing shared workspace barrier with a transaction-local compatibility
  stamp, preserving the original lock and authorization behavior. Old worker/read
  entry paths must fail after confirmed handover; current stamped paths recover.
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
rollback and byte accounting passed there. The broader source/handover changes
require a new matrix run. Browser acceptance, query plans, paired performance,
SQLx and final repository gates remain open.
