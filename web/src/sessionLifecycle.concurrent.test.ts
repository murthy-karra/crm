import { afterEach, describe, expect, it, vi } from 'vitest'

const originalStorage = Object.getOwnPropertyDescriptor(window, 'localStorage')
const originalBroadcastChannel = Object.getOwnPropertyDescriptor(window, 'BroadcastChannel')

type Lifecycle = typeof import('./sessionLifecycle')

type SharedStorage = {
  values: Map<string, string>
  onSet: ((key: string, value: string) => void) | undefined
}

function installSharedStorage(): SharedStorage {
  const shared: SharedStorage = { values: new Map(), onSet: undefined }
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: {
      get length() { return shared.values.size },
      key: (index: number) => [...shared.values.keys()][index] ?? null,
      getItem: (key: string) => shared.values.get(key) ?? null,
      setItem: (key: string, value: string) => {
        shared.values.set(key, value)
        shared.onSet?.(key, value)
      },
      removeItem: (key: string) => { shared.values.delete(key) },
      clear: () => { shared.values.clear() },
    },
  })
  // These tests model durable storage polling directly. Avoid a same-window
  // BroadcastChannel delivering a synthetic self-event that browsers do not.
  Object.defineProperty(window, 'BroadcastChannel', { configurable: true, value: undefined })
  return shared
}

async function independentCoordinator(): Promise<Lifecycle> {
  vi.resetModules()
  return import('./sessionLifecycle')
}

function latest(values: number[], label: string): number {
  const value = values.at(-1)
  if (value === undefined) throw new Error(`missing authoritative verification for ${label}`)
  return value
}

function jsonResponse(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { 'Content-Type': 'application/json' } })
}

afterEach(() => {
  if (originalStorage) Object.defineProperty(window, 'localStorage', originalStorage)
  else delete (window as { localStorage?: unknown }).localStorage
  if (originalBroadcastChannel) Object.defineProperty(window, 'BroadcastChannel', originalBroadcastChannel)
  else delete (window as { BroadcastChannel?: unknown }).BroadcastChannel
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.resetModules()
})

describe('independent session lifecycle coordinators', () => {
  it('converges overlapping cookie replacements only through authoritative verification and clears byte-identical A-to-A replacements', async () => {
    installSharedStorage()
    const a = await independentCoordinator()
    const b = await independentCoordinator()
    const installedA: unknown[] = []
    const installedB: unknown[] = []
    const verifyA: number[] = []
    const verifyB: number[] = []
    let discardedA = 0
    let discardedB = 0
    const alice = { actor: 'Alice' }
    const carol = { actor: 'Carol' }
    let sharedCookieIdentity: unknown

    a.configureSessionLifecycle({
      discardPrivateState: () => { discardedA += 1 },
      verify: (generation) => { verifyA.push(generation) },
      installVerifiedIdentity: (identity) => { installedA.push(identity) },
    })
    b.configureSessionLifecycle({
      discardPrivateState: () => { discardedB += 1 },
      verify: (generation) => { verifyB.push(generation) },
      installVerifiedIdentity: (identity) => { installedB.push(identity) },
    })

    // Two independent tabs begin overlapping authentication attempts. Their
    // only shared state is the opaque durable marker/pending-record set.
    const attemptA = a.beginSessionTransition()
    expect(b.isSessionVerified()).toBe(false)
    const attemptB = b.beginSessionTransition()
    expect(a.isSessionVerified()).toBe(false)

    // B's HTTP response replaces the browser cookie with Carol, but its JS
    // callback is delayed. No lifecycle callback has been allowed to seed a
    // response identity into either coordinator.
    sharedCookieIdentity = carol
    expect(sharedCookieIdentity).toBe(carol)
    expect(installedA).toEqual([])
    expect(installedB).toEqual([])

    // A's response wins the shared cookie and settles first. B remains an
    // opaque outstanding attempt, so A cannot verify or install Alice yet.
    sharedCookieIdentity = alice
    a.settleSessionTransition(attemptA, alice)
    expect(installedA).toEqual([])
    expect(installedB).toEqual([])
    expect(a.isSessionVerified()).toBe(false)
    expect(b.isSessionVerified()).toBe(false)

    // B's stale callback now runs. Passing Carol here must never seed Carol;
    // settlement only creates a fresh opaque boundary and starts `/me`.
    b.settleSessionTransition(attemptB, carol)
    expect(installedA).toEqual([])
    expect(installedB).toEqual([])
    expect(b.useSessionVerificationPending().value).toBe(true)
    expect(b.completeSessionVerification(latest(verifyB, 'B'), sharedCookieIdentity)).toBe(true)
    expect(installedB).toEqual([alice])

    // A separately observes the final marker and obtains the same identity
    // through its own authoritative verification. It cannot infer Carol from
    // B's callback or reuse a prior private lifetime.
    expect(a.isSessionVerified()).toBe(false)
    expect(a.completeSessionVerification(latest(verifyA, 'A'), sharedCookieIdentity)).toBe(true)
    expect(installedA).toEqual([alice])
    expect(installedA).not.toContainEqual(carol)
    expect(installedB).not.toContainEqual(carol)
    expect(discardedA).toBeGreaterThan(0)
    expect(discardedB).toBeGreaterThan(0)

    // Even when the newly verified identity is byte-identical Alice, an
    // A-to-A cookie replacement remains a new session and clears both tabs'
    // private state before either one verifies again.
    const lifetimeA = a.useAuthSessionLifetime().value
    const lifetimeB = b.useAuthSessionLifetime().value
    const clearedA = discardedA
    const clearedB = discardedB
    const sameIdentityAttempt = a.beginSessionTransition()
    expect(b.isSessionVerified()).toBe(false)
    a.settleSessionTransition(sameIdentityAttempt, alice)
    expect(a.completeSessionVerification(latest(verifyA, 'A to A'), alice)).toBe(true)
    expect(b.isSessionVerified()).toBe(false)
    expect(b.completeSessionVerification(latest(verifyB, 'B A to A'), alice)).toBe(true)

    expect(a.useAuthSessionLifetime().value).toBeGreaterThan(lifetimeA)
    expect(b.useAuthSessionLifetime().value).toBeGreaterThan(lifetimeB)
    expect(discardedA).toBeGreaterThan(clearedA)
    expect(discardedB).toBeGreaterThan(clearedB)
    expect(installedA.at(-1)).toEqual(alice)
    expect(installedB.at(-1)).toEqual(alice)
  })

  it('keeps independent durable probe writes from causing a false coordination failure', async () => {
    const storage = installSharedStorage()
    const a = await independentCoordinator()
    const b = await independentCoordinator()
    a.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => undefined,
      installVerifiedIdentity: () => undefined,
    })
    b.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => undefined,
      installVerifiedIdentity: () => undefined,
    })

    let interleaved = false
    storage.onSet = (key) => {
      // Model A writing its unique durability probe, then B writing/removing
      // its own probe before A verifies its write. A coordinator must never
      // use B's probe key as its own proof of durable storage.
      if (!interleaved && key.startsWith(`${a.SESSION_LIFECYCLE_STORAGE_KEY}.probe`)) {
        interleaved = true
        b.synchronizeSessionLifecycle()
      }
    }

    expect(a.isSessionVerified()).toBe(true)
    expect(interleaved).toBe(true)
    expect(b.isSessionVerified()).toBe(true)
    expect(a.useSessionCoordinationUnavailable().value).toBe(false)
    expect(b.useSessionCoordinationUnavailable().value).toBe(false)
  })

  it('lets public authentication routes dispatch while an orphaned private-session transition remains pending', async () => {
    installSharedStorage()
    vi.resetModules()
    const lifecycle = await import('./sessionLifecycle')
    const { apiFetch } = await import('./api/client')
    const fetchMock = vi.fn(async () => jsonResponse({ ok: true }))
    vi.stubGlobal('fetch', fetchMock)

    lifecycle.beginSessionTransition() // intentionally never settled
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)

    await expect(Promise.all([
      apiFetch('/me'),
      apiFetch('/session'),
      apiFetch('/invitations/preview'),
      apiFetch('/invitations/accept', { method: 'POST', body: '{}' }),
    ])).resolves.toEqual([
      { ok: true },
      { ok: true },
      { ok: true },
      { ok: true },
    ])
    expect(fetchMock).toHaveBeenCalledTimes(4)
  })

  it('F1: resetSessionCoordination recovers this tab\'s own never-settled attempt and lets a private request dispatch', async () => {
    installSharedStorage()
    vi.resetModules()
    const lifecycle = await import('./sessionLifecycle')
    const { apiFetch } = await import('./api/client')
    // Registers the real `verify`/`installVerifiedIdentity` handlers
    // (api/queries.ts's module-load side effect) — neither `sessionLifecycle`
    // nor `api/client` alone wires them.
    await import('./api/queries')
    const fetchMock = vi.fn(async (input: RequestInfo | URL) =>
      String(input).endsWith('/me')
        ? jsonResponse({ user: { id: 'u1', email: 'alice@example.test', display_name: 'Alice' }, organization: null, platform_admin: true })
        : jsonResponse({ ok: true }),
    )
    vi.stubGlobal('fetch', fetchMock)

    lifecycle.beginSessionTransition() // intentionally never settled, like the tab above
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)
    await expect(apiFetch('/today')).rejects.toBeInstanceOf(lifecycle.SessionVerificationPendingError)

    lifecycle.resetSessionCoordination()

    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled())
    await vi.waitFor(() => expect(lifecycle.isSessionVerified()).toBe(true))
    await expect(apiFetch('/today')).resolves.toEqual({ ok: true })
  })
})
