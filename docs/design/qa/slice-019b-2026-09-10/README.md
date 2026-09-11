# Slice 019b browser verification

Synthetic-only walkthrough against an isolated normal API build and Vite on
ports 3001/5180, PostgreSQL database `crm_slice019b_qa`, and separate Centrifugo
on 8001. No shared development runtime or user data was changed. API, Vite and
the separate Centrifugo were stopped afterward; the QA database is retained.

| Evidence | Observed behavior |
|---|---|
| `01-invalid-range.png` | Reversed numeric draft has an inline error and disabled Apply; applied criteria/results are retained. |
| `02-mobile-choice.png` | Searchable choice editor is usable at a 390 × 844 viewport. |
| `03-five-field-list.png` | Two independent text fields, number, calendar date and choice narrow to Grace; list saved and connected to Today. |
| `04-today-realtime.png` | Restoring Budget through a normal HTTP command restores the source reason on the already-open Today page, without navigation or manual refresh. |
| `05-archived-field-repair.png` | Archiving Budget pauses the saved list, retains removable criteria, and reports a repairable error. |
| `06-today-rule-preview.png` | Admin previews Budget ≥ 500000 for the member; required assignee/response anchors remain and only Grace matches. |
| `07-restored-list.png` | Restoring Budget recovers the same list at revision 1; held archived Warm option still matches. |
| `08-organization-isolation.png` | Second Organization has no People or prior criteria; its picker reports no custom fields (full text in `second-organization.txt`). |

Also observed: explicit Apply/Enter; Escape returns focus and cancels; outside
click discards the text draft; literal `%_` matches; fifth custom field disables
a sixth with an explanation; custom-only search has no false empty message;
remove buttons distinguish Referrer from Referral note. Clearing Budget through
the Person UI changes list count to zero, removes its Today reason, and keeps
built-in work. Archived field count returns 422 `invalid_field`, never zero;
Today reports the unavailable source. A member cannot see Fields management;
the admin cannot open the member's personal list. Archive confirmation contains
general filter consequences without private-list usage information.

`lifecycle.json` records exact archive/restore HTTP outcomes and unchanged
revision. `read-filter-smoke.json` contains 13 earlier authenticated HTTP cases.
The `*.txt` snapshots retain selected UI states. All eight screenshots were
visually inspected. Browser logs during the walkthrough contained Vite debug
messages and no warning/error entries. The driver's empty-string fill did not
clear a native input; Select-all/Backspace did, and the one-sided range worked.

Live Operator inference was disabled in this QA runtime; the saved-list tool,
untrusted description, privacy and unchanged tool surface are covered by the
executed Rust/DB tests rather than a fabricated inference walkthrough.

Final [gate logs](gates/README.md), [code-tree hashes](code-tree.json),
[changed-file inventory](changed-files.txt) and
[performance evidence](../../perf/slice-019b-2026-09-10/README.md) are retained
alongside the walkthrough.
