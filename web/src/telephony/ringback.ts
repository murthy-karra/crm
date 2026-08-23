// SLICE_006c §12's "ringback tone" rider: a local ringback (North American
// 440 Hz + 480 Hz, 2 s on / 4 s off) played through WebAudio while the call
// is `ringing`, so the caller hears something between dial and answer — the
// SIP leg carries no early media. No asset file: two oscillators and a gain
// node whose on/off cadence is scheduled up front (no timers), long enough
// to outlast any ring timeout. Stopped on any phase change (the microphone
// mute does not silence it — that is the caller's own mic).
// The `AudioContext` is injected (tests pass a fake; a browser without one
// gets a no-op), the same pattern as the LiveKit room in useCall.ts.

/** The structural slice of `AudioContext` this module needs. */
export interface RingbackAudioContext {
  readonly currentTime: number
  readonly state?: AudioContextState
  resume?(): Promise<void>
  readonly destination: AudioNode
  createOscillator(): OscillatorNode
  createGain(): GainNode
  close(): Promise<void>
}

export type RingbackContextFactory = () => RingbackAudioContext | null

export interface Ringback {
  start(): void
  stop(): void
  readonly playing: boolean
}

const FREQUENCIES_HZ = [440, 480] as const
const VOLUME = 0.08
const ON_SECONDS = 2
const PERIOD_SECONDS = 6
// Slice 006 §9's ring timeout is well under two minutes; a ringback that
// outlasts it is simply stopped by the phase change.
const CYCLES = 20

/** The browser default, guarded for environments without WebAudio. */
export function defaultRingbackContext(): RingbackAudioContext | null {
  if (typeof AudioContext === 'undefined') return null
  return new AudioContext()
}

export function createRingback(createContext: RingbackContextFactory): Ringback {
  let context: RingbackAudioContext | null = null

  return {
    get playing() {
      return context !== null
    },
    start() {
      if (context !== null) return
      let created: RingbackAudioContext | null
      try {
        created = createContext()
      } catch {
        created = null
      }
      if (created === null) return
      context = created
      // Autoplay policy: a context created outside a user gesture starts
      // suspended; resuming is best-effort (the call is unaffected).
      if (created.state === 'suspended' && created.resume) {
        void created.resume().catch(() => undefined)
      }
      try {
        const gain = created.createGain()
        gain.gain.setValueAtTime(0, created.currentTime)
        const t0 = created.currentTime
        for (let cycle = 0; cycle < CYCLES; cycle += 1) {
          const on = t0 + cycle * PERIOD_SECONDS
          gain.gain.setValueAtTime(VOLUME, on)
          gain.gain.setValueAtTime(0, on + ON_SECONDS)
        }
        gain.connect(created.destination)
        for (const frequency of FREQUENCIES_HZ) {
          const oscillator = created.createOscillator()
          oscillator.type = 'sine'
          oscillator.frequency.setValueAtTime(frequency, t0)
          oscillator.connect(gain)
          oscillator.start(t0)
          oscillator.stop(t0 + CYCLES * PERIOD_SECONDS)
        }
      } catch {
        // A half-wired graph must not leak: release the context and stay silent.
        context = null
        void created.close().catch(() => undefined)
      }
    },
    stop() {
      const current = context
      if (current === null) return
      context = null
      // Closing the context releases the oscillators and the output.
      void current.close().catch(() => undefined)
    },
  }
}
