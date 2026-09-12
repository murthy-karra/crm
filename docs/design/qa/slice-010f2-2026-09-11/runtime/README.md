# Slice 010f2 synthetic runtime evidence

All 18 browser phases completed against the production Web build and real API
using three disposable Organizations with existing completed People imports.
The fixture used retained synthetic source captures; no live FUB or customer data
was accessed. The [safe summary](browser-safe-summary.json) records browser/build
hashes, checks and qualifications. The [checkpoints](browser-checkpoints.json)
preserve individual attempts, response status/size evidence and screenshot paths.

| Case | Completed phases | Final native outcome |
|---|---|---|
| Complete | Pre-child core; plan; source disconnect; lost confirmation response recovery; drain; native review; access checks | 54 notes and 7 tasks; completed |
| Cancel | Pre-child core; plan; partial execution; revision change and cancellation; native review | 27 notes and 7 tasks; permanently cancelled |
| Budget | Pre-child core; plan; queue; lowered-ceiling probe; restored ceiling with allowance increase and Resume; native review | 54 notes and 7 tasks; completed |

Each pre-child core showed zero activity through the bounded review route. Each
plan traversed all 70 source records and disclosed 54 eligible notes, 7 eligible
tasks, 9 held records and 61 source-only components. The source-only count is the
disclosed exclusion count, not a count of unique attachments. Dirty choices were
discarded without applying them. The UI explicitly mapped source user 7 to the
case administrator for author, creator and assignee; source user 8 to an inactive
former member for historical authorship/creation; and unknown author 999 to
unmapped. The inactive member was unavailable as a task assignee. All five task
types were explicitly mapped, including Appointment to `other`.

Source inspection covered readable HTML with subject text, preserved original
HTML, replies and exact retained observations. Replies remained source-only.
The administrator confirmed `America/Los_Angeles`: spring and fall date-only
tasks converted to `2026-03-09T06:59:59Z` and `2026-11-02T07:59:59Z` respectively.
The completed-task preview preserved `2026-09-01T16:00:00.123456Z`. The
[native reconciliation](native-reconciliation.json) checks exact bodies, task
titles/types, source identities, timestamps and mapped actors, including absent
actors and NULL completion actors. It checks the complete expected row sets for
the completed cases and every row in the strict cancelled subset.

Native review traversed Notes, Open tasks and Completed tasks for both People,
with unique IDs and the production limit of 50 per page. The completed cases
required two note pages for Person 101. Full-note dialogs preserved 10,000 Unicode
code points and exposed original-source provenance. Reload/focus recovery and
absence of ordinary edit/task/contact controls were checked. No source text
triggered an external request. The source connection was removed after capture,
and retained planning/review continued with source access unavailable.

Cancellation froze 33 native rows at activity revision 41, then allowed one more
settlement. An authenticated API request using an actual **limit-25 cursor**
returned `409 activity_refresh_required` at revision 42. Separately, a focus
refresh in the unchanged **default-50 Web UI** cleared the open full-note body
and page series and loaded the new revision. This is not evidence of a Web Next
button receiving 409. The named cancellation dialog then retained exactly 34
native rows. The Organization remained under the migration review hold.

The lost-response check submitted the real confirmation, accepted its 202
response internally, and deliberately aborted delivery to the browser. The UI
retried the same reviewed command; byte equality was asserted before the retry
was sent, and that retry returned 202. The independent
[receipt reconciliation](lost-response-reconciliation.json), performed before
worker execution, found one child, one matching confirmation receipt and zero
native rows/results. Recovery used this existing trace and receipt; it did not
send another confirmation or create another intent.

The [deployment events](budget-deployment-events.json) record lowering the run
ceiling from 268,435,456 bytes to the exact snapshot retained-plus-reserved total
of **19,262,579 bytes**. One worker unit paused with `storage_limit` and zero
native rows. Restoring the deployment ceiling to **536,870,912 bytes (512 MiB)**
left the child paused. The UI explicitly increased the retained run allowance
from 256 to 512 MiB, with the Organization allowance already 512 MiB, and again
verified that the child remained paused. Only explicit Resume started execution.
This demonstrates exhaustion of lowered deployment headroom, not exhaustion of
the original 256 MiB allowance; no database ledger or allowance was edited for
the probe. The [storage reconciliation](storage-reconciliation.json) compares
measured retention, ledger totals, reservations and separately accounted native
bytes. These logical policy costs are not physical disk-capacity measurements.

Access checks demoted the current administrator through the normal member-role
API, cleared native review after focus/reload and observed API denial. A separate
helper restored the administrator. An ordinary member saw the hold and received
403 for native review. This access phase passed with slow private teardown; the
role changes were not repeated. The
[business reconciliation](business-reconciliation.json) reports unchanged
baseline hashes for 25 business tables per Organization. Source-call and
publication counters remained zero across the recorded runtime epochs.

Three failed private attempts remain in the evidence:

1. The first login assertion ran before Vue mounted. A readiness wait corrected
   the harness; this attempt made no business mutation.
2. The lost-response harness incorrectly waited for 200 after the successful UI
   retry returned 202. The original request and trace were preserved, and the
   read-only receipt audit established the existing outcome before drain.
3. A child GET overlapped normal confirmation and returned 503. API logs traced
   this to `workspace_busy` while confirmation held the exclusive workspace
   barrier. The harness was corrected to await the confirmation response before
   polling; execution resumed the same queued child without another confirmation.

The complete access phase and cancel plan also finished slowly while the private
driver drained response logging/teardown. Later phases bounded that cleanup.
Every successful phase asserted zero page errors; all retained attempts recorded
zero page errors and zero external requests. Console errors are **not** claimed
to be globally zero: per-attempt counts/hashes and HTTP failure context include
the intentional response abort, authentication/authority denials, stale cursor
and confirmation-overlap barrier denial.

Representative screenshots use desktop 1440 px and narrow 390 px viewports with
horizontal-overflow checks:

- [Original HTML and source-only replies at 390 px](screenshots/1789186838773-complete-plan-complete-source-html-replies-390.png).
- [Named confirmation at 390 px](screenshots/1789187930849-budget-budget-queue-budget-named-confirmation-390.png).
- [Normalized spring timestamp](screenshots/1789187315541-cancel-plan-cancel-dst-normalized-501.png) and [normalized fall timestamp](screenshots/1789187315541-cancel-plan-cancel-dst-normalized-504.png).
- [Loaded native core at 390 px](screenshots/1789188143157-budget-native-budget-person-101-native-core-loaded-390.png), [loaded notes](screenshots/1789188143157-budget-native-budget-person-101-notes-loaded-390.png) and [paging controls](screenshots/1789188143157-budget-native-budget-person-101-notes-paging-controls-390.png).
- [Full Unicode note at 390 px](screenshots/1789188143157-budget-native-budget-full-native-note-390.png) and [source provenance](screenshots/1789188143157-budget-native-budget-native-source-provenance-390.png).
- [Storage pause before restoration](screenshots/1789188049949-budget-budget-probe-lowered-budget-deployment-headroom-storage-pause.png).

The initial uncertain-confirmation screenshot caught a modal transition and does
not establish a settled retry state. Early complete-case native core screenshots
caught loading; the later loaded cancel/budget screenshots replace that visual
evidence. Initial DST screenshots showed source dates with normalized timestamps
below the viewport; the later normalized screenshots above and exact API/native
assertions supply the timestamp evidence. No screenshot is substituted for the
receipt or database reconciliation. The owned preview and browser processes were
stopped after preserving the artifacts.
