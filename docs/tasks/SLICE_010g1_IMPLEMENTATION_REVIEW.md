# 010g1 implementation review

Independent read-only review, round 1, 2026-09-16. Scope: accepted combined
family-refresh plan and contract, including the uncommitted Remainder and legacy
baseline adapters. One implementation reviewer; no additional writer lane.

## Round 1 — changes required

1. **P1 / TRUST:** Confirm acquired shared workspace admission before installing
   the durable compatibility requirement. It must take the exclusive workspace
   barrier before membership/storage locks, draining older admitted transactions.
2. **P2 / BOUNDARY:** Web required the entire bundle to be ready, preventing a
   ready family from being confirmed while a sibling was preparing or paused.
   Permit the server-supported preparing/ready states while retaining exact
   selected-plan readiness, expiry, digest and acknowledgement checks.

Both fixes are implemented. Targeted database and Web regressions are being
verified. No further concrete findings were established in inspected legacy
reconstruction, Remainder, inherited prerequisites, accounting/readiness,
execution, history projection or API paths.

Final-tree checks, serial database compatibility coverage, production-Web
acceptance and D-050 performance evidence remain pending. This is not a completed
slice or deployment approval.
