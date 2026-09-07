import { describe, expect, it } from 'vitest'
import {
  DEFAULT_SORT,
  nextDirectionFor,
  normalizeSort,
  parseSortToken,
  serializeSort,
  sortLabel,
  sortsEqual,
  type PersonSort,
} from './sort'

const ALL_TOKENS: PersonSort[] = [
  { key: 'created', direction: 'asc' },
  { key: 'created', direction: 'desc' },
  { key: 'name', direction: 'asc' },
  { key: 'name', direction: 'desc' },
  { key: 'stage', direction: 'asc' },
  { key: 'stage', direction: 'desc' },
  { key: 'assignee', direction: 'asc' },
  { key: 'assignee', direction: 'desc' },
]

describe('People sort token parsing (SLICE_011b_SORT.md §11.12 item 1)', () => {
  it('round-trips all eight tokens', () => {
    for (const sort of ALL_TOKENS) {
      const token = serializeSort(sort)
      expect(parseSortToken(token)).toEqual(sort)
    }
  })

  it.each([
    'NAME.asc',
    'name',
    'name.',
    'name.up',
    'inquiry_count.asc',
    '',
    '   ',
    '.asc',
    'name.asc.extra',
    'name.asc.',
    ' name.asc',
    'name.asc ',
  ])('rejects %j', (raw) => {
    expect(parseSortToken(raw)).toBeNull()
  })

  it('normalizes created.desc to no sort, leaving every other token intact', () => {
    expect(normalizeSort({ key: 'created', direction: 'desc' })).toBeUndefined()
    expect(normalizeSort(null)).toBeUndefined()
    expect(normalizeSort(undefined)).toBeUndefined()
    expect(normalizeSort(DEFAULT_SORT)).toBeUndefined()
    for (const sort of ALL_TOKENS) {
      if (sort.key === 'created' && sort.direction === 'desc') continue
      expect(normalizeSort(sort)).toEqual(sort)
    }
  })
})

describe('People sort equality and natural direction', () => {
  it('treats absent, null, and explicit created.desc as the same sort', () => {
    expect(sortsEqual(undefined, null)).toBe(true)
    expect(sortsEqual(undefined, { key: 'created', direction: 'desc' })).toBe(true)
    expect(sortsEqual({ key: 'created', direction: 'desc' }, { key: 'created', direction: 'desc' })).toBe(true)
    expect(sortsEqual({ key: 'name', direction: 'asc' }, undefined)).toBe(false)
    expect(sortsEqual({ key: 'name', direction: 'asc' }, { key: 'name', direction: 'desc' })).toBe(false)
    expect(sortsEqual({ key: 'name', direction: 'asc' }, { key: 'name', direction: 'asc' })).toBe(true)
  })

  it('the first click on a column uses its natural direction; the second toggles', () => {
    expect(nextDirectionFor('name', undefined)).toBe('asc')
    expect(nextDirectionFor('stage', undefined)).toBe('asc')
    expect(nextDirectionFor('assignee', undefined)).toBe('asc')
    expect(nextDirectionFor('created', undefined)).toBe('desc')

    expect(nextDirectionFor('name', { key: 'name', direction: 'asc' })).toBe('desc')
    expect(nextDirectionFor('name', { key: 'name', direction: 'desc' })).toBe('asc')
    expect(nextDirectionFor('created', { key: 'created', direction: 'desc' })).toBe('asc')
    // A different active column does not affect this column's natural first click.
    expect(nextDirectionFor('stage', { key: 'name', direction: 'asc' })).toBe('asc')
  })
})

describe('People sort human labels', () => {
  it('matches the labels named in the spec', () => {
    expect(sortLabel({ key: 'created', direction: 'desc' })).toBe('Added newest first')
    expect(sortLabel({ key: 'created', direction: 'asc' })).toBe('Added oldest first')
    expect(sortLabel({ key: 'name', direction: 'asc' })).toBe('Name (A–Z)')
    expect(sortLabel({ key: 'name', direction: 'desc' })).toBe('Name (Z–A)')
    expect(sortLabel({ key: 'stage', direction: 'asc' })).toBe('Stage (pipeline order)')
    expect(sortLabel({ key: 'stage', direction: 'desc' })).toBe('Stage (reverse pipeline order)')
    expect(sortLabel({ key: 'assignee', direction: 'asc' })).toBe('Assignee (A–Z)')
    expect(sortLabel({ key: 'assignee', direction: 'desc' })).toBe('Assignee (Z–A)')
  })
})
