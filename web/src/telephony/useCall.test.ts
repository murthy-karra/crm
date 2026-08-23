// SLICE_006 §13 item 4: `useCall` with a fake LiveKit client — every
// transition, remote leave → `hangup` exactly once, mic denied → `hangup`,
// `call.changed` → refetch, error copy per code. Service-free: `apiFetch`
// is mocked and the room is a fake emitter (no SDK, no WebRTC).
import { effectScope, nextTick, watch } from 'vue'
import { QueryClient } from '@tanstack/vue-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type { CallView, StartCallResponse } from '../api/types'
import { invalidationsFor } from '../realtime/events'
import {
  useCall,
  type CallParticipant,
  type CallRemoteTrack,
  type CallRoom,
  type CallRoomEvents,
  type CallRoomFactory,
} from './useCall'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const PERSON_ID = '33333333-3333-3333-3333-333333333333'
const CONTACT_METHOD_ID = '44444444-4444-4444-4444-444444444444'
const CALL_ID = '55555555-5555-5555-5555-555555555555'
const PREVIOUS_CALL_ID = '66666666-6666-6666-6666-666666666666'
const SIP: CallParticipant = { identity: `sip:${CALL_ID}`, attributes: {} }
const OTHER_AGENT: CallParticipant = { identity: 'agent:someone', attributes: {} }

function callView(overrides: Partial<CallView> = {}): CallView {
  return {
    id: CALL_ID,
    person_id: PERSON_ID,
    contact_method_id: CONTACT_METHOD_ID,
    caller: { id: 'u-alice', display_name: 'Alice' },
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

function startResponse(): StartCallResponse {
  return { call: callView(), join: { url: 'wss://livekit.test', token: 'jwt-not-logged', room: `call:${CALL_ID}` } }
}

class FakeTrack implements CallRemoteTrack {
  kind: string
  attached: HTMLMediaElement[] = []
  constructor(kind = 'audio') {
    this.kind = kind
  }
  attach(): HTMLMediaElement {
    const element = document.createElement('audio')
    this.attached.push(element)
    return element
  }
  detach(): HTMLMediaElement[] {
    const out = this.attached
    this.attached = []
    return out
  }
}

class FakeRoom implements CallRoom {
  handlers: { [E in keyof CallRoomEvents]: CallRoomEvents[E][] } = {
    participantConnected: [],
    participantDisconnected: [],
    participantAttributesChanged: [],
    trackSubscribed: [],
    trackUnsubscribed: [],
    disconnected: [],
  }
  micDenied = false
  connectFails = false
  loadFails = false
  /** When set, `connect` waits on it before resolving. */
  connectGate: Promise<void> | null = null
  /** When set, `acquireMicrophone` waits on it before resolving. */
  micGate: Promise<void> | null = null
  connected: Array<{ url: string; token: string }> = []
  disconnectCalls = 0
  muted: boolean[] = []

  on<E extends keyof CallRoomEvents>(event: E, handler: CallRoomEvents[E]): void {
    this.handlers[event].push(handler)
  }
  emit<E extends keyof CallRoomEvents>(event: E, ...args: Parameters<CallRoomEvents[E]>): void {
    for (const handler of this.handlers[event]) {
      ;(handler as (...a: Parameters<CallRoomEvents[E]>) => void)(...args)
    }
  }
  async load(): Promise<void> {
    if (this.loadFails) throw new Error('chunk load failed')
  }
  async acquireMicrophone(): Promise<void> {
    if (this.micGate) await this.micGate
    if (this.micDenied) throw new DOMException('Permission denied', 'NotAllowedError')
  }
  async connect(url: string, token: string): Promise<void> {
    if (this.connectFails) throw new Error('could not connect')
    if (this.connectGate) await this.connectGate
    this.connected.push({ url, token })
  }
  async setMicrophoneMuted(muted: boolean): Promise<void> {
    this.muted.push(muted)
  }
  async disconnect(): Promise<void> {
    this.disconnectCalls += 1
    // The real SDK emits `disconnected` on its own `disconnect()` too.
    this.emit('disconnected')
  }
}

function harness(configure: (room: FakeRoom) => void = () => {}) {
  const rooms: FakeRoom[] = []
  const createRoom: CallRoomFactory = () => {
    const room = new FakeRoom()
    configure(room)
    rooms.push(room)
    return room
  }
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const scope = effectScope()
  const result = scope.run(() => useCall({ orgId: ORG_ID, createRoom, queryClient }))
  if (!result) throw new Error('effectScope.run returned undefined')
  return { ...result, rooms, queryClient, scope, room: () => rooms[rooms.length - 1] }
}

/** Default happy-path API: start 201, dial 202, hangup 200, get 200. */
function stubApi(overrides: { start?: () => Promise<unknown> } = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === `/people/${PERSON_ID}/calls`) {
      if (overrides.start) return overrides.start()
      return startResponse()
    }
    if (path === `/calls/${CALL_ID}/dial`) return { call: callView() }
    if (path.endsWith('/hangup')) return { call: callView({ status: 'failed', failure_reason: 'cancelled' }) }
    if (path === `/calls/${CALL_ID}` && (init?.method ?? 'GET') === 'GET') return { call: callView() }
    throw new Error(`unexpected ${init?.method ?? 'GET'} ${path}`)
  })
}

function requests(): string[] {
  return apiFetchMock.mock.calls.map(([path, init]) => `${init?.method ?? 'GET'} ${path}`)
}

function hangupRequests(): string[] {
  return requests().filter((r) => r.endsWith('/hangup'))
}

/** Drive a call through start → mic → join → dial, leaving it in `placing`. */
async function placeCall(h: ReturnType<typeof harness>) {
  const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
  await started
  await nextTick()
  return started
}

beforeEach(() => {
  vi.useFakeTimers()
  apiFetchMock.mockReset()
  document.body.innerHTML = ''
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useCall transitions', () => {
  it('starts idle with no call and nothing requested', () => {
    const h = harness()
    expect(h.phase.value).toBe('idle')
    expect(h.callId.value).toBe('')
    expect(h.active.value).toBe(false)
    expect(apiFetchMock).not.toHaveBeenCalled()
    h.scope.stop()
  })

  it('walks requesting_mic → joining → placing, sending only contact_method_id and dialing after joining', async () => {
    stubApi()
    const h = harness()
    const phases: string[] = []
    watch(h.phase, (value) => phases.push(value), { flush: 'sync' })
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    // The SDK chunk loads before the phase leaves idle (a load failure is
    // join_failed with no call); then mic → join → dial.
    expect(phases).toEqual(['requesting_mic', 'joining', 'placing'])
    expect(h.callId.value).toBe(CALL_ID)
    expect(h.personId.value).toBe(PERSON_ID)
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/dial`])
    const startCall = apiFetchMock.mock.calls[0]
    expect(JSON.parse(String(startCall[1]?.body))).toEqual({ contact_method_id: CONTACT_METHOD_ID })
    // The join grant went to the room, verbatim, and the room joined before dial.
    expect(h.room().connected).toEqual([{ url: 'wss://livekit.test', token: 'jwt-not-logged' }])
    h.scope.stop()
  })

  it('passes through joining while the room connects', async () => {
    stubApi()
    let release: () => void = () => {}
    const h = harness((room) => {
      room.connectGate = new Promise<void>((resolve) => {
        release = resolve
      })
    })
    const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.waitFor(() => expect(h.phase.value).toBe('joining'))
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`]) // not dialed yet
    release()
    await started
    expect(h.phase.value).toBe('placing')
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/dial`])
    h.scope.stop()
  })

  it('ringing on the sip participant joining, connected on sip.callStatus=active, ended on remote leave', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()

    room.emit('participantConnected', OTHER_AGENT)
    expect(h.phase.value).toBe('placing') // only sip:* moves the phase

    room.emit('participantConnected', SIP)
    expect(h.phase.value).toBe('ringing')

    room.emit('participantAttributesChanged', { 'sip.callStatus': 'ringing' }, SIP)
    expect(h.phase.value).toBe('ringing')

    room.emit('participantAttributesChanged', { 'sip.callStatus': 'active' }, SIP)
    expect(h.phase.value).toBe('connected')
    expect(h.active.value).toBe(true)

    vi.advanceTimersByTime(12_000)
    expect(h.elapsedSeconds.value).toBe(12)

    room.emit('participantDisconnected', SIP)
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('ended')
    expect(h.active.value).toBe(false)
    expect(hangupRequests()).toEqual([`POST /calls/${CALL_ID}/hangup`])
    expect(room.disconnectCalls).toBe(1)
    // The timer is frozen at the final value.
    vi.advanceTimersByTime(5_000)
    expect(h.elapsedSeconds.value).toBe(12)
    h.scope.stop()
  })

  it('a sub-second answer: sip participant arrives already active', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    h.room().emit('participantConnected', { ...SIP, attributes: { 'sip.callStatus': 'active' } })
    expect(h.phase.value).toBe('connected')
    h.scope.stop()
  })

  it('dialing status keeps ringing; an unknown status is ignored', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'dialing' }, SIP)
    expect(h.phase.value).toBe('ringing')
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'automation' }, SIP)
    expect(h.phase.value).toBe('ringing')
    room.emit('participantAttributesChanged', { 'other.attr': 'x' }, SIP)
    expect(h.phase.value).toBe('ringing')
    h.scope.stop()
  })

  it('remote leave before answer → failed, hangup exactly once', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    room.emit('participantConnected', SIP)
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'ringing' }, SIP)
    room.emit('participantDisconnected', SIP)
    // The SDK may follow with a hangup status and a room disconnect: still one hangup.
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'hangup' }, SIP)
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(h.error.value).toBeNull() // not an error: the server settles no_answer/busy/…
    expect(hangupRequests()).toEqual([`POST /calls/${CALL_ID}/hangup`])
    expect(room.disconnectCalls).toBe(1)
    h.scope.stop()
  })

  it('sip.callStatus=hangup alone ends the call with one hangup', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'active' }, SIP)
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'hangup' }, SIP)
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('ended')
    expect(hangupRequests()).toHaveLength(1)
    h.scope.stop()
  })

  it('local hangup while ringing → failed (server: cancelled), hangup exactly once, room left', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    room.emit('participantConnected', SIP)
    await h.hangup()
    await h.hangup() // a double click is one request
    room.emit('participantDisconnected', SIP) // the remote leg leaving afterwards is a no-op
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(hangupRequests()).toEqual([`POST /calls/${CALL_ID}/hangup`])
    expect(room.disconnectCalls).toBe(1)
    h.scope.stop()
  })

  it('local hangup while connected → ended', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    h.room().emit('participantAttributesChanged', { 'sip.callStatus': 'active' }, SIP)
    await h.hangup()
    expect(h.phase.value).toBe('ended')
    expect(hangupRequests()).toHaveLength(1)
    h.scope.stop()
  })

  it('an unexpected room disconnect ends the call with one hangup', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    room.emit('participantAttributesChanged', { 'sip.callStatus': 'active' }, SIP)
    room.emit('disconnected')
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('ended')
    expect(hangupRequests()).toHaveLength(1)
    h.scope.stop()
  })

  it('mic denied → hangup exactly once, failed with the microphone copy, never joins or dials', async () => {
    stubApi()
    const h = harness((room) => {
      room.micDenied = true
    })
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(h.error.value?.code).toBe('microphone_denied')
    expect(h.error.value?.message).toBe('Microphone access was denied. Allow the microphone and try again.')
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/hangup`])
    expect(h.room().connected).toEqual([])
    expect(h.room().disconnectCalls).toBe(1)
    h.scope.stop()
  })

  it('room.connect failure → hangup once, join_failed copy, never dials', async () => {
    stubApi()
    const h = harness((room) => {
      room.connectFails = true
    })
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(h.error.value?.code).toBe('join_failed')
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/hangup`])
    h.scope.stop()
  })

  it('dial failure → hangup once, failed with the code copy', async () => {
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === `/people/${PERSON_ID}/calls`) return startResponse()
      if (path === `/calls/${CALL_ID}/dial`) throw new ApiError(409, 'invalid_call_state')
      if (path.endsWith('/hangup')) return { call: callView({ status: 'failed', failure_reason: 'cancelled' }) }
      return { call: callView() }
    })
    const h = harness()
    await placeCall(h)
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(h.error.value?.code).toBe('invalid_call_state')
    expect(hangupRequests()).toHaveLength(1)
    expect(h.room().disconnectCalls).toBe(1)
    h.scope.stop()
  })

  it('start is a no-op while a call is active', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    expect(h.rooms).toHaveLength(1)
    expect(requests().filter((r) => r.endsWith('/calls'))).toHaveLength(1)
    h.scope.stop()
  })

  it('dismiss returns to idle only after the call is over', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    h.dismiss()
    expect(h.phase.value).toBe('placing')
    await h.hangup()
    h.dismiss()
    expect(h.phase.value).toBe('idle')
    expect(h.callId.value).toBe('')
    h.scope.stop()
  })

  it('mute toggles the room microphone while active', async () => {
    stubApi()
    const h = harness()
    await h.toggleMute() // idle: no-op
    await placeCall(h)
    await h.toggleMute()
    expect(h.muted.value).toBe(true)
    await h.toggleMute()
    expect(h.muted.value).toBe(false)
    expect(h.room().muted).toEqual([true, false])
    h.scope.stop()
  })

  it('stopping the scope mid-call hangs up once and leaves the room', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    h.scope.stop()
    await vi.runAllTimersAsync()
    expect(hangupRequests()).toHaveLength(1)
    expect(h.room().disconnectCalls).toBe(1)
  })
})

describe('useCall audio', () => {
  it('attaches an audio element only for the sip:* participant and removes it at the end', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    const sipTrack = new FakeTrack()
    const agentTrack = new FakeTrack()
    const sipVideo = new FakeTrack('video')
    room.emit('trackSubscribed', agentTrack, OTHER_AGENT)
    room.emit('trackSubscribed', sipVideo, SIP)
    room.emit('trackSubscribed', sipTrack, SIP)
    expect(agentTrack.attached).toHaveLength(0)
    expect(sipVideo.attached).toHaveLength(0)
    expect(sipTrack.attached).toHaveLength(1)
    expect(document.body.querySelectorAll('audio[data-call-audio]')).toHaveLength(1)

    room.emit('participantDisconnected', SIP)
    await vi.runAllTimersAsync()
    expect(sipTrack.attached).toHaveLength(0)
    expect(document.body.querySelectorAll('audio[data-call-audio]')).toHaveLength(0)
    h.scope.stop()
  })

  it('trackUnsubscribed removes a previously attached element', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    const sipTrack = new FakeTrack()
    room.emit('trackSubscribed', sipTrack, SIP)
    room.emit('trackUnsubscribed', sipTrack, SIP)
    expect(document.body.querySelectorAll('audio[data-call-audio]')).toHaveLength(0)
    h.scope.stop()
  })
})

describe('useCall start errors (§10 copy per code)', () => {
  it.each([
    [new ApiError(503, 'telephony_disabled'), 'Calling is not configured on this server.'],
    [new ApiError(503, 'telephony_unavailable'), 'Calling is temporarily unavailable — try again in a moment.'],
    [new ApiError(422, 'invalid_contact_method'), "That number can't be called."],
    [new ApiError(409, 'call_in_progress', { call_id: PREVIOUS_CALL_ID }), 'You already have a call in progress.'],
    [new ApiError(503, 'unavailable'), 'The server is temporarily unavailable. Try again shortly.'],
    [new ApiError(0, 'network_error'), 'Could not reach the server. Check your connection and try again.'],
    [new ApiError(500, 'unknown_error'), 'Could not place the call.'],
  ])('%s → failed with the exact copy, no room, no hangup', async (failure, message) => {
    stubApi({ start: () => Promise.reject(failure) })
    const h = harness()
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    expect(h.phase.value).toBe('failed')
    expect(h.error.value?.code).toBe(failure.code)
    expect(h.error.value?.message).toBe(message)
    // A room object exists (the SDK was loaded first) but never joined.
    expect(h.rooms).toHaveLength(1)
    expect(h.room().connected).toEqual([])
    expect(h.room().disconnectCalls).toBe(0)
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`])
    h.scope.stop()
  })

  it('a chunk-load failure is join_failed before any call exists', async () => {
    stubApi()
    const h = harness((room) => {
      room.loadFails = true
    })
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    expect(h.phase.value).toBe('failed')
    expect(h.error.value?.code).toBe('join_failed')
    expect(h.error.value?.message).toBe('Could not connect to the call server. Try again in a moment.')
    expect(requests()).toEqual([])
    h.dismiss()
    expect(h.phase.value).toBe('idle')
    h.scope.stop()
  })

  it('409 call_in_progress exposes the previous call id; hangupPrevious hangs it up and clears the error', async () => {
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === `/people/${PERSON_ID}/calls`) {
        throw new ApiError(409, 'call_in_progress', { call_id: PREVIOUS_CALL_ID })
      }
      if (path === `/calls/${PREVIOUS_CALL_ID}/hangup`) {
        return { call: callView({ id: PREVIOUS_CALL_ID, status: 'ended', end_reason: 'agent_hangup' }) }
      }
      throw new Error(`unexpected ${path}`)
    })
    const h = harness()
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    expect(h.error.value?.previousCallId).toBe(PREVIOUS_CALL_ID)
    await h.hangupPrevious()
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${PREVIOUS_CALL_ID}/hangup`])
    expect(h.error.value).toBeNull()
    expect(h.phase.value).toBe('idle')
    h.scope.stop()
  })

  it('hangupPrevious surfaces a failure and keeps the affordance', async () => {
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === `/people/${PERSON_ID}/calls`) {
        throw new ApiError(409, 'call_in_progress', { call_id: PREVIOUS_CALL_ID })
      }
      throw new ApiError(403, 'forbidden')
    })
    const h = harness()
    await h.start(PERSON_ID, CONTACT_METHOD_ID)
    await h.hangupPrevious()
    expect(h.error.value?.code).toBe('forbidden')
    expect(h.error.value?.message).toBe('Only the caller can control this call.')
    expect(h.error.value?.previousCallId).toBe(PREVIOUS_CALL_ID)
    h.scope.stop()
  })
})

describe('useCall and call.changed (D-023: invalidation only)', () => {
  it('seeds the call query from start and refetches GET /api/calls/{id} when the key is invalidated', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    await vi.runAllTimersAsync()
    // The 201 seeded the key; no GET yet.
    expect(h.call.value?.status).toBe('placing')
    expect(requests().filter((r) => r === `GET /calls/${CALL_ID}`)).toHaveLength(0)

    // The server settles `answered`; the realtime layer invalidates exactly
    // the §6 keys and the observer refetches — phase is untouched.
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === `/calls/${CALL_ID}`) {
        return { call: callView({ status: 'answered', answered_at: '2026-08-22T10:00:10.000Z' }) }
      }
      return { call: callView() }
    })
    const event = {
      v: 1,
      type: 'call.changed',
      organization_id: ORG_ID,
      occurred_at: '2026-08-22T10:00:10.000Z',
      correlation_id: 'corr',
      data: { call_id: CALL_ID, person_id: PERSON_ID },
    }
    const keys = invalidationsFor(event, ORG_ID)
    expect(keys).toContainEqual(queryKeys.call(ORG_ID, CALL_ID))
    await Promise.all(keys.map((queryKey) => h.queryClient.invalidateQueries({ queryKey })))
    await vi.runAllTimersAsync()

    expect(requests().filter((r) => r === `GET /calls/${CALL_ID}`)).toHaveLength(1)
    expect(h.call.value?.status).toBe('answered')
    expect(h.phase.value).toBe('placing') // LiveKit, not Centrifugo, drives the phase
    h.scope.stop()
  })

  it('hangup seeds the call key with the settled call and invalidates the Person and Today', async () => {
    stubApi()
    const h = harness()
    const invalidate = vi.spyOn(h.queryClient, 'invalidateQueries')
    await placeCall(h)
    await h.hangup()
    await vi.runAllTimersAsync()
    expect(h.call.value?.status).toBe('failed')
    expect(h.call.value?.failure_reason).toBe('cancelled')
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.person(ORG_ID, PERSON_ID) })
    expect(invalidate).toHaveBeenCalledWith({ queryKey: queryKeys.today(ORG_ID) })
    h.scope.stop()
  })
})

describe('useCall abandoned mid-start (hangup / dispose races)', () => {
  it('dispose during the start POST → no mic, no connect, no dial; exactly one hangup once the id is known', async () => {
    let resolveStart: (value: StartCallResponse) => void = () => {}
    stubApi({
      start: () =>
        new Promise<StartCallResponse>((resolve) => {
          resolveStart = resolve
        }),
    })
    const h = harness()
    const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.waitFor(() => expect(h.phase.value).toBe('requesting_mic'))
    h.scope.stop()
    expect(hangupRequests()).toEqual([]) // no id yet: nothing to hang up
    resolveStart(startResponse())
    await started
    await vi.runAllTimersAsync()
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/hangup`])
    expect(h.room().connected).toEqual([])
    expect(h.phase.value).toBe('failed')
    expect(h.active.value).toBe(false)
  })

  it('hangup() while the microphone prompt is pending → no connect, no dial, stays failed', async () => {
    stubApi()
    let release: () => void = () => {}
    const h = harness((room) => {
      room.micGate = new Promise<void>((resolve) => {
        release = resolve
      })
    })
    const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.waitFor(() => expect(h.callId.value).toBe(CALL_ID))
    await h.hangup()
    expect(h.phase.value).toBe('failed')
    release()
    await started
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(h.active.value).toBe(false)
    expect(h.room().connected).toEqual([])
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/hangup`])
    // The track released after the prompt resolved is dropped too.
    expect(h.room().disconnectCalls).toBe(2)
    h.scope.stop()
  })

  it('hangup() while connect is pending → no dial, stays terminal', async () => {
    stubApi()
    let release: () => void = () => {}
    const h = harness((room) => {
      room.connectGate = new Promise<void>((resolve) => {
        release = resolve
      })
    })
    const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.waitFor(() => expect(h.phase.value).toBe('joining'))
    await h.hangup()
    expect(h.phase.value).toBe('failed')
    release()
    await started
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(requests()).toEqual([`POST /people/${PERSON_ID}/calls`, `POST /calls/${CALL_ID}/hangup`])
    h.scope.stop()
  })

  it('a hung-up attempt is never resurrected by a late mic denial or a late connect failure', async () => {
    stubApi()
    let release: () => void = () => {}
    const h = harness((room) => {
      room.micDenied = true
      room.micGate = new Promise<void>((resolve) => {
        release = resolve
      })
    })
    const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.waitFor(() => expect(h.callId.value).toBe(CALL_ID))
    await h.hangup()
    release()
    await started
    await vi.runAllTimersAsync()
    expect(h.phase.value).toBe('failed')
    expect(h.error.value).toBeNull() // the user hung up; no microphone error is shown
    expect(hangupRequests()).toHaveLength(1)
    h.scope.stop()
  })
})

describe('useCall never retains the join token', () => {
  it('neither cache holds the token after start()', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    await vi.runAllTimersAsync()
    const queries = JSON.stringify(h.queryClient.getQueryCache().getAll().map((q) => q.state))
    const mutations = JSON.stringify(h.queryClient.getMutationCache().getAll().map((m) => m.state))
    expect(queries).not.toContain('jwt-not-logged')
    expect(mutations).not.toContain('jwt-not-logged')
    expect(h.queryClient.getMutationCache().getAll().some((m) => m.state.data && 'join' in (m.state.data as object))).toBe(false)
    h.scope.stop()
  })
})

describe('useCall cache discipline', () => {
  it('a late dial 202 does not regress the call after a call.changed refetch', async () => {
    let resolveDial: (value: unknown) => void = () => {}
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === `/people/${PERSON_ID}/calls`) return startResponse()
      if (path === `/calls/${CALL_ID}/dial`) {
        return new Promise((resolve) => {
          resolveDial = resolve
        })
      }
      if (path === `/calls/${CALL_ID}`) return { call: callView({ status: 'answered', answered_at: 'x' }) }
      return { call: callView() }
    })
    const h = harness()
    const started = h.start(PERSON_ID, CONTACT_METHOD_ID)
    await vi.waitFor(() => expect(h.phase.value).toBe('placing'))
    await h.queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await vi.runAllTimersAsync()
    expect(h.call.value?.status).toBe('answered')
    resolveDial({ call: callView({ status: 'placing' }) })
    await started
    await vi.runAllTimersAsync()
    expect(h.call.value?.status).toBe('answered')
    h.scope.stop()
  })

  it('a failed hangup request invalidates the call key so the panel still reads the settled call', async () => {
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === `/people/${PERSON_ID}/calls`) return startResponse()
      if (path === `/calls/${CALL_ID}/dial`) return { call: callView() }
      if (path.endsWith('/hangup')) throw new ApiError(0, 'network_error')
      if (path === `/calls/${CALL_ID}` && (init?.method ?? 'GET') === 'GET') {
        return { call: callView({ status: 'failed', failure_reason: 'cancelled', ringing_at: 'x' }) }
      }
      throw new Error(`unexpected ${path}`)
    })
    const h = harness()
    await placeCall(h)
    await h.hangup()
    await vi.runAllTimersAsync()
    expect(hangupRequests()).toHaveLength(1)
    expect(requests().filter((r) => r === `GET /calls/${CALL_ID}`)).toHaveLength(1)
    expect(h.call.value?.failure_reason).toBe('cancelled')
    h.scope.stop()
  })

  it('a duplicate trackSubscribed attaches one element; zero remain after the call', async () => {
    stubApi()
    const h = harness()
    await placeCall(h)
    const room = h.room()
    const track = new FakeTrack()
    room.emit('trackSubscribed', track, SIP)
    room.emit('trackSubscribed', track, SIP)
    expect(track.attached).toHaveLength(1)
    expect(document.body.querySelectorAll('audio[data-call-audio]')).toHaveLength(1)
    await h.hangup()
    expect(document.body.querySelectorAll('audio[data-call-audio]')).toHaveLength(0)
    expect(track.attached).toHaveLength(0)
    h.scope.stop()
  })
})
