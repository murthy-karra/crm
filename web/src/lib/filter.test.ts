import { describe, expect, it } from 'vitest'
import type { CustomField, FilterClause, FilterDefinition } from '../api/types'
import { canonicalFilterDefinition, committedClauses, customClauseError, defaultClauseFor, describeClause, FILTER_CLAUSE_KINDS, parseFilter, serializeFilter, type CustomFilterClause, type FilterNames } from './filter'

const names: FilterNames = {
  stageNames: { lead: 'Lead', hot: 'Hot Prospect', nurture: 'Nurture' },
  memberNames: { member: 'Morgan Vale' },
  tagNames: { 'tag-1': 'Investor', 'tag-2': 'Past client' },
}

const fieldId = '11111111-1111-1111-1111-111111111111'
const secondFieldId = '22222222-2222-2222-2222-222222222222'
const optionId = 'aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa'

describe('custom filter wire validation', () => {
  it('round-trips every operator and preserves decimal spelling and operand order', () => {
    const variants: CustomFilterClause[] = [
      ...(['custom_text', 'custom_number', 'custom_date', 'custom_choice'] as const).flatMap((kind) =>
        (['is_set', 'is_not_set'] as const).map((op) => ({ kind, field_id: fieldId, test: { op } }))),
      ...(['contains', 'not_contains'] as const).map((op) => ({ kind: 'custom_text' as const, field_id: fieldId, test: { op, text: 'Keep %_\\ literal' } })),
      ...(['range', 'not_range'] as const).map((op) => ({ kind: 'custom_number' as const, field_id: fieldId, test: { op, min: '-0.00', max: '012.50' } })),
      ...(['range', 'not_range'] as const).map((op) => ({ kind: 'custom_date' as const, field_id: fieldId, test: { op, min: '2000-02-29' } })),
      ...(['any_of', 'none_of'] as const).map((op) => ({ kind: 'custom_choice' as const, field_id: fieldId, test: { op, option_ids: [optionId, secondFieldId] } })),
    ]
    for (const clause of variants) {
      expect(customClauseError(clause)).toBeNull()
      expect(parseFilter(serializeFilter([clause]))).toEqual([clause])
    }
    const oneBound: FilterClause = { kind: 'custom_number', field_id: fieldId, test: { op: 'range', max: '12.50' } }
    expect(serializeFilter([oneBound])).toBe('{"version":1,"clauses":[{"kind":"custom_number","field_id":"11111111-1111-1111-1111-111111111111","test":{"op":"range","max":"12.50"}}]}')
  })

  it.each([
    { kind: 'custom_text', test: { op: 'is_set', text: 'extra' } },
    { kind: 'custom_text', test: { op: 'contains', text: ' padded' } },
    { kind: 'custom_text', test: { op: 'contains', text: 'bad\u0085control' } },
    { kind: 'custom_text', test: { op: 'contains', text: '' } },
    { kind: 'custom_text', test: { op: 'contains', text: '😀'.repeat(501) } },
    { kind: 'custom_text', test: { op: 'contains', text: 2 } },
    { kind: 'custom_text', test: [] },
    { kind: 'custom_text', test: { op: 'is_not_set', extra: true } },
    { kind: 'custom_number', test: { op: 'range' } },
    { kind: 'custom_number', test: { op: 'range', min: null } },
    { kind: 'custom_number', test: { op: 'range', min: 0 } },
    { kind: 'custom_number', test: { op: 'range', min: '1e2' } },
    { kind: 'custom_number', test: { op: 'range', min: '0.00001' } },
    { kind: 'custom_number', test: { op: 'range', max: '1000000000000000' } },
    { kind: 'custom_number', test: { op: 'range', min: '999999999999999.9999', max: '999999999999999.9998' } },
    { kind: 'custom_date', test: { op: 'range', min: '1900-02-29' } },
    { kind: 'custom_date', test: { op: 'range', min: '2026-9-01' } },
    { kind: 'custom_date', test: { op: 'range', min: '2201-01-01' } },
    { kind: 'custom_date', test: { op: 'range', min: '2026-09-02', max: '2026-09-01' } },
    { kind: 'custom_choice', test: { op: 'any_of', option_ids: [] } },
    { kind: 'custom_choice', test: { op: 'any_of', option_ids: [optionId, optionId] } },
    { kind: 'custom_choice', test: { op: 'any_of', option_ids: [optionId.toUpperCase()] } },
    { kind: 'custom_choice', test: { op: 'contains', text: 'name' } },
  ])('rejects invalid custom criteria without partially reading them: %j', (clause) => {
    const value = { ...clause, field_id: fieldId }
    expect(customClauseError(value)).not.toBeNull()
    expect(parseFilter(JSON.stringify({ version: 1, clauses: [value] }))).toBeNull()
  })

  it('accepts exact numeric/date/text boundaries and fifty choices', () => {
    for (const test of [
      { op: 'range', min: '-999999999999999.9999', max: '999999999999999.9999' },
      { op: 'range', min: '-0.0000', max: '0' },
      { op: 'range', min: '2', max: '10' },
    ]) expect(customClauseError({ kind: 'custom_number', field_id: fieldId, test })).toBeNull()
    expect(customClauseError({ kind: 'custom_date', field_id: fieldId, test: { op: 'range', min: '1900-01-01', max: '2200-12-31' } })).toBeNull()
    expect(customClauseError({ kind: 'custom_text', field_id: fieldId, test: { op: 'contains', text: '😀'.repeat(500) } })).toBeNull()
    const ids = Array.from({ length: 51 }, (_, i) => `00000000-0000-0000-0000-${String(i).padStart(12, '0')}`)
    const choice = { kind: 'custom_choice', field_id: fieldId, test: { op: 'none_of', option_ids: ids.slice(0, 50) } }
    expect(customClauseError(choice)).toBeNull()
    expect(customClauseError({ ...choice, test: { ...choice.test, option_ids: ids } })).not.toBeNull()
  })

  it('caps custom fields by identity, preserving two independent fields of the same type', () => {
    const clauses: FilterClause[] = Array.from({ length: 6 }, (_, i) => ({
      kind: 'custom_text', field_id: `00000000-0000-0000-0000-${String(i).padStart(12, '0')}`, test: { op: 'is_set' },
    }))
    expect(parseFilter(serializeFilter(clauses.slice(0, 5)))).toEqual(clauses.slice(0, 5))
    expect(parseFilter(serializeFilter(clauses))).toBeNull()
    expect(parseFilter(serializeFilter([clauses[0], { kind: 'custom_number', field_id: '00000000-0000-0000-0000-000000000000', test: { op: 'is_set' } }]))).toBeNull()
    const oldClauses = FILTER_CLAUSE_KINDS.filter((kind) => !kind.startsWith('custom_')).map(defaultClauseFor)
    expect(parseFilter(serializeFilter([...oldClauses, ...clauses.slice(0, 5)]))).not.toBeNull()
    expect(parseFilter(serializeFilter([...oldClauses, { kind: 'has_phone', value: true }, ...clauses.slice(0, 5)]))).toBeNull()
    expect(committedClauses([{ kind: 'custom_number', field_id: fieldId, test: { op: 'range' } }])).toEqual([])
  })
})

describe('custom field labels', () => {
  const field: CustomField = { id: fieldId, field_type: 'choice', label: 'Temperature', position: 0, archived_at: null, person_count: 1,
    options: [{ id: optionId, label: 'Warm', position: 0, archived_at: '2026-09-10T00:00:00Z' }] }
  const customNames: FilterNames = { ...names, customFields: { [fieldId]: field } }

  it('describes operands and inclusive ranges without hiding negative empty membership', () => {
    expect(describeClause({ kind: 'custom_number', field_id: fieldId, test: { op: 'range', min: '12.50' } }, customNames)).toBe('Temperature: At least 12.50')
    expect(describeClause({ kind: 'custom_number', field_id: fieldId, test: { op: 'not_range', min: '-1', max: '0' } }, customNames)).toBe('Temperature: Not between -1 and 0 (or empty)')
    expect(describeClause({ kind: 'custom_date', field_id: fieldId, test: { op: 'range', max: '2026-09-10' } }, customNames)).toBe('Temperature: On or before 2026-09-10')
    expect(describeClause({ kind: 'custom_choice', field_id: fieldId, test: { op: 'none_of', option_ids: [optionId, secondFieldId] } }, customNames)).toBe('Temperature: Is not one of (or empty) Warm (Archived) or an unavailable option')
  })

  it('retains archived field labels and never exposes unresolved identifiers', () => {
    const missing: FilterClause = { kind: 'custom_choice', field_id: secondFieldId, test: { op: 'any_of', option_ids: [optionId] } }
    expect(describeClause(missing, customNames)).toBe('an unavailable custom field: Is one of an unavailable option')
    expect(describeClause({ kind: 'custom_text', field_id: fieldId, test: { op: 'is_not_set' } }, { ...names, customFields: { [fieldId]: { ...field, archived_at: '2026-09-10T00:00:00Z' } } })).toBe('Temperature (Archived): Is empty')
  })
})

describe('People filter descriptions', () => {
  it('keeps nullable not-within semantics visible and distinguishes created time', () => {
    for (const kind of ['last_contact', 'last_inbound', 'last_inquiry'] as const) {
      expect(describeClause({ kind, age: { op: 'not_within_days', days: 14 } }, names)).toContain('Not in the last 14 days (or never)')
    }
    expect(describeClause({ kind: 'created', age: { op: 'not_within_days', days: 14 } }, names)).toBe('Created: Not in the last 14 days')
    expect(describeClause({ kind: 'last_contact', age: { op: 'never' } }, names)).toBe('Last contact attempt: Never')
    expect(describeClause({ kind: 'last_inbound', age: { op: 'within_days', days: 7 } }, names)).toBe('Last received email: In the last 7 days')
  })

  it('makes property/value and recorded email semantics explicit', () => {
    expect(describeClause({ kind: 'source', sources: ['zillow'] }, names)).toBe('Source (latest inquiry): zillow')
    expect(describeClause({ kind: 'has_replied', value: true }, names)).toBe('Received email: Yes')
    expect(describeClause({ kind: 'has_replied', value: false }, names)).toBe('Received email: No')
    expect(describeClause({ kind: 'has_phone', value: false }, names)).toBe('Has phone: No')
    expect(describeClause({ kind: 'has_email', value: true }, names)).toBe('Has email: Yes')
  })

  it('compacts long selections while retaining a full description for accessible text', () => {
    const clauses: FilterClause[] = [
      { kind: 'stage', stage_ids: ['lead', 'hot', 'nurture'] },
      { kind: 'assigned_to', assignees: ['me', 'unassigned', { user_id: 'member' }] },
      { kind: 'source', sources: ['website', 'zillow', 'email'] },
    ]
    expect(clauses.map((clause) => describeClause(clause, names, 2))).toEqual([
      'Stage: Lead or Hot Prospect +1',
      'Assignee: Me or Unassigned +1',
      'Source (latest inquiry): website or zillow +1',
    ])
    expect(describeClause(clauses[0], names)).toBe('Stage: Lead or Hot Prospect or Nurture')
    expect(describeClause(clauses[1], names)).toBe('Assignee: Me or Unassigned or Morgan Vale')
  })

  it('never displays unresolved identifiers as names', () => {
    expect(describeClause({ kind: 'stage', stage_ids: ['missing-private-id'] }, names)).toBe('Stage: Unknown stage')
    expect(describeClause({ kind: 'assigned_to', assignees: [{ user_id: 'missing-private-id' }] }, names)).toBe('Assignee: Unknown member')
  })

  // SLICE_011d §2's describe() lines verbatim — full sentences per value,
  // not the "Label: Yes/No" pattern the other boolean kinds use.
  it('uses SLICE_011d §2\'s full-sentence describe() lines for the three derived clause kinds', () => {
    expect(describeClause({ kind: 'awaiting_response', value: true }, names)).toBe('Awaiting a response')
    expect(describeClause({ kind: 'awaiting_response', value: false }, names)).toBe('Not awaiting a response')
    expect(describeClause({ kind: 'client_replied_unanswered', value: true }, names)).toBe('Client replied, unanswered')
    expect(describeClause({ kind: 'client_replied_unanswered', value: false }, names)).toBe('No unanswered client reply')
    expect(describeClause({ kind: 'awaiting_call_outcome', value: true }, names)).toBe('A call of mine needs an outcome')
    expect(describeClause({ kind: 'awaiting_call_outcome', value: false }, names)).toBe('No call of mine needs an outcome')
  })

  // Slice 011e e2 (docs/specs/SLICE_011e.md §5, §9.17): one name, several
  // names, and the "an unknown tag" placeholder -- never a raw uuid.
  it('describes tags and not_tags with comma-joined names and the unknown-tag placeholder', () => {
    expect(describeClause({ kind: 'tags', tag_ids: ['tag-1'] }, names)).toBe('Tagged: Investor')
    expect(describeClause({ kind: 'tags', tag_ids: ['tag-1', 'tag-2'] }, names)).toBe('Tagged: Investor, Past client')
    expect(describeClause({ kind: 'not_tags', tag_ids: ['tag-2'] }, names)).toBe('Not tagged: Past client')
    expect(describeClause({ kind: 'tags', tag_ids: ['missing-tag-id'] }, names)).toBe('Tagged: an unknown tag')
    expect(describeClause({ kind: 'not_tags', tag_ids: ['missing-tag-id'] }, names)).toBe('Not tagged: an unknown tag')
  })
})

describe('People filter canonical serialization', () => {
  it('treats recursively reordered object keys as the same filter', () => {
    const first: FilterDefinition = {
      version: 1,
      clauses: [
        { kind: 'assigned_to', assignees: ['me', { user_id: 'member-1' }, 'unassigned'] },
        { kind: 'last_contact', age: { op: 'within_days', days: 14 } },
        { kind: 'has_phone', value: true },
      ],
    }
    const sameFilterWithDifferentObjectInsertionOrder: FilterDefinition = {
      clauses: [
        { assignees: ['me', { user_id: 'member-1' }, 'unassigned'], kind: 'assigned_to' },
        { age: { days: 14, op: 'within_days' }, kind: 'last_contact' },
        { value: true, kind: 'has_phone' },
      ],
      version: 1,
    }

    expect(canonicalFilterDefinition(sameFilterWithDifferentObjectInsertionOrder)).toBe(canonicalFilterDefinition(first))
  })

  it('preserves clause and value-array order while canonicalizing object keys', () => {
    const first: FilterDefinition = {
      version: 1,
      clauses: [
        { kind: 'assigned_to', assignees: ['me', { user_id: 'member-1' }, 'unassigned'] },
        { kind: 'last_contact', age: { op: 'within_days', days: 14 } },
      ],
    }
    const clausesReordered: FilterDefinition = {
      clauses: [
        { age: { days: 14, op: 'within_days' }, kind: 'last_contact' },
        { assignees: ['me', { user_id: 'member-1' }, 'unassigned'], kind: 'assigned_to' },
      ],
      version: 1,
    }
    const assigneesReordered: FilterDefinition = {
      clauses: [
        { assignees: ['unassigned', { user_id: 'member-1' }, 'me'], kind: 'assigned_to' },
        { age: { days: 14, op: 'within_days' }, kind: 'last_contact' },
      ],
      version: 1,
    }

    expect(canonicalFilterDefinition(clausesReordered)).not.toBe(canonicalFilterDefinition(first))
    expect(canonicalFilterDefinition(assigneesReordered)).not.toBe(canonicalFilterDefinition(first))

    const parsed = JSON.parse(canonicalFilterDefinition(first)) as { clauses: FilterClause[] }
    expect(parsed.clauses.map((clause) => clause.kind)).toEqual(['assigned_to', 'last_contact'])
    expect((parsed.clauses[0] as Extract<FilterClause, { kind: 'assigned_to' }>).assignees).toEqual([
      'me', { user_id: 'member-1' }, 'unassigned',
    ])
  })
})
