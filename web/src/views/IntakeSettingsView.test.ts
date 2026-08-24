// SLICE_007a §11: the Intake settings page renders the server-rendered
// address (never composes it client-side), copies it, and surfaces errors.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type { IntakeAddressResponse, MeResponse } from '../api/types'
import IntakeSettingsView from './IntakeSettingsView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ADDRESS = 'leads-k7f3q2wd@acme-realty.elysianfeld.com'

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'admin' },
    platform_admin: false,
  }
}

function stub(addressResult: () => Promise<IntakeAddressResponse>) {
  apiFetchMock.mockImplementation((path: string) => {
    if (path === '/me') return Promise.resolve(me())
    if (path === '/organization/intake-address') return addressResult()
    return Promise.reject(new Error(`unexpected ${path}`))
  })
}

async function mountView() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(IntakeSettingsView, {
    global: { plugins: [[VueQueryPlugin, { queryClient }]] },
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

describe('IntakeSettingsView', () => {
  it('renders the server-rendered address verbatim and copies it', async () => {
    stub(() => Promise.resolve({ address: ADDRESS, scheme: 'subdomain' }))
    const writeText = vi.fn(() => Promise.resolve())
    vi.stubGlobal('navigator', { clipboard: { writeText } })
    const wrapper = await mountView()

    expect(wrapper.get('[data-testid="intake-address"]').text()).toBe(ADDRESS)
    await wrapper.get('[data-testid="intake-copy"]').trigger('click')
    await flushPromises()
    expect(writeText).toHaveBeenCalledWith(ADDRESS)
    expect(wrapper.get('[data-testid="intake-copy"]').text()).toContain('Copied')
  })

  it('shows the error copy when the endpoint fails (e.g. 403 for a member)', async () => {
    stub(() => Promise.reject(new ApiError(403, 'forbidden', {})))
    const wrapper = await mountView()
    expect(wrapper.get('[data-testid="intake-error"]').text().length).toBeGreaterThan(0)
    expect(wrapper.find('[data-testid="intake-address"]').exists()).toBe(false)
  })
})
