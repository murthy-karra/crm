import { describe, expect, it } from 'vitest'
import { ApiError } from '../api/client'
import {
  deriveScreenContext,
  describeOperatorError,
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

describe('isToggleShortcut', () => {
  it('accepts ⌘K and Ctrl+K only', () => {
    expect(isToggleShortcut({ key: 'k', metaKey: true, ctrlKey: false, altKey: false })).toBe(true)
    expect(isToggleShortcut({ key: 'K', metaKey: false, ctrlKey: true, altKey: false })).toBe(true)
    expect(isToggleShortcut({ key: 'k', metaKey: false, ctrlKey: false, altKey: false })).toBe(false)
    expect(isToggleShortcut({ key: 'k', metaKey: true, ctrlKey: false, altKey: true })).toBe(false)
    expect(isToggleShortcut({ key: 'j', metaKey: true, ctrlKey: false, altKey: false })).toBe(false)
  })
})
