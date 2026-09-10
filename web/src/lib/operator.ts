// Pure helpers for the Ask drawer (docs/specs/SLICE_005.md §10). Kept out
// of the component so each rule is unit-testable without a DOM: screen
// context from the route, the client-carried history window, and the
// error copy per API code.
import { ApiError } from '../api/client'
import type { OperatorHistoryMessage, OperatorScreenContext } from '../api/types'

/** §5/§10: the last 6 messages travel with each turn; older are dropped. */
export const HISTORY_WINDOW = 6
/** §5: `message` is 1–2000 chars after trim. */
export const MAX_MESSAGE_CHARS = 2000
/** §5: the history total is ≤ 6000 chars (the server counts Unicode scalars; UTF-16 units here are ≥ that, so this bound is conservative). */
export const MAX_HISTORY_TOTAL_CHARS = 6000

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i

/**
 * §10 context derivation, from the *current* route at send time: `/today`
 * → `today`; `/people/:id` → `person` + id; `/people` → `people`; anything
 * else → `other`. The id is only forwarded when it looks like a UUID — the
 * server re-validates it through the scope regardless (§7), but a junk
 * segment should not turn a whole turn into a 400.
 */
export function deriveScreenContext(path: string): OperatorScreenContext {
  const clean = path.split('?')[0].split('#')[0].replace(/\/+$/, '')
  if (clean === '/today') return { route: 'today' }
  if (clean === '/people') return { route: 'people' }
  const person = /^\/people\/([^/]+)$/.exec(clean)
  if (person) {
    const id = decodeURIComponent(person[1])
    return UUID_RE.test(id) ? { route: 'person', person_id: id } : { route: 'person' }
  }
  return { route: 'other' }
}

/** §10: the Ask button exists on Organization routes only. */
export function isOrganizationRoute(path: string): boolean {
  return !path.startsWith('/platform') && !path.startsWith('/invite') && path !== '/login'
}

/** The request `history` for the next turn: the newest `HISTORY_WINDOW` entries, oldest dropped until the §5 char total fits. */
export function historyWindow(messages: OperatorHistoryMessage[]): OperatorHistoryMessage[] {
  const window = messages.slice(-HISTORY_WINDOW)
  let total = window.reduce((sum, m) => sum + m.content.length, 0)
  while (total > MAX_HISTORY_TOTAL_CHARS && window.length > 0) {
    total -= window[0].content.length
    window.shift()
  }
  return window
}

/** §10 error copy by code; anything else falls back to the generic pattern Today uses. */
export const OPERATOR_ERROR_COPY: Record<string, string> = {
  operator_disabled: 'The Operator is not configured on this server.',
  operator_unavailable: 'The Operator is temporarily unavailable — try again in a moment.',
  operator_busy: 'One question at a time — wait for the current answer.',
}

export function describeOperatorError(err: unknown): string {
  if (err instanceof ApiError) {
    const copy = OPERATOR_ERROR_COPY[err.code]
    if (copy) return copy
    if (err.status === 0) return 'Could not reach the server. Check your connection and try again.'
    if (err.status === 401) return 'Your session has expired. Redirecting to sign in…'
    if (err.code === 'unavailable') return 'The server is temporarily unavailable. Try again shortly.'
  }
  return 'Something went wrong. Try again.'
}

/** `⌘K` on macOS, `Ctrl+K` elsewhere (§10); both are accepted everywhere. */
export function isToggleShortcut(event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey' | 'altKey'>): boolean {
  return (event.metaKey || event.ctrlKey) && !event.altKey && event.key.toLowerCase() === 'k'
}

// --- Slice 018: complete_task / create_task (docs/specs/SLICE_018.md §3,
// §8) ------------------------------------------------------------------

/** The turn request's `utc_offset_minutes` (§3): `-new Date().
 * getTimezoneOffset()`, sign-flipped so a positive value means "ahead of
 * UTC" (east) — the D-054 §3 client rule with the browser as "the client". */
export function currentUtcOffsetMinutes(): number {
  return -new Date().getTimezoneOffset()
}

/** A `create_task` proposal card's due line (§8's "due Fri 12 Sep, 11:59
 * PM" example): local weekday, day, month, and time. `null` when the
 * proposal carries no due date (the tool reported the date could not be
 * used, or none was asked for). */
export function describeProposalDue(dueAtIso: string | null): string | null {
  if (dueAtIso === null) return null
  const due = new Date(dueAtIso)
  const date = new Intl.DateTimeFormat(undefined, { weekday: 'short', day: 'numeric', month: 'short' }).format(due)
  const time = new Intl.DateTimeFormat(undefined, { timeStyle: 'short' }).format(due)
  return `due ${date}, ${time}`
}

/** A `complete_task` receipt card's due line (§1's "was due today"
 * example): "today"/"yesterday" for the two adjacent local calendar days,
 * else a short date. `null` when the completed task carried no due date.
 * "Yesterday" is `now`'s calendar day minus one, constructed with the
 * `Date` day-rollover constructor — never `now`'s midnight minus a fixed
 * 24h, which is wrong by an hour on either side of a DST transition (a
 * local calendar day is 23h or 25h there, not always 24h). */
export function describeReceiptDue(dueAtIso: string | null, now: Date = new Date()): string | null {
  if (dueAtIso === null) return null
  const due = new Date(dueAtIso)
  const dueMidnight = new Date(due.getFullYear(), due.getMonth(), due.getDate()).getTime()
  const nowMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime()
  const yesterdayMidnight = new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1).getTime()
  if (dueMidnight === nowMidnight) return 'was due today'
  if (dueMidnight === yesterdayMidnight) return 'was due yesterday'
  return `was due ${new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(due)}`
}
