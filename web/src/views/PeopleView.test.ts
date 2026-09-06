import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter, RouterView } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type {
  CreateSavedListRequest,
  CreateSavedListResponse,
  FilterClause,
  InquirySourcesResponse,
  MeResponse,
  PeopleResponse,
  PersonDetailResponse,
  SavedListCountResponse,
  SavedListDetailResponse,
  UpdateSavedListRequest,
  UpdateSavedListResponse,
} from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import SavedListDialog from '../components/SavedListDialog.vue'
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
const SAVED_LIST_A = '66666666-6666-4666-8666-666666666666'
const SAVED_LIST_B = '77777777-7777-4777-8777-777777777777'
const phoneFilter: FilterClause[] = [{ kind: 'has_phone', value: true }]
const emailFilter: FilterClause[] = [{ kind: 'has_email', value: false }]
const meFilter: FilterClause[] = [{ kind: 'assigned_to', assignees: ['me'] }]

function me(orgId = ORG_ID, role: 'member' | 'admin' = 'member'): MeResponse {
  return {
    user: { id: ALICE_ID, email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: orgId, name: 'Acme Realty', role },
    platform_admin: false,
  }
}
function sameOrgOtherActor(): MeResponse {
  return {
    ...me(),
    user: { id: '99999999-9999-4999-8999-999999999999', email: 'other@acme.test', display_name: 'Other agent' },
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
function savedDetail(id: string, filter: FilterClause[] = []): SavedListDetailResponse {
  return {
    list: {
      id,
      name: `Saved ${id.slice(0, 4)}`,
      scope: 'shared',
      revision: 1,
      created_at: '2026-09-06T00:00:00.000Z',
      updated_at: '2026-09-06T00:00:00.000Z',
      can_edit: false,
      can_delete: false,
    },
    filter: { version: 1, clauses: filter },
    description: filter.length === 0 ? ['All people'] : ['Assigned to me'],
    filter_error: null,
  }
}
function editableSavedDetail(
  id: string,
  filter: FilterClause[] = [],
  revision = 1,
  name = `Saved ${id.slice(0, 4)}`,
): SavedListDetailResponse {
  const detail = savedDetail(id, filter)
  return {
    ...detail,
    list: { ...detail.list, name, revision, can_edit: true, can_delete: true },
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
  person?: (id: string) => PersonDetailResponse | Promise<PersonDetailResponse> | ApiError
  savedList?: (id: string) => SavedListDetailResponse | Promise<SavedListDetailResponse> | ApiError
  savedCount?: (id: string, revision: number) => SavedListCountResponse | Promise<SavedListCountResponse> | ApiError
  updateSavedList?: (id: string, body: UpdateSavedListRequest) =>
    UpdateSavedListResponse | Promise<UpdateSavedListResponse> | ApiError
  createSavedList?: (body: CreateSavedListRequest) =>
    CreateSavedListResponse | Promise<CreateSavedListResponse> | ApiError
}
function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === '/me') return me()
    if (path === '/stages') return { stages: [{ id: STAGE_ID, name: 'Lead', position: 1 }] }
    if (path === '/organization/members') return { members: [] }
    if (path === '/inquiry-sources') return options.sources?.() ?? { sources: ['website', 'zillow'], truncated: false }
    if (path === '/saved-lists' && init?.method === 'POST') {
      const body = JSON.parse(String(init.body)) as CreateSavedListRequest
      const response = options.createSavedList?.(body)
      if (!response) throw new Error(`unexpected POST ${path}`)
      if (response instanceof ApiError) throw response
      return response
    }
    if (path.startsWith('/saved-lists/') && path.includes('/count?')) {
      const [id, query] = path.slice('/saved-lists/'.length).split('/count?')
      const revision = Number(new URLSearchParams(query).get('revision'))
      const response = options.savedCount?.(id!, revision) ?? { list_id: id!, revision, count: 1, truncated: false }
      if (response instanceof ApiError) throw response
      return response
    }
    if (path.startsWith('/saved-lists/') && init?.method === 'PUT') {
      const id = path.slice('/saved-lists/'.length)
      const body = JSON.parse(String(init.body)) as UpdateSavedListRequest
      const response = options.updateSavedList?.(id, body)
      if (!response) throw new Error(`unexpected PUT ${path}`)
      if (response instanceof ApiError) throw response
      return response
    }
    if (path.startsWith('/saved-lists/')) {
      const response = options.savedList?.(path.slice('/saved-lists/'.length)) ?? savedDetail(path.slice('/saved-lists/'.length))
      if (response instanceof ApiError) throw response
      return response
    }
    if (path.startsWith('/people/')) {
      const response = options.person?.(path.slice('/people/'.length)) ?? detail()
      if (response instanceof ApiError) throw response
      return response
    }
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
      { path: '/lists', component: { template: '<p>Lists index</p>' } },
      { path: '/lists/:savedListId', component: PeopleView, props: true },
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
  vi.unstubAllGlobals()
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

describe('Saved-list workspace safety', () => {
  it('loads an intentionally empty saved definition through the explicit filtered path', async () => {
    stub({ savedList: (id) => savedDetail(id, []) })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(peoplePaths()).toContain(filteredPath([]))
    expect(peoplePaths()).not.toContain('/people')
    expect(wrapper.text()).toContain('Shared with everyone in this Organization')
  })

  it('does not issue People when the saved definition is missing', async () => {
    stub({ savedList: () => new ApiError(404, 'not_found') })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(peoplePaths()).toEqual([])
    expect(wrapper.text()).toContain('This saved list is not available.')
  })

  it('pauses invalid saved criteria without presenting disabled work as loading', async () => {
    stub({
      savedList: (id) => ({
        ...savedDetail(id, [{ kind: 'stage', stage_ids: ['missing-stage'] }]),
        filter_error: 'invalid_stage',
      }),
    })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(peoplePaths()).toEqual([])
    expect(wrapper.text()).toContain('People are paused until the criteria are repaired.')
    expect(wrapper.text()).toContain('Match count paused')
    expect(wrapper.text()).not.toContain('Loading people…')
    expect(wrapper.text()).not.toContain('Updating match count…')
  })

  it('does not fall back to an unfiltered People request during a cached-list switch', async () => {
    const second = deferred<SavedListDetailResponse>()
    stub({
      savedList: (id) => id === SAVED_LIST_B ? second.promise : savedDetail(id, []),
    })
    const { router } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(peoplePaths()).toContain(filteredPath([]))
    apiFetchMock.mockClear()

    await router.push(`/lists/${SAVED_LIST_B}`)
    await flushPromises()
    expect(peoplePaths()).not.toContain('/people')
    second.resolve(savedDetail(SAVED_LIST_B, []))
    await flushPromises()
    expect(peoplePaths()).toContain(filteredPath([]))
    expect(peoplePaths()).not.toContain('/people')
  })

  it('does not mark a server filter dirty solely because wire keys are ordered differently', async () => {
    const wireOrdered = JSON.parse('{"clauses":[{"assignees":["me"],"kind":"assigned_to"}],"version":1}')
    stub({
      savedList: (id) => ({ ...savedDetail(id, meFilter), filter: wireOrdered }),
    })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(wrapper.text()).not.toContain('Unsaved changes')
    expect(wrapper.text()).toContain('Assigned to me')
  })

  it('keeps the original revision after a definite conflict until Reload is chosen', async () => {
    const requests: UpdateSavedListRequest[] = []
    let latest = editableSavedDetail(SAVED_LIST_A, [], 1, 'Original')
    stub({
      savedList: () => latest,
      updateSavedList: (_id, body) => {
        requests.push(body)
        latest = editableSavedDetail(SAVED_LIST_A, emailFilter, 2, 'Concurrent edit')
        return new ApiError(409, 'saved_list_conflict')
      },
    })
    const { wrapper, queryClient } = await mountView(`/lists/${SAVED_LIST_A}`)
    edit(wrapper, phoneFilter)
    await flushPromises()
    const save = () => wrapper.findAll('button').find((button) => button.text() === 'Save')!

    await save().trigger('click')
    await flushPromises()
    expect(requests).toHaveLength(1)
    expect(requests[0]?.expected_revision).toBe(1)
    expect(wrapper.text()).toContain('Reload saved version')

    await queryClient.invalidateQueries({ queryKey: queryKeys.savedList(ORG_ID, ALICE_ID, SAVED_LIST_A) })
    await flushPromises()
    expect(wrapper.text()).toContain('Unsaved changes')
    await save().trigger('click')
    await flushPromises()

    expect(requests).toHaveLength(2)
    expect(requests[1]?.expected_revision).toBe(1)
  })

  it('revalidates authority after a definite denied update without adopting the dirty draft', async () => {
    let latest = editableSavedDetail(SAVED_LIST_A, [], 1, 'Initially writable')
    stub({
      savedList: () => latest,
      updateSavedList: () => {
        latest = {
          ...editableSavedDetail(SAVED_LIST_A, [], 2, 'Now read only'),
          list: { ...editableSavedDetail(SAVED_LIST_A).list, revision: 2, name: 'Now read only', can_edit: false, can_delete: false },
        }
        return new ApiError(403, 'forbidden')
      },
    })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    edit(wrapper, phoneFilter)
    await flushPromises()
    await wrapper.findAll('button').find((button) => button.text() === 'Save')!.trigger('click')
    await flushPromises()
    await flushPromises()

    expect(wrapper.text()).toContain('Unsaved changes')
    expect(wrapper.findAll('button').some((button) => button.text() === 'Save')).toBe(false)
    expect(wrapper.text()).toContain('Now read only')
  })

  it('Refresh refetches unchanged named People and count data while retaining the original dirty baseline', async () => {
    stub({ savedList: (id) => editableSavedDetail(id, meFilter, 1, 'Saved Me') })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    edit(wrapper, phoneFilter)
    await flushPromises()
    const countPaths = () => apiFetchMock.mock.calls
      .map(([path]) => path)
      .filter((path) => path.startsWith(`/saved-lists/${SAVED_LIST_A}/count?`))
    const beforePeople = peoplePaths().filter((path) => path === filteredPath(phoneFilter)).length
    const beforeCounts = countPaths().length

    await wrapper.findAll('button').find((button) => button.text() === 'Refresh')!.trigger('click')
    await flushPromises()
    await flushPromises()

    expect(peoplePaths().filter((path) => path === filteredPath(phoneFilter)).length).toBeGreaterThan(beforePeople)
    expect(countPaths().length).toBeGreaterThan(beforeCounts)
    expect(wrapper.text()).toContain('Unsaved changes')
    expect((wrapper.get('[aria-label="List name"]').element as HTMLInputElement).value).toBe('Saved Me')
  })

  it('window focus refreshes an unchanged relative-time definition and keeps a dirty local name', async () => {
    const ageFilter: FilterClause[] = [{ kind: 'last_contact', age: { op: 'within_days', days: 1 } }]
    let crossedCutoff = false
    let countCalls = 0
    stub({
      savedList: (id) => editableSavedDetail(id, ageFilter, 1, 'Recent contact'),
      people: () => crossedCutoff ? result('New cutoff match') : result('Old cutoff match'),
      savedCount: (id, revision) => {
        countCalls++
        return { list_id: id, revision, count: crossedCutoff ? 2 : 1, truncated: false }
      },
    })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(wrapper.text()).toContain('Old cutoff match')
    await wrapper.get('[aria-label="List name"]').setValue('Local age draft')
    await flushPromises()
    const beforePeople = peoplePaths().filter((path) => path === filteredPath(ageFilter)).length
    const beforeCounts = countCalls

    crossedCutoff = true
    window.dispatchEvent(new Event('focus'))
    await flushPromises()
    await flushPromises()

    expect(peoplePaths().filter((path) => path === filteredPath(ageFilter)).length).toBeGreaterThan(beforePeople)
    expect(countCalls).toBeGreaterThan(beforeCounts)
    expect(wrapper.text()).toContain('New cutoff match')
    expect(wrapper.text()).not.toContain('Old cutoff match')
    expect(wrapper.text()).toContain('Unsaved changes')
    expect((wrapper.get('[aria-label="List name"]').element as HTMLInputElement).value).toBe('Local age draft')
  })

  it('removes retained People after a definite update finds the list deleted', async () => {
    let deleted = false
    stub({
      savedList: () => deleted ? new ApiError(404, 'not_found') : editableSavedDetail(SAVED_LIST_A, meFilter),
      updateSavedList: () => {
        deleted = true
        return new ApiError(404, 'not_found')
      },
    })
    const { wrapper } = await mountView(`/lists/${SAVED_LIST_A}`)
    edit(wrapper, phoneFilter)
    await flushPromises()
    await wrapper.findAll('button').find((button) => button.text() === 'Save')!.trigger('click')
    await flushPromises()
    await flushPromises()

    expect(wrapper.text()).toContain('This saved list is not available.')
    expect(wrapper.find('[data-testid="people-result-count"]').exists()).toBe(false)
    expect(peoplePaths()).not.toContain('/people')
  })

  it('lets a shared reader preview locally, labels saved metadata, and closes the inspector on a list switch', async () => {
    stub({
      savedList: (id) => savedDetail(id, meFilter),
      people: (filter) => filter === serialized(meFilter) ? result('Saved match') : result('Local preview'),
    })
    const { wrapper, router } = await mountView(`/lists/${SAVED_LIST_A}`)
    expect(wrapper.findComponent(FilterBar).exists()).toBe(true)
    await wrapper.get(`a[href="/people/${PERSON_ID}"]`).trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(true)
    edit(wrapper, [])
    await flushPromises()
    expect(wrapper.text()).toContain('Saved version: Assigned to me')
    expect(wrapper.text()).toContain('Saved version: 1 match')
    vi.stubGlobal('confirm', vi.fn(() => true))
    await router.push(`/lists/${SAVED_LIST_B}`)
    await flushPromises()
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(false)
  })

  it('uses the real route guard to retain a dirty local preview until discard is confirmed', async () => {
    stub({ savedList: (id) => savedDetail(id, meFilter) })
    const { wrapper, router } = await mountView(`/lists/${SAVED_LIST_A}`)
    edit(wrapper, [])
    await flushPromises()
    const rejectDiscard = vi.fn(() => false)
    vi.stubGlobal('confirm', rejectDiscard)
    await router.push(`/lists/${SAVED_LIST_B}`)
    expect(router.currentRoute.value.params.savedListId).toBe(SAVED_LIST_A)
    expect(rejectDiscard).toHaveBeenCalledTimes(1)

    vi.stubGlobal('confirm', vi.fn(() => true))
    await router.push(`/lists/${SAVED_LIST_B}`)
    await flushPromises()
    expect(router.currentRoute.value.params.savedListId).toBe(SAVED_LIST_B)
  })

  it('never generates a new create token after a frozen retry reaches a deleted attempt', async () => {
    const first = deferred<CreateSavedListResponse>()
    const requests: CreateSavedListRequest[] = []
    stub({
      createSavedList: (body) => {
        requests.push(body)
        return requests.length === 1 ? first.promise : new ApiError(409, 'saved_list_deleted')
      },
    })
    const { wrapper } = await mountView()
    const dialog = wrapper.findComponent(SavedListDialog)

    dialog.vm.$emit('submit', { name: 'Frozen attempt', scope: 'personal' })
    await flushPromises()
    first.reject(new Error('network lost after commit'))
    await flushPromises()
    expect(requests).toHaveLength(1)
    expect(dialog.props('freezePayload')).toBe(true)

    dialog.vm.$emit('submit', { name: 'Edited but ignored', scope: 'shared' })
    await flushPromises()
    expect(requests).toHaveLength(2)
    expect(requests[1]).toEqual(requests[0])
    expect(dialog.props('retryUnavailable')).toBe(true)

    dialog.vm.$emit('submit', { name: 'Must not create', scope: 'personal' })
    await flushPromises()
    expect(requests).toHaveLength(2)
  })

  it('keeps a dirty Duplicate in place and requires its explicit copy link to navigate', async () => {
    const copyId = '88888888-8888-4888-8888-888888888888'
    stub({
      savedList: (id) => id === copyId ? savedDetail(id, meFilter) : savedDetail(id, meFilter),
      createSavedList: (request) => ({
        list: {
          ...savedDetail(copyId).list,
          id: copyId,
          name: request.name,
          scope: request.scope,
          can_edit: true,
          can_delete: true,
        },
        created: true,
      }),
    })
    const { wrapper, router } = await mountView(`/lists/${SAVED_LIST_A}`)
    edit(wrapper, [])
    await flushPromises()
    await wrapper.findAll('button').find((button) => button.text() === 'Duplicate')!.trigger('click')
    await flushPromises()
    wrapper.findComponent(SavedListDialog).vm.$emit('submit', { name: 'Saved copy', scope: 'personal' })
    await flushPromises()

    expect(router.currentRoute.value.params.savedListId).toBe(SAVED_LIST_A)
    expect(wrapper.text()).toContain('Unsaved changes')
    const copyLink = wrapper.get(`a[href="/lists/${copyId}"]`)
    const rejectDiscard = vi.fn(() => false)
    vi.stubGlobal('confirm', rejectDiscard)
    await copyLink.trigger('click')
    await flushPromises()
    expect(rejectDiscard).toHaveBeenCalledTimes(1)
    expect(router.currentRoute.value.params.savedListId).toBe(SAVED_LIST_A)
    expect(wrapper.text()).toContain('Unsaved changes')

    vi.stubGlobal('confirm', vi.fn(() => true))
    await copyLink.trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.params.savedListId).toBe(copyId)
  })

  it('does not navigate or restore private UI when a pending create completes after a same-Organization actor change', async () => {
    const pending = deferred<CreateSavedListResponse>()
    const createdId = '99999999-9999-4999-8999-999999999998'
    stub({ createSavedList: () => pending.promise })
    const { wrapper, router, queryClient } = await mountView()
    wrapper.findComponent(SavedListDialog).vm.$emit('submit', { name: 'Late create', scope: 'personal' })
    await flushPromises()

    queryClient.setQueryData(queryKeys.me, sameOrgOtherActor())
    await flushPromises()
    pending.resolve({
      list: { ...savedDetail(createdId).list, id: createdId, name: 'Late create', scope: 'personal' },
      created: true,
    })
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/people')
    expect(wrapper.text()).not.toContain('Created Late create.')
    expect(queryClient.getQueryData(queryKeys.savedList(ORG_ID, ALICE_ID, createdId))).toBeUndefined()
  })

  it('does not navigate after a pending create completes after its view unmounts', async () => {
    const pending = deferred<CreateSavedListResponse>()
    const createdId = '99999999-9999-4999-8999-999999999997'
    stub({ createSavedList: () => pending.promise })
    const { wrapper, router } = await mountView()
    wrapper.findComponent(SavedListDialog).vm.$emit('submit', { name: 'Unmounted create', scope: 'personal' })
    await flushPromises()
    await router.push('/lists')
    await flushPromises()

    pending.resolve({
      list: { ...savedDetail(createdId).list, id: createdId, name: 'Unmounted create', scope: 'personal' },
      created: true,
    })
    await flushPromises()

    expect(router.currentRoute.value.path).toBe('/lists')
  })

  it('explains symbolic Me in a mixed saved-list create definition', async () => {
    stub()
    const { wrapper } = await mountView()
    edit(wrapper, [
      ...meFilter,
      { kind: 'stage', stage_ids: [STAGE_ID] },
    ])
    await flushPromises()
    await wrapper.findAll('button').find((button) => button.text() === 'Save as list')!.trigger('click')
    await flushPromises()

    expect(wrapper.findComponent(SavedListDialog).props('description')).toContain('“Me” stays symbolic')
  })
})


function detail(name = 'Grace Hopper', id = PERSON_ID): PersonDetailResponse {
  return {
    person: { ...result(name).people[0]!, id },
    contact_methods: [{ id: 'email-1', kind: 'email', value: 'grace@example.com' }],
    inquiries: [{ id: 'inquiry-1', source: 'website', source_external_id: null, message: null, received_at: '2026-08-22T09:00:00.000Z' }],
    history: [],
  }
}

describe('People inspector', () => {
  it('loads on selection, preserves filtered URL, and returns focus on Escape', async () => {
    stub()
    const { wrapper, router, queryClient } = await mountView(filteredPath(phoneFilter))
    const link = wrapper.get(`a[href="/people/${PERSON_ID}"]`)
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(false)
    expect(peoplePaths().some((path) => path.startsWith('/people/'))).toBe(false)
    await link.trigger('click')
    await flushPromises()
    const preview = wrapper.get('[data-testid="person-preview"]')
    expect(preview.text()).toContain('Grace Hopper')
    expect(preview.text()).toContain('Latest source')
    expect(preview.text()).toContain('website')
    expect(preview.get('a[aria-label="Open full profile"]').attributes('href')).toBe(`/people/${PERSON_ID}`)
    expect(router.currentRoute.value.path).toBe('/people')
    expect(router.currentRoute.value.query.filter).toBe(serialized(phoneFilter))
    expect(queryClient.getQueryData(queryKeys.person(ORG_ID, PERSON_ID))).toEqual(detail())
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(false)
    expect(document.activeElement).toBe(link.element)
  })

  it('does not intercept modified links and keeps full-profile navigation available', async () => {
    stub()
    const { wrapper, router } = await mountView()
    await wrapper.get(`a[href="/people/${PERSON_ID}"]`).trigger('click', { ctrlKey: true })
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(false)
    await wrapper.get(`a[href="/people/${PERSON_ID}"]`).trigger('click')
    await flushPromises()
    await wrapper.get('[aria-label="Open full profile"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe(`/people/${PERSON_ID}`)
  })

  it('does not show a slow previous person after switching, and clears on organization changes', async () => {
    const secondId = '66666666-6666-6666-6666-666666666666'
    const pending = deferred<PersonDetailResponse>()
    stub({
      people: () => ({ people: [detail().person, detail('Ada Lovelace', secondId).person], truncated: false }),
      person: (id) => id === PERSON_ID ? pending.promise : detail('Ada Lovelace', secondId),
    })
    const { wrapper, queryClient } = await mountView()
    await wrapper.get(`a[href="/people/${PERSON_ID}"]`).trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="person-preview"]').text()).toContain('Loading person')
    await wrapper.get(`a[href="/people/${secondId}"]`).trigger('click')
    await flushPromises()
    pending.resolve(detail())
    await flushPromises()
    expect(wrapper.get('[data-testid="person-preview"]').text()).toContain('Ada Lovelace')
    expect(wrapper.get('[data-testid="person-preview"]').text()).not.toContain('Grace Hopper')
    queryClient.setQueryData(queryKeys.me, me('another-organization'))
    await flushPromises()
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(false)
    expect(queryClient.getQueryData(queryKeys.person('another-organization', secondId))).toBeUndefined()
  })

  it('closes the preview when the filter changes', async () => {
    stub()
    const { wrapper } = await mountView()
    await wrapper.get(`a[href="/people/${PERSON_ID}"]`).trigger('click')
    await flushPromises()
    edit(wrapper, phoneFilter)
    await flushPromises()
    expect(wrapper.find('[data-testid="person-preview"]').exists()).toBe(false)
  })

  it('shows a retryable error and never exposes details after access is lost', async () => {
    let failure = true
    stub({ person: () => failure ? new ApiError(503, 'unavailable') : detail() })
    const { wrapper, queryClient } = await mountView()
    await wrapper.get(`a[href="/people/${PERSON_ID}"]`).trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="person-preview"] [role="alert"]').exists()).toBe(true)
    expect(wrapper.find('[aria-label="Open email app"]').exists()).toBe(false)
    failure = false
    await wrapper.get('[data-testid="person-preview"] [role="alert"] button').trigger('click')
    await flushPromises()
    expect(wrapper.find('[aria-label="Open email app"]').exists()).toBe(true)
    stub({ person: () => new ApiError(404, 'not_found') })
    await queryClient.invalidateQueries({ queryKey: queryKeys.person(ORG_ID, PERSON_ID) })
    await flushPromises()
    expect(wrapper.get('[data-testid="person-preview"]').text()).toContain('no longer available')
    expect(wrapper.get('[data-testid="person-preview"]').text()).not.toContain('Grace Hopper')
    expect(wrapper.find('[aria-label="Open email app"]').exists()).toBe(false)
  })
})
