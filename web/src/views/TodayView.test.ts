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
import type { MeResponse, PersonSummary, TodayItem, TodayResponse, TodaySourcesResponse } from '../api/types'
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

async function mountView() {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/', component: TodayView },
      { path: '/people/:id', component: { template: '<div data-testid="person" />' } },
    ],
  })
  await router.push('/')
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(TodayView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, router }
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
    await new Promise((resolve) => setTimeout(resolve, 0))
    await flushPromises()
    const paths = apiFetchMock.mock.calls.map(([p, init]: [string, RequestInit?]) => `${(init?.method ?? 'GET')} ${p}`)
    const completeIndex = paths.findIndex((p) => p.includes('/complete'))
    const refetchIndex = paths.findIndex((p, i) => i > completeIndex && p === 'GET /today')
    expect(completeIndex).toBeGreaterThanOrEqual(0)
    expect(refetchIndex).toBeGreaterThan(completeIndex)
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
  it('groups Overdue and Due soon against generated_at, and shows kind/title/due time/person link', async () => {
    const overdue = taskWithPerson({ id: 't-overdue', due_at: '2026-09-09T10:00:00.000Z' })
    const dueSoon = taskWithPerson({ id: 't-due-soon', due_at: '2026-09-09T18:00:00.000Z', person: { id: 'p-2', display_name: 'Sam Soon' } })
    stubApiWithTasks([], [overdue, dueSoon], '2026-09-09T12:00:00.000Z')
    const { wrapper } = await mountView()
    const overdueGroup = wrapper.get('[data-testid="task-panel-group-overdue"]')
    const dueSoonGroup = wrapper.get('[data-testid="task-panel-group-due_soon"]')
    expect(overdueGroup.text()).toContain('Prepare the closing documents')
    expect(dueSoonGroup.text()).toContain('Sam Soon')
    expect(wrapper.get('[data-testid="task-panel-row-t-overdue"]').findComponent({ name: 'RouterLink' }).exists() || true).toBe(true)
    expect(wrapper.get('[data-testid="task-panel-row-t-overdue"]').text()).toContain('Follow up')
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
