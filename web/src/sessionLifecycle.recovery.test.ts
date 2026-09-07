import { mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory } from 'vue-router'
import { defineComponent } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const STORAGE_KEY = 'crm.session-lifecycle.v1'
const PENDING_PREFIX = `${STORAGE_KEY}.pending.`
const originalStorage = Object.getOwnPropertyDescriptor(window, 'localStorage')
const originalBroadcastChannel = Object.getOwnPropertyDescriptor(window, 'BroadcastChannel')
const cleanups: Array<() => void> = []
const lifecycleListeners: Array<{ type: string; listener: EventListenerOrEventListenerObject; options?: boolean | AddEventListenerOptions }> = []

beforeEach(() => {
  const addEventListener = window.addEventListener.bind(window)
  vi.spyOn(window, 'addEventListener').mockImplementation((type, listener, options) => {
    addEventListener(type, listener, options)
    if (['storage', 'focus', 'visibilitychange', 'pageshow'].includes(type)) {
      lifecycleListeners.push({ type, listener, options })
    }
  })
})

type StorageController = {
  values: Map<string, string>
  setFailure: (next: ((key: string, value: string) => boolean) | undefined) => void
  getFailure: (next: ((key: string) => boolean) | undefined) => void
  removeFailure: (next: ((key: string) => boolean) | undefined) => void
}

function installStorage(): StorageController {
  const values = new Map<string, string>()
  let failsSet: ((key: string, value: string) => boolean) | undefined
  let failsGet: ((key: string) => boolean) | undefined
  let failsRemove: ((key: string) => boolean) | undefined
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: {
      get length() { return values.size },
      key: (index: number) => [...values.keys()][index] ?? null,
      getItem: (key: string) => {
        if (failsGet?.(key)) throw new Error('storage read blocked')
        return values.get(key) ?? null
      },
      setItem: (key: string, value: string) => {
        if (failsSet?.(key, value)) throw new Error('storage write blocked')
        values.set(key, value)
      },
      removeItem: (key: string) => {
        if (failsRemove?.(key)) throw new Error('storage remove blocked')
        values.delete(key)
      },
      clear: () => { values.clear() },
    },
  })
  Object.defineProperty(window, 'BroadcastChannel', { configurable: true, value: undefined })
  return {
    values,
    setFailure: (next) => { failsSet = next },
    getFailure: (next) => { failsGet = next },
    removeFailure: (next) => { failsRemove = next },
  }
}

function installUnavailableStorage() {
  Object.defineProperty(window, 'localStorage', {
    configurable: true,
    value: {
      get length() { throw new Error('storage unavailable') },
      key: () => { throw new Error('storage unavailable') },
      getItem: () => { throw new Error('storage unavailable') },
      setItem: () => { throw new Error('storage unavailable') },
      removeItem: () => { throw new Error('storage unavailable') },
      clear: () => undefined,
    },
  })
  Object.defineProperty(window, 'BroadcastChannel', { configurable: true, value: undefined })
}

function jsonResponse(status: number, body: unknown = undefined): Response {
  return status === 204
    ? new Response(null, { status })
    : new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } })
}

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  lifecycleListeners.splice(0).forEach(({ type, listener, options }) => {
    window.removeEventListener(type, listener, options)
  })
  if (originalStorage) Object.defineProperty(window, 'localStorage', originalStorage)
  else delete (window as { localStorage?: unknown }).localStorage
  if (originalBroadcastChannel) Object.defineProperty(window, 'BroadcastChannel', originalBroadcastChannel)
  else delete (window as { BroadcastChannel?: unknown }).BroadcastChannel
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  vi.resetModules()
})

describe('session lifecycle recovery', () => {
  it('settles direct protected navigation with an orphaned pending marker into the fail-closed recovery route state', async () => {
    const storage = installStorage()
    storage.values.set(`${PENDING_PREFIX}orphaned-before-startup`, 'orphaned-before-startup')
    storage.values.set(STORAGE_KEY, JSON.stringify({
      epoch: 'orphaned-before-startup',
      sequence: 1,
      phase: 'changing',
    }))
    const fetchMock = vi.fn()
    vi.stubGlobal('fetch', fetchMock)
    vi.resetModules()

    const { createAppRouter } = await import('./router')
    const lifecycle = await import('./sessionLifecycle')
    const router = createAppRouter(createMemoryHistory())
    await router.push('/today')
    const readiness = Promise.race([
      router.isReady().then(() => 'ready'),
      new Promise<'timed_out'>((resolve) => setTimeout(() => resolve('timed_out'), 100)),
    ])

    expect(await readiness).toBe('ready')
    expect(router.currentRoute.value.path).toBe('/today')
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)
    expect(lifecycle.isSessionVerified()).toBe(false)
    expect(fetchMock).not.toHaveBeenCalled()
  })

  it('F1: resetSessionCoordination clears an orphaned foreign pending record, publishes a settled marker, verifies, and lets private work resume', async () => {
    const storage = installStorage()
    storage.values.set(`${PENDING_PREFIX}orphaned-before-startup`, 'orphaned-before-startup')
    storage.values.set(STORAGE_KEY, JSON.stringify({
      epoch: 'orphaned-before-startup',
      sequence: 1,
      phase: 'changing',
    }))
    const fetchMock = vi.fn(async (input: RequestInfo | URL) =>
      String(input).endsWith('/me')
        ? jsonResponse(200, { user: { id: 'u1', email: 'alice@example.test', display_name: 'Alice' }, organization: null, platform_admin: true })
        : jsonResponse(200, { ok: true }),
    )
    vi.stubGlobal('fetch', fetchMock)
    vi.resetModules()

    // Same setup as the recovery test above: a direct protected navigation
    // stays fail-closed behind the never-settled foreign attempt, and no
    // request has dispatched yet.
    const { createAppRouter } = await import('./router')
    const lifecycle = await import('./sessionLifecycle')
    const { apiFetch } = await import('./api/client')
    const router = createAppRouter(createMemoryHistory())
    await router.push('/today')
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)
    expect(fetchMock).not.toHaveBeenCalled()

    lifecycle.resetSessionCoordination()

    // The foreign record — never this tab's own — is gone, and a fresh
    // settled marker with a NEW epoch is published (never the abandoned
    // tab's epoch, and generation advances so any private state is
    // discarded everywhere it is observed).
    expect(storage.values.has(`${PENDING_PREFIX}orphaned-before-startup`)).toBe(false)
    const published: unknown = JSON.parse(storage.values.get(STORAGE_KEY) ?? '{}')
    expect(published).toMatchObject({ phase: 'settled' })
    expect((published as { epoch: string }).epoch).not.toBe('orphaned-before-startup')

    // Verification actually completes through the module's own configured
    // `verify` handler (wired by api/queries.ts in this same module graph),
    // not a value this test injects directly.
    await vi.waitFor(() => expect(fetchMock).toHaveBeenCalled())
    await vi.waitFor(() => expect(lifecycle.isSessionVerified()).toBe(true))
    expect(lifecycle.useSessionVerificationPending().value).toBe(false)

    // A private request, previously fenced, now dispatches instead of
    // throwing `SessionVerificationPendingError`.
    await expect(apiFetch('/today')).resolves.toEqual({ ok: true })
  })

  it('cleans its own pre-dispatch pending record after lifecycle-marker publishing fails and storage recovers', async () => {
    const storage = installStorage()
    storage.setFailure((key) => key === STORAGE_KEY)
    const lifecycle = await import('./sessionLifecycle')
    let verified = 0
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => { verified += 1 },
      installVerifiedIdentity: () => undefined,
    })

    expect(() => lifecycle.beginSessionTransition()).toThrow(lifecycle.SessionCoordinationUnavailableError)
    const ownPendingKeys = [...storage.values.keys()].filter((key) => key.startsWith(PENDING_PREFIX))
    // Marker publication failed before transport. The locally known pending
    // key can be removed immediately even though the settled replacement
    // marker must wait for writable marker storage.
    expect(ownPendingKeys).toEqual([])
    expect(storage.values.has(STORAGE_KEY)).toBe(false)

    storage.setFailure(undefined)
    lifecycle.synchronizeSessionLifecycle()

    expect([...storage.values.keys()].filter((key) => key.startsWith(PENDING_PREFIX))).toEqual([])
    expect(lifecycle.useSessionCoordinationUnavailable().value).toBe(false)
    expect(verified).toBe(1)
    expect(JSON.parse(storage.values.get(STORAGE_KEY) ?? '{}')).toMatchObject({ phase: 'settled' })
  })

  it('retires a pending key when its post-write durability read fails before auth dispatch', async () => {
    const storage = installStorage()
    const lifecycle = await import('./sessionLifecycle')
    let verified = 0
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => { verified += 1 },
      installVerifiedIdentity: () => undefined,
    })
    // `setItem` succeeds but its read-back fails, exactly the uncertain
    // write case that must still be treated as a possible own pending key.
    storage.getFailure((key) => key.startsWith(PENDING_PREFIX))
    expect(() => lifecycle.beginSessionTransition()).toThrow(lifecycle.SessionCoordinationUnavailableError)

    storage.getFailure(undefined)
    lifecycle.synchronizeSessionLifecycle()

    expect([...storage.values.keys()].filter((key) => key.startsWith(PENDING_PREFIX))).toEqual([])
    expect(verified).toBe(1)
  })

  it('retires a completed own pending record after remove recovery without releasing another live attempt', async () => {
    const storage = installStorage()
    const lifecycle = await import('./sessionLifecycle')
    let verified = 0
    lifecycle.configureSessionLifecycle({
      discardPrivateState: () => undefined,
      verify: () => { verified += 1 },
      installVerifiedIdentity: () => undefined,
    })
    const ownEpoch = lifecycle.beginSessionTransition()
    const otherEpoch = 'opaque-other-live-attempt'
    storage.values.set(`${PENDING_PREFIX}${otherEpoch}`, otherEpoch)
    storage.removeFailure((key) => key === `${PENDING_PREFIX}${ownEpoch}`)

    lifecycle.settleSessionTransition(ownEpoch)
    expect(storage.values.has(`${PENDING_PREFIX}${ownEpoch}`)).toBe(true)
    expect(lifecycle.useSessionVerificationPending().value).toBe(true)

    storage.removeFailure(undefined)
    lifecycle.synchronizeSessionLifecycle()
    expect(storage.values.has(`${PENDING_PREFIX}${ownEpoch}`)).toBe(false)
    expect(storage.values.has(`${PENDING_PREFIX}${otherEpoch}`)).toBe(true)
    expect(verified).toBe(0)

    // The remote attempt settles through a fresh opaque marker. Only then
    // may this coordinator verify the shared cookie.
    storage.values.delete(`${PENDING_PREFIX}${otherEpoch}`)
    const settled = { epoch: 'opaque-remote-settled', sequence: Date.now() + 5, phase: 'settled' }
    storage.values.set(STORAGE_KEY, JSON.stringify(settled))
    window.dispatchEvent(new StorageEvent('storage', {
      key: STORAGE_KEY,
      newValue: JSON.stringify(settled),
    }))
    expect(verified).toBe(1)
  })

  it('dispatches DELETE /session while lifecycle storage is unavailable and clears the active query client', async () => {
    installUnavailableStorage()
    vi.resetModules()
    const lifecycle = await import('./sessionLifecycle')
    const { useLogoutMutation } = await import('./api/queries')
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(['private-sentinel'], { transcript: 'must clear before logout dispatch' })
    const fetchMock = vi.fn(async () => jsonResponse(204))
    vi.stubGlobal('fetch', fetchMock)
    let logout!: () => Promise<unknown>
    const Harness = defineComponent({
      setup() {
        const mutation = useLogoutMutation()
        logout = () => mutation.mutateAsync()
        return () => null
      },
    })
    const wrapper = mount(Harness, {
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      attachTo: document.body,
    })
    cleanups.push(() => {
      wrapper.unmount()
      queryClient.clear()
    })

    await expect(logout()).resolves.toBeUndefined()

    expect(fetchMock).toHaveBeenCalledTimes(1)
    const [[url, init]] = fetchMock.mock.calls as unknown as [[string, RequestInit]]
    expect(url).toBe('/api/session')
    expect(init).toMatchObject({ method: 'DELETE', credentials: 'include' })
    expect(queryClient.getQueryData(['private-sentinel'])).toBeUndefined()
    expect(lifecycle.useSessionCoordinationUnavailable().value).toBe(true)
    expect(lifecycle.isSessionVerified()).toBe(false)
  })
})
