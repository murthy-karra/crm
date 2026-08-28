// SLICE_011a §8 acceptance criteria 11-13: FilterBar add/remove chip ->
// refetch with the new factory key; count + truncated notice render; days
// input commits on blur; URL mount-rehydrate / router.replace sync /
// invalid-and-server-rejected degrade (review F5); `me` serialized
// symbolically; the sources picker populated from the new query.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type { InquirySourcesResponse, MeResponse, MembersResponse, PeopleResponse, StagesResponse } from '../api/types'
import PeopleView from './PeopleView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ALICE_ID = '22222222-2222-2222-2222-222222222222'
const BOB_ID = '33333333-3333-3333-3333-333333333333'
const STAGE_ID = '44444444-4444-4444-4444-444444444444'
const PERSON_ID = '55555555-5555-5555-5555-555555555555'

function me(): MeResponse {
  return {
    user: { id: ALICE_ID, email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}

function stages(): StagesResponse {
  return { stages: [{ id: STAGE_ID, name: 'Lead', position: 1 }] }
}

function members(): MembersResponse {
  return {
    members: [
      {
        user_id: BOB_ID,
        display_name: 'Bob',
        email: 'bob@acme.test',
        role: 'member',
        status: 'active',
        joined_at: '2026-08-20T00:00:00.000Z',
        assigned_people_count: 0,
      },
    ],
  }
}

function sourcesResponse(sources: string[] = ['website', 'zillow']): InquirySourcesResponse {
  return { sources, truncated: false }
}

function person(id = PERSON_ID) {
  return {
    id,
    first_name: 'Grace',
    last_name: 'Hopper',
    display_name: 'Grace Hopper',
    stage: { id: STAGE_ID, name: 'Lead' },
    assigned_user: null,
    primary_email: 'grace@example.com',
    primary_phone: null,
    inquiry_count: 1,
    last_inquiry_at: '2026-08-22T09:00:00.000Z',
    created_at: '2026-08-22T09:00:00.000Z',
  }
}

interface StubOptions {
  people?: (filter: string | null) => PeopleResponse | Promise<PeopleResponse> | ApiError
  sources?: InquirySourcesResponse
}

/**
 * R1 review follow-up: the server 400s a filter carrying a structurally
 * invalid clause (§4b — above all, an empty value array, exactly what a
 * DRAFT multi-value clause looks like before the R1 fix). The original
 * mock blanket-200'd everything, which is why 9 web tests never caught
 * the bug the real server would have rejected — this makes the mock
 * behave like the real API for that one structural class.
 */
function hasStructurallyInvalidClause(filterJson: string): boolean {
  try {
    const parsed = JSON.parse(filterJson) as { clauses?: unknown[] }
    if (!Array.isArray(parsed.clauses)) return true
    return parsed.clauses.some((c) => {
      const clause = c as Record<string, unknown>
      if (clause.kind === 'stage') return Array.isArray(clause.stage_ids) && clause.stage_ids.length === 0
      if (clause.kind === 'assigned_to') return Array.isArray(clause.assignees) && clause.assignees.length === 0
      if (clause.kind === 'source') return Array.isArray(clause.sources) && clause.sources.length === 0
      return false
    })
  } catch {
    return true
  }
}

function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string) => {
    if (path === '/me') return me()
    if (path === '/stages') return stages()
    if (path === '/organization/members') return members()
    if (path === '/inquiry-sources') return options.sources ?? sourcesResponse()
    if (path.startsWith('/people')) {
      const url = new URL(path, 'http://x')
      const filter = url.searchParams.get('filter')
      if (filter !== null && hasStructurallyInvalidClause(filter)) {
        throw new ApiError(400, 'malformed_request')
      }
      const result = options.people
        ? options.people(filter)
        : ({ people: [person()], truncated: false } satisfies PeopleResponse)
      if (result instanceof ApiError) throw result
      return result
    }
    throw new Error(`unexpected GET ${path}`)
  })
}

async function mountView(initialPath = '/people') {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/people', component: PeopleView },
      { path: '/people/:id', component: { template: '<div />' } },
      { path: '/intake/new', component: { template: '<div />' } },
    ],
  })
  await router.push(initialPath)
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(PeopleView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, router }
}

function peoplePaths(): string[] {
  return apiFetchMock.mock.calls.map(([p]) => p).filter((p) => p.startsWith('/people'))
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('PeopleView — FilterBar data flow (criteria 11, 13)', () => {
  it('adding a chip refetches /people with the new filter query param, and removing it refetches the plain list', async () => {
    stub()
    const { wrapper } = await mountView()
    expect(peoplePaths()).toEqual(['/people'])

    // Add the has_replied clause via the "Add filter" select (the first
    // rendered Select, mirroring IntakeSettingsView.test.ts's `selects(...)[0]`
    // precedent — the age-op select only exists once an editor is open).
    const addSelect = wrapper.findAllComponents(Select)[0]
    await addSelect.vm.$emit('update:model-value', 'has_replied')
    await flushPromises()

    // The editor opens with Yes/No buttons; click Yes.
    const yesButton = wrapper.get('[data-testid="filter-bool-yes-has_replied"]')
    await yesButton.trigger('click')
    await flushPromises()

    const filteredCalls = peoplePaths().filter((p) => p.includes('filter='))
    expect(filteredCalls.length).toBeGreaterThan(0)
    const decoded = decodeURIComponent(filteredCalls[filteredCalls.length - 1].split('filter=')[1])
    expect(JSON.parse(decoded)).toEqual({ version: 1, clauses: [{ kind: 'has_replied', value: true }] })

    // Remove the chip -> back to the plain (unfiltered) list.
    await wrapper.get('[data-testid="filter-chip-remove-has_replied"]').trigger('click')
    await flushPromises()
    expect(peoplePaths()[peoplePaths().length - 1]).toBe('/people')
  })

  it('count and truncated notice render from the DataTable footer', async () => {
    stub({ people: () => ({ people: [person('a'), person('b')], truncated: true }) })
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('2 people')
    expect(wrapper.text()).toContain('more exist')
  })

  it('the days input on an age-clause editor commits on blur, not per keystroke', async () => {
    stub()
    const { wrapper } = await mountView()

    const addSelect = wrapper.findAllComponents(Select)[0]
    await addSelect.vm.$emit('update:model-value', 'last_contact')
    await flushPromises()

    const daysInput = wrapper.get('[data-testid="filter-days-last_contact"]')
    await daysInput.setValue('45')
    // No commit yet — still keystrokes only.
    expect(peoplePaths().some((p) => p.includes('45'))).toBe(false)

    await daysInput.trigger('blur')
    await flushPromises()

    const filteredCalls = peoplePaths().filter((p) => p.includes('filter='))
    const decoded = decodeURIComponent(filteredCalls[filteredCalls.length - 1].split('filter=')[1])
    expect(JSON.parse(decoded)).toEqual({
      version: 1,
      clauses: [{ kind: 'last_contact', age: { op: 'within_days', days: 45 } }],
    })
  })

  it('the Source editor lists options from GET /api/inquiry-sources', async () => {
    stub({ sources: sourcesResponse(['facebook', 'referral']) })
    const { wrapper } = await mountView()

    const addSelect = wrapper.findAllComponents(Select)[0]
    await addSelect.vm.$emit('update:model-value', 'source')
    await flushPromises()

    const editor = wrapper.get('[data-testid="filter-editor-source"]')
    expect(editor.text()).toContain('facebook')
    expect(editor.text()).toContain('referral')
  })

  // Review R1 pin (a): adding a multi-value chip (stage/assigned_to/
  // source) is a DRAFT with an empty value array — it must render as a
  // chip (the editor needs somewhere to attach to) but never be
  // serialized to the wire. The stub above would 400 an empty-array
  // filter exactly like the real server; this pins that the request is
  // never even attempted.
  it('adding a multi-value chip (a draft) triggers no fetch at all — never an empty-array filter', async () => {
    stub()
    const { wrapper } = await mountView()
    expect(peoplePaths()).toEqual(['/people'])

    const addSelect = wrapper.findAllComponents(Select)[0]
    await addSelect.vm.$emit('update:model-value', 'stage')
    await flushPromises()

    expect(wrapper.find('[data-testid="filter-chip-stage"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="filter-chip-stage"]').text()).toContain('choose a value')
    expect(peoplePaths()).toEqual(['/people'])
    expect(peoplePaths().some((p) => p.includes('filter='))).toBe(false)

    // The first committed value fires the fetch.
    await wrapper.get(`input[type="checkbox"]`).setValue(true)
    await flushPromises()
    expect(peoplePaths().some((p) => p.includes('filter='))).toBe(true)
  })
})

describe('PeopleView — URL sync (criterion 12)', () => {
  it('mounting with ?filter= rehydrates the chips', async () => {
    const filter = { version: 1, clauses: [{ kind: 'has_phone', value: true }] }
    stub()
    const { wrapper } = await mountView(`/people?filter=${encodeURIComponent(JSON.stringify(filter))}`)

    expect(wrapper.find('[data-testid="filter-chip-has_phone"]').exists()).toBe(true)
    const filteredCalls = peoplePaths().filter((p) => p.includes('filter='))
    expect(filteredCalls.length).toBeGreaterThan(0)
  })

  it('a chip edit router.replace()s the query', async () => {
    stub()
    const { wrapper, router } = await mountView()

    const addSelect = wrapper.findAllComponents(Select)[0]
    await addSelect.vm.$emit('update:model-value', 'has_email')
    await flushPromises()
    await wrapper.get('[data-testid="filter-bool-no-has_email"]').trigger('click')
    await flushPromises()

    const query = router.currentRoute.value.query.filter
    expect(typeof query).toBe('string')
    expect(JSON.parse(query as string)).toEqual({ version: 1, clauses: [{ kind: 'has_email', value: false }] })
  })

  it('an invalid/undecodable URL filter is dropped on mount: chips empty, param cleared, no error toast', async () => {
    stub()
    const { wrapper, router } = await mountView('/people?filter=not-even-json')

    expect(wrapper.find('[data-testid^="filter-chip-"]').exists()).toBe(false)
    expect(router.currentRoute.value.query.filter).toBeUndefined()
    // The plain list still loads (no error banner).
    expect(wrapper.text()).not.toContain('Could not load people')
    expect(peoplePaths()).toContain('/people')
  })

  it('a decodable-but-server-rejected filter (422) degrades identically: drop, clear, refetch plain', async () => {
    let rejected = false
    stub({
      people: (filter) => {
        if (filter && !rejected) {
          rejected = true
          return new ApiError(422, 'invalid_stage')
        }
        return { people: [person()], truncated: false }
      },
    })
    const filter = { version: 1, clauses: [{ kind: 'stage', stage_ids: ['some-org-b-stage'] }] }
    const { wrapper, router } = await mountView(`/people?filter=${encodeURIComponent(JSON.stringify(filter))}`)

    // Degrade: chips cleared, param cleared, plain list eventually renders.
    await flushPromises()
    expect(wrapper.find('[data-testid^="filter-chip-"]').exists()).toBe(false)
    expect(router.currentRoute.value.query.filter).toBeUndefined()
    expect(wrapper.text()).not.toContain('Could not load people')
    expect(wrapper.text()).toContain('Grace Hopper')
  })

  // Pin (d): the same degrade path for a 400, not just a 422 — both are
  // "decodable but the server rejects it" per amended §6.
  it('a decodable-but-server-rejected filter (400) degrades identically: drop, clear, refetch plain', async () => {
    let rejected = false
    stub({
      people: (filter) => {
        if (filter && !rejected) {
          rejected = true
          return new ApiError(400, 'malformed_request')
        }
        return { people: [person()], truncated: false }
      },
    })
    // A shape the client-side `parseFilter` accepts (non-empty array) but
    // the server's stricter canonical-uuid check (§4b) would 400 — the
    // client can't and shouldn't replicate every server-side rule.
    const filter = { version: 1, clauses: [{ kind: 'stage', stage_ids: ['NOT-CANONICAL-UUID-FORM'] }] }
    const { wrapper, router } = await mountView(`/people?filter=${encodeURIComponent(JSON.stringify(filter))}`)

    await flushPromises()
    expect(wrapper.find('[data-testid^="filter-chip-"]').exists()).toBe(false)
    expect(router.currentRoute.value.query.filter).toBeUndefined()
    expect(wrapper.text()).not.toContain('Could not load people')
    expect(wrapper.text()).toContain('Grace Hopper')
  })

  // Amended §6 / review R1: a 5xx on ANY filtered fetch — URL-origin or
  // user-composed — NEVER degrades. Chips and the URL param stay exactly
  // as they were; only the error banner shows (the F5 complement).
  it('a 5xx on a filtered fetch keeps chips and the URL param intact (error banner, never the degrade)', async () => {
    stub({
      people: (filter) => {
        if (filter) return new ApiError(503, 'unavailable')
        return { people: [person()], truncated: false }
      },
    })
    const filter = { version: 1, clauses: [{ kind: 'has_email', value: true }] }
    const { wrapper, router } = await mountView(`/people?filter=${encodeURIComponent(JSON.stringify(filter))}`)

    await flushPromises()
    expect(wrapper.find('[data-testid="filter-chip-has_email"]').exists()).toBe(true)
    expect(router.currentRoute.value.query.filter).toBeDefined()
    expect(wrapper.text()).toContain('The server is temporarily unavailable. Try again shortly.')
  })

  it("`me` is serialized symbolically in the URL, not as the viewer's own id", async () => {
    stub()
    const { wrapper, router } = await mountView()

    const addSelect = wrapper.findAllComponents(Select)[0]
    await addSelect.vm.$emit('update:model-value', 'assigned_to')
    await flushPromises()
    await wrapper.get('[data-testid="filter-assignee-me"]').setValue(true)
    await flushPromises()

    // `route.query` values are already decoded by vue-router — this is the
    // literal serialized filter string, the same one that gets
    // percent-encoded into the actual URL and into the `?filter=` API call.
    const query = router.currentRoute.value.query.filter as string
    expect(query).toContain('"me"')
    expect(query).not.toContain(ALICE_ID)
    expect(JSON.parse(query)).toEqual({
      version: 1,
      clauses: [{ kind: 'assigned_to', assignees: ['me'] }],
    })
  })
})
