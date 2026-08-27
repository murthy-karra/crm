# Hardening chunk V1 — `NormalizedEmail` / `NormalizedPhone`

Parent plan: `docs/design/type-safety-hardening.md` (chunk 7).
Branch: `hardening-v1`, worktree `/Users/karrad/projects/crm-wt-v1`
— a PARALLEL LANE: chunk N4 runs simultaneously in another worktree.
Single writer within this lane. Read `AGENTS.md` first, then the
predecessor briefs (`docs/tasks/HARDENING_{S1,N1,N2,A1,N3}.md`) —
invariants verbatim: no SQL string changes (`.sqlx` byte-untouched
with proof), no wire changes, no migrations, crm-operator untouched,
no unrelated cleanup.

## LANE OWNERSHIP (hard boundary — parallel-lane safety)

You may edit: `domain/contact.rs`, `domain/inquiry/parse.rs`,
`domain/intake/email/format.rs` (and `formats/*` if signatures
force), `domain/intake/extraction/mod.rs` (ValidatedLead area only),
`domain/person/queries.rs` (ONLY the contact-method fns:
`upsert_contact_methods`, `identify`-related), plus their tests.

You must NOT edit: `ids.rs`, `facts.rs`, `envelope.rs`, realtime
`events.rs`/`publisher.rs`, telephony anything, `receive_inquiry.rs`,
any crm-api route or operator file, `extraction/worker.rs` — those
belong to the N4 lane or are shared. If your change compile-forces an
edit in any of them, STOP that edit and report it as a
shared-contract conflict instead of making it. (Design so it
doesn't: keep `ParsedLead`'s FIELD types and all public fn
signatures that cross out of your ownership IDENTICAL — see below.)

## Goal

`identify(conn, org, email: Option<&str>, phone: Option<&str>)`
(contact.rs) takes two adjacent same-typed options; a transposition
compiles today and silently breaks dedup → duplicate Person. Make
the normalized contact values TYPED so that swap dies:

- `NormalizedEmail(String)` / `NormalizedPhone(String)` in
  `contact.rs` (NOT ids.rs — different concept, and ids.rs is the
  other lane's file). Constructible ONLY via `normalize_email`/
  `normalize_phone` (now returning `Option<NormalizedEmail>` etc.);
  private inner field, `as_str()` accessor, Display delegating,
  Debug REDACTED (these are PII — mirror ParsedMail's Debug
  discipline: never print the value; a redaction unit test like the
  config-secret ones).
- `identify(..., email: Option<&NormalizedEmail>, phone:
  Option<&NormalizedPhone>)` — the swap now fails to compile.
- `upsert_contact_methods`' internals consume the typed values.

## The lane-safety design constraint (critical)

`ParsedLead` (parse.rs) is consumed by `receive_inquiry.rs` and
`extraction/worker.rs`, which you may NOT edit. Therefore
`ParsedLead`'s public field types must NOT change in a way that
forces edits there. Recommended shape: keep
`ParsedLead.email/phone: Option<String>`? NO — check first: grep how
receive_inquiry.rs/worker.rs actually ACCESS those fields. If they
only pass `&ParsedLead` through to your owned fns, you may type the
fields (`Option<NormalizedEmail>`) freely. If any un-owned file reads
`.email`/`.phone` directly, keep the field a `String` and add typed
accessor methods instead — choose whichever keeps the un-owned files
byte-identical. State which case you found in your report.

Producers you own and must update: `parse.rs`'s `parse()`,
`format.rs::to_parsed_lead`, `ValidatedLead::to_parsed` (+ the
anti-hallucination re-normalization in extraction/mod.rs which calls
normalize_* on candidate strings — adapt to the new return types).
Raw (pre-normalization) values stay plain `String`s everywhere
(`raw_email`, `raw_phone`, `email_raw` etc.) — only the NORMALIZED
values get types.

## Checks before reporting done

Worktree specifics: `pnpm install --frozen-lockfile` in `web/` first
if the check script's web steps fail on missing node_modules. Then
`cargo fmt`, `source ~/.nvm/nvm.sh && ./scripts/check`. Do NOT run
`./scripts/check-db` (parallel lane; the coordinator runs it serially
per lane). Instead run affected suites directly: `source .env &&
DATABASE_URL="$MIGRATION_DATABASE_URL" cargo test -p crm-api --test
db_people --test db_intake --test db_inbound_email_intake --test
db_intake_extraction --test db_intake_workbench -- --ignored` (plus
any suite your diff touches). `git status --short backend/.sqlx/`
must be empty.

Report format as predecessors — and explicitly confirm you never
edited the N4-owned/shared files, and which ParsedLead case (typed
fields vs accessors) you found and why.
