# Verify behavior and review the delivered change

Use for independent acceptance review of meaningful changes. Default profile:
`design` in [MODEL_ROUTING.md](MODEL_ROUTING.md). Provide the specification/brief,
the exact review target (working tree or verified base/head), and existing evidence.
Read-only review is the default; repairs belong to the assigned writer.

```text
Independently verify the assigned change against its acceptance criteria.
Read and apply docs/prompts/README.md and the verification routing in
docs/prompts/MODEL_ROUTING.md. Preserve the existing reviewer assignment.

Resolve the exact review target and inspect the real diff, untracked files,
specification, task brief, and affected code paths. Do not rely on the
implementer's changed-file list or test claims without checking their evidence.
Attribute supplied results to their runner and identify the tree they covered.

Map every applicable acceptance criterion to a test, direct observation, or an
explicit gap. Run the checks required of this review and investigate missing
or stale evidence. Scope additional checks to unresolved concerns. Coordinate
database runs with the writer; never overlap them in one checkout. If a writer
changes the target during review, re-establish the tree and evidence affected.

Examine authorization and cross-Organization attempts, contract compatibility,
error paths, idempotency/retries, concurrent transitions, historical fidelity,
and sensitive-data exposure where relevant. Review telemetry and performance
against task requirements. For UI work, inspect the required flows and states;
use screenshots only when they provide useful, non-sensitive evidence.

Return findings ordered by severity, each with file/line or spec section,
triggering scenario, evidence, impact, and suggested correction. Distinguish
regressions from pre-existing issues. Report no actionable findings when that
is the evidence; do not invent findings or replace requirements with preferences.

State which criteria pass, fail, or remain unverified, plus the exact commands
run. Approval readiness requires all required checks and blocking findings to
be resolved. Describe any user-authorized deferral honestly; never call an
unverified criterion passed or treat readiness as release approval.

Send corrections to the assigned implementer using 05-implement.md; a spec gap
returns to 02-specify.md. Do not edit the same files concurrently or repair
code during a read-only review. Once fixes land, verify affected behavior and
any required final-tree gates. Hand off the result and remaining release gate.
```

## Calibrating review and adversarial findings to slice size

Recorded 2026-09-06 after Slice 011b-sort, where about a third of the
review-driven test additions guarded states that were unreachable by
construction. Reviews and adversarial analyses are meant to produce long lists;
the coordinator's job is to filter them, not to apply them wholesale.

**Ask each reviewer and tester to tag every finding** with exactly one of:

- `SEEN_HERE`: maps to a failure mode this codebase has actually had (cite the
  verification record or decision that recorded it);
- `BOUNDARY`: a customer-visible boundary such as a cap, an ordering key, a
  quota, a revision, or money;
- `TRUST`: tenant isolation, authorization, privacy, or an untrusted-input path;
- `CONTRACT`: the wire shape or error precedence of an HTTP, realtime or tool
  contract;
- `RESTATES`: the same behaviour already proven one step away;
- `UNREACHABLE`: a state blocked by a CHECK constraint, a typed enum, or a
  server that never emits it, or behaviour a framework already guarantees.

**Disposition by slice size.** For a size-S rung apply `SEEN_HERE`, `BOUNDARY`,
`TRUST` and `CONTRACT` items; apply `RESTATES` and `UNREACHABLE` items only when
the test is a few lines with no new fixture, and otherwise record them as
LATER in one line each. For M and larger rungs, or any rung touching money,
history or erasure, apply `RESTATES` too and decide `UNREACHABLE` case by case.
A finding that is an actual defect is always applied regardless of tag.

**Keep the cost visible.** Note in the verification record how many findings
were applied and deferred, and watch test-file size: a single test file
growing by more than a few hundred lines in one rung is a signal to filter
harder or to split the file, not a sign of rigour.

## Practical envelope and round cap (D-050, 2026-09-07)

Correctness, tenant isolation and privacy hold everywhere. Seamlessness and
measured performance are owed only inside the v1 operating envelope recorded
in [D-050](../decisions/DECISION_LOG.md): 25,000 People and 50 members per
Organization, 5 concurrent Today loads, one active browser tab per agent.

- **Tag `BEYOND_ENVELOPE`** for any finding whose triggering scenario needs
  load, data volume, tab count or timing outside that envelope. Its default
  disposition is LATER at every slice size. A `TRUST` finding beyond the
  envelope is still applied, but the required correction is fail-closed
  behaviour (no leak, no corruption, an honest error), not seamless recovery.
- **Two rounds, then stop.** A slice gets at most two review-then-fix rounds.
  Findings still open afterwards are listed as LATER in the verification
  record, one line each. A third round needs the user's explicit approval.
- **Performance gates are relative, not absolute.** Gate only on the paired
  regression against the previous code on the same machine
  (max(25 ms, 10%)) and on plan shape from one `EXPLAIN (ANALYZE, BUFFERS)`
  per changed hot statement. Report absolute latency, concurrency above 5,
  pool wait and planner toggles without pass/fail. Laptop numbers do not
  predict production hardware; capacity is measured once on real hardware.
