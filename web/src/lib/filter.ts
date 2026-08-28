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
  assigned_to: 'Assigned to',
  source: 'Source',
  created: 'Created',
  last_inquiry: 'Last inquiry',
  last_contact: 'Last contact',
  last_inbound: 'Last inbound',
  has_replied: 'Has replied',
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
 * API param — only [`committedClauses`] may be. They still render as
 * chips (the editor needs somewhere to attach to) via
 * [`describeClause`]'s placeholder text.
 */
export function isDraftClause(clause: FilterClause): boolean {
  if (clause.kind === 'stage') return clause.stage_ids.length === 0
  if (clause.kind === 'assigned_to') return clause.assignees.length === 0
  if (clause.kind === 'source') return clause.sources.length === 0
  return false
}

/** `clauses` with every [`isDraftClause`] entry removed — what's actually
 * eligible for the wire/URL (amended §6). Age/bool clauses always carry a
 * committed default and are never draft. */
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

function joinOr(items: string[]): string {
  return items.join(' or ')
}

function ageLabel(axis: string, neverPhrase: string, age: AgeOp): string {
  if (age.op === 'within_days') return `${axis} within the last ${age.days} days`
  if (age.op === 'not_within_days') return `${axis} not within the last ${age.days} days (or never)`
  return neverPhrase
}

/** A short, human-readable chip label for one clause — this component's
 * own rendering, not a call into the backend's `describe()` (§4d). A
 * DRAFT clause (empty value array — amended §6) renders a "choose a
 * value" placeholder instead of an empty join, since it still renders as
 * a chip while its editor is open. */
export function describeClause(clause: FilterClause, names: FilterNames): string {
  switch (clause.kind) {
    case 'stage': {
      if (clause.stage_ids.length === 0) return `${CLAUSE_KIND_LABEL.stage} — choose a value`
      const labels = clause.stage_ids.map((id) => names.stageNames[id] ?? 'an unknown stage')
      return `Stage is ${joinOr(labels)}`
    }
    case 'assigned_to': {
      if (clause.assignees.length === 0) return `${CLAUSE_KIND_LABEL.assigned_to} — choose a value`
      const labels = clause.assignees.map((a) => {
        if (a === 'me') return 'me'
        if (a === 'unassigned') return 'unassigned'
        return names.memberNames[a.user_id] ?? 'an unknown person'
      })
      return `Assigned to ${joinOr(labels)}`
    }
    case 'source':
      if (clause.sources.length === 0) return `${CLAUSE_KIND_LABEL.source} — choose a value`
      return `Source is ${joinOr(clause.sources)}`
    case 'created':
      return ageLabel('Created', 'Created never (invalid)', clause.age)
    case 'last_inquiry':
      return ageLabel('Last inquiry', 'Never inquired', clause.age)
    case 'last_contact':
      return ageLabel('Last contact', 'Never contacted', clause.age)
    case 'last_inbound':
      return ageLabel('Last inbound message', 'Never received an inbound message', clause.age)
    case 'has_replied':
      return clause.value ? 'Has replied' : 'Has not replied'
    case 'has_phone':
      return clause.value ? 'Has a phone number' : 'No phone number'
    case 'has_email':
      return clause.value ? 'Has an email address' : 'No email address'
  }
}
