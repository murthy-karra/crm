# Saved lists and Today sources E2E

Implements catalog family 07's personal-source, shared-definition and broken-source
journeys as `lists-v1`. The coordinator owns registration and full-stack execution;
this lane owns only the family spec, seed and this record.

## Implemented sequence

1. Seed two operational Organizations, four memberships, three People and a tag
   through bootstrap and authenticated typed API commands. Sign in four real Web
   sessions and verify Organization-wide Person visibility.
2. Compose tag criteria in People, preview, sort by Name ascending, save a personal
   list and reopen. Assert the saved criteria, sort, revision and owner in PostgreSQL.
3. Connect the list to Today. Preview unsaved broader criteria without changing
   the saved Today definition, then reset the draft.
4. A second agent tags another Person. Prove a real ids-only Centrifugo publication
   causes an authoritative count refetch before polling; verify People membership,
   list count, Today reason and relational links.
5. An admin saves a shared symbolic `me` list. Each agent sees their own matching
   Person. Alice duplicates it into a private copy and edits that copy without
   changing the shared definition.
6. Deny other members, admins and a foreign Organization access to personal list
   definitions, counts and deletion. Deny member edits to the shared original.
7. Enable the shared source, then delete the tag as admin. The original personal
   definition remains intact but invalid; Today reports partial availability and
   keeps work from the usable source and built-in rules. Count fails explicitly
   rather than pretending the list has zero members.
8. Repair the personal list to symbolic `me`, preserve its sort, reject a stale
   write and verify two overlapping source reasons produce one Today Person row.
9. Delete the personal list through its confirmation dialog. Assert erased
   definition content, revisioned tombstone, atomic preference cleanup, preserved
   People and remaining Today source. Record tenant, realtime and mock isolation.

## Reuse and execution

Reuses `seedOperationalFamily`, `createJourney`, typed HTTP observation, read-only
database assertions, video/trace capture and the existing Docker runner. It adds
no dependencies, services, application changes or shared contract changes. The
family carries state between dependent steps; a failure blocks later steps.

Run with `./scripts/e2e --family lists`. Cached API, Web and browser images are
reused when unchanged. The coordinator runs this with the other families in
parallel against separate private networks, volumes and identities.

List definition and tag-catalog changes deliberately use reload/navigation for
other actors: the contracts promise no realtime publication for those changes.
The cross-actor tag application tests the realtime path that is actually promised.

## Verification and remaining catalog branches

`node --check e2e/families/lists.spec.mjs` and
`node --check e2e/support/lists-seed.mjs` passed during authoring. Full Docker
execution run `f6b7900849fb` passed all nine steps with cleanup `verified_empty`.
Initial authoring failures remain in the preceding run artifacts. Final combined
suite evidence follows below.

This family does not yet cover catalog 07d built-in rule tuning, custom-field
archive/restore, archived options, five-source quota, create retry after lost
responses, time-window/contact-based membership, or large-book performance.
The implemented invalid-reference branch uses tag deletion. It verifies a stale
revision at the authenticated API boundary, not the concurrent-edit conflict UI.
The seed is deliberately small and synthetic; no external provider is called.

Authorities: AGENTS.md; D-015, D-043, D-046–D-048, D-050, D-051;
SLICE_011b, SLICE_011b_SORT, SLICE_011c, SLICE_011d, SLICE_011e and SLICE_019b.

## Final combined verification

Run `949f3e952fb3` passed all six families (61 steps), with three browser families
active simultaneously, all images reused and every environment removed. See
[expansion verification](WEB_E2E_EXPANSION.md) for authoritative evidence and the
shared recorder fixes discovered during parallel execution.
