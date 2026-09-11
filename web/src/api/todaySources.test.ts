import { flushPromises, mount } from '@vue/test-utils'
import { focusManager, QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { defineComponent, effectScope, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from './client'
import {
  queryKeys,
  useDisableTodaySourceMutation,
  useEnableTodaySourceMutation,
  useTodaySources,
} from './queries'
import type { MeResponse, TodaySourceChange, TodaySourcesResponse } from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ACTOR_A = '22222222-2222-2222-2222-222222222222'
const ACTOR_B = '33333333-3333-3333-3333-333333333333'
const LIST_ID = '44444444-4444-4444-4444-444444444444'

function me(actorId = ACTOR_A, organizationId = ORG_ID): MeResponse {
  return {
    user: { id: actorId, email: `${actorId}@example.test`, display_name: actorId },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: organizationId, name: 'Example Realty', role: 'member' },
    platform_admin: false,
  }
}

function sources(name: string): TodaySourcesResponse {
  return {
    limit: 5,
    sources: [{
      list_id: LIST_ID,
      name,
      scope: 'personal',
      revision: 1,
      filter_error: null,
    }],
  }
}

function change(enabled: boolean): TodaySourceChange {
  return { enabled, changed: true }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}

const cleanups: Array<() => void> = []

async function mountSources(
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } }),
) {
  const orgId = ref(ORG_ID)
  const actorId = ref(ACTOR_A)
  let query!: ReturnType<typeof useTodaySources>
  const Harness = defineComponent({
    setup() {
      query = useTodaySources(orgId, actorId)
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
  await flushPromises()
  return { actorId, orgId, query, queryClient }
}

function invalidatedKeys(invalidate: ReturnType<typeof vi.spyOn>) {
  return (invalidate.mock.calls as Array<[{ queryKey?: unknown }]>).map(
    (call: [{ queryKey?: unknown }]) => call[0].queryKey,
  )
}

function expectTodaySourceInvalidations(invalidate: ReturnType<typeof vi.spyOn>) {
  const expected = [queryKeys.today(ORG_ID), queryKeys.todaySources(ORG_ID, ACTOR_A)].map((key) => JSON.stringify(key))
  expect(new Set(invalidatedKeys(invalidate).map((key: unknown) => JSON.stringify(key)))).toEqual(new Set(expected))
}

beforeEach(() => {
  apiFetchMock.mockReset()
  focusManager.setFocused(true)
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  focusManager.setFocused(undefined)
  document.body.innerHTML = ''
})

describe('Today source cache identity and recovery', () => {
  it('drops a late same-Organization actor response and isolates the replacement actor key', async () => {
    const alice = deferred<TodaySourcesResponse>()
    const bob = deferred<TodaySourcesResponse>()
    apiFetchMock.mockReturnValueOnce(alice.promise).mockReturnValueOnce(bob.promise)
    const harness = await mountSources()

    expect(apiFetchMock.mock.calls[0]?.[0]).toBe('/today/sources')
    harness.actorId.value = ACTOR_B
    await flushPromises()
    expect(apiFetchMock).toHaveBeenCalledTimes(2)

    alice.resolve(sources('Alice private source'))
    await flushPromises()
    expect(harness.query.data.value?.sources.some((source) => source.name === 'Alice private source')).not.toBe(true)

    bob.resolve(sources('Bob private source'))
    await flushPromises()
    expect(harness.query.data.value?.sources.map((source) => source.name)).toEqual(['Bob private source'])
    expect(queryKeys.todaySources(ORG_ID, ACTOR_A)).not.toEqual(queryKeys.todaySources(ORG_ID, ACTOR_B))
    expect(queryKeys.todaySources(ORG_ID, ACTOR_A)).not.toEqual(queryKeys.todaySources('other-org', ACTOR_A))
    expect(harness.queryClient.getQueryData(queryKeys.todaySources(ORG_ID, ACTOR_A))).toBeUndefined()
    expect(harness.queryClient.getQueryData(queryKeys.todaySources(ORG_ID, ACTOR_B))).toEqual(sources('Bob private source'))
  })

  it('forces a fresh source read on entry even when the cached configuration is fresh', async () => {
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
    queryClient.setQueryData(queryKeys.todaySources(ORG_ID, ACTOR_A), sources('Old private source'))
    apiFetchMock.mockResolvedValueOnce(sources('Current private source'))

    const harness = await mountSources(queryClient)

    expect(apiFetchMock).toHaveBeenCalledTimes(1)
    expect(harness.query.data.value?.sources.map((source) => source.name)).toEqual(['Current private source'])
  })

  it('forces a fresh source read when the window regains focus inside the stale interval', async () => {
    apiFetchMock.mockResolvedValueOnce(sources('Before focus')).mockResolvedValueOnce(sources('After focus'))
    const harness = await mountSources()
    expect(harness.query.data.value?.sources.map((source) => source.name)).toEqual(['Before focus'])

    focusManager.setFocused(false)
    await flushPromises()
    focusManager.setFocused(true)
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledTimes(2)
    expect(harness.query.data.value?.sources.map((source) => source.name)).toEqual(['After focus'])
  })

  it('refetches active source configuration through the Organization recovery prefix', async () => {
    apiFetchMock.mockResolvedValueOnce(sources('Before reconnect')).mockResolvedValueOnce(sources('After reconnect'))
    const harness = await mountSources()
    expect(apiFetchMock).toHaveBeenCalledTimes(1)

    await harness.queryClient.invalidateQueries({ queryKey: queryKeys.org(ORG_ID) })
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledTimes(2)
    expect(harness.query.data.value?.sources.map((source) => source.name)).toEqual(['After reconnect'])
  })
})

describe('Today source mutation reconciliation', () => {
  it('invalidates both Today and the actor-specific source configuration after enable and disable', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    apiFetchMock.mockResolvedValueOnce(change(true)).mockResolvedValueOnce(change(false))
    const scope = effectScope()
    const enable = scope.run(() => useEnableTodaySourceMutation(orgId, actorId, queryClient))!
    const disable = scope.run(() => useDisableTodaySourceMutation(orgId, actorId, queryClient))!

    await enable.mutateAsync({ listId: LIST_ID, body: { expected_list_revision: 1 } })
    expectTodaySourceInvalidations(invalidate)
    invalidate.mockClear()
    await disable.mutateAsync(LIST_ID)

    expect(apiFetchMock.mock.calls).toEqual([
      [`/today/sources/${LIST_ID}`, { method: 'PUT', body: JSON.stringify({ expected_list_revision: 1 }) }],
      [`/today/sources/${LIST_ID}`, { method: 'DELETE' }],
    ])
    expectTodaySourceInvalidations(invalidate)
    scope.stop()
  })

  it('reconciles a network-uncertain enable or disable by invalidating the same caches without retrying', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    apiFetchMock.mockRejectedValueOnce(new ApiError(0, 'network_error')).mockRejectedValueOnce(new ApiError(0, 'network_error'))
    const scope = effectScope()
    const enable = scope.run(() => useEnableTodaySourceMutation(orgId, actorId, queryClient))!
    const disable = scope.run(() => useDisableTodaySourceMutation(orgId, actorId, queryClient))!

    await expect(enable.mutateAsync({ listId: LIST_ID, body: { expected_list_revision: 1 } })).rejects.toMatchObject({
      code: 'network_error',
    })
    expectTodaySourceInvalidations(invalidate)
    invalidate.mockClear()
    await expect(disable.mutateAsync(LIST_ID)).rejects.toMatchObject({ code: 'network_error' })

    expect(apiFetchMock).toHaveBeenCalledTimes(2)
    expectTodaySourceInvalidations(invalidate)
    scope.stop()
  })

  it('does not invalidate a replacement same-Organization actor after an old source mutation resolves', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const response = deferred<TodaySourceChange>()
    apiFetchMock.mockReturnValueOnce(response.promise)
    const scope = effectScope()
    const enable = scope.run(() => useEnableTodaySourceMutation(orgId, actorId, queryClient))!

    const pending = enable.mutateAsync({ listId: LIST_ID, body: { expected_list_revision: 1 } })
    await Promise.resolve()
    actorId.value = ACTOR_B
    queryClient.setQueryData(queryKeys.me, me(ACTOR_B))
    response.resolve(change(true))
    await pending

    expect(invalidate).not.toHaveBeenCalled()
    scope.stop()
  })
})
