// Client-side helpers for the People filter vocabulary
// (docs/specs/SLICE_011a.md §4a, §6). The wire shape mirrors
// backend/crates/crm-app/src/domain/person/filter.rs exactly; this module
// stays a THIN, separate client-side reader/writer — `describe()`-parity
// with the backend is explicitly not required (§4d: "the web FilterBar
// renders its own chip labels client-side from data it already has").
import type { AgeOp, Assignee, CustomField, FilterClause, FilterClauseKind, FilterDefinition } from '../api/types'

// SLICE_011d §2: three more derived boolean clause kinds join the vocabulary
// (`awaiting_response`, `client_replied_unanswered`, `awaiting_call_outcome`)
// — usable everywhere this vocabulary is accepted (People, list editing, and
// the Today rules editor's locked-clause mode, FilterBar.vue's
// `lockedAnchorKind`/`requireAssigneeMe` props).
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
  'awaiting_response',
  'client_replied_unanswered',
  'awaiting_call_outcome',
  // Slice 011e e2 (docs/specs/SLICE_011e.md §4a): tags (any-of) / not_tags
  // (none-of), one clause per kind, same 20-clause cap.
  'tags',
  'not_tags',
  'custom_text', 'custom_number', 'custom_date', 'custom_choice',
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
  awaiting_response: 'Awaiting a response',
  client_replied_unanswered: 'Client replied, unanswered',
  awaiting_call_outcome: 'A call of mine needs an outcome',
  tags: 'Tagged',
  not_tags: 'Not tagged',
  custom_text: 'Custom text', custom_number: 'Custom number', custom_date: 'Custom date', custom_choice: 'Custom choice',
}

export const AGE_CLAUSE_KINDS: FilterClauseKind[] = ['created', 'last_inquiry', 'last_contact', 'last_inbound']
export const MULTI_VALUE_CLAUSE_KINDS: FilterClauseKind[] = ['stage', 'assigned_to', 'source', 'tags', 'not_tags']
export const BOOL_CLAUSE_KINDS: FilterClauseKind[] = [
  'has_replied', 'has_phone', 'has_email',
  'awaiting_response', 'client_replied_unanswered', 'awaiting_call_outcome',
]

export type CustomFilterClause = Extract<FilterClause, { field_id: string }>

export function isCustomClause(clause: FilterClause): clause is CustomFilterClause {
  return clause.kind === 'custom_text' || clause.kind === 'custom_number' ||
    clause.kind === 'custom_date' || clause.kind === 'custom_choice'
}

const CANONICAL_UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/
const DECIMAL = /^-?[0-9]{1,15}(\.[0-9]{1,4})?$/

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function exactKeys(value: Record<string, unknown>, allowed: string[]): boolean {
  return Object.keys(value).every((key) => allowed.includes(key))
}

function scaledDecimal(value: string): bigint {
  const [whole, fraction = ''] = value.replace(/^-/, '').split('.')
  const scaled = BigInt(whole) * 10000n + BigInt(fraction.padEnd(4, '0'))
  return value.startsWith('-') ? -scaled : scaled
}

function validCalendarDate(value: string): boolean {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value) || value < '1900-01-01' || value > '2200-12-31') return false
  const [year, month, day] = value.split('-').map(Number)
  const leap = year % 4 === 0 && (year % 100 !== 0 || year % 400 === 0)
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
  return month >= 1 && month <= 12 && day >= 1 && day <= days[month - 1]
}

/** A local draft must be wire-valid before Apply. References remain server-owned;
 * missing/archived metadata can still describe a saved clause for repair. */
export function customClauseError(value: unknown): string | null {
  if (!record(value) || !exactKeys(value, ['kind', 'field_id', 'test']) ||
      !['custom_text', 'custom_number', 'custom_date', 'custom_choice'].includes(String(value.kind))) {
    return 'Choose a supported custom field.'
  }
  if (typeof value.field_id !== 'string' || !CANONICAL_UUID.test(value.field_id)) return 'Choose a custom field.'
  if (!record(value.test)) return 'Choose an operator.'
  const test = value.test
  if (test.op === 'is_set' || test.op === 'is_not_set') {
    return exactKeys(test, ['op']) ? null : 'Presence filters do not take a value.'
  }
  if (value.kind === 'custom_text') {
    if (!['contains', 'not_contains'].includes(String(test.op)) || !exactKeys(test, ['op', 'text'])) return 'Choose a text operator.'
    if (typeof test.text !== 'string' || [...test.text].length < 1 || [...test.text].length > 500) return 'Enter 1–500 characters.'
    if (/\p{Cc}/u.test(test.text)) return 'Remove control characters.'
    if (/^[ \t\r\n]|[ \t\r\n]$/.test(test.text)) return 'Remove spaces at the start or end.'
    return null
  }
  if (value.kind === 'custom_choice') {
    if (!['any_of', 'none_of'].includes(String(test.op)) || !exactKeys(test, ['op', 'option_ids'])) return 'Choose a choice operator.'
    if (!Array.isArray(test.option_ids) || test.option_ids.length < 1 || test.option_ids.length > 50) return 'Choose 1–50 options.'
    if (!test.option_ids.every((id) => typeof id === 'string' && CANONICAL_UUID.test(id)) ||
        new Set(test.option_ids).size !== test.option_ids.length) return 'Choose distinct valid options.'
    return null
  }
  if (!['range', 'not_range'].includes(String(test.op)) || !exactKeys(test, ['op', 'min', 'max'])) return 'Choose a range operator.'
  if (!Object.hasOwn(test, 'min') && !Object.hasOwn(test, 'max')) return 'Enter at least one bound.'
  for (const key of ['min', 'max'] as const) {
    if (!Object.hasOwn(test, key)) continue
    const bound = test[key]
    if (typeof bound !== 'string' || (value.kind === 'custom_number' ? !DECIMAL.test(bound) : !validCalendarDate(bound))) {
      return value.kind === 'custom_number'
        ? 'Use up to 15 digits and 4 decimal places, without commas.'
        : 'Enter a valid date from 1900-01-01 through 2200-12-31.'
    }
  }
  if (typeof test.min === 'string' && typeof test.max === 'string') {
    const reversed = value.kind === 'custom_number'
      ? scaledDecimal(test.min) > scaledDecimal(test.max)
      : test.min > test.max
    if (reversed) return value.kind === 'custom_number' ? 'From must be less than or equal to To.' : 'From must be on or before To.'
  }
  return null
}

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
    case 'awaiting_response':
    case 'client_replied_unanswered':
    case 'awaiting_call_outcome':
      return { kind, value: true }
    case 'tags':
    case 'not_tags':
      return { kind, tag_ids: [] }
    case 'custom_text': return { kind, field_id: '', test: { op: 'is_set' } }
    case 'custom_number': return { kind, field_id: '', test: { op: 'is_set' } }
    case 'custom_date': return { kind, field_id: '', test: { op: 'is_set' } }
    case 'custom_choice': return { kind, field_id: '', test: { op: 'is_set' } }
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
  if (clause.kind === 'tags' || clause.kind === 'not_tags') return clause.tag_ids.length === 0
  if (isCustomClause(clause)) return customClauseError(clause) !== null
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
    case 'awaiting_response':
    case 'client_replied_unanswered':
    case 'awaiting_call_outcome':
      return typeof v.value === 'boolean'
    case 'tags':
    case 'not_tags':
      return Array.isArray(v.tag_ids) && v.tag_ids.every((x) => typeof x === 'string')
    case 'custom_text':
    case 'custom_number':
    case 'custom_date':
    case 'custom_choice':
      return customClauseError(v) === null
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
  const custom = v.clauses.filter(isCustomClause)
  if (custom.length > 5 || v.clauses.length > 20 || new Set(custom.map((clause) => clause.field_id)).size !== custom.length) return null
  return v.clauses as FilterClause[]
}

export interface FilterNames {
  stageNames: Record<string, string>
  memberNames: Record<string, string>
  // Slice 011e e2 (docs/specs/SLICE_011e.md §4d).
  tagNames: Record<string, string>
  customFields?: Record<string, CustomField>
}

function joinOr(items: string[], maxValues: number): string {
  const visible = items.slice(0, maxValues).join(' or ')
  return items.length > maxValues ? `${visible} +${items.length - maxValues}` : visible
}

function joinComma(items: string[], maxValues: number): string {
  const visible = items.slice(0, maxValues).join(', ')
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
    // SLICE_011d §2's describe() lines verbatim — full sentences, not the
    // generic "Label: Yes/No" pattern the other boolean kinds use above.
    case 'awaiting_response':
      return clause.value ? 'Awaiting a response' : 'Not awaiting a response'
    case 'client_replied_unanswered':
      return clause.value ? 'Client replied, unanswered' : 'No unanswered client reply'
    case 'awaiting_call_outcome':
      return clause.value ? 'A call of mine needs an outcome' : 'No call of mine needs an outcome'
    // Slice 011e e2 (docs/specs/SLICE_011e.md §5): 50-value cap matches the
    // server's MAX_VALUES; an unresolvable id never renders the raw uuid.
    case 'tags': {
      if (clause.tag_ids.length === 0) return 'Tagged: Choose a value'
      const labels = clause.tag_ids.map((id) => names.tagNames[id] ?? 'an unknown tag')
      return `Tagged: ${joinComma(labels, Math.min(maxValues, 50))}`
    }
    case 'not_tags': {
      if (clause.tag_ids.length === 0) return 'Not tagged: Choose a value'
      const labels = clause.tag_ids.map((id) => names.tagNames[id] ?? 'an unknown tag')
      return `Not tagged: ${joinComma(labels, Math.min(maxValues, 50))}`
    }
    case 'custom_text':
    case 'custom_number':
    case 'custom_date':
    case 'custom_choice': {
      const field = names.customFields?.[clause.field_id]
      const label = field ? `${field.label}${field.archived_at ? ' (Archived)' : ''}` : 'an unavailable custom field'
      const test = clause.test
      if (test.op === 'is_set') return `${label}: Is not empty`
      if (test.op === 'is_not_set') return `${label}: Is empty`
      if ('text' in test) return `${label}: ${test.op === 'contains' ? 'Contains' : 'Does not contain (or empty)'} ${test.text}`
      if ('option_ids' in test) {
        const options = test.option_ids.map((id) => {
          const option = field?.options.find((candidate) => candidate.id === id)
          return option ? `${option.label}${option.archived_at ? ' (Archived)' : ''}` : 'an unavailable option'
        })
        return `${label}: ${test.op === 'any_of' ? 'Is one of' : 'Is not one of (or empty)'} ${joinOr(options, maxValues)}`
      }
      if ('min' in test || 'max' in test) {
        const bounds = test.min !== undefined && test.max !== undefined
          ? `between ${test.min} and ${test.max}`
          : test.min !== undefined
            ? `${clause.kind === 'custom_date' ? 'on or after' : 'at least'} ${test.min}`
            : `${clause.kind === 'custom_date' ? 'on or before' : 'at most'} ${test.max}`
        const description = test.op === 'not_range' ? `Not ${bounds} (or empty)` : bounds[0].toUpperCase() + bounds.slice(1)
        return `${label}: ${description}`
      }
      return `${label}: Enter at least one bound`
    }
  }
}
