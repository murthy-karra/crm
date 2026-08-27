# Hardening chunk A1 — `Actor` enum (+ N2 residual)

Parent plan: `docs/design/type-safety-hardening.md` (chunk 4).
Branch: `hardening-a1`. Single lane, one writer. Read `AGENTS.md`
first, then the merged predecessors' briefs
(`docs/tasks/HARDENING_{S1,N1,N2}.md`) — their invariants apply
verbatim: no SQL string changes (`.sqlx` byte-untouched, prove it),
no wire changes, no migrations, crm-operator untouched, no unrelated
cleanup.

## Goal

1. `FactEnvelope` currently carries the raw pair
   `actor_kind: ActorKind` + `actor_user_id: Option<UserId>` as pub
   fields, literal-constructed in 7 files. `(System, Some(user))` and
   `(User, None)` are representable today and only the DB CHECK
   `(actor_kind='user')=(actor_user_id IS NOT NULL)` (migration
   20260821000004) catches them — as a runtime 500. Replace the pair
   with a single `actor: Actor` where
   `enum Actor { User(UserId), System }` — the invalid combinations
   become unrepresentable (the `SenderTrust` "no capacity for it"
   house style). The DB CHECK stays as defense-in-depth.
2. N2 residual (ladder doc "Recorded residuals"): give
   `determine_routing` a small params struct (named fields for
   `assign_to_user_id` and `current_assignee`) so its two same-role
   `Option<UserId>` params stop being positionally transposable.
   Internal-only; pick a minimal shape that reads well at the two
   sites (definition + single call site).

## Design constraints

- `Actor` lives in `domain/envelope.rs` beside `ActorKind`. Keep
  `ActorKind` itself (the DB vocabulary enum) — `Actor` maps onto it.
- Give `Actor` (or keep on `FactEnvelope` as delegating methods)
  accessors `kind() -> ActorKind` and `user_id() -> Option<UserId>`
  so the 10 `facts.rs` insert fns and `rotate.rs` change one line
  each (`envelope.actor_kind.as_str()` →
  `envelope.actor.kind().as_str()` or equivalent) and bind lists
  keep their exact positions — same line-audit discipline as N2.
- `on_behalf_of_user_id` stays a free `Option<UserId>` field —
  System-with-on-behalf is legal (SLICE_007e §4); do NOT restrict it.
- The 7 literal-construction sites (admin commands ×5-ish,
  telephony/settle.rs, commands/correct_call_outcome.rs) plus
  `FactEnvelope::for_system`, `IntakeActor::envelope`,
  `CommandContext` construction — update to construct `Actor`
  directly. If `CommandContext` itself carries kind+user separately,
  migrate it to `Actor` too (same invalid-state argument); follow
  the code, not this sentence.
- Envelope tests pin the valid combos today — adapt mechanically; add
  one test asserting the accessors' kind/user_id mapping for both
  variants. A compile_fail doctest is unnecessary (enum shape is the
  proof); skip unless trivial.

## Checks before reporting done

`cargo fmt`, then `source ~/.nvm/nvm.sh && ./scripts/check` and
`./scripts/check-db`; `git status --short backend/.sqlx/` must be
empty. Same report format as N1/N2 (files changed matching
`git status` exactly, construction sites converted, bind-list
integrity statement, checks + results, assumptions, surprises).
