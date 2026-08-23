// SLICE_006 §13 item 4: Call disabled without a phone; number picker only
// with ≥ 2 phones; `call_completed` history rendering; one primary per
// view; the Call → panel → Hang up flow against a fake room. Service-free:
// `apiFetch` is mocked, the LiveKit room is a fake injected via the
// `createRoom` prop, and PrimeVue runs unstyled as in main.ts.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type {
  CallCompletedOutcome,
  CallView,
  ContactMethod,
  HistoryEntry,
  MeResponse,
  PersonDetailResponse,
} from '../api/types'
import type { CallRoom, CallRoomEvents, CallRoomFactory } from '../telephony/useCall'
import PersonDetailView from './PersonDetailView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const PERSON_ID = '33333333-3333-3333-3333-333333333333'
const CALL_ID = '55555555-5555-5555-5555-555555555555'
const PREVIOUS_CALL_ID = '66666666-6666-6666-6666-666666666666'
const PHONE_A: ContactMethod = { id: 'cm-phone-a', kind: 'phone', value: '+1 555 0100' }
const PHONE_B: ContactMethod = { id: 'cm-phone-b', kind: 'phone', value: '+1 555 0101' }
const EMAIL: ContactMethod = { id: 'cm-email', kind: 'email', value: 'grace@example.com' }

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}

function detail(contactMethods: ContactMethod[], history: HistoryEntry[] = []): PersonDetailResponse {
  return {
    person: {
      id: PERSON_ID,
      first_name: 'Grace',
      last_name: 'Hopper',
      display_name: 'Grace Hopper',
      stage: { id: 'stage-lead', name: 'Lead' },
      assigned_user: null,
      primary_email: null,
      primary_phone: null,
      inquiry_count: 0,
      last_inquiry_at: null,
      created_at: '2026-08-22T09:00:00.000Z',
    },
    contact_methods: contactMethods,
    inquiries: [],
    history,
  }
}

function callView(overrides: Partial<CallView> = {}): CallView {
  return {
    id: CALL_ID,
    person_id: PERSON_ID,
    contact_method_id: PHONE_A.id,
    caller: { id: 'u-alice', display_name: 'Alice' },
    status: 'placing',
    failure_reason: null,
    end_reason: null,
    placed_at: '2026-08-22T10:00:00.000Z',
    ringing_at: null,
    answered_at: null,
    ended_at: null,
    talk_seconds: null,
    ...overrides,
  }
}

class FakeRoom implements CallRoom {
  handlers: { [E in keyof CallRoomEvents]: CallRoomEvents[E][] } = {
    participantConnected: [],
    participantDisconnected: [],
    participantAttributesChanged: [],
    trackSubscribed: [],
    trackUnsubscribed: [],
    disconnected: [],
  }
  on<E extends keyof CallRoomEvents>(event: E, handler: CallRoomEvents[E]): void {
    this.handlers[event].push(handler)
  }
  emit<E extends keyof CallRoomEvents>(event: E, ...args: Parameters<CallRoomEvents[E]>): void {
    for (const handler of this.handlers[event]) {
      ;(handler as (...a: Parameters<CallRoomEvents[E]>) => void)(...args)
    }
  }
  async load(): Promise<void> {}
  async acquireMicrophone(): Promise<void> {}
  async connect(): Promise<void> {}
  async setMicrophoneMuted(): Promise<void> {}
  async disconnect(): Promise<void> {
    this.emit('disconnected')
  }
}

interface StubOptions {
  settledHangup?: CallView
  /** Per-attempt start responses (thrown if an Error); the last repeats. */
  starts?: Array<unknown>
}

function stubApi(personDetail: PersonDetailResponse, options: StubOptions = {}) {
  const settledHangup = options.settledHangup ?? callView({ status: 'failed', failure_reason: 'cancelled', ringing_at: 'x' })
  const starts = [...(options.starts ?? [])]
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return me()
    if (path.startsWith('/people/') && !path.endsWith('/calls') && method === 'GET') {
      const id = path.slice('/people/'.length)
      return id === PERSON_ID ? personDetail : { ...personDetail, person: { ...personDetail.person, id, display_name: 'Someone Else' } }
    }
    if (path === '/stages') return { stages: [{ id: 'stage-lead', name: 'Lead', position: 1 }] }
    if (path === '/organization/members') return { members: [] }
    if (path === `/people/${PERSON_ID}/calls`) {
      const next = starts.length > 1 ? starts.shift() : starts[0]
      if (next instanceof Error) throw next
      return next ?? { call: callView(), join: { url: 'wss://livekit.test', token: 't', room: `call:${CALL_ID}` } }
    }
    if (path === `/calls/${PREVIOUS_CALL_ID}/hangup`) {
      return { call: callView({ id: PREVIOUS_CALL_ID, status: 'ended', end_reason: 'agent_hangup' }) }
    }
    if (path === `/calls/${CALL_ID}/dial`) return { call: callView() }
    if (path === `/calls/${CALL_ID}/hangup`) return { call: settledHangup }
    if (path === `/calls/${CALL_ID}`) return { call: callView() }
    throw new Error(`unexpected ${method} ${path}`)
  })
}

async function mountView() {
  const rooms: FakeRoom[] = []
  const createRoom: CallRoomFactory = () => {
    const room = new FakeRoom()
    rooms.push(room)
    return room
  }
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/people', component: { template: '<div />' } },
      { path: '/people/:id', component: { template: '<div />' } },
    ],
  })
  await router.push(`/people/${PERSON_ID}`)
  await router.isReady()
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  })
  const wrapper = mount(PersonDetailView, {
    props: { id: PERSON_ID, createRoom },
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return { wrapper, rooms, queryClient }
}

function requests(): string[] {
  return apiFetchMock.mock.calls.map(([path, init]) => `${init?.method ?? 'GET'} ${path}`)
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('PersonDetailView — Call button', () => {
  it('is disabled with "No phone number" when the Person has no phone', async () => {
    stubApi(detail([EMAIL]))
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="call-button"]')
    expect(button.attributes('disabled')).toBeDefined()
    expect(button.attributes('title')).toBe('No phone number')
    expect(wrapper.get('[data-testid="call-no-phone"]').text()).toBe('No phone number')
    await button.trigger('click')
    expect(requests().some((r) => r.endsWith('/calls'))).toBe(false)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('is the one primary while Log contact is secondary', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    expect(wrapper.get('[data-testid="call-button"]').classes()).toContain('bg-accent')
    expect(wrapper.get('[data-testid="log-contact"]').classes()).not.toContain('bg-accent')
    expect(wrapper.findAll('.bg-accent')).toHaveLength(1)
  })

  it('with one phone, starts the call directly — no picker, only contact_method_id on the wire', async () => {
    stubApi(detail([PHONE_A, EMAIL]))
    const { wrapper, rooms } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)
    const start = apiFetchMock.mock.calls.find(([path]) => path === `/people/${PERSON_ID}/calls`)
    expect(start).toBeDefined()
    expect(JSON.parse(String(start?.[1]?.body))).toEqual({ contact_method_id: PHONE_A.id })
    expect(JSON.stringify(apiFetchMock.mock.calls)).not.toContain(PHONE_A.value)
    expect(rooms).toHaveLength(1)
  })

  it('with two phones, shows the number picker and calls the chosen one', async () => {
    stubApi(detail([PHONE_A, PHONE_B]))
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="call-button"]')
    expect(button.attributes('aria-haspopup')).toBe('menu')
    await button.trigger('click')
    expect(requests().some((r) => r.endsWith('/calls'))).toBe(false)
    const picker = wrapper.get('[data-testid="call-number-picker"]')
    const items = picker.findAll('[role="menuitem"]')
    expect(items.map((i) => i.text())).toEqual([PHONE_A.value, PHONE_B.value])
    await items[1].trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)
    const start = apiFetchMock.mock.calls.find(([path]) => path === `/people/${PERSON_ID}/calls`)
    expect(JSON.parse(String(start?.[1]?.body))).toEqual({ contact_method_id: PHONE_B.id })
  })

  it('walks Connecting… → Ringing… → Hang up → the no-answer line, with Hang up the only primary meanwhile', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper, rooms } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()

    const panel = wrapper.get('[data-testid="call-panel"]')
    expect(panel.text()).toContain('Grace Hopper')
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Connecting…')
    expect(wrapper.get('[data-testid="call-button"]').attributes('disabled')).toBeDefined()
    expect(wrapper.findAll('.bg-accent')).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-hangup"]').classes()).toContain('bg-accent')

    rooms[0].emit('participantConnected', { identity: `sip:${CALL_ID}`, attributes: {} })
    await flushPromises()
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Ringing…')

    await wrapper.get('[data-testid="call-hangup"]').trigger('click')
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/hangup`)).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('No answer')
    expect(wrapper.get('[data-testid="call-logged"]').text()).toBe('Logged as contact attempt — call, no answer')
    // The Call button is the primary again.
    expect(wrapper.get('[data-testid="call-button"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.get('[data-testid="call-button"]').classes()).toContain('bg-accent')

    await wrapper.get('[data-testid="call-dismiss"]').trigger('click')
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })
})

describe('PersonDetailView — number picker dismissal', () => {
  it('closes on Escape and on an outside click, stays open on an inside click', async () => {
    stubApi(detail([PHONE_A, PHONE_B]))
    const { wrapper } = await mountView()
    const button = wrapper.get('[data-testid="call-button"]')
    await button.trigger('click')
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(true)
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)

    await button.trigger('click')
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(true)
    wrapper.get('[data-testid="call-number-picker"]').element.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(true)
    document.body.dispatchEvent(new MouseEvent('click', { bubbles: true }))
    await flushPromises()
    expect(wrapper.find('[data-testid="call-number-picker"]').exists()).toBe(false)
    expect(requests().some((r) => r.endsWith('/calls'))).toBe(false)
  })
})

describe('PersonDetailView — call in progress (409) and mid-call navigation', () => {
  it('409 → Hang up previous call → Call again → 201', async () => {
    stubApi(detail([PHONE_A]), {
      starts: [new ApiError(409, 'call_in_progress', { call_id: PREVIOUS_CALL_ID }), undefined],
    })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-error"]').text()).toBe('You already have a call in progress.')
    await wrapper.get('[data-testid="call-hangup-previous"]').trigger('click')
    await flushPromises()
    expect(requests()).toContain(`POST /calls/${PREVIOUS_CALL_ID}/hangup`)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Connecting…')
    expect(requests().filter((r) => r === `POST /people/${PERSON_ID}/calls`)).toHaveLength(2)
    expect(requests()).toContain(`POST /calls/${CALL_ID}/dial`)
  })

  it('keeps the original callee name in the panel when the route param changes mid-call', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-panel"]').text()).toContain('Grace Hopper')
    await wrapper.setProps({ id: '77777777-7777-7777-7777-777777777777' })
    await flushPromises()
    expect(wrapper.get('h1').text()).toBe('Someone Else')
    expect(wrapper.get('[data-testid="call-panel"]').text()).toContain('Grace Hopper')
    expect(wrapper.get('[data-testid="call-panel"]').text()).not.toContain('Someone Else')
    expect(wrapper.get('[data-testid="call-status"]').text()).toBe('Connecting…')
  })
})

describe('PersonDetailView — leaving the page mid-call', () => {
  async function mountThroughRouter() {
    const rooms: FakeRoom[] = []
    const createRoom: CallRoomFactory = () => {
      const room = new FakeRoom()
      rooms.push(room)
      return room
    }
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [
        { path: '/people', component: { template: '<div data-testid="people" />' } },
        {
          path: '/people/:id',
          component: PersonDetailView,
          props: (route) => ({ id: route.params.id, createRoom }),
        },
      ],
    })
    await router.push(`/people/${PERSON_ID}`)
    await router.isReady()
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
    const wrapper = mount(
      { template: '<RouterView />' },
      { global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] }, attachTo: document.body },
    )
    await flushPromises()
    return { wrapper, router, rooms }
  }

  it('asks "End the call?" — cancel stays, confirm leaves and hangs up once', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper, router } = await mountThroughRouter()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    const confirm = vi.fn(() => false)
    vi.stubGlobal('confirm', confirm)
    await router.push('/people')
    expect(confirm).toHaveBeenCalledWith('End the call?')
    expect(router.currentRoute.value.path).toBe(`/people/${PERSON_ID}`)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(true)

    confirm.mockReturnValue(true)
    await router.push('/people')
    await flushPromises()
    expect(router.currentRoute.value.path).toBe('/people')
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/hangup`)).toHaveLength(1)
    vi.unstubAllGlobals()
  })

  it('leaves without asking when no call is active', async () => {
    stubApi(detail([PHONE_A]))
    const { router } = await mountThroughRouter()
    const confirm = vi.fn(() => false)
    vi.stubGlobal('confirm', confirm)
    await router.push('/people')
    expect(confirm).not.toHaveBeenCalled()
    expect(router.currentRoute.value.path).toBe('/people')
    vi.unstubAllGlobals()
  })
})

describe('PersonDetailView — call_completed history', () => {
  function entry(outcome: CallCompletedOutcome, talkSeconds: number | null): HistoryEntry {
    return {
      kind: 'call_completed',
      id: `h-${outcome}`,
      occurred_at: '2026-08-22T10:01:12.000Z',
      recorded_at: '2026-08-22T10:01:12.000Z',
      actor: { id: 'u-alice', display_name: 'Alice' },
      origin: 'web_session',
      correlation_id: 'corr',
      detail: { call_id: CALL_ID, outcome, talk_seconds: talkSeconds, answered_at: null },
    }
  }

  it('renders "Call — reached, 1 min 12 s" and "Call — no answer"', async () => {
    stubApi(detail([PHONE_A], [entry('reached', 72), entry('no_answer', null)]))
    const { wrapper } = await mountView()
    const text = wrapper.text()
    expect(text).toContain('Call — reached, 1 min 12 s')
    expect(text).toContain('Call — no answer')
    expect(text).toContain('Alice')
  })
})
