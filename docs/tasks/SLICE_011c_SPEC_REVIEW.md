# Slice 011c specification review

Date: 2026-09-06. Base: local `main` at `9d62e86`.
Draft: [SLICE_011c](../specs/SLICE_011c.md).
Brief: [SLICE_011c_IMPL](SLICE_011c_IMPL.md).

## Assignment and result

The actual specification and independent-review agents use `gpt-6-astra` with
`xhigh`; the feasibility agent uses `gpt-5.6-terra` with `xhigh`. This follows
the user's change from ultra to extra high for new work. The reviewer is
read-only. Terra alone owns database execution for the isolated prototype.

The initial independent review returned **READY-WITH-FIXES**. Final independent
review is **READY for approval of the reviewed proposal**: R1–R3 are closed,
with no substantive findings remaining. Measured performance, explicit limits
and the retained pool-headroom caveat support this result. Review is not user
approval or completed implementation acceptance.

After this READY verdict, the user approved the complete specification and
brief on 2026-09-06. Implementation has started under Terra / `xhigh`, with the
same Astra / `xhigh` reviewer assigned to independent implementation acceptance.
The findings below describe the completed specification review.

## Findings and dispositions

| Finding | Concrete issue | Disposition |
|---|---|---|
| C011C-R1 · P1 | The spec required performance evidence before feed implementation, while the brief built the feed before measuring it. | Closed: Phase A completed; original failures and bounded improvements are recorded, with full payload/order parity and explicit HTTP/source/recovery limits. Phase B verifies the actual application after approval. |
| C011C-R2 · P1 | The mounted Operator panel could retain an earlier actor's private list names and replay them as another actor's history. | Closed: spec §6 and AC10 require actor/Organization/session isolation, state clearing and late-callback fences; brief assigns bounded AppShell/OperatorPanel ownership and regressions. |
| C011C-R3 · P2 | An older binary could soft-delete lists without removing source preferences, causing invisible rows to consume the five-source quota after upgrade. | Closed: spec §2 counts live visible enabled definitions; AC2 covers five old-style tombstones, zero visible sources and five successful new enables. No cleanup on reads. |

The reviewer found the common-order prefix selection sound, including complete
reasons for overlapping lists and reservation of built-in work at the cap.
The draft also covers nullable Inquiry/waiting fields, no-contact actions,
caller-owned call outcomes, consistent snapshots, staged source failure and
untrusted list names. These are design-review conclusions, not executed
implementation tests.

## Performance review and remaining implementation gates

The [evidence](../design/perf/slice-011c-2026-09-06/README.md) records the
history-rich 50k-Person fixture and ten-connection pool. The reviewer independently
checked the 272 critical and 210 supplemental complete requests and recomputed
the raw repeat's 40 complete requests / 200 complete source evaluations. It
verified the bounded SQL diffs and preserved ordering/caps, source provenance
and recorded full payload parity. The raw repeat's request p95 was 3,017 ms;
pool wait p95/max was 1,622/1,662 ms.

The earlier 1,967 ms p95 pool wait remains a material limitation against a
2,000 ms timeout. All results are retained. The selected query corrections
support implementation planning, not production capacity or final application
acceptance. Authenticated HTTP, privacy, failure/recovery, source-cap concurrency,
tenant isolation, browser checks and all repository gates remain required by
the specification. The reviewer ran no database commands or tests; cleanup is
supported by Terra's recorded zero remaining databases and sessions.
