// SLICE_006c §5a (D-033): the Today "outcome needed" tier — a `low` item
// renders "Outcome needed" / "Low" / "Set outcome" with the call's relative
// time, and its action navigates to the Person page with `?outcome=<call_id>`
// (which opens the Set-outcome dialog there — PersonDetailView.test.ts). A
// Person qualifying by Inquiry keeps its tier and action with the reason
// appended. Server order is rendered as served (`low` arrives last).
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../api/client'
import type { MeResponse, PersonSummary, TodayItem, TodayResponse } from '../api/types'
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

function stubApi(items: TodayItem[]) {
  apiFetchMock.mockImplementation(async (path: string) => {
    if (path === '/me') return me()
    if (path === '/today') return { generated_at: ENDED_AT, items, truncated: false } satisfies TodayResponse
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
