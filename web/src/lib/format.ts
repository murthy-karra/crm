// Small, dependency-free formatting helpers shared by the table and detail
// views. Deliberately not a date library: the package list for this slice
// (docs/specs/SLICE_002.md §10) does not include one, and Intl covers what
// these screens need.

const RELATIVE_UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 60 * 60 * 24 * 365],
  ['month', 60 * 60 * 24 * 30],
  ['week', 60 * 60 * 24 * 7],
  ['day', 60 * 60 * 24],
  ['hour', 60 * 60],
  ['minute', 60],
]

const relativeFormatter = new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
const absoluteFormatter = new Intl.DateTimeFormat(undefined, {
  dateStyle: 'medium',
  timeStyle: 'short',
})

/** "3 hours ago", "just now" — for §7's "actor + relative time in
 * text-muted". Accepts an epoch-millis number too (TanStack Query's
 * `dataUpdatedAt`, SLICE_003 §10's Today subtitle: "Updated
 * <relative TanStack dataUpdatedAt>"), since `Date`'s constructor already
 * handles both — everywhere else keeps passing an ISO string. */
export function formatRelativeTime(iso: string | number): string {
  const date = new Date(iso)
  const seconds = (date.getTime() - Date.now()) / 1000

  for (const [unit, unitSeconds] of RELATIVE_UNITS) {
    if (Math.abs(seconds) >= unitSeconds) {
      return relativeFormatter.format(Math.round(seconds / unitSeconds), unit)
    }
  }
  return seconds < 0 ? 'just now' : relativeFormatter.format(Math.round(seconds), 'second')
}

/** Absolute timestamp for a `title` attribute alongside the relative one. */
export function formatAbsoluteTime(iso: string): string {
  return absoluteFormatter.format(new Date(iso))
}

/** Up to two initials for a neutral avatar tile (UI_STYLE.md §9). */
export function initials(name: string): string {
  const parts = name.trim().split(/\s+/).filter(Boolean)
  if (parts.length === 0) return ''
  if (parts.length === 1) return parts[0].slice(0, 2).toUpperCase()
  return (parts[0][0] + parts[parts.length - 1][0]).toUpperCase()
}

/** "482 B", "1.3 KB" for the Unresolved queue's byte_len column. */
export function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  const kb = bytes / 1024
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`
  const mb = kb / 1024
  return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`
}
