# Test-binary consolidation chunk — implementation brief

Charter: the QUEUED entry in `docs/plans/PROJECT_STATE.md`
(scoped 2026-08-28, approved for implementation same day — "go").
Read it, plus `AGENTS.md` §14/§15, before changing anything. This
chunk changes the BUILD-GRAPH SHAPE of `crm-api/tests/`, never
test behavior. Acceptance = identical test counts (incl.
`#[ignore]`d) + a timed before/after on a REPRESENTATIVE
SINGLE-FILE EDIT (not just full-gate totals — this chunk's entire
point is edit-to-green latency, which the prior gate-speedup chunk
did not touch).

Branch: `test-binary-consolidation` off `main` (currently at
`5dea91e`; run `git log --oneline -1` to confirm before you start
in case something merged since). No product code, no script
changes outside `crm-api/tests/` and its `Cargo.toml` (adding
`[[test]]` sections is expected and in scope).

## The problem, precisely

`backend/crates/crm-api/tests/` holds 41 `*.rs` files. Cargo
compiles+LINKS each as an independent test binary by default. 369
total `#[sqlx::test]` functions live across them (363 `#[ignore]`d
DB-backed, the rest fast). Editing `crm-app` or `crm-api` forces
all 41 to relink. Today's gate-speedup chunk fixed full-gate-run
latency (nextest, line-tables-only, fingerprint isolation) but did
NOT reduce link COUNT — that's this chunk's job.

`crm-app`/`crm-api`'s own `#[cfg(test)]` unit tests are OUT OF
SCOPE and unaffected — those already compile into one harness
binary per crate; this is purely about the `tests/` integration
directory.

## The fix

Consolidate the 41 binaries into one (or a small, deliberate few
— e.g. splitting fast vs `#[ignore]`d DB tests into two binaries
is a reasonable variant if it simplifies anything) via `#[path]`
module aggregation: a hub file (e.g. `tests/all.rs`) declares
`#[path = "db_people.rs"] mod db_people;` per existing file. Cargo
then has one Rust source root to compile+link for this binary,
instead of 41. Existing test files need NO content rewrite in the
common case — only the hub gains a reference to each.

## THE gotcha — read before starting, budget real time for it

26 of the 41 files contain `mod common;`, referencing the shared
harness at `tests/common/mod.rs` (`create_org_with_stages_and_
member`, `build_router`, `login_cookie`, etc.). Under `#[path]`
aggregation, a `mod common;` declaration INSIDE an aggregated file
resolves relative to that file's NEW position in the module tree
(e.g. `tests/all::db_people::common`), NOT its original filesystem
location — rustc will look for `db_people/common.rs` or similar,
not `tests/common/mod.rs`, and will fail to compile or (worse)
silently resolve to a different file if one happens to exist at
that path. This is a well-documented trap with this exact
technique (search: "cargo integration test compile time
consolidation #[path]" if you want independent confirmation of the
mechanism before coding around it).

Do NOT discover this by trial and error across all 26 files. Fix
it once, structurally, before touching any test file:
- The clean approach: give `mod common;` its own explicit
  `#[path = "../common/mod.rs"] mod common;` (or the correct
  relative path for wherever the hub places things) in EVERY
  aggregated file that has it — OR, likely cleaner, declare
  `#[path = "common/mod.rs"] mod common;` ONCE in the hub file and
  have aggregated files reference it via `super::common` /
  `crate::common` instead of their own `mod common;` (requires a
  small, mechanical find-replace across the 26 files: remove their
  local `mod common;`, adjust `common::` call sites to
  `super::common::` or equivalent — verify which actually compiles,
  don't assume).
  Pick whichever approach compiles cleanly and is less invasive;
  either is fine, but get ONE approach working end-to-end on 2-3
  files first, confirm it compiles AND that `common`'s state
  (if any — check it has none, matches the no-global-state finding
  below) behaves identically, before applying it to the rest.

## Verified ground truth (coordinator pre-check, trust but re-verify)

- No `OnceLock`/`once_cell`/`lazy_static`/`thread_local!` found
  anywhere in `crm-api/tests/*.rs` or `tests/common/mod.rs` — low
  risk of hidden per-binary global state. Re-grep after your
  changes land to make sure nothing new was introduced, but this
  was clean as of `main` `5dea91e`.
- 369 total `#[sqlx::test]` functions (363 `#[ignore]`d + 6
  non-ignored spread across files) — this is the number that must
  be unchanged after consolidation. `#[sqlx::test]`'s ephemeral
  database naming is per-TEST-FUNCTION (derived from the test's
  fully-qualified path, not the binary), so consolidation SHOULD
  be transparent — but two things change the fully-qualified path
  under aggregation (the module nesting shown above), so CONFIRM
  no database-name collisions occur (run the full DB suite, not
  just a subset) rather than assuming this holds.
- `#[ignore]` discovery: `cargo nextest run --run-ignored only`
  must still find and run all 363 after consolidation. Verify by
  count, not by "it didn't error."

## Order of work

1. Confirm `main`'s current commit; branch off it.
2. Pick ONE small test file with no `mod common;` dependency first
   (e.g. `health.rs` or `operator_deps.rs`) and get the
   `#[path]` hub pattern compiling for just that one file. Prove
   the mechanism before scaling it.
3. Solve the `mod common;` resolution problem (see gotcha above)
   on 2-3 files that use it. Confirm compiles + one test from each
   passes.
4. Apply the pattern to all 41 files. Decide and record whether
   you're doing one binary or splitting `#[ignore]`d/non-ignored
   into two — either is acceptable, document which and why.
5. Full verification: `cargo nextest run --workspace --locked`
   AND `cargo nextest run --workspace --locked --run-ignored only`
   (against a real migrated DB per `scripts/check-db`'s own
   pattern) — confirm EXACT counts: 369 total, 363 ignored, same
   pass/fail as before consolidation. Any count mismatch is a stop
   condition, not a "close enough."
6. Timed comparison on a REPRESENTATIVE SINGLE-FILE EDIT: touch
   one line in `crm-app` (pick something low-level enough to
   cascade widely, e.g. `crm-app/src/domain/person/model.rs` or
   similar — not a leaf file only one test file exercises), then
   time `cargo nextest run --workspace --locked --run-ignored only
   --no-run` (compile+link only, no execution) BEFORE this chunk's
   changes (checkout `main`, time it, note the baseline — reuse
   today's known number if you trust it: full workspace already
   warm, editing one crm-app file previously forced ~41 relinks)
   and AFTER (on your branch, same edit, same `--no-run` timing).
   Revert your test edit before finishing.
7. Full gate sanity: run `./scripts/check` and `./scripts/check-db`
   once each on the finished branch to confirm nothing else broke
   (these should still complete in ~79s / ~2min per the current
   steady state — if they're dramatically slower, something in
   your consolidation regressed the gate-speedup chunk's fixes;
   investigate before reporting done).
8. OPTIONAL bundled experiment, only if time permits and only kept
   if it measurably helps: try `rust-lld` via
   `backend/.cargo/config.toml` rustflags
   (`[target.aarch64-apple-darwin] rustflags = ["-C",
   "link-arg=-fuse-ld=lld"]` or the current correct stable-channel
   syntax — verify it's usable on stable without nightly flags
   before committing to it). Time one representative link with and
   without it. If it doesn't clearly help, revert it and say so —
   do not ship an unverified linker change.

## Rules

- Never two db-backed runs at once in this checkout.
- Do NOT touch the running dev-api, Docker, or any process you
  didn't start.
- Do not commit or push — leave the tree uncommitted for review.
- If the `mod common;` fix requires touching more than the 26
  files' `mod`/`use` lines (i.e., if you find yourself needing to
  change `common/mod.rs`'s own content or its signature), that's
  still in scope (it's inside `crm-api/tests/`) — just disclose it
  clearly, it wasn't anticipated as necessary going in.
- If ANY test's behavior seems to depend on being in a separate
  process (isolation assumptions beyond what the no-global-state
  grep found), STOP and report rather than working around it
  silently.

## Report

Files changed matching `git status --short` exactly; the
`mod common;` resolution approach chosen and why; test count
reconciliation (369 total / 363 ignored, before vs after, exact);
the single-file-edit timing comparison (the number that matters
most for this chunk); full gate sanity results; the lld experiment
result if attempted; surprises; any file outside the anticipated
scope that had to change.
