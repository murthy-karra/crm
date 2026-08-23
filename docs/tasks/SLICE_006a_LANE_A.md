# Slice 006a — Lane A (sole lane): `crm-app` extraction

Read first: `AGENTS.md`, `docs/specs/SLICE_006a.md` (the whole thing),
DECISION_LOG D-028 §1/§5 and D-030.

Branch: `slice-006a-crm-app` from `main`. One writer. Pure refactor.

## Ownership

Everything under `backend/` except `crm-operator/`, `migrations/`,
`.sqlx/`; `scripts/check-db`; `crm-api/tests/operator_deps.rs` (new
fence only). Docs: PROJECT_STATE, a one-line SLICE_005 §3 note, D-028
§5 status remark.

Do NOT touch: `web/`, `backend/crates/crm-operator/`,
`backend/crates/crm-api/migrations/`, `backend/.sqlx/`, any route
body, any SQL text, any test assertion (only import paths and the two
`Config::from_source` fixture helpers in `raw_payload/crypto.rs` and
`realtime/token.rs`).

## Steps

Follow SLICE_006a §5 in order; run `cargo check --workspace` after each
step. Use `git mv` for the moves so the diff shows renames. Prefer
shims over rewriting import paths in `tests/` and `bin/`; you may sed
in-crate `crate::domain::…` paths to `crm_app::…` if it compiles
cleanly.

If a step needs something the spec does not allow (e.g. a public
signature change beyond `from_config(&TelephonyConfig)` and the four
validating newtype constructors in SLICE_006a §4), STOP and report; do not improvise.

## Required before reporting complete

Every item in SLICE_006a §6, with the actual command output recorded
(counts, empty diffs, grep equality). Source `~/.nvm/nvm.sh` before
`./scripts/check` (web lint needs pnpm).

## Report

Files renamed vs modified (from `git diff main -M50% --name-status`),
the §6 results verbatim, any dep removed from `crm-api/Cargo.toml` and
why, and anything you had to do that the spec did not anticipate.
