// The real LiveKit adapter against a mocked `livekit-client` module: the
// SDK is loaded lazily, the microphone is acquired before connect and
// published after, and a track that resolves after `disconnect()` is
// stopped at once (no hot mic). No WebRTC, no network.
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const state = vi.hoisted(() => ({
  rooms: [] as FakeSdkRoom[],
  tracks: [] as FakeTrack[],
  micGate: null as Promise<void> | null,
}))

class FakeTrack {
  stopped = 0
  muted: boolean[] = []
  stop() {
    this.stopped += 1
  }
  async mute() {
    this.muted.push(true)
    return this
  }
  async unmute() {
    this.muted.push(false)
    return this
  }
}

class FakeSdkRoom {
  handlers = new Map<string, Array<(...args: unknown[]) => void>>()
  connected: Array<[string, string]> = []
  disconnects = 0
  published: FakeTrack[] = []
  localParticipant = {
    publishTrack: async (track: FakeTrack) => {
      this.published.push(track)
    },
  }
  constructor() {
    state.rooms.push(this)
  }
  on(event: string, handler: (...args: unknown[]) => void) {
    const list = this.handlers.get(event) ?? []
    list.push(handler)
    this.handlers.set(event, list)
    return this
  }
  emit(event: string, ...args: unknown[]) {
    for (const h of this.handlers.get(event) ?? []) h(...args)
  }
  async connect(url: string, token: string) {
    this.connected.push([url, token])
  }
  async disconnect() {
    this.disconnects += 1
  }
}

vi.mock('livekit-client', () => ({
  Room: FakeSdkRoom,
  RoomEvent: {
    ParticipantConnected: 'participantConnected',
    ParticipantDisconnected: 'participantDisconnected',
    ParticipantAttributesChanged: 'participantAttributesChanged',
    TrackSubscribed: 'trackSubscribed',
    TrackUnsubscribed: 'trackUnsubscribed',
    Disconnected: 'disconnected',
  },
  createLocalAudioTrack: async () => {
    if (state.micGate) await state.micGate
    const track = new FakeTrack()
    state.tracks.push(track)
    return track
  },
}))

import { createLiveKitRoom } from './client'

beforeEach(() => {
  state.rooms.length = 0
  state.tracks.length = 0
  state.micGate = null
})

afterEach(() => {
  vi.clearAllMocks()
})

describe('createLiveKitRoom', () => {
  it('creates the SDK room only on load, and forwards events to handlers registered before it', async () => {
    const room = createLiveKitRoom()
    const connected: string[] = []
    room.on('participantConnected', (p) => connected.push(p.identity))
    expect(state.rooms).toHaveLength(0)
    await room.load()
    expect(state.rooms).toHaveLength(1)
    state.rooms[0].emit('participantConnected', { identity: 'sip:x', attributes: {} })
    expect(connected).toEqual(['sip:x'])
  })

  it('forwards the other events, dropping the SDK publication argument', async () => {
    const room = createLiveKitRoom()
    const seen: string[] = []
    room.on('participantDisconnected', (p) => seen.push(`left:${p.identity}`))
    room.on('participantAttributesChanged', (changed, p) => seen.push(`attr:${p.identity}:${changed['sip.callStatus']}`))
    room.on('trackSubscribed', (track, p) => seen.push(`sub:${p.identity}:${track.kind}`))
    room.on('trackUnsubscribed', (track, p) => seen.push(`unsub:${p.identity}:${track.kind}`))
    room.on('disconnected', () => seen.push('disconnected'))
    await room.load()
    const sdk = state.rooms[0]
    const sip = { identity: 'sip:x', attributes: {} }
    sdk.emit('participantAttributesChanged', { 'sip.callStatus': 'active' }, sip)
    sdk.emit('trackSubscribed', { kind: 'audio' }, { sid: 'pub' }, sip)
    sdk.emit('trackUnsubscribed', { kind: 'audio' }, { sid: 'pub' }, sip)
    sdk.emit('participantDisconnected', sip)
    sdk.emit('disconnected', 1)
    expect(seen).toEqual(['attr:sip:x:active', 'sub:sip:x:audio', 'unsub:sip:x:audio', 'left:sip:x', 'disconnected'])
  })

  it('acquires the microphone, publishes it after connect, mutes, and stops it on disconnect', async () => {
    const room = createLiveKitRoom()
    await room.acquireMicrophone()
    expect(state.tracks).toHaveLength(1)
    expect(state.rooms[0].published).toHaveLength(0)
    await room.connect('wss://lk', 'tok')
    expect(state.rooms[0].connected).toEqual([['wss://lk', 'tok']])
    expect(state.rooms[0].published).toEqual([state.tracks[0]])
    await room.setMicrophoneMuted(true)
    await room.setMicrophoneMuted(false)
    expect(state.tracks[0].muted).toEqual([true, false])
    await room.disconnect()
    expect(state.tracks[0].stopped).toBe(1)
    expect(state.rooms[0].disconnects).toBe(1)
    await room.setMicrophoneMuted(true) // released: no-op
    expect(state.tracks[0].muted).toEqual([true, false])
  })

  it('a microphone that resolves after disconnect is stopped immediately (no hot mic)', async () => {
    let release: () => void = () => {}
    state.micGate = new Promise<void>((resolve) => {
      release = resolve
    })
    const room = createLiveKitRoom()
    const acquiring = room.acquireMicrophone()
    await Promise.resolve()
    await room.disconnect()
    release()
    await acquiring
    expect(state.tracks).toHaveLength(1)
    expect(state.tracks[0].stopped).toBe(1)
    // Nothing is published even if connect is still attempted afterwards.
    await room.connect('wss://lk', 'tok')
    expect(state.rooms[0].published).toHaveLength(0)
    expect(state.rooms[0].disconnects).toBe(2)
  })

  it('disconnect before load is a no-op that still marks the room disposed', async () => {
    const room = createLiveKitRoom()
    await room.disconnect()
    expect(state.rooms).toHaveLength(0)
    await room.acquireMicrophone()
    expect(state.tracks[0].stopped).toBe(1)
  })
})
