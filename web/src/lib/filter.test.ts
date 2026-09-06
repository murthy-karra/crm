import { describe, expect, it } from 'vitest'
import type { FilterClause } from '../api/types'
import { describeClause, type FilterNames } from './filter'

const names: FilterNames = {
  stageNames: { lead: 'Lead', hot: 'Hot Prospect', nurture: 'Nurture' },
  memberNames: { member: 'Morgan Vale' },
}

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
})
