// SLICE_014 §4, §8.6: the router guard's `prefetchTodayData` call and
// TodayView's own `useToday`/`useTodaySources`/`useTodayFeeds` mount must
// share one in-flight request per endpoint rather than doubling up.
// Exercised at the level the guard and the view actually share — one real
// QueryClient carrying an already-resolved `me` (as the guard's own
// `ensureQueryData` would have left it) — rather than driving the full
// router guard's session-lifecycle machinery, which is covered on its own
// terms in router.test.ts and sessionLifecycle.recovery.test.ts.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './api/client'
import { prefetchTodayData, queryKeys } from './api/queries'
import TodayView from './views/TodayView.vue'
import type { MeResponse } from './api/types'

vi.mock('./api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)

const MEMBER: MeResponse = {
  user: { id: 'actor-1', email: 'alice@acme.test', display_name: 'Alice' },
  organization: { id: 'org-1', name: 'Acme Realty', role: 'member' },
  platform_admin: false,
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => { resolve = yes })
  return { promise, resolve }
}

async function mountTodayView(queryClient: QueryClient) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }],
  })
  await router.push('/today')
  const wrapper = mount(TodayView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

describe('Today preload/prefetch join (SLICE_014 §4, §8.6)', () => {
  it('issues exactly one request each to Today, sources and feeds when TodayView mounts while the guard-triggered prefetch is still in flight', async () => {
    // staleTime matches query-client.ts's real default (30s): the guard's
    // own `ensureQueryData` would have left `me` fresh, so TodayView's
    // `useMe()` must read the cache rather than firing its own GET /me.
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 30_000 } } })
    queryClient.setQueryData(queryKeys.me, MEMBER)

    // Round-1 review fix, item 9: collect any request this test did not
    // expect (rather than letting it surface only as a swallowed rejected
    // promise inside the mock, e.g. a query error state nothing here
    // asserts on) and check the list is empty at the end.
    const unexpectedPaths: string[] = []
    const todayDeferred = deferred<unknown>()
    const sourcesDeferred = deferred<unknown>()
    const feedsDeferred = deferred<unknown>()
    apiFetchMock.mockImplementation((path: string) => {
      if (path === '/today') return todayDeferred.promise
      if (path === '/today/sources') return sourcesDeferred.promise
      if (path === '/today/feeds') return feedsDeferred.promise
      // Slice 016b: TodayView's own Tasks-panel read (`useTasks`) — not
      // part of the router guard's prefetch join this test exercises, so
      // it resolves immediately rather than joining the `unexpectedPaths`
      // tracking below (which is specifically about the three prefetched
      // endpoints never double-firing).
      if (path === '/tasks?scope=mine') return Promise.resolve({ tasks: [], generated_at: '2026-09-08T00:00:00.000Z', truncated: false })
      unexpectedPaths.push(path)
      return Promise.reject(new Error(`unexpected path ${path}`))
    })

    // The router guard's own call, made before the Today route chunk (and
    // therefore this component) has necessarily finished importing.
    prefetchTodayData(queryClient, MEMBER)
    await flushPromises()
    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today')).toHaveLength(1)

    // TodayView mounts while all three requests are still unresolved —
    // `refetchOnMount: 'always'` must join the in-flight fetch, not start a
    // second one (only a prefetch that had already SETTLED before mount
    // would legitimately be refetched again).
    const wrapper = await mountTodayView(queryClient)

    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today')).toHaveLength(1)
    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today/sources')).toHaveLength(1)
    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today/feeds')).toHaveLength(1)

    todayDeferred.resolve({
      generated_at: '2026-09-08T00:00:00.000Z',
      items: [],
      truncated: false,
      sources: { status: 'complete', issues: [], system_feed_issues: [] },
    })
    sourcesDeferred.resolve({ limit: 10, sources: [] })
    feedsDeferred.resolve({ feeds: [] })
    await flushPromises()

    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today')).toHaveLength(1)
    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today/sources')).toHaveLength(1)
    expect(apiFetchMock.mock.calls.filter(([p]) => p === '/today/feeds')).toHaveLength(1)
    expect(unexpectedPaths).toEqual([])
    wrapper.unmount()
  })

  afterEach(() => {
    vi.restoreAllMocks()
  })
})
