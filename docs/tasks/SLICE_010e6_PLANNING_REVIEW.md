# 010e6 — Author planning check

**READY — independent planning review, round 1, 2026-09-15.**

The user authorized planning after the 010e5 release. D-090 accepts implementation. Application code had not
been changed at review. [Plan/specification](../plans/SLICE_010e6_NEVER_IMPORTED_RECOVERY.md)
owns proposed scope and contracts. Implementation acceptance is recorded in D-090.

Code/spec inspection identified these necessary distinctions:

1. 010e3 deliberately excludes original-present IDs; positive held-origin authority
   is needed, rather than deleting that exclusion.
2. A missing identity row alone cannot prove “never imported”; existing successful
   result evidence and durable erasure markers must also block recreation.
3. 010e5's successful refresh mapping bindings cannot be invented before a Person
   exists. Recovery needs admission-owned initial approvals, recognized by refresh.
4. Follow-on families require exact successful admission/result identities. Merely
   adding a UI link would leave recovered People unusable by those workflows.
5. A source boundary older than a later admitted boundary must not sneak through
   an ordinary retry; explicit selection and remainder ordering are required.
6. Original and admission histories must continue to say what actually happened.
   New recovery success does not rewrite old hold counts or fabricate old results.

The proposal contains explicit contracts, owner/target/source checks, atomic
settlement, old-worker fences, cancelled remainder rules, family handoff, bounded
queries/DTOs and a concrete test matrix. A subsequent independent review should
test these against the frozen SQL/DTO contract once implementation is accepted.
No tests or runtime evidence for the unimplemented recovery feature are claimed.

## Independent review

The one authorized reviewer returned READY with no blocking design finding.
Read-only inspection covered AGENTS, D-015/D-050, admission identity/classification
(`people_admission_worker.rs`), confirmed-boundary rules (`people_admission.rs`),
and representative admitted-family result qualification (`admitted_metadata/
preparation.rs`). The reviewer required the already-owned acceptance amendment,
concrete mode/engine/ownership/confirmation/ledger contract and full initial-
mapping/proof reader inventory before code integration. No DB tests were run
by the reviewer; implementation correctness remains to be verified.
