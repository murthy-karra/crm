// SLICE_005 §13 item 5: pending/disabled states, each error code's copy,
// cards from `references` only (a reply containing a UUID or `<a>` renders
// as text), history capped at 6 and cleared by Clear, context per route.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type { OperatorTurnRequest, OperatorTurnResponse } from '../api/types'
import OperatorPanel from './OperatorPanel.vue'

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

async function mountPanel(path = '/today') {
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
  const wrapper = mount(OperatorPanel, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }]] },
    attachTo: document.body,
  })
  return { wrapper, router }
}

async function type(wrapper: Awaited<ReturnType<typeof mountPanel>>['wrapper'], text: string) {
  await wrapper.get('[data-testid="operator-input"]').setValue(text)
}

function lastRequest(): OperatorTurnRequest {
  const call = apiFetchMock.mock.calls.at(-1)
  if (!call) throw new Error('no request')
  return JSON.parse(String(call[1]?.body)) as OperatorTurnRequest
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
})

describe('OperatorPanel', () => {
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
    expect(lastRequest()).toEqual({ message: 'Who next?', history: [], context: { route: 'today' } })

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
    expect(bubble.text()).toBe(hostile)
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
    expect(wrapper.emitted('close')).toHaveLength(1)
  })
})
