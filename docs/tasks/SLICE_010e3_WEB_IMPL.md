# 010e3 Web implementation lane

D-078 implementation is approved. Read AGENTS.md, DECISION_LOG.md,
SLICE_010e3.md, SLICE_010e3_CONTRACT.md and SLICE_010e3_IMPL.md first.

Worktree: crm-worktrees/migration-010e3-web, branch codex/migration-010e3-web.
Primary writer: Terra high. Own web/src/api/peopleAdmissions.ts, admission-only
components/tests, and SLICE_010e3_WEB_VERIFICATION.md. Reuse existing import access,
workspace/session boundaries, core-change report/parent APIs, and current styles.
Do not modify backend, migrations, other migration features or shared files
without coordinating with root. Root owns shared profile/history/workspace seams.

Replace the scaffold with the specified usable admin flow. Choose completed
parents/reports from real paginated data; poll bounded preparation/settlement;
show all counts, coverage, source intervals and inherited mapping acknowledgment;
page items, contacts, full fields and results. Support exact frozen confirmation,
expiry/re-preview, bounded cancel/retry and new same-report remainder after terminal
cancellation. Preserve uncertain request IDs/bodies and recover by replay. Never
convert source IDs/counts/revisions through JavaScript Number. Guard all reads,
pending responses and query caches with the established access/session patterns;
clear on 401/403, identity/org/workspace transitions and unmount. Render raw text
as escaped text, never unsafe links/HTML; no body text leaks from old scoped cache.

PersonAdmissionProvenance must be a readable admin-only review card with bounded
contact paging and full UTF-8 field fragments. Existing import-provenance behavior
and its authorized 404 fallback remain unchanged. Avoid technical UUID entry as
normal UI. Follow PeopleRefreshPanel and its child components as proven patterns,
adapting final admission semantics rather than retaining old refresh operations.

Required checks: focused Vitest behavior/authority/pagination/uncertain-request
coverage, typecheck, lint and isolated build. Root will run the real API3103/Web5174
browser acceptance after backend settles, including a 390px viewport and long
Unicode source fields/56 contacts. Do not mark actual browser acceptance passed
from mocks. Root owns API3103 harness and runtime; do not replace shared API3000,
Web5173, mobile API3102 or demo API3101. No push, main merge or shared deployment.

## Completed implementation evidence

The Web lane implements the approved admission-only review surface in
`web/src/api/peopleAdmissions.ts` and the admission components. The panel has
real parent/report/admission selectors with opaque cursor paging, including
visible report paging; access/session fencing; active-state polling; frozen
confirmation and request replay; and pages for items, contacts, result rows and
full UTF-8 provenance fields. It renders the backend's source-boundary,
coverage, exact decimal-string counts, published field prefixes/byte totals and
categorical held reasons. It offers full-text paging for every server-published
truncated provenance field, including retained fields such as `sourceUrl`.

Focused component coverage verifies report and item cursor transitions, a
published `sourceUrl` field request, request replay, access re-verification,
identity purge, stale selection fencing, plan acknowledgement reset and preview
expiry. The final commands and results are recorded in
`SLICE_010e3_WEB_VERIFICATION.md`; browser acceptance remains assigned to root.
