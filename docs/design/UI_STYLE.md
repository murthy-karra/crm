# Web UI Style

Status: ACCEPTED reference for the Vue web client (2026-08-21). Derived
from four user-supplied reference screens (`samples/navattic-0{1..4}.png`,
Navattic's product UI). Applies to every screen from Slice 002 onward;
the Slice 002 frontend lane implements it as the Tailwind theme and the
PrimeVue `pt` objects. Precedence: a slice specification may add to this
but not contradict it.

The one-line brief: **quiet, white, hairline-bordered, generous
whitespace; one dark accent for primary actions; color only where it
carries meaning.**

## 1. Layout

- **App shell**: a fixed sidebar on the left, content on the right.
  Sidebar ≈ 280 px, white, separated from the page by a 1 px hairline
  (no shadow). Samples 1–3 also show a 64 px icon rail left of the
  sidebar; we do **not** ship the rail in Slice 002 (five routes do not
  need two levels of navigation). Keep the sidebar's internal layout so a
  rail can be added later without moving anything.
- **Sidebar content**: product name at top (semibold, 17 px). Groups with
  a small gray sentence-case label (`Analyze`, `Leads`, `Manage` in the
  samples — ours: `Work`, `Intake`, then later `Manage`). Items are
  icon + label, 15 px, 40 px tall, 8 px radius. Active item: light gray
  fill (`surface-2`) and semibold text; no accent color, no left bar.
  Hover: the same fill at lower opacity.
- **Page**: page background `surface-1` (very light gray). Content column
  has 40 px padding, max width ≈ 1280 px, left-aligned (not centered).
- **Page header**: title 30 px semibold, tight tracking; optional one-line
  gray subtitle directly under it (sample 2, 3). Primary page action
  sits at the right of the title row (sample 2, "+ Add Property").
  Breadcrumb above the title only when there is a parent (sample 1).
- **Toolbar row** under the header when needed: filters on the left,
  period/sort controls on the right, all as secondary (outline) buttons.

## 2. Surfaces

- **Card**: white, 1 px border `border`, radius 12 px, no shadow. Cards
  are the only container; never nest a card in a card.
- **Floating surfaces** (dialogs, popovers, select menus): white, radius
  12 px, 1 px border plus a soft shadow (`0 8px 24px rgb(0 0 0 / 0.08)`)
  — the only place shadows appear (sample 4's form preview).
- **Dividers**: 1 px `border`. Prefer spacing over dividers; use them
  inside cards to separate rows (tables, settings rows).

## 3. Color

Tailwind v4 `@theme` tokens. Values are the design intent; the frontend
lane may nudge them by a step for contrast, not restyle.

| Token | Value | Use |
|---|---|---|
| `surface-0` | `#FFFFFF` | cards, sidebar, inputs |
| `surface-1` | `#F9FAFB` | page background, table header row |
| `surface-2` | `#F3F4F6` | active nav item, segmented-control track, hover fills |
| `border` | `#E5E7EB` | every hairline |
| `text` | `#111827` | primary text |
| `text-muted` | `#6B7280` | subtitles, table headers, secondary values, placeholders |
| `text-subtle` | `#9CA3AF` | tertiary (timestamps, "0 last period") |
| `accent` | `#1E1B4B` | primary buttons, toggles-on, chart lines; **one** accent, deep indigo-navy |
| `accent-hover` | `#2E2A6B` | |
| `focus` | `#6366F1` | focus ring (2 px, 2 px offset) and the selected-item outline in sample 4 |
| `danger` | `#DC2626` | destructive actions, error text |
| `success` | `#059669` | success states only |

Semantic tints for badges (background / text), used sparingly:

| Tint | bg | text | meaning |
|---|---|---|---|
| warm | `#FFF4E5` | `#B45309` | source / origin (sample 4's `Visitor` / `Account` badges) |
| neutral | `surface-2` | `text-muted` | counts, stages by default |
| indigo | `#EEF2FF` | `#3730A3` | selected / "you" |
| green | `#ECFDF5` | `#047857` | resolved |
| red | `#FEF2F2` | `#B91C1C` | unresolved / error |

Stages (D-019) render as **neutral** badges; no per-stage colors until a
product decision assigns meaning to them.

**Dark mode**: not in Slice 002. Tokens are named so a dark set can be
added under `prefers-color-scheme` later without touching components.

## 4. Typography

- Family: `Inter`, falling back to the system stack
  (`ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif`).
  Self-hosted via `@fontsource-variable/inter` — no external font
  requests from the app (D-016 tunnel + future CSP).
- Scale (size / line-height / weight):
  - Page title: 30 / 36 / 600, letter-spacing −0.01em
  - Section title (card heading, sample 1 "Advanced Analytics"): 20 / 28 / 600
  - Body and controls: 15 / 22 / 400
  - Small (table headers, captions, badges): 13 / 18 / 500
  - Table header labels: 13 px, `text-muted`, sentence case (sample 2) —
    not uppercase (sample 1 uses uppercase in one widget; we standardize
    on sentence case).
  - Numbers in tables: tabular figures (`font-variant-numeric:
    tabular-nums`), right-aligned when numeric.
- Weight carries hierarchy; color does not. Bold is 600, never 700.

## 5. Controls

- **Primary button**: `accent` fill, white text, 15 px / 500, height
  40 px, radius 8 px, horizontal padding 16 px. One per view at most.
- **Secondary button**: white, 1 px `border`, `text`; same metrics. The
  default for everything else (filters, "Replace", "Try a demo").
- **Ghost / text button**: no border, `text`; for tertiary actions
  ("Talk to Sales") and pagination Previous/Next.
- **Inputs**: white, 1 px `border`, radius 8 px, height 40 px, 12 px
  horizontal padding, 15 px text, placeholder `text-muted`. Label above
  the input (15 px / 500), 6 px gap; optional helper text below in
  `text-muted` 13 px. Focus: `focus` ring, border unchanged. Error:
  `danger` border and a 13 px `danger` message below.
- **Select / combobox** (PrimeVue `Select`, unstyled): looks exactly like
  an input with a trailing chevron; menu is a floating surface with
  40 px rows and a `surface-2` highlighted row.
- **Toggle**: 44 × 24, `accent` when on, `border`-gray when off, white
  knob (sample 2, 3).
- **Segmented control** (sample 2 "Visitor | Company"): `surface-2`
  track, radius 10 px, 4 px inset; selected segment is a white card with
  a hairline. Use for 2–4 mutually exclusive views.
- **Badge**: 13 px / 500, radius 6 px, 2 × 8 px padding, one of the
  tints above.
- **Icons**: Lucide (outline, 1.5 px stroke, 18 px in nav and buttons,
  16 px inline). Monochrome — `text-muted` by default, `text` when active.
  Never a filled or multicolor icon. **One exception** (2026-08-21, product
  owner): the Hot Prospect stage carries a 16 px `Flame` in `danger` red
  wherever its name is rendered — the People table badge and the stage
  `Select`'s value and options, all through `components/StageLabel.vue`.
  Stage-specific markers are otherwise still out (see §9).

## 6. Tables (TanStack Table + `components/DataTable.vue`)

Sample 2 is the reference.

- Lives inside a card; the card provides the border and radius; the
  table has none of its own.
- Header row: `surface-1` background, 13 px `text-muted` labels, 48 px
  tall, 20 px horizontal cell padding.
- Body rows: 56 px tall, hairline dividers, white background, `surface-1`
  on hover; the first column is the entity name in `text` / 500, other
  columns regular `text` or `text-muted` for secondary values. Two-line
  cells (name over email, sample 1's company list) use 15 px / 13 px
  `text-muted`.
- Row click navigates to the detail page; the row is a link, not a
  button.
- Footer row: left — "rows per page" select; right — "1–8 of 8" and
  Previous / Next as secondary buttons, disabled when inapplicable
  (sample 2). For Slice 002, which has no pagination, show only the
  count and a `truncated` notice when the server flag is set.
- Empty state: centered in the card, a short sentence in `text-muted`,
  and the primary action if one applies ("No people yet. Add a lead.").
- No zebra striping, no vertical rules, no sort arrows unless a column is
  sorted.

## 7. Forms and detail pages

Sample 3 (settings) and sample 4 (form builder) are the references.

- A form is a vertical stack of cards, 16 px apart, each card one field
  or one logical group: heading (15 px / 500), optional description
  (`text-muted`, 15 px), then the control. Card padding 20 px.
- Toggle rows: icon tile on the left (36 px, `surface-2`, radius 8 px),
  label + description, toggle right-aligned (sample 3).
- Submit bar: the primary button right-aligned at the bottom of the
  form, not sticky, not in a footer (sample 4).
- Detail pages use the same stack: a header card with the entity's
  identity (name, stage badge, assignee), then cards per section —
  "Contact methods", "Inquiries", "History". The history timeline is a
  list of 56 px rows (icon tile, one-line summary in `text`, actor +
  relative time in `text-muted`), newest at the bottom in Slice 002
  (chronological, matching spec §5 ordering).

## 8. Motion and density

- Transitions 150 ms ease-out on hover fills, focus rings, and menus;
  nothing else animates.
- Density is fixed; no compact mode.
- Minimum interactive target 40 px.

## 9. Non-goals

- No gradients, no colored page backgrounds, no illustration, no
  decorative shadows.
- No accent color on navigation or tables.
- No per-stage colors beyond the single Hot Prospect flame carved out in
  §5, no avatars with random colors (initials on `surface-2` only).
- No component library theme (PrimeVue's Aura/Lara are not used;
  `unstyled: true` with local `pt` objects built from the tokens above).

## 10. What the samples show that we are deliberately not copying

- The icon rail (sample 1–3): deferred, see §1.
- Uppercase table headers (sample 1): standardized to sentence case.
- Colored company squares (sample 1): source of color with no meaning;
  use neutral initials tiles.
- Orange-tinted type badges (sample 4): kept for **source** only, not as
  a general badge color.
