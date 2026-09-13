# Mobile 001 — Bounded planning review

**Targeted recheck — 2026-09-12. Verdict: READY FOR IMPLEMENTATION APPROVAL.**
Both first-review findings are resolved in the proposed design. This reviews
[the proposed specification](../specs/MOBILE_001_OFFLINE_FIELD_WORK.md) and
[execution briefs](MOBILE_001_IMPL.md) against D-073 and the existing command,
workspace, Today and revision implementation. Read-only design inspection;
no application changes, tests or native verification were performed. The
recheck was restricted to the two findings below and the added server-owned
context binding; it was not another broad review. READY means the proposal can
be presented for approval, not that approval or implementation has occurred.

**Subsequent user approval:** D-074 accepts this corrected specification and
implementation. The review verdict below is historical; no repeat mobile approval
is pending. No implementation or native-app verification is implied.

## Resolved findings

1. **[P1 — RESOLVED] Coordinate generation promotion with concurrently accepted uploads.**
   The original specification §§4–7 allowed one upload and two downloads
   concurrently and atomic promotion, but did not define when a synced
   operation's local overlay/body can be retired. Concrete race: generation G
   seals; a note creation or task completion then commits and its receipt is
   saved locally; G is promoted afterward. G legitimately lacks that accepted
   note/completion. If acknowledgment retires the pending overlay, the phone
   hides the just-synced note or shows the task open again until another full
   reconciliation. The server result survives, but the local field-work view
   regresses during the next outage. Specify a durable local promotion/write
   fence and/or retain confirmed operation overlays until a sufficiently new
   authorized reconciliation accounts for them. Define how current deletion or
   a newer task revision ends that overlay without resurrecting old state.
   Apply the rule to full bundle replacement, operation replay, crash recovery
   and same-actor multi-device work. Add a deterministic seal → accepted upload
   → delayed promotion test, including restart between receipt and promotion.

   **Resolution:** §§4/6 now capture the enclosing Person revision inside the
   command/receipt transaction, retain durable accepted overlays and serialize
   acknowledgment application with SQLite promotion. Older bundles cannot
   replace newer active versions or retire an overlay before a complete bundle
   covers its receipt revision. Deletion/removal lacking causal evidence retains
   a visible conflict instead of hiding the accepted action. §8 and lane A now
   require the acknowledgment/promotion race proof. These rules cover both
   receipt-before-promotion and receipt-after-a-newer-bundle, including replay;
   runtime tests remain required before a completion claim.

2. **[P2 — RESOLVED] Bound generation admission by server-owned identity, not only an
   installation identifier.** The original specification §4 limited live
   generations to two
   per actor and installation, while expressly declaring the installation UUID
   client-provided and unauthenticated. A caller can rotate that UUID to create
   arbitrarily many 25,000-ID manifests within the 30-minute lifetime. The
   original rule therefore did not bound metadata storage or simultaneous
   selection/seal computation. Add authoritative per-actor/Organization
   admission and concurrency bounds, transactional enforcement, finite database
   execution/wait budgets, and bounded expiry cleanup independent of another
   client request. Keep installation bounds as an additional fairness measure.
   Freeze numeric implementation limits and excess-admission error behavior in
   lane A; these are technical resource bounds, not customer retention policy.
   Verify concurrent requests and identifier rotation cannot bypass them.

   **Resolution:** §4 now caps generations by registered context, trusted
   actor/Organization and Organization, and caps context registration. Capacity
   is reserved under trusted admission locks, exact manifest entries are counted,
   and expired-but-retained rows consume capacity until bounded reclamation.
   A failed cleanup therefore prevents new admission instead of growing retained
   storage without limit. Retryable 429 behavior is explicit, active generations
   are not evicted, and operation markers are excluded from ephemeral cleanup.
   §8/lane A require rotating-identifier and cleanup-failure tests. Lane A must
   still freeze execution/wait budgets, cleanup scheduling/batch limits and lock
   order as part of its exact implementation contract; those ordinary technical
   details do not require a new customer-policy decision.

## Context-binding recheck

Bootstrap now issues a server-owned context binding, and every native sync
request compares it with the current authenticated actor/Organization. The
immutable operation digest includes that context; same-identity renewal keeps
the binding instead of adopting another actor's queued work. The native store
also verifies the context before upload and response application. This closes
the account-switch attribution hazard while preserving server-owned identity,
current permission checks and unchanged queued operation IDs. The exact renewal
fixtures remain in lane A's freeze/verification responsibilities.

## Approval and freeze boundary (at review time)

D-073 already accepts the record-selection direction and seven-day local-access
window. It does not yet accept the proposed conflict, sign-out/revocation,
encrypted-storage/backup, new HTTP or persistence contracts. Present those
corrected concrete proposals for approval. Exact schemas, indexes,
canonicalization vectors, transaction/lock order, trigger fanout, stable page
ordering, bootstrap/lease fixtures and protocol DTOs must then be frozen by the
backend owner before parallel native implementation; future changes outside the
approved declaration return to contract review.

The generation's revision-fenced pages and final consistent selection/version
validation otherwise provide a coherent bounded-reconciliation design without
a generic event log. Resource deletion, authority changes, expired generations
and partial Today results are explicitly prevented from masquerading as a
complete successful download. Operation receipt replay and task revisions
address the important complete/reopen/lost-response failure case while
preserving the existing Web/Operator command behavior.

Customer retention/erasure, independent distribution/support and actual physical
device evidence remain honestly identified later gates. They do not block an
approved, isolated synthetic implementation. No extra customer-policy decision
or production readiness claim is required merely to fix the two technical
findings above. The single targeted recheck is complete; no necessary issue
remains open within its reviewed scope.
