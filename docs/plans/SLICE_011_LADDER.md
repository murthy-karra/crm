# Slice 011 — Smart Lists ladder

Status: ACCEPTED (user, 2026-08-28) with the three ladder-level
decisions taken. Charter: D-043 (smart lists first-class FUB-shaped;
lists feed Today as explainable sources; built-in Today logic
becomes org-tweakable system feeds in the same vocabulary — the
filter model IS the Today configuration language). Sizing rule
(user, standing): every rung independently landable at S–M; rung
specs are written when the previous rung merges and absorb what was
learned. Format per SLICE_007_LADDER.

## In plain language

This ladder builds "smart lists": saved, shareable filters over
the People list that eventually drive the Today page. It lands in
five small steps, each shippable on its own:

- **011a — Filtering the People page.** Adds filter chips to the
  People page: narrow the list by stage, who it's assigned to,
  where the lead came from, how long since anyone was contacted,
  whether they have a phone/email, and so on. The filter is
  reflected in the URL so a link can be shared with a colleague
  (a filter on "assigned to me" means *them* when they open it).
  Nothing is saved yet — close the page and the filter is gone.
  Under the hood this rung defines the filter "language" that
  every later rung reuses.
- **011b — Saved lists.** Lets agents save a filter as a named
  list ("my stale Zillow leads") and come back to it. Personal
  lists belong to one agent; shared lists are curated by admins,
  and agents duplicate them rather than editing the original.
- **011c — Lists feed Today.** An agent can mark any list as a
  Today source: people matching that list start appearing on
  their Today page, each labeled with which list put them there.
- **011d — Tweakable built-ins.** The Today page's three
  hardcoded rules (unanswered inquiry, client replied, call
  needs an outcome) are re-expressed as editable filters, so an
  org admin can adjust them — with a preview before saving and a
  revert-to-default. The riskiest rung by design, because it
  changes how Today works for the whole org.
- **011e — Tags.** Adds free-form tags on people ("investor",
  "past client") and lets filters match on them — proving the
  filter language can grow new clause types without a rebuild.

The rest of this document is the precise engineering record of
the same plan.

## Decisions taken at ladder acceptance

1. **Order: a → b → c → d → e (tags last).** Tags depend only on b
   and may run parallel to c/d if ever useful; no FUB tag data
   exists to import while Slice 010 is parked.
2. **D-043 §3 strict (user, against the planner's carve-out): ALL
   THREE built-ins — unanswered-inquiry, client-replied, AND
   call-outcome-needed — are re-expressed fully in the filter
   vocabulary.** Consequence, recorded honestly: the vocabulary
   gains viewer-relative derived axes including call state
   (`AwaitingCallOutcome`: an ended call of MINE without an outcome)
   and the reason-payload plumbing (`call_id` → the Set-outcome
   action) must flow through the feed path. 011d carries a
   pre-declared split seam (d1 = the two person-state feeds; d2 =
   the call-state axis + feed) if it runs past M.
3. **`Source` clause matches the LATEST inquiry's source** (the
   working-intent reading; D-006's original attribution remains
   untouched in the data and available to future reporting).

## The rungs

| Rung | Outcome | Schema | Size |
|---|---|---|---|
| **011a** Filter vocabulary + ad-hoc People filtering | Typed versioned `FilterDefinition` (AND of clauses: Stage, AssignedTo incl. Me/Unassigned, Source [latest-inquiry], Created/LastInquiry/LastContact/LastInbound ages incl. Never, HasReplied, HasPhone/HasEmail) + validation (caps ≤20 clauses/≤50 values, org-scoped id checks, deny_unknown_fields, unknown-clause-fails-closed on read) + `describe()` human-readable clauses + `filtered_summaries()` as FIXED-MATRIX STATIC SQL (NULL-guarded optional predicates; org boundary stays literal text; `.sqlx` discipline intact) + `GET /api/people?filter=` (declared additive SLICE_002 §5; absent = byte-identical) + `GET /api/inquiry-sources` + PeopleView FilterBar with chips, live count, URL-synced shareable filters. No persistence. Split seam if hot: a1 backend, a2 web. | none | M |
| **011b** Saved lists | `saved_list` CRUD via typed commands: personal (owner-only) + shared (admin-curated; agents DUPLICATE, never edit the shared original — FUB shape, re-verified at spec time); Lists nav + index with membership counts; list view = PeopleView preloaded + Save/Save-as/Duplicate/Delete. Limits: 200/org, 50/owner. | `saved_list` | M |
| **011c** Lists feed Today | Per-viewer "Use as a Today source" on any visible list (agent-for-self v1; admin org-push deferred); Today evaluates marked lists at QUERY TIME (no materialization — derived-truth posture; per-feed static evaluation, merge+dedup in Rust); additive `TodayReason::ListMember {list_id, name}` (declared SLICE_003 §5); list-only band below stale built-ins, above the low outcome tier, oldest-effective-contact-first with the key displayed (no secret priority); multi-qualification = one item, list reasons appended; 200 cap, list band sheds first. Known footgun stated: a no-activity-clause list has no organic exit but stage change — guidance, not enforcement. | `today_work_source` | M |
| **011d** Tweakable built-ins | Vocabulary gains the named derived axes: `AwaitingResponse`, `ClientRepliedUnanswered`, and (decision 2) `AwaitingCallOutcome` with its payload plumbing. The three built-ins become seeded per-org `today_system_feed` rows (seed on create_organization + backfill migration), managed on an admin "Today feeds" surface: enable/disable, edit clause values + freshness-window param, PREVIEW before save, REVERT-to-default (canonical regenerated from code), `today_feed_changed` audit fact. Acceptance gate: equivalence db tests prove feed-evaluation ≡ the old hardcoded arms (items, reasons, priorities, order) BEFORE arm deletion. Priority-tier order stays fixed system policy v1 (orgs tweak membership, not tiers). Pre-declared split d1/d2 per decision 2. | `today_system_feed` + backfill + fact table | M (split-ready) |
| **011e** Tags | `tag` + `person_tag` model, chips on PersonDetail (monochrome per UI_STYLE), `Tags`/`NotTags` clause variants proving additive vocabulary extension; re-opens part of parked 010f (tags import — noted in SLICE_010_LADDER). Creation: any member inline; rename/delete admin (confirm at spec). | `tag`, `person_tag` | S–M |

Dependencies: a → b → c → d; e after b (parallelizable with c/d).

## Standing tensions (carried into rung specs)

- Evaluation is O(people-in-org) per feed via LATERAL probes — fine
  to ~50k people on existing indexes; recorded levers, not built:
  denormalized last-activity columns, custom-plan mode, QueryBuilder
  fork (which becomes REQUIRED if custom fields ever join the
  vocabulary — the deliberate future fork point).
- Query-time feeds have no "entered the list at" timestamp — no
  per-item newness; ordering comes from data. A membership-
  transition fact table is the later upgrade if "alert on list
  entry" is ever wanted.
- O-010 boundary: no free-text/fuzzy clause anywhere; a name
  quick-find is search and stays parked.
- 011d is org-wide Today semantics — admin-only + preview + revert +
  audit fact + equivalence gate are the rails; decision 2's strict
  scope makes d the ladder's riskiest rung by intent.

## Explicitly not in this ladder

Snooze/dismiss (thesis §8, separate), O-008 AI suggestions, O-010
search, an Operator `filter_people` tool (natural post-ladder
extension), custom fields, OR-groups/absolute dates/per-list sorts,
realtime count push, org-pushed work sources, mobile.
