// Client-side helpers for the People filter vocabulary
// (docs/specs/SLICE_011a.md §4a, §6). The wire shape mirrors
// backend/crates/crm-app/src/domain/person/filter.rs exactly; this module
// stays a THIN, separate client-side reader/writer — `describe()`-parity
// with the backend is explicitly not required (§4d: "the web FilterBar
// renders its own chip labels client-side from data it already has").
import type { AgeOp, Assignee, FilterClause, FilterClauseKind, FilterDefinition } from '../api/types'

export const FILTER_CLAUSE_KINDS: FilterClauseKind[] = [
  'stage',
  'assigned_to',
  'source',
  'created',
  'last_inquiry',
  'last_contact',
  'last_inbound',
  'has_replied',
  'has_phone',
  'has_email',
]

export const CLAUSE_KIND_LABEL: Record<FilterClauseKind, string> = {
  stage: 'Stage',
  assigned_to: 'Assignee',
  source: 'Source (latest inquiry)',
  created: 'Created',
  last_inquiry: 'Last inquiry',
  last_contact: 'Last contact attempt',
  last_inbound: 'Last received email',
  has_replied: 'Received email',
  has_phone: 'Has phone',
  has_email: 'Has email',
}

export const AGE_CLAUSE_KINDS: FilterClauseKind[] = ['created', 'last_inquiry', 'last_contact', 'last_inbound']
export const MULTI_VALUE_CLAUSE_KINDS: FilterClauseKind[] = ['stage', 'assigned_to', 'source']
export const BOOL_CLAUSE_KINDS: FilterClauseKind[] = ['has_replied', 'has_phone', 'has_email']

export function defaultClauseFor(kind: FilterClauseKind): FilterClause {
  switch (kind) {
    case 'stage':
      return { kind, stage_ids: [] }
    case 'assigned_to':
      return { kind, assignees: [] }
    case 'source':
      return { kind, sources: [] }
    case 'created':
    case 'last_inquiry':
    case 'last_contact':
    case 'last_inbound':
      return { kind, age: { op: 'within_days', days: 30 } }
    case 'has_replied':
    case 'has_phone':
    case 'has_email':
      return { kind, value: true }
  }
}

/**
 * A DRAFT clause is a multi-value clause (stage/assigned_to/source) whose
 * value array is still empty — the state right after "Add filter" is
 * clicked, before the user has picked anything (amended §6, review R1
 * fix). Draft clauses are wire-invalid (§4b: an empty value array is a
 * structural 400) and must never be serialized to the URL or the `?filter=`
 * API param. Kept for callers restoring older in-progress state;
 * FilterBar now keeps its drafts local and emits committed clauses only.
 */
export function isDraftClause(clause: FilterClause): boolean {
  if (clause.kind === 'stage') return clause.stage_ids.length === 0
  if (clause.kind === 'assigned_to') return clause.assignees.length === 0
  if (clause.kind === 'source') return clause.sources.length === 0
  return false
}

/** `clauses` with every [`isDraftClause`] entry removed — what's actually
 * eligible for the wire/URL (amended §6). An age/bool clause is already a
 * complete value; merely opening those editors does not create one. */
export function committedClauses(clauses: FilterClause[]): FilterClause[] {
  return clauses.filter((c) => !isDraftClause(c))
}

/** The same percent-encodable JSON both the `?filter=` API param and the
 * URL/query-key element use (§6). Callers pass [`committedClauses`], never
 * the raw in-progress chip list. */
export function serializeFilter(clauses: FilterClause[]): string {
  const filter: FilterDefinition = { version: 1, clauses }
  return JSON.stringify(filter)
}

/**
 * Stable deep JSON form used only when comparing saved definitions. The API
 * can deserialize a typed clause through `serde_json::Value`, whose object
 * member order need not match the local object literal order. Sort object
 * keys recursively, while deliberately retaining clause and value-array
 * order because those are part of the saved definition's request identity.
 *
 * Do not use this for URL/query-key serialization: [`serializeFilter`] keeps
 * the established People URL bytes intact.
 */
function canonicalJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(canonicalJson)
  if (typeof value !== 'object' || value === null) return value
  const record = value as Record<string, unknown>
  return Object.fromEntries(
    Object.keys(record).sort().map((key) => [key, canonicalJson(record[key])]),
  )
}

export function canonicalFilterDefinition(filter: FilterDefinition): string {
  return JSON.stringify(canonicalJson(filter))
}

function isAgeOp(value: unknown): value is AgeOp {
  if (typeof value !== 'object' || value === null) return false
  const v = value as Record<string, unknown>
  if (v.op === 'never') return true
  if (v.op === 'within_days' || v.op === 'not_within_days') return typeof v.days === 'number'
  return false
}

function isAssignee(value: unknown): value is Assignee {
  if (value === 'me' || value === 'unassigned') return true
  if (typeof value !== 'object' || value === null) return false
  return typeof (value as Record<string, unknown>).user_id === 'string'
}

function isFilterClause(value: unknown): value is FilterClause {
  if (typeof value !== 'object' || value === null) return false
  const v = value as Record<string, unknown>
  switch (v.kind) {
    case 'stage':
      return Array.isArray(v.stage_ids) && v.stage_ids.every((x) => typeof x === 'string')
    case 'assigned_to':
      return Array.isArray(v.assignees) && v.assignees.every(isAssignee)
    case 'source':
      return Array.isArray(v.sources) && v.sources.every((x) => typeof x === 'string')
    case 'created':
    case 'last_inquiry':
    case 'last_contact':
    case 'last_inbound':
      return isAgeOp(v.age)
    case 'has_replied':
    case 'has_phone':
    case 'has_email':
      return typeof v.value === 'boolean'
    default:
      return false
  }
}

/**
 * Parses a `?filter=` URL value into clauses, or `null` for anything
 * undecodable or structurally unrecognizable (§6: "An invalid or
 * undecodable URL filter is DROPPED on mount"). Deliberately looser than
 * the server's `deny_unknown_fields`/cap/dedup rules — a filter that
 * parses here but the server still rejects (e.g. duplicate kinds, an
 * org-B stage id) degrades identically via the same drop/clear path once
 * the fetch 400s/422s (§6, review F5); this function only needs to tell
 * "not even shaped like a filter" apart from "shaped like a filter".
 */
export function parseFilter(raw: string): FilterClause[] | null {
  let value: unknown
  try {
    value = JSON.parse(raw)
  } catch {
    return null
  }
  if (typeof value !== 'object' || value === null) return null
  const v = value as Record<string, unknown>
  if (v.version !== 1 || !Array.isArray(v.clauses)) return null
  if (!v.clauses.every(isFilterClause)) return null
  return v.clauses as FilterClause[]
}

export interface FilterNames {
  stageNames: Record<string, string>
  memberNames: Record<string, string>
}

function joinOr(items: string[], maxValues: number): string {
  const visible = items.slice(0, maxValues).join(' or ')
  return items.length > maxValues ? `${visible} +${items.length - maxValues}` : visible
}

function ageLabel(axis: string, age: AgeOp, nullable = true): string {
  if (age.op === 'within_days') return `${axis}: In the last ${age.days} days`
  if (age.op === 'not_within_days') return `${axis}: Not in the last ${age.days} days${nullable ? ' (or never)' : ''}`
  return `${axis}: Never`
}

/** Property/value labels are independent of backend describe() (§4d).
 * Chips can limit visible values; accessible labels retain the full list.
 * Unknown ids never appear as raw UUIDs. */
export function describeClause(clause: FilterClause, names: FilterNames, maxValues = Infinity): string {
  switch (clause.kind) {
    case 'stage': {
      if (clause.stage_ids.length === 0) return 'Stage: Choose a value'
      const labels = clause.stage_ids.map((id) => names.stageNames[id] ?? 'Unknown stage')
      return `Stage: ${joinOr(labels, maxValues)}`
    }
    case 'assigned_to': {
      if (clause.assignees.length === 0) return 'Assignee: Choose a value'
      const labels = clause.assignees.map((a) => {
        if (a === 'me') return 'Me'
        if (a === 'unassigned') return 'Unassigned'
        return names.memberNames[a.user_id] ?? 'Unknown member'
      })
      return `Assignee: ${joinOr(labels, maxValues)}`
    }
    case 'source':
      if (clause.sources.length === 0) return 'Source (latest inquiry): Choose a value'
      return `Source (latest inquiry): ${joinOr(clause.sources, maxValues)}`
    case 'created':
      return ageLabel('Created', clause.age, false)
    case 'last_inquiry':
      return ageLabel('Last inquiry', clause.age)
    case 'last_contact':
      return ageLabel('Last contact attempt', clause.age)
    case 'last_inbound':
      return ageLabel('Last received email', clause.age)
    case 'has_replied':
      return `Received email: ${clause.value ? 'Yes' : 'No'}`
    case 'has_phone':
      return `Has phone: ${clause.value ? 'Yes' : 'No'}`
    case 'has_email':
      return `Has email: ${clause.value ? 'Yes' : 'No'}`
  }
}
