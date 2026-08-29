# Gate-speedup chunk (local phase 1) — implementation brief

Charter: the QUEUED GATE-SPEEDUP entry in
`docs/plans/PROJECT_STATE.md` (user-approved 2026-08-28). Read it,
plus `AGENTS.md` §14/§15 and both scripts (`scripts/check`,
`scripts/check-db`) before changing anything. This chunk changes
HOW the gates run, never WHAT they cover. Acceptance = a
before/after timing table + provably identical coverage
(reconciled test counts, incl. doctests, per suite).

Branch: `gate-speedup` off `main`. No migrations, no product code.
Allowed surface: `scripts/check`, `scripts/check-db`,
`scripts/bootstrap` (tool install), `backend/Cargo.toml` (profile
only), the three 501-sequential-INSERT truncation tests
(db_people.rs, db_people_filter.rs cap test, the sources
truncation test) — batched via `generate_series`/single-statement
inserts, behavior-identical assertions. NOTHING else. If a lever
needs anything outside this list, STOP and report.

## Order of work

1. **Timed BASELINE first, before any change**: run `./scripts/check`
   then `./scripts/check-db` once each with per-step wall-clock
   timing (wrap each step in `time` or timestamp echoes — a
   temporary local measurement harness is fine, but the committed
   scripts may keep permanent lightweight per-step `==> [Ns]`
   timing output if clean). Record every step's seconds. Machine
   conditions: leave the user's running dev processes (vite,
   dev-api, Docker) alone; both baseline and after runs happen
   under the same ambient load.
2. **Lever 1 — cargo-nextest** (biggest win): install via
   `cargo install cargo-nextest --locked` (and add an install line
   + presence check to `scripts/bootstrap`; the gate scripts must
   fail with a clear "run ./scripts/bootstrap" message if nextest
   is absent). `check`'s test step becomes
   `cargo nextest run --workspace --locked` **PLUS a separate
   `cargo test --doc --workspace --locked` step — NEXTEST DOES NOT
   RUN DOCTESTS and the ids.rs compile_fail doctests are
   load-bearing type-safety pins; dropping them is silent coverage
   loss and an automatic fail of this chunk.** `check-db`'s suite
   becomes `cargo nextest run --workspace --locked --run-ignored
   only` (flag spelling per the installed nextest version). Verify
   sqlx per-test databases behave under process-per-test
   parallelism (they should — each #[sqlx::test] owns its DB);
   if Postgres connection limits bite, cap nextest's jobs rather
   than serializing.
3. **Lever 2**: `[profile.dev] debug = "line-tables-only"` in
   `backend/Cargo.toml` (test profile inherits). Full rebuild
   follows once; that cost lands in your run, not the timings
   (do a throwaway warm build before the AFTER timing run).
4. **Lever 3**: restructure `scripts/check` so the web half (lint,
   typecheck, vitest, build) runs concurrently with the rust half:
   background subshell to a log file, `wait` on the PID, cat the
   log, propagate failure correctly under `set -euo pipefail`
   (test the failure path deliberately: break a web file, confirm
   the gate fails loudly, restore). Keep output readable (rust
   block then web block, never interleaved).
5. **Lever 4**: batch the three 501-row-insert tests. Assertions
   byte-identical; only the fixture setup changes.
6. **Lever 5**: scope `cargo sqlx prepare --check` to the two
   query-bearing crates IF the installed sqlx-cli supports package
   filtering cleanly; otherwise skip and record (smallest lever —
   do not fight the tool). `crm-operator` is sqlx-fenced, so
   coverage is unchanged by construction; say so in the report.
7. **sccache: do NOT implement** (recorded-optional pending
   measurement; it trades away local incremental compilation).
8. **Timed AFTER run**: full `./scripts/check` + `./scripts/check-db`
   from the changed scripts, same per-step timing, warm build.
9. Reconcile coverage: before/after counts per suite — nextest
   unit+integration counts, doctest count from the --doc step,
   web test count, worker tests. Any count that shrinks must be
   explained line-by-line or the chunk fails.

## Rules

- Never two db-backed runs at once; you own the checkout while
  you work. Do NOT touch the running dev-api or Docker; kill no
  processes. Do not commit or push — leave the tree uncommitted.
- The scripts' step ORDER and failure semantics are contract-ish
  (AGENTS §14 leans on them): every existing step still runs (or
  is provably subsumed), failures still stop the gate, no step
  becomes best-effort.
- The known db_calls ordering flake may CHANGE behavior under
  process-per-test isolation (likely improves). If it trips,
  isolate-rerun and note it; do not chase it.

## Report

Files changed matching `git status --short` exactly; the timing
table (per step, before/after, and the pair totals); coverage
reconciliation table; the nextest/sqlx-cli versions installed;
levers skipped with reasons; failure-path test evidence for the
parallel-check restructure; surprises.

---

## RESUME STATE (written 2026-08-28 ~20:20, session ended by VS Code restart)

Implementation COMPLETE on branch `gate-speedup` (UNCOMMITTED — do
not discard). The in-flight proof run was deliberately killed
(exact PID, SIGTERM/RC=143 — the "external termination" in the
lane's final addendum was this, nothing mysterious) at the user's
request before a VS Code restart to apply the macOS Developer
Tools exemption. That run's step-2 recompile was the EXPECTED
one-time echo of the last pre-isolation run's fingerprint clobber
plus the new backend/.cargo/config.toml landing mid-sequence — the
zero-Compiling proof intentionally belongs to the NEXT run.

LANDED: nextest for both test steps + separate `cargo test --doc`
(ids.rs compile_fail doctests preserved, 5/5); `[profile.dev]
debug = "line-tables-only"`; web half of check runs concurrently
(failure path proven); the three named 501-INSERT tests batched
via generate_series; SQLX_OFFLINE=true unification on test
compiles (DATABASE_URL runtime-only); prepare --check moved to an
ISOLATED CARGO_TARGET_DIR (it was clobbering step 2's fingerprints
every run — the root cause of per-run recompile+relink). SKIPPED
(recorded): prepare --check package-scoping (sqlx-cli 0.8.6 has no
-p), sccache (deliberate).

MEASURED: ./scripts/check 2096s → 39s (~54x); coverage 1324 → 1324
tests, zero shrinkage (647 fast + 363 db + 5 doctests + 300 web +
9 worker). DB suite executes in 117–160s under nextest (three
Summary lines). End-to-end check-db was still ~35 min because
freshly-relinked test binaries pay macOS first-launch assessment
(~2000s silent gap before nextest output; cached-binary rerun =
123s, no gap). Both repo-side causes of pointless relinks are now
fixed; the machine-side scan cost is addressed by the user's
Developer Tools exemption (enabled 19:21 2026-08-28 for iTerm/
Terminal/VS Code/Claude — requires app relaunch, hence this
restart).

TO CLOSE THE CHUNK (next session): (1) ONE full ./scripts/check-db
— expect step 1 ~25s (isolated dir warm), step 2 with ZERO
"Compiling" lines and ~2-3 min tests, total ~3-4 min; that run is
simultaneously the steady-state number, the fingerprint-stability
proof, and the exemption verification. If step 2 still recompiles,
capture CARGO_LOG=cargo::core::compiler::fingerprint=info once and
report — no open-ended diagnosing. (2) One ./scripts/check
(~39s expected). (3) Review the diffs (scripts/check, check-db,
bootstrap; backend/Cargo.toml; the two batched test files), then
commit gate → merge gate (user approvals). (4) Completion sync
must record: the three 011a plain-language docs were authored by
the user's crm-db session AT THE USER'S REQUEST (authorship
closed); crm-db held a STAGED doc-only patch fixing SIX gaps in
SLICE_011a_EXPLAINED.md (code-verified, symbols cited). POST-MERGE
NOTE (2026-08-28 ~20:55): the crm-db session did NOT survive the
VS Code restart (socket gone, no live peers), so the free-signal
was undeliverable. To land the patch: resume that session if its
transcript survives, or have any session redo it — the six gaps:
(1) §4b canonical-uuid-only wire forms, (2) §4b duplicate-JSON-
keys fail closed anywhere, (3) §6 drop-and-clear degrade is
URL-origin-only and 400/422-only — 5xx keeps chips+URL (the
walkthrough's one true correctness defect), (4) §6 back/forward
re-rehydrates via a route.query.filter watcher (loop-guarded by
lastWrittenFilterParam), (5) §6 empty-array draft clauses never
serialize (fetch on first committed value; degrade gate also
requires ≥1 committed clause), (6) §6 zero-clause decodable
filters also canonicalize the URL param away (all four
rehydrateFromUrlValue cases clear it). (5) Residuals: a fourth
501-INSERT test (db_people.rs unresolved-queue truncation) left
unbatched per scope; the killed runs may have left stray sqlx
ephemeral test databases in local Postgres (harmless; drop at
leisure); the lane's full timing tables live in this session's
transcript — the numbers above are the durable summary.
