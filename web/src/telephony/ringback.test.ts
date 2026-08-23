// SLICE_006c §12's ringback rider: start/stop against a fake AudioContext —
// oscillators at 440/480 Hz through a gain node with the on/off cadence
// scheduled, closed on stop, idempotent, and a no-op without WebAudio.
import { describe, expect, it, vi } from 'vitest'
import { createRingback, type RingbackAudioContext } from './ringback'

function fakeParam() {
  return { setValueAtTime: vi.fn() }
}

function fakeContext() {
  const oscillators: Array<{ frequency: { setValueAtTime: ReturnType<typeof vi.fn> }; start: ReturnType<typeof vi.fn>; stop: ReturnType<typeof vi.fn>; connect: ReturnType<typeof vi.fn>; type: string }> = []
  const gains: Array<{ gain: { setValueAtTime: ReturnType<typeof vi.fn> }; connect: ReturnType<typeof vi.fn> }> = []
  const destination = {} as AudioNode
  const close = vi.fn(async () => undefined)
  const context = {
    currentTime: 10,
    destination,
    createOscillator: () => {
      const oscillator = { frequency: fakeParam(), start: vi.fn(), stop: vi.fn(), connect: vi.fn(), type: 'sine' }
      oscillators.push(oscillator)
      return oscillator as unknown as OscillatorNode
    },
    createGain: () => {
      const gain = { gain: fakeParam(), connect: vi.fn() }
      gains.push(gain)
      return gain as unknown as GainNode
    },
    close,
  } satisfies RingbackAudioContext
  return { context, oscillators, gains, destination, close }
}

describe('createRingback', () => {
  it('start wires two oscillators (440/480 Hz) through a gated gain into the destination', () => {
    const fake = fakeContext()
    const ringback = createRingback(() => fake.context)
    expect(ringback.playing).toBe(false)
    ringback.start()
    expect(ringback.playing).toBe(true)
    expect(fake.gains).toHaveLength(1)
    expect(fake.gains[0].connect).toHaveBeenCalledWith(fake.destination)
    expect(fake.oscillators.map((o) => o.frequency.setValueAtTime.mock.calls[0][0])).toEqual([440, 480])
    for (const oscillator of fake.oscillators) {
      expect(oscillator.connect).toHaveBeenCalledWith(fake.gains[0])
      expect(oscillator.start).toHaveBeenCalledWith(10)
      expect(oscillator.stop).toHaveBeenCalledTimes(1)
    }
    // 2 s on / 4 s off from `currentTime`, scheduled up front.
    const schedule = fake.gains[0].gain.setValueAtTime.mock.calls.slice(0, 5)
    expect(schedule).toEqual([
      [0, 10],
      [0.08, 10],
      [0, 12],
      [0.08, 16],
      [0, 18],
    ])
  })

  it('start is idempotent and stop closes the context once', () => {
    const fake = fakeContext()
    const factory = vi.fn(() => fake.context)
    const ringback = createRingback(factory)
    ringback.start()
    ringback.start()
    expect(factory).toHaveBeenCalledTimes(1)
    ringback.stop()
    ringback.stop()
    expect(fake.close).toHaveBeenCalledTimes(1)
    expect(ringback.playing).toBe(false)
    // A later start opens a fresh context.
    ringback.start()
    expect(factory).toHaveBeenCalledTimes(2)
  })

  it('is a no-op when there is no AudioContext (or creating one throws)', () => {
    const silent = createRingback(() => null)
    silent.start()
    expect(silent.playing).toBe(false)
    silent.stop()
    const throwing = createRingback(() => {
      throw new Error('not allowed')
    })
    throwing.start()
    expect(throwing.playing).toBe(false)
  })
})

describe('createRingback hardening', () => {
  it('resumes a suspended context (best-effort) and closes the context if wiring throws', () => {
    const resume = vi.fn(async () => undefined)
    const close = vi.fn(async () => undefined)
    const suspended = {
      currentTime: 0,
      state: 'suspended' as const,
      resume,
      destination: {} as AudioNode,
      createOscillator: () => ({ frequency: fakeParam(), start: vi.fn(), stop: vi.fn(), connect: vi.fn(), type: 'sine' }) as unknown as OscillatorNode,
      createGain: () => ({ gain: fakeParam(), connect: vi.fn() }) as unknown as GainNode,
      close,
    } satisfies RingbackAudioContext
    const ringback = createRingback(() => suspended)
    ringback.start()
    expect(resume).toHaveBeenCalledTimes(1)
    expect(ringback.playing).toBe(true)

    const broken = {
      ...suspended,
      state: 'running' as const,
      createGain: () => {
        throw new Error('no audio')
      },
      close: vi.fn(async () => undefined),
    } satisfies RingbackAudioContext
    const failing = createRingback(() => broken)
    failing.start()
    expect(failing.playing).toBe(false)
    expect(broken.close).toHaveBeenCalledTimes(1)
  })
})
