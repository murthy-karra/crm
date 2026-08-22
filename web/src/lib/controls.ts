// Tailwind class recipes for the control specs in docs/design/UI_STYLE.md
// §5. Centralized so every view renders the same button/input/select
// instead of each screen re-deriving the spec slightly differently.
import type { DialogPassThroughOptions } from 'primevue/dialog'
import type { SelectPassThroughOptions } from 'primevue/select'

const FOCUS_RING =
  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-focus focus-visible:ring-offset-2'

export type ButtonVariant = 'primary' | 'secondary' | 'ghost' | 'danger'

/** UI_STYLE.md §3 semantic badge tints — the closed set; do not invent new ones. */
export type BadgeTint = 'warm' | 'neutral' | 'indigo' | 'green' | 'red'

const BUTTON_BASE = `inline-flex items-center justify-center gap-2 h-10 px-4 rounded-lg text-body font-medium whitespace-nowrap transition-colors duration-150 ease-out disabled:opacity-50 disabled:pointer-events-none ${FOCUS_RING} focus-visible:ring-offset-surface-1`

const BUTTON_VARIANTS: Record<ButtonVariant, string> = {
  primary: `${BUTTON_BASE} bg-accent text-white hover:bg-accent-hover`,
  secondary: `${BUTTON_BASE} bg-surface-0 text-text border border-border hover:bg-surface-1`,
  ghost: `${BUTTON_BASE} bg-transparent text-text hover:bg-surface-2`,
  // SLICE_004: the confirm action for Deactivate/Revoke. Not in UI_STYLE.md
  // §5's control list, but built from its own §3 token ("danger: destructive
  // actions, error text") rather than inventing a new color.
  danger: `${BUTTON_BASE} bg-danger text-white hover:bg-danger/90`,
}

export function buttonClasses(variant: ButtonVariant = 'secondary'): string {
  return BUTTON_VARIANTS[variant]
}

/** Shared by <input>, <textarea>, and Select's `root` part (§5 "looks exactly like an input"). */
export const INPUT_CLASSES = `w-full h-10 px-3 rounded-lg border border-border bg-surface-0 text-body text-text placeholder:text-text-muted transition-colors duration-150 ease-out ${FOCUS_RING} focus-visible:ring-offset-surface-0`

export const TEXTAREA_CLASSES = `w-full min-h-[88px] px-3 py-2 rounded-lg border border-border bg-surface-0 text-body text-text placeholder:text-text-muted transition-colors duration-150 ease-out resize-y ${FOCUS_RING} focus-visible:ring-offset-surface-0`

export const LABEL_CLASSES = 'block text-body font-medium text-text mb-1.5'
export const DESCRIPTION_CLASSES = 'text-body text-text-muted mb-3'
export const HELP_TEXT_CLASSES = 'mt-1.5 text-small text-text-muted'

/**
 * PT object for PrimeVue's unstyled `Select` (docs/specs/SLICE_002.md §10:
 * "Select for stage/assignee ... with local pt objects"). Every section
 * value is a plain class string, which PrimeVue accepts directly for the
 * `class` attribute of that part.
 */
export function selectPt(): SelectPassThroughOptions {
  return {
    root: `${INPUT_CLASSES} flex items-center justify-between gap-2 cursor-pointer text-left`,
    label: 'truncate',
    dropdownIcon: 'w-4 h-4 text-text-muted shrink-0',
    overlay: 'bg-surface-0 border border-border rounded-xl shadow-floating py-1 z-50',
    // The scroll belongs on `listContainer`, not on the `list` <ul>.
    // PrimeVue puts an inline `max-height: {scrollHeight}` (14rem) on the
    // container but, unstyled, ships no `overflow` for it, so a taller
    // <ul> spills out below the panel's background and border and paints
    // over the page behind the menu. Clipping the container keeps the menu
    // inside the box its own surface draws.
    listContainer: 'overflow-auto',
    list: 'py-1',
    // Parameter type is inferred contextually from selectPt()'s declared
    // SelectPassThroughOptions return type — do not annotate it explicitly
    // (a hand-written shape here risks drifting from SelectContext).
    option: ({ context }) => ({
      class: [
        'flex items-center h-10 px-3 text-body cursor-pointer',
        context.focused ? 'bg-surface-2' : '',
        context.selected ? 'font-medium text-text' : 'text-text',
      ],
    }),
    emptyMessage: 'px-3 py-2 text-body text-text-muted',
  }
}

/**
 * PT object for PrimeVue's unstyled `Dialog` (UI_STYLE.md §2 "Floating
 * surfaces (dialogs, popovers, select menus): white, radius 12px, 1px
 * border plus a soft shadow — the only place shadows appear"; §8 "150ms
 * ease-out on ... menus"). `closable: false` on every caller (no styled
 * close-button pt needed) — a Cancel button in the footer plus the default
 * mask-click/Escape dismissal cover it.
 */
export function dialogPt(): DialogPassThroughOptions {
  return {
    mask: {
      class: 'fixed inset-0 z-50 flex items-center justify-center bg-black/40 p-4',
    },
    root: {
      class: 'w-full max-w-md rounded-xl border border-border bg-surface-0 shadow-floating',
    },
    header: {
      class: 'flex items-center justify-between gap-4 px-5 pt-5',
    },
    content: {
      class: 'px-5 py-4',
    },
    footer: {
      class: 'flex items-center justify-end gap-3 px-5 pb-5 pt-2',
    },
    transition: {
      enterFromClass: 'opacity-0 scale-95',
      enterActiveClass: 'transition-all duration-150 ease-out',
      leaveActiveClass: 'transition-all duration-150 ease-out',
      leaveToClass: 'opacity-0 scale-95',
    },
  }
}
