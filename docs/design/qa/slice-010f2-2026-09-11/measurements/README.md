# 010f2 query and reader measurements

The actual application SQL collector runs against an isolated synthetic
25,000-Person,50-member book. Both artifacts retain92 EXPLAIN ANALYZE probes,
actual application-source hashes, returned-row assertions and zero temporary
spill blocks. The successful original run took121.91s; the final run154.88s.
The initial generator failure remains in `../checks/query-plans.log`: its inert
500-character task title ended in whitespace and correctly failed the native
constraint. Only that synthetic generator was corrected.

| Probe | Shared blocks before → final | Execution ms before → final |
|---|---:|---:|
| Missing negative-detail fingerprint | 2,504 → 2 | 9.577 → 0.008 |
| Rare manifest issue,25 matches | 25,284 → 76 | 14.885 → 0.227 |
| Empty manifest issue | 25,284 → 1 | 7.498 → 0.021 |
| Rare result issue,25 matches | 25,138 → 76 | 12.553 → 0.207 |
| Empty result issue | 25,138 → 1 | 7.416 → 0.018 |

The measured correction adds the partial negative-source lookup index, keys
result/issue page indexes to the actual plan predicate, and starts selective
issue filters from their immutable projection. Empty issue probes perform zero
manifest/result row lookups. These timings are individual observations, not
production latency promises. No comparable whole-plan scan remains among the
final probes. Dense-Person counts use1,189 blocks for503 notes and1,005 tasks;
counts intentionally cover that Person's complete activity.

The book has25,502 notes and51,003 tasks. Its concentrated Person has503 notes
(including501 at10,000 characters),503 open tasks and502 completed tasks. Actual
native readers traverse each family in11 pages, with no duplicates and at most
50 items. Maximum response bytes are39,491/47,298/47,304 respectively, below
512KiB. Full-note retrieval returns10,000 characters. Readers add zero source
calls.

Source qualification uses a real small synthetic book:10 source reads followed
by one imported note and one imported task. The bulk retained rows include inert
copied ciphertext and250 synthetic capture pages beyond that real source
boundary. No retained decryptor/worker is run over those bulk rows. They establish
query cardinality and native reader bounds, not legal large-source ingestion.
Physical relation allocation includes update/dead-space overhead and must not be
extrapolated into per-agent capacity or confused with logical import allowances.

Before artifact SHA256:
`b361f97652cdcb37547dc6aef1f484fdac6ec274cdd373425b40a0a8de5c9908`.
Final artifact SHA256:
`2ee7aa065d40c816bc1f15cbff96fb7dd570560d67b5e255d0de3d766f84a687`.

The one actual paired operational Person-detail comparison passed in20.88s.
An earlier fixture setup attempt stopped before either HTTP arm or any timing
sample because its generated title ended with a space; appending a non-space
suffix corrected only that opt-in fixture. Both setup and successful logs are
retained. No comparison was discarded or repeated.

The paired book has25,000 People,50 members,75,018 notes and100,017 tasks,
including tombstones. The selected Person has20 live notes,10 open tasks,
10 completed tasks and one core history item. Both arms use the same executable,
fixture and clock, with verified frozen `cd3b010` function bodies. Five warmups
and40 measured requests per arm alternate AB/BA without overlapping arms.
All90 responses are complete HTTP200, identical33,955-byte bodies and JSON,
with expected frozen/current entry counters. All fixture counts remain unchanged;
the workspace remains operational and contains zero activity-import children.

| Dispatch through complete-body latency | Baseline | Current |
|---|---:|---:|
| p50 | 9.949ms | 10.524ms |
| p95 | 14.145ms | 15.906ms |
| Maximum | 16.765ms | 17.269ms |

Current p95 is1.761ms higher and passes the specified39.145ms limit:
baseline p95 + max(25ms,10%). This is a single laptop comparison, not an absolute
SLO or a concurrency/capacity benchmark. The artifact preserves all raw timing,
completion, equality and entry-counter metadata without response contents.
Paired artifact SHA256:
`01ebb69e8f1cd26d54f26a9d35beea1f6d7e58564b5b4337af02641e86d41974`.
