# People workspace visual refresh

Approved by the product owner, 2026-09-06, after reviewing
`docs/design/concepts/people-attio-glass-approved.png`. Checkpoint before
implementation: `0117b87`.

## Scope and plan

1. Apply the approved white/black, muted Attio-inspired typography and
   selective Apple-inspired glass to shared tokens, controls and navigation.
2. Refresh People with a compact header, quiet table and an accessible,
   responsive person inspector. Keep filters, errors and capped counts honest.
3. Reuse the existing organization-scoped Person query, call host and Operator.
   Full profiles remain available; normal links retain new-tab behavior.
4. Record D-045 and update UI_STYLE. No HTTP, realtime, Operator-tool or
   persistence contract changes; no backend changes or new dependencies.
5. User follow-up: bring the main login page into the same white/glass style
   with the black wordmark and quiet typography; preserve authentication,
   credential autocomplete, validation, errors and redirect behavior.

The concept is visual direction, not a new feature specification: do not
invent totals, People search/sort, saved lists, tasks, notes or email sending
that the current product does not implement. Display Last inquiry accurately,
not the concept's unsupported Last activity. Existing warnings, filter
semantics, permissions and call-outcome handling remain authoritative.

Stage tint matching follows D-020's normalized-name convention; custom stages
remain neutral. A preview is selected from returned People and is cleared on
Organization/filter changes. Requests and caches remain organization-scoped.
Preview requests handle loading, denial/not-found, errors and retries without
showing a previously selected person's details. Recent history stays in server
order; full history and outcome correction remain on the full profile.

## Checks

- `pnpm lint`, `pnpm typecheck`, `pnpm test`, `pnpm build` in `web`.
- Regression coverage for selection, full-profile links, preview query scope,
  rapid switching, errors, dismiss/focus, and context sent to the Operator.
- Browser verification of People, filters, preview, full profile, Operator,
  narrow layout and keyboard dismissal. No real calls/messages during QA.
- Reduced-motion, reduced-transparency and forced-colors fallbacks.


## Verification (2026-09-06)

- Checkpoint `0117b87`: lint, typecheck, 346 tests and build passed before edits.
- Refresh: `pnpm lint`, `pnpm typecheck`, `pnpm test` (355 tests, 26 files),
  and `pnpm build` passed. `git diff --check` passed.
- Browser: desktop and 390px-wide People; preview selection, Contact/Activity,
  full-profile/Operator navigation, filter open/apply/Escape and restoring the
  desktop viewport. Calls and email sending were not performed during QA;
  call-host delegation/number selection is covered with mocks.
- Contrast calculation: all new text and stage pairs >=4.5:1 against their
  base backgrounds (body 14.69, muted 5.40, tertiary 4.53, stage text >=5.15).
- Build retains the pre-existing >500kB LiveKit chunk warning. No dependencies
  added. Backend, HTTP contracts and database policy are unchanged.
- Login follow-up: lint, typecheck, all 355 tests and build passed again.
  Browser checked at desktop and 390px: no horizontal overflow, required email
  blocks empty submission, labels and credential autocomplete remain intact.
  Mobile credential text is 16px to prevent iOS focus zoom. The preview used a
  separate unauthenticated browser session; no credentials were submitted.
