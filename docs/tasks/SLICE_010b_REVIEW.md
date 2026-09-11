# Slice 010b — Planning review

Updated 2026-09-11. **REVIEW READY; USER APPROVED IMPLEMENTATION (D-063).**
Inputs: [spec](../specs/SLICE_010b.md), [brief](SLICE_010b_IMPL.md),
[source qualification](../research/SLICE_010b_FUB_SOURCE_CONTRACT.md), current
migration implementation and decisions D-012–016, D-050, D-059–062.
Application baseline is `0735015`; published documentation baseline is `a498a2c`.

The user requested commit/push of the existing documentation, then a return to
010b. Main `a498a2c` was pushed successfully before these draft revisions began.
The original core scope remains accepted; this review does not approve new
contracts, implementation, source access, import or deployment.

## Coordinator findings and revised disposition

A prior coordinator review identified these four issues. A bounded read-only
source/contract consultation confirmed their consequences; that consultation
was not a full independent readiness review of the revised spec.

| ID | Severity / original problem | Revised disposition |
|---|---|---|
| 010b-R1 | P1 — Raw-byte differences conflated expected note list/detail enrichment with drift | §3 separates source identity, representation and semantic variant; §5 handles detail/list provenance and only qualified comparable conflicts. Raw bytes remain untouched. §8 adds format, large-number, enrichment and conflicting-variant cases. |
| 010b-R2 | P1 — Source-initiator/current-credential rules also blocked promised retained reads | §4 authority matrix separates source work from current-admin retained reads/cancel/budgets and DB-only previews. Source credentials remain frozen; preview retry fences previous execution. Preserve the different existing 010a retry behavior. |
| 010b-R3 | P2 — Frozen allowances had no counting definition or recovery path | §4 defines logical payload accounting/reservations and the distinction from physical disk usage. Add audited monotonic increases within operator ceilings and explicit source/preview resume. Original proposal budgets survive. Allowance values and delegated authority remain proposed. |
| 010b-R4 | P2 — Denied content could mean either complete-with-gaps or permission pause | §4 closed matrix qualifies note-detail 404 as a negative item receipt, not a success/deletion. Collection/unqualified denial pauses; evidence/checkpoint settlement is atomic. Successful enumeration and gap counts remain distinct. |

No new policy is silently accepted by marking an ambiguity corrected. In
particular, D-050 remains the supported performance envelope; removal of the
draft record-count stops is a proposal about capture admission, not a claim
that larger books are performant. D-062 does not relocate 010b's core evidence.

## Independent review and verification

- Independent review of the complete revised spec/brief returned
  **READY-WITH-FIXES**. Its two findings were corrected; the second, targeted
  pass returned **READY**, confirming both resolved and no introduced blocker.
  This is not implementation approval. No additional review pass was run.

| ID | Severity / finding | Correction and required implementation proof |
|---|---|---|
| 010b-IR1 | P1 — Destination timestamp/fingerprint and mutable coverage cannot reproduce a report after interruption | §4/§5 now persist encrypted actual destination inputs and frozen coverage plus an immutable source sequence boundary before generation. Input/comparison versions are pinned and input bytes budgeted. §8 requires destination edits/source retries during interruption to leave old report results unchanged and correctly stale. |
| 010b-IR2 | P2 — Shared contacts could expand into unbounded or quadratic overlap candidates despite record pagination | §5/§6 now represent shared keys as groups with precomputed counts and separately paginated group lists/members, all scoped to frozen report input. No all-pairs storage; §8 covers a shared office phone within D-050, complete navigation, bounded pages and query shape. |

The budget stop is intentional: existing summaries/reports remain readable, but
the first content preview may need more allowance. The draft now states that
limit explicitly instead of implying a raw-content inspection endpoint exists.

- Documentation checks: **PASS** — 47 local paths/anchors across the six revised
  documents; tracked and new-file whitespace checks; documentation-only scope.
- Application, database, browser and performance checks: **NOT RUN**; there is
  no 010b implementation. Existing 010a evidence is not 010b test evidence.
- Live FUB validation: **USER-DEFERRED**. No source call/customer record was used.

## Subsequent approval

After this review, the user said “approved...lets move on to implementation”.
D-063 records acceptance of the specification/brief and the allowance/delegation
policy below. The review observations above are historical planning evidence;
implementation and its synthetic checks are now authorized. No deployment or
live source validation is implied.

### Policy presented for approval (now accepted)

The concrete storage proposal starts synthetic development at 2 GiB/run and
4 GiB retained/Organization. Deployment operators set maximum ceilings; current
Organization admins can confirm increases within those ceilings without starting
work. Resume is a separate authorized action. These values/delegation were
accepted with the specification; no production customer quota or retention
period is chosen. The operator-only alternative was not selected.

The spec/brief and declared contracts now have acceptance under AGENTS §11.
FUB/customer-data prerequisites stay
separate; implementation can use qualified synthetic fixtures without claiming
live behavior proven.
