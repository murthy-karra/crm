// Slice 011d Lane W step 1: request/response stubs for the six routes
// declared in docs/specs/SLICE_011d.md §6, before Lane B's routes exist in
// the integration branch (SLICE_011d_IMPL.md "Lane W codes against §6's
// contracts and may stub the API in tests"). Mirrors api/todaySources.test.ts's
// structure.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { defineComponent, effectScope, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from './client'
import {
  queryKeys,
  usePreviewTodayFeedMutation,
  useRevertTodayFeedMutation,
  useSetTodayFeedEnabledMutation,
  useTodayFeeds,
  useTodayFeedsAdmin,
  useUpdateTodayFeedMutation,
} from './queries'
import type {
  Feed,
  MeResponse,
  MemberFeed,
  MemberTodayFeedsResponse,
  PreviewTodayFeedResponse,
  TodayFeedMutationResponse,
  TodayFeedsResponse,
} from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ACTOR_A = '22222222-2222-2222-2222-222222222222'

function me(): MeResponse {
  return {
    user: { id: ACTOR_A, email: 'alice@example.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Example Realty', role: 'admin' },
    platform_admin: false,
  }
}

function feed(overrides: Partial<Feed> = {}): Feed {
  return {
    feed_key: 'unanswered_inquiry',
    enabled: true,
    revision: 1,
    is_default: true,
    filter: { version: 1, clauses: [{ kind: 'assigned_to', assignees: ['me'] }, { kind: 'awaiting_response', value: true }] },
    fresh_within_hours: 24,
    description: ['Assignee: Me', 'Awaiting a response'],
    filter_error: null,
    updated_at: '2026-09-01T00:00:00Z',
    updated_by: null,
    default: {
      filter: { version: 1, clauses: [{ kind: 'assigned_to', assignees: ['me'] }, { kind: 'awaiting_response', value: true }] },
      fresh_within_hours: 24,
    },
    ...overrides,
  }
}

function memberFeed(overrides: Partial<MemberFeed> = {}): MemberFeed {
  return { feed_key: 'unanswered_inquiry', enabled: true, is_default: true, description: ['Awaiting a response'], ...overrides }
}

const cleanups: Array<() => void> = []

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

async function mountQuery<T>(run: () => T, providedQueryClient?: QueryClient) {
  const queryClient = providedQueryClient ?? new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: Infinity } } })
  let result!: T
  const Harness = defineComponent({
    setup() {
      result = run()
      return () => null
    },
  })
  const wrapper = mount(Harness, { global: { plugins: [[VueQueryPlugin, { queryClient }]] }, attachTo: document.body })
  cleanups.push(() => {
    wrapper.unmount()
    if (!providedQueryClient) queryClient.clear()
  })
  await flushPromises()
  return { result, queryClient }
}

describe('GET /api/today/feeds (member)', () => {
  it('fetches the member-visible effective feed summary', async () => {
    const response: MemberTodayFeedsResponse = {
      feeds: [
        memberFeed({ feed_key: 'unanswered_inquiry' }),
        memberFeed({ feed_key: 'client_replied', is_default: false, description: ['Client replied, unanswered'] }),
        memberFeed({ feed_key: 'call_outcome_needed', enabled: false, description: ['A call of mine needs an outcome'] }),
      ],
    }
    apiFetchMock.mockResolvedValueOnce(response)
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const { result } = await mountQuery(() => useTodayFeeds(orgId, actorId))

    expect(apiFetchMock).toHaveBeenCalledWith('/today/feeds', expect.objectContaining({ signal: expect.anything() }))
    expect(result.data.value).toEqual(response)
  })

  it('keys the member summary under the existing Today invalidation prefix', () => {
    expect(queryKeys.todayFeeds(ORG_ID, ACTOR_A).slice(0, 3)).toEqual(queryKeys.today(ORG_ID))
  })
})

describe('GET /api/organization/today-feeds (admin)', () => {
  it('fetches the admin feed list in fixed key order', async () => {
    const response: TodayFeedsResponse = {
      feeds: [feed({ feed_key: 'unanswered_inquiry' }), feed({ feed_key: 'client_replied' }), feed({ feed_key: 'call_outcome_needed', fresh_within_hours: null })],
    }
    apiFetchMock.mockResolvedValueOnce(response)
    const orgId = ref(ORG_ID)
    const { result } = await mountQuery(() => useTodayFeedsAdmin(orgId))

    expect(apiFetchMock).toHaveBeenCalledWith('/organization/today-feeds', expect.objectContaining({ signal: expect.anything() }))
    expect(result.data.value?.feeds.map((f) => f.feed_key)).toEqual(['unanswered_inquiry', 'client_replied', 'call_outcome_needed'])
  })

  it('uses the spec-literal admin key, separate from the actor-scoped member key', () => {
    expect(queryKeys.todayFeedsAdmin(ORG_ID)).toEqual(['org', ORG_ID, 'today-feeds-admin'])
  })
})

describe('PUT /api/organization/today-feeds/{feed_key} (update)', () => {
  it('sends the loaded revision and candidate definition, then invalidates admin, member and Today caches', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const response: TodayFeedMutationResponse = { feed: feed({ revision: 2, is_default: false }), changed: true }
    apiFetchMock.mockResolvedValueOnce(response)
    const scope = effectScope()
    const mutation = scope.run(() => useUpdateTodayFeedMutation(orgId, actorId, queryClient))!

    const body = {
      expected_revision: 1,
      filter: { version: 1 as const, clauses: [{ kind: 'assigned_to' as const, assignees: ['me' as const] }, { kind: 'awaiting_response' as const, value: true }] },
      fresh_within_hours: 24,
    }
    const result = await mutation.mutateAsync({ feedKey: 'unanswered_inquiry', body })

    expect(apiFetchMock).toHaveBeenCalledWith('/organization/today-feeds/unanswered_inquiry', {
      method: 'PUT',
      body: JSON.stringify(body),
    })
    expect(result).toEqual(response)
    const invalidatedKeys = new Set(invalidate.mock.calls.map((call) => JSON.stringify((call[0] as { queryKey: unknown }).queryKey)))
    expect(invalidatedKeys).toEqual(new Set([
      JSON.stringify(queryKeys.todayFeedsAdmin(ORG_ID)),
      JSON.stringify(queryKeys.today(ORG_ID)),
      JSON.stringify(queryKeys.todayFeeds(ORG_ID, ACTOR_A)),
    ]))
    scope.stop()
  })

  it('still refetches the admin list on a definite 409 conflict (uncertain-mutation reconciliation covers every settled outcome)', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false }, queries: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    // An active admin-list observer, matching TodayFeedsView.vue's own
    // mount, so the mutation's onSettled `type: 'active'` refetch has a
    // query to actually refetch.
    apiFetchMock.mockResolvedValueOnce({ feeds: [feed({ revision: 1 })] } satisfies TodayFeedsResponse)
    await mountQuery(() => useTodayFeedsAdmin(orgId), queryClient)

    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    apiFetchMock.mockRejectedValueOnce(new ApiError(409, 'today_feed_conflict'))
    apiFetchMock.mockResolvedValueOnce({ feeds: [feed({ revision: 2 })] } satisfies TodayFeedsResponse)
    const scope = effectScope()
    const mutation = scope.run(() => useUpdateTodayFeedMutation(orgId, actorId, queryClient))!

    await expect(mutation.mutateAsync({
      feedKey: 'unanswered_inquiry',
      body: { expected_revision: 1, filter: { version: 1, clauses: [] }, fresh_within_hours: 24 },
    })).rejects.toMatchObject({ status: 409, code: 'today_feed_conflict' })
    await flushPromises()

    expect(invalidate).toHaveBeenCalled()
    // The reconciliation refetch reads the true current row for the caller's
    // own 409-reload flow (TodayFeedsView.vue), on top of the invalidation.
    expect(apiFetchMock).toHaveBeenCalledWith('/organization/today-feeds', expect.anything())
    scope.stop()
  })

  it('does not invalidate or refetch for a replacement same-Organization actor', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    let resolveFetch!: (value: TodayFeedMutationResponse) => void
    apiFetchMock.mockReturnValueOnce(new Promise((resolve) => { resolveFetch = resolve }))
    const scope = effectScope()
    const mutation = scope.run(() => useUpdateTodayFeedMutation(orgId, actorId, queryClient))!

    const pending = mutation.mutateAsync({
      feedKey: 'unanswered_inquiry',
      body: { expected_revision: 1, filter: { version: 1, clauses: [] }, fresh_within_hours: 24 },
    })
    await Promise.resolve()
    actorId.value = 'a-different-actor'
    queryClient.setQueryData(queryKeys.me, { ...me(), user: { id: 'a-different-actor', email: 'bob@example.test', display_name: 'Bob' } })
    resolveFetch({ feed: feed({ revision: 2 }), changed: true })
    await pending

    expect(invalidate).not.toHaveBeenCalled()
    scope.stop()
  })
})

describe('POST /api/organization/today-feeds/{feed_key}/revert', () => {
  it('sends the loaded revision and invalidates the same three cache branches', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const response: TodayFeedMutationResponse = { feed: feed({ revision: 3 }), changed: true }
    apiFetchMock.mockResolvedValueOnce(response)
    const scope = effectScope()
    const mutation = scope.run(() => useRevertTodayFeedMutation(orgId, actorId, queryClient))!

    await mutation.mutateAsync({ feedKey: 'client_replied', body: { expected_revision: 2 } })

    expect(apiFetchMock).toHaveBeenCalledWith('/organization/today-feeds/client_replied/revert', {
      method: 'POST',
      body: JSON.stringify({ expected_revision: 2 }),
    })
    expect(invalidate).toHaveBeenCalled()
    scope.stop()
  })
})

describe('PUT /api/organization/today-feeds/{feed_key}/enabled', () => {
  it('turns a feed off with the loaded revision', async () => {
    const orgId = ref(ORG_ID)
    const actorId = ref(ACTOR_A)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    const response: TodayFeedMutationResponse = { feed: feed({ enabled: false, revision: 2 }), changed: true }
    apiFetchMock.mockResolvedValueOnce(response)
    const scope = effectScope()
    const mutation = scope.run(() => useSetTodayFeedEnabledMutation(orgId, actorId, queryClient))!

    const result = await mutation.mutateAsync({ feedKey: 'unanswered_inquiry', body: { expected_revision: 1, enabled: false } })

    expect(apiFetchMock).toHaveBeenCalledWith('/organization/today-feeds/unanswered_inquiry/enabled', {
      method: 'PUT',
      body: JSON.stringify({ expected_revision: 1, enabled: false }),
    })
    expect(result.feed.enabled).toBe(false)
    scope.stop()
  })
})

describe('POST /api/organization/today-feeds/{feed_key}/preview', () => {
  it('posts the candidate definition and subject without touching the cache', async () => {
    const orgId = ref(ORG_ID)
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    const response: PreviewTodayFeedResponse = {
      subject: { id: ACTOR_A, display_name: 'Alice' },
      items: [],
      truncated: false,
      description: ['Awaiting a response'],
    }
    apiFetchMock.mockResolvedValueOnce(response)
    const scope = effectScope()
    const mutation = scope.run(() => usePreviewTodayFeedMutation(orgId, queryClient))!

    const body = {
      filter: { version: 1 as const, clauses: [{ kind: 'awaiting_response' as const, value: true }] },
      fresh_within_hours: 24,
      subject_user_id: ACTOR_A,
    }
    const result = await mutation.mutateAsync({ feedKey: 'unanswered_inquiry', body })

    expect(apiFetchMock).toHaveBeenCalledWith('/organization/today-feeds/unanswered_inquiry/preview', {
      method: 'POST',
      body: JSON.stringify(body),
    })
    expect(result).toEqual(response)
    expect(invalidate).not.toHaveBeenCalled()
    scope.stop()
  })
})
