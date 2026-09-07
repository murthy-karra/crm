import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type { MeResponse, MemberTodayFeedsResponse, PersonSummary, TodayItem, TodayResponse, TodaySourcesResponse } from '../api/types'
import TodayView from './TodayView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const LIST_A = '22222222-2222-2222-2222-222222222222'
const LIST_B = '33333333-3333-3333-3333-333333333333'
const cleanups: Array<() => void> = []

function me(): MeResponse {
  return {
    user: { id: 'alice', email: 'alice@example.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Example Realty', role: 'member' },
    platform_admin: false,
  }
}

function person(id: string, name: string): PersonSummary {
  return {
    id,
    first_name: name,
    last_name: null,
    display_name: name,
    stage: { id: 'stage-1', name: 'Lead' },
    assigned_user: null,
    primary_email: null,
    primary_phone: null,
    inquiry_count: 0,
    last_inquiry_at: null,
    created_at: '2026-09-06T09:00:00.000Z',
  }
}

function listItem(
  id: string,
  name: string,
  lastContactAt: string | null,
  reasons: TodayItem['reasons'],
): TodayItem {
  return {
    person: person(id, name),
    priority: 'list',
    recommended_action: 'review_person',
    reasons,
    waiting_since: null,
    latest_inquiry: null,
    last_contact_attempt: lastContactAt === null
      ? null
      : { id: `attempt-${id}`, channel: 'call', outcome: 'reached', occurred_at: lastContactAt },
  }
}

function completeToday(items: TodayItem[] = []): TodayResponse {
  return {
    generated_at: new Date().toISOString(),
    items,
    truncated: false,
    sources: { status: 'complete', issues: [], system_feed_issues: [] },
  }
}

function defaultFeeds(): MemberTodayFeedsResponse {
  return {
    feeds: [
      { feed_key: 'unanswered_inquiry', enabled: true, is_default: true, description: ['Awaiting a response'] },
      { feed_key: 'client_replied', enabled: true, is_default: true, description: ['Client replied, unanswered'] },
      { feed_key: 'call_outcome_needed', enabled: true, is_default: true, description: ['A call of mine needs an outcome'] },
    ],
  }
}

function sourceConfig(
  enabled = true,
  filterError: TodaySourcesResponse['sources'][number]['filter_error'] = null,
  extraSources: TodaySourcesResponse['sources'] = [],
): TodaySourcesResponse {
  return {
    limit: 5,
    sources: enabled
      ? [{ list_id: LIST_A, name: 'Repair this queue', scope: 'personal', revision: 1, filter_error: filterError }, ...extraSources]
      : [],
  }
}

interface StubOptions {
  today?: () => TodayResponse
  sources?: () => TodaySourcesResponse | ApiError
  disable?: () => { enabled: false; changed: boolean } | ApiError
  feeds?: () => MemberTodayFeedsResponse | ApiError
}

function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === '/me') return me()
    if (path === '/today') return options.today?.() ?? completeToday()
    if (path === '/today/sources') {
      const response = options.sources?.() ?? sourceConfig(false)
      if (response instanceof ApiError) throw response
      return response
    }
    if (path === '/today/feeds') {
      const response = options.feeds?.() ?? defaultFeeds()
      if (response instanceof ApiError) throw response
      return response
    }
    if (path === `/today/sources/${LIST_A}` && init?.method === 'DELETE') {
      const response = options.disable?.() ?? { enabled: false, changed: true }
      if (response instanceof ApiError) throw response
      return response
    }
    throw new Error(`unexpected ${init?.method ?? 'GET'} ${path}`)
  })
}

async function mountView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: TodayView },
      { path: '/lists', component: { template: '<p>Lists</p>' } },
      { path: '/lists/:id', component: { template: '<p>List</p>' } },
      { path: '/people/:id', component: { template: '<p>Person</p>' } },
    ],
  })
  await router.push('/')
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(TodayView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanups.push(() => {
    wrapper.unmount()
    queryClient.clear()
  })
  await flushPromises()
  return { wrapper, router }
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

describe('TodayView source panel route state', () => {
  it('opens the manager directly from the cap-alert route state', async () => {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/', component: TodayView }],
    })
    await router.push('/?sources=1')
    await router.isReady()
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const wrapper = mount(TodayView, {
      global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    })
    cleanups.push(() => { wrapper.unmount(); queryClient.clear() })
    await flushPromises()
    expect(wrapper.find('[aria-label="Today sources"]').exists()).toBe(true)
  })
})

describe('TodayView source items', () => {
  it('uses actual list-only contact state, preserves null inquiry fields, and links every list reason', async () => {
    const contactAt = new Date(Date.now() - 5 * 60_000).toISOString()
    stub({
      today: () => completeToday([
        listItem('never', 'Never Contacted', null, [
          { code: 'list_member', list_id: LIST_A, name: 'First queue' },
          { code: 'list_member', list_id: LIST_B, name: 'Second queue' },
        ]),
        listItem('contacted', 'Contacted Already', contactAt, [
          { code: 'list_member', list_id: LIST_A, name: 'First queue' },
        ]),
      ]),
    })
    const { wrapper } = await mountView()

    const rows = wrapper.findAll('tbody tr')
    expect(rows).toHaveLength(2)
    expect(rows[0]!.findAll('td')[2]!.text()).toBe('From your lists')
    expect(rows[0]!.findAll('td')[3]!.text()).toBe('Never contacted')
    expect(rows[1]!.findAll('td')[3]!.text()).not.toContain('Never contacted')
    expect(rows[1]!.findAll('td')[3]!.get('span').attributes('title')).toBeTruthy()
    expect(rows[0]!.findAll('td')[4]!.text()).toBe('Review person')
    expect(rows[0]!.findAll('td')[1]!.findAll('a').map((link) => ({
      text: link.text(), href: link.attributes('href'),
    }))).toEqual([
      { text: 'First queue', href: `/lists/${LIST_A}` },
      { text: 'Second queue', href: `/lists/${LIST_B}` },
    ])
  })
})

describe('TodayView incomplete sources', () => {
  it('does not make the complete-empty claim for partial work and exposes issue recovery controls', async () => {
    stub({
      today: () => ({
        ...completeToday(),
        sources: {
          status: 'partial',
          issues: [{ list_id: LIST_A, name: 'Slow personal queue', revision: 4, error: 'unavailable' }],
          system_feed_issues: [],
        },
      }),
    })
    const { wrapper } = await mountView()

    expect(wrapper.text()).toContain('No available work to show')
    expect(wrapper.text()).not.toContain("You're all caught up")
    const notice = wrapper.get('[role="status"]')
    expect(notice.text()).toContain('Some Today rules or sources could not load')
    expect(notice.get(`a[href="/lists/${LIST_A}"]`).text()).toBe('Slow personal queue')
    await notice.get('button').trigger('click')
    await flushPromises()
    expect(apiFetchMock.mock.calls.filter(([path]) => path === '/today').length).toBeGreaterThan(1)
    await notice.get('button:nth-of-type(2)').trigger('click')
    await flushPromises()
    expect(wrapper.find('[aria-label="Today sources"]').exists()).toBe(true)
  })

  it('renders the truthful unavailable-empty state without inventing source-specific issues', async () => {
    stub({
      today: () => ({ ...completeToday(), sources: { status: 'unavailable', issues: [], system_feed_issues: [] } }),
    })
    const { wrapper } = await mountView()

    expect(wrapper.text()).toContain('No available work to show')
    expect(wrapper.text()).toContain('Some sources could not load. Retry when they are available.')
    expect(wrapper.find('[role="status"] ul').exists()).toBe(false)
  })
})

describe('TodayView source-removal reconciliation', () => {
  it('keeps keyboard focus in the manager after a removed source row disappears', async () => {
    let firstEnabled = true
    const next = { list_id: LIST_B, name: 'Next queue', scope: 'shared' as const, revision: 1, filter_error: null }
    stub({
      sources: () => firstEnabled ? sourceConfig(true, null, [next]) : { limit: 5, sources: [next] },
      disable: () => {
        firstEnabled = false
        return { enabled: false, changed: true }
      },
    })
    const { wrapper } = await mountView()
    await wrapper.findAll('button').find((button) => button.text() === 'Manage sources')!.trigger('click')
    await flushPromises()

    await wrapper.get('[aria-label="Remove Repair this queue source"]').trigger('click')
    await flushPromises()

    const nextRemove = wrapper.get('[aria-label="Remove Next queue source"]')
    expect(document.activeElement).toBe(nextRemove.element)
  })

  it('refetches after a lost remove response and reports a completed removal only when configuration confirms it', async () => {
    let enabled = true
    stub({
      sources: () => sourceConfig(enabled, 'unsupported_filter'),
      disable: () => {
        enabled = false
        return new ApiError(0, 'network_error')
      },
    })
    const { wrapper } = await mountView()
    await wrapper.findAll('button').find((button) => button.text() === 'Manage sources')!.trigger('click')
    await flushPromises()
    const panel = wrapper.get('[aria-label="Today sources"]')
    expect(panel.text()).toContain('This list definition is no longer supported.')
    await panel.get('button').trigger('click')
    await flushPromises()
    await flushPromises()

    expect(apiFetchMock.mock.calls.some(([path, init]) => path === `/today/sources/${LIST_A}` && init?.method === 'DELETE')).toBe(true)
    expect(panel.text()).toContain('Removal completed, but the response was lost.')
    expect(panel.text()).toContain('0 of 5 sources')
  })
})

describe('TodayView source manager failures', () => {
  it('does not present an unavailable source configuration as an empty manager and offers retry', async () => {
    stub({ sources: () => new ApiError(503, 'unavailable') })
    const { wrapper } = await mountView()
    await wrapper.findAll('button').find((button) => button.text() === 'Manage sources')!.trigger('click')
    await flushPromises()

    const panel = wrapper.get('[aria-label="Today sources"]')
    expect(panel.text()).toContain('Could not load your source settings.')
    expect(panel.text()).not.toContain('No saved lists are feeding Today.')
    await panel.get('button').trigger('click')
    await flushPromises()
    expect(apiFetchMock.mock.calls.filter(([path]) => path === '/today/sources').length).toBeGreaterThan(1)
  })

  it('keeps the manager and its removal control visible after an uncertain removal', async () => {
    const enabled = true
    stub({
      sources: () => sourceConfig(enabled),
      disable: () => new ApiError(0, 'network_error'),
    })
    const { wrapper } = await mountView()
    await wrapper.findAll('button').find((button) => button.text() === 'Manage sources')!.trigger('click')
    await flushPromises()
    const panel = wrapper.get('[aria-label="Today sources"]')
    await panel.get('button').trigger('click')
    await flushPromises()
    await flushPromises()

    expect(panel.text()).toContain('Could not confirm removal. Try again.')
    expect(panel.text()).toContain('Repair this queue')
    expect(panel.get('button').text()).toBe('Remove')
  })
})
