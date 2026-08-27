# Slice 009 implementation brief (single lane)

The CONTRACT is `docs/specs/SLICE_009.md` (APPROVED — read it in
full first; every §, including the twelve folded review fixes, is
binding — the future-Date clamp, digest token lookup,
terminal-status PII nulling, the forward-dedup message_id rule, and
the link/transition matrix are all load-bearing, not commentary).
Then read `AGENTS.md`, `docs/decisions/DECISION_LOG.md` D-042 +
O-014 (both 2026-08-27 blocks) + D-038/D-040, and skim the
predecessor patterns you will reuse: `docs/specs/SLICE_007g.md`
(token newtype/rotation/fact-table templates — code in
`domain/intake/{address,rotate}.rs`, migration `20260902000001`),
`SLICE_007h1.md` (`email/forward.rs` unwrapper you are extending),
`SLICE_007b.md` (the frozen endpoint you must NOT change).

Branch: `slice-009-correspondence-capture`. Sole owner of migration
`20260904000001`. Working rules: implement the approved spec only;
no unrelated cleanup; typed ids everywhere (ids.rs has the full
set); `.sqlx` regeneration expected (run `./scripts/sqlx-prepare`
after new query! macros and report the cache delta); run
`./scripts/db-migrate` after writing the migration (crm_dev never
auto-migrates).

Suggested order (compile-guided within each):
1. Migration (spec §4, all five items + grants) → db-migrate.
2. `mime.rs` additive fields + `forward.rs` extensions (spec §7 —
   continuation folding and the inner-Date parser have existing
   fixture coverage to extend; keep the fence tests green).
3. `CaptureToken` + address mint/parse/rotate (spec §3 — mirror
   IntakeToken/rotate.rs; the digest `token_lookup` column is the
   lookup path; disjointness unit tests BOTH directions incl. the
   hyphenated-`save-*` slug edge).
4. Capture pipeline (spec §5: store-raw-first, ladder, clamp,
   dedup, auto-attempt via `facts::insert_contact_attempted` +
   `FactEnvelope::for_system` — NOT the log_contact_attempt command,
   which hardcodes now()).
5. Today arm + realtime variant (spec §6, incl. the precedence and
   TryFrom third-arm consistency).
6. Endpoints (spec §8) + held-queue semantics (spec §4.4 transition
   matrix).
7. Web (spec §8: new view, timeline, Today label, TS types) —
   match existing view/Vitest patterns.
8. Declared pointer amendments: SLICE_002 §5, SLICE_003 §5 and §6,
   SLICE_004 (minting), carried as small edits to those spec files.
9. Tests per spec §10's twelve criteria + §9's isolation/tracing
   pins; then `cargo fmt`, `./scripts/check`, `./scripts/check-db`.

Size note: this is M–L — bigger than any single 007 rung. Work it
to completion; if you genuinely cannot finish in one session,
STOP at a clean seam (compiling, gates green on what exists), and
report exactly where the next session resumes — do not leave a
broken tree. Do not exercise any spec §12 trim lever (each requires
a D-042 amendment — coordinator/user territory).

Environment: backend/ workspace; `source ~/.nvm/nvm.sh` before
`./scripts/check`; run the FULL `./scripts/check-db` yourself
(nothing else is using the DB); known flake db_calls
"a_second_correction_chains…" — isolate-rerun then full gate if it
trips; NEVER run two db suites/gates concurrently in this checkout.
Do NOT touch the running dev-api or Docker outside the scripts.
Do not commit or push — leave the tree uncommitted for review.

Report (per predecessor format): files changed matching
`git status --short` exactly (incl. the `.sqlx` delta), migration
applied to crm_dev, per-§ delivery notes with the ladder/clamp/dedup
implementations quoted where subtle, the pointer amendments made,
every check run with actual results, assumptions, surprises.
