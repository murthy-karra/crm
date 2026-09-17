# Stateful Web E2E: migration review

The migration family uses a separate, disposable API example executable built
from the same application and existing `test-support` FubReader injection seam.
The production FUB reader hardcodes its HTTPS host, so this fixture substitutes
only the source Reader. It calls the family-local HTTP source fixture, while
normal Web, session/authentication, typed commands, PostgreSQL, real migration
workers and configured Centrifugo remain in use. It does not prove the production
FUB HTTPS adapter or source API fidelity.

The example also uses the existing explicit synthetic release-readiness fixture
used by `import_qa`; this is not proof of a deployed fleet's compatibility inventory.
Workspace policy and all ordinary mutation guards remain real. Seeds use normal
administration commands and verify the destination has no People/business data.
No direct business SQL writes or fixture-only business endpoints are introduced.

The sixteen-step journey covers source connection, bounded assessment, core
capture, overlap preview, immutable stage/agent mapping revisions, exact People
confirmation, separate imported identities despite shared contacts, provenance,
reconciliation, admin review hold, member/tenant denial and reload. It continues
with retained tags/custom fields, readable notes, timezone-qualified tasks,
historical metadata capture/import, a later retained source comparison, combined
metadata/activity/history refresh, cancellation of one confirmed family, and its
exact unfinished remainder. SQL assertions are read-only and check authoritative
results alongside actual browser operations and HTTP receipts.

The private source fixture has two deterministic source versions. A private
control also gates execution in the fixture-owned refresh worker scheduler, so
cancellation happens after confirmation and before execution without relying on
a race. The preparation/execution functions and all business commands are the
real application functions. This scheduling control does not expose a production
business endpoint or change command authorization.

Synthetic source requests are recorded separately as `fub`; retained imports and
refreshes must make no additional Reader requests after the last authorized
capture. The final assertion checks the family scope and zero external requests.
The imported workspace intentionally remains in administrator review, which does
not receive normal operational change events; captured realtime frames are
checked for tenant leakage, not presented as proof of operational publication.

This is representative catalog 10 coverage, not every migration branch. It does
not cover the production HTTPS adapter, typed source failure/retry fidelity,
standalone People refresh/admission or admitted dependent cohorts, all repair and
recovery branches, or large-source pagination. The fixture does not start the
standalone/admitted workers and must not be used to claim those flows passed.

JavaScript syntax and Rust formatting passed, and the expanded Rust fixture
compiled in the isolated migration image. Run `1fab01ab4411` passed all sixteen
steps; its cleanup log confirms containers, the database volume and network were
removed. Evidence is in
`.e2e/runs/1fab01ab4411/migration-1-a1/` (steps, HTTP, read-only database checks,
source requests, realtime frames, trace and videos).

Earlier runs exposed E2E synchronization errors: `2e6e2a31d0bb` passed ten steps
before history acknowledgements were enumerated before rendering;
`db02d4a7c2ef` passed thirteen before combined confirmation enumerated controls
before the last ready plan and global acknowledgement rendered. The test now
waits for rendered controls, all expected ready family selections, and the named
global acknowledgement. No application behavior was changed for these fixes.
