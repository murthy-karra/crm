// The production `CallRoomFactory` (SLICE_006 §10): the one module that
// touches `livekit-client`. useCall.ts never does — it takes the factory as
// an option, so its tests inject a fake and stay free of the SDK, WebRTC,
// and the network (the realtime/client.ts pattern).
//
// The SDK is loaded with a dynamic `import()` on the first call attempt,
// not at module load: it is ~500 kB minified and most Person page views
// never place a call, so the Person route's chunk stays the size it was.
// Handlers registered before that point are buffered and wired once the
// `Room` exists — `on` stays synchronous for the composable.
//
// Microphone handling is split in two on purpose: `acquireMicrophone`
// creates the local audio track (the browser's permission prompt) before
// `connect` joins the room and publishes it — the callee must never answer
// to silence (§5 "why two steps"), and a denial must be a clean
// `failed{cancelled}` before anything is dialed (§9).
import type { LocalAudioTrack, Room } from 'livekit-client'
import type { CallRoom, CallRoomEvents, CallRoomFactory } from './useCall'

type LiveKitSdk = typeof import('livekit-client')

export const createLiveKitRoom: CallRoomFactory = (): CallRoom => {
  const handlers: { [E in keyof CallRoomEvents]: CallRoomEvents[E][] } = {
    participantConnected: [],
    participantDisconnected: [],
    participantAttributesChanged: [],
    trackSubscribed: [],
    trackUnsubscribed: [],
    disconnected: [],
  }
  let microphone: LocalAudioTrack | null = null
  let loading: Promise<{ sdk: LiveKitSdk; room: Room }> | null = null
  // Set by `disconnect()`. A microphone track that resolves after that point
  // (the permission prompt answered after the call was abandoned) is stopped
  // immediately rather than left open — never a hot mic.
  let disposed = false

  // The SDK's `RemoteParticipant` (identity/attributes) and `RemoteTrack`
  // (kind/attach/detach) satisfy the `CallParticipant` / `CallRemoteTrack`
  // surfaces structurally, so nothing is adapted — only the SDK's extra
  // `publication` argument is dropped.
  function wire(sdk: LiveKitSdk, room: Room): void {
    room.on(sdk.RoomEvent.ParticipantConnected, (participant) => {
      for (const h of handlers.participantConnected) h(participant)
    })
    room.on(sdk.RoomEvent.ParticipantDisconnected, (participant) => {
      for (const h of handlers.participantDisconnected) h(participant)
    })
    room.on(sdk.RoomEvent.ParticipantAttributesChanged, (changed, participant) => {
      for (const h of handlers.participantAttributesChanged) h(changed, participant)
    })
    room.on(sdk.RoomEvent.TrackSubscribed, (track, _publication, participant) => {
      for (const h of handlers.trackSubscribed) h(track, participant)
    })
    room.on(sdk.RoomEvent.TrackUnsubscribed, (track, _publication, participant) => {
      for (const h of handlers.trackUnsubscribed) h(track, participant)
    })
    room.on(sdk.RoomEvent.Disconnected, () => {
      for (const h of handlers.disconnected) h()
    })
  }

  function load(): Promise<{ sdk: LiveKitSdk; room: Room }> {
    if (!loading) {
      loading = import('livekit-client').then((sdk) => {
        const room = new sdk.Room()
        wire(sdk, room)
        return { sdk, room }
      })
    }
    return loading
  }

  function releaseMicrophone(): void {
    const track = microphone
    microphone = null
    track?.stop()
  }

  return {
    on(event, handler) {
      handlers[event].push(handler)
    },
    async load() {
      await load()
    },
    async acquireMicrophone() {
      const { sdk } = await load()
      const track = await sdk.createLocalAudioTrack()
      if (disposed) {
        track.stop()
        return
      }
      microphone = track
    },
    async connect(url, token) {
      const { room } = await load()
      await room.connect(url, token)
      if (disposed) {
        releaseMicrophone()
        await room.disconnect()
        return
      }
      if (microphone) {
        await room.localParticipant.publishTrack(microphone)
      }
    },
    async setMicrophoneMuted(muted) {
      if (!microphone) return
      if (muted) await microphone.mute()
      else await microphone.unmute()
    },
    async disconnect() {
      disposed = true
      releaseMicrophone()
      if (!loading) return
      const { room } = await loading.catch(() => ({ room: null }))
      if (room) await room.disconnect()
    },
  }
}
