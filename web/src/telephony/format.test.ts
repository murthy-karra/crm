import { describe, expect, it } from 'vitest'
import type { CallView } from '../api/types'
import { attemptOutcome, callCompletedSummary, formatElapsed, formatTalkSeconds, postCallLine, statusLine } from './format'

function view(overrides: Partial<CallView>): CallView {
  return {
    id: 'c',
    person_id: 'p',
    contact_method_id: 'cm',
    caller: { id: 'u', display_name: 'Alice' },
    status: 'placing',
    failure_reason: null,
    end_reason: null,
    placed_at: '2026-08-22T10:00:00.000Z',
    ringing_at: null,
    answered_at: null,
    ended_at: null,
    talk_seconds: null,
    ...overrides,
  }
}

describe('formatElapsed / formatTalkSeconds', () => {
  it('formats the panel timer', () => {
    expect(formatElapsed(0)).toBe('00:00')
    expect(formatElapsed(12)).toBe('00:12')
    expect(formatElapsed(72)).toBe('01:12')
    expect(formatElapsed(3723)).toBe('1:02:03')
  })

  it('formats history talk time per §1 ("1 min 12 s")', () => {
    expect(formatTalkSeconds(72)).toBe('1 min 12 s')
    expect(formatTalkSeconds(45)).toBe('45 s')
    expect(formatTalkSeconds(0)).toBe('0 s')
    expect(formatTalkSeconds(120)).toBe('2 min')
    expect(formatTalkSeconds(3723)).toBe('1 h 2 min 3 s')
  })
})

describe('callCompletedSummary (§1 steps 4–5)', () => {
  it('renders reached with the talk time and no answer without', () => {
    expect(callCompletedSummary('reached', 72)).toBe('Call — reached, 1 min 12 s')
    expect(callCompletedSummary('reached', null)).toBe('Call — reached')
    expect(callCompletedSummary('no_answer', null)).toBe('Call — no answer')
    expect(callCompletedSummary('busy', null)).toBe('Call — no answer')
    expect(callCompletedSummary('declined', null)).toBe('Call — no answer')
    expect(callCompletedSummary('ring_timeout', null)).toBe('Call — no answer')
    expect(callCompletedSummary('cancelled', null)).toBe('Call — cancelled')
    expect(callCompletedSummary('agent_not_joined', null)).toBe('Call — failed')
    expect(callCompletedSummary('provider_error', null)).toBe('Call — failed')
    expect(callCompletedSummary('expired', null)).toBe('Call — failed')
  })
})

describe('attemptOutcome / postCallLine (D-031 mapping)', () => {
  it('answered or ended → reached', () => {
    expect(attemptOutcome(view({ status: 'answered', answered_at: 'x' }))).toBe('reached')
    expect(attemptOutcome(view({ status: 'ended', end_reason: 'remote_hangup' }))).toBe('reached')
    expect(postCallLine(view({ status: 'ended', end_reason: 'agent_hangup' }))).toBe(
      'Logged as contact attempt — call, reached',
    )
  })

  it('busy / declined / no_answer / ring_timeout → no answer', () => {
    for (const reason of ['busy', 'declined', 'no_answer', 'ring_timeout'] as const) {
      expect(attemptOutcome(view({ status: 'failed', failure_reason: reason }))).toBe('no_answer')
    }
    expect(postCallLine(view({ status: 'failed', failure_reason: 'no_answer' }))).toBe(
      'Logged as contact attempt — call, no answer',
    )
  })

  it('cancelled counts as no answer only once ringing had started', () => {
    expect(attemptOutcome(view({ status: 'failed', failure_reason: 'cancelled', ringing_at: 'x' }))).toBe('no_answer')
    expect(attemptOutcome(view({ status: 'failed', failure_reason: 'cancelled' }))).toBeNull()
  })

  it('nothing reached the callee → no attempt line', () => {
    for (const reason of ['agent_not_joined', 'provider_error', 'expired'] as const) {
      expect(postCallLine(view({ status: 'failed', failure_reason: reason }))).toBeNull()
    }
    expect(postCallLine(view({ status: 'ringing' }))).toBeNull()
    expect(postCallLine(undefined)).toBeNull()
  })
})

describe('statusLine', () => {
  it('reads per phase', () => {
    expect(statusLine('requesting_mic', 0, undefined)).toBe('Connecting…')
    expect(statusLine('joining', 0, undefined)).toBe('Connecting…')
    expect(statusLine('placing', 0, undefined)).toBe('Connecting…')
    expect(statusLine('ringing', 0, undefined)).toBe('Ringing…')
    expect(statusLine('connected', 12, undefined)).toBe('Connected 00:12')
    expect(statusLine('ended', 72, undefined)).toBe('Call ended · 01:12')
    expect(statusLine('failed', 0, view({ status: 'failed', failure_reason: 'declined' }))).toBe('No answer')
    expect(statusLine('failed', 0, undefined)).toBe('Call not connected')
    expect(statusLine('idle', 0, undefined)).toBe('')
  })
})
