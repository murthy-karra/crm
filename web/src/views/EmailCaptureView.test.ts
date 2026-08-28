// SLICE_009 §11: the agent's own capture address (render, copy, rotate
// behind a confirm) and the unmatched held queue (render, link with a
// chosen Person + optional contact-method add, dismiss) — mirrors
// IntakeSettingsView.test.ts's structure for the address/rotate half.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type {
  CaptureAddressResponse,
  CaptureUnmatchedResponse,
  MeResponse,
  PeopleResponse,
} from '../api/types'
import EmailCaptureView from './EmailCaptureView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ADDRESS = 'save-abcdefghijkl@leads.elysianfeld.com'
const PERSON_ID = '22222222-2222-2222-2222-222222222222'
const HELD_ID = '33333333-3333-3333-3333-333333333333'

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'member' },
    platform_admin: false,
  }
}

function people(): PeopleResponse {
  return {
    people: [
      {
        id: PERSON_ID,
        first_name: 'Grace',
        last_name: 'Hopper',
        display_name: 'Grace Hopper',
        stage: { id: 's-1', name: 'Lead' },
        assigned_user: null,
        primary_email: 'grace@example.com',
        primary_phone: null,
        inquiry_count: 1,
        last_inquiry_at: null,
        created_at: '2026-08-01T00:00:00.000Z',
      },
    ],
    truncated: false,
  }
}

function heldItem(overrides: Partial<CaptureUnmatchedResponse['items'][number]> = {}) {
  return {
    id: HELD_ID,
    counterparty_email: 'client@example.com',
    captured_at: '2026-08-27T09:00:00.000Z',
    direction_hint: 'inbound' as const,
    status: 'held' as const,
    ...overrides,
  }
}

interface StubOptions {
  address?: () => Promise<CaptureAddressResponse>
  rotate?: () => Promise<CaptureAddressResponse>
  unmatched?: () => Promise<CaptureUnmatchedResponse>
  link?: (id: string, body: unknown) => Promise<{ status: 'linked' }>
  dismiss?: (id: string) => Promise<{ status: 'dismissed' }>
}

function stub(options: StubOptions = {}) {
  const address = options.address ?? (() => Promise.resolve({ address: ADDRESS }))
  const unmatched = options.unmatched ?? (() => Promise.resolve({ items: [], truncated: false }))
  apiFetchMock.mockImplementation((path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return Promise.resolve(me())
    if (path === '/people') return Promise.resolve(people())
    if (path === '/capture/address' && method === 'GET') return address()
    if (path === '/capture/address/rotate' && method === 'POST') {
      if (!options.rotate) return Promise.reject(new Error('unexpected rotate POST'))
      return options.rotate()
    }
    if (path === '/capture/unmatched' && method === 'GET') return unmatched()
    const linkMatch = /^\/capture\/unmatched\/(.+)\/link$/.exec(path)
    if (linkMatch && method === 'POST') {
      if (!options.link) return Promise.reject(new Error('unexpected link POST'))
      return options.link(linkMatch[1]!, JSON.parse((init?.body as string) ?? '{}'))
    }
    const dismissMatch = /^\/capture\/unmatched\/(.+)\/dismiss$/.exec(path)
    if (dismissMatch && method === 'POST') {
      if (!options.dismiss) return Promise.reject(new Error('unexpected dismiss POST'))
      return options.dismiss(dismissMatch[1]!)
    }
    return Promise.reject(new Error(`unexpected ${method} ${path}`))
  })
}

async function mountView() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(EmailCaptureView, {
    global: { plugins: [[VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  await flushPromises()
  return wrapper
}

beforeEach(() => {
  apiFetchMock.mockReset()
})

afterEach(() => {
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
})

describe('EmailCaptureView — address card', () => {
  it('renders the server-rendered address verbatim and copies it', async () => {
    stub()
    const writeText = vi.fn(() => Promise.resolve())
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    const wrapper = await mountView()

    expect(wrapper.get('[data-testid="capture-address"]').text()).toBe(ADDRESS)
    await wrapper.get('[data-testid="capture-copy"]').trigger('click')
    await flushPromises()
    expect(writeText).toHaveBeenCalledWith(ADDRESS)
    expect(wrapper.get('[data-testid="capture-copy"]').text()).toContain('Copied')
  })

  it('shows the error copy when the address endpoint fails', async () => {
    stub({ address: () => Promise.reject(new ApiError(503, 'unavailable', {})) })
    const wrapper = await mountView()
    expect(wrapper.get('[data-testid="capture-address-error"]').text().length).toBeGreaterThan(0)
    expect(wrapper.find('[data-testid="capture-address"]').exists()).toBe(false)
  })

  it('the signature snippet embeds the current address and copies independently of the address button', async () => {
    stub()
    const writeText = vi.fn(() => Promise.resolve())
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    const wrapper = await mountView()

    expect(wrapper.get('[data-testid="capture-signature-snippet"]').text()).toContain(ADDRESS)
    await wrapper.get('[data-testid="capture-copy-signature"]').trigger('click')
    await flushPromises()
    expect(writeText).toHaveBeenCalledWith(expect.stringContaining(ADDRESS))
    expect(wrapper.get('[data-testid="capture-copy-signature"]').text()).toContain('Copied')
    // Copying the signature must not also mark the address button copied.
    expect(wrapper.get('[data-testid="capture-copy"]').text()).toContain('Copy')
  })

  it('rotates the address behind a confirm and shows the new one', async () => {
    let current = ADDRESS
    stub({
      address: () => Promise.resolve({ address: current }),
      rotate: () => {
        current = 'save-newtoken999@leads.elysianfeld.com'
        return Promise.resolve({ address: current })
      },
    })
    const wrapper = await mountView()

    await wrapper.find('[data-testid="rotate-capture-address"]').trigger('click')
    await flushPromises()
    expect(document.body.textContent).toContain('stops working immediately')
    expect(
      apiFetchMock.mock.calls.filter(([p]) => p === '/capture/address/rotate'),
    ).toHaveLength(0)

    const confirm = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Rotate' && b.closest('[role="dialog"]'),
    )
    confirm?.click()
    await flushPromises()

    expect(
      apiFetchMock.mock.calls.filter(([p]) => p === '/capture/address/rotate'),
    ).toHaveLength(1)
    expect(wrapper.find('[data-testid="capture-address"]').text()).toContain('newtoken999')
    wrapper.unmount()
  })
})

describe('EmailCaptureView — unmatched held queue', () => {
  it('shows the empty state when nothing is held', async () => {
    stub({ unmatched: () => Promise.resolve({ items: [], truncated: false }) })
    const wrapper = await mountView()
    expect(wrapper.find('[data-testid="capture-unmatched-empty"]').exists()).toBe(true)
  })

  it('shows the error copy when the unmatched endpoint fails', async () => {
    stub({ unmatched: () => Promise.reject(new ApiError(503, 'unavailable', {})) })
    const wrapper = await mountView()
    expect(wrapper.get('[data-testid="capture-unmatched-error"]').text().length).toBeGreaterThan(0)
  })

  it('renders a held row with its counterparty address and direction hint, never the status column value hardcoded', async () => {
    stub({
      unmatched: () =>
        Promise.resolve({ items: [heldItem({ direction_hint: 'outbound' })], truncated: false }),
    })
    const wrapper = await mountView()
    const row = wrapper.get(`[data-testid="capture-unmatched-row-${HELD_ID}"]`)
    expect(row.text()).toContain('client@example.com')
    expect(row.text()).toContain('Presumed outbound')
  })

  it('renders "(unknown address)" when counterparty_email is null (the pathological no-From, no-recipient edge)', async () => {
    stub({
      unmatched: () =>
        Promise.resolve({
          items: [heldItem({ counterparty_email: null, direction_hint: null })],
          truncated: false,
        }),
    })
    const wrapper = await mountView()
    const row = wrapper.get(`[data-testid="capture-unmatched-row-${HELD_ID}"]`)
    expect(row.text()).toContain('(unknown address)')
    expect(row.text()).toContain('Unknown')
  })

  it('links to a chosen Person with add_contact_method true by default, then closes the form', async () => {
    const link = vi.fn(() => Promise.resolve({ status: 'linked' as const }))
    stub({
      unmatched: () => Promise.resolve({ items: [heldItem()], truncated: false }),
      link,
    })
    const wrapper = await mountView()

    await wrapper.get(`[data-testid="link-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    const personSelect = wrapper.get(`[data-testid="link-person-select-${HELD_ID}"]`).findComponent(Select)
    await personSelect.vm.$emit('update:model-value', PERSON_ID)
    await flushPromises()

    await wrapper.get(`[data-testid="confirm-link-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    expect(link).toHaveBeenCalledWith(HELD_ID, { person_id: PERSON_ID, add_contact_method: true })
    expect(wrapper.find(`[data-testid="link-person-select-${HELD_ID}"]`).exists()).toBe(false)
  })

  it('unchecking "add as contact method" sends add_contact_method false', async () => {
    const link = vi.fn(() => Promise.resolve({ status: 'linked' as const }))
    stub({
      unmatched: () => Promise.resolve({ items: [heldItem()], truncated: false }),
      link,
    })
    const wrapper = await mountView()

    await wrapper.get(`[data-testid="link-${HELD_ID}"]`).trigger('click')
    await flushPromises()
    await wrapper.get(`[data-testid="link-add-contact-method-${HELD_ID}"]`).setValue(false)
    const personSelect = wrapper.get(`[data-testid="link-person-select-${HELD_ID}"]`).findComponent(Select)
    await personSelect.vm.$emit('update:model-value', PERSON_ID)
    await wrapper.get(`[data-testid="confirm-link-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    expect(link).toHaveBeenCalledWith(HELD_ID, { person_id: PERSON_ID, add_contact_method: false })
  })

  it('the Link button is disabled until a Person is chosen', async () => {
    stub({ unmatched: () => Promise.resolve({ items: [heldItem()], truncated: false }) })
    const wrapper = await mountView()

    await wrapper.get(`[data-testid="link-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    const confirmButton = wrapper.get(`[data-testid="confirm-link-${HELD_ID}"]`)
    expect((confirmButton.element as HTMLButtonElement).disabled).toBe(true)
  })

  it('a 409 conflict on link surfaces a row-scoped error and keeps the form open', async () => {
    stub({
      unmatched: () => Promise.resolve({ items: [heldItem()], truncated: false }),
      link: () => Promise.reject(new ApiError(409, 'capture_conflict', {})),
    })
    const wrapper = await mountView()

    await wrapper.get(`[data-testid="link-${HELD_ID}"]`).trigger('click')
    await flushPromises()
    const personSelect = wrapper.get(`[data-testid="link-person-select-${HELD_ID}"]`).findComponent(Select)
    await personSelect.vm.$emit('update:model-value', PERSON_ID)
    await wrapper.get(`[data-testid="confirm-link-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Could not link this message.')
    // The form stays open so the agent can retry or pick someone else.
    expect(wrapper.find(`[data-testid="link-person-select-${HELD_ID}"]`).exists()).toBe(true)
  })

  it('dismisses a held row directly, without a confirm dialog', async () => {
    const dismiss = vi.fn(() => Promise.resolve({ status: 'dismissed' as const }))
    stub({
      unmatched: () => Promise.resolve({ items: [heldItem()], truncated: false }),
      dismiss,
    })
    const wrapper = await mountView()

    await wrapper.get(`[data-testid="dismiss-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    expect(dismiss).toHaveBeenCalledWith(HELD_ID)
    expect(document.body.querySelector('[role="dialog"]')).toBeNull()
  })

  it('a failed dismiss surfaces a row-scoped error', async () => {
    stub({
      unmatched: () => Promise.resolve({ items: [heldItem()], truncated: false }),
      dismiss: () => Promise.reject(new ApiError(409, 'capture_conflict', {})),
    })
    const wrapper = await mountView()

    await wrapper.get(`[data-testid="dismiss-${HELD_ID}"]`).trigger('click')
    await flushPromises()

    expect(wrapper.text()).toContain('Could not dismiss this message.')
  })
})
