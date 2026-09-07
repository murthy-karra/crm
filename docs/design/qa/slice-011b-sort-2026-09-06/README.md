# Slice 011b-sort — live browser evidence

Captured 2026-09-06 (late evening) by the coordinator with `playwright-core`
driving headless Chrome against the branch stack on the development ports
(branch `crm-api` on 3000, Vite on 5173), signed in as the seeded Acme admin.
Synthetic data only; the list created during the check was deleted through
the API (status 200) and the walkthrough left no data behind.

| Artifact | Observed |
|---|---|
| [People sorted by Name](people-sorted-by-name.png) | Header buttons named "Sort by Name, ascending", "Sort by Stage, ascending", "Sort by Assignee, ascending", "Sort by Added, ascending"; after one click the URL is `?sort=name.asc` and the Name header carries `aria-sort="ascending"` while the others carry `none`; a second click gives `?sort=name.desc`. |
| [Saved list sorted Name Z–A](saved-list-sorted-name-desc.png) | Save as list showed "Sorted by Name (Z–A)"; the create returned 201; the detail read returned `sort: "name.desc"`; the list page opened with the Name header `aria-sort="descending"`. |
| [1100 px, inspector open](narrow-1100-inspector-open.png) and [700 px, inspector open](narrow-700-inspector-open.png) | With the Person inspector open and the sorted six-column table, the document width equals the viewport at both sizes (no page-level horizontal overflow); the table scrolls inside its own container as before. |

[ARTIFACT_SHA256.txt](ARTIFACT_SHA256.txt) hashes the four images.
