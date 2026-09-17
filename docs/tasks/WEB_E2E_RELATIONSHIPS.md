# Web E2E — Relationship context (catalog 05)

Status: implemented; all ten steps passed in run `d5a97d898038` with cleanup `verified_empty`. Initial authoring failures and corrections remain in earlier run artifacts. Final combined-suite evidence follows below.

Owned files: `e2e/families/relationships.spec.mjs`, `e2e/support/relationships-seed.mjs`, this record. No application contracts or application code changed.

Authority: AGENTS.md; D-015, D-023, D-050, D-051, D-053, D-058 and O-012; SLICE_011e, SLICE_015, SLICE_019; reviewed journey catalog 05a–05c. Definition changes intentionally have no realtime publication: observers explicitly reload for those changes, while Person mutations must produce real Centrifugo invalidation followed by an authoritative GET.

Seed `relationships-v1` reuses operational-family bootstrap: two Organizations, four active members, nine stages each, then one synthetic Rowan Person through ReceiveInquiry. No SQL business mutations. The browser creates the note, tag and four field definitions. The read-only audit role verifies PostgreSQL state.

Ten dependent steps cover:

1. Independent authenticated browser contexts and clean relationship state.
2. Author note creation/edit, member observation, real invalidation/refetch, persisted attribution.
3. Non-author edit and foreign deletion denied; admin edits/deletes; tombstone body erased and absent after reload.
4. Inline tag creation/application; normalized create-or-get returns the same definition; repeated application has no duplicate link.
5. Used-tag creator denial, admin rename, link removal, creator rename/delete once unused.
6. Admin creates text, number, date and single-choice fields through the Web.
7. Member sets all four and edits text; observer receives updates; database retains typed values.
8. Member definition creation, invalid number/type and foreign Person writes denied without corrupting existing state.
9. Field and held-option archive/restore preserve values; archived writes are refused; existing held option remains readable.
10. Clear a date through the Web; fresh observer login verifies durable state; no new immutable facts/contact credit; Organization-scoped realtime and mock evidence; no external calls; exact Person set.

API calls supplement UI actions for requests that the UI deliberately does not permit (authorization/type errors), normalized create-or-get and idempotent re-application. They use the actor's real authenticated browser-context request client and record sanitized HTTP evidence. They do not fabricate CRM responses.

Limits: this family does not cover imported records/review holds, Operator reads, filter references to deleted tags or archived fields (saved-list family owns filtering), quota boundaries, concurrent note editing, or provider integrations. It does not claim full production erasure/backups verification. Artifacts contain synthetic business content only. Each family runs in its own database/network and reuses cached images; video, trace and diagnostic handling reuse the shared harness.

Checks run: `node --check e2e/families/relationships.spec.mjs`; `node --check e2e/support/relationships-seed.mjs` — passed. Coordinator owns Docker execution and final runtime evidence.

Observed accessibility limitation: FieldsView renders its new-field Label through FormField but does not bind the slot ID onto the input. The E2E uses the existing `new-field-label` test ID; this does not verify accessible label association. No application fix is included.

## Final combined verification

Run `949f3e952fb3` passed all six families (61 steps), with three browser families
active simultaneously, all images reused and every environment removed. See
[expansion verification](WEB_E2E_EXPANSION.md) for authoritative evidence and the
shared recorder fixes discovered during parallel execution.
