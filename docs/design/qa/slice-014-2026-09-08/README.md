# Slice 014 — live walkthrough over the tunnel, production mode (spec §8.10)

Recorded 2026-09-08 by the coordinator against `https://app.tarams.org` served
by `scripts/dev-web-prod` from the slice tree at `fa9bcab` (Vite preview on
5173, the user's dev server stopped by exact PID beforehand as pre-approved).
Real Chrome via `playwright-core` (headless), the seeded Acme admin, the
shared development database. The script lived in the coordinator's scratch
space and is not committed; `results.json` is its raw output and the PNGs are
its screenshots. One scenario (6) first failed on the script's own selector
expectations and passed on a corrected re-run; no product defect was found.
The Person used was restored to its original stage and assignee and the
walkthrough's tag was deleted afterwards.

| # | Scenario | Result | Evidence |
|---|---|---|---|
| 1 | login → Today renders with data | PASS | heading present; Loading blocks remaining: 0 |
| 2 | hover prefetch then preview without Loading | PASS | detail prefetched on hover: true; "Loading person…" seen: false |
| 3 | stage change is optimistic and does not revert | PASS | during flight: "Nurture", after settle: "Nurture" (target Nurture, was Lead) |
| 3b | assignee change is optimistic and does not revert | PASS | during flight: "Unassigned", after: "Unassigned" (target Unassigned, was Alice Anderson) |
| 4 | tag apply and optimistic remove | PASS | chip appeared 2813 ms after create row click; gone during delete flight: true; gone after: true |
| 5 | Today rules editor shows locked chips; no remove; no Clear all when locked-only | PASS | locked me chip: 1; anchor remove control: 0; Clear all present: 0 |
| 6 | FilterBar: selected trigger with count, chip chevron, Clear all with a non-locked clause | PASS | first run failed on the script's own expectations (the chip test id is on a wrapper span; the "Edit …" name is on the inner button; the second SVG is the remove icon; only one checkbox registered); re-run with explicit checks: trigger "Stage · 2", edit button "Edit Stage: Lead or Hot Prospect" with the chevron, 40 px target, two stage ids in the URL, Clear all present (06b screenshot) |
| 7 | popover anchored to its chip in a wrapped row | PASS | vertical gap 7 px, horizontal overlap true; chip {"x":80,"y":203,"width":159.59375,"height":42}; popover {"x":81,"y":252,"width":320,"height":503} |
| 8 | caching headers through the tunnel | PASS | {"/":{"cache-control":"no-cache","cf-cache-status":"DYNAMIC","etag":null},"/assets/index-WtkJVROi.js":{"cache-control":"max-age=14400","cf-cache-status":"REVALIDATED","etag":"W/\"207949-1788908141607\""}} |

Screenshots: `01-today.png`, `02-preview.png`, `03-person-after-optimistic.png`,
`04-tag-applied.png`, `05-today-rules-locked.png`, `06-filterbar-selected.png`,
`06b-filterbar-two-stages.png`, `07-popover-anchored-wrapped.png`.
