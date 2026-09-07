import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter, RouterLink, RouterView } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type {
  EnableTodaySourceRequest,
  FilterClause,
  MeResponse,
  SavedListDetailResponse,
  TodaySourceChange,
  TodaySourcesResponse,
} from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import PeopleView from './PeopleView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ALICE_ID = '22222222-2222-2222-2222-222222222222'
const STAGE_ID = '33333333-3333-3333-3333-333333333333'
const LIST_ID = '44444444-4444-4444-4444-444444444444'
const cleanups: Array<() => void> = []

function me(): MeResponse {
  return {
    user: { id: ALICE_ID, email: 'alice@example.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Example Realty', role: 'member' },
    platform_admin: false,
  }
}

function detail(
  filter: FilterClause[] = [],
  revision = 1,
  filterError: SavedListDetailResponse['filter_error'] = null,
): SavedListDetailResponse {
  return {
    list: {
      id: LIST_ID,
      name: 'My queue',
      scope: 'personal',
      revision,
      created_at: '2026-09-06T00:00:00.000Z',
      updated_at: '2026-09-06T00:00:00.000Z',
      can_edit: true,
      can_delete: true,
    },
    filter: { version: 1, clauses: filter },
    description: ['All people'],
    filter_error: filterError,
  }
}

function unsupportedDetail(): SavedListDetailResponse {
  return {
    ...detail(),
    filter: null,
    description: [],
    filter_error: 'unsupported_filter',
  }
}

function sources(enabled = false, filterError: TodaySourcesResponse['sources'][number]['filter_error'] = null): TodaySourcesResponse {
  return {
    limit: 5,
    sources: enabled
      ? [{ list_id: LIST_ID, name: 'My queue', scope: 'personal', revision: 1, filter_error: filterError }]
      : [],
  }
}

interface StubOptions {
  savedList?: () => SavedListDetailResponse
  todaySources?: () => TodaySourcesResponse | ApiError
  enable?: (body: EnableTodaySourceRequest) => TodaySourceChange | ApiError
  disable?: () => TodaySourceChange | ApiError
}

function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === '/me') return me()
    if (path === '/stages') return { stages: [{ id: STAGE_ID, name: 'Lead', position: 1 }] }
    if (path === '/organization/members') return { members: [] }
    if (path === '/inquiry-sources') return { sources: [], truncated: false }
    if (path.startsWith(`/saved-lists/${LIST_ID}/count?`)) {
      return { list_id: LIST_ID, revision: 1, count: 0, truncated: false }
    }
    if (path === `/saved-lists/${LIST_ID}`) return options.savedList?.() ?? detail()
    if (path === '/today/sources') {
      const response = options.todaySources?.() ?? sources()
      if (response instanceof ApiError) throw response
      return response
    }
    if (path === `/today/sources/${LIST_ID}` && init?.method === 'PUT') {
      const response = options.enable?.(JSON.parse(String(init.body)) as EnableTodaySourceRequest) ?? { enabled: true, changed: true }
      if (response instanceof ApiError) throw response
      return response
    }
    if (path === `/today/sources/${LIST_ID}` && init?.method === 'DELETE') {
      const response = options.disable?.() ?? { enabled: false, changed: true }
      if (response instanceof ApiError) throw response
      return response
    }
    if (path.startsWith('/people')) return { people: [], truncated: false }
    throw new Error(`unexpected ${init?.method ?? 'GET'} ${path}`)
  })
}

async function mountView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/people', component: PeopleView },
      { path: '/lists/:savedListId', component: PeopleView, props: true },
      { path: '/lists', component: { template: '<p>Lists</p>' } },
      { path: '/people/:id', component: { template: '<p>Person</p>' } },
      { path: '/intake/new', component: { template: '<p>Intake</p>' } },
    ],
  })
  await router.push(`/lists/${LIST_ID}`)
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 30_000 } } })
  queryClient.setQueryData(queryKeys.me, me())
  const wrapper = mount(RouterView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanups.push(() => {
    wrapper.unmount()
    queryClient.clear()
  })
  await flushPromises()
  return { wrapper, queryClient }
}

function edit(wrapper: VueWrapper, clauses: FilterClause[]) {
  wrapper.findComponent(FilterBar).vm.$emit('update:clauses', clauses)
}

function sourceButton(wrapper: VueWrapper, label: 'Use as a Today source' | 'Remove from Today') {
  return wrapper.findAll('button').find((button) => button.text() === label)!
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

describe('PeopleView Today source controls', () => {
  it('submits the retained revision-one baseline after dirty edits and a background revision-two update', async () => {
    let latest = detail([], 1)
    const submissions: EnableTodaySourceRequest[] = []
    stub({
      savedList: () => latest,
      enable: (body) => {
        submissions.push(body)
        return new ApiError(409, 'saved_list_conflict')
      },
    })
    const { wrapper, queryClient } = await mountView()
    edit(wrapper, [{ kind: 'has_phone', value: true }])
    await flushPromises()

    latest = detail([{ kind: 'has_email', value: true }], 2)
    queryClient.setQueryData(queryKeys.savedList(ORG_ID, ALICE_ID, LIST_ID), latest)
    await flushPromises()

    const button = sourceButton(wrapper, 'Use as a Today source')
    expect(button.attributes('disabled')).toBeUndefined()
    await button.trigger('click')
    await flushPromises()
    await flushPromises()

    expect(submissions).toEqual([{ expected_list_revision: 1 }])
    expect(wrapper.get('[role="alert"]').text()).toContain('This list changed. Reload the saved version')
  })

  it('does not enable an unsaved local repair of an invalid persisted definition', async () => {
    stub({ savedList: () => detail([{ kind: 'stage', stage_ids: ['missing-stage'] }], 1, 'invalid_stage') })
    const { wrapper } = await mountView()
    edit(wrapper, [])
    await flushPromises()

    const button = sourceButton(wrapper, 'Use as a Today source')
    expect(button.attributes('disabled')).toBeDefined()
    await button.trigger('click')
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/today/sources/${LIST_ID}` && init?.method === 'PUT')).toBe(false)
  })

  it('allows an enabled unsupported source to be removed despite its absent saved baseline', async () => {
    let enabled = true
    stub({
      savedList: unsupportedDetail,
      todaySources: () => sources(enabled, 'unsupported_filter'),
      disable: () => {
        enabled = false
        return { enabled: false, changed: true }
      },
    })
    const { wrapper } = await mountView()
    const button = sourceButton(wrapper, 'Remove from Today')
    expect(button.attributes('disabled')).toBeUndefined()
    await button.trigger('click')
    await flushPromises()
    await flushPromises()

    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/today/sources/${LIST_ID}` && init?.method === 'DELETE')).toBe(true)
  })

  it('shows the private source capacity beside a named list', async () => {
    stub({ todaySources: () => sources(true) })
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('1 of 5 Today sources in use.')
  })

  it('gates source enable while private configuration is unavailable and surfaces the cap response without a false success', async () => {
    stub({ todaySources: () => new ApiError(503, 'unavailable') })
    const unavailable = await mountView()
    expect(sourceButton(unavailable.wrapper, 'Use as a Today source').attributes('disabled')).toBeDefined()
    expect(unavailable.wrapper.text()).toContain('Could not load your Today source settings.')
    const retry = unavailable.wrapper.findAll('button').find((button) => button.text() === 'Try again')!
    await retry.trigger('click')
    await flushPromises()
    expect(apiFetchMock.mock.calls.filter(([path]) => path === '/today/sources').length).toBeGreaterThan(1)

    unavailable.wrapper.unmount()
    stub({ enable: () => new ApiError(409, 'today_source_limit_reached') })
    const capped = await mountView()
    await sourceButton(capped.wrapper, 'Use as a Today source').trigger('click')
    await flushPromises()
    const limitAlert = capped.wrapper.get('[role="alert"]')
    expect(limitAlert.text()).toContain('You have reached the five-source limit')
    expect(limitAlert.getComponent(RouterLink).props('to')).toBe('/?sources=1')
  })
})
