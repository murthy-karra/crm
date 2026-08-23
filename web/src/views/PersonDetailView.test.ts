// SLICE_006 §13 item 4: Call disabled without a phone; number picker only
// with ≥ 2 phones; `call_completed` history rendering; one primary per
// view; the Call → panel → Hang up flow against a fake room. SLICE_006c §13
// item 3: the post-call prompt's Save/Skip against the mocked route, Save
// gated on the server's status (seeded `answered`, refetched `ended`), the
// error copy per code, superseded/corrected history rendering, Change
// outcome only on the caller's own non-superseded call rows, and the manual
// dialog's widened vocabulary. Service-free:
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
  ContactAttemptedDetail,
  ContactMethod,
  CorrectOutcomeResponse,
  HistoryEntry,
  MeResponse,
  PersonDetailResponse,
} from '../api/types'
import { queryKeys } from '../api/queries'
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

function correction(changed: boolean): CorrectOutcomeResponse {
  return {
    attempt: {
      id: 'att-2',
      channel: 'call',
      outcome: 'left_message',
      occurred_at: '2026-08-22T10:00:00.000Z',
      recorded_at: '2026-08-22T10:05:00.000Z',
      corrects_id: 'att-1',
    },
    changed,
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
  /** `GET /api/calls/{id}` responses in order; the last repeats. Defaults to `placing`. */
  gets?: CallView[]
  /** `POST /api/calls/{id}/outcome` response, or an Error to throw. */
  outcome?: CorrectOutcomeResponse | Error
}

function stubApi(personDetail: PersonDetailResponse, options: StubOptions = {}) {
  const settledHangup = options.settledHangup ?? callView({ status: 'failed', failure_reason: 'cancelled', ringing_at: 'x' })
  const starts = [...(options.starts ?? [])]
  const gets = [...(options.gets ?? [])]
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
    if (path === `/calls/${CALL_ID}/outcome`) {
      if (options.outcome instanceof Error) throw options.outcome
      return options.outcome ?? correction(true)
    }
    if (path === `/calls/${CALL_ID}`) {
      const next = gets.length > 1 ? gets.shift() : gets[0]
      return { call: next ?? callView() }
    }
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
    // SLICE_006c: the prompt replaces the logged line; Save outcome is the
    // one primary, so the Call button (enabled again) is secondary.
    expect(wrapper.get('[data-testid="call-outcome-prompt"]').text()).toBe('How did it go?')
    expect(wrapper.get('[data-testid="call-button"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.findAll('.bg-accent')).toHaveLength(1)
    expect(wrapper.get('[data-testid="call-outcome-save"]').classes()).toContain('bg-accent')

    await wrapper.get('[data-testid="call-outcome-skip"]').trigger('click')
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
    expect(requests().some((r) => r.endsWith('/outcome'))).toBe(false)
    // The Call button is the primary again.
    expect(wrapper.get('[data-testid="call-button"]').classes()).toContain('bg-accent')
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

// ---- SLICE_006c ------------------------------------------------------------

const ATTEMPT_ID = '88888888-8888-8888-8888-888888888888'

function attemptEntry(
  id: string,
  detail: Partial<ContactAttemptedDetail>,
  actor: { id: string; display_name: string } | null = { id: 'u-alice', display_name: 'Alice' },
): HistoryEntry {
  return {
    kind: 'contact_attempted',
    id,
    occurred_at: '2026-08-22T10:00:00.000Z',
    recorded_at: '2026-08-22T10:00:00.000Z',
    actor,
    origin: 'web_session',
    correlation_id: 'corr',
    detail: { channel: 'call', outcome: 'reached', call_id: CALL_ID, corrects_id: null, superseded: false, ...detail },
  }
}

/** Call → SIP leg answered → remote hangup. The hangup response leaves the
 * server at `answered` (the hangup request races the webhook), so the
 * panel's Save is gated until a `GET` returns `ended`. */
async function answeredCallEnded(options: StubOptions = {}) {
  stubApi(detail([PHONE_A]), {
    settledHangup: callView({ status: 'answered', answered_at: 'x' }),
    gets: [callView({ status: 'ended', end_reason: 'remote_hangup', answered_at: 'x', talk_seconds: 5 })],
    ...options,
  })
  const mounted = await mountView()
  await mounted.wrapper.get('[data-testid="call-button"]').trigger('click')
  await flushPromises()
  mounted.rooms[0].emit('participantConnected', { identity: `sip:${CALL_ID}`, attributes: { 'sip.callStatus': 'active' } })
  await flushPromises()
  mounted.rooms[0].emit('participantDisconnected', { identity: `sip:${CALL_ID}`, attributes: {} })
  await flushPromises()
  return mounted
}

describe('PersonDetailView — How did it go? (SLICE_006c §10)', () => {
  it('Save waits for the server: disabled at answered, enabled after the call.changed refetch shows ended', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    expect(wrapper.find('[data-testid="call-outcome-prompt"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeDefined()
    // D-023: `call.changed` is invalidation-only — the refetch carries the status.
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
  })

  it('Save posts exactly {"outcome"} to /calls/{id}/outcome, shows the saved line, Done closes', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    const invalidate = vi.spyOn(queryClient, 'invalidateQueries')
    await wrapper.get('[data-outcome="left_message"]').trigger('click')
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(([path]) => path === `/calls/${CALL_ID}/outcome`)
    expect(post?.[1]?.method).toBe('POST')
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ outcome: 'left_message' })
    expect(wrapper.get('[data-testid="call-outcome-saved"]').text()).toBe('Outcome saved — voicemail')
    const keys = invalidate.mock.calls.map(([f]) => JSON.stringify((typeof f === 'function' ? f() : f)?.queryKey))
    expect(keys).toContain(JSON.stringify(queryKeys.person(ORG_ID, PERSON_ID)))
    expect(keys).toContain(JSON.stringify(queryKeys.today(ORG_ID)))
    await wrapper.get('[data-testid="call-dismiss"]').trigger('click')
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('saving the outcome already recorded (changed: false) closes the panel', async () => {
    const { wrapper, queryClient } = await answeredCallEnded({ outcome: correction(false) })
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    expect(requests().filter((r) => r === `POST /calls/${CALL_ID}/outcome`)).toHaveLength(1)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it.each([
    [new ApiError(409, 'invalid_call_state'), "The call hasn't finished yet."],
    [new ApiError(422, 'no_contact_attempt'), "There's no contact attempt to correct."],
    [new ApiError(409, 'correction_conflict'), 'This outcome was just changed — refreshed.'],
    [new ApiError(500, 'internal_error'), 'Could not save the outcome.'],
  ])('renders the §10 copy for %s and keeps the prompt open', async (failure, message) => {
    const { wrapper, queryClient } = await answeredCallEnded({ outcome: failure })
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    const personGets = () => requests().filter((r) => r === `GET /people/${PERSON_ID}`).length
    const before = personGets()
    await wrapper.get('[data-testid="call-outcome-save"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="call-outcome-error"]').text()).toBe(message)
    expect(wrapper.find('[data-testid="outcome-picker"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    // correction_conflict refetches the Person (history) — the others do not.
    expect(personGets() > before).toBe(failure.code === 'correction_conflict')
  })

  it('Skip sends nothing — no /outcome request, panel closed', async () => {
    const { wrapper, queryClient } = await answeredCallEnded()
    await queryClient.invalidateQueries({ queryKey: queryKeys.call(ORG_ID, CALL_ID) })
    await flushPromises()
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('[data-testid="call-outcome-skip"]').trigger('click')
    await flushPromises()
    expect(requests().some((r) => r.endsWith('/outcome'))).toBe(false)
    expect(wrapper.find('[data-testid="call-panel"]').exists()).toBe(false)
  })

  it('shows "Finishing up…" under a disabled Save and a follow-up GET enables it without a realtime event', async () => {
    const { wrapper } = await answeredCallEnded()
    expect(wrapper.get('[data-testid="call-outcome-finishing"]').text()).toBe('Finishing up…')
    const getCount = () => requests().filter((r) => r === `GET /calls/${CALL_ID}`).length
    expect(getCount()).toBe(0)
    await new Promise((resolve) => setTimeout(resolve, 1100))
    await flushPromises()
    expect(getCount()).toBeGreaterThanOrEqual(1)
    expect(wrapper.get('[data-testid="call-outcome-save"]').attributes('disabled')).toBeUndefined()
    expect(wrapper.find('[data-testid="call-outcome-finishing"]').exists()).toBe(false)
  })

  it('a call that never reached the callee gets Done, not the prompt', async () => {
    stubApi(detail([PHONE_A]), { settledHangup: callView({ status: 'failed', failure_reason: 'cancelled', ringing_at: null }) })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    await wrapper.get('[data-testid="call-hangup"]').trigger('click')
    await flushPromises()
    expect(wrapper.find('[data-testid="call-outcome-prompt"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="call-dismiss"]').exists()).toBe(true)
  })
})

describe('PersonDetailView — corrected history (SLICE_006c §10)', () => {
  it('renders a superseded row muted with "(superseded)" and a correction as "Outcome corrected — voicemail", by detail not position', async () => {
    const completed: HistoryEntry = {
      kind: 'call_completed',
      id: 'h-completed',
      occurred_at: '2026-08-22T10:00:00.000Z',
      recorded_at: '2026-08-22T10:00:00.000Z',
      actor: null,
      origin: 'system',
      correlation_id: 'corr',
      detail: { call_id: CALL_ID, outcome: 'no_answer', talk_seconds: null, answered_at: null },
    }
    // The busy-call order from §2: original → call_completed → correction.
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'no_answer', superseded: true }),
          completed,
          attemptEntry('att-2', { outcome: 'busy', corrects_id: ATTEMPT_ID }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const rows = wrapper.findAll('[data-superseded="true"]')
    expect(rows).toHaveLength(1)
    expect(rows[0].text()).toBe('Contact attempted — call, no answer (superseded)')
    expect(rows[0].classes()).toContain('text-text-muted')
    expect(rows[0].classes()).not.toContain('text-text')
    expect(wrapper.text()).toContain('Outcome corrected — busy')
    expect(wrapper.text()).toContain('Call — no answer')
  })

  it('a chain: the first correction is itself superseded; the head reads as the correction', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry(ATTEMPT_ID, { outcome: 'reached', superseded: true }),
          attemptEntry('att-2', { outcome: 'left_message', corrects_id: ATTEMPT_ID, superseded: true }),
          attemptEntry('att-3', { outcome: 'wrong_number', corrects_id: 'att-2' }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const summaries = wrapper.findAll('li p.text-body').map((p) => p.text())
    expect(summaries).toEqual([
      'Contact attempted — call, reached (superseded)',
      'Outcome corrected — voicemail (superseded)',
      'Outcome corrected — wrong number',
    ])
    expect(wrapper.findAll('[data-testid="change-outcome"]')).toHaveLength(1)
  })
})

describe('PersonDetailView — Change outcome (SLICE_006c §1 step 7)', () => {
  it('is offered only on the caller\'s own, call-derived, non-superseded attempt rows', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [
          attemptEntry('mine-call', {}),
          attemptEntry('mine-manual', { call_id: null }),
          attemptEntry('mine-superseded', { superseded: true }),
          attemptEntry('carols-call', {}, { id: 'u-carol', display_name: 'Carol' }),
        ],
      ),
    )
    const { wrapper } = await mountView()
    const items = wrapper.findAll('li').filter((li) => li.text().includes('Contact attempted'))
    expect(items).toHaveLength(4)
    expect(items.map((li) => li.findAll('[data-testid="change-outcome"]').length)).toEqual([1, 0, 0, 0])
  })

  it('opens the picker on the row\'s outcome and posts to /calls/{call_id}/outcome', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, { outcome: 'no_answer' })]))
    const { wrapper } = await mountView()
    const action = wrapper.get('[data-testid="change-outcome"]')
    expect(action.classes()).toContain('bg-transparent')
    await action.trigger('click')
    await flushPromises()
    const picker = document.querySelector('[data-testid="outcome-picker"]')
    expect(picker).not.toBeNull()
    expect(picker?.querySelector('[aria-checked="true"]')?.getAttribute('data-outcome')).toBe('no_answer')
    ;(picker?.querySelector('[data-outcome="busy"]') as HTMLButtonElement).click()
    await flushPromises()
    ;(document.querySelector('[data-testid="change-outcome-save"]') as HTMLButtonElement).click()
    await flushPromises()
    const post = apiFetchMock.mock.calls.find(([path]) => path === `/calls/${CALL_ID}/outcome`)
    expect(JSON.parse(String(post?.[1]?.body))).toEqual({ outcome: 'busy' })
    expect(document.querySelector('[data-testid="outcome-picker"]')).toBeNull()
  })

  it('is disabled while the post-call prompt is open (one primary)', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, {})]), {
      settledHangup: callView({ status: 'ended', end_reason: 'remote_hangup', answered_at: 'x' }),
    })
    const { wrapper, rooms } = await mountView()
    expect(wrapper.get('[data-testid="change-outcome"]').attributes('disabled')).toBeUndefined()
    await wrapper.get('[data-testid="call-button"]').trigger('click')
    await flushPromises()
    rooms[0].emit('participantConnected', { identity: `sip:${CALL_ID}`, attributes: { 'sip.callStatus': 'active' } })
    await flushPromises()
    rooms[0].emit('participantDisconnected', { identity: `sip:${CALL_ID}`, attributes: {} })
    await flushPromises()
    expect(wrapper.find('[data-testid="call-outcome-prompt"]').exists()).toBe(true)
    const action = wrapper.get('[data-testid="change-outcome"]')
    expect(action.attributes('disabled')).toBeDefined()
    action.element.removeAttribute('disabled')
    await action.trigger('click')
    await flushPromises()
    expect(document.querySelector('[data-testid="change-outcome-save"]')).toBeNull()
    expect(wrapper.findAll('.bg-accent')).toHaveLength(1)
    await wrapper.get('[data-testid="call-outcome-skip"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="change-outcome"]').attributes('disabled')).toBeUndefined()
  })

  it('re-seeds the picker when reopened on a different row', async () => {
    stubApi(
      detail(
        [PHONE_A],
        [attemptEntry('a1', { outcome: 'no_answer', call_id: 'call-a' }), attemptEntry('a2', { outcome: 'reached', call_id: 'call-b' })],
      ),
    )
    const { wrapper } = await mountView()
    const actions = wrapper.findAll('[data-testid="change-outcome"]')
    expect(actions).toHaveLength(2)
    const checkedOutcome = () =>
      document.querySelector('[data-testid="outcome-picker"] [aria-checked="true"]')?.getAttribute('data-outcome')
    await actions[0].trigger('click')
    await flushPromises()
    expect(checkedOutcome()).toBe('no_answer')
    ;(document.querySelector('[data-testid="outcome-picker"] [data-outcome="busy"]') as HTMLButtonElement).click()
    await flushPromises()
    expect(checkedOutcome()).toBe('busy')
    ;(document.querySelector('[data-testid="change-outcome-cancel"]') as HTMLButtonElement).click()
    await flushPromises()
    await actions[1].trigger('click')
    await flushPromises()
    expect(checkedOutcome()).toBe('reached')
  })

  it('shows the §10 copy inside the dialog on failure', async () => {
    stubApi(detail([PHONE_A], [attemptEntry(ATTEMPT_ID, {})]), { outcome: new ApiError(422, 'no_contact_attempt') })
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="change-outcome"]').trigger('click')
    await flushPromises()
    ;(document.querySelector('[data-testid="change-outcome-save"]') as HTMLButtonElement).click()
    await flushPromises()
    expect(document.querySelector('[data-testid="change-outcome-error"]')?.textContent?.trim()).toBe(
      "There's no contact attempt to correct.",
    )
  })
})

describe('PersonDetailView — Log contact dialog vocabulary (SLICE_006c §1)', () => {
  it('offers Voicemail / left message, Busy and Wrong number', async () => {
    stubApi(detail([PHONE_A]))
    const { wrapper } = await mountView()
    await wrapper.get('[data-testid="log-contact"]').trigger('click')
    await flushPromises()
    const outcome = document.querySelector('[aria-label="Outcome"]') as HTMLElement
    outcome.click()
    await flushPromises()
    const labels = Array.from(document.querySelectorAll('[role="option"]')).map((o) => o.textContent?.trim())
    expect(labels).toEqual(['Reached', 'No answer', 'Voicemail / left message', 'Sent', 'Busy', 'Wrong number'])
  })
})
