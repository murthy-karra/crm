# Web UI Style

Status: ACCEPTED, revised 2026-09-06 by D-045. The user approved the
Attio-inspired layout with selective Apple-inspired glass in
[the People concept](concepts/people-attio-glass-approved.png). This replaces
the 2026-08-21 Navattic visual metrics; functional slice contracts still apply.

Brief: **white surfaces, muted readable text, compact hierarchy, and a small
amount of polished glass on controls.** Avoid redundant explanations. Every
visible action must work with current application capabilities.

## 1. Layout

- Desktop sidebar: 220px, nearly white, thin right separator. Keep current
  role-aware navigation and organization identity; no mockup-only routes.
- Narrow screens: 64px icon navigation with accessible names/tooltips; account
  logout stays available. Content uses 16px horizontal padding, desktop 32px.
- Page header: compact 22px medium title. A subtitle is optional only when it
  conveys necessary state. Primary action at the right.
- People: All people / My people shortcuts use existing filters, compact
  FilterBar and nearby truthful count, flat table, optional person inspector.
  No fake search, sorting, saved-list tabs, pagination or totals.
- The inspector sits beside the list at >=1200px and overlays it on smaller
  screens. Explicit close and Escape dismiss; focus returns to the selected
  person's link. No modal focus trap: the list remains operable.

## 2. Surfaces

- Main content and tables remain opaque white and flat. Thin neutral dividers.
- Ordinary cards retain a 12px radius and hairline border, no decorative shadow.
- `glass-control`: restrained white-to-silver sheen, bright inset upper edge,
  shallow shadow. Buttons, selected navigation and small floating controls.
- `glass-panel`: white translucent surface, 18px radius, fine rim and soft
  separation shadow. Inspector, popovers, dialogs and Operator panel.
- `glass-primary`: near-black background with a quiet graphite reflection.
- No colorful wallpaper, dramatic refraction, fog over text or inflated bevels.
  Glass must never reduce text legibility. Opaque/reduced-transparency fallback.

## 3. Color

`web/src/style.css` owns Tailwind v4 `@theme` tokens.

| Token | Value | Role |
|---|---|---|
| surface-0 | #FFFFFF | content |
| surface-1 | #FAFAFB | navigation, subtle selection |
| surface-2 | #F0F1F3 | neutral hover and inset fill |
| border | #E7E8EC | dividers |
| text | #27282E | names and headings |
| text-muted | #646772 | labels and secondary values |
| text-subtle | #70737E | tertiary text, still readable |
| accent | #202126 | primary controls |
| accent-hover | #36383F | primary hover |
| focus | #626B80 | keyboard focus |
| danger | #DC2626 | errors/destructive actions |
| success | #059669 | success feedback |

Source, selection, success and error badges retain semantic tints from the
existing theme. D-045 adds stage-specific muted pairs:

| Stage | Background | Text |
|---|---|---|
| Lead | #EDF2F9 | #486789 |
| Active Client | #EDF5F0 | #486E59 |
| Nurture | #F2EEF7 | #6D5889 |
| Hot Prospect | #FAF0E9 | #8D5940 |

Match normalized stage names, as D-020 did for its flame. Other/custom stages
are neutral; no inferred meaning and no storage or API change. Stage names
always accompany color; Hot Prospect retains the small Flame icon.
Dark mode remains deferred; white/black is the approved direction.

## 4. Typography

- Self-hosted Inter Variable; system fallbacks remain.
- Title: 22/30 medium; section: 16/24 medium; body: 14/21 regular;
  small: 13/18 regular or medium. Restrained semibold only where useful.
- Muted metadata, clearer names. No giant headings, uppercase section labels
  or explanatory prose that repeats the screen title.
- Full accessible names and error descriptions stay intact. Never remove
  important filter semantics, truncation/retry feedback or call warnings.

## 5. Controls

- 40px minimum ordinary interactive target; buttons 8px radius, 16px padding.
- One main action per view. Black primary; secondary glass; quiet ghost actions.
- Inputs and selects remain labeled, white, bordered, with visible focus and
  error states. PrimeVue remains unstyled with local pass-through recipes.
- Menus/popovers use glass-panel with an opaque readable interior and bounded
  scrolling. StageLabel owns the flame and optional glass stage badge.
- Lucide monochrome outline icons, 16–18px; icon-only actions need accessible
  names and tooltips. Avatars use neutral initials and a silver-white edge.

## 6. Tables

- TanStack Table remains the shared headless grid. People uses a flat table
  with 50px rows, muted headers and fine horizontal separators.
- One-line People names with a neutral initial avatar; email is accessible in
  the preview's Contact tab and the name tooltip. Full assignee names remain.
- Preserve Inquiries and Last inquiry semantics; never relabel the latter as
  Last activity. Only return counts actually supplied by the current response.
- Plain activation of a People link opens a preview; modified clicks and
  context-menu open-in-new-tab retain real full-profile hrefs. Other tables
  retain their existing navigation/dialog behavior. Links remain focusable.
- No decorative shadows on rows, no vertical rules, no random colored avatars.

## 7. Forms and detail pages

Login uses a single centered glass panel on white, the existing horizontal
black wordmark, a compact Sign in heading, muted visible field labels and a
black glass primary action. Keep credential autocomplete, validation, pending
and error feedback, and post-login routing. No decorative marketing copy.

Existing forms and full Person pages retain their command/receipt behavior.
Shared typography/controls update together. The People preview shows identity,
existing call/email-app/full-profile actions, latest source, owner, added date,
Contact methods and the three most recent presentation rows in server order.
Full history and call outcome correction remain on the full profile. No notes
or email-composer capabilities are invented from the concept.

The preview's Operator action opens the full profile and existing Operator so
its current route supplies the existing person context. Opening never sends a
message. Calls use the app-owned call host; email links open the user's client.

## 8. Motion and accessibility

- Short 150ms transitions; no ambient animation or moving reflections.
- Honor reduced-motion, reduced-transparency and forced-colors preferences.
- Keep focus rings and keyboard dismissal. Glass is progressive decoration,
  with white opaque content when transparency is unavailable.
- Small-screen tables scroll inside their container, not the entire page.

## 9. Non-goals

No new component library/theme, no canvas/WebGL glass renderer, no backend or
wire-contract changes. No mockup-only features, invented customer data, random
stage mappings, dark sidebar, decorative gradients across content or shadows
on every surface.

## 10. References and validation

D-045 supersedes D-020's former single-stage-color restriction and the prior
no-gradient/no-control-shadow visual rules. D-044's logo geometry remains
unchanged; its existing black variant is used in the shell.

Implementation scope/checks: `docs/tasks/UI_REFRESH.md`. The approved concept
is a generated visual reference, not a screenshot of implemented capabilities.
