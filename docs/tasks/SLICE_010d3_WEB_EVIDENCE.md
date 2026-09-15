# Slice 010d3 — admitted history Web evidence

## Result

A real desktop and responsive browser journey passed against the opt-in retained
fixture. The journey used the production Web migration page, the production Axum
API, the typed admitted-history worker, and an isolated PostgreSQL database. No
live Follow Up Boss request was made.

The local evidence state is retained at:

`/private/tmp/crm-mobile007-010d3-yyoxjx50/history-ui/progress.json`

The fixture runner is the pinned binary
`/private/tmp/crm-mobile007-010d3-yyoxjx50/history-ui/admitted-history-ui-ambiguous`.
Its SHA-256 is
`0fac0cdb627a9f3e71de6819388f7584e51096a1dfb88de7bd6a16b8731ab659`.
The isolated services were API `127.0.0.1:3108`, Web `127.0.0.1:5178`, and
PostgreSQL database `crm_010d3_ui_20260915`.

## Journey

The browser logged into the seeded administrator workspace and opened
`/manage/migration`. The admitted-history panel showed the selected terminal
admission cohort and qualified retained capture, including the frozen capture
interval, source access (`User 3`), and source-family coverage:

- Events: 75 enumerated records
- Calls: 1 enumerated record
- Text messages: 1 enumerated record
- Coverage warnings: API restricted/records unknown, detail content not fetched,
  non-atomic snapshot, and external facts only

The ready preview displayed **77 unique planned, 77 occurrences, 77 eligible,
0 already present, 0 imported, 0 held, 0 equal repeats, 0 excluded**, and one
unknown date. Both review acknowledgements and the final import confirmation
were completed through the visible Web controls.

The worker allowance file then permitted exactly one bounded unit. The UI showed
50 imported while the worker stats recorded one successful unit and zero source
reader calls. Cancelling the run retained those 50 committed facts and exposed
“Continue never-settled remainder” for the remaining 27. Continuing the
remainder and granting the second bounded unit resulted in 77 imported total;
worker stats ended as:

```json
{"applied_units":3,"last_unit_failed":false,"source_reader_calls":0}
```

The third worker tick only finalized the terminal root after the second bounded
unit; it did not read the source or add another record.

The Results tab displayed imported, fact-committed, settled rows. The Manifests
tab was traversed with its bounded cursor: positions 1–25, then 26–50, then
back to the first page. The panel exposed the Family and Outcome controls and
the page-size message (“Pages contain at most 25 rows”).

A second retained capture in the same isolated workspace exercised held and
recovery behavior. Its source coverage contained four observations (two Events,
one Call, one Text), with one Event carrying an unresolved Person relationship.
The fresh ready preview displayed **4 unique planned, 4 occurrences, 3
eligible, 0 imported, 1 held, 0 equal repeats, 0 excluded**, and one unknown
date. The held row was visible as `Events · 901`, `Held`, `Admitted Person`, with
`Relationship uncertain`. The retained source observation carried valid primary
Person `104` and a second valid participant Person `105`; the source Person was
not part of the settled parent cohort, so the import held the ambiguous
relationship rather than inventing ownership. The
operator checked both held-outcome and coverage acknowledgements before the
visible confirmation dialog.

For recovery, the private fixture control set `release_ready=false` while a
fresh attempt was prepared. The real worker paused the run; the panel showed
`Paused · Building` and `Paused: Release Not Ready`, with a visible `Resume`
control. After restoring `release_ready=true`, the browser clicked `Resume`,
reconfirmed the rebuilt preview, and let the bounded worker finish. The final
panel showed **4 occurrences, 3 eligible, 3 already present, 0 imported, 1
held, 0 equal repeats, 0 excluded**, and `Completed · Ready`. Worker stats ended
at `applied_units=9`, `last_unit_failed=false`, and `source_reader_calls=0`.
The already-present outcome is expected because the same retained facts were
used to verify idempotent recovery.

## Responsive evidence

Desktop browser-tool captures covered the 77-record happy path, the paused
`Release Not Ready` state, and the final held summary. A temporary explicit
390×844 viewport captured the stacked H3 controls and the summary cards showing
4 occurrences, 3 eligible, 3 already present, and 1 held; a second mobile view
showed `Completed · Ready`. The viewport override was reset after the capture.
These screenshots were emitted inline by the browser tool and no image files
were retained. Both layouts remained usable without horizontal page overflow in
the observed panel.

## Reproduction notes

The fixture was already seeded and the API/Web processes were already running;
no services or databases were recreated during the journey. The worker gate was
controlled only by these private files:

- `/private/tmp/crm-mobile007-010d3-yyoxjx50/history-ui/worker-units.json`
- `/private/tmp/crm-mobile007-010d3-yyoxjx50/history-ui/worker-stats.json`

The first unrelated core-history attempt created while locating the page was
cancelled immediately and is not part of this admitted-history evidence. The
admitted run itself is the 77-record journey described above.
