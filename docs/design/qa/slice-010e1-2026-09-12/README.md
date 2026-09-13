# 010e1 synthetic implementation evidence

Observed 2026-09-12 PDT. This directory contains synthetic fixtures only.
The real retained collector/import/report tests and query plans are attributed in
[verification](../../../tasks/SLICE_010e1_VERIFICATION.md).

## Actual browser walkthrough

The production Web build was made from integrated `d6e7c74` plus the coordinator's
two presentation-only corrections in `CoreChangeReportRows.vue`. Its exact files
are listed in `web-build-sha256.json`. The API was the final 010e1 build from
`fc1acc5`, SHA-256
`e88ccf09fd015332a332a3fb5c4e551323996926984df44e3ec6e77b3502fe4c`,
on loopback 3102 with the isolated `crm_migration_010e1` database.
Browser: installed Google Chrome through Playwright Core, headless, viewport
1440×1050 and 390×844. The QA preview served loopback 5202 from an isolated output.
A fresh actual-inventory/database preflight admitted the report request.

`walkthrough.jsonl` records the completed actual API flow; `walkthrough.mjs` is
the harness. Synthetic credentials and authenticated browser state remain outside
Git, so running the script requires its private fixture paths. No HTTP response
was mocked. The first accepted POST was executed using Playwright `route.fetch`
and its response deliberately discarded; the UI retried exactly the same body
and recovered the same report. The test also checked cancelled unpublished
output, filtered People absence, inaccessible note detail, representative evidence,
authorized snapshot navigation and report recovery after reload. No page errors
occurred and the narrow page had exactly 390 pixels of scroll width.

The coordinator inspected `desktop.png` and `narrow-note-detail.png`: controls,
closed result categories and wrapping evidence references are readable. The
fixture has eight comparison identities; actual multi-page enumeration is
covered by the DB/API test, not claimed for this browser run.

`walkthrough-attempt1.jsonl` preserves the initial harness failure: it requested a
button's visible text as the exact accessible name despite its more specific
`aria-label`. Correcting that selector completed the flow; no product correction
was needed for that failure.

## Shared preview incident and restoration

An initial verification build used the root `web/dist`, which the existing shared
preview also serves. This temporarily replaced its assets. The coordinator
reconstructed the released Web source in a temporary export and verified all
73 file hashes against the retained 010d2 release manifest, restored those files,
and verified all 73 served responses match the released hashes. API health was
200; neither shared process was restarted. `shared-preview-restoration.json`
records this result. Subsequent QA uses `/private/tmp/crm-010e1-qa/web-dist-final`.
This was an incidental preview change, not a new deployment approval.
