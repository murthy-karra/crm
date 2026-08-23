// SLICE_006 §13 item 4: the panel's states, the 409 "hang up previous call"
// affordance, and the §10 error copy rendered verbatim from `useCall`'s
// error. SLICE_006c §13 item 3: the "How did it go?" prompt — iff an
// attempt exists, pre-selection per observed outcome, Save gated on the
// server's terminal status, Skip sends nothing, the saved line, error copy,
// one primary.
import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import type { CallOutcomeCorrection, CallView } from '../api/types'
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
  outcomePrompt?: boolean
  outcomeSaving?: boolean
  outcomeSaved?: CallOutcomeCorrection | null
  outcomeError?: string | null
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

  it('after an answered call: ended line and the "How did it go?" prompt, no Done', () => {
    const w = mountPanel({
      phase: 'ended',
      elapsedSeconds: 72,
      call: view({ status: 'ended', end_reason: 'remote_hangup', talk_seconds: 72 }),
      outcomePrompt: true,
    })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Call ended · 01:12')
    expect(w.get('[data-testid="call-outcome-prompt"]').text()).toBe('How did it go?')
    expect(w.find('[data-testid="call-hangup"]').exists()).toBe(false)
    expect(w.find('[data-testid="call-dismiss"]').exists()).toBe(false)
  })

  it('after a ring-out: "No answer" and the prompt', () => {
    const w = mountPanel({ phase: 'failed', call: view({ status: 'failed', failure_reason: 'ring_timeout', ringing_at: 'x' }), outcomePrompt: true })
    expect(w.get('[data-testid="call-status"]').text()).toBe('No answer')
    expect(w.find('[data-testid="call-outcome-prompt"]').exists()).toBe(true)
  })

  it('a failure before ringing has no prompt — Done only', () => {
    const w = mountPanel({ phase: 'failed', call: view({ status: 'failed', failure_reason: 'provider_error' }) })
    expect(w.get('[data-testid="call-status"]').text()).toBe('Call not connected')
    expect(w.find('[data-testid="call-outcome-prompt"]').exists()).toBe(false)
    expect(w.find('[data-testid="outcome-picker"]').exists()).toBe(false)
    expect(w.find('[data-testid="call-dismiss"]').exists()).toBe(true)
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

/** The owner's `showsOutcomePrompt` says yes: a finished call with an attempt. */
function mountPrompt(props: Parameters<typeof mountPanel>[0]) {
  return mountPanel({ outcomePrompt: true, ...props })
}

function checked(w: ReturnType<typeof mountPanel>): string | undefined {
  return w.get('[data-testid="outcome-picker"] [aria-checked="true"]').attributes('data-outcome')
}

describe('CallPanel — How did it go? (SLICE_006c §10, §13 item 3)', () => {
  it('offers the five choices in order, pre-selecting Talked to them for an answered call', () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'remote_hangup' }) })
    const options = w.findAll('[data-testid="outcome-picker"] [role="radio"]')
    expect(options.map((o) => o.text())).toEqual(['Talked to them', 'Voicemail', 'No answer', 'Busy', 'Wrong number'])
    expect(options.map((o) => o.attributes('data-outcome'))).toEqual(['reached', 'left_message', 'no_answer', 'busy', 'wrong_number'])
    expect(checked(w)).toBe('reached')
  })

  it('pre-selects No answer for a busy / declined / ring-out call', () => {
    for (const failure_reason of ['busy', 'declined', 'ring_timeout'] as const) {
      const w = mountPrompt({ phase: 'failed', call: view({ status: 'failed', failure_reason, ringing_at: 'x' }) })
      expect(checked(w)).toBe('no_answer')
    }
  })

  it('Save is disabled while the server still says answered, enabled once the refetch shows ended', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'answered', answered_at: 'x' }) })
    expect(w.find('[data-testid="call-outcome-prompt"]').exists()).toBe(true)
    const save = w.get('[data-testid="call-outcome-save"]')
    expect(save.attributes('disabled')).toBeDefined()
    expect(w.get('[data-testid="call-outcome-finishing"]').text()).toBe('Finishing up…')
    await w.setProps({ call: view({ status: 'ended', end_reason: 'remote_hangup', answered_at: 'x' }) })
    expect(w.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    expect(w.find('[data-testid="call-outcome-finishing"]').exists()).toBe(false)
    // The pre-selection survived the refetch.
    expect(checked(w)).toBe('reached')
  })

  it('Save is the one primary and Skip is ghost', () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'agent_hangup' }) })
    expect(w.findAll('.bg-accent')).toHaveLength(1)
    expect(w.get('[data-testid="call-outcome-save"]').classes()).toContain('bg-accent')
    expect(w.get('[data-testid="call-outcome-save"]').text()).toBe('Save outcome')
    expect(w.get('[data-testid="call-outcome-skip"]').classes()).toContain('bg-transparent')
  })

  it('Skip emits skip and nothing else', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'agent_hangup' }) })
    await w.get('[data-testid="call-outcome-skip"]').trigger('click')
    expect(w.emitted('skip')).toHaveLength(1)
    expect(w.emitted('save-outcome')).toBeUndefined()
    expect(w.emitted('dismiss')).toBeUndefined()
  })

  it('picking Voicemail then Save emits save-outcome(left_message)', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'remote_hangup' }) })
    await w.get('[data-outcome="left_message"]').trigger('click')
    expect(checked(w)).toBe('left_message')
    await w.get('[data-testid="call-outcome-save"]').trigger('click')
    expect(w.emitted('save-outcome')).toEqual([['left_message']])
  })

  it('a pick is not overwritten when the server view refetches', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'answered', answered_at: 'x' }) })
    await w.get('[data-outcome="busy"]').trigger('click')
    await w.setProps({ call: view({ status: 'ended', end_reason: 'remote_hangup', answered_at: 'x' }) })
    expect(checked(w)).toBe('busy')
  })

  it('while saving: picker and Skip disabled, Save reads Saving…', () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'agent_hangup' }), outcomeSaving: true })
    expect(w.get('[data-testid="call-outcome-save"]').text()).toBe('Saving…')
    expect(w.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeDefined()
    expect(w.get('[data-testid="call-outcome-skip"]').attributes('disabled')).toBeDefined()
    expect(w.get('[data-outcome="busy"]').attributes('disabled')).toBeDefined()
  })

  it('after save: "Outcome saved — voicemail" then Done', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'agent_hangup' }), outcomeSaved: 'left_message' })
    expect(w.get('[data-testid="call-outcome-saved"]').text()).toBe('Outcome saved — voicemail')
    expect(w.find('[data-testid="outcome-picker"]').exists()).toBe(false)
    expect(w.find('[data-testid="call-outcome-save"]').exists()).toBe(false)
    await w.get('[data-testid="call-dismiss"]').trigger('click')
    expect(w.emitted('dismiss')).toHaveLength(1)
  })

  it('renders the owner-supplied error copy with the picker still open', () => {
    const w = mountPrompt({
      phase: 'ended',
      call: view({ status: 'ended', end_reason: 'agent_hangup' }),
      outcomeError: 'This outcome was just changed — refreshed.',
    })
    expect(w.get('[data-testid="call-outcome-error"]').text()).toBe('This outcome was just changed — refreshed.')
    expect(w.find('[data-testid="outcome-picker"]').exists()).toBe(true)
    expect(w.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
  })

  it('a call error (no attempt) shows the error and Done, never the prompt', () => {
    const w = mountPanel({
      phase: 'failed',
      call: view({ status: 'failed', failure_reason: 'provider_error' }),
      error: { code: 'telephony_unavailable', message: 'Calling is temporarily unavailable — try again in a moment.', previousCallId: null },
    })
    expect(w.find('[data-testid="call-outcome-prompt"]').exists()).toBe(false)
    expect(w.find('[data-testid="call-dismiss"]').exists()).toBe(true)
  })
})

describe('CallPanel — prompt guards (SLICE_006c §10)', () => {
  it('the save guard itself emits nothing while the server is non-terminal (button forced enabled)', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'answered', answered_at: 'x' }) })
    const save = w.get('[data-testid="call-outcome-save"]')
    save.element.removeAttribute('disabled')
    await save.trigger('click')
    expect(w.emitted('save-outcome')).toBeUndefined()
  })

  it('the save guard emits nothing while a save is pending (button forced enabled)', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ status: 'ended', end_reason: 'agent_hangup' }), outcomeSaving: true })
    const save = w.get('[data-testid="call-outcome-save"]')
    save.element.removeAttribute('disabled')
    await save.trigger('click')
    expect(w.emitted('save-outcome')).toBeUndefined()
  })

  it('a second call with a different id re-seeds the selection from its observed outcome', async () => {
    const w = mountPrompt({ phase: 'ended', call: view({ id: 'c1', status: 'ended', end_reason: 'agent_hangup' }) })
    await w.get('[data-outcome="busy"]').trigger('click')
    expect(checked(w)).toBe('busy')
    await w.setProps({ phase: 'failed', call: view({ id: 'c2', status: 'failed', failure_reason: 'ring_timeout', ringing_at: 'x' }) })
    expect(checked(w)).toBe('no_answer')
  })

  it('without the owner\'s prompt flag the picker never renders, whatever the call says', () => {
    const w = mountPanel({ phase: 'ended', call: view({ status: 'ended', end_reason: 'agent_hangup' }), outcomePrompt: false })
    expect(w.find('[data-testid="outcome-picker"]').exists()).toBe(false)
    expect(w.find('[data-testid="call-dismiss"]').exists()).toBe(true)
  })
})
