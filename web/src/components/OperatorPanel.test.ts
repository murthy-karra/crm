/* eslint-disable vue/one-component-per-file -- local harnesses provide identity and call-host fixtures. */
// SLICE_005 §13 item 5: pending/disabled states, each error code's copy,
// cards from `references` only (a reply containing a UUID or `<a>` renders
// as text), history capped at 6 and cleared by Clear, context per route.
import { flushPromises, mount, type DOMWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type {
  OperatorCreateTaskProposal,
  OperatorReceipt,
  OperatorTurnRequest,
  OperatorTurnResponse,
} from '../api/types'
import { defineComponent, ref } from 'vue'
import OperatorPanel from './OperatorPanel.vue'
import { CALL_HOST_KEY, provideCallHost, type CallHost } from '../telephony/callHost'
import type { CallRoomFactory, CallRoom } from '../telephony/useCall'
import { beginSessionTransition, settleSessionTransition } from '../sessionLifecycle'

const ORG_ID = '11111111-1111-1111-1111-111111111111'

/** A no-op room; `events` records the mic/connect order so the
 * mic-BEFORE-confirm rule (SLICE_006b §6) is assertable. */
function fakeRoomFactory(behavior: { denyMic?: boolean; events?: string[] } = {}): CallRoomFactory {
  return () =>
    ({
      on: () => undefined,
      load: () => Promise.resolve(),
      acquireMicrophone: () => {
        behavior.events?.push('mic')
        return behavior.denyMic ? Promise.reject(new Error('denied')) : Promise.resolve()
      },
      connect: () => {
        behavior.events?.push('connect')
        return Promise.resolve()
      },
      setMicrophoneMuted: () => Promise.resolve(),
      disconnect: () => Promise.resolve(),
    }) satisfies CallRoom
}

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ID = '5a0c1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b7c'

function response(overrides: Partial<OperatorTurnResponse> = {}): OperatorTurnResponse {
  return {
    turn_id: 'turn-1',
    reply: 'Call Grace first.',
    proposal: null,
    receipt: null,
    references: {
      people: [
        {
          id: ID,
          display_name: 'Grace Hopper',
          stage_name: 'Lead',
          assigned_user_display_name: 'Alice',
          primary_email: 'grace@example.com',
          primary_phone: null,
          inquiry_count: 1,
          last_inquiry_at: null,
        },
      ],
    },
    tool_calls: [{ name: 'get_next_work_item', outcome: 'ok', duration_ms: 3 }],
    outcome: 'completed',
    ...overrides,
  }
}

async function mountPanel(
  path = '/today',
  roomBehavior: { denyMic?: boolean; events?: string[] } = {},
  identityKey = ref(`${ORG_ID}:actor-a:0`),
) {
  const router = createRouter({
    history: createMemoryHistory(),
    routes: [
      { path: '/today', component: { template: '<div />' } },
      { path: '/people', component: { template: '<div />' } },
      { path: '/people/:id', component: { template: '<div />' } },
      { path: '/:pathMatch(.*)*', component: { template: '<div />' } },
    ],
  })
  await router.push(path)
  await router.isReady()
  const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
  // The drawer needs the app-level call host (SLICE_006b §6).
  let host: CallHost | undefined
  const Harness = defineComponent({
    components: { OperatorPanel },
    setup() {
      host = provideCallHost({ orgId: () => ORG_ID, createRoom: fakeRoomFactory(roomBehavior) })
      return { identityKey }
    },
    template: '<OperatorPanel :identity-key="identityKey" />',
  })
  const wrapper = mount(Harness, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  })
  return { wrapper, router, host: host!, identityKey, queryClient }
}

/** The structural surface both harnesses (provideCallHost wrapper and the
 * mock-host direct mount) expose to the shared helpers. */
interface PanelDom {
  get(selector: string): Pick<DOMWrapper<Element>, 'setValue' | 'trigger' | 'text' | 'attributes'>
}

async function type(wrapper: PanelDom, text: string) {
  await wrapper.get('[data-testid="operator-input"]').setValue(text)
}

function lastRequest(): OperatorTurnRequest {
  const call = apiFetchMock.mock.calls.at(-1)
  if (!call) throw new Error('no request')
  return JSON.parse(String(call[1]?.body)) as OperatorTurnRequest
}

function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => {
    resolve = yes
    reject = no
  })
  return { promise, resolve, reject }
}

/** `settleTaskMutation`/`settlePersonMutation` defer their invalidate
 * decision past a macrotask (`setTimeout(fn, 0)`, docs/specs/SLICE_018.md
 * §8, review round 1) — `flushPromises()` alone only drains microtasks, so
 * a test asserting post-settle invalidation needs a real timer tick too. */
function waitForMacrotask() {
  return new Promise<void>((resolve) => setTimeout(resolve, 0))
}

const PRIVATE_SENTINEL = 'PRIVATE LIST NAME — ALICE ONLY'

function privateTurn(): OperatorTurnResponse {
  const person = {
    ...response().references.people[0]!,
    display_name: PRIVATE_SENTINEL,
  }
  return response({
    reply: PRIVATE_SENTINEL,
    references: { people: [person] },
    proposal: { ...proposal(), person },
  })
}

beforeEach(() => {
  apiFetchMock.mockReset()
  // docs/specs/SLICE_018.md §3: pinned so `utc_offset_minutes` assertions
  // check a real sign-flip, not the tautological "recompute the same
  // expression the component itself might use" the un-mocked value would
  // give. -330 (UTC+5:30, e.g. IST) -> the request carries 330.
  vi.spyOn(Date.prototype, 'getTimezoneOffset').mockReturnValue(-330)
})

afterEach(() => {
  document.body.innerHTML = ''
  vi.restoreAllMocks()
})

describe('OperatorPanel', () => {
  it('synchronously drops an old draft when a shared-cookie transition is discovered before send', async () => {
    const { wrapper } = await mountPanel()
    await type(wrapper, PRIVATE_SENTINEL)
    const transition = beginSessionTransition()

    await wrapper.get('form').trigger('submit')
    await flushPromises()

    expect(apiFetchMock).not.toHaveBeenCalled()
    expect(wrapper.findAll('[data-testid="operator-user"]')).toHaveLength(0)
    settleSessionTransition(transition, {})
  })

  it('disables Send when empty and while pending, then renders the reply and cards', async () => {
    let resolve!: (value: OperatorTurnResponse) => void
    apiFetchMock.mockImplementationOnce(
      () =>
        new Promise<OperatorTurnResponse>((r) => {
          resolve = r
        }) as Promise<never>,
    )
    const { wrapper } = await mountPanel()
    const send = wrapper.get('[data-testid="operator-send"]')
    expect(send.attributes('disabled')).toBeDefined()

    await type(wrapper, '  Who next?  ')
    expect(send.attributes('disabled')).toBeUndefined()
    await send.trigger('submit')
    await flushPromises()

    expect(wrapper.find('[data-testid="operator-pending"]').exists()).toBe(true)
    expect(wrapper.get('[data-testid="operator-send"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-testid="operator-user"]').text()).toBe('Who next?')
    expect(apiFetchMock).toHaveBeenCalledWith('/operator/turns', expect.objectContaining({ method: 'POST' }))
    expect(lastRequest()).toEqual({
      message: 'Who next?',
      history: [],
      context: { route: 'today' },
      utc_offset_minutes: 330,
    })

    resolve(response())
    await flushPromises()
    expect(wrapper.find('[data-testid="operator-pending"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="operator-assistant"]').text()).toContain('Call Grace first.')
    const cards = wrapper.findAll('[data-testid="operator-person-card"]')
    expect(cards).toHaveLength(1)
    expect(cards[0].text()).toContain('Grace Hopper')
    expect(cards[0].text()).toContain('Lead')
    expect(cards[0].text()).toContain('Alice')
    expect(cards[0].attributes('href')).toBe(`/people/${ID}`)
  })

  it('renders a reply containing a UUID and <a href> as literal text, cards only from references', async () => {
    const hostile = `See <a href="https://evil.example">this</a> and ${ID} **now**`
    apiFetchMock.mockResolvedValueOnce(response({ reply: hostile, references: { people: [] } }))
    const { wrapper } = await mountPanel()
    await type(wrapper, 'hi')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    const bubble = wrapper.get('[data-testid="operator-assistant"]')
    expect(bubble.get('p:last-of-type').text()).toBe(hostile)
    expect(bubble.find('a').exists()).toBe(false)
    expect(bubble.element.querySelector('a')).toBeNull()
    expect(wrapper.findAll('[data-testid="operator-person-card"]')).toHaveLength(0)
  })

  it('shows the §10 copy for each error code and the generic fallback', async () => {
    const cases: Array<[ApiError, string]> = [
      [new ApiError(503, 'operator_disabled'), 'The Operator is not configured on this server.'],
      [new ApiError(503, 'operator_unavailable'), 'The Operator is temporarily unavailable — try again in a moment.'],
      [new ApiError(429, 'operator_busy'), 'One question at a time — wait for the current answer.'],
      [new ApiError(503, 'unavailable'), 'The server is temporarily unavailable. Try again shortly.'],
      [new ApiError(400, 'malformed_request'), 'Something went wrong. Try again.'],
    ]
    for (const [err, copy] of cases) {
      apiFetchMock.mockRejectedValueOnce(err)
      const { wrapper } = await mountPanel()
      await type(wrapper, 'hi')
      await wrapper.get('form').trigger('submit')
      await flushPromises()
      expect(wrapper.get('[data-testid="operator-error"]').text()).toBe(copy)
      expect(wrapper.find('[data-testid="operator-assistant"]').exists()).toBe(false)
      // The input is usable again after an error.
      expect(wrapper.get('[data-testid="operator-input"]').attributes('disabled')).toBeUndefined()
      wrapper.unmount()
    }
  })

  it('sends the last six messages as history and Clear resets it', async () => {
    const { wrapper } = await mountPanel()
    for (let i = 0; i < 5; i += 1) {
      apiFetchMock.mockResolvedValueOnce(response({ reply: `r${i}`, references: { people: [] } }))
      await type(wrapper, `q${i}`)
      await wrapper.get('form').trigger('submit')
      await flushPromises()
    }
    // 5 turns = 10 transcript messages; the 5th request carried the last 6
    // of the 8 that preceded it.
    expect(lastRequest().history).toEqual([
      { role: 'user', content: 'q1' },
      { role: 'assistant', content: 'r1' },
      { role: 'user', content: 'q2' },
      { role: 'assistant', content: 'r2' },
      { role: 'user', content: 'q3' },
      { role: 'assistant', content: 'r3' },
    ])
    expect(wrapper.findAll('[data-testid="operator-user"]')).toHaveLength(5)

    await wrapper.get('[data-testid="operator-clear"]').trigger('click')
    expect(wrapper.findAll('[data-testid="operator-user"]')).toHaveLength(0)
    apiFetchMock.mockResolvedValueOnce(response({ reply: 'fresh', references: { people: [] } }))
    await type(wrapper, 'again')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(lastRequest().history).toEqual([])
  })

  it('derives context from the current route at send time', async () => {
    const { wrapper, router } = await mountPanel(`/people/${ID}`)
    apiFetchMock.mockResolvedValueOnce(response({ references: { people: [] } }))
    await type(wrapper, 'why is she first?')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(lastRequest().context).toEqual({ route: 'person', person_id: ID })

    await router.push('/people')
    apiFetchMock.mockResolvedValueOnce(response({ references: { people: [] } }))
    await type(wrapper, 'find grace')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(lastRequest().context).toEqual({ route: 'people' })

    await router.push('/intake/new')
    apiFetchMock.mockResolvedValueOnce(response({ references: { people: [] } }))
    await type(wrapper, 'x')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(lastRequest().context).toEqual({ route: 'other' })
  })

  it('Enter sends and Shift+Enter does not; close emits', async () => {
    apiFetchMock.mockResolvedValueOnce(response({ references: { people: [] } }))
    const { wrapper } = await mountPanel()
    const input = wrapper.get('[data-testid="operator-input"]')
    await input.setValue('hello')
    await input.trigger('keydown', { key: 'Enter', shiftKey: true })
    expect(apiFetchMock).not.toHaveBeenCalled()
    await input.trigger('keydown', { key: 'Enter' })
    await flushPromises()
    expect(apiFetchMock).toHaveBeenCalledTimes(1)

    await wrapper.get('[data-testid="operator-close"]').trigger('click')
    expect(wrapper.findComponent(OperatorPanel).emitted('close')).toHaveLength(1)
  })
})

describe('OperatorPanel identity boundary (SLICE_011c §6)', () => {
  it.each([
    ['same-Organization actor switch', `${ORG_ID}:actor-b:0`],
    ['same-ID authentication-session replacement', `${ORG_ID}:actor-a:1`],
    // SLICE_011c §9 criterion 10: a different Organization id with the same
    // actor (AppShell's `operatorIdentityKey` includes `orgId`, so an
    // Organization switch changes this same prop) must discard state too.
    ['Organization switch (same actor)', '22222222-2222-2222-2222-222222222222:actor-a:0'],
  ])('discards transcript, history, draft, cards, and proposals on a %s', async (_label, nextIdentity) => {
    apiFetchMock.mockResolvedValueOnce(privateTurn()).mockResolvedValueOnce(response({
      reply: 'Fresh session reply',
      references: { people: [] },
    }))
    const { wrapper, identityKey } = await mountPanel()
    await type(wrapper, 'Remember my private source')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.text()).toContain(PRIVATE_SENTINEL)
    expect(wrapper.find('[data-testid="operator-person-card"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="operator-proposal"]').exists()).toBe(true)

    await type(wrapper, `${PRIVATE_SENTINEL} draft`)
    identityKey.value = nextIdentity
    await flushPromises()

    expect(wrapper.text()).not.toContain(PRIVATE_SENTINEL)
    expect(wrapper.find('[data-testid="operator-person-card"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="operator-proposal"]').exists()).toBe(false)
    expect((wrapper.get('[data-testid="operator-input"]').element as HTMLTextAreaElement).value).toBe('')

    await type(wrapper, 'What belongs to this session?')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(lastRequest()).toEqual({
      message: 'What belongs to this session?',
      history: [],
      context: { route: 'today' },
      utc_offset_minutes: 330,
    })
    expect(wrapper.text()).not.toContain(PRIVATE_SENTINEL)
  })

  it('drops a late successful turn after the identity changes and starts the next history empty', async () => {
    const first = deferred<OperatorTurnResponse>()
    apiFetchMock.mockReturnValueOnce(first.promise as Promise<never>).mockResolvedValueOnce(response({
      reply: 'Fresh response',
      references: { people: [] },
    }))
    const { wrapper, identityKey } = await mountPanel()
    await type(wrapper, 'Private question')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    identityKey.value = `${ORG_ID}:actor-b:0`
    first.resolve(privateTurn())
    await flushPromises()

    expect(wrapper.text()).not.toContain(PRIVATE_SENTINEL)
    expect(wrapper.find('[data-testid="operator-assistant"]').exists()).toBe(false)
    await type(wrapper, 'New agent question')
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(lastRequest().history).toEqual([])
  })

  it('drops a late failed turn after the identity changes', async () => {
    const first = deferred<OperatorTurnResponse>()
    apiFetchMock.mockReturnValueOnce(first.promise as Promise<never>)
    const { wrapper, identityKey } = await mountPanel()
    await type(wrapper, 'Private failure')
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    identityKey.value = `${ORG_ID}:actor-b:0`
    first.reject(new ApiError(503, 'operator_unavailable'))
    await flushPromises()

    expect(wrapper.find('[data-testid="operator-error"]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain('Private failure')
  })
})

// ---- SLICE_006b §6: the proposal card ---------------------------------------

const PROPOSAL_ID = '9b1c1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b7c'
const CALL_ID = '7c2d1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b7d'

function proposal(expiresInMs = 120_000) {
  return {
    id: PROPOSAL_ID,
    kind: 'start_call' as const,
    person: response().references.people[0],
    phone: '(555) 015-0100',
    contact_method_id: '6d3e1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b7e',
    expires_at: new Date(Date.now() + expiresInMs).toISOString(),
  }
}

function stubTurnThenConfirm(turn: OperatorTurnResponse, confirmResult?: () => Promise<unknown>) {
  apiFetchMock.mockImplementation((path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (method === 'POST' && path === '/operator/turns') return Promise.resolve(turn)
    if (method === 'POST' && path === `/operator/proposals/${PROPOSAL_ID}/confirm`) {
      if (confirmResult) return confirmResult()
      return Promise.resolve({
        call: { id: CALL_ID, person_id: proposal().person.id, status: 'placing' },
        join: { url: 'wss://lk', token: 'tok', room: 'call-x' },
      })
    }
    if (method === 'POST' && path === `/calls/${CALL_ID}/dial`) {
      return Promise.resolve({ call: { id: CALL_ID, status: 'ringing' } })
    }
    if (method === 'GET' && path === `/calls/${CALL_ID}`) {
      return Promise.resolve({ call: { id: CALL_ID, status: 'ringing' } })
    }
    if (method === 'POST' && path === `/calls/${CALL_ID}/hangup`) {
      return Promise.resolve({ call: { id: CALL_ID, status: 'ended' } })
    }
    return Promise.reject(new Error(`unexpected ${method} ${path}`))
  })
}

async function sendTurn(wrapper: PanelDom) {
  await type(wrapper, 'call grace')
  await wrapper.get('[data-testid="operator-send"]').trigger('click')
  await flushPromises()
}

describe('OperatorPanel — start_call proposal card (SLICE_006b)', () => {
  it('renders the card from the server proposal object only, and Confirm asks for the mic BEFORE the confirm POST', async () => {
    const events: string[] = []
    stubTurnThenConfirm(response({ proposal: proposal(), reply: 'Call (999) 999-9999 now!' }))
    const { wrapper } = await mountPanel('/today', { events })
    await sendTurn(wrapper)

    const card = wrapper.get('[data-testid="operator-proposal"]')
    // Server data, not the model's prose number.
    expect(card.text()).toContain('Grace Hopper')
    expect(card.text()).toContain('(555) 015-0100')
    expect(card.text()).not.toContain('(999) 999-9999')

    await card.get('[data-testid="operator-proposal-confirm"]').trigger('click')
    await flushPromises()
    const confirmIndex = apiFetchMock.mock.calls.findIndex(
      ([path]) => path === `/operator/proposals/${PROPOSAL_ID}/confirm`,
    )
    expect(confirmIndex).toBeGreaterThan(-1)
    expect(events[0]).toBe('mic')
    // The card hands over to the docked panel.
    expect(wrapper.get('[data-testid="operator-proposal-started"]').text()).toContain('Calling')
  })

  it('mic denial never consumes the proposal: no confirm POST, Confirm clickable again', async () => {
    stubTurnThenConfirm(response({ proposal: proposal() }))
    const { wrapper } = await mountPanel('/today', { denyMic: true })
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(
      apiFetchMock.mock.calls.some(([path]) => path === `/operator/proposals/${PROPOSAL_ID}/confirm`),
    ).toBe(false)
    const confirm = wrapper.get('[data-testid="operator-proposal-confirm"]')
    expect(confirm.attributes('disabled')).toBeUndefined()
  })

  it('an expired proposal disables Confirm with the expiry copy', async () => {
    stubTurnThenConfirm(response({ proposal: proposal(-1000) }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const confirm = wrapper.get('[data-testid="operator-proposal-confirm"]')
    expect(confirm.attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-testid="operator-proposal-expired"]').text()).toBe(
      'This suggestion expired — ask again.',
    )
  })

  it('a 409 proposal_expired from confirm shows the copy and finalizes the card', async () => {
    stubTurnThenConfirm(response({ proposal: proposal() }), () =>
      Promise.reject(new ApiError(409, 'proposal_expired', {})),
    )
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-proposal-message"]').text()).toBe(
      'This suggestion expired — ask again.',
    )
    expect(
      wrapper.get('[data-testid="operator-proposal-confirm"]').attributes('disabled'),
    ).toBeDefined()
  })

  it('Dismiss is local: no request, card finalized', async () => {
    stubTurnThenConfirm(response({ proposal: proposal() }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)
    const before = apiFetchMock.mock.calls.length

    await wrapper.get('[data-testid="operator-proposal-dismiss"]').trigger('click')
    await flushPromises()
    expect(apiFetchMock.mock.calls.length).toBe(before)
    expect(wrapper.get('[data-testid="operator-proposal-message"]').text()).toBe('Dismissed.')
    expect(
      wrapper.get('[data-testid="operator-proposal-confirm"]').attributes('disabled'),
    ).toBeDefined()
  })
})

describe('OperatorPanel — local pre-checks never consume the proposal (SLICE_006b §6)', () => {
  async function mountWithMockHost(
    result: () => Promise<string | null>,
    identityKey = ref(`${ORG_ID}:actor-a:0`),
  ) {
    const router = createRouter({
      history: createMemoryHistory(),
      routes: [{ path: '/today', component: { template: '<div />' } }],
    })
    await router.push('/today')
    await router.isReady()
    const queryClient = new QueryClient({ defaultOptions: { mutations: { retry: false } } })
    const startFromProposal = vi.fn(result)
    const host = { startFromProposal, call: { error: { value: null } } } as unknown as CallHost
    const Harness = defineComponent({
      components: { OperatorPanel },
      setup: () => ({ identityKey }),
      template: '<OperatorPanel :identity-key="identityKey" />',
    })
    const wrapper = mount(Harness, {
      global: {
        plugins: [router, [VueQueryPlugin, { queryClient }]],
        provide: { [CALL_HOST_KEY as symbol]: host },
      },
      attachTo: document.body,
    })
    return { wrapper, startFromProposal, identityKey }
  }

  it.each([
    ['call_in_progress', 'You already have a call in progress — hang up first.'],
    ['outcome_pending', "Save the previous call's outcome first."],
  ])('%s: shows its copy and keeps Confirm retryable', async (code, copy) => {
    stubTurnThenConfirm(response({ proposal: proposal() }))
    const { wrapper, startFromProposal } = await mountWithMockHost(() => Promise.resolve(code))
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(startFromProposal).toHaveBeenCalledTimes(1)
    expect(wrapper.get('[data-testid="operator-proposal-message"]').text()).toBe(copy)
    // Retryable: the pre-check consumed nothing.
    expect(
      wrapper.get('[data-testid="operator-proposal-confirm"]').attributes('disabled'),
    ).toBeUndefined()
  })

  it('does not apply a deferred proposal completion after the identity changes', async () => {
    const completion = deferred<string | null>()
    apiFetchMock.mockResolvedValueOnce(response({ proposal: proposal(), reply: PRIVATE_SENTINEL }))
    const { wrapper, identityKey, startFromProposal } = await mountWithMockHost(() => completion.promise)
    await sendTurn(wrapper)
    await wrapper.get('[data-testid="operator-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(startFromProposal).toHaveBeenCalledTimes(1)

    identityKey.value = `${ORG_ID}:actor-b:0`
    await flushPromises()
    completion.resolve(null)
    await flushPromises()

    expect(wrapper.text()).not.toContain(PRIVATE_SENTINEL)
    expect(wrapper.find('[data-testid="operator-proposal"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="operator-proposal-started"]').exists()).toBe(false)
  })
})

// ---- Slice 018: complete_task receipt / Undo, create_task proposal --------

const TASK_ID = '3f1c1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b8f'
const TASK_PROPOSAL_ID = '4f1c1a3e-2b6a-4c1e-9a1f-0d3e4f5a6b90'
const PERSON_ID = ID

function receipt(overrides: Partial<OperatorReceipt> = {}): OperatorReceipt {
  return {
    kind: 'complete_task',
    task_id: TASK_ID,
    person: response().references.people[0]!,
    title: 'Call about the listing',
    task_kind: 'call',
    due_at: new Date().toISOString(),
    completed_at: new Date().toISOString(),
    ...overrides,
  }
}

function taskProposal(
  expiresInMs = 120_000,
  overrides: Partial<OperatorCreateTaskProposal> = {},
): OperatorCreateTaskProposal {
  return {
    id: TASK_PROPOSAL_ID,
    kind: 'create_task',
    person: response().references.people[0]!,
    title: 'Call Grace',
    task_kind: 'follow_up',
    due_at: null,
    assignee: { id: 'user-alice', display_name: 'Alice' },
    expires_at: new Date(Date.now() + expiresInMs).toISOString(),
    ...overrides,
  }
}

const REOPEN_PATH = `/people/${PERSON_ID}/tasks/${TASK_ID}/reopen`
const CONFIRM_TASK_PATH = `/operator/proposals/${TASK_PROPOSAL_ID}/confirm`

function stubTurn(turn: OperatorTurnResponse, extra?: (path: string, init?: RequestInit) => Promise<unknown> | undefined) {
  apiFetchMock.mockImplementation((path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (method === 'POST' && path === '/operator/turns') return Promise.resolve(turn)
    const handled = extra?.(path, init)
    if (handled) return handled
    return Promise.reject(new Error(`unexpected ${method} ${path}`))
  })
}

describe('OperatorPanel — complete_task receipt and Undo (SLICE_018 §8)', () => {
  it('renders the receipt only from `receipt` — a reply claiming a different title never reaches the card', async () => {
    stubTurn(response({ receipt: receipt({ title: 'Real title' }), reply: 'I completed "A totally different title".' }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const card = wrapper.get('[data-testid="operator-receipt"]')
    expect(card.text()).toContain('Real title')
    expect(card.text()).not.toContain('A totally different title')
  })

  it('no receipt card for already_completed/forbidden outcomes (receipt is null)', async () => {
    stubTurn(response({ receipt: null, reply: 'That task is already done.' }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)
    expect(wrapper.find('[data-testid="operator-receipt"]').exists()).toBe(false)
  })

  it('Undo: 200 changed:true shows "Reopened" and finalizes the button', async () => {
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') {
        return Promise.resolve({ task: { id: TASK_ID }, changed: true })
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(apiFetchMock).toHaveBeenCalledWith(REOPEN_PATH, expect.objectContaining({ method: 'POST' }))
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe('Reopened')
    expect(wrapper.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeDefined()
  })

  it('Undo: 200 changed:false (someone reopened first) is the same final "Reopened" copy', async () => {
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') {
        return Promise.resolve({ task: { id: TASK_ID }, changed: false })
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe('Reopened')
    expect(wrapper.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeDefined()
  })

  it('Undo: 404 shows "This task no longer exists." and finalizes', async () => {
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') {
        return Promise.reject(new ApiError(404, 'not_found'))
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe('This task no longer exists.')
    expect(wrapper.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeDefined()
  })

  it('Undo: 403 shows "You can no longer change this task." and finalizes', async () => {
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') {
        return Promise.reject(new ApiError(403, 'forbidden'))
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe('You can no longer change this task.')
    expect(wrapper.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeDefined()
  })

  it('Undo: a network error stays retryable, and a second click after the error actually retries', async () => {
    let reopenCalls = 0
    let fail = true
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') {
        reopenCalls += 1
        if (fail) return Promise.reject(new ApiError(0, 'network_error'))
        return Promise.resolve({ task: { id: TASK_ID }, changed: true })
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe(
      'Could not reach the server. Check your connection and try again.',
    )
    expect(wrapper.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeUndefined()
    expect(reopenCalls).toBe(1)

    // The button is genuinely retryable, not just visually so: flip the
    // stub to succeed and click again.
    fail = false
    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(reopenCalls).toBe(2)
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe('Reopened')
    expect(wrapper.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeDefined()
  })

  // Review round 1 (W1): Undo state is keyed by the TRANSCRIPT ENTRY's id,
  // not `task_id` — two receipts for the same task (e.g. re-completed
  // after an Undo, or just two turns reporting the same task in one
  // session) must never share Undo state.
  it('keys receipt Undo state by transcript entry, not task_id: undoing the first receipt leaves a second receipt for the same task retryable', async () => {
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') {
        return Promise.resolve({ task: { id: TASK_ID }, changed: true })
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)
    await sendTurn(wrapper)

    const cards = wrapper.findAll('[data-testid="operator-receipt"]')
    expect(cards).toHaveLength(2)

    await cards[0]!.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    expect(cards[0]!.get('[data-testid="operator-receipt-message"]').text()).toBe('Reopened')
    expect(cards[0]!.get('[data-testid="operator-receipt-undo"]').attributes('disabled')).toBeDefined()

    const secondUndo = cards[1]!.get('[data-testid="operator-receipt-undo"]')
    expect(secondUndo.attributes('disabled')).toBeUndefined()
    expect(cards[1]!.find('[data-testid="operator-receipt-message"]').exists()).toBe(false)
  })
})

describe('OperatorPanel — create_task proposal card (SLICE_018 §8)', () => {
  it('renders the card from the server proposal object only: Person, title, kind, due, assignee', async () => {
    stubTurn(
      response({
        proposal: taskProposal(120_000, { due_at: '2026-09-12T23:59:00.000Z' }),
        reply: 'Ready to confirm.',
      }),
    )
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const card = wrapper.get('[data-testid="operator-task-proposal"]')
    expect(card.text()).toContain('Grace Hopper')
    expect(card.text()).toContain('Call Grace')
    expect(card.text()).toContain('Follow up')
    expect(card.text()).toContain('Alice')
  })

  it('Confirm posts to the confirm route; 201 shows "Task added." and finalizes', async () => {
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') {
        return Promise.resolve({
          task: { id: 'new-task-1', person_id: PERSON_ID, title: 'Call Grace' },
        })
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(apiFetchMock).toHaveBeenCalledWith(CONFIRM_TASK_PATH, expect.objectContaining({ method: 'POST' }))
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe('Task added.')
    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-testid="operator-task-proposal-dismiss"]').attributes('disabled')).toBeDefined()
  })

  it('Dismiss is local: no request, card finalized', async () => {
    stubTurn(response({ proposal: taskProposal() }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)
    const before = apiFetchMock.mock.calls.length

    await wrapper.get('[data-testid="operator-task-proposal-dismiss"]').trigger('click')
    await flushPromises()
    expect(apiFetchMock.mock.calls.length).toBe(before)
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe('Dismissed.')
    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
  })

  it('an expired proposal disables Confirm with the expiry copy', async () => {
    stubTurn(response({ proposal: taskProposal(-1000) }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
    expect(wrapper.get('[data-testid="operator-task-proposal-expired"]').text()).toBe(
      'This suggestion expired — ask again.',
    )
  })

  it('409 proposal_consumed shows "This task was already added." and finalizes', async () => {
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') {
        return Promise.reject(new ApiError(409, 'proposal_consumed', { task_id: 'other-task' }))
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe(
      'This task was already added.',
    )
    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
  })

  it('409 proposal_expired from confirm shows the expiry copy and finalizes', async () => {
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') {
        return Promise.reject(new ApiError(409, 'proposal_expired'))
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe(
      'This suggestion expired — ask again.',
    )
    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
  })

  // CONTRACT (docs/specs/SLICE_018.md §5, §10): a pass-through task error
  // from confirm means the backend already finalized the proposal row
  // `failed` — never retryable, regardless of which task error it is.
  it('a pass-through task error (invalid_assignee, 422) finalizes with the TASK_ERROR_COPY, and a second click posts nothing more', async () => {
    let confirmCalls = 0
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') {
        confirmCalls += 1
        return Promise.reject(new ApiError(422, 'invalid_assignee'))
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe(
      'That member is not active — ask again.',
    )
    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
    expect(confirmCalls).toBe(1)

    // Disabled means unclickable in the real DOM, but drive the handler
    // directly to prove the guard itself (not just the disabled attribute)
    // refuses a second attempt.
    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(confirmCalls).toBe(1)
  })

  it('409 proposal_consumed with a null task_id (never actually created) reads as "no longer be used", not "already added"', async () => {
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') {
        return Promise.reject(new ApiError(409, 'proposal_consumed', { task_id: null }))
      }
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe(
      'This suggestion can no longer be used — ask again.',
    )
    expect(wrapper.get('[data-testid="operator-task-proposal-confirm"]').attributes('disabled')).toBeDefined()
  })
})

// ---- Review round 1 (W6): titles are untrusted text, never markup -------

describe('OperatorPanel — receipt and create_task titles render as literal text (SLICE_018 §8)', () => {
  const HOSTILE_TITLE = 'Call <a href="https://evil.example">now</a>'

  it('a receipt title with markup renders as literal text, never a link', async () => {
    stubTurn(response({ receipt: receipt({ title: HOSTILE_TITLE }) }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const card = wrapper.get('[data-testid="operator-receipt"]')
    expect(card.text()).toContain(HOSTILE_TITLE)
    expect(card.find('a').exists()).toBe(false)
    expect(card.element.querySelector('a')).toBeNull()
  })

  it('a create_task proposal title with markup renders as literal text, never a link', async () => {
    stubTurn(response({ proposal: taskProposal(120_000, { title: HOSTILE_TITLE }) }))
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const card = wrapper.get('[data-testid="operator-task-proposal"]')
    expect(card.text()).toContain(HOSTILE_TITLE)
    expect(card.find('a').exists()).toBe(false)
    expect(card.element.querySelector('a')).toBeNull()
  })
})

describe('OperatorPanel — receipts clear with proposals on identity change (SLICE_018 §8)', () => {
  it('clears a receipt on an identity-key change', async () => {
    stubTurn(response({ receipt: receipt({ title: PRIVATE_SENTINEL }) }))
    const { wrapper, identityKey } = await mountPanel()
    await sendTurn(wrapper)
    expect(wrapper.find('[data-testid="operator-receipt"]').exists()).toBe(true)
    expect(wrapper.text()).toContain(PRIVATE_SENTINEL)

    identityKey.value = `${ORG_ID}:actor-b:0`
    await flushPromises()
    expect(wrapper.find('[data-testid="operator-receipt"]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain(PRIVATE_SENTINEL)
  })

  it('clears a create_task proposal on an identity-key change', async () => {
    stubTurn(response({ proposal: taskProposal(120_000, { title: PRIVATE_SENTINEL }) }))
    const { wrapper, identityKey } = await mountPanel()
    await sendTurn(wrapper)
    expect(wrapper.find('[data-testid="operator-task-proposal"]').exists()).toBe(true)

    identityKey.value = `${ORG_ID}:actor-b:0`
    await flushPromises()
    expect(wrapper.find('[data-testid="operator-task-proposal"]').exists()).toBe(false)
    expect(wrapper.text()).not.toContain(PRIVATE_SENTINEL)
  })
})

// ---- Review round 1 (W4): the same three keys settleTaskMutation always
// invalidates (Person detail, Today, Tasks panel) — Undo's own invalidate
// only after the reopen POST resolves, never before. -----------------------

describe('OperatorPanel — task-mutation cache invalidation (SLICE_018 §8)', () => {
  const EXPECTED_KEYS = [
    queryKeys.person(ORG_ID, PERSON_ID),
    queryKeys.today(ORG_ID),
    queryKeys.tasks(ORG_ID),
  ]

  it('a turn response carrying a receipt invalidates Person/Today/Tasks', async () => {
    stubTurn(response({ receipt: receipt() }))
    const { wrapper, queryClient } = await mountPanel()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')

    await sendTurn(wrapper)
    await waitForMacrotask()

    for (const key of EXPECTED_KEYS) {
      expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: key }))
    }
  })

  it('Undo invalidates only after the reopen POST resolves, and the POST precedes the invalidate', async () => {
    const gate = deferred<unknown>()
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') return gate.promise
      return undefined
    })
    const { wrapper, queryClient } = await mountPanel()
    await sendTurn(wrapper)
    // The turn response itself carries a receipt, which already settles
    // (invalidates) once on arrival — the spy starts only after that
    // settles, so it observes Undo's OWN invalidate in isolation.
    await waitForMacrotask()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')

    await wrapper.get('[data-testid="operator-receipt-undo"]').trigger('click')
    await flushPromises()
    await waitForMacrotask()
    expect(invalidateSpy).not.toHaveBeenCalled()

    gate.resolve({ task: { id: TASK_ID }, changed: true })
    await flushPromises()
    await waitForMacrotask()

    for (const key of EXPECTED_KEYS) {
      expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: key }))
    }
    const reopenCallIndex = apiFetchMock.mock.calls.findIndex(
      ([path, init]) => path === REOPEN_PATH && (init as RequestInit | undefined)?.method === 'POST',
    )
    expect(reopenCallIndex).toBeGreaterThan(-1)
    const reopenOrder = apiFetchMock.mock.invocationCallOrder[reopenCallIndex]!
    const firstInvalidateOrder = invalidateSpy.mock.invocationCallOrder[0]!
    expect(reopenOrder).toBeLessThan(firstInvalidateOrder)
  })

  it('a 201 create_task confirm invalidates Person/Today/Tasks', async () => {
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') {
        return Promise.resolve({ task: { id: 'new-task-1', person_id: PERSON_ID, title: 'Call Grace' } })
      }
      return undefined
    })
    const { wrapper, queryClient } = await mountPanel()
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries')
    await sendTurn(wrapper)

    await wrapper.get('[data-testid="operator-task-proposal-confirm"]').trigger('click')
    await flushPromises()
    await waitForMacrotask()

    for (const key of EXPECTED_KEYS) {
      expect(invalidateSpy).toHaveBeenCalledWith(expect.objectContaining({ queryKey: key }))
    }
  })
})

// ---- Review round 1 (W7): a second click while a mutation is in flight
// posts nothing more — the in-component guard, not just the disabled
// attribute (SLICE_018 §8, the same "drive the handler directly" style as
// the existing invalid_assignee double-click test above). --------------

describe('OperatorPanel — double-click guards (SLICE_018 §8)', () => {
  it('double-clicking Undo while the reopen is pending posts exactly one reopen request', async () => {
    const gate = deferred<unknown>()
    stubTurn(response({ receipt: receipt() }), (path, init) => {
      if (path === REOPEN_PATH && init?.method === 'POST') return gate.promise
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const undo = wrapper.get('[data-testid="operator-receipt-undo"]')
    await undo.trigger('click')
    await undo.trigger('click')
    await flushPromises()

    const reopenCalls = apiFetchMock.mock.calls.filter(
      ([path, init]) => path === REOPEN_PATH && (init as RequestInit | undefined)?.method === 'POST',
    )
    expect(reopenCalls).toHaveLength(1)

    gate.resolve({ task: { id: TASK_ID }, changed: true })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-receipt-message"]').text()).toBe('Reopened')
  })

  it('double-clicking Confirm on a create_task proposal while pending posts exactly one confirm request', async () => {
    const gate = deferred<unknown>()
    stubTurn(response({ proposal: taskProposal() }), (path, init) => {
      if (path === CONFIRM_TASK_PATH && init?.method === 'POST') return gate.promise
      return undefined
    })
    const { wrapper } = await mountPanel()
    await sendTurn(wrapper)

    const confirm = wrapper.get('[data-testid="operator-task-proposal-confirm"]')
    await confirm.trigger('click')
    await confirm.trigger('click')
    await flushPromises()

    const confirmCalls = apiFetchMock.mock.calls.filter(
      ([path, init]) => path === CONFIRM_TASK_PATH && (init as RequestInit | undefined)?.method === 'POST',
    )
    expect(confirmCalls).toHaveLength(1)

    gate.resolve({ task: { id: 'new-task-1', person_id: PERSON_ID, title: 'Call Grace' } })
    await flushPromises()
    expect(wrapper.get('[data-testid="operator-task-proposal-message"]').text()).toBe('Task added.')
  })
})
