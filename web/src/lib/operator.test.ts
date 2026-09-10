import { describe, expect, it, vi } from 'vitest'
import { ApiError } from '../api/client'
import {
  currentUtcOffsetMinutes,
  deriveScreenContext,
  describeOperatorError,
  describeProposalDue,
  describeReceiptDue,
  historyWindow,
  isOrganizationRoute,
  isToggleShortcut,
} from './operator'

const ID = '5a0c1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b7c'

describe('deriveScreenContext (SLICE_005 §10)', () => {
  it('maps each route', () => {
    expect(deriveScreenContext('/today')).toEqual({ route: 'today' })
    expect(deriveScreenContext('/people')).toEqual({ route: 'people' })
    expect(deriveScreenContext(`/people/${ID}`)).toEqual({ route: 'person', person_id: ID })
    expect(deriveScreenContext('/intake/new')).toEqual({ route: 'other' })
    expect(deriveScreenContext('/manage/members')).toEqual({ route: 'other' })
    expect(deriveScreenContext('/')).toEqual({ route: 'other' })
  })

  it('ignores query strings, hashes, and trailing slashes', () => {
    expect(deriveScreenContext('/today?x=1#y')).toEqual({ route: 'today' })
    expect(deriveScreenContext(`/people/${ID}/`)).toEqual({ route: 'person', person_id: ID })
  })

  it('does not forward a non-UUID person segment', () => {
    expect(deriveScreenContext('/people/not-an-id')).toEqual({ route: 'person' })
  })
})

describe('isOrganizationRoute', () => {
  it('hides Ask on platform, invite, and login routes', () => {
    expect(isOrganizationRoute('/today')).toBe(true)
    expect(isOrganizationRoute(`/people/${ID}`)).toBe(true)
    expect(isOrganizationRoute('/platform')).toBe(false)
    expect(isOrganizationRoute('/platform/organizations/x')).toBe(false)
    expect(isOrganizationRoute('/invite/abc')).toBe(false)
    expect(isOrganizationRoute('/login')).toBe(false)
  })
})

describe('historyWindow', () => {
  it('keeps the newest six', () => {
    const messages = Array.from({ length: 9 }, (_, i) => ({
      role: i % 2 === 0 ? ('user' as const) : ('assistant' as const),
      content: `m${i}`,
    }))
    const window = historyWindow(messages)
    expect(window).toHaveLength(6)
    expect(window[0].content).toBe('m3')
    expect(window[5].content).toBe('m8')
    expect(historyWindow([])).toEqual([])
  })

  it('drops oldest entries until the 6000-char total fits', () => {
    const big = (content: string) => ({ role: 'user' as const, content })
    const window = historyWindow([big('a'.repeat(2000)), big('b'.repeat(2000)), big('c'.repeat(2000)), big('d')])
    expect(window.map((m) => m.content[0])).toEqual(['b', 'c', 'd'])
    expect(window.reduce((n, m) => n + m.content.length, 0)).toBeLessThanOrEqual(6000)
  })
})

describe('describeOperatorError (§10 copy)', () => {
  it('uses the exact copy per code', () => {
    expect(describeOperatorError(new ApiError(503, 'operator_disabled'))).toBe(
      'The Operator is not configured on this server.',
    )
    expect(describeOperatorError(new ApiError(503, 'operator_unavailable'))).toBe(
      'The Operator is temporarily unavailable — try again in a moment.',
    )
    expect(describeOperatorError(new ApiError(429, 'operator_busy'))).toBe(
      'One question at a time — wait for the current answer.',
    )
  })

  it('falls back to the generic patterns', () => {
    expect(describeOperatorError(new ApiError(503, 'unavailable'))).toMatch(/temporarily unavailable/)
    expect(describeOperatorError(new ApiError(0, 'network_error'))).toMatch(/Could not reach/)
    expect(describeOperatorError(new ApiError(400, 'malformed_request'))).toBe('Something went wrong. Try again.')
    expect(describeOperatorError(new Error('x'))).toBe('Something went wrong. Try again.')
  })
})

// --- Slice 018: describeReceiptDue / describeProposalDue / currentUtcOffsetMinutes
// (docs/specs/SLICE_018.md §3, §8) ------------------------------------------

describe('describeReceiptDue (SLICE_018 §8)', () => {
  it('today, yesterday, two days ago, null, and a future date — with a pinned now', () => {
    const now = new Date(2026, 8, 15, 10, 0, 0) // local 2026-09-15 10:00

    expect(describeReceiptDue(new Date(2026, 8, 15, 3, 0, 0).toISOString(), now)).toBe('was due today')
    expect(describeReceiptDue(new Date(2026, 8, 14, 23, 0, 0).toISOString(), now)).toBe('was due yesterday')

    const twoDaysAgo = describeReceiptDue(new Date(2026, 8, 13, 12, 0, 0).toISOString(), now)
    expect(twoDaysAgo).toMatch(/^was due /)
    expect(twoDaysAgo).not.toBe('was due today')
    expect(twoDaysAgo).not.toBe('was due yesterday')

    expect(describeReceiptDue(null, now)).toBeNull()

    const future = describeReceiptDue(new Date(2026, 8, 20, 9, 0, 0).toISOString(), now)
    expect(future).toMatch(/^was due /)
    expect(future).not.toBe('was due today')
  })

  it('the day after a DST transition still reads "yesterday" — a calendar day, not a fixed 24h', () => {
    // 2026-03-08 is the US DST-start Sunday (clocks spring forward, a
    // 23-hour local day where the host runs a DST-observing zone); "now"
    // is the very next day. A fixed `now - 24h` computation would miss
    // this local midnight by an hour and read this as "two days ago".
    const now = new Date(2026, 2, 9, 10, 0, 0)
    const dueOnTransitionDay = new Date(2026, 2, 8, 6, 0, 0)
    expect(describeReceiptDue(dueOnTransitionDay.toISOString(), now)).toBe('was due yesterday')
  })
})

describe('describeProposalDue (SLICE_018 §8)', () => {
  it('is null for no due date, and "due …" otherwise', () => {
    expect(describeProposalDue(null)).toBeNull()
    const due = new Date(2026, 8, 12, 23, 59, 0)
    expect(describeProposalDue(due.toISOString())).toMatch(/^due /)
  })
})

describe('currentUtcOffsetMinutes (SLICE_018 §3)', () => {
  it('sign-flips Date.prototype.getTimezoneOffset', () => {
    const spy = vi.spyOn(Date.prototype, 'getTimezoneOffset').mockReturnValue(-330)
    try {
      expect(currentUtcOffsetMinutes()).toBe(330)
    } finally {
      spy.mockRestore()
    }
  })
})

describe('isToggleShortcut', () => {
  it('accepts ⌘K and Ctrl+K only', () => {
    expect(isToggleShortcut({ key: 'k', metaKey: true, ctrlKey: false, altKey: false })).toBe(true)
    expect(isToggleShortcut({ key: 'K', metaKey: false, ctrlKey: true, altKey: false })).toBe(true)
    expect(isToggleShortcut({ key: 'k', metaKey: false, ctrlKey: false, altKey: false })).toBe(false)
    expect(isToggleShortcut({ key: 'k', metaKey: true, ctrlKey: false, altKey: true })).toBe(false)
    expect(isToggleShortcut({ key: 'j', metaKey: true, ctrlKey: false, altKey: false })).toBe(false)
  })
})
