# Slice 008 implementation brief (single lane)

The CONTRACT is `docs/specs/SLICE_008.md` (APPROVED — read it in
full first; every §, including the safe-defaults list and the six
folded review fixes, is binding). Then read `AGENTS.md`,
`docs/decisions/DECISION_LOG.md` D-035 + D-041 + D-027, and
`docs/specs/SLICE_007c.md` (the contract you are extending and
partially superseding — the supersessions are declared in spec §5
and §3.5; make the pointer amendments in SLICE_007c §5 and
SLICE_002 §5 as part of this work).

Branch: `slice-008-round-robin`. Sole migration owner. Working
rules: implement the approved spec only; no unrelated cleanup; the
type-safety invariants are ambient now (typed ids everywhere, bind
`.0`/`as_str()`, `.sqlx` regeneration IS expected this slice for the
new/changed queries — run `./scripts/sqlx-prepare` when query!
macros change, and say so in the report; that differs from the
hardening chunks).

Implementation order suggestion: migration → `db-migrate` →
`IntakeRoutingMode` + rotation.rs (pure fn + take_next) →
`RoutingStrategy::RoundRobin` → determine_routing dispatch →
settings queries/routes → web → tests per spec §7 → gates.

Amendment duties (declared, not silent): SLICE_007c §5 pointer
paragraph (PUT superseded by 008), SLICE_002 §5 pointer (strategy
vocabulary +1), and re-pin any 007c intake-settings PUT tests the
new contract invalidates (the spec scopes which pins may change —
fact/routing/Today pins must stay byte-identical).

Environment: rust workspace at backend/; `source ~/.nvm/nvm.sh`
before `./scripts/check`; `./scripts/check-db` runs the DB suite
(run the full gate yourself); `./scripts/db-migrate` after writing
the migration (crm_dev never auto-migrates); known flake db_calls
"a_second_correction_chains…" — isolate-rerun then full gate if it
trips. Do NOT touch the running dev-api or Docker. Do not commit or
push — leave the tree uncommitted for review.

Report: files changed matching `git status --short` exactly (note
`.sqlx/` WILL legitimately change — list the new/changed cache
files), migration applied to crm_dev confirmation, per-§ delivery
notes, checks run with actual results, the 007c pins you amended
and why each was in the allowed scope, assumptions, surprises.
