// SLICE_006 §13 item 4: the panel's states, the post-call line, the 409
// "hang up previous call" affordance, and the §10 error copy rendered
// verbatim from `useCall`'s error.
import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { CallView } from '../api/types'
import type { CallError, CallPhase } from '../telephony/useCall'
import CallPanel from './CallPanel.vue'

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

function mountPanel(props: {
  phase: CallPhase
  elapsedSeconds?: number
  muted?: boolean
  error?: CallError | null
  call?: CallView
}) {
  return mount(CallPanel, {
    props: {
      personName: 'Grace Hopper',
      elapsedSeconds: 0,
      muted: false,
      error: null,
      call: undefined,
      ...props,
    },
  })
}

describe('CallPanel', () => {
  it('renders nothing while idle', () => {
    const w = mountPanel({ phase: 'idle' })
    expect(w.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('shows Connecting… with Hang up as the primary while connecting', () => {
    const w = mountPanel({ phase: 'placing' })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Connecting…')
    const hangup = w.get('[data-testid="call-hangup"]')
    expect(hangup.classes()).toContain('bg-accent')
    expect(w.get('[data-testid="call-mute"]').text()).toBe('Mute')
    expect(w.find('[data-testid="call-dismiss"]').exists()).toBe(false)
  })

  it('shows Ringing…, then Connected with the elapsed timer', async () => {
    const w = mountPanel({ phase: 'ringing' })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Ringing…')
    await w.setProps({ phase: 'connected', elapsedSeconds: 12 })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Connected 00:12')
  })

  it('emits hangup and toggle-mute; mute reads Unmute when muted', async () => {
    const w = mountPanel({ phase: 'connected', muted: true })
    expect(w.get('[data-testid="call-mute"]').text()).toBe('Unmute')
    expect(w.get('[data-testid="call-mute"]').attributes('aria-pressed')).toBe('true')
    await w.get('[data-testid="call-hangup"]').trigger('click')
    await w.get('[data-testid="call-mute"]').trigger('click')
    expect(w.emitted('hangup')).toHaveLength(1)
    expect(w.emitted('toggle-mute')).toHaveLength(1)
  })

  it('after an answered call: ended line, "Logged as contact attempt — call, reached", Done dismisses', async () => {
    const w = mountPanel({
      phase: 'ended',
      elapsedSeconds: 72,
      call: view({ status: 'ended', end_reason: 'remote_hangup', talk_seconds: 72 }),
    })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Call ended · 01:12')
    expect(w.get('[data-testid="call-logged"]').text()).toBe('Logged as contact attempt — call, reached')
    expect(w.find('[data-testid="call-hangup"]').exists()).toBe(false)
    await w.get('[data-testid="call-dismiss"]').trigger('click')
    expect(w.emitted('dismiss')).toHaveLength(1)
  })

  it('after a ring-out: "No answer" and the no-answer attempt line', () => {
    const w = mountPanel({ phase: 'failed', call: view({ status: 'failed', failure_reason: 'ring_timeout', ringing_at: 'x' }) })
    expect(w.get('[data-testid="call-status"]').text()).toBe('No answer')
    expect(w.get('[data-testid="call-logged"]').text()).toBe('Logged as contact attempt — call, no answer')
  })

  it('a failure before ringing has no attempt line', () => {
    const w = mountPanel({ phase: 'failed', call: view({ status: 'failed', failure_reason: 'provider_error' }) })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Call not connected')
    expect(w.find('[data-testid="call-logged"]').exists()).toBe(false)
  })

  it.each([
    ['telephony_disabled', 'Calling is not configured on this server.'],
    ['telephony_unavailable', 'Calling is temporarily unavailable — try again in a moment.'],
    ['invalid_contact_method', "That number can't be called."],
    ['microphone_denied', 'Microphone access was denied. Allow the microphone and try again.'],
    ['unknown_error', 'Could not place the call.'],
  ])('renders the %s copy verbatim without the previous-call action', (code, message) => {
    const w = mountPanel({ phase: 'failed', error: { code, message, previousCallId: null } })
    expect(w.get('[data-testid="call-error"]').text()).toBe(message)
    expect(w.find('[data-testid="call-hangup-previous"]').exists()).toBe(false)
    expect(w.find('[data-testid="call-dismiss"]').exists()).toBe(true)
  })

  it('409 call_in_progress offers "Hang up previous call"', async () => {
    const w = mountPanel({
      phase: 'failed',
      error: { code: 'call_in_progress', message: 'You already have a call in progress.', previousCallId: 'prev' },
    })
    expect(w.get('[data-testid="call-error"]').text()).toBe('You already have a call in progress.')
    const action = w.get('[data-testid="call-hangup-previous"]')
    expect(action.text()).toBe('Hang up previous call')
    await action.trigger('click')
    expect(w.emitted('hangup-previous')).toHaveLength(1)
  })
})
