import { effectScope, nextTick, ref, type Ref } from 'vue'
import { QueryClient } from '@tanstack/vue-query'
import { UnauthorizedError } from 'centrifuge'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import { useRealtime, type RealtimeClient, type RealtimeClientFactory } from './useRealtime'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const ORG_ID = '11111111-1111-1111-1111-111111111111'

/** A service-free stand-in for the real `Centrifuge` client (SLICE_003
 * §10: "Client factory injected (fake client in tests, no SDK)"). Mirrors
 * just enough of the real SDK's observed behavior (verified against
 * centrifuge@5.7.1's source, see useRealtime.ts's comments) for the
 * composable's own logic to be exercised: `connect()` emits `connecting`
 * synchronously the way the real client's `_startConnecting` does, and
 * `disconnect()` emits `disconnected` with code 0 the way the real client's
 * `disconnect()` does. Every other transition is driven explicitly by the
 * test via `emit`, standing in for the server. */
class FakeRealtimeClient implements RealtimeClient {
  // `never` so every overload of `RealtimeClient.on` (each with its own
  // typed ctx) is assignable; `emit` passes the test's ctx through.
  handlers = new Map<string, Array<(ctx: never) => void>>()
  disconnectCalls = 0
  connectCalls = 0

  on(event: string, handler: (ctx: never) => void): this {
    const list = this.handlers.get(event) ?? []
    list.push(handler)
    this.handlers.set(event, list)
    return this
  }

  emit(event: string, ctx: unknown): void {
    for (const handler of this.handlers.get(event) ?? []) {
      handler(ctx as never)
    }
  }

  connect(): void {
    this.connectCalls += 1
    this.emit('connecting', { code: 0, reason: 'connect called' })
  }

  disconnect(): void {
    this.disconnectCalls += 1
    this.emit('disconnected', { code: 0, reason: 'disconnect called' })
  }
}

interface Harness {
  orgId: Ref<string>
  clients: FakeRealtimeClient[]
  createClient: RealtimeClientFactory
  urls: string[]
  getTokens: Array<() => Promise<string>>
  queryClient: QueryClient
  invalidateSpy: ReturnType<typeof vi.spyOn>
}

function harness(): Harness {
  const clients: FakeRealtimeClient[] = []
  const urls: string[] = []
  const getTokens: Array<() => Promise<string>> = []
  const createClient: RealtimeClientFactory = ({ url, getToken }) => {
    urls.push(url)
    getTokens.push(getToken)
    const client = new FakeRealtimeClient()
    clients.push(client)
    return client
  }
  const queryClient = new QueryClient()
  const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')
  return { orgId: ref(''), clients, createClient, urls, getTokens, queryClient, invalidateSpy }
}

/** Runs `useRealtime` inside its own `effectScope` — the composable's
 * `onScopeDispose` cleanup fires when the scope stops, standing in for a
 * component unmounting (SLICE_003 §10: "disconnect on unmount"). Returns
 * the result plus the scope so a test can call `scope.stop()`. */
function run(h: Harness, overrides: Partial<Parameters<typeof useRealtime>[0]> = {}) {
  const scope = effectScope()
  const result = scope.run(() =>
    useRealtime({
      orgId: h.orgId,
      createClient: h.createClient,
      queryClient: h.queryClient,
      resolveUrl: () => 'ws://test/connection/websocket',
      ...overrides,
    }),
  )
  if (!result) throw new Error('effectScope.run returned undefined')
  return { scope, ...result }
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.mocked(apiFetch).mockReset()
})

afterEach(() => {
  vi.useRealTimers()
})

describe('useRealtime', () => {
  it('starts idle and does not connect while orgId is empty', () => {
    const h = harness()
    const { status, scope } = run(h)
    expect(status.value).toBe('idle')
    expect(h.clients).toHaveLength(0)
    scope.stop()
  })

  it('connects once orgId resolves, status "connecting" before any prior connect', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    expect(h.clients).toHaveLength(1)
    expect(h.urls).toEqual(['ws://test/connection/websocket'])
    expect(status.value).toBe('connecting')
    scope.stop()
  })

  it('moves to "connected" on the connected event, without invalidating on the first connect', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })
    expect(status.value).toBe('connected')
    expect(h.invalidateSpy).not.toHaveBeenCalled()
    scope.stop()
  })

  it('reports "reconnecting" for connecting after a prior connected, and invalidates everything on the next connected', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    const client = h.clients[0]!
    client.emit('connected', { client: 'c1', transport: 'websocket' })
    expect(status.value).toBe('connected')

    // Network blip: the SDK goes straight to `connecting` again (no
    // intervening `disconnected` for a reconnectable drop — verified
    // against the installed package's `_disconnect` source).
    client.emit('connecting', { code: 1, reason: 'transport closed' })
    expect(status.value).toBe('reconnecting')

    client.emit('connected', { client: 'c2', transport: 'websocket' })
    expect(status.value).toBe('connected')
    await vi.advanceTimersByTimeAsync(250)
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.org(ORG_ID) })
    scope.stop()
  })

  it('maps a terminal disconnect code (unauthorized) to "unavailable"', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('disconnected', { code: 1, reason: 'token rejected' })
    expect(status.value).toBe('unavailable')
    scope.stop()
  })

  it('maps a terminal server disconnect code in 3500-3999 to "unavailable"', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('disconnected', { code: 3600, reason: 'insufficient state' })
    expect(status.value).toBe('unavailable')
    scope.stop()
  })

  // Regression: a 4500-4999 disconnect used to fall through to the "leave
  // status alone" branch, freezing `status` at 'connected' forever (the SDK
  // never emits another event once it has genuinely given up) — the pill
  // would then never show a dead connection as anything but healthy.
  // Verified against centrifuge@5.7.1's `_handleDisconnect` source: this
  // range is non-reconnectable exactly like 3500-3999.
  it('maps a terminal server disconnect code in 4500-4999 to "unavailable" from "connected"', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })
    expect(status.value).toBe('connected')
    h.clients[0]!.emit('disconnected', { code: 4600, reason: 'fatal' })
    expect(status.value).toBe('unavailable')
    scope.stop()
  })

  it('leaves status alone for a non-terminal, non-self-initiated disconnect', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })
    h.clients[0]!.emit('disconnected', { code: 2, reason: 'bad protocol' })
    expect(status.value).toBe('connected')
    scope.stop()
  })

  it('coalesces a burst of publications into one invalidation per key within 250ms', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { scope } = run(h)
    await nextTick()
    const client = h.clients[0]!
    const event = (personId: string) => ({
      v: 1,
      type: 'person.changed',
      organization_id: ORG_ID,
      occurred_at: '2026-08-21T18:02:11.512Z',
      correlation_id: 'c',
      data: { person_id: personId, change: 'inquiry_received' },
    })
    client.emit('publication', { channel: `org:${ORG_ID}`, data: event('a') })
    client.emit('publication', { channel: `org:${ORG_ID}`, data: event('a') })
    client.emit('publication', { channel: `org:${ORG_ID}`, data: event('b') })
    expect(h.invalidateSpy).not.toHaveBeenCalled()

    await vi.advanceTimersByTimeAsync(250)

    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.person(ORG_ID, 'a') })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.person(ORG_ID, 'b') })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.people(ORG_ID) })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.today(ORG_ID) })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.unresolved(ORG_ID) })
    // One call per distinct key, not one per event (5 keys total: person(a),
    // person(b), people, today, unresolved — the last three deduped across
    // all three events).
    expect(h.invalidateSpy).toHaveBeenCalledTimes(5)
    scope.stop()
  })

  it('disconnects and goes idle when orgId becomes empty', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })

    h.orgId.value = ''
    await nextTick()

    expect(h.clients[0]!.disconnectCalls).toBe(1)
    expect(status.value).toBe('idle')
    scope.stop()
  })

  it('tears down and creates a new client on an Organization change', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })

    const otherOrg = '99999999-9999-9999-9999-999999999999'
    h.orgId.value = otherOrg
    await nextTick()

    expect(h.clients).toHaveLength(2)
    expect(h.clients[0]!.disconnectCalls).toBe(1)
    // The new client's first connect is a fresh connection, not a
    // reconnect — hasConnectedBefore resets per client.
    expect(h.clients[1]!.connectCalls).toBe(1)
    scope.stop()
  })

  it('disconnects on scope stop (stands in for component unmount)', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })

    scope.stop()

    expect(h.clients[0]!.disconnectCalls).toBe(1)
    expect(status.value).toBe('idle')
  })

  it('exposes an explicit disconnect() that tears down without waiting for scope cleanup', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { status, disconnect, scope } = run(h)
    await nextTick()
    h.clients[0]!.emit('connected', { client: 'c1', transport: 'websocket' })

    disconnect()

    expect(h.clients[0]!.disconnectCalls).toBe(1)
    expect(status.value).toBe('idle')
    scope.stop()
  })

  describe('getToken (§9)', () => {
    it('on a 401, throws the SDK UnauthorizedError and invalidates the me query', async () => {
      const h = harness()
      h.orgId.value = ORG_ID
      const { scope } = run(h)
      await nextTick()

      vi.mocked(apiFetch).mockRejectedValueOnce(new ApiError(401, 'unauthorized'))
      const getToken = h.getTokens[0]!
      await expect(getToken()).rejects.toBeInstanceOf(UnauthorizedError)
      expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.me })
      scope.stop()
    })

    it('on any other error, rethrows unchanged so the SDK retries', async () => {
      const h = harness()
      h.orgId.value = ORG_ID
      const { scope } = run(h)
      await nextTick()

      const unavailable = new ApiError(503, 'unavailable')
      vi.mocked(apiFetch).mockRejectedValueOnce(unavailable)
      const getToken = h.getTokens[0]!
      await expect(getToken()).rejects.toBe(unavailable)
      expect(h.invalidateSpy).not.toHaveBeenCalledWith({ queryKey: queryKeys.me })
      scope.stop()
    })

    it('resolves with the token on success', async () => {
      const h = harness()
      h.orgId.value = ORG_ID
      const { scope } = run(h)
      await nextTick()

      vi.mocked(apiFetch).mockResolvedValueOnce({ token: 'jwt-abc' })
      const getToken = h.getTokens[0]!
      await expect(getToken()).resolves.toBe('jwt-abc')
      scope.stop()
    })
  })
})
