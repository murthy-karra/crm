/* eslint-disable vue/one-component-per-file -- small hook harnesses keep each cache case isolated. */
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { defineComponent, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { queryKeys, useSavedList, useSavedLists } from './queries'
import type { MeResponse, SavedListDetailResponse, SavedListMetadata, SavedListsResponse } from './types'

vi.mock('./client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('./client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ACTOR_ID = '22222222-2222-2222-2222-222222222222'
const LIST_ID = '33333333-3333-3333-3333-333333333333'
const cleanups: Array<() => void> = []

function me(): MeResponse {
  return {
    user: { id: ACTOR_ID, email: 'agent@example.test', display_name: 'Agent' },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: ORG_ID, name: 'Example', role: 'member' },
    platform_admin: false,
  }
}

function metadata(revision = 1): SavedListMetadata {
  return {
    id: LIST_ID,
    name: 'Current list',
    scope: 'personal',
    revision,
    created_at: '2026-09-06T00:00:00.000Z',
    updated_at: '2026-09-06T00:00:00.000Z',
    can_edit: true,
    can_delete: true,
  }
}

function detail(revision = 1): SavedListDetailResponse {
  return {
    list: metadata(revision),
    filter: { version: 1, clauses: [] },
    sort: null,
    description: ['All people'],
    filter_error: null,
  }
}

function deferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((yes) => { resolve = yes })
  return { promise, resolve }
}

function mountHooks(options: { detail?: boolean } = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  queryClient.setQueryData(queryKeys.me, me())
  const orgId = ref(ORG_ID)
  const actorId = ref(ACTOR_ID)
  const Harness = defineComponent({
    setup() {
      useSavedLists(orgId, actorId)
      if (options.detail) useSavedList(orgId, actorId, ref(LIST_ID))
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
  return { queryClient }
}

beforeEach(() => apiFetchMock.mockReset())
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

describe('useSavedLists cache reconciliation', () => {
  it('evicts removed detail/count data even when the index is freshly mounted with cached metadata', async () => {
    const empty: SavedListsResponse = { lists: [] }
    apiFetchMock.mockResolvedValue({ lists: [] })
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    queryClient.setQueryData(queryKeys.savedLists(ORG_ID, ACTOR_ID), empty)
    queryClient.setQueryData(queryKeys.savedList(ORG_ID, ACTOR_ID, LIST_ID), detail())
    queryClient.setQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, LIST_ID, 1), {
      list_id: LIST_ID, revision: 1, count: 3, truncated: false,
    })

    const Harness = defineComponent({
      setup() {
        useSavedLists(ref(ORG_ID), ref(ACTOR_ID))
        return () => null
      },
    })
    const wrapper = mount(Harness, {
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      attachTo: document.body,
    })
    cleanups.push(() => { wrapper.unmount(); queryClient.clear() })
    await flushPromises()
    await flushPromises()

    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, ACTOR_ID, LIST_ID))).toBeUndefined()
    expect(queryClient.getQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, LIST_ID, 1))).toBeUndefined()
  })

  it('cancels an obsolete detail request before a late ignored-Abort response can recreate it', async () => {
    const index = deferred<SavedListsResponse>()
    const oldDetail = deferred<SavedListDetailResponse>()
    let detailSignal: AbortSignal | null | undefined
    const unexpectedPaths: unknown[] = []
    apiFetchMock.mockImplementation((path, init) => {
      if (path === '/saved-lists') return index.promise
      if (path === `/saved-lists/${LIST_ID}`) {
        detailSignal = init?.signal
        return oldDetail.promise
      }
      unexpectedPaths.push(path)
      return Promise.resolve({ lists: [] })
    })
    const { queryClient } = mountHooks({ detail: true })
    await flushPromises()
    index.resolve({ lists: [] })
    await flushPromises()
    await flushPromises()

    expect(detailSignal?.aborted).toBe(true)
    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, ACTOR_ID, LIST_ID))).toBeUndefined()
    oldDetail.resolve(detail())
    await flushPromises()

    expect(unexpectedPaths).toEqual([])
    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, ACTOR_ID, LIST_ID))).toBeUndefined()
  })

  it('removes superseded revision counts and stale detail data on a cached metadata refresh', async () => {
    apiFetchMock.mockResolvedValue({ lists: [metadata(2)] })
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    queryClient.setQueryData(queryKeys.me, me())
    queryClient.setQueryData(queryKeys.savedLists(ORG_ID, ACTOR_ID), { lists: [metadata(2)] })
    queryClient.setQueryData(queryKeys.savedList(ORG_ID, ACTOR_ID, LIST_ID), detail(1))
    queryClient.setQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, LIST_ID, 1), {
      list_id: LIST_ID, revision: 1, count: 3, truncated: false,
    })

    const Harness = defineComponent({
      setup() {
        useSavedLists(ref(ORG_ID), ref(ACTOR_ID))
        return () => null
      },
    })
    const wrapper = mount(Harness, {
      global: { plugins: [[VueQueryPlugin, { queryClient }]] },
      attachTo: document.body,
    })
    cleanups.push(() => { wrapper.unmount(); queryClient.clear() })
    await flushPromises()
    await flushPromises()

    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, ACTOR_ID, LIST_ID))).toBeUndefined()
    expect(queryClient.getQueryData(queryKeys.savedListCount(ORG_ID, ACTOR_ID, LIST_ID, 1))).toBeUndefined()
  })
})
