# Slice 011a implementation brief (single lane)

The CONTRACT is `docs/specs/SLICE_011a.md` (reviewed
READY-WITH-FIXES, all fixes folded — read it in full first; every
§, including §10's safe defaults and the folded review fixes
F1–F5, is binding). Then read `AGENTS.md`,
`docs/plans/SLICE_011_LADDER.md` (the 011a row + standing
tensions), `docs/decisions/DECISION_LOG.md` D-043 (+ D-019, D-027,
D-042, D-006 for context), and `docs/specs/SLICE_002.md` §5/§6
(the contract you are additively amending).

Branch: `slice-011a-filter-vocabulary` off current `main`. NO
migration exists in this slice — if you believe you need one, STOP
and report instead. Working rules: implement the approved spec
only; no unrelated cleanup; typed-id invariants are ambient (bind
`.0`, no implicit conversions); `.sqlx` regeneration IS expected
for the NEW queries — run `./scripts/sqlx-prepare` after adding
`query!`/`query_as!` sites — but every EXISTING cache entry
(above all `list_summaries`) must stay byte-identical; verify
with `git diff --stat backend/.sqlx` and say so in the report.

Key spec points implementers have historically gotten wrong —
all binding:

- Absent `?filter=` calls the UNTOUCHED `list_summaries` — do not
  refactor the legacy path onto the filtered query (§5a).
- `filtered_summaries` is ONE static `query_as!` string (fixed
  matrix, NULL-guarded predicates, §4e binding convention:
  clause absent → NULLs; present → non-NULL possibly-EMPTY
  arrays; org boundary literal; every lateral/subselect carries
  `AND x.organization_id = p.organization_id`).
- `me` resolves server-side AFTER validation, appended to the
  bound user array; never a wire value reaching SQL as a token.
- Error order 401 → 400 → 422 per §5a; every filter failure in
  the `{"error": code}` envelope, never axum's bare rejection.
- The §7 F1 trace-layer change (custom `make_span_with`, path
  only, query stripped, ALL routes) is in scope and required.
- Sources: reject non-canonical, never normalize (§4b).
- Web: query-key element OMITTED when unfiltered (§6); invalid
  and server-rejected URL filters both drop+clear (§6).

Amendment duties (declared, not silent): SLICE_002 §5 pointer
amendments — the `filter` param note on the `GET /api/people` row
and the new `GET /api/inquiry-sources` row, in the existing
in-place italic amendment style (007e/008 precedent).

Suggested order: `filter.rs` (types + serde + validate +
describe, unit tests) → `filtered_summaries` + sources query →
routes (filter parse/envelope, error order, span fields, F1
trace-layer change) → `sqlx-prepare` → db tests
(`db_people_filter.rs` + sources; harness per `db_people.rs`,
correspondence fixtures per `db_today_client_replied.rs`) → web
(FilterBar, URL sync, key factory, `useInquirySources`, Vitest)
→ full gates.

Environment: rust workspace at `backend/`; `source ~/.nvm/nvm.sh`
before `./scripts/check`; `./scripts/check-db` runs sqlx
prepare-check + the DB suite against a throwaway DB (run BOTH full
gates yourself, serially — never overlap two db runs in one
checkout). Known flake: db_calls
"a_second_correction_chains…" — isolate-rerun then full gate if
it trips. Do NOT touch the running dev-api or Docker; kill no
processes. Do not commit or push — leave the tree uncommitted for
review.

Report: files changed matching `git status --short` EXACTLY (list
the new/changed `.sqlx` cache files; confirm existing entries
untouched), per-§ delivery notes, every acceptance criterion
(§8 items 1–13) mapped to its test name(s), checks run with
actual results, any spec deviation (disclose, don't improvise),
assumptions, surprises.
