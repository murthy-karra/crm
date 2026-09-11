import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { computed, defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from './client'
import { fetchSavedListCount, queryKeys, useMe } from './queries'
import { invalidationsFor, reconnectInvalidations } from '../realtime/events'
import { useSavedListCountScheduler } from './savedListCounts'
import type { MeResponse, SavedListCountResponse, SavedListMetadata } from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})
vi.mock('./queries', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./queries')>()
  return { ...actual, fetchSavedListCount: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const fetchSavedListCountMock = vi.mocked(fetchSavedListCount)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ACTOR_ID = '22222222-2222-2222-2222-222222222222'

type CountItem = Pick<SavedListMetadata, 'id' | 'revision'>
type Scheduler = ReturnType<typeof useSavedListCountScheduler>

interface Deferred<T> {
  promise: Promise<T>
  resolve(value: T): void
  reject(reason: unknown): void
  readonly settled: boolean
}

function deferred<T>(): Deferred<T> {
  let settled = false
  let resolvePromise!: (value: T) => void
  let rejectPromise!: (reason: unknown) => void
  const promise = new Promise<T>((resolve, reject) => {
    resolvePromise = (value) => {
      settled = true
      resolve(value)
    }
    rejectPromise = (reason) => {
      settled = true
      reject(reason)
    }
  })
  return {
    promise,
    resolve: resolvePromise,
    reject: rejectPromise,
    get settled() { return settled },
  }
}

interface CountRequest {
  listId: string
  revision: number
  signal: AbortSignal
  response: Deferred<SavedListCountResponse>
}

function item(number: number): CountItem {
  return { id: `list-${number}`, revision: 1 }
}

function count(item: CountItem, value: number): SavedListCountResponse {
  return { list_id: item.id, revision: item.revision, count: value, truncated: false }
}

function me(displayName = 'Agent'): MeResponse {
  return {
    user: { id: ACTOR_ID, email: 'agent@example.test', display_name: displayName },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: ORG_ID, name: 'Example Realty', role: 'member' },
    platform_admin: false,
  }
}

const cleanups: Array<() => void> = []

async function mountScheduler(initialItems: readonly CountItem[] = []) {
  const items = ref<readonly CountItem[]>(initialItems)
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
  queryClient.setQueryData(queryKeys.me, me())
  let scheduler!: Scheduler
  let session!: ReturnType<typeof useMe>['data']

  const Harness = defineComponent({
    setup() {
      // ListsView passes this exact Vue Query ref to the scheduler. This
      // retains Vue's proxy representation rather than using a raw fixture.
      const meQuery = useMe()
      session = meQuery.data
      scheduler = useSavedListCountScheduler(
        computed(() => meQuery.data.value?.organization?.id ?? ''),
        computed(() => meQuery.data.value?.user.id ?? ''),
        items,
        meQuery.data,
      )
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

  return { items, queryClient, scheduler, session }
}

function installDeferredFetches() {
  const requests: CountRequest[] = []
  let maxUnsettled = 0
  fetchSavedListCountMock.mockImplementation((listId, revision, signal) => {
    const response = deferred<SavedListCountResponse>()
    requests.push({ listId, revision, signal: signal!, response })
    maxUnsettled = Math.max(maxUnsettled, requests.filter((request) => !request.response.settled).length)
    return response.promise
  })
  return { requests, maxUnsettled: () => maxUnsettled }
}

async function settleAll(fetches: ReturnType<typeof installDeferredFetches>) {
  // Settling a slot admits the next queued item. Walk the growing request
  // list so a complete 25-row page is genuinely warm before an invalidation
  // starts its replacement batch.
  for (let index = 0; index < fetches.requests.length; index++) {
    const request = fetches.requests[index]!
    if (!request.response.settled) {
      request.response.resolve({
        list_id: request.listId,
        revision: request.revision,
        count: index,
        truncated: false,
      })
    }
    await flushPromises()
  }
  await flushPromises()
}

beforeEach(() => {
  apiFetchMock.mockReset()
  // `useMe` starts from a seeded cache. If its cache is removed to model a
  // logout, keep the automatic refetch pending rather than inventing a new
  // authenticated session in the test.
  apiFetchMock.mockImplementation(() => new Promise<never>(() => {}))
  fetchSavedListCountMock.mockReset()
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

describe('useSavedListCountScheduler', () => {
  it('keeps 25 initial, manual-refresh, and retry requests four-wide', async () => {
    const fetches = installDeferredFetches()
    const harness = await mountScheduler()
    const page = Array.from({ length: 25 }, (_, index) => item(index + 1))

    harness.items.value = page
    await flushPromises()

    expect(fetches.requests).toHaveLength(4)
    expect(fetches.maxUnsettled()).toBe(4)
    expect(harness.scheduler.isRefreshing.value).toBe(true)

    harness.scheduler.refresh()
    await flushPromises()

    // The mock deliberately ignores aborts. Held jobs must not permit a
    // second batch to exceed the scheduler's four active slots.
    expect(fetches.requests).toHaveLength(4)
    expect(fetches.requests.every((request) => request.signal.aborted)).toBe(true)

    for (const [index, request] of fetches.requests.entries()) {
      request.response.resolve(count(page[index]!, 100 + index))
    }
    await flushPromises()

    expect(fetches.requests).toHaveLength(8)
    expect(fetches.maxUnsettled()).toBe(4)

    // A retry while a four-wide batch is held is queued, not dispatched past
    // the concurrency cap.
    harness.scheduler.retry(page[0]!)
    await flushPromises()
    expect(fetches.requests).toHaveLength(8)
    expect(fetches.maxUnsettled()).toBe(4)
  })

  it('does not admit an ignored late count after A → logout → A', async () => {
    const fetches = installDeferredFetches()
    const harness = await mountScheduler()
    const list = item(1)
    harness.items.value = [list]
    await flushPromises()

    const old = fetches.requests[0]!
    const oldSession = harness.session.value!
    void harness.queryClient.resetQueries({ queryKey: queryKeys.me, exact: true })
    await flushPromises()
    expect(harness.session.value).toBeUndefined()
    expect(old.signal.aborted).toBe(true)

    const replacementSession = me('Agent after relogin')
    harness.queryClient.setQueryData(queryKeys.me, replacementSession)
    await flushPromises()
    expect(harness.session.value).not.toBe(oldSession)

    old.response.resolve(count(list, 73))
    await flushPromises()

    expect(harness.queryClient.getQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, list.id, list.revision))).toBeUndefined()
    expect(harness.scheduler.stateFor(list).kind).toBe('loading')

    const current = fetches.requests.at(-1)!
    expect(current).not.toBe(old)
    expect(current.signal.aborted).toBe(false)
    current.response.resolve(count(list, 7))
    await flushPromises()

    expect(harness.scheduler.stateFor(list)).toEqual({ kind: 'ready', count: 7, truncated: false })
    expect(harness.queryClient.getQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, list.id, list.revision))).toEqual(count(list, 7))
  })

  it('releases held jobs and restarts them for a same-ID replacement MeResponse', async () => {
    const fetches = installDeferredFetches()
    const harness = await mountScheduler()
    const lists = Array.from({ length: 5 }, (_, index) => item(index + 1))
    harness.items.value = lists
    await flushPromises()

    const held = [...fetches.requests]
    const initialSession = harness.session.value!
    expect(held).toHaveLength(4)

    harness.queryClient.setQueryData(queryKeys.me, me('Agent after session refresh'))
    await flushPromises()

    expect(harness.session.value).not.toBe(initialSession)
    expect(held.every((request) => request.signal.aborted)).toBe(true)
    expect(fetches.requests).toHaveLength(4)

    for (const [index, request] of held.entries()) {
      request.response.resolve(count(lists[index]!, 90 + index))
    }
    await flushPromises()

    expect(fetches.requests).toHaveLength(8)
    expect(fetches.requests.slice(4).map((request) => request.listId)).toEqual(lists.slice(0, 4).map((list) => list.id))
    expect(fetches.requests.slice(4).every((request) => !request.signal.aborted)).toBe(true)
  })

  it('restarts a warmed 25-row page four-wide through the realtime and reconnect invalidation keys', async () => {
    const fetches = installDeferredFetches()
    const harness = await mountScheduler()
    const page = Array.from({ length: 25 }, (_, index) => item(index + 1))
    harness.items.value = page
    await flushPromises()
    await settleAll(fetches)

    expect(fetches.requests).toHaveLength(25)
    expect(fetches.maxUnsettled()).toBe(4)
    expect(page.every((entry) => harness.scheduler.stateFor(entry).kind === 'ready')).toBe(true)

    const personChanged = invalidationsFor({
      v: 1,
      type: 'person.changed',
      organization_id: ORG_ID,
      occurred_at: '2026-09-06T00:00:00.000Z',
      correlation_id: 'count-refresh',
      data: { person_id: 'person-1', change: 'assignment_changed' },
    }, ORG_ID)
    for (const key of personChanged) await harness.queryClient.invalidateQueries({ queryKey: key })
    await flushPromises()

    expect(fetches.requests).toHaveLength(29)
    expect(fetches.maxUnsettled()).toBe(4)
    await settleAll(fetches)
    expect(fetches.requests).toHaveLength(50)

    await harness.queryClient.invalidateQueries({ queryKey: reconnectInvalidations(ORG_ID)[0] })
    await flushPromises()
    expect(fetches.requests).toHaveLength(54)
    expect(fetches.maxUnsettled()).toBe(4)
  })

  it.each([
    new ApiError(404, 'not_found'),
    new ApiError(409, 'saved_list_conflict'),
  ])('marks a count rejected as %s stale without caching a count', async (error) => {
    const fetches = installDeferredFetches()
    const harness = await mountScheduler()
    const list = item(1)
    harness.items.value = [list]
    await flushPromises()

    fetches.requests[0]!.response.reject(error)
    await flushPromises()

    expect(harness.scheduler.stateFor(list)).toEqual({ kind: 'stale' })
    expect(harness.scheduler.isRefreshing.value).toBe(false)
    expect(harness.queryClient.getQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, list.id, list.revision))).toBeUndefined()
  })

  // Slice 011e e2 review round 1, F1: `invalid_tag` must reach `kind:
  // 'invalid'`, not silently fall through to `kind: 'unavailable'` the
  // way an unrecognized code would.
  it.each(['invalid_tag', 'invalid_field', 'invalid_option'] as const)('marks a count rejected 422 %s as repairable', async (code) => {
    const fetches = installDeferredFetches()
    const harness = await mountScheduler()
    const list = item(1)
    harness.items.value = [list]
    await flushPromises()

    fetches.requests[0]!.response.reject(new ApiError(422, code))
    await flushPromises()

    expect(harness.scheduler.stateFor(list)).toEqual({ kind: 'invalid', error: code })
  })
})
