// The browser side of one outbound call (docs/specs/SLICE_006.md §10;
// D-023, D-031). A state machine
//
//   idle → requesting_mic → joining → placing → ringing → connected → ended
//                                                                    ↘ failed
//
// driven by `POST /api/people/{id}/calls` → `room.connect(url, token)` →
// `POST /api/calls/{id}/dial` → LiveKit's own participant events for the
// `sip:*` leg. The LiveKit SDK is always injected via `createRoom` (the real
// adapter lives in client.ts; tests pass a fake), the same pattern as
// realtime/useRealtime.ts, so this module never imports `livekit-client`.
//
// Hard rules (§10, the Lane B brief):
// - the client never sends a phone number — only `contact_method_id`;
// - `hangup` is sent exactly once per call, whichever of local Hang up,
//   remote leave, mic denial, dial failure, or room disconnect ends it;
// - audio elements are created only for the `sip:*` participant;
// - no call state is persisted beyond this composable's refs (the join
//   token lives in a local for the duration of `connect` and nowhere else);
// - Centrifugo's `call.changed` is invalidation-only: it refetches
//   `GET /api/calls/{id}` (the `call` ref below) and never drives `phase`.
import { computed, onScopeDispose, ref, toValue, type MaybeRefOrGetter, type Ref } from 'vue'
import { useQueryClient, type QueryClient } from '@tanstack/vue-query'
import { queryKeys, useCall as useCallQuery, useDialCall, useHangupCall, useStartCall } from '../api/queries'
import type { CallView } from '../api/types'
import { CallClientError, callInProgressId, describeCallError } from './errors'

export type CallPhase = 'idle' | 'requesting_mic' | 'joining' | 'placing' | 'ringing' | 'connected' | 'ended' | 'failed'

/** LiveKit's `sip.callStatus` participant attribute values (§10). Anything
 * else (e.g. `automation`) is ignored rather than guessed. */
export type SipCallStatus = 'dialing' | 'ringing' | 'active' | 'hangup'

export const SIP_CALL_STATUS_ATTRIBUTE = 'sip.callStatus'
const SIP_IDENTITY_PREFIX = 'sip:'

// Minimal structural surface this composable needs from a LiveKit `Room` —
// satisfied by client.ts's adapter over the real SDK and by a fake in tests.
// Narrower than the SDK's own types on purpose: only the six events and
// four methods used here.
export interface CallParticipant {
  identity: string
  attributes: Record<string, string>
}

export interface CallRemoteTrack {
  /** `'audio'` for the SIP leg's audio; anything else is never attached. */
  kind: string
  attach(): HTMLMediaElement
  detach(): HTMLMediaElement[]
}

/** The LiveKit room events this composable consumes, by their SDK names. */
export interface CallRoomEvents {
  participantConnected: (participant: CallParticipant) => void
  participantDisconnected: (participant: CallParticipant) => void
  participantAttributesChanged: (changed: Record<string, string>, participant: CallParticipant) => void
  trackSubscribed: (track: CallRemoteTrack, participant: CallParticipant) => void
  trackUnsubscribed: (track: CallRemoteTrack, participant: CallParticipant) => void
  disconnected: () => void
}

export interface CallRoom {
  on<E extends keyof CallRoomEvents>(event: E, handler: CallRoomEvents[E]): unknown
  /** Loads whatever the room needs before a call can start (the real
   * adapter's lazily imported SDK chunk). Rejects when that fails, so a
   * chunk-load failure is reported as `join_failed` before any call is
   * created — never as a microphone problem. */
  load(): Promise<void>
  /** Asks for microphone permission and prepares the local audio track.
   * Rejects when the browser denies it. Called before `connect` so the
   * callee never answers to silence (§5 "why two steps"). */
  acquireMicrophone(): Promise<void>
  /** Joins the room with the one-room grant and publishes the microphone. */
  connect(url: string, token: string): Promise<void>
  setMicrophoneMuted(muted: boolean): Promise<void>
  /** Leaves the room and releases the microphone. Idempotent. */
  disconnect(): Promise<void>
}

export type CallRoomFactory = () => CallRoom

export interface CallError {
  /** The `ApiError`/`CallClientError` code, for tests and the 409 affordance. */
  code: string
  /** §10 copy, via telephony/errors.ts. */
  message: string
  /** Set for 409 `call_in_progress`: the call the panel can offer to hang up. */
  previousCallId: string | null
}

export interface UseCallOptions {
  orgId: MaybeRefOrGetter<string>
  createRoom: CallRoomFactory
  /** Defaults to the Vue-provided client; tests pass their own. */
  queryClient?: QueryClient
}

export interface UseCallResult {
  phase: Ref<CallPhase>
  /** '' when no call has been started (or after `dismiss`). */
  callId: Ref<string>
  personId: Ref<string>
  /** The server's authoritative `CallView` — refetched on `call.changed`. */
  call: Ref<CallView | undefined>
  error: Ref<CallError | null>
  muted: Ref<boolean>
  /** Whole seconds since `connected`; frozen at the final value after the call. */
  elapsedSeconds: Ref<number>
  /** True from `requesting_mic` through `connected`. */
  active: Ref<boolean>
  start(personId: string, contactMethodId: string): Promise<void>
  hangup(): Promise<void>
  /** The 409 affordance: hangs up `error.previousCallId`, then clears the error. */
  hangupPrevious(): Promise<void>
  toggleMute(): Promise<void>
  /** Back to `idle` after `ended`/`failed`. No-op while a call is active. */
  dismiss(): void
}

const ACTIVE_PHASES: ReadonlySet<CallPhase> = new Set(['requesting_mic', 'joining', 'placing', 'ringing', 'connected'])

function isSipParticipant(participant: CallParticipant): boolean {
  return participant.identity.startsWith(SIP_IDENTITY_PREFIX)
}

export function useCall(options: UseCallOptions): UseCallResult {
  const phase = ref<CallPhase>('idle')
  const callId = ref('')
  const personId = ref('')
  const error = ref<CallError | null>(null)
  const muted = ref(false)
  const elapsedSeconds = ref(0)
  const active = computed(() => ACTIVE_PHASES.has(phase.value))

  const qc = options.queryClient ?? useQueryClient()
  const startMutation = useStartCall(options.orgId, options.queryClient)
  const dialMutation = useDialCall(options.orgId, options.queryClient)
  const hangupMutation = useHangupCall(options.orgId, options.queryClient)
  const { data: callData } = useCallQuery(options.orgId, callId, options.queryClient)
  const call = computed(() => callData.value?.call)

  // One `Session` per call attempt; events from a previous room (its
  // `disconnected` arriving after a new call started) are dropped by
  // identity comparison rather than by guessing from phase.
  interface Session {
    room: CallRoom | null
    hangupSent: boolean
    ending: boolean
    wasConnected: boolean
    connectedAt: number | null
    timer: ReturnType<typeof setInterval> | null
    audioElements: Map<CallRemoteTrack, HTMLMediaElement>
  }
  let session: Session | null = null
  // Re-entrancy guard for the window before `phase` leaves `idle` (the SDK
  // chunk load in `start`), when `active` is still false.
  let starting = false

  function stopTimer(s: Session): void {
    if (s.timer !== null) {
      clearInterval(s.timer)
      s.timer = null
    }
    if (s.connectedAt !== null) {
      elapsedSeconds.value = Math.floor((Date.now() - s.connectedAt) / 1000)
    }
  }

  function detachAll(s: Session): void {
    for (const [track, element] of s.audioElements) {
      track.detach()
      element.remove()
    }
    s.audioElements.clear()
  }

  /** Sends `hangup` for the current call — exactly once per session. The
   * server call is idempotent, but the client still guards so every path
   * (local, remote leave, mic denial, disconnect) converges on one request. */
  function sendHangupOnce(s: Session, id: string): Promise<void> {
    if (s.hangupSent || id === '') return Promise.resolve()
    s.hangupSent = true
    return hangupMutation.mutateAsync(id).then(
      () => undefined,
      () => {
        // Idempotent server-side; a lost request is the sweep's job (§9).
        // The call key was not seeded with the settled call, so refetch it
        // for the panel's post-call line.
        void qc.invalidateQueries({ queryKey: queryKeys.call(toValue(options.orgId), id) })
      },
    )
  }

  /** The single exit path. Stops the timer, detaches audio, hangs up once,
   * leaves the room, and sets the terminal phase. */
  async function endCall(s: Session, finalPhase: 'ended' | 'failed', failure?: unknown): Promise<void> {
    if (s.ending) return
    s.ending = true
    stopTimer(s)
    detachAll(s)
    if (failure !== undefined) {
      error.value = { code: errorCode(failure), message: describeCallError(failure), previousCallId: null }
    }
    phase.value = finalPhase
    const hangup = sendHangupOnce(s, callId.value)
    const room = s.room
    s.room = null
    if (room) {
      await room.disconnect().catch(() => undefined)
    }
    await hangup
  }

  function errorCode(failure: unknown): string {
    if (failure instanceof CallClientError) return failure.code
    if (typeof failure === 'object' && failure !== null && 'code' in failure) {
      const code = (failure as { code: unknown }).code
      if (typeof code === 'string') return code
    }
    return 'unknown_error'
  }

  function applySipStatus(s: Session, status: string): void {
    if (s.ending) return
    switch (status as SipCallStatus) {
      case 'dialing':
      case 'ringing':
        if (phase.value === 'placing') phase.value = 'ringing'
        return
      case 'active':
        if (phase.value === 'placing' || phase.value === 'ringing') {
          phase.value = 'connected'
          s.wasConnected = true
          s.connectedAt = Date.now()
          elapsedSeconds.value = 0
          s.timer = setInterval(() => {
            if (s.connectedAt !== null) elapsedSeconds.value = Math.floor((Date.now() - s.connectedAt) / 1000)
          }, 1000)
        }
        return
      case 'hangup':
        void endCall(s, s.wasConnected ? 'ended' : 'failed')
        return
      default:
        return
    }
  }

  function wireRoom(s: Session, room: CallRoom): void {
    room.on('participantConnected', (participant) => {
      if (session !== s || !isSipParticipant(participant)) return
      // The SIP leg exists: the PSTN call is being placed. Its initial
      // attributes may already carry a status (a sub-second answer).
      if (phase.value === 'placing') phase.value = 'ringing'
      const status = participant.attributes[SIP_CALL_STATUS_ATTRIBUTE]
      if (status !== undefined) applySipStatus(s, status)
    })
    room.on('participantAttributesChanged', (changed, participant) => {
      if (session !== s || !isSipParticipant(participant)) return
      const status = changed[SIP_CALL_STATUS_ATTRIBUTE]
      if (status !== undefined) applySipStatus(s, status)
    })
    room.on('participantDisconnected', (participant) => {
      if (session !== s || !isSipParticipant(participant)) return
      // Callee hung up, declined, or rang out: the server settles which
      // (§9); the client's job is one idempotent `hangup`.
      void endCall(s, s.wasConnected ? 'ended' : 'failed')
    })
    room.on('trackSubscribed', (track, participant) => {
      if (session !== s || s.ending || !isSipParticipant(participant) || track.kind !== 'audio') return
      if (s.audioElements.has(track)) return // a duplicate subscribe never doubles the audio
      const element = track.attach()
      element.setAttribute('data-call-audio', participant.identity)
      document.body.appendChild(element)
      s.audioElements.set(track, element)
    })
    room.on('trackUnsubscribed', (track) => {
      if (session !== s) return
      const element = s.audioElements.get(track)
      if (!element) return
      track.detach()
      element.remove()
      s.audioElements.delete(track)
    })
    room.on('disconnected', () => {
      if (session !== s) return
      // Our own `disconnect()` in endCall also lands here — `ending` makes
      // it a no-op; an unexpected drop (network, server) ends the call.
      void endCall(s, s.wasConnected ? 'ended' : 'failed')
    })
  }

  async function start(targetPersonId: string, contactMethodId: string): Promise<void> {
    if (starting || active.value) return
    starting = true
    try {
      await startInner(targetPersonId, contactMethodId)
    } finally {
      starting = false
    }
  }

  /** After every await: the scope may have been disposed or `hangup()`
   * called (`s.ending`), or a newer session may have replaced this one
   * (`session !== s`). Either way this attempt must not take another step
   * — in particular a terminal phase is never resurrected to
   * `joining`/`placing`. */
  function abandoned(s: Session): boolean {
    return session !== s || s.ending
  }

  async function startInner(targetPersonId: string, contactMethodId: string): Promise<void> {
    const s: Session = {
      room: null,
      hangupSent: false,
      ending: false,
      wasConnected: false,
      connectedAt: null,
      timer: null,
      audioElements: new Map(),
    }
    session = s
    error.value = null
    muted.value = false
    elapsedSeconds.value = 0
    callId.value = ''
    personId.value = targetPersonId

    // 0. The client itself (the lazily loaded SDK chunk), before any call
    //    exists — so a load failure is `join_failed` with nothing to hang up.
    const room = options.createRoom()
    s.room = room
    wireRoom(s, room)
    try {
      await room.load()
      if (abandoned(s)) return
    } catch (cause) {
      if (abandoned(s)) return
      s.ending = true
      phase.value = 'failed'
      error.value = { code: 'join_failed', message: describeCallError(new CallClientError('join_failed', cause)), previousCallId: null }
      return
    }
    phase.value = 'requesting_mic'

    // 1. Create the call. No call exists on failure, so nothing to hang up;
    //    the §10 copy (incl. the 409 "hang up previous call" affordance)
    //    comes from the error code. The join grant is read out of the
    //    response and the mutation reset at once, so the token lives in
    //    this local only (never the MutationCache, never a ref).
    let join: { url: string; token: string }
    try {
      const response = await startMutation.mutateAsync({ personId: targetPersonId, contactMethodId })
      join = { url: response.join.url, token: response.join.token }
      startMutation.reset()
      if (session !== s) return
      callId.value = response.call.id
      if (s.ending) {
        // Hung up / disposed while the POST was in flight: the call now
        // exists, so settle it (one hangup) and go no further.
        await sendHangupOnce(s, response.call.id)
        return
      }
    } catch (failure) {
      startMutation.reset()
      if (abandoned(s)) return
      s.ending = true
      phase.value = 'failed'
      error.value = {
        code: errorCode(failure),
        message: describeCallError(failure),
        previousCallId: callInProgressId(failure),
      }
      return
    }

    // 2. Microphone first (§9 "Mic permission denied → hangup → failed{cancelled}").
    try {
      await room.acquireMicrophone()
      if (abandoned(s)) {
        // Ended while the prompt was open: make sure the track is released.
        await room.disconnect().catch(() => undefined)
        return
      }
    } catch (cause) {
      if (abandoned(s)) {
        await room.disconnect().catch(() => undefined)
        return
      }
      await endCall(s, 'failed', new CallClientError('microphone_denied', cause))
      return
    }

    // 3. Join the room before the PSTN leg is dialed (§5 "why two steps").
    phase.value = 'joining'
    try {
      await room.connect(join.url, join.token)
      if (abandoned(s)) {
        await room.disconnect().catch(() => undefined)
        return
      }
    } catch (cause) {
      if (abandoned(s)) return
      await endCall(s, 'failed', new CallClientError('join_failed', cause))
      return
    }

    // 4. Dial. From here LiveKit's events drive the phase.
    phase.value = 'placing'
    try {
      await dialMutation.mutateAsync(callId.value)
    } catch (failure) {
      if (abandoned(s)) return
      await endCall(s, 'failed', failure)
    }
  }

  async function hangup(): Promise<void> {
    const s = session
    if (!s || s.ending || !(active.value || starting)) return
    await endCall(s, s.wasConnected ? 'ended' : 'failed')
  }

  async function hangupPrevious(): Promise<void> {
    const previous = error.value?.previousCallId
    if (!previous) return
    try {
      await hangupMutation.mutateAsync(previous)
      error.value = null
      phase.value = 'idle'
    } catch (failure) {
      error.value = { code: errorCode(failure), message: describeCallError(failure), previousCallId: previous }
    }
  }

  async function toggleMute(): Promise<void> {
    const s = session
    if (!s?.room || !active.value) return
    const next = !muted.value
    await s.room.setMicrophoneMuted(next)
    muted.value = next
  }

  function dismiss(): void {
    if (active.value || starting) return
    session = null
    phase.value = 'idle'
    callId.value = ''
    personId.value = ''
    error.value = null
    muted.value = false
    elapsedSeconds.value = 0
  }

  // Leaving the page mid-call: hang up (once) and leave the room, rather
  // than leaving the PSTN leg to the server's `agent:*` participant_left
  // webhook alone (§9).
  onScopeDispose(() => {
    const s = session
    if (s && !s.ending && (active.value || starting)) {
      void endCall(s, s.wasConnected ? 'ended' : 'failed')
    }
  })

  return {
    phase,
    callId,
    personId,
    call,
    error,
    muted,
    elapsedSeconds,
    active,
    start,
    hangup,
    hangupPrevious,
    toggleMute,
    dismiss,
  }
}
