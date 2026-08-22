// The realtime composable (SLICE_003 §9, §10; D-023). Turns a Centrifugo
// connection's lifecycle into a `status` a view can render (a quiet
// connection indicator, §10) and its `publication` events into coalesced
// TanStack Query invalidations. The SDK client is always injected via
// `createClient` — this module never imports `centrifuge` itself (see
// client.ts) — so its tests are service-free: a fake emitter stands in for
// the real client and no network or WebSocket is ever opened.
import { onScopeDispose, ref, toValue, watch, type MaybeRefOrGetter, type Ref } from 'vue'
import type { QueryClient, QueryKey } from '@tanstack/vue-query'
import { UnauthorizedError } from 'centrifuge'
import { ApiError } from '../api/client'
import { fetchRealtimeToken, queryKeys } from '../api/queries'
import { queryClient as sharedQueryClient } from '../query-client'
import { invalidationsFor, reconnectInvalidations } from './events'

export type RealtimeStatus = 'idle' | 'connecting' | 'connected' | 'reconnecting' | 'unavailable'

// Minimal structural surface useRealtime needs from a Centrifuge client —
// satisfied by the real SDK's `Centrifuge` (client.ts's `createRealtimeClient`)
// and by a fake emitter in tests. Deliberately narrower than the SDK's own
// `ClientEvents`/`TypedEventEmitter` types (read from the installed
// `centrifuge@5.7.1` package, not guessed): only the four events and two
// methods this composable actually uses.
export interface RealtimeConnectingContext {
  code: number
  reason: string
}
export interface RealtimeConnectedContext {
  client: string
  transport: string
}
export interface RealtimeDisconnectedContext {
  code: number
  reason: string
}
export interface RealtimePublicationContext {
  channel: string
  data: unknown
}

export interface RealtimeClient {
  on(event: 'connecting', handler: (ctx: RealtimeConnectingContext) => void): unknown
  on(event: 'connected', handler: (ctx: RealtimeConnectedContext) => void): unknown
  on(event: 'disconnected', handler: (ctx: RealtimeDisconnectedContext) => void): unknown
  on(event: 'publication', handler: (ctx: RealtimePublicationContext) => void): unknown
  connect(): void
  disconnect(): void
}

export type RealtimeClientFactory = (params: { url: string; getToken: () => Promise<string> }) => RealtimeClient

export interface UseRealtimeOptions {
  /** The viewer's active Organization id. Connects once this is non-empty
   * (i.e. after `me` resolves — the caller derives this from `useMe()`),
   * disconnects when it becomes empty again, and tears down and reconnects
   * on every change (a different Organization is a different connection,
   * never a channel resubscribe — D-023 §1's channel is fixed per
   * connection). */
  orgId: MaybeRefOrGetter<string>
  /** Builds the underlying client. Production callers pass
   * `createRealtimeClient` from client.ts; tests inject a fake. */
  createClient: RealtimeClientFactory
  /** Defaults to the app's shared singleton (query-client.ts) — overridable
   * so tests never touch global state. */
  queryClient?: QueryClient
  /** Resolves the Centrifugo endpoint URL. Production callers pass
   * `() => resolveRealtimeUrl(window.location)` (client.ts); required
   * (not defaulted here) so this module never imports client.ts — which
   * imports the real `centrifuge` SDK for `createRealtimeClient` — keeping
   * useRealtime.test.ts free of the SDK even transitively. */
  resolveUrl: () => string
  /** §9 "duplicate events": a burst of events within this window collapses
   * into one refetch per key. Default 250 ms per §10. */
  coalesceMs?: number
}

export interface UseRealtimeResult {
  status: Ref<RealtimeStatus>
  /** Explicit teardown, for a caller that wants to disconnect without
   * waiting for reactive cleanup (its effect scope stopping — component
   * unmount does this automatically via `onScopeDispose` below). */
  disconnect: () => void
}

// disconnectedCodes.unauthorized (1) and the server-pushed non-reconnectable
// ranges 3500–3999 and 4500–4999 — verified against centrifuge@5.7.1's
// `codes.d.ts` and the `_handleDisconnect` source directly:
//   let reconnect = true;
//   if ((code >= 3500 && code < 4000) || (code >= 4500 && code < 5000)) {
//     reconnect = false;
//   }
// SLICE_003 §10's wording only named the first range; the second is a real
// terminal case the SDK produces (a fixed spec typo/omission, corrected
// here — not a contract widening, since this classification is purely
// client-internal and never observed by the backend or on the wire).
// Getting this wrong is not benign: a reconnectable drop never reaches this
// function at all (the SDK goes straight to a `connecting` event, no
// `disconnected`), so a 4500–4999 code that fell through here uncaught
// would leave `status` frozen at whatever it last was — typically
// `'connected'` — forever, since the SDK is genuinely done and emits
// nothing further. The pill would then never show, silently misreporting a
// dead connection as healthy.
const UNAUTHORIZED_CODE = 1
const TERMINAL_CODE_RANGES: ReadonlyArray<readonly [number, number]> = [
  [3500, 3999],
  [4500, 4999],
]
// disconnectedCodes.disconnectCalled — the code the SDK's own `.disconnect()`
// reports back on its `disconnected` event (verified against the installed
// package's source: `disconnect() { this._disconnect(disconnectedCodes
// .disconnectCalled, 'disconnect called', false); }`).
const DISCONNECT_CALLED_CODE = 0

function isTerminalDisconnect(code: number): boolean {
  if (code === UNAUTHORIZED_CODE) return true
  return TERMINAL_CODE_RANGES.some(([min, max]) => code >= min && code <= max)
}

export function useRealtime(options: UseRealtimeOptions): UseRealtimeResult {
  const qc = options.queryClient ?? sharedQueryClient
  const coalesceMs = options.coalesceMs ?? 250
  const resolveUrl = options.resolveUrl

  const status = ref<RealtimeStatus>('idle')

  let client: RealtimeClient | null = null
  let hasConnectedBefore = false
  let pending = new Map<string, QueryKey>()
  let flushTimer: ReturnType<typeof setTimeout> | null = null

  function scheduleInvalidation(keys: QueryKey[]): void {
    if (keys.length === 0) return
    for (const key of keys) {
      pending.set(JSON.stringify(key), key)
    }
    if (flushTimer !== null) return
    flushTimer = setTimeout(() => {
      const keysToFlush = [...pending.values()]
      pending = new Map()
      flushTimer = null
      for (const key of keysToFlush) {
        void qc.invalidateQueries({ queryKey: key })
      }
    }, coalesceMs)
  }

  /** §9 getToken error mapping: a 401 means the session is dead — throw the
   * SDK's own `UnauthorizedError` (imported from `centrifuge`, not a
   * lookalike: the real client's internal `instanceof` check is what turns
   * this into a terminal disconnect, verified against the installed
   * package's source) and invalidate `me` so its refetch surfaces the 401 as
   * a *query* failure, which query-client.ts's global handler routes to
   * /login. Anything else (503, network) rethrows unchanged so the SDK
   * retries with backoff — a transient outage must not kill realtime. */
  async function getToken(): Promise<string> {
    try {
      const response = await fetchRealtimeToken()
      return response.token
    } catch (error) {
      if (error instanceof ApiError && error.status === 401) {
        void qc.invalidateQueries({ queryKey: queryKeys.me })
        throw new UnauthorizedError('unauthorized')
      }
      throw error
    }
  }

  function teardown(): void {
    if (flushTimer !== null) {
      clearTimeout(flushTimer)
      flushTimer = null
    }
    pending = new Map()
    if (client) {
      const current = client
      client = null
      current.disconnect()
    }
    hasConnectedBefore = false
  }

  function connectFor(orgId: string): void {
    teardown()

    const newClient = options.createClient({ url: resolveUrl(), getToken })
    client = newClient

    newClient.on('connecting', () => {
      status.value = hasConnectedBefore ? 'reconnecting' : 'connecting'
    })

    newClient.on('connected', () => {
      const isReconnect = hasConnectedBefore
      status.value = 'connected'
      hasConnectedBefore = true
      if (isReconnect) {
        scheduleInvalidation(reconnectInvalidations(orgId))
      }
    })

    newClient.on('disconnected', (ctx) => {
      if (ctx.code === DISCONNECT_CALLED_CODE) {
        status.value = 'idle'
      } else if (isTerminalDisconnect(ctx.code)) {
        status.value = 'unavailable'
      }
      // Any other code: per `_handleDisconnect`'s own `reconnect` decision
      // (see the note above `isTerminalDisconnect`), every reconnectable
      // code skips a `disconnected` event entirely and goes straight to
      // `connecting` — so a non-terminal, non-disconnectCalled code should
      // not reach here in practice. Defensive no-op rather than guessing a
      // status, so a code neither this composable nor the SDK's own source
      // anticipates cannot mislabel the connection.
    })

    newClient.on('publication', (ctx) => {
      scheduleInvalidation(invalidationsFor(ctx.data, orgId))
    })

    newClient.connect()
  }

  watch(
    () => toValue(options.orgId),
    (orgId) => {
      if (orgId === '') {
        teardown()
        status.value = 'idle'
        return
      }
      connectFor(orgId)
    },
    { immediate: true },
  )

  function disconnect(): void {
    teardown()
    status.value = 'idle'
  }

  // Works both inside a component (its effect scope stops on unmount) and
  // in a bare `effectScope()` (useRealtime.test.ts) — unlike `onUnmounted`,
  // which only fires inside a component instance. Vue 3.5's
  // `onScopeDispose` silently no-ops with no active scope, so calling
  // useRealtime outside both is inert rather than throwing.
  onScopeDispose(disconnect)

  return { status, disconnect }
}
