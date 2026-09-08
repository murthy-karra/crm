// SLICE_011d.md §6, §9.7: Manage → Today rules (`/manage/today-feeds`).
// Covers: locked anchor/`me` in the editor; preview with a member picker and
// an honest empty state; revert/off confirmations including the typed
// confirmation for `unanswered_inquiry`; the 409 reload flow requiring a new
// explicit Save click. Lane W codes against §6's contracts (SLICE_011d_IMPL.md
// "Lane W ... may stub the API in tests until Lane B's routes exist").
import { DOMWrapper, flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type {
  Feed,
  MeResponse,
  Member,
  MembersResponse,
  PreviewTodayFeedResponse,
  Stage,
  StagesResponse,
  TagsResponse,
  TodayFeedMutationResponse,
  TodayFeedsResponse,
} from '../api/types'
import TodayFeedsView from './TodayFeedsView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ADMIN_ID = 'u-admin'
const BOB_ID = '22222222-2222-2222-2222-222222222222'

function me(): MeResponse {
  return {
    user: { id: ADMIN_ID, email: 'admin@example.test', display_name: 'Admin Alice' },
    organization: { id: ORG_ID, name: 'Example Realty', role: 'admin' },
    platform_admin: false,
  }
}

function members(): MembersResponse {
  return {
    members: [
      { user_id: ADMIN_ID, display_name: 'Admin Alice', email: 'admin@example.test', role: 'admin', status: 'active', joined_at: '2026-01-01T00:00:00Z', assigned_people_count: 0 },
      { user_id: BOB_ID, display_name: 'Bob', email: 'bob@example.test', role: 'member', status: 'active', joined_at: '2026-01-01T00:00:00Z', assigned_people_count: 0 },
    ] satisfies Member[],
  }
}

function stages(): StagesResponse {
  return { stages: [{ id: 'stage-1', name: 'Lead', position: 1 }] satisfies Stage[] }
}

// Slice 011e e2 (docs/specs/SLICE_011e.md §9.17).
function tags(): TagsResponse {
  return { tags: [{ id: 'tag-1', name: 'Investor', person_count: 0, can_manage: true }] }
}

const UNANSWERED_DEFAULT_FILTER = { version: 1 as const, clauses: [{ kind: 'assigned_to' as const, assignees: ['me' as const] }, { kind: 'awaiting_response' as const, value: true }] }
const CLIENT_REPLIED_DEFAULT_FILTER = { version: 1 as const, clauses: [{ kind: 'assigned_to' as const, assignees: ['me' as const] }, { kind: 'client_replied_unanswered' as const, value: true }] }
const CALL_DEFAULT_FILTER = { version: 1 as const, clauses: [{ kind: 'awaiting_call_outcome' as const, value: true }] }

function baseFeeds(overrides: Partial<Record<Feed['feed_key'], Partial<Feed>>> = {}): TodayFeedsResponse {
  const defaults: Record<Feed['feed_key'], Feed> = {
    unanswered_inquiry: {
      feed_key: 'unanswered_inquiry', enabled: true, revision: 1, is_default: true,
      filter: UNANSWERED_DEFAULT_FILTER, fresh_within_hours: 24,
      description: ['Assignee: Me', 'Awaiting a response'], filter_error: null,
      updated_at: '2026-09-01T00:00:00Z', updated_by: null,
      default: { filter: UNANSWERED_DEFAULT_FILTER, fresh_within_hours: 24 },
    },
    client_replied: {
      feed_key: 'client_replied', enabled: true, revision: 1, is_default: true,
      filter: CLIENT_REPLIED_DEFAULT_FILTER, fresh_within_hours: 24,
      description: ['Assignee: Me', 'Client replied, unanswered'], filter_error: null,
      updated_at: '2026-09-01T00:00:00Z', updated_by: null,
      default: { filter: CLIENT_REPLIED_DEFAULT_FILTER, fresh_within_hours: 24 },
    },
    call_outcome_needed: {
      feed_key: 'call_outcome_needed', enabled: true, revision: 1, is_default: true,
      filter: CALL_DEFAULT_FILTER, fresh_within_hours: null,
      description: ['A call of mine needs an outcome'], filter_error: null,
      updated_at: '2026-09-01T00:00:00Z', updated_by: null,
      default: { filter: CALL_DEFAULT_FILTER, fresh_within_hours: null },
    },
  }
  return {
    feeds: (Object.keys(defaults) as Feed['feed_key'][]).map((key) => ({ ...defaults[key], ...overrides[key] })),
  }
}

interface StubOptions {
  feeds?: () => TodayFeedsResponse
  update?: (feedKey: string, body: unknown) => TodayFeedMutationResponse | ApiError
  revert?: (feedKey: string) => TodayFeedMutationResponse | ApiError
  enabled?: (feedKey: string, body: unknown) => TodayFeedMutationResponse | ApiError
  preview?: (feedKey: string, body: unknown) => PreviewTodayFeedResponse | ApiError
}

function stub(options: StubOptions = {}) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return me()
    if (path === '/organization/members') return members()
    if (path === '/stages') return stages()
    if (path === '/inquiry-sources') return { sources: [], truncated: false }
    if (path === '/tags') return tags()
    if (path === '/organization/today-feeds' && method === 'GET') return options.feeds?.() ?? baseFeeds()
    const updateMatch = /^\/organization\/today-feeds\/([a-z_]+)$/.exec(path)
    if (updateMatch && method === 'PUT') {
      const body = JSON.parse((init?.body as string) ?? '{}')
      const result = options.update?.(updateMatch[1], body) ?? { feed: baseFeeds().feeds.find((f) => f.feed_key === updateMatch[1])!, changed: true }
      if (result instanceof ApiError) throw result
      return result
    }
    const revertMatch = /^\/organization\/today-feeds\/([a-z_]+)\/revert$/.exec(path)
    if (revertMatch && method === 'POST') {
      const result = options.revert?.(revertMatch[1]) ?? { feed: baseFeeds().feeds.find((f) => f.feed_key === revertMatch[1])!, changed: true }
      if (result instanceof ApiError) throw result
      return result
    }
    const enabledMatch = /^\/organization\/today-feeds\/([a-z_]+)\/enabled$/.exec(path)
    if (enabledMatch && method === 'PUT') {
      const body = JSON.parse((init?.body as string) ?? '{}')
      const result = options.enabled?.(enabledMatch[1], body) ??
        { feed: { ...baseFeeds().feeds.find((f) => f.feed_key === enabledMatch[1])!, enabled: body.enabled }, changed: true }
      if (result instanceof ApiError) throw result
      return result
    }
    const previewMatch = /^\/organization\/today-feeds\/([a-z_]+)\/preview$/.exec(path)
    if (previewMatch && method === 'POST') {
      const body = JSON.parse((init?.body as string) ?? '{}')
      const result = options.preview?.(previewMatch[1], body) ??
        { subject: { id: body.subject_user_id, display_name: body.subject_user_id === ADMIN_ID ? 'Admin Alice' : 'Bob' }, items: [], truncated: false, description: [] }
      if (result instanceof ApiError) throw result
      return result
    }
    throw new Error(`unexpected ${method} ${path}`)
  })
}

async function mountView() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  // Preview's item table (DataTable.vue) resolves row hrefs through the
  // router even before any row link is clicked.
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [{ path: '/', component: TodayFeedsView }, { path: '/people/:id', component: { template: '<div />' } }],
  })
  await router.push('/')
  await router.isReady()
  const wrapper = mount(TodayFeedsView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

function dialogButton(text: string) {
  return Array.from(document.body.querySelectorAll('button')).find(
    (b) => b.textContent?.trim() === text && b.closest('[role="dialog"]'),
  )
}

beforeEach(() => {
  apiFetchMock.mockReset()
  // Every test that needs non-default responses calls stub({...}) again
  // itself, which replaces this implementation outright.
  stub()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('TodayFeedsView cards', () => {
  it('renders the three feeds in fixed order with chips, freshness and default status', async () => {
    const wrapper = await mountView()
    const cardTitles = wrapper.findAll('h2').map((h) => h.text())
    expect(cardTitles).toEqual(['Unanswered inquiry', 'Client replied', 'Call outcome needed'])
    expect(wrapper.get('[data-testid="feed-status-unanswered_inquiry"]').text()).toBe('Default')
    expect(wrapper.get('[data-testid="feed-card-unanswered_inquiry"]').text()).toContain('Fresh within 24 hours')
    expect(wrapper.get('[data-testid="feed-card-unanswered_inquiry"]').text()).toContain('Awaiting a response')
    expect(wrapper.get('[data-testid="feed-card-call_outcome_needed"]').text()).not.toContain('Fresh within')
  })

  it('shows Off and the invalid-fallback status', async () => {
    stub({
      feeds: () => baseFeeds({
        unanswered_inquiry: { enabled: false },
        client_replied: { is_default: false, filter_error: 'invalid_stage', updated_by: { id: BOB_ID, display_name: 'Bob' } },
      }),
    })
    const wrapper = await mountView()
    expect(wrapper.get('[data-testid="feed-status-unanswered_inquiry"]').text()).toBe('Off')
    expect(wrapper.get('[data-testid="feed-status-client_replied"]').text()).toBe('Using the default because the saved rule is invalid')
  })

  it('shows the customized-by status with editor name and date', async () => {
    stub({
      feeds: () => baseFeeds({
        client_replied: { is_default: false, updated_by: { id: BOB_ID, display_name: 'Bob' }, updated_at: '2026-09-02T10:00:00Z' },
      }),
    })
    const wrapper = await mountView()
    const status = wrapper.get('[data-testid="feed-status-client_replied"]').text()
    expect(status).toContain('Customized by Bob on')
  })
})

describe('TodayFeedsView editor locking (SLICE_011d §1 rules 3-4)', () => {
  it('locks the anchor clause and the `me` assignee for a person-state feed', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-edit-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    expect(document.body.querySelector('[data-testid="filter-chip-remove-awaiting_response"]')).toBeNull()
    expect(document.body.querySelector('[data-testid="filter-chip-locked-awaiting_response"]')).not.toBeNull()
    expect(document.body.querySelector('[data-testid="filter-chip-remove-assigned_to"]')).toBeNull()
    expect(document.body.querySelector('[data-testid="filter-chip-locked-assigned_to"]')).not.toBeNull()
  })

  it('exposes no assigned_to lock for the viewer-relative call feed', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-edit-call_outcome_needed"]').trigger('click')
    await flushPromises()

    expect(document.body.querySelector('[data-testid="filter-chip-remove-assigned_to"]')).toBeNull()
    expect(document.body.querySelector('[data-testid="filter-chip-locked-assigned_to"]')).toBeNull()
    expect(document.body.querySelector('[data-testid="filter-chip-remove-awaiting_call_outcome"]')).toBeNull()
    expect(wrapper.find('[data-testid="feed-fresh-hours"]').exists()).toBe(false)
  })
})

// Slice 011e e2 (docs/specs/SLICE_011e.md §5, §9.17): the system-feed
// editor accepts a `tags` chip alongside its locked anchor, exactly like
// the People list editor.
describe('TodayFeedsView tags chip (Slice 011e e2)', () => {
  it('adds a tags clause alongside the locked anchor and PUTs it', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-edit-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    const editorPanel = wrapper.get('[data-testid="feed-card-unanswered_inquiry"]')
    const body = new DOMWrapper(document.body)
    await editorPanel.get('[data-testid="filter-add"]').trigger('click')
    await flushPromises()
    await body.get('[data-testid="filter-add-tags"]').trigger('click')
    await flushPromises()
    await body.get('[data-testid="filter-option-tag-1"]').setValue(true)
    await flushPromises()
    expect(document.body.querySelector('[data-testid="filter-chip-tags"]')?.textContent).toContain('Tagged: Investor')

    await wrapper.get('[data-testid="feed-save-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    const call = apiFetchMock.mock.calls.find(([path]) => path === '/organization/today-feeds/unanswered_inquiry')!
    const putBody = JSON.parse((call[1] as RequestInit).body as string)
    expect(putBody.filter.clauses).toEqual([
      ...UNANSWERED_DEFAULT_FILTER.clauses,
      { kind: 'tags', tag_ids: ['tag-1'] },
    ])
  })
})

describe('TodayFeedsView save and the 409 reload flow', () => {
  it('PUTs the loaded revision with the candidate filter and freshness', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-edit-unanswered_inquiry"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="feed-fresh-hours"]').setValue('48')
    await wrapper.get('[data-testid="feed-save-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    const call = apiFetchMock.mock.calls.find(([path]) => path === '/organization/today-feeds/unanswered_inquiry')!
    const body = JSON.parse((call[1] as RequestInit).body as string)
    expect(body).toEqual({ expected_revision: 1, filter: UNANSWERED_DEFAULT_FILTER, fresh_within_hours: 48 })
    // The editor closes on success.
    expect(wrapper.find('[data-testid="feed-save-unanswered_inquiry"]').exists()).toBe(false)
  })

  it('reloads the feed for review on a 409 (discarding the draft) and requires a new explicit Save click', async () => {
    let updateAttempt = 0
    let feedsFetch = 0
    stub({
      update: (feedKey) => {
        updateAttempt += 1
        if (updateAttempt === 1) return new ApiError(409, 'today_feed_conflict')
        return { feed: { ...baseFeeds().feeds.find((f) => f.feed_key === feedKey)!, revision: 2, is_default: false }, changed: true }
      },
      // The initial load returns revision 1; the reload after the 409
      // (and every fetch thereafter) reflects the concurrent edit that
      // bumped it to revision 2 — this is what makes the reload flow
      // meaningful (the draft rebases onto the true current row).
      feeds: () => {
        feedsFetch += 1
        return feedsFetch === 1 ? baseFeeds() : baseFeeds({ unanswered_inquiry: { revision: 2, is_default: false } })
      },
    })
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-edit-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    // The admin adds a stage clause before saving — this is the draft that
    // must be visibly discarded, not silently resubmitted, on the conflict.
    // FilterBar's popover teleports to document.body, so it (and its own
    // options) must be queried there, not inside the card's own subtree.
    const editorPanel = wrapper.get('[data-testid="feed-card-unanswered_inquiry"]')
    const body = new DOMWrapper(document.body)
    await editorPanel.get('[data-testid="filter-trigger-stage"]').trigger('click')
    await flushPromises()
    await body.get('[data-testid="filter-option-stage-1"]').setValue(true)
    await flushPromises()
    expect(document.body.querySelector('[data-testid="filter-chip-stage"]')).not.toBeNull()

    await wrapper.get('[data-testid="feed-save-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    // The editor stays open with an explicit reload notice; only ONE PUT
    // has been sent so far.
    const notice = wrapper.get('[data-testid="feed-edit-notice-unanswered_inquiry"]').text()
    expect(notice).toBe('This rule was changed by someone else. The saved version has been reloaded and your draft was discarded.')
    expect(apiFetchMock.mock.calls.filter(([p, i]) => p === '/organization/today-feeds/unanswered_inquiry' && (i as RequestInit | undefined)?.method === 'PUT')).toHaveLength(1)
    expect(wrapper.find('[data-testid="feed-save-unanswered_inquiry"]').exists()).toBe(true)
    // The added stage clause is gone: the editor now shows the server's
    // reloaded definition (revision 2), not the discarded draft.
    expect(wrapper.get('[data-testid="feed-card-unanswered_inquiry"]').find('[data-testid="filter-chip-stage"]').exists()).toBe(false)

    // A fresh, explicit click sends the reloaded revision — never the stale
    // one, and never auto-retried on the admin's behalf.
    await wrapper.get('[data-testid="feed-save-unanswered_inquiry"]').trigger('click')
    await flushPromises()
    const calls = apiFetchMock.mock.calls.filter(([p, i]) => p === '/organization/today-feeds/unanswered_inquiry' && (i as RequestInit | undefined)?.method === 'PUT')
    expect(calls).toHaveLength(2)
    const secondBody = JSON.parse((calls[1][1] as RequestInit).body as string)
    expect(secondBody.expected_revision).toBe(2)
    expect(secondBody.filter.clauses.some((c: { kind: string }) => c.kind === 'stage')).toBe(false)
  })

  it('keeps the editor open with an error notice when the post-409 reload itself fails', async () => {
    let feedsFetch = 0
    stub({
      update: () => new ApiError(409, 'today_feed_conflict'),
      // The initial load must succeed (the page has to render at all); only
      // the reload triggered by the 409 fails.
      feeds: () => {
        feedsFetch += 1
        if (feedsFetch === 1) return baseFeeds()
        throw new ApiError(503, 'unavailable')
      },
    })
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-edit-unanswered_inquiry"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="feed-save-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    expect(wrapper.get('[data-testid="feed-edit-notice-unanswered_inquiry"]').text()).toBe(
      'This rule was changed by someone else, and the latest version could not be loaded. Try Save again.',
    )
    // The editor stays open (never silently closes on a failed reload) and
    // the admin's in-progress draft is left exactly as it was.
    expect(wrapper.find('[data-testid="feed-save-unanswered_inquiry"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="feed-fresh-hours"]').exists()).toBe(true)
  })
})

describe('TodayFeedsView preview (SLICE_011d §4, §6)', () => {
  it('previews for the admin by default and shows the honest empty state', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-preview-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    const previewCall = apiFetchMock.mock.calls.find(([p]) => p === '/organization/today-feeds/unanswered_inquiry/preview')!
    const body = JSON.parse((previewCall[1] as RequestInit).body as string)
    expect(body.subject_user_id).toBe(ADMIN_ID)
    expect(document.body.textContent).toContain('No matches')
    expect(document.body.textContent).toContain('Admin Alice')
  })

  it('re-previews for a chosen member through the picker', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-preview-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    // The Dialog is teleported to document.body, outside the wrapper's own
    // root element, so it must be queried there (matches FilterBar.test.ts's
    // `body()` pattern for its own teleported Popover).
    const select = new DOMWrapper(document.body).get('[data-testid="preview-subject"]').findComponent(Select)
    await select.vm.$emit('update:model-value', BOB_ID)
    await flushPromises()

    const previewCalls = apiFetchMock.mock.calls.filter(([p]) => p === '/organization/today-feeds/unanswered_inquiry/preview')
    expect(previewCalls).toHaveLength(2)
    const secondBody = JSON.parse((previewCalls[1][1] as RequestInit).body as string)
    expect(secondBody.subject_user_id).toBe(BOB_ID)
  })
})

describe('TodayFeedsView revert and turn off/on confirmations', () => {
  it('confirms Revert with a plain dialog naming the feed', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-revert-client_replied"]').trigger('click')
    await flushPromises()
    expect(document.body.textContent).toContain('Client replied')
    expect(apiFetchMock.mock.calls.some(([p]) => p === '/organization/today-feeds/client_replied/revert')).toBe(false)

    dialogButton('Revert')?.click()
    await flushPromises()
    expect(apiFetchMock.mock.calls.some(([p]) => p === '/organization/today-feeds/client_replied/revert')).toBe(true)
  })

  it('confirms Turn off for the client-replied feed with a plain (non-typed) dialog', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-turn-off-client_replied"]').trigger('click')
    await flushPromises()
    expect(document.body.querySelector('[data-testid="typed-confirm-input"]')).toBeNull()

    dialogButton('Turn off')?.click()
    await flushPromises()
    const call = apiFetchMock.mock.calls.find(([p]) => p === '/organization/today-feeds/client_replied/enabled')!
    expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ expected_revision: 1, enabled: false })
  })

  it('requires typing the exact feed name before Turn off is enabled for unanswered_inquiry', async () => {
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-turn-off-unanswered_inquiry"]').trigger('click')
    await flushPromises()

    const submit = document.body.querySelector<HTMLButtonElement>('[data-testid="typed-confirm-submit"]')!
    expect(submit.disabled).toBe(true)

    const input = document.body.querySelector<HTMLInputElement>('[data-testid="typed-confirm-input"]')!
    input.value = 'Unanswered'
    input.dispatchEvent(new Event('input'))
    await flushPromises()
    expect(submit.disabled).toBe(true)

    input.value = 'Unanswered inquiry'
    input.dispatchEvent(new Event('input'))
    await flushPromises()
    expect(submit.disabled).toBe(false)

    submit.click()
    await flushPromises()
    const call = apiFetchMock.mock.calls.find(([p]) => p === '/organization/today-feeds/unanswered_inquiry/enabled')!
    expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ expected_revision: 1, enabled: false })
  })

  it('turns a feed back on without a confirmation dialog', async () => {
    stub({ feeds: () => baseFeeds({ client_replied: { enabled: false } }) })
    const wrapper = await mountView()
    await wrapper.get('[data-testid="feed-turn-on-client_replied"]').trigger('click')
    await flushPromises()

    const call = apiFetchMock.mock.calls.find(([p]) => p === '/organization/today-feeds/client_replied/enabled')!
    expect(JSON.parse((call[1] as RequestInit).body as string)).toEqual({ expected_revision: 1, enabled: true })
  })
})
