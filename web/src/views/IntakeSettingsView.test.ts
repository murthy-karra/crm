// SLICE_007a §11: the Intake settings page renders the server-rendered
// address (never composes it client-side), copies it, and surfaces errors.
// SLICE_007c §10: below that, the "Unattended lead routing" card — dropdown
// lists active members only, the unset and deactivated-default warnings.
import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import type {
  IntakeAddressResponse,
  IntakeSettingsRequest,
  IntakeSettingsResponse,
  MeResponse,
  MembersResponse,
} from '../api/types'
import IntakeSettingsView from './IntakeSettingsView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})
const apiFetchMock = vi.mocked(apiFetch)

const ORG_ID = '11111111-1111-1111-1111-111111111111'
const ADDRESS = 'leads-k7f3q2wd@acme-realty.elysianfeld.com'
const BOB_ID = '22222222-2222-2222-2222-222222222222'
const DAVE_ID = '33333333-3333-3333-3333-333333333333'

function me(): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@acme.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Acme Realty', role: 'admin' },
    platform_admin: false,
  }
}

function members(): MembersResponse {
  return {
    members: [
      {
        user_id: BOB_ID,
        display_name: 'Bob',
        email: 'bob@acme.test',
        role: 'member',
        status: 'active',
        joined_at: '2026-08-20T00:00:00.000Z',
        assigned_people_count: 0,
      },
      {
        user_id: DAVE_ID,
        display_name: 'Dave',
        email: 'dave@acme.test',
        role: 'member',
        status: 'inactive',
        joined_at: '2026-08-20T00:00:00.000Z',
        assigned_people_count: 0,
      },
    ],
  }
}

interface StubOptions {
  address?: () => Promise<IntakeAddressResponse>
  members?: () => Promise<MembersResponse>
  settings?: () => Promise<IntakeSettingsResponse>
  update?: (body: IntakeSettingsRequest) => Promise<IntakeSettingsResponse>
}

function stub(options: StubOptions = {}) {
  const address = options.address ?? (() => Promise.resolve({ address: ADDRESS, scheme: 'subdomain' as const }))
  const membersFn = options.members ?? (() => Promise.resolve(members()))
  const settings = options.settings ?? (() => Promise.resolve({ intake_default_assignee_user_id: null }))
  apiFetchMock.mockImplementation((path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return Promise.resolve(me())
    if (path === '/organization/intake-address') return address()
    if (path === '/organization/members') return membersFn()
    if (path === '/organization/intake-settings' && method === 'GET') return settings()
    if (path === '/organization/intake-settings' && method === 'PUT') {
      if (!options.update) return Promise.reject(new Error('unexpected PUT /organization/intake-settings'))
      return options.update(JSON.parse((init?.body as string) ?? '{}'))
    }
    return Promise.reject(new Error(`unexpected ${method} ${path}`))
  })
}

async function mountView() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(IntakeSettingsView, {
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

describe('IntakeSettingsView', () => {
  it('renders the server-rendered address verbatim and copies it', async () => {
    stub({ address: () => Promise.resolve({ address: ADDRESS, scheme: 'subdomain' }) })
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
    stub({ address: () => Promise.reject(new ApiError(403, 'forbidden', {})) })
    const wrapper = await mountView()
    expect(wrapper.get('[data-testid="intake-error"]').text().length).toBeGreaterThan(0)
    expect(wrapper.find('[data-testid="intake-address"]').exists()).toBe(false)
  })
})

describe('IntakeSettingsView — unattended lead routing (SLICE_007c §6)', () => {
  it('unset: shows the unset warning and no deactivated warning', async () => {
    stub({ settings: () => Promise.resolve({ intake_default_assignee_user_id: null }) })
    const wrapper = await mountView()

    expect(wrapper.find('[data-testid="intake-default-assignee-unset-warning"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
  })

  it('set to an active member: no warning at all', async () => {
    stub({ settings: () => Promise.resolve({ intake_default_assignee_user_id: BOB_ID }) })
    const wrapper = await mountView()

    expect(wrapper.find('[data-testid="intake-default-assignee-unset-warning"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
  })

  it('set to a since-deactivated member: shows the deactivated warning, not the unset one', async () => {
    stub({ settings: () => Promise.resolve({ intake_default_assignee_user_id: DAVE_ID }) })
    const wrapper = await mountView()

    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="intake-default-assignee-unset-warning"]').exists()).toBe(false)
  })

  it('the dropdown lists only active members plus Unassigned', async () => {
    stub({})
    const wrapper = await mountView()

    const options = wrapper.findComponent(Select).props('options') as Array<{ display_name: string }>
    expect(options.map((o) => o.display_name)).toEqual(['Unassigned', 'Bob'])
  })

  it('choosing a member PUTs the new value and the warning updates', async () => {
    let stored: string | null = null
    stub({
      settings: () => Promise.resolve({ intake_default_assignee_user_id: stored }),
      update: (body) => {
        stored = body.intake_default_assignee_user_id
        return Promise.resolve({ intake_default_assignee_user_id: stored })
      },
    })
    const wrapper = await mountView()
    expect(wrapper.find('[data-testid="intake-default-assignee-unset-warning"]').exists()).toBe(true)

    await wrapper.findComponent(Select).vm.$emit('update:model-value', BOB_ID)
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledWith(
      '/organization/intake-settings',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ intake_default_assignee_user_id: BOB_ID }),
      }),
    )
  })

  it('choosing Unassigned PUTs an explicit null, not an omitted key', async () => {
    let stored: string | null = BOB_ID
    stub({
      settings: () => Promise.resolve({ intake_default_assignee_user_id: stored }),
      update: (body) => {
        stored = body.intake_default_assignee_user_id
        return Promise.resolve({ intake_default_assignee_user_id: stored })
      },
    })
    const wrapper = await mountView()
    expect(wrapper.find('[data-testid="intake-default-assignee-unset-warning"]').exists()).toBe(false)

    // The PrimeVue Select emits `null` for the "Unassigned" option (its
    // `option-value` is `user_id`, which is `null` for that entry).
    await wrapper.findComponent(Select).vm.$emit('update:model-value', null)
    await flushPromises()

    // The backend's contract (SLICE_007c §5) treats an absent key as 400
    // and only an explicit `null` as a clear — the PUT body must carry
    // the key, not omit it.
    const call = apiFetchMock.mock.calls.find(
      ([path, init]) => path === '/organization/intake-settings' && init?.method === 'PUT',
    )
    expect(call).toBeDefined()
    const body = JSON.parse((call?.[1]?.body as string) ?? '{}')
    expect(Object.prototype.hasOwnProperty.call(body, 'intake_default_assignee_user_id')).toBe(true)
    expect(body.intake_default_assignee_user_id).toBeNull()

    expect(wrapper.find('[data-testid="intake-default-assignee-unset-warning"]').exists()).toBe(true)
  })
})
