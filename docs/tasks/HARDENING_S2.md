# Hardening chunk S2 — fact vocabulary, AttemptOutcome, IntakeToken, Publication (+ carry-overs). FINAL CHUNK.

Parent plan: `docs/design/type-safety-hardening.md` (chunk 8, plus
its "Flagged for sign-off" item 2 and the S1-review carry-over baked
into the chunk row). Branch: `hardening-s2`. Single lane, one
writer. Read `AGENTS.md` first, then ALL predecessor briefs
(`docs/tasks/HARDENING_*.md`) — invariants verbatim: no SQL string
changes (`.sqlx` byte-untouched with proof), no wire changes, no
migrations, crm-operator untouched, no unrelated cleanup.

## Work items

1. **Typed fact fields** (`facts.rs`): fields typed with their
   EXISTING enums — `RoutingDecisionFact.strategy: RoutingStrategy`,
   `ContactAttemptedFact.channel: ContactChannel` / `.outcome:
   ContactOutcome`, membership/invitation facts' `role:
   Role`/`from_status`/`to_status: MembershipStatus`, any
   `matched_by` → `ContactKind`. The `reason: &'static str` field(s)
   get a micro-enum (e.g. `RoutingReason { Intake, Manual }` — read
   the actual values used before naming variants; if only ONE value
   is ever passed, still enum it with that one variant and say so).
   Bind positions stay intact via `.as_str()` — same line-audit
   discipline as every predecessor. Construction sites pass the enum
   values their callers already hold (most callers already own the
   typed value and stringify at the call site today — DELETE the
   stringify, pass the value).
2. **`AttemptOutcome` enum** (`extraction/worker.rs`): replaces the
   `&'static str` ledger outcome tags. Variants for every existing
   tag (extracted, superseded, not_a_lead, email_extraction_failed?,
   provider_timeout, provider_unavailable, rate_limited, intake_busy,
   malformed_response, schema_invalid, low_confidence,
   hallucinated_contact, no_contact_method, internal_error — READ the
   code for the true set; do not invent or drop any). `as_str()`
   yields the exact ledger strings; `is_transport()` replaces the
   string-list match at the retryability decision so adding a
   transport variant without classifying it is impossible
   (exhaustive match, no wildcard arm on the retryability path).
   Ledger inserts bind `.as_str()` — byte-identical rows.
3. **`IntakeToken`** (SECURITY-SENSITIVE — the one flagged item):
   newtype over the token `String` in `domain/intake/` (address.rs /
   receive.rs / rotate.rs / admin queries surface). Requirements:
   - Debug redacted (`IntakeToken(REDACTED)`, pinned by a test);
     Display NOT implemented (unlike NormalizedEmail — a token must
     never flow through format!; provide `reveal() -> &str` for the
     exactly-two legitimate render sites: the org settings/address
     response and the rotate response; grep to confirm the real
     count and name them in your report).
   - `verify(&self, candidate: &[u8]) -> bool` wrapping the EXISTING
     constant-time comparison — constant-time becomes the ONLY
     equality (no PartialEq derive!). The receive.rs auth path,
     including the dummy-token timing equalizer for unknown slugs,
     must be byte-for-byte behaviorally identical: same comparisons,
     same order, same dummy value. Quote the before/after of that
     path in your report.
   - Minting (`admin/validation.rs`) returns `IntakeToken`;
     `rotate_intake_token` returns it; row reads wrap at the
     boundary.
4. **Constructor-only `Publication`** (`realtime/events.rs`):
   `channel` field private; `Publication::for_event` stays the only
   way to build one — a cross-org publish (channel for org A, event
   for org B) becomes unwritable outside the module. Adapt any
   direct-field constructions (tests included) to the constructor;
   if a test NEEDS a mismatched publication to test something, it
   belongs inside the module's own tests.
5. **S1 carry-over — `Decode → Corrupt`**: in the `From<sqlx::Error>`
   impls for `CommandError` and `WorkbenchError` (find the actual
   impls), map `sqlx::Error::Decode` to the existing `Corrupt`
   variant so a row-boundary decode failure surfaces as the 500-class
   `internal_error` instead of a 503 — matching the established
   `CallError` precedent (commands/mod.rs). Adjust/add the minimal
   test pinning the mapping.

## Checks before reporting done

`cargo fmt`, then `source ~/.nvm/nvm.sh && ./scripts/check` and
`./scripts/check-db` (no parallel lane this time — run the full db
gate yourself); `git status --short backend/.sqlx/` must be empty.
Known flake: db_calls "a_second_correction_chains…" — isolate-rerun
then full gate if it trips. Report format as predecessors, PLUS the
quoted before/after of the receive.rs token-compare path (item 3).
