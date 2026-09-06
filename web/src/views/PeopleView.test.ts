import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter, RouterView } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type { FilterClause, InquirySourcesResponse, MeResponse, PeopleResponse } from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import PeopleView from './PeopleView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ALICE_ID = '22222222-2222-2222-2222-222222222222'
const STAGE_ID = '44444444-4444-4444-4444-444444444444'
const PERSON_ID = '55555555-5555-5555-5555-555555555555'
const phoneFilter: FilterClause[] = [{ kind: 'has_phone', value: true }]
const emailFilter: FilterClause[] = [{ kind: 'has_email', value: false }]
const meFilter: FilterClause[] = [{ kind: 'assigned_to', assignees: ['me'] }]

function me(orgId = ORG_ID): MeResponse {
  return {
    user: { id: ALICE_ID, email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: orgId, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}
function result(name = 'Grace Hopper', truncated = false): PeopleResponse {
  return {
    people: [{
      id: PERSON_ID, first_name: name, last_name: '', display_name: name,
      stage: { id: STAGE_ID, name: 'Lead' }, assigned_user: null,
      primary_email: null, primary_phone: null, inquiry_count: 1,
      last_inquiry_at: '2026-08-22T09:00:00.000Z', created_at: '2026-08-22T09:00:00.000Z',
    }],
    truncated,
  }
}
function serialized(clauses: FilterClause[]) {
  return JSON.stringify({ version: 1, clauses })
}
function filteredPath(clauses: FilterClause[]) {
  return `/people?filter=${encodeURIComponent(serialized(clauses))}`
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
interface StubOptions {
  people?: (filter: string | null) => PeopleResponse | Promise<PeopleResponse> | ApiError
  sources?: () => InquirySourcesResponse | Promise<InquirySourcesResponse>
}
function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string) => {
    if (path === '/me') return me()
    if (path === '/stages') return { stages: [{ id: STAGE_ID, name: 'Lead', position: 1 }] }
    if (path === '/organization/members') return { members: [] }
    if (path === '/inquiry-sources') return options.sources?.() ?? { sources: ['website', 'zillow'], truncated: false }
    if (path.startsWith('/people')) {
      const filter = new URL(path, 'http://test').searchParams.get('filter')
      const response = options.people?.(filter) ?? result()
      if (response instanceof ApiError) throw response
      return response
    }
    throw new Error(`unexpected GET ${path}`)
  })
}
const cleanups: Array<() => void> = []
async function mountView(initialPath = '/people') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/people', component: PeopleView },
      { path: '/people/:id', component: { template: '<p>Person detail</p>' } },
      { path: '/intake/new', component: { template: '<p>New lead form</p>' } },
    ],
  })
  await router.push(initialPath)
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 30_000 } } })
  // The real router seeds this cache before People mounts. Initial URL
  // parsing must happen before a cached session enables the People query.
  queryClient.setQueryData(queryKeys.me, me())
  const wrapper = mount(RouterView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanups.push(() => { wrapper.unmount(); queryClient.clear() })
  await flushPromises()
  return { wrapper, router, queryClient }
}
function peoplePaths() {
  return apiFetchMock.mock.calls.map(([path]) => path).filter((path) => path.startsWith('/people'))
}
function edit(wrapper: VueWrapper, clauses: FilterClause[]) {
  wrapper.findComponent(FilterBar).vm.$emit('update:clauses', clauses)
}
function filterState(wrapper: VueWrapper) {
  return wrapper.findComponent(FilterBar).props('clauses')
}
beforeEach(() => { apiFetchMock.mockReset() })
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup())
  document.body.innerHTML = ''
})

describe('People filters and URL navigation', () => {
  it('a shared URL with a cached session makes exactly one filtered request', async () => {
    stub()
    const { wrapper } = await mountView(filteredPath(phoneFilter))
    expect(filterState(wrapper)).toEqual(phoneFilter)
    expect(peoplePaths()).toEqual([filteredPath(phoneFilter)])
  })

  it('edits and individual removal replace the URL without duplicate requests', async () => {
    stub()
    const { wrapper, router } = await mountView('/people?context=team#list')
    const replace = vi.spyOn(router, 'replace')
    edit(wrapper, [...phoneFilter, ...emailFilter])
    await flushPromises()
    edit(wrapper, emailFilter)
    await flushPromises()
    expect(JSON.parse(router.currentRoute.value.query.filter as string).clauses).toEqual(emailFilter)
    expect(router.currentRoute.value.query.context).toBe('team')
    expect(router.currentRoute.value.hash).toBe('#list')
    expect(replace).toHaveBeenCalledTimes(2)
    expect(peoplePaths()).toEqual(['/people', filteredPath([...phoneFilter, ...emailFilter]), filteredPath(emailFilter)])
  })

  it('Clear all clears the URL and all criteria while preserving unrelated params', async () => {
    stub()
    const { wrapper, router } = await mountView(`${filteredPath([...phoneFilter, ...emailFilter])}&context=team`)
    edit(wrapper, [])
    await flushPromises()
    expect(filterState(wrapper)).toEqual([])
    expect(router.currentRoute.value.query).toEqual({ context: 'team' })
    expect(peoplePaths().at(-1)).toBe('/people')
  })

  it('immediate select then Clear all supersedes the first pending URL replacement', async () => {
    stub()
    const { wrapper, router } = await mountView()
    edit(wrapper, phoneFilter)
    edit(wrapper, [])
    await flushPromises()
    expect(router.currentRoute.value.query.filter).toBeUndefined()
    expect(filterState(wrapper)).toEqual([])
  })

  it('rapid consecutive selections preserve the latest composed filter', async () => {
    stub()
    const { wrapper, router } = await mountView()
    edit(wrapper, phoneFilter)
    edit(wrapper, [...phoneFilter, ...meFilter])
    await flushPromises()
    expect(filterState(wrapper)).toEqual([...phoneFilter, ...meFilter])
    expect(router.currentRoute.value.query.filter).toBe(serialized([...phoneFilter, ...meFilter]))
    expect(peoplePaths().at(-1)).toBe(filteredPath([...phoneFilter, ...meFilter]))
  })

  it('refetching an inactive cached filter uses its own key, not the current selection', async () => {
    stub()
    const { wrapper, queryClient } = await mountView(filteredPath(phoneFilter))
    edit(wrapper, emailFilter)
    await flushPromises()
    await queryClient.refetchQueries({ queryKey: queryKeys.people(ORG_ID, serialized(phoneFilter)), exact: true, type: 'inactive' })
    await flushPromises()
    expect(peoplePaths().at(-1)).toBe(filteredPath(phoneFilter))
    expect(filterState(wrapper)).toEqual(emailFilter)
  })

  it('navigating to plain People clears a previously applied filter', async () => {
    stub()
    const { wrapper, router } = await mountView(filteredPath(phoneFilter))
    await router.push('/people')
    await flushPromises()
    expect(filterState(wrapper)).toEqual([])
    expect(wrapper.find('[data-testid="filter-chip-has_phone"]').exists()).toBe(false)
    expect(peoplePaths().at(-1)).toBe('/people')
  })

  it('Back/Forward restores both filtered and unfiltered views', async () => {
    stub()
    const { wrapper, router } = await mountView()
    await router.push(filteredPath(phoneFilter))
    await router.push(filteredPath(emailFilter))
    await flushPromises()
    router.back()
    await flushPromises()
    expect(filterState(wrapper)).toEqual(phoneFilter)
    router.back()
    await flushPromises()
    expect(filterState(wrapper)).toEqual([])
    router.forward()
    await flushPromises()
    expect(filterState(wrapper)).toEqual(phoneFilter)
    router.forward()
    await flushPromises()
    expect(filterState(wrapper)).toEqual(emailFilter)
  })

  it('visiting a person and going Back remounts the same filtered view', async () => {
    stub()
    const { wrapper, router } = await mountView(filteredPath(meFilter))
    await router.push(`/people/${PERSON_ID}`)
    await flushPromises()
    expect(wrapper.findComponent(FilterBar).exists()).toBe(false)
    router.back()
    await flushPromises()
    expect(filterState(wrapper)).toEqual(meFilter)
    expect(router.currentRoute.value.query.filter).toBe(serialized(meFilter))
  })

  it.each(['filter', 'filter=', 'filter=a&filter=b', 'filter=not-json', `filter=${encodeURIComponent(serialized([]))}`])(
    'normalizes invalid/empty %s without losing unrelated query state', async (param) => {
      stub()
      const { wrapper, router } = await mountView(`/people?${param}&context=team#list`)
      expect(filterState(wrapper)).toEqual([])
      expect(router.currentRoute.value.query).toEqual({ context: 'team' })
      expect(router.currentRoute.value.hash).toBe('#list')
      expect(peoplePaths()).toEqual(['/people'])
      expect(wrapper.text()).not.toContain('Could not load people')
    },
  )

  it('Me stays symbolic and reload reconstructs the same filter', async () => {
    stub()
    const { wrapper, router } = await mountView()
    edit(wrapper, meFilter)
    await flushPromises()
    expect(router.currentRoute.value.query.filter).toBe(serialized(meFilter))
    expect(router.currentRoute.value.query.filter).not.toContain(ALICE_ID)
    const reloaded = await mountView(router.currentRoute.value.fullPath)
    expect(filterState(reloaded.wrapper)).toEqual(meFilter)
  })
})

describe('People result feedback and failures', () => {
  it.each([400, 422])('only URL-origin %s rejections clear criteria and refetch the plain list', async (status) => {
    stub({ people: (filter) => filter ? new ApiError(status, 'invalid_stage') : result() })
    const { wrapper, router } = await mountView(filteredPath(phoneFilter))
    expect(router.currentRoute.value.query.filter).toBeUndefined()
    expect(filterState(wrapper)).toEqual([])
    expect(wrapper.text()).toContain('Grace Hopper')
    expect(wrapper.text()).not.toContain('Could not load people')
    expect(peoplePaths()).toEqual([filteredPath(phoneFilter), '/people'])
  })

  it.each([400, 422])('a user-composed %s rejection remains user-origin after its router echo', async (status) => {
    const response = deferred<PeopleResponse>()
    stub({ people: (filter) => filter ? response.promise : result() })
    const { wrapper, router } = await mountView()
    edit(wrapper, phoneFilter)
    await flushPromises()
    response.reject(new ApiError(status, 'malformed_request'))
    await flushPromises()
    expect(filterState(wrapper)).toEqual(phoneFilter)
    expect(router.currentRoute.value.query.filter).toBe(serialized(phoneFilter))
    expect(wrapper.text()).toContain('Could not load people')
    expect(wrapper.text()).toContain('Try again')
  })

  it('503 preserves a shared filter and offers retry', async () => {
    let fail = true
    stub({ people: () => fail ? new ApiError(503, 'unavailable') : result() })
    const { wrapper, router } = await mountView(filteredPath(phoneFilter))
    expect(filterState(wrapper)).toEqual(phoneFilter)
    expect(router.currentRoute.value.query.filter).toBe(serialized(phoneFilter))
    expect(wrapper.text()).toContain('The server is temporarily unavailable')
    fail = false
    // Retry through the actual button rather than incidental toolbar order.
    const retry = wrapper.findAll('button').find((button) => button.text() === 'Try again')!
    await retry.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('Grace Hopper')
  })

  it('zero matches has filter recovery copy and no lead-creation empty action', async () => {
    stub({ people: () => ({ people: [], truncated: false }) })
    const { wrapper } = await mountView(filteredPath(phoneFilter))
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toBe('0 matches')
    expect(wrapper.text()).toContain('No people match these filters')
    expect(wrapper.text()).not.toContain('No people yet')
    expect(wrapper.text()).not.toContain('Add a lead')
  })

  it('an empty unfiltered CRM keeps the onboarding empty state', async () => {
    stub({ people: () => ({ people: [], truncated: false }) })
    const { wrapper } = await mountView()
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toBe('0 people')
    expect(wrapper.text()).toContain('No people yet')
    expect(wrapper.text()).toContain('Add a lead')
  })

  it('the capped count near the filters never claims an exact total', async () => {
    const capped = result()
    capped.people = Array.from({ length: 500 }, (_, i) => ({ ...capped.people[0]!, id: String(i) }))
    capped.truncated = true
    stub({ people: () => capped })
    const { wrapper } = await mountView(filteredPath(phoneFilter))
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toBe('500+ matches  · Showing the first 500')
  })

  it('keeps previous rows visibly updating and inert until new matches arrive', async () => {
    const response = deferred<PeopleResponse>()
    stub({ people: (filter) => filter ? response.promise : result('Previous person') })
    const { wrapper } = await mountView()
    edit(wrapper, phoneFilter)
    await flushPromises()
    expect(wrapper.text()).toContain('Previous person')
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toContain('Updating results')
    expect(wrapper.get('[aria-busy="true"]').attributes()).toHaveProperty('inert')
    response.resolve(result('Current match'))
    await flushPromises()
    expect(wrapper.text()).toContain('Current match')
    expect(wrapper.text()).not.toContain('Previous person')
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toBe('1 match')
  })

  it.each([true, false])('does not reuse a previous empty state while a new request is pending (clearing: %s)', async (clearing) => {
    const response = deferred<PeopleResponse>()
    stub({ people: (filter) => Boolean(filter) === clearing ? { people: [], truncated: false } : response.promise })
    const { wrapper } = await mountView(clearing ? filteredPath(phoneFilter) : '/people')
    edit(wrapper, clearing ? [] : phoneFilter)
    await flushPromises()
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toBe('Updating results…')
    expect(wrapper.text()).not.toContain('No people yet')
    expect(wrapper.text()).not.toContain('No people match these filters')
    expect(wrapper.text()).not.toContain('Add a lead')
    response.resolve(result('Current result'))
    await flushPromises()
    expect(wrapper.text()).toContain('Current result')
  })

  it.each(['data', 'error'])('a late old response (%s) cannot replace or clear a newer filter', async (outcome) => {
    const old = deferred<PeopleResponse>()
    stub({ people: (filter) => filter === serialized(phoneFilter) ? old.promise : result('Newer result') })
    const { wrapper, router } = await mountView(filteredPath(phoneFilter))
    const oldSignal = apiFetchMock.mock.calls.find(([path]) => path === filteredPath(phoneFilter))?.[1]?.signal
    await router.push(filteredPath(emailFilter))
    await flushPromises()
    expect(oldSignal?.aborted).toBe(true)
    if (outcome === 'data') old.resolve(result('Stale person'))
    else old.reject(new ApiError(422, 'invalid_stage'))
    await flushPromises()
    expect(filterState(wrapper)).toEqual(emailFilter)
    expect(router.currentRoute.value.query.filter).toBe(serialized(emailFilter))
    expect(wrapper.text()).toContain('Newer result')
    expect(wrapper.text()).not.toContain('Stale person')
  })

  it('does not retain previous rows across an Organization change', async () => {
    const pending = deferred<PeopleResponse>()
    let changedOrg = false
    stub({ people: () => changedOrg ? pending.promise : result('First org person') })
    const { wrapper, queryClient } = await mountView()
    changedOrg = true
    queryClient.setQueryData(queryKeys.me, me('99999999-9999-9999-9999-999999999999'))
    await flushPromises()
    expect(wrapper.text()).not.toContain('First org person')
    expect(wrapper.get('[data-testid="people-result-count"]').text()).toBe('Loading people…')
    pending.resolve(result('Second org person'))
    await flushPromises()
  })

  it('picker loading, error, retry, and truncation are forwarded distinctly', async () => {
    const response = deferred<InquirySourcesResponse>()
    let retried = false
    stub({ sources: () => retried ? { sources: ['referral'], truncated: true } : response.promise })
    const { wrapper } = await mountView()
    expect(wrapper.findComponent(FilterBar).props('sourcesPending')).toBe(true)
    response.reject(new ApiError(503, 'unavailable'))
    await flushPromises()
    expect(wrapper.findComponent(FilterBar).props('sourcesError')).toBe(true)
    retried = true
    wrapper.findComponent(FilterBar).vm.$emit('retry-options', 'source')
    await flushPromises()
    expect(wrapper.findComponent(FilterBar).props('sources')).toEqual(['referral'])
    expect(wrapper.findComponent(FilterBar).props('sourcesTruncated')).toBe(true)
    expect(wrapper.findComponent(FilterBar).props('sourcesError')).toBe(false)
  })
})
