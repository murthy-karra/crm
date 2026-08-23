# Slice 006b — Lane A (backend): propose → confirm → receipt

Read first: `AGENTS.md`, `docs/specs/SLICE_006b.md` (whole), SLICE_005
§3/§5/§9, SLICE_006 §2/§9, D-009, D-028 §1, D-029, D-034.

Branch: worktree lane on the shared `slice-006b-operator-call` branch
strategy set by the coordinator. Ownership: `backend/**`,
`backend/crates/crm-api/migrations/20260827000001_operator_proposal.sql`,
`.sqlx` regeneration, `.env.example` (new TTL var). Do NOT touch
`web/**` or `docs/**`.

Deliverables (SLICE_006b §§2–5, §8–10, §13):
1. Migration + grants exactly as §2 (one file; you own migrations).
2. `ToolBackend::propose_start_call` + views (§3), loop rule one-per-turn,
   tool definition (snapshot updated deliberately — this is the declared
   contract change), prompt edit (§5).
3. `SqlxToolBackend::propose_start_call`: org-scoped person + phone
   lookup; 0 phones → NoPhone; >1 and no id → NeedsNumberChoice; id
   given → validate it belongs to the person (else NotFound); insert row.
4. `POST /api/operator/proposals/{id}/confirm` (§4): claim-then-execute,
   `CommandContext::for_operator` (new envelope helper, origin
   Operator, correlation = turn_id), unchanged `start_call` command,
   finalize row, error mapping incl. new ApiError variants.
5. Fence: operator_deps.rs "+ crm-app forbidden for crm-operator"
   (scripts/check's graph fence already covers it — do not touch).
6. Observability §9; tests §13 (unit + db_operator_call.rs).

Frozen contract: §4 shapes, §2 DDL, tool schema. If any needs to
change, STOP and report — do not improvise. `start_call` stays
consequential (§15): no auto-confirm, no batch, no reuse.

Checks before reporting: `./scripts/check`, `./scripts/check-db`,
`scripts/sqlx-prepare` committed, log-capture test covers new paths.
Report: files, behavior, checks run + results, deviations.
