import { flushPromises, mount } from '@vue/test-utils'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { computed, ref } from 'vue'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { MeResponse, SavedListMetadata } from '../api/types'
import ListsView from './ListsView.vue'

const mocks = vi.hoisted(() => ({
  useMe: vi.fn(),
  useSavedLists: vi.fn(),
  useSavedListCountScheduler: vi.fn(),
}))

vi.mock('../api/queries', () => ({
  useMe: mocks.useMe,
  useSavedLists: mocks.useSavedLists,
}))
vi.mock('../api/savedListCounts', () => ({
  useSavedListCountScheduler: mocks.useSavedListCountScheduler,
}))

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ACTOR_ID = '22222222-2222-2222-2222-222222222222'
const cleanups: Array<() => void> = []

function me(): MeResponse {
  return {
    user: { id: ACTOR_ID, email: 'agent@example.test', display_name: 'Agent' },
    organization: { workspace_mode: 'operational', workspace_revision: '1', id: ORG_ID, name: 'Example Realty', role: 'member' },
    platform_admin: false,
  }
}

function list(index: number): SavedListMetadata {
  return {
    id: `00000000-0000-4000-8000-${String(index).padStart(12, '0')}`,
    name: `List ${index}`,
    scope: 'personal',
    revision: 1,
    created_at: '2026-09-06T00:00:00.000Z',
    updated_at: '2026-09-06T00:00:00.000Z',
    can_edit: true,
    can_delete: true,
  }
}

beforeEach(() => {
  const data = ref({ lists: Array.from({ length: 25 }, (_, index) => list(index + 1)) })
  const refetch = vi.fn(async () => ({ data: data.value, isError: false }))
  const schedulerRefresh = vi.fn()
  mocks.useMe.mockReturnValue({ data: ref(me()) })
  mocks.useSavedLists.mockReturnValue({
    data,
    isFetching: ref(false),
    isError: ref(false),
    error: ref(undefined),
    isPending: ref(false),
    refetch,
  })
  mocks.useSavedListCountScheduler.mockReturnValue({
    isRefreshing: computed(() => false),
    refresh: schedulerRefresh,
    retry: vi.fn(),
    stateFor: () => ({ kind: 'ready', count: 1, truncated: false }),
  })
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  vi.clearAllMocks()
  document.body.innerHTML = ''
})

describe('ListsView count recovery', () => {
  it('routes a real visible-window focus through metadata refetch then the bounded scheduler', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/lists', component: ListsView },
        { path: '/lists/:savedListId', component: { template: '<p>List detail</p>' } },
        { path: '/people', component: { template: '<p>People</p>' } },
      ],
    })
    await router.push('/lists')
    await router.isReady()
    const wrapper = mount(ListsView, {
      global: { plugins: [router, [PrimeVue, { unstyled: true }]] },
      attachTo: document.body,
    })
    cleanups.push(() => wrapper.unmount())
    await flushPromises()

    window.dispatchEvent(new Event('focus'))
    await flushPromises()

    const listsQuery = mocks.useSavedLists.mock.results[0]?.value
    const scheduler = mocks.useSavedListCountScheduler.mock.results[0]?.value
    expect(listsQuery.refetch).toHaveBeenCalledTimes(1)
    expect(scheduler.refresh).toHaveBeenCalledTimes(1)
  })

  // Slice 011e e2 review round 1, F1: a count 422 invalid_tag must reach
  // "Invalid definition" with no Retry control, exactly like
  // invalid_stage/invalid_assignee -- never "Unavailable" with Retry (the
  // unrecognized-code fallback).
  it('renders Invalid definition with no Retry for a count state of invalid_tag', async () => {
    mocks.useSavedListCountScheduler.mockReturnValue({
      isRefreshing: computed(() => false),
      refresh: vi.fn(),
      retry: vi.fn(),
      stateFor: (item: SavedListMetadata) =>
        item.id === list(1).id
          ? { kind: 'invalid', error: 'invalid_tag' }
          : { kind: 'ready', count: 1, truncated: false },
    })
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/lists', component: ListsView },
        { path: '/lists/:savedListId', component: { template: '<p>List detail</p>' } },
        { path: '/people', component: { template: '<p>People</p>' } },
      ],
    })
    await router.push('/lists')
    await router.isReady()
    const wrapper = mount(ListsView, {
      global: { plugins: [router, [PrimeVue, { unstyled: true }]] },
      attachTo: document.body,
    })
    cleanups.push(() => wrapper.unmount())
    await flushPromises()

    const row = wrapper.findAll('div').find((div) => div.text().includes('List 1'))!
    expect(row.text()).toContain('Invalid definition')
    expect(row.text()).not.toContain('Unavailable')
    expect(row.findAll('button').some((button) => button.text() === 'Retry')).toBe(false)
  })
})
