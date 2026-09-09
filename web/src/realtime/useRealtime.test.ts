import { effectScope, nextTick, ref, type Ref } from 'vue'
import { QueryClient, useMutation, useQuery } from '@tanstack/vue-query'
import { UnauthorizedError } from 'centrifuge'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { personMutationKey, queryKeys, useAddPersonTagMutation } from '../api/queries'
import type { PeopleResponse, PersonDetailResponse, PersonTagMutationResponse } from '../api/types'
import { useRealtime, type RealtimeClient, type RealtimeClientFactory } from './useRealtime'

/** Registers a mutation on `qc` with the given Person's mutationKey and
 * never resolves it — a stand-in for "a Person mutation is in flight",
 * exercising the same `qc.isMutating({ mutationKey: [...] })` prefix match
 * the four real Person mutations (api/queries.ts) register under. Returns
 * the owning `effectScope` so the test can stop it (which does not cancel
 * the pending mutation — a mismatched expectation would surface as a
 * failed assertion, not a hang, since nothing here awaits it). */
function startPendingPersonMutation(qc: QueryClient, orgId: string, personId: string) {
  const scope = effectScope()
  const mutation = scope.run(() =>
    useMutation(
      {
        mutationKey: personMutationKey(orgId, personId),
        mutationFn: () => new Promise<void>(() => {}),
      },
      qc,
    ),
  )!
  mutation.mutate()
  return scope
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

/** Mounts a real, active `useQuery` observer for `queryKey` on `qc`, so
 * `refetchQueries({ type: 'active', ... })` (the item 5 hold release) has
 * something to actually refetch and this test can count real
 * `apiFetch` calls against it. Returns the owning `effectScope`. */
function mountActiveQuery<T>(qc: QueryClient, queryKey: readonly unknown[], queryFn: () => Promise<T>) {
  const scope = effectScope()
  scope.run(() => useQuery({ queryKey: queryKey as unknown[], queryFn }, qc))
  return scope
}

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
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.savedListCounts(ORG_ID) })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.unresolved(ORG_ID) })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.inquirySources(ORG_ID) })
    // One call per distinct key, not one per event (7 keys total: person(a),
    // person(b), people, today, saved-list counts, unresolved,
    // inquiry-sources — shared keys are deduped across all three events).
    expect(h.invalidateSpy).toHaveBeenCalledTimes(7)
    scope.stop()
  })

  // LATER item 5 (docs/tasks/LATER_BATCH_2026-09-08.md): while any Person
  // mutation is pending (a blanket check — the event here is for an
  // UNRELATED Person, not the one being mutated), the People and Person
  // keys are marked stale without refetching, so this `person.changed`
  // cannot revert a still-in-flight optimistic row; every other key
  // (Today, saved-list counts, unresolved, inquiry sources) refetches
  // normally, since only the People/Person shapes carry a Person row a
  // mutation could be holding optimistic state in.
  it('marks People/Person keys stale without refetching while a Person mutation is pending, but refetches every other key normally', async () => {
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
      data: { person_id: personId, change: 'stage_changed' },
    })

    // A mutation for a DIFFERENT Person than the one the event is about —
    // the guard is blanket, not scoped to the event's own Person id.
    const mutationScope = startPendingPersonMutation(h.queryClient, ORG_ID, 'unrelated-person')
    await nextTick()

    client.emit('publication', { channel: `org:${ORG_ID}`, data: event('a') })
    await vi.advanceTimersByTimeAsync(250)

    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.person(ORG_ID, 'a'), refetchType: 'none' })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.people(ORG_ID), refetchType: 'none' })
    // stage_changed does not touch unresolved/inquiry-sources; today and
    // saved-list counts are neither 'people' nor 'person' shaped, so they
    // keep their normal (refetching) invalidate call.
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.today(ORG_ID) })
    expect(h.invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.savedListCounts(ORG_ID) })

    mutationScope.stop()
    scope.stop()
  })

  // Round 1 review FIX (item 5 hold release): the hold used to be released
  // only by stage/assignment settles, because a tag mutation's own settle
  // invalidate targets `person(id)` only, never the People list. This
  // uses a REAL, deferred `useAddPersonTagMutation` (not the fake pending
  // mutation above) so its actual settle path exercises
  // settlePersonMutation's hold-release `refetchQueries`.
  it('a pending tag mutation for one Person: an unrelated person.changed holds the People list stale, and the tag mutation settling releases the hold', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { scope } = run(h)
    await nextTick()
    const client = h.clients[0]!

    let peopleFetchCount = 0
    vi.mocked(apiFetch).mockImplementation((path: string) => {
      if (path === '/people') {
        peopleFetchCount += 1
        return Promise.resolve({ people: [], truncated: false } satisfies PeopleResponse)
      }
      return Promise.reject(new Error(`unexpected path ${path}`))
    })
    // An ACTIVE People observer -- refetchQueries({ type: 'active' }) only
    // touches observed queries.
    const peopleScope = mountActiveQuery<PeopleResponse>(h.queryClient, queryKeys.people(ORG_ID), () =>
      apiFetch('/people'),
    )
    await nextTick()
    await nextTick()
    expect(peopleFetchCount).toBe(1)

    // A tag mutation for a Person is now pending.
    const tagDeferred = deferred<PersonTagMutationResponse>()
    vi.mocked(apiFetch).mockImplementation((path: string) => {
      if (path === '/people') {
        peopleFetchCount += 1
        return Promise.resolve({ people: [], truncated: false } satisfies PeopleResponse)
      }
      if (path.includes('/tags/')) return tagDeferred.promise
      return Promise.reject(new Error(`unexpected path ${path}`))
    })
    const mutationScope = effectScope()
    const tagMutation = mutationScope.run(() => useAddPersonTagMutation(ORG_ID, 'mutated-person', h.queryClient))!
    tagMutation.mutate({ personId: 'mutated-person', tagId: 'tag-x' })
    await nextTick()

    // A person.changed for an UNRELATED Person arrives while the tag
    // mutation is pending.
    client.emit('publication', {
      channel: `org:${ORG_ID}`,
      data: {
        v: 1,
        type: 'person.changed',
        organization_id: ORG_ID,
        occurred_at: '2026-08-21T18:02:11.512Z',
        correlation_id: 'c',
        data: { person_id: 'other-person', change: 'stage_changed' },
      },
    })
    await vi.advanceTimersByTimeAsync(250)

    // Held: marked stale, but no extra fetch beyond the initial mount.
    expect(peopleFetchCount).toBe(1)
    expect(h.queryClient.getQueryState(queryKeys.people(ORG_ID))?.isInvalidated).toBe(true)

    // The tag mutation resolves: settlePersonMutation's deferred
    // hold-release refetches every stale active query under the org
    // branch, including the People list the realtime path marked stale.
    tagDeferred.resolve({ tags: [], changed: true })
    await vi.advanceTimersByTimeAsync(0)
    await nextTick()
    await nextTick()
    expect(peopleFetchCount).toBe(2)

    mutationScope.stop()
    peopleScope.stop()
    scope.stop()
  })

  // Round 1 review FIX (item 5 hold release), the SAME-Person case: the
  // optimistic tag write must stay visible (not reverted by the realtime
  // stale mark) while the mutation is pending, and once it errors and
  // rolls back, the hold's release must not leave the rolled-back
  // snapshot as the last word — a real refetch confirms server truth.
  it('a pending tag mutation for the SAME Person that then errors: the realtime stale mark is refetched on rollback, leaving no stale value behind', async () => {
    const h = harness()
    h.orgId.value = ORG_ID
    const { scope } = run(h)
    await nextTick()
    const client = h.clients[0]!

    const personId = 'mutated-person'
    const existingTag = { id: 'tag-existing', name: 'Existing' }
    const newTag = { id: 'tag-new', name: 'New' }
    const serverPerson: PersonDetailResponse = {
      person: {
        id: personId,
        first_name: null,
        last_name: null,
        display_name: 'P',
        stage: { id: 'stage-1', name: 'Lead' },
        assigned_user: null,
        primary_email: 'p@example.test',
        primary_phone: null,
        inquiry_count: 1,
        last_inquiry_at: null,
        created_at: '2026-01-01T00:00:00Z',
      },
      contact_methods: [],
      inquiries: [],
      history: [],
      tags: [existingTag],
    }
    h.queryClient.setQueryData(queryKeys.tags(ORG_ID), { tags: [{ ...newTag, person_count: 0, can_manage: true }] })
    h.queryClient.setQueryData(queryKeys.person(ORG_ID, personId), serverPerson)

    let personFetchCount = 0
    vi.mocked(apiFetch).mockImplementation((path: string) => {
      if (path === `/people/${personId}`) {
        personFetchCount += 1
        return Promise.resolve(serverPerson)
      }
      return Promise.reject(new Error(`unexpected path ${path}`))
    })
    const personScope = mountActiveQuery<PersonDetailResponse>(h.queryClient, queryKeys.person(ORG_ID, personId), () =>
      apiFetch(`/people/${personId}`),
    )
    await nextTick()
    await nextTick()
    expect(personFetchCount).toBe(1)

    const tagDeferred = deferred<PersonTagMutationResponse>()
    vi.mocked(apiFetch).mockImplementation((path: string) => {
      if (path === `/people/${personId}`) {
        personFetchCount += 1
        return Promise.resolve(serverPerson)
      }
      if (path.includes('/tags/')) return tagDeferred.promise
      return Promise.reject(new Error(`unexpected path ${path}`))
    })
    const mutationScope = effectScope()
    const tagMutation = mutationScope.run(() => useAddPersonTagMutation(ORG_ID, personId, h.queryClient))!
    tagMutation.mutate({ personId, tagId: newTag.id })
    // onMutate is async (it awaits cancelQueries before writing the
    // optimistic value) — a timer/microtask flush, not just one nextTick,
    // is needed for it to have fully applied.
    await vi.advanceTimersByTimeAsync(0)
    await nextTick()

    // The optimistic write is visible (lower-cased-name order:
    // "Existing" sorts before "New").
    expect(h.queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, personId))?.tags).toEqual([
      existingTag,
      newTag,
    ])

    // A person.changed for the SAME Person arrives while pending.
    client.emit('publication', {
      channel: `org:${ORG_ID}`,
      data: {
        v: 1,
        type: 'person.changed',
        organization_id: ORG_ID,
        occurred_at: '2026-08-21T18:02:11.512Z',
        correlation_id: 'c',
        data: { person_id: personId, change: 'tags_changed' },
      },
    })
    await vi.advanceTimersByTimeAsync(250)

    // Held: the optimistic value is untouched (no extra fetch yet).
    expect(personFetchCount).toBe(1)
    expect(h.queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, personId))?.tags).toEqual([
      existingTag,
      newTag,
    ])

    // The mutation errors: rollback restores the pre-mutation snapshot,
    // then settlePersonMutation's deferred check (nothing else pending)
    // fires both its calls — the mutation's own `invalidateQueries({
    // queryKey: person(id) })` (GET #2) and the hold-release
    // `refetchQueries` right behind it (GET #3, `cancelRefetch` defaults
    // to `true` so it supersedes GET #2) — releasing the realtime hold
    // too, so the rolled-back snapshot is not the last word (same
    // double-supersede mechanism as queries.test.ts's "never shows the
    // old stage" test).
    tagDeferred.reject(new ApiError(409, 'person_tag_limit_reached'))
    await vi.advanceTimersByTimeAsync(0)
    await nextTick()
    await nextTick()

    expect(personFetchCount).toBe(3)
    expect(h.queryClient.getQueryData<PersonDetailResponse>(queryKeys.person(ORG_ID, personId))?.tags).toEqual([
      existingTag,
    ])

    mutationScope.stop()
    personScope.stop()
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
