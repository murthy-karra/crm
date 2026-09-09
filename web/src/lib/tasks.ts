// Shared helpers for the Slice 016b Today task surfaces (the ranked item's
// task badge/Waiting cell and the Tasks panel, docs/specs/SLICE_016.md §8).
// Deliberately separate from PersonDetailView.vue's own local task-kind/
// due-badge helpers (016a): duplicated rather than shared so this file
// never has to touch already-reviewed 016a view code, the note precedent
// elsewhere in this codebase for "a new lane's own small helper module".
import type { TaskKind } from '../api/types'

export interface TaskGroups<T> {
  overdue: T[]
  dueSoon: T[]
}

/** Round-2 review fix 3: the Tasks panel's Overdue/Due soon split, pulled
 * out as a pure function and compared with `Date.parse` rather than a raw
 * ISO-string `<` — a whole-second `due_at` ("…T12:00:00Z") sorts AFTER a
 * fractional-second `generated_at` ("…T12:00:00.500000Z") under lexical
 * string comparison ('.' is 0x2E, 'Z' is 0x5A, so the shorter string is
 * "greater"), misclassifying a task due at exactly `generated_at` minus
 * the fractional remainder as "Due soon" instead of "Overdue". */
export function groupTasks<T extends { due_at: string | null }>(
  tasks: T[],
  generatedAt: string,
): TaskGroups<T> {
  const reference = Date.parse(generatedAt)
  const overdue: T[] = []
  const dueSoon: T[] = []
  for (const task of tasks) {
    const isOverdue = task.due_at !== null && Date.parse(task.due_at) < reference
    ;(isOverdue ? overdue : dueSoon).push(task)
  }
  return { overdue, dueSoon }
}

export const TASK_KIND_LABEL: Record<TaskKind, string> = {
  follow_up: 'Follow up',
  call: 'Call',
  email: 'Email',
  text: 'Text',
  other: 'Other',
}

/** The Reasons-column badge text for a `task_overdue`/`task_due` reason
 * (docs/specs/SLICE_016.md §8: "a badge showing the clipped title" — the
 * `list_member` chip precedent, which shows the full list name because
 * list names are short; a task title can run to 500 characters, so this
 * clips). `maxChars` counts Unicode code points, matching the server's own
 * title-length counting (`TaskTitle::parse`), not UTF-16 units. */
export function clipTitle(title: string, maxChars = 40): string {
  const chars = Array.from(title)
  if (chars.length <= maxChars) return title
  return `${chars.slice(0, maxChars).join('')}…`
}

/** Rule 2's client-side date-only-to-local-end-of-day conversion (§1 rule
 * 2), applied to TOMORROW specifically for the Tasks panel's Snooze
 * control ("Tomorrow" posts `due_at` = tomorrow at local end of day). */
export function tomorrowLocalEndOfDay(): string {
  const now = new Date()
  const tomorrow = new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1, 23, 59, 59, 0)
  return tomorrow.toISOString()
}

/** The Tasks panel's due cell (§8): a time for a row due the same local
 * calendar day as `referenceIso` (normally the response's `generated_at`),
 * otherwise the date — "a row past local midnight shows its date in the
 * due cell". */
export function panelDueCellText(dueAtIso: string, referenceIso: string): string {
  const due = new Date(dueAtIso)
  const reference = new Date(referenceIso)
  const sameDay =
    due.getFullYear() === reference.getFullYear() &&
    due.getMonth() === reference.getMonth() &&
    due.getDate() === reference.getDate()
  if (sameDay) return new Intl.DateTimeFormat(undefined, { timeStyle: 'short' }).format(due)
  return new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' }).format(due)
}
