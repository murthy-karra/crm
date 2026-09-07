import { afterEach, describe, expect, it, vi } from 'vitest'

const originalStorage = Object.getOwnPropertyDescriptor(window, 'localStorage')
const originalBroadcastChannel = Object.getOwnPropertyDescriptor(window, 'BroadcastChannel')

type StorageOptions = { getThrows?: boolean, setThrows?: boolean }

function installStorage(options: StorageOptions = {}) {
  const values = new Map<string, string>()
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: {
      get length() {
        if (options.getThrows) throw new Error('storage disabled')
        return values.size
      },
      key: (index: number) => {
        if (options.getThrows) throw new Error('storage disabled')
        return [...values.keys()][index] ?? null
      },
      getItem: (key: string) => {
        if (options.getThrows) throw new Error('storage disabled')
        return values.get(key) ?? null
      },
      setItem: (key: string, value: string) => {
        if (options.setThrows) throw new Error('storage disabled')
        values.set(key, value)
      },
      removeItem: (key: string) => {
        if (options.setThrows) throw new Error('storage disabled')
        values.delete(key)
      },
      clear: () => values.clear(),
    },
  })
  return values
}

afterEach(() => {
  if (originalStorage) Object.defineProperty(window, 'localStorage', originalStorage)
  else delete (window as { localStorage?: unknown }).localStorage
  if (originalBroadcastChannel) Object.defineProperty(window, 'BroadcastChannel', originalBroadcastChannel)
  else delete (window as { BroadcastChannel?: unknown }).BroadcastChannel
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.useRealTimers()
  vi.resetModules()
})

async function freshLifecycle() {
  vi.resetModules()
  return import('./sessionLifecycle')
}

describe('shared session lifecycle', () => {
  it('receives an identity-free changing marker before coordinator configuration and keeps the tab blocked through settlement', async () => {
    const marker = { epoch: 'opaque-remote-epoch', sequence: 1234, phase: 'changing' }
    const storage = installStorage()
    storage.set('crm.session-lifecycle.v1', JSON.stringify(marker))
    const lifecycle = await freshLifecycle()
    let discarded = 0
    let verified = 0

    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => { discarded += 1 },
      verify: () => { verified += 1 },
      installVerifiedIdentity: () => undefined,
    })

    expect(lifecycle.useAuthSessionLifetime().value).toBe(1)
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)
    expect(discarded).toBe(1)
    expect(() => lifecycle.requireVerifiedSession()).toThrow(lifecycle.SessionVerificationPendingError)

    lifecycle.settleSessionTransition(marker.epoch)
    expect(verified).toBe(1)
    const persisted = storage.get(lifecycle.SESSION_LIFECYCLE_STORAGE_KEY) ?? ''
    expect(persisted).not.toContain('alice')
    expect(persisted).not.toContain('organization')
    expect(persisted).not.toContain('private')
  })

  it('fails closed when durable writes are unavailable even if reads still return an old marker', async () => {
    class HeldBroadcastChannel {
      addEventListener() {}
      postMessage() {}
    }
    Object.defineProperty(window, 'BroadcastChannel', { configurable: true, value: HeldBroadcastChannel })
    installStorage({ setThrows: true })
    const lifecycle = await freshLifecycle()
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => undefined,
      installVerifiedIdentity: () => undefined,
    })

    expect(lifecycle.isSessionVerified()).toBe(false)
    expect(lifecycle.useSessionCoordinationUnavailable().value).toBe(true)
    expect(() => lifecycle.requireVerifiedSession()).toThrow(lifecycle.SessionCoordinationUnavailableError)
  })

  it('discards state on an opaque storage event before verification returns an identity', async () => {
    const storage = installStorage()
    const lifecycle = await freshLifecycle()
    let discarded = 0
    let verificationGeneration: number | undefined
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => { discarded += 1 },
      verify: (generation) => { verificationGeneration = generation },
      installVerifiedIdentity: () => undefined,
    })

    const remote = { epoch: 'opaque-other-tab', sequence: 55, phase: 'changing' }
    storage.set(lifecycle.SESSION_LIFECYCLE_STORAGE_KEY, JSON.stringify(remote))
    window.dispatchEvent(new StorageEvent('storage', {
      key: lifecycle.SESSION_LIFECYCLE_STORAGE_KEY,
      newValue: JSON.stringify(remote),
    }))

    expect(discarded).toBe(1)
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)
    expect(() => lifecycle.requireVerifiedSession()).toThrow(lifecycle.SessionVerificationPendingError)

    lifecycle.settleSessionTransition(remote.epoch)
    expect(verificationGeneration).toBe(lifecycle.currentSessionGeneration())
  })

  it('keeps every tab blocked until each opaque auth-attempt record has settled', async () => {
    const storage = installStorage()
    const lifecycle = await freshLifecycle()
    const installed: unknown[] = []
    let verificationGeneration: number | undefined
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: (generation) => { verificationGeneration = generation },
      installVerifiedIdentity: (identity) => { installed.push(identity) },
    })

    const earlier = lifecycle.beginSessionTransition()
    const later = 'opaque-newer'
    storage.set(`crm.session-lifecycle.v1.pending.${later}`, later)
    const laterMarker = { epoch: later, sequence: Date.now() + 10, phase: 'changing' }
    storage.set(lifecycle.SESSION_LIFECYCLE_STORAGE_KEY, JSON.stringify(laterMarker))
    window.dispatchEvent(new StorageEvent('storage', {
      key: lifecycle.SESSION_LIFECYCLE_STORAGE_KEY,
      newValue: JSON.stringify(laterMarker),
    }))

    lifecycle.settleSessionTransition(later, { actor: 'Carol' })
    expect(installed).toEqual([])
    expect(() => lifecycle.requireVerifiedSession()).toThrow(lifecycle.SessionVerificationPendingError)

    lifecycle.settleSessionTransition(earlier, { actor: 'Alice' })
    lifecycle.completeSessionVerification(verificationGeneration!, { actor: 'Alice' })
    expect(installed).toEqual([{ actor: 'Alice' }])
  })

  it('converges two independent coordinators after overlapping auth responses and clears both private lifetimes', async () => {
    installStorage()
    const a = await freshLifecycle()
    const b = await freshLifecycle()
    const installedA: unknown[] = []
    const installedB: unknown[] = []
    let discardedA = 0
    let discardedB = 0
    let verifyA: number | undefined
    let verifyB: number | undefined
    a.configureSessionLifecycle({
      discardPrivateState: () => { discardedA += 1 },
      verify: (generation) => { verifyA = generation },
      installVerifiedIdentity: (identity) => { installedA.push(identity) },
    })
    b.configureSessionLifecycle({
      discardPrivateState: () => { discardedB += 1 },
      verify: (generation) => { verifyB = generation },
      installVerifiedIdentity: (identity) => { installedB.push(identity) },
    })

    const attemptA = a.beginSessionTransition()
    expect(b.isSessionVerified()).toBe(false) // synchronously observes A's pending record
    const attemptB = b.beginSessionTransition()
    expect(a.isSessionVerified()).toBe(false)

    // B's response arrives first but cannot release either coordinator while
    // A still owns an opaque pending record.
    b.settleSessionTransition(attemptB, { actor: 'Carol' })
    expect(installedA).toEqual([])
    expect(installedB).toEqual([])

    // A's later response becomes the only current cookie/identity. B reads
    // the final durable marker and verifies independently rather than
    // retaining Carol's prior private lifetime.
    a.settleSessionTransition(attemptA, { actor: 'Alice' })
    a.completeSessionVerification(verifyA!, { actor: 'Alice' })
    expect(installedA).toEqual([{ actor: 'Alice' }])
    expect(b.isSessionVerified()).toBe(false)
    b.completeSessionVerification(verifyB!, { actor: 'Alice' })
    expect(installedB).toEqual([{ actor: 'Alice' }])
    expect(discardedA).toBeGreaterThan(0)
    expect(discardedB).toBeGreaterThan(0)

    // A byte-identical A→A replacement still advances B's lifetime and
    // invokes its private-state clear handler.
    const lifetimeBefore = b.useAuthSessionLifetime().value
    const sameActor = a.beginSessionTransition()
    expect(b.isSessionVerified()).toBe(false)
    a.settleSessionTransition(sameActor, { actor: 'Alice' })
    expect(b.useAuthSessionLifetime().value).toBeGreaterThan(lifetimeBefore)
    expect(discardedB).toBeGreaterThan(1)
  })

  it('never releases a changing transition solely because time passed', async () => {
    vi.useFakeTimers()
    installStorage()
    const lifecycle = await freshLifecycle()
    let verified = 0
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => { verified += 1 },
      installVerifiedIdentity: () => undefined,
    })

    lifecycle.beginSessionTransition()
    await vi.advanceTimersByTimeAsync(2_000)
    expect(verified).toBe(0)
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)
  })

  it('completeSessionVerification returns false and installs nothing once a newer marker has superseded the generation it targets', async () => {
    installStorage()
    const lifecycle = await freshLifecycle()
    const installed: unknown[] = []
    const verified: number[] = []
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: (generation) => { verified.push(generation) },
      installVerifiedIdentity: (identity) => { installed.push(identity) },
    })

    const first = lifecycle.beginSessionTransition()
    lifecycle.settleSessionTransition(first, { actor: 'Alice' })
    expect(verified).toHaveLength(1)
    const staleGeneration = verified[0]!

    // A second boundary supersedes the first before its (stale) callback
    // would resolve — e.g. a slow `/me` response racing a newer login.
    const second = lifecycle.beginSessionTransition()
    lifecycle.settleSessionTransition(second, { actor: 'Bob' })
    expect(verified).toHaveLength(2)
    expect(verified[1]).toBeGreaterThan(staleGeneration)

    expect(lifecycle.completeSessionVerification(staleGeneration, { actor: 'Alice' })).toBe(false)
    expect(installed).toEqual([])

    // The current generation still verifies normally.
    expect(lifecycle.completeSessionVerification(lifecycle.currentSessionGeneration(), { actor: 'Bob' })).toBe(true)
    expect(installed).toEqual([{ actor: 'Bob' }])
  })
})
