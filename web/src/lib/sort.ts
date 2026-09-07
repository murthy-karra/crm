// Client-side helpers for the People sort vocabulary
// (docs/specs/SLICE_011b_SORT.md §3, §9). The wire token form mirrors
// backend/crates/crm-app/src/domain/person/sort.rs exactly: four keys,
// two directions, dotted lowercase tokens ("name.asc"). This module stays
// a thin, separate client-side reader/writer, the same shape as
// `lib/filter.ts` for the filter vocabulary.
import type { PersonSortToken } from '../api/types'

export type SortKey = 'created' | 'name' | 'stage' | 'assignee'
export type SortDirection = 'asc' | 'desc'

export interface PersonSort {
  key: SortKey
  direction: SortDirection
}

const SORT_KEYS: readonly SortKey[] = ['created', 'name', 'stage', 'assignee']
const SORT_DIRECTIONS: readonly SortDirection[] = ['asc', 'desc']

/** `created.desc` — today's order. Never sent on the wire; see `normalizeSort`. */
export const DEFAULT_SORT: PersonSort = { key: 'created', direction: 'desc' }

/** The column's first-click direction (§9: "Name, Stage and Assignee
 * ascending; Added descending"). Later clicks on the same column toggle. */
export const NATURAL_DIRECTION: Record<SortKey, SortDirection> = {
  created: 'desc',
  name: 'asc',
  stage: 'asc',
  assignee: 'asc',
}

/** Plain column names, matching the People table headers (Added/Name/Stage/Assignee). */
export const COLUMN_LABEL: Record<SortKey, string> = {
  created: 'Added',
  name: 'Name',
  stage: 'Stage',
  assignee: 'Assignee',
}

export function isSortKey(value: string): value is SortKey {
  return (SORT_KEYS as readonly string[]).includes(value)
}

function isSortDirection(value: string): value is SortDirection {
  return (SORT_DIRECTIONS as readonly string[]).includes(value)
}

/** `<key>.<direction>` — the exact eight lowercase dotted tokens the server
 * accepts (§3, §6). */
export function serializeSort(sort: PersonSort): PersonSortToken {
  return `${sort.key}.${sort.direction}` as PersonSortToken
}

/**
 * Parses a `sort` token (from the URL or a saved-list response) into a
 * `PersonSort`, or `null` for anything malformed: wrong case, missing/extra
 * segments, an unknown key or direction, empty or whitespace (§11.12
 * "Parsing"). Deliberately strict — unlike `parseFilter`, there is no server
 * round trip that can loosen this; a token that fails here must never reach
 * the wire or a saved-list request.
 */
export function parseSortToken(raw: string): PersonSort | null {
  const parts = raw.split('.')
  if (parts.length !== 2) return null
  const [key, direction] = parts
  if (!key || !direction) return null
  if (!isSortKey(key) || !isSortDirection(direction)) return null
  return { key, direction }
}

/**
 * `DEFAULT_SORT` (`created.desc`) normalizes to `undefined` — "no sort" and
 * the explicit default are byte-identical everywhere (query keys, the `?sort=`
 * URL param, the saved-list request body, dirty comparison) (§3, §6, §9).
 */
export function normalizeSort(sort: PersonSort | null | undefined): PersonSort | undefined {
  if (!sort) return undefined
  return sort.key === DEFAULT_SORT.key && sort.direction === DEFAULT_SORT.direction ? undefined : sort
}

/** Two `PersonSort | undefined` values compare equal after normalization
 * (used for the saved-list dirty check, §9). */
export function sortsEqual(left: PersonSort | null | undefined, right: PersonSort | null | undefined): boolean {
  const a = normalizeSort(left)
  const b = normalizeSort(right)
  if (a === undefined || b === undefined) return a === b
  return a.key === b.key && a.direction === b.direction
}

/** The direction a header click on `key` will apply next: the column's
 * natural direction when it is not already the active sort, otherwise the
 * toggled direction (§9). `current` is `undefined` for the default order. */
export function nextDirectionFor(key: SortKey, current: PersonSort | undefined): SortDirection {
  if (current?.key === key) return current.direction === 'asc' ? 'desc' : 'asc'
  return NATURAL_DIRECTION[key]
}

/**
 * Human labels for the cap message and the Save as/Duplicate dialog summary
 * (§9: "Added newest/oldest first, Name A–Z/Z–A, Stage pipeline order/reverse,
 * Assignee A–Z/Z–A"; the cap-message example is `"...by Name (A–Z)..."`).
 */
const SORT_LABELS: Record<SortKey, Record<SortDirection, string>> = {
  created: { desc: 'Added newest first', asc: 'Added oldest first' },
  name: { asc: 'Name (A–Z)', desc: 'Name (Z–A)' },
  stage: { asc: 'Stage (pipeline order)', desc: 'Stage (reverse pipeline order)' },
  assignee: { asc: 'Assignee (A–Z)', desc: 'Assignee (Z–A)' },
}

export function sortLabel(sort: PersonSort): string {
  return SORT_LABELS[sort.key][sort.direction]
}
