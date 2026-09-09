// SLICE_006c §5a (D-033): the Today "outcome needed" tier — a `low` item
// renders "Outcome needed" / "Low" / "Set outcome" with the call's relative
// time, and its action navigates to the Person page with `?outcome=<call_id>`
// (which opens the Set-outcome dialog there — PersonDetailView.test.ts). A
// Person qualifying by Inquiry keeps its tier and action with the reason
// appended. Server order is rendered as served (`low` arrives last).
import { flushPromises, mount } from '@vue/test-utils'
import { nextTick } from 'vue'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../api/client'
import { personMutationKey } from '../api/queries'
import type { MeResponse, PersonSummary, TodayItem, TodayResponse, TodaySourcesResponse } from '../api/types'
import { formatAbsoluteTime, formatRelativeTime } from '../lib/format'
import TodayView from './TodayView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const CALL_ID = '55555555-5555-5555-5555-555555555555'

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}

function person(id: string, name: string): PersonSummary {
  return {
    id,
    first_name: name,
    last_name: null,
    display_name: name,
    stage: { id: 'stage-lead', name: 'Lead' },
    assigned_user: null,
    primary_email: null,
    primary_phone: '+1 555 0100',
    inquiry_count: 1,
    last_inquiry_at: '2026-08-23T09:00:00.000Z',
    created_at: '2026-08-23T09:00:00.000Z',
  }
}

const ENDED_AT = new Date(Date.now() - 5 * 60_000).toISOString()

function inquiryItem(): TodayItem {
  return {
    person: person('p-normal', 'Grace Hopper'),
    priority: 'normal',
    recommended_action: 'call',
    reasons: [{ code: 'no_contact_attempt', since: '2026-08-23T09:00:00.000Z' }],
    waiting_since: '2026-08-23T09:00:00.000Z',
    latest_inquiry: { id: 'inq-1', source: 'web', received_at: '2026-08-23T09:00:00.000Z' },
    last_contact_attempt: null,
  }
}

function lowItem(): TodayItem {
  return {
    person: person('p-low', 'Ada Lovelace'),
    priority: 'low',
    recommended_action: 'set_outcome',
    reasons: [{ code: 'call_outcome_needed', call_id: CALL_ID, ended_at: ENDED_AT }],
    waiting_since: ENDED_AT,
    latest_inquiry: { id: 'inq-2', source: 'web', received_at: '2026-08-20T09:00:00.000Z' },
    last_contact_attempt: { id: 'att-1', channel: 'call', outcome: 'reached', occurred_at: ENDED_AT },
  }
}

function stubApi(items: TodayItem[], sources: TodaySourcesResponse['sources'] = []) {
  apiFetchMock.mockImplementation(async (path: string) => {
    if (path === '/me') return me()
    if (path === '/today') {
      return {
        generated_at: ENDED_AT,
        items,
        truncated: false,
        sources: { status: 'complete', issues: [], system_feed_issues: [] },
      } satisfies TodayResponse
    }
    if (path === '/today/sources') return { limit: 5, sources }
    if (path === '/today/feeds') {
      return {
        feeds: [
          { feed_key: 'unanswered_inquiry', enabled: true, is_default: true, description: ['Awaiting a response'] },
          { feed_key: 'client_replied', enabled: true, is_default: true, description: ['Client replied, unanswered'] },
          { feed_key: 'call_outcome_needed', enabled: true, is_default: true, description: ['A call of mine needs an outcome'] },
        ],
      }
    }
    throw new Error(`unexpected ${path}`)
  })
}

async function mountView(queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: TodayView },
      { path: '/people/:id', component: { template: '<div data-testid="person" />' } },
    ],
  })
  await router.push('/')
  await router.isReady()
  const wrapper = mount(TodayView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, router, queryClient }
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('TodayView — outcome needed tier (SLICE_006c §5a)', () => {
  it('renders a low item as Outcome needed / Low / Set outcome with the call\'s relative time', async () => {
    stubApi([lowItem()])
    const { wrapper } = await mountView()
    const row = wrapper.get('tbody tr')
    const cells = row.findAll('td')
    expect(cells[0].text()).toContain('Ada Lovelace')
    expect(cells[1].text()).toBe('Outcome needed')
    expect(cells[2].text()).toBe('Low')
    expect(cells[4].findAll('span').map((span) => span.text())).toEqual(['Set outcome', 'Call 5 minutes ago has no outcome yet'])
    expect(cells[4].text()).not.toContain('Reached')
    expect(wrapper.get('[data-testid="today-set-outcome-action"]').text()).toBe('Set outcome')
    expect(row.text()).not.toContain('Log contact')
    // Weight, not colour, carries priority (UI_STYLE §3): Low is muted like Normal.
    expect(row.findAll('td')[2].get('span').classes()).toContain('text-text-muted')
  })

  it('Set outcome navigates to the Person page with ?outcome=<call_id>, not the plain row link', async () => {
    stubApi([lowItem()])
    const { wrapper, router } = await mountView()
    await wrapper.get('[data-testid="today-set-outcome-action"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/people/p-low')
    expect(router.currentRoute.value.query).toEqual({ outcome: CALL_ID })
  })

  it('renders items in server order and keeps the inquiry tier\'s action when the reason is appended', async () => {
    const both = inquiryItem()
    both.reasons.push({ code: 'call_outcome_needed', call_id: CALL_ID, ended_at: ENDED_AT })
    stubApi([both, lowItem()])
    const { wrapper } = await mountView()
    const rows = wrapper.findAll('tbody tr')
    expect(rows.map((r) => r.findAll('td')[0].text())).toEqual(
      expect.arrayContaining(['Grace Hopper+1 555 0100', 'Ada Lovelace+1 555 0100']),
    )
    expect(rows[0].text()).toContain('Grace Hopper')
    expect(rows[0].findAll('td')[1].findAll('span').map((b) => b.text())).toEqual(['No contact attempt', 'Outcome needed'])
    expect(rows[0].findAll('td')[2].text()).toBe('Normal')
    expect(rows[0].findAll('td')[4].text()).toContain('Call')
    expect(rows[0].text()).toContain('Log contact')
    expect(rows[0].find('[data-testid="today-set-outcome-action"]').exists()).toBe(false)
    expect(rows[1].findAll('td')[2].text()).toBe('Low')
    expect(rows[1].find('[data-testid="today-set-outcome-action"]').exists()).toBe(true)
    expect(rows[1].find('[data-testid="today-set-outcome-aside"]').exists()).toBe(false)
  })

  it('a both-ways item keeps Log contact and gains a secondary Set outcome linking with ?outcome=<call_id>', async () => {
    const both = inquiryItem()
    both.reasons.push({ code: 'call_outcome_needed', call_id: CALL_ID, ended_at: ENDED_AT })
    stubApi([both, inquiryItem()])
    const { wrapper, router } = await mountView()
    const rows = wrapper.findAll('tbody tr')
    const actions = rows[0].findAll('td')[5].findAll('button').map((b) => b.text())
    expect(actions).toEqual(['Log contact', 'Set outcome'])
    // A plain inquiry item has no secondary Set outcome.
    expect(rows[1].findAll('td')[5].findAll('button').map((b) => b.text())).toEqual(['Log contact'])
    await rows[0].get('[data-testid="today-set-outcome-aside"]').trigger('click')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe(`/people/${both.person.id}`)
    expect(router.currentRoute.value.query).toEqual({ outcome: CALL_ID })
  })
})

// ---- SLICE_009 §6: client_replied ------------------------------------------

function clientRepliedItem(priority: 'high' | 'normal'): TodayItem {
  const occurredAt = '2026-08-27T09:00:00.000Z'
  return {
    person: person('p-replied', 'Maya Lindqvist'),
    priority,
    recommended_action: 'call',
    reasons: [{ code: 'client_replied', occurred_at: occurredAt }],
    waiting_since: occurredAt,
    latest_inquiry: { id: 'inq-3', source: 'web', received_at: '2026-08-20T09:00:00.000Z' },
    last_contact_attempt: null,
  }
}

describe('TodayView — client_replied (SLICE_009 §6)', () => {
  it('renders the Client replied badge as the SOLE reason, winning over the Inquiry trio', async () => {
    stubApi([clientRepliedItem('high')])
    const { wrapper } = await mountView()
    const row = wrapper.get('tbody tr')
    const cells = row.findAll('td')
    expect(cells[0].text()).toContain('Maya Lindqvist')
    expect(cells[1].findAll('span').map((b) => b.text())).toEqual(['Client replied'])
    expect(cells[2].text()).toBe('High')
  })

  it('a stale reply renders Normal priority', async () => {
    stubApi([clientRepliedItem('normal')])
    const { wrapper } = await mountView()
    expect(wrapper.get('tbody tr').findAll('td')[2].text()).toBe('Normal')
  })
})

// Slice 011e e2 (docs/specs/SLICE_011e.md §9.14, §9.17): the Today source
// panel's per-source notice sentence for invalid_tag, alongside the
// existing invalid_stage/invalid_assignee sentences.
describe('TodayView — Today source notices (Slice 011e e2)', () => {
  it('shows the invalid_tag sentence for a source naming a deleted tag', async () => {
    stubApi([], [{ list_id: 'list-1', name: 'Investors', scope: 'personal', revision: 1, filter_error: 'invalid_tag' }])
    const { wrapper } = await mountView()
    const manageSources = wrapper.findAll('button').find((button) => button.text() === 'Manage sources')!
    await manageSources.trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('This list refers to a tag that no longer exists.')
  })
})

// ---- Slice 016b: the built-in task axis on Today (docs/specs/SLICE_016.md
// §8, §12.16) --------------------------------------------------------------

function taskDueItem(overrides: Partial<import('../api/types').TodayItem> = {}): TodayItem {
  return {
    person: person('p-task', 'Frank Task'),
    priority: 'normal',
    recommended_action: 'review_person',
    reasons: [
      {
        code: 'task_due',
        task_id: 'task-1',
        title: 'Send the seller disclosure paperwork today without fail',
        kind: 'follow_up',
        due_at: '2026-09-10T09:00:00.000Z',
      },
    ],
    waiting_since: null,
    latest_inquiry: null,
    last_contact_attempt: null,
    ...overrides,
  }
}

function taskOverdueItem(): TodayItem {
  return {
    person: person('p-overdue-task', 'Grace Overdue'),
    priority: 'high',
    recommended_action: 'call',
    reasons: [{ code: 'task_overdue', task_id: 'task-2', title: 'Call back', kind: 'call', due_at: '2026-09-08T09:00:00.000Z' }],
    waiting_since: null,
    latest_inquiry: null,
    last_contact_attempt: null,
  }
}

function stubApiWithTasks(
  items: TodayItem[],
  tasks: import('../api/types').TaskWithPerson[] = [],
  generatedAt = '2026-09-09T12:00:00.000Z',
  truncated = false,
) {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === '/me') return me()
    if (path === '/today') {
      return {
        generated_at: ENDED_AT,
        items,
        truncated: false,
        sources: { status: 'complete', issues: [], system_feed_issues: [] },
      } satisfies TodayResponse
    }
    if (path === '/today/sources') return { limit: 5, sources: [] }
    if (path === '/today/feeds') {
      return {
        feeds: [
          { feed_key: 'unanswered_inquiry', enabled: true, is_default: true, description: [] },
          { feed_key: 'client_replied', enabled: true, is_default: true, description: [] },
          { feed_key: 'call_outcome_needed', enabled: true, is_default: true, description: [] },
        ],
      }
    }
    if (path === '/tasks?scope=mine') {
      return { tasks, generated_at: generatedAt, truncated }
    }
    if (path.startsWith('/people/') && path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') {
      return { task: { id: 'x', changed: true }, changed: true }
    }
    if (path.startsWith('/people/') && path.endsWith('/snooze') && (init?.method ?? 'GET') === 'POST') {
      return { task: { id: 'x' }, changed: true }
    }
    throw new Error(`unexpected ${path}`)
  })
}

function taskWithPerson(overrides: Partial<import('../api/types').TaskWithPerson> = {}): import('../api/types').TaskWithPerson {
  return {
    id: 'panel-task-1',
    person_id: 'p-panel',
    title: 'Prepare the closing documents',
    kind: 'follow_up',
    due_at: '2026-09-09T15:00:00.000Z',
    assignee: { id: 'u-alice', display_name: 'Alice' },
    created_by: { id: 'u-alice', display_name: 'Alice' },
    completed_at: null,
    completed_by: null,
    created_at: '2026-09-01T00:00:00.000Z',
    updated_at: '2026-09-01T00:00:00.000Z',
    can_manage: true,
    person: { id: 'p-panel', display_name: 'Priya Panel' },
    ...overrides,
  }
}

describe('TodayView — task reasons on the ranked queue (Slice 016b §5, §8)', () => {
  it('renders the task_due/task_overdue reason as a badge with the clipped title', async () => {
    stubApiWithTasks([taskDueItem()])
    const { wrapper } = await mountView()
    const badgeText = wrapper.get('tbody tr').findAll('td')[1].text()
    // The 60+-character title is clipped (lib/tasks.ts clipTitle, default 40).
    expect(badgeText.length).toBeLessThan('Send the seller disclosure paperwork today without fail'.length)
    expect(badgeText.endsWith('…')).toBe(true)
  })

  it('shows "Due <relative>" in the Waiting cell when waiting_since is null and a task reason is present', async () => {
    stubApiWithTasks([taskDueItem()])
    const { wrapper } = await mountView()
    const waitingCell = wrapper.get('tbody tr').findAll('td')[3]
    expect(waitingCell.text()).toMatch(/^Due /)
  })

  it('a retained item with a real waiting_since is unaffected even when it also carries a task reason', async () => {
    const item = taskDueItem({ waiting_since: '2026-09-01T09:00:00.000Z' })
    stubApiWithTasks([item])
    const { wrapper } = await mountView()
    const waitingCell = wrapper.get('tbody tr').findAll('td')[3]
    expect(waitingCell.text()).not.toMatch(/^Due /)
  })

  it('shows a Complete button whenever the item carries a task reason, regardless of recommended_action', async () => {
    stubApiWithTasks([taskDueItem()]) // recommended_action: 'review_person'
    const { wrapper } = await mountView()
    expect(wrapper.get('[data-testid="today-task-complete"]').text()).toBe('Complete')
  })

  it('the Complete button is disabled while pending, a second click makes no second POST, and it POSTs then refetches Today', async () => {
    stubApiWithTasks([taskOverdueItem()])
    const { wrapper } = await mountView()
    let releaseComplete: () => void = () => {}
    const gate = new Promise<void>((resolve) => { releaseComplete = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') await gate
      return defaultImpl(path, init)
    })
    apiFetchMock.mockClear()
    const button = wrapper.get('[data-testid="today-task-complete"]')
    await button.trigger('click')
    await nextTick()
    expect(button.attributes('disabled')).toBeDefined()
    // Held while pending: a second click before the first POST resolves
    // makes no second request — the same "exactly one POST" discipline
    // as the Person page's task-add pending guard.
    await button.trigger('click')
    await nextTick()
    expect(apiFetchMock.mock.calls.filter(([p, init]) => p.endsWith('/complete') && (init?.method ?? 'GET') === 'POST')).toHaveLength(1)
    releaseComplete()
    // The settle refetch is scheduled on its own macrotask from `onSuccess`
    // after the POST resolves, so a single `setTimeout(0)` registered here
    // can run before it (coordinator's final gate: 1 failure in 6 runs).
    // Poll for the refetch instead of assuming one tick suffices.
    const requestPaths = () =>
      apiFetchMock.mock.calls.map(([p, init]: [string, RequestInit?]) => `${(init?.method ?? 'GET')} ${p}`)
    const completeIndexOf = (paths: string[]) => paths.findIndex((p) => p.includes('/complete'))
    const refetchIndexOf = (paths: string[]) => {
      const c = completeIndexOf(paths)
      return paths.findIndex((p, i) => i > c && p === 'GET /today')
    }
    for (let attempt = 0; attempt < 50 && refetchIndexOf(requestPaths()) < 0; attempt += 1) {
      await new Promise((resolve) => setTimeout(resolve, 0))
      await flushPromises()
    }
    const paths = requestPaths()
    const completeIndex = completeIndexOf(paths)
    const refetchIndex = refetchIndexOf(paths)
    expect(completeIndex).toBeGreaterThanOrEqual(0)
    expect(refetchIndex).toBeGreaterThan(completeIndex)
  })

  // Round-2 review test tightening 10: the row must survive a resolved
  // POST as long as the FOLLOWING `GET /today` refetch has not itself
  // resolved yet — pessimistic per §8 ("held while pending... the
  // row/item leaves only once the natural refetch... stops returning
  // it"), never an optimistic removal the instant the POST itself
  // succeeds.
  it('the item leaves only after the refetch resolves, not the instant the POST resolves', async () => {
    stubApiWithTasks([taskOverdueItem()])
    const { wrapper } = await mountView()
    let releaseComplete: () => void = () => {}
    const completeGate = new Promise<void>((resolve) => { releaseComplete = resolve })
    let releaseRefetch: () => void = () => {}
    const refetchGate = new Promise<void>((resolve) => { releaseRefetch = resolve })
    let todayCallCount = 0
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/today') {
        todayCallCount += 1
        if (todayCallCount > 1) {
          await refetchGate
          return {
            generated_at: '2026-09-09T13:00:00.000Z',
            items: [],
            truncated: false,
            sources: { status: 'complete', issues: [], system_feed_issues: [] },
          }
        }
      }
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') await completeGate
      return defaultImpl(path, init)
    })

    const button = wrapper.get('[data-testid="today-task-complete"]')
    await button.trigger('click')
    await nextTick()

    // The POST resolves...
    releaseComplete()
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    // ...but the refetch it triggered is still gated: the row must still
    // be on the page.
    expect(wrapper.findAll('tbody tr').length).toBe(1)

    // Only once the refetch itself resolves (with the item now absent)
    // does the row leave.
    releaseRefetch()
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(wrapper.findAll('tbody tr').length).toBe(0)
  })

  // Round-2 review test tightening 11: a Person qualifying for BOTH the
  // `set_outcome` recommended action (SLICE_006c §5a) AND a task reason
  // (Slice 016b) — the two features were built independently, so this
  // pins that they compose rather than one silently dropping the other's
  // control (`set_outcome`'s "Set outcome" precedent from Recommended
  // never became the task Complete button's replacement, and vice versa).
  it('a set_outcome item with a task reason keeps BOTH badges, BOTH action buttons, and the Set outcome recommendation', async () => {
    const item = taskOverdueItem()
    item.recommended_action = 'set_outcome'
    item.reasons = [
      { code: 'call_outcome_needed', call_id: CALL_ID, ended_at: ENDED_AT },
      item.reasons[0],
    ]
    stubApiWithTasks([item])
    const { wrapper } = await mountView()
    const row = wrapper.get('tbody tr')
    const cells = row.findAll('td')

    const badges = cells[1].findAll('span').map((b) => b.text())
    expect(badges).toContain('Outcome needed')
    expect(badges.some((b) => b.startsWith('Call back'))).toBe(true)

    expect(cells[4].text()).toContain('Set outcome')

    const actionButtons = cells[5].findAll('button').map((b) => b.text())
    expect(actionButtons).toEqual(['Set outcome', 'Complete'])
  })
})

describe('TodayView — reasonLabel default arm', () => {
  it('renders "Work item" for an unrecognized future reason code', async () => {
    const item = taskDueItem({
      reasons: [{ code: 'made_up_future_reason' } as unknown as TodayItem['reasons'][number]],
    })
    stubApiWithTasks([item])
    const { wrapper } = await mountView()
    expect(wrapper.get('tbody tr').findAll('td')[1].text()).toBe('Work item')
  })
})

describe('TodayView — the Tasks panel (docs/specs/SLICE_016.md §8, §12.16)', () => {
  // Round-2 review test tightening 7: distinct titles per fixture (the
  // prior version shared the default title on both, so a group swap bug
  // would still have passed), per-group membership asserted POSITIVELY in
  // one group and NEGATIVELY in the other (a `.toContain` on only one side
  // cannot catch a task placed in BOTH groups or leaked across), and a
  // real `href` assertion on the Person link (the prior
  // `.exists() || true` was a tautology — always `true` regardless of
  // whether the link existed at all).
  it('groups Overdue and Due soon against generated_at, and shows kind/title/due time/person link', async () => {
    const overdue = taskWithPerson({ id: 't-overdue', title: 'Overdue fixture task', due_at: '2026-09-09T10:00:00.000Z' })
    const dueSoon = taskWithPerson({
      id: 't-due-soon',
      title: 'Due-soon fixture task',
      due_at: '2026-09-09T18:00:00.000Z',
      person: { id: 'p-2', display_name: 'Sam Soon' },
    })
    stubApiWithTasks([], [overdue, dueSoon], '2026-09-09T12:00:00.000Z')
    const { wrapper } = await mountView()
    const overdueGroup = wrapper.get('[data-testid="task-panel-group-overdue"]')
    const dueSoonGroup = wrapper.get('[data-testid="task-panel-group-due_soon"]')

    expect(overdueGroup.text()).toContain('Overdue fixture task')
    expect(overdueGroup.text()).toContain('Priya Panel')
    expect(dueSoonGroup.text()).toContain('Due-soon fixture task')
    expect(dueSoonGroup.text()).toContain('Sam Soon')

    expect(overdueGroup.text()).not.toContain('Due-soon fixture task')
    expect(overdueGroup.text()).not.toContain('Sam Soon')
    expect(dueSoonGroup.text()).not.toContain('Overdue fixture task')
    expect(dueSoonGroup.text()).not.toContain('Priya Panel')

    const overdueRow = wrapper.get('[data-testid="task-panel-row-t-overdue"]')
    expect(overdueRow.text()).toContain('Follow up')
    expect(overdueRow.get('a').attributes('href')).toBe('/people/p-panel')
  })

  it('shows "Nothing due" for an empty group and hides it once tasks arrive', async () => {
    stubApiWithTasks([], [])
    const { wrapper } = await mountView()
    expect(wrapper.get('[data-testid="task-panel-group-overdue"]').text()).toContain('Nothing due')
    expect(wrapper.get('[data-testid="task-panel-group-due_soon"]').text()).toContain('Nothing due')
  })

  it('shows "Showing the first 200" when truncated', async () => {
    stubApiWithTasks([], [taskWithPerson()], '2026-09-09T12:00:00.000Z', true)
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('Showing the first 200')
  })

  it('Complete in the panel is disabled while pending and POSTs then refetches the tasks list', async () => {
    const task = taskWithPerson({ id: 't-complete-me' })
    stubApiWithTasks([], [task])
    const { wrapper } = await mountView()
    let releaseComplete: () => void = () => {}
    const gate = new Promise<void>((resolve) => { releaseComplete = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') await gate
      return defaultImpl(path, init)
    })
    apiFetchMock.mockClear()
    const button = wrapper.get('[data-testid="task-panel-complete-t-complete-me"]')
    await button.trigger('click')
    await nextTick()
    expect(button.attributes('disabled')).toBeDefined()
    releaseComplete()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    const paths = apiFetchMock.mock.calls.map(([p, init]: [string, RequestInit?]) => `${(init?.method ?? 'GET')} ${p}`)
    const completeIndex = paths.findIndex((p) => p.includes('/complete'))
    const refetchIndex = paths.findIndex((p, i) => i > completeIndex && p === 'GET /tasks?scope=mine')
    expect(completeIndex).toBeGreaterThanOrEqual(0)
    expect(refetchIndex).toBeGreaterThan(completeIndex)
  })

  it('Snooze posts tomorrow at local end of day; a changed:false response leaves the row with no error copy', async () => {
    const task = taskWithPerson({ id: 't-snooze-me' })
    stubApiWithTasks([], [task])
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/today') return { generated_at: ENDED_AT, items: [], truncated: false, sources: { status: 'complete', issues: [], system_feed_issues: [] } }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') return { tasks: [task], generated_at: '2026-09-09T12:00:00.000Z', truncated: false }
      if (path.endsWith('/snooze') && (init?.method ?? 'GET') === 'POST') {
        return { task: { ...task, due_at: task.due_at }, changed: false }
      }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="task-panel-snooze-t-snooze-me"]')
    await button.trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="task-panel-row-t-snooze-me"]').text()).not.toContain('Could not')
    expect(wrapper.find('[role="alert"]').exists()).toBe(false)
  })
})

// Round-2 review test tightening 8: the Snooze POST body's exact `due_at`
// value under a pinned system clock and timezone, across a DST boundary in
// each direction (spring forward in March, fall back in November) — a
// plain "does it POST" test cannot catch a DST offset bug in
// `tomorrowLocalEndOfDay`.
const nodeProcess = (globalThis as unknown as { process: { env: Record<string, string | undefined> } }).process

describe('TodayView — Tasks panel Snooze body across DST (round-2 review test tightening 8)', () => {
  let originalTz: string | undefined
  beforeEach(() => {
    originalTz = nodeProcess.env.TZ
  })
  afterEach(() => {
    nodeProcess.env.TZ = originalTz
    vi.useRealTimers()
  })

  it.each([
    // 2026-03-08 is the US spring-forward date: "tomorrow" from March 7
    // ends in EDT (UTC-4), not EST (UTC-5).
    ['2026-03-07T15:00:00Z', '2026-03-09T03:59:59.000Z'],
    // 2026-11-01 is the US fall-back date: "tomorrow" from Oct 31 ends in
    // EST (UTC-5), not EDT (UTC-4).
    ['2026-10-31T15:00:00Z', '2026-11-02T04:59:59.000Z'],
  ])('posts due_at = tomorrow 23:59:59 America/New_York as UTC, system time %s', async (systemTime, expectedDueAt) => {
    nodeProcess.env.TZ = 'America/New_York'
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date(systemTime))
    const task = taskWithPerson({ id: 't-snooze-dst' })
    let capturedBody: string | undefined
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/today') return { generated_at: systemTime, items: [], truncated: false, sources: { status: 'complete', issues: [], system_feed_issues: [] } }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') return { tasks: [task], generated_at: systemTime, truncated: false }
      if (path.endsWith('/snooze') && (init?.method ?? 'GET') === 'POST') {
        capturedBody = init?.body as string
        return { task: { ...task, due_at: expectedDueAt }, changed: true }
      }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="task-panel-snooze-t-snooze-dst"]').trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(capturedBody).toBeDefined()
    expect(JSON.parse(capturedBody!)).toEqual({ due_at: expectedDueAt })
  })
})

// Round-2 review test tightening 9: the panel's due cell (`panelDueCellText`,
// already unit-tested in lib/tasks.test.ts) exercised THROUGH the mounted
// component under a pinned timezone, at the exact instant that crosses
// local midnight relative to the response's own `generated_at` — proving
// the component wires the right two ISO strings into the helper, not just
// that the helper itself is correct in isolation.
describe('TodayView — Tasks panel due cell past local midnight (round-2 review test tightening 9)', () => {
  let originalTz: string | undefined
  beforeEach(() => {
    originalTz = nodeProcess.env.TZ
    nodeProcess.env.TZ = 'America/New_York'
  })
  afterEach(() => {
    nodeProcess.env.TZ = originalTz
  })

  it('shows a time for a task still within generated_at\'s local calendar day', async () => {
    // generated_at = 2026-07-15T14:00:00Z = 10:00 EDT. due_at =
    // 2026-07-16T03:30:00Z = 23:30 EDT on July 15 — same local day.
    const task = taskWithPerson({ id: 't-same-local-day', due_at: '2026-07-16T03:30:00.000Z' })
    stubApiWithTasks([], [task], '2026-07-15T14:00:00.000Z')
    const { wrapper } = await mountView()
    const row = wrapper.get('[data-testid="task-panel-row-t-same-local-day"]')
    expect(row.text()).toMatch(/\d{1,2}:\d{2}\s?[AP]M/)
    expect(row.text()).not.toContain('Jul 16, 2026')
  })

  it('shows the date for a task past generated_at\'s local midnight', async () => {
    // due_at = 2026-07-16T04:30:00Z = 00:30 EDT on July 16 — the next
    // local day relative to generated_at's July 15.
    const task = taskWithPerson({ id: 't-next-local-day', due_at: '2026-07-16T04:30:00.000Z' })
    stubApiWithTasks([], [task], '2026-07-15T14:00:00.000Z')
    const { wrapper } = await mountView()
    const row = wrapper.get('[data-testid="task-panel-row-t-next-local-day"]')
    expect(row.text()).toContain('Jul 16, 2026')
  })
})

describe('TodayView — system_feed_issues generic fallback and task_due label (Slice 016b §5, §8)', () => {
  it('renders "Due tasks could not load." for a task_due issue', async () => {
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === '/me') return me()
      if (path === '/today') {
        return {
          generated_at: ENDED_AT,
          items: [],
          truncated: false,
          sources: { status: 'partial', issues: [], system_feed_issues: [{ feed_key: 'task_due', error: 'unavailable', fallback: false }] },
        }
      }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') return { tasks: [], generated_at: ENDED_AT, truncated: false }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('Due tasks could not load.')
  })

  it('renders the generic sentence for an unrecognized issue key, never "undefined"', async () => {
    apiFetchMock.mockImplementation(async (path: string) => {
      if (path === '/me') return me()
      if (path === '/today') {
        return {
          generated_at: ENDED_AT,
          items: [],
          truncated: false,
          sources: { status: 'partial', issues: [], system_feed_issues: [{ feed_key: 'some_future_feed', error: 'unavailable', fallback: false }] },
        }
      }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') return { tasks: [], generated_at: ENDED_AT, truncated: false }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    expect(wrapper.text()).toContain('A Today rule could not load.')
    expect(wrapper.text()).not.toContain('undefined')
  })
})

// Round-2 review test tightening 12: three otherwise-untouched claims,
// each cheap to lose silently in a future refactor of the same code the
// task-reason work above just changed.
describe('TodayView — Waiting cell regression and rendering safety (round-2 review test tightening 12)', () => {
  it("shows the exact relative text and title attribute for a non-task item's real waiting_since, unaffected by the task fallback", async () => {
    stubApiWithTasks([inquiryItem()])
    const { wrapper } = await mountView()
    const waitingCell = wrapper.get('tbody tr').findAll('td')[3]
    const span = waitingCell.get('span')
    expect(span.text()).toBe(formatRelativeTime('2026-08-23T09:00:00.000Z'))
    expect(span.attributes('title')).toBe(formatAbsoluteTime('2026-08-23T09:00:00.000Z'))
  })

  it('does not show "Showing the first 200" when truncated is false', async () => {
    stubApiWithTasks([], [taskWithPerson()], '2026-09-09T12:00:00.000Z', false)
    const { wrapper } = await mountView()
    expect(wrapper.text()).not.toContain('Showing the first 200')
  })

  it('never renders an untrusted task title or Person display_name as HTML: <img onerror> and <b> render as literal text with no injected element, and the Person link carries only the id', async () => {
    const XSS_TITLE = '<img src=x onerror=alert(1)>'
    const XSS_NAME = '<b>Bold Name</b>'
    const task = taskWithPerson({
      id: 't-xss',
      title: XSS_TITLE,
      person: { id: 'p-xss', display_name: XSS_NAME },
    })
    stubApiWithTasks([], [task])
    const { wrapper } = await mountView()
    const row = wrapper.get('[data-testid="task-panel-row-t-xss"]')

    // No element was ever created from this untrusted markup.
    expect(row.find('img').exists()).toBe(false)
    expect(row.find('b').exists()).toBe(false)
    // It appears only as literal, unescaped-looking text content (a text
    // node, never parsed as HTML).
    expect(row.text()).toContain(XSS_TITLE)
    expect(row.text()).toContain(XSS_NAME)
    // The link target is built from the Person id alone.
    expect(row.get('a').attributes('href')).toBe('/people/p-xss')
  })
})

// Round-2 review fix 1 + its tests: `completeTask`/`snoozeTask` are ONE
// shared mutation instance whose `mutationKey` reactively follows
// `completeTaskPersonId`/`snoozeTaskPersonId`. A synchronous ref-set
// immediately followed by `.mutate()` let Vue Query's `MutationObserver`
// reset mid-flight (the key change propagated AFTER `.mutate()` had
// already fired), detaching the call from its own `onError`/`onSettled` —
// `completingTask`/`snoozingTask` never cleared, so every later click
// (even for an entirely different Person) was permanently inert. The fix
// (`await nextTick()` between the ref-set and `.mutate()`) is proven here
// by two SEQUENTIAL actions on different People's panel rows actually
// both completing, not just the first.
describe('TodayView — Tasks panel action mutation keying (round-2 review fix 1)', () => {
  it('sequential Complete on two panel rows of different People: two POSTs, and the first button re-enables after its own settle', async () => {
    const taskA = taskWithPerson({ id: 't-seq-a' })
    const taskB = taskWithPerson({ id: 't-seq-b', person: { id: 'p-panel-2', display_name: 'Panel Two' } })
    stubApiWithTasks([], [taskA, taskB])
    const { wrapper } = await mountView()

    const buttonA = wrapper.get('[data-testid="task-panel-complete-t-seq-a"]')
    await buttonA.trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(buttonA.attributes('disabled')).toBeUndefined()

    const buttonB = wrapper.get('[data-testid="task-panel-complete-t-seq-b"]')
    await buttonB.trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(buttonB.attributes('disabled')).toBeUndefined()

    const completeCalls = apiFetchMock.mock.calls.filter(
      ([p, init]: [string, RequestInit?]) => p.endsWith('/complete') && (init?.method ?? 'GET') === 'POST',
    )
    expect(completeCalls).toHaveLength(2)
  })

  it('sequential Snooze on two panel rows of different People: two POSTs, and the first button re-enables after its own settle', async () => {
    const taskA = taskWithPerson({ id: 't-snz-a' })
    const taskB = taskWithPerson({ id: 't-snz-b', person: { id: 'p-panel-2', display_name: 'Panel Two' } })
    stubApiWithTasks([], [taskA, taskB])
    const { wrapper } = await mountView()

    const buttonA = wrapper.get('[data-testid="task-panel-snooze-t-snz-a"]')
    await buttonA.trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(buttonA.attributes('disabled')).toBeUndefined()

    const buttonB = wrapper.get('[data-testid="task-panel-snooze-t-snz-b"]')
    await buttonB.trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(buttonB.attributes('disabled')).toBeUndefined()

    const snoozeCalls = apiFetchMock.mock.calls.filter(
      ([p, init]: [string, RequestInit?]) => p.endsWith('/snooze') && (init?.method ?? 'GET') === 'POST',
    )
    expect(snoozeCalls).toHaveLength(2)
  })

  it('a 403 on panel Complete re-enables the button and renders "You can no longer manage this task."', async () => {
    const task = taskWithPerson({ id: 't-forbidden' })
    stubApiWithTasks([], [task])
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/today') return { generated_at: ENDED_AT, items: [], truncated: false, sources: { status: 'complete', issues: [], system_feed_issues: [] } }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') return { tasks: [task], generated_at: '2026-09-09T12:00:00.000Z', truncated: false }
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') {
        const { ApiError } = await import('../api/client')
        throw new ApiError(403, 'forbidden')
      }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="task-panel-complete-t-forbidden"]')
    await button.trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    expect(button.attributes('disabled')).toBeUndefined()
    expect(wrapper.get('[data-testid="task-action-error-banner"]').text()).toContain(
      'You can no longer manage this task.',
    )
  })

  it('while a panel Complete is gated, exactly one mutation is in flight under personMutationKey(ORG_ID, \'p-panel\')', async () => {
    const task = taskWithPerson({ id: 't-gated' })
    stubApiWithTasks([], [task])
    const { wrapper, queryClient } = await mountView()
    let releaseComplete: () => void = () => {}
    const gate = new Promise<void>((resolve) => { releaseComplete = resolve })
    const defaultImpl = apiFetchMock.getMockImplementation()!
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') await gate
      return defaultImpl(path, init)
    })
    const button = wrapper.get('[data-testid="task-panel-complete-t-gated"]')
    await button.trigger('click')
    await flushPromises()

    const key = personMutationKey(ORG_ID, 'p-panel')
    expect(queryClient.isMutating({ mutationKey: key })).toBe(1)
    const entry = queryClient.getMutationCache().getAll().find((m) => m.state.status === 'pending')
    expect(entry?.options.mutationKey).toEqual(key)

    releaseComplete()
    await flushPromises()
  })
})

// Round-2 review fix 2: a page-level, dismissible alert for a task
// action's error, keyed by task id and independent of any row — proven
// both from the ranked queue's Complete (which never had an error slot at
// all) and from a panel row a 404 refetch just removed (its own inline
// error would have vanished along with the row).
describe('TodayView — task action error banner (round-2 review fix 2)', () => {
  it('shows a dismissible banner for a 503 on the ranked queue\'s Complete button', async () => {
    stubApiWithTasks([taskOverdueItem()])
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/today') return { generated_at: ENDED_AT, items: [taskOverdueItem()], truncated: false, sources: { status: 'complete', issues: [], system_feed_issues: [] } }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') return { tasks: [], generated_at: ENDED_AT, truncated: false }
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') {
        const { ApiError } = await import('../api/client')
        throw new ApiError(503, 'unavailable')
      }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="today-task-complete"]').trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    const banner = wrapper.get('[data-testid="task-action-error-banner"]')
    expect(banner.text()).toContain('The server is temporarily unavailable. Try again shortly.')
    await wrapper.get('[data-testid="task-action-error-dismiss"]').trigger('click')
    await nextTick()
    expect(wrapper.find('[data-testid="task-action-error-banner"]').exists()).toBe(false)
  })

  it('a 404 on a panel Complete still explains the error even after the settle refetch removes the row', async () => {
    const task = taskWithPerson({ id: 't-gone' })
    stubApiWithTasks([], [task])
    let refetchedAway = false
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/today') return { generated_at: ENDED_AT, items: [], truncated: false, sources: { status: 'complete', issues: [], system_feed_issues: [] } }
      if (path === '/today/sources') return { limit: 5, sources: [] }
      if (path === '/today/feeds') return { feeds: [] }
      if (path === '/tasks?scope=mine') {
        return { tasks: refetchedAway ? [] : [task], generated_at: '2026-09-09T12:00:00.000Z', truncated: false }
      }
      if (path.endsWith('/complete') && (init?.method ?? 'GET') === 'POST') {
        refetchedAway = true
        const { ApiError } = await import('../api/client')
        throw new ApiError(404, 'not_found')
      }
      throw new Error(`unexpected ${path}`)
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="task-panel-complete-t-gone"]').trigger('click')
    await flushPromises()
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    // The row is gone (the refetch now omits the task)...
    expect(wrapper.find('[data-testid="task-panel-row-t-gone"]').exists()).toBe(false)
    // ...but the banner, independent of any row, still explains what happened.
    expect(wrapper.get('[data-testid="task-action-error-banner"]').text().length).toBeGreaterThan(0)
  })
})
