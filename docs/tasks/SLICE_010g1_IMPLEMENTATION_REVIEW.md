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

Both fixes passed targeted database and Web regressions. Round 2 independently
inspected commit `12586c3` and found no remaining actionable code issues in the
reviewed implementation. Evidence: 40/40 focused DB tests and 5/5 Web workflow
tests. No third review round is requested.

## Verification follow-up

Production-Web acceptance subsequently exposed a mixed-family terminal-state
bug: a completed sibling overwrote a confirmed cancellation with bundle state
`completed`, hiding Remainder. Execution now preserves `cancelled` when a
confirmed family was cancelled; unselected/unconfirmed exclusions do not cause
that state. The new partial-completion regression confirms the eligible exact
successor and runs it to completion with a byte audit
(`/private/tmp/010g1-partial-completion-fix.log`, 1 passed). This is a verification
failure fix within the accepted lifecycle, not an additional review round.

The repository gate passed before that small execution fix (1,037 Rust tests,
1,327 Web tests, Clippy, production shape, doctests and supporting checks).
Final compatibility, production-Web and D-050 evidence remain pending; this
record does not establish slice completion or authorize deployment.
