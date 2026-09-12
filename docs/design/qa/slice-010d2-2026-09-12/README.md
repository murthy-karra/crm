# 010d2 synthetic Web walkthrough

One production-Web-build walkthrough against the actual application router in
an isolated SQLx PostgreSQL database. No mocked HTTP responses or live source
requests. The opt-in `timeline_browser_fixture` creates a completed People parent,
retained capture, and prepared plan with 110 events, one call and one undated text.

The first walkthrough passed:

1. Signed in as the synthetic administrator and inspected the completed preview.
2. Confirmed all four acknowledgements through the review dialog.
3. Withheld readiness at the worker boundary, observed the pause, then resumed
   through the administrator UI.
4. Applied 50 facts, cancelled, and prepared/confirmed the same frozen plan.
5. Completed with 62 newly inserted and 50 already-imported records: 112 total.
6. Opened the Person, filtered/paged events, inspected metadata and dismissed the
   dialog with Escape. Reviewed the undated text at 390px.

There were no browser page errors, no displayed body sentinel, and no horizontal
overflow at 390px. These are specific walkthrough observations, not an exhaustive
accessibility or capacity claim. Source metadata and existing core source-field
review are separate from captured message/note bodies.

Evidence: [desktop](desktop.png), [mobile](mobile.png),
[structured results](walkthrough.jsonl). The screenshot fixtures are synthetic.
Private fixture credentials were removed, browser cookies/context closed, the
SQLx fixture completed successfully, and owned API/Web ports 13012/15175 stopped.
The shared development deployment was not changed.
