# Slice 011a — People filter UX review

Reviewed 2026-09-06. **Proposal, not an accepted specification or decision.** No application behavior or shared contract was changed by this review.

## Recommendation

Redesign the filter interaction before building saved lists on top of it. Preserve the v1 filtering model, but give it a compact toolbar, focused editors, clear applied state, accurate result feedback, and labels that explain the actual matching behavior.

The current experience makes agents manage a form while trying to work a list. Its problems are both presentational and behavioral: oversized controls and wrapping checkbox groups consume attention, while inconsistent defaults and recovery states make it difficult to trust what is being shown. D-043 makes this especially important: this vocabulary will eventually configure Today.

## Evidence and limits

Reviewed AGENTS.md, the decision log, architecture baseline, SLICE_011a specification and implementation brief, SLICE_011 ladder, UI_STYLE, FilterBar, PeopleView, filter helpers, query flow, and relevant tests. An independent agent reviewed the implementation alongside the live walkthrough.

The running CRM was inspected in a separate authenticated browser tab. The existing People data was only read; no People, assignments, stages, or other business records were changed. The original user tab was not edited.

Live reproductions covered Stage selection, a zero-result filter, clearing the last selected stage, sidebar navigation with a filter active, Created → Never, and removing/re-adding a custom time filter. The live fixture had 17 People; large-team/source scaling observations come from the implementation and documented caps, not a 50-agent live fixture. FUB comparisons use its official help documentation, not a live FUB account. This was a heuristic review, not user research or a full accessibility audit.

Existing focused tests were run by the independent review agent:

```sh
source ~/.nvm/nvm.sh
cd web
pnpm exec vitest run src/components/FilterBar.test.ts src/views/PeopleView.test.ts
```

Result: **2 test files, 14 tests passed.** These checks do not cover several reproduced interaction problems. The full backend and database suites were not run for this review.

## Findings, in priority order

### 1. Applied state can disagree with what the user sees

**Live confirmed:** select Stage = Nurture, then click People in the sidebar. The URL becomes `/people`, but the chip and filtered results remain. A supposedly unfiltered link and the visible audience now disagree. The route watcher ignores an absent filter parameter. See [PeopleView.vue:135](/Users/karrad/projects/crm/web/src/views/PeopleView.vue:135).

**Live confirmed:** set Last contact to 7 days, remove the filter, then add Last contact again. The chip applies 30 days, but the input displays 7. The input draft initializes only once per field and survives removal. See [FilterBar.vue:160](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:160).

Fix these before cosmetic work. URL, committed criteria, editor values, and query results must have one coherent lifecycle. Reopening an editor must reflect the current applied clause. Navigating to unfiltered People must clear criteria and stale drafts. Browser Back/Forward should restore the URL's view; `router.replace` need not make every checkbox change an undo step.

### 2. The UI lets users create invalid filters

**Live confirmed:** Created → Never displays the literal chip “Created never (invalid)” and then “Could not load people.” Created is non-nullable and the spec forbids this operator. The UI shares one operator list across every age field. See [FilterBar.vue:144](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:144) and [filter.ts:193](/Users/karrad/projects/crm/web/src/lib/filter.ts:193).

Code inspection also shows no local guard against selecting more than the API's 50-value limit, and numeric entry silently clamps or truncates some invalid values. Use field-specific operators and inline validation. A supported control should never deliberately produce an invalid clause. Preserve the user's last valid filter while they correct an incomplete number.

### 3. Zero matches is presented as an empty database

**Live confirmed:** a filter with no matches shows “No people yet,” introductory lead-creation text, and “Add a lead.” The normal count footer disappears. This suggests the data is missing, when the user's criteria merely exclude it. See [PeopleView.vue:281](/Users/karrad/projects/crm/web/src/views/PeopleView.vue:281) and [DataTable.vue:72](/Users/karrad/projects/crm/web/src/components/DataTable.vue:72).

Use **“No people match these filters”**, retain an explicit **0 matches**, and offer **Clear filters**. Keep the active chips editable. Reserve the onboarding empty state for an unfiltered empty database.

### 4. The controls have the wrong visual hierarchy

**Live observed:** Add filter stretches across the content area. The open editor is another full-width card, with its checkbox choices arranged horizontally. Active filters look like small passive badges, even though they are primary editing controls. There is one editor open at a time, but it displaces the table whenever opened.

The full-width selector is consistent with `selectPt()` inheriting `w-full`, despite the FilterBar also supplying `w-40`. See [controls.ts:32](/Users/karrad/projects/crm/web/src/lib/controls.ts:32) and [FilterBar.vue:239](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:239).

Use a compact toolbar: **Stage**, **Assigned to**, **+ Add filter**. Open one anchored popover from the relevant control or chip. Make editable chips clearly interactive, with adequate target sizes. Keep the app's existing quiet, neutral styling. A permanent filter sidebar would consume too much width alongside the existing app navigation; a drawer can be evaluated later for complex list editing.

### 5. Editing has inconsistent rules

Selecting Stage creates an unapplied draft; selecting an age field immediately applies 30 days; selecting a boolean immediately applies Yes. Done just closes the editor. Closing an untouched Stage editor leaves a “choose a value” chip that filters nothing. **Live confirmed:** deselecting the sole Stage value closes its editor, interrupting the common action of replacing one stage with another. See [FilterBar.vue:62](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:62), [FilterBar.vue:114](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:114), and [filter.ts:39](/Users/karrad/projects/crm/web/src/lib/filter.ts:39).

Recommended rule: opening a field does not change results. Selecting a complete valid value applies it live. Closing discards an untouched draft. Done, outside click, and Escape dismiss the editor without undoing committed choices. Clearing the final selected value removes that criterion but keeps the editor open so the user can select a replacement. Numeric edits apply on Enter/blur after validation. A time preset can commit its displayed operator and duration together. No global Apply step is needed for ordinary ad-hoc filtering.

### 6. The pickers will become hard to scan

Stage, member, and source options are unsearched, wrapping checkbox groups. The product targets 10–50 agents; the source endpoint can return 500 options. The chip text concatenates every selected value with “or.” See [FilterBar.vue:269](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:269) and [filter.ts:158](/Users/karrad/projects/crm/web/src/lib/filter.ts:158).

Use a vertical checkbox list with local option search and a bounded popover height. Put Me and Unassigned first; preserve inactive members in an explicitly labeled group. Keep selection order stable while clicking. Summarize longer chips as **“Stage: Lead +2”**, and expose all selections when opened and through accessible naming. A local search of filter names/options does not add the deferred People-search feature.

### 7. Labels can produce the wrong mental model

| Current label | Recommended label | Meaning to make explicit |
|---|---|---|
| Source | Latest inquiry source | Later inquiries can change which source filter matches; original attribution remains unchanged. |
| Last contact | Last contact attempt | Any recorded attempt by the team, including unsuccessful attempts; not proof of a conversation. |
| Last inbound | Last received email | Current correspondence capture is email; uses the captured message's date. |
| Has replied | Received email: Yes / No | Whether any inbound email is recorded, including an already answered one. It does not mean a reply needs attention. |
| Has phone / Has email | Phone number / Email address | Use “Available” / “Missing”; these do not assert validity, deliverability, or consent. |

Add **“Match all filters”** near the active criteria. Inside multi-selects, explain that any selected value matches. For example, `(Lead OR Nurture) AND assigned to me` should be understandable without knowing boolean syntax.

For relative dates, offer **In the last**, **Not in the last**, and **Never**, where supported, with 7/14/30/90-day presets. “Not in the last 7 days” must keep **“Includes people with no recorded attempt”** visible. Never label that existing predicate simply “older than 7 days,” because it also includes missing timestamps. Explain that “Assigned to me” changes with the viewer when a URL is shared.

### 8. Feedback is too far from the action

The result count is below the whole table; even the small live fixture places it below the initial viewport. Uncached filter changes replace the table with a loading block. Picker query loading, error, and truncation states are discarded, so a failed source lookup can look like “No inquiry sources yet.” See [PeopleView.vue:41](/Users/karrad/projects/crm/web/src/views/PeopleView.vue:41) and [PeopleView.vue:267](/Users/karrad/projects/crm/web/src/views/PeopleView.vue:267).

Put the count beside the filters. Show **“23 matches”** when complete and **“500+ matches · Showing the first 500”** when truncated, using the existing response. Do not invent a total. Keep table geometry stable during requests, visibly mark retained rows/counts as updating, and never imply old rows match newly edited criteria. Give each picker distinct loading, retry, empty, and incomplete-options states.

### 9. Broken shared links silently broaden the audience

The current specification explicitly requires invalid or server-rejected URL filters to disappear and the unfiltered list to load without explanation. That is an intentional specified behavior, not an implementation mistake. See [SLICE_011a.md:339](/Users/karrad/projects/crm/docs/specs/SLICE_011a.md:339).

Recommend a deliberate spec amendment: show **“This filter could not be loaded”** with **Clear filters** or **View all people**. Do not quietly show a broader list that the recipient might mistake for the shared audience. This becomes more consequential when saved lists feed work or future bulk actions.

### 10. Keyboard and accessibility need explicit design

Code inspection found no programmatic labels for the age operator/number, no selected-state semantics for Yes/No buttons, no expanded state on chip editors, and no explicit focus entry/restoration. Remove icons have very small effective controls. See [FilterBar.vue:217](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:217) and [FilterBar.vue:356](/Users/karrad/projects/crm/web/src/components/FilterBar.vue:356).

Use named controls, fieldsets, radio/pressed semantics, Escape dismissal, focus restoration, and a polite result announcement. Meet the repository's 40px interactive-target guidance. Test keyboard navigation and narrow layouts, not just emitted component events.

## What to borrow from Follow Up Boss

FUB documents searchable filter selection, column-header entry, automatically updated results, and both individual and collective clearing. Borrow this emphasis on fast refinement and visible recovery. Column-header entry is a later convenience, after the core toolbar works well. [Filtering contacts](https://help.followupboss.com/hc/en-us/articles/360018863574-Filtering-contacts)

FUB uses field-specific operators and separates sent, received, and broader communication activity. Its Source refers to original acquisition, although editable. Borrow the precise vocabulary, while preserving our accepted latest-inquiry source meaning. Avoid copying a generic “Me” access scope: our control specifically filters assignment. [Filter Definitions](https://help.followupboss.com/hc/en-us/articles/360025651313-Filter-Definitions), [Filtering contacts](https://help.followupboss.com/hc/en-us/articles/360018863574-Filtering-contacts)

FUB Smart Lists save a filtered working view, with an explicit update action for changes to an existing list. This is the natural next step for 011b after the filter experience is reliable. [Smart Lists Overview](https://help.followupboss.com/hc/en-us/articles/1500008374882-Smart-Lists-Overview)

## Scope and delivery order

1. **Correctness and recovery:** fix URL clearing, stale numeric drafts, invalid operators, last-value dismissal, zero matches, value limits, and picker errors. Add targeted regressions for these exact failures.
2. **Interaction redesign:** compact toolbar, anchored searchable pickers, explicit matching logic, shorter editable chips, field-specific wording, date presets, stable loading feedback, and accessible keyboard behavior. Amend SLICE_011a §6 and acceptance criteria for changed interaction/draft/count placement requirements. Most of this needs no HTTP or persistence change.
3. **Shared-link policy:** explicitly approve and record replacement of silent filter degradation in §6/§10. The error response contract can remain unchanged.
4. **Saved lists in 011b:** add naming, save/update, duplication, and clear unsaved-change state around the same editor.

Exclusions such as “Stage is neither Closed nor Trash” are a valuable next vocabulary extension: selecting every other stage is fragile when new stages are added. However, exclusion is not supported by v1 and should be separately specified. Original-source filtering, successful-contact filtering, exact dates, sorting, pagination, and true totals also need contract/spec work. Unanswered replies belong to 011d; tags belong to 011e. Do not present these omissions as regressions in 011a or silently implement them during UI work.

Keep organization-wide visibility, server-derived Me, inactive-member support, AND across fields/OR within values, and shared URLs. Preserve accepted meanings while improving how the user understands them.

## Acceptance scenarios for the redesign

- Build “Lead or Nurture, assigned to me, no contact attempt in 7 days,” then correctly explain who it includes and that never-attempted People are included.
- Replace the only selected stage without reopening the picker; close an untouched editor without creating an active chip.
- Enter 7 days, remove/re-add the field, and verify input, chip, URL, and results agree.
- Get zero results and recover through Clear filters; distinguish that state from an empty database or failed request.
- Open a shared Me filter as another member; see viewer-relative assignment and unchanged organization boundaries.
- Navigate People → detail → Back; navigate filtered People → unfiltered People; verify criteria and URL stay synchronized.
- Reject Created → Never and invalid numeric/value-count choices locally with useful feedback.
- Exercise a 50-member picker, large source list, unavailable options, keyboard-only use, and narrow browser width.
- With more than 500 matches, show an honest capped count and never imply all matching People are loaded.

The accompanying interactive concept uses fictional data and demonstrates Stage, Assigned to, and Last contact attempt only. It proposes an interaction direction; it is not a production implementation or the complete future filter catalog.
