// SLICE_007a §11: the Intake settings page renders the server-rendered
// address (never composes it client-side), copies it, and surfaces errors.
// SLICE_008 §7 (supersedes SLICE_007c §10): below that, the "Unattended
// lead routing" card — a three-mode picker (default assignee / round-robin
// / unassigned); the assignee dropdown (active members only, no
// "Unassigned" entry — D-041) renders only in `default_assignee` mode; the
// deactivated-default warning and the round-robin description are pinned.
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
  rotate?: () => Promise<IntakeAddressResponse>
  members?: () => Promise<MembersResponse>
  settings?: () => Promise<IntakeSettingsResponse>
  update?: (body: IntakeSettingsRequest) => Promise<IntakeSettingsResponse>
}

function stub(options: StubOptions = {}) {
  const address = options.address ?? (() => Promise.resolve({ address: ADDRESS, scheme: 'subdomain' as const }))
  const membersFn = options.members ?? (() => Promise.resolve(members()))
  const settings =
    options.settings ??
    (() => Promise.resolve({ intake_routing_mode: 'unassigned', intake_default_assignee_user_id: null }))
  apiFetchMock.mockImplementation((path: string, init?: RequestInit) => {
    const method = init?.method ?? 'GET'
    if (path === '/me') return Promise.resolve(me())
    if (path === '/organization/intake-address' && (init?.method ?? 'GET') === 'GET')
      return address()
    if (path === '/organization/intake-address/rotate' && init?.method === 'POST') {
      if (!options.rotate) return Promise.reject(new Error('unexpected rotate POST'))
      return options.rotate()
    }
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

function selects(wrapper: Awaited<ReturnType<typeof mountView>>) {
  return wrapper.findAllComponents(Select)
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

describe('IntakeSettingsView — unattended lead routing (SLICE_008 §5, D-041)', () => {
  it('unassigned mode: shows the unassigned warning, no assignee dropdown, no round-robin description', async () => {
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'unassigned', intake_default_assignee_user_id: null }),
    })
    const wrapper = await mountView()

    expect(wrapper.find('[data-testid="intake-unassigned-warning"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="intake-default-assignee"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-round-robin-description"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
  })

  it('round_robin mode: shows the rotation description, no assignee dropdown, no warnings', async () => {
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'round_robin', intake_default_assignee_user_id: null }),
    })
    const wrapper = await mountView()

    expect(wrapper.get('[data-testid="intake-round-robin-description"]').text()).toContain(
      'Rotates across all active members in join order.',
    )
    expect(wrapper.find('[data-testid="intake-default-assignee"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-unassigned-warning"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
  })

  it('round_robin mode with a stale stored assignee: still no dropdown or deactivated warning (mode gates it)', async () => {
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'round_robin', intake_default_assignee_user_id: DAVE_ID }),
    })
    const wrapper = await mountView()

    expect(wrapper.find('[data-testid="intake-default-assignee"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
  })

  it('default_assignee mode set to an active member: shows the dropdown, no warning', async () => {
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'default_assignee', intake_default_assignee_user_id: BOB_ID }),
    })
    const wrapper = await mountView()

    expect(wrapper.find('[data-testid="intake-default-assignee"]').exists()).toBe(true)
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-unassigned-warning"]').exists()).toBe(false)
    expect(wrapper.find('[data-testid="intake-round-robin-description"]').exists()).toBe(false)
  })

  it('default_assignee mode set to a since-deactivated member: shows the deactivated warning', async () => {
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'default_assignee', intake_default_assignee_user_id: DAVE_ID }),
    })
    const wrapper = await mountView()

    expect(wrapper.get('[data-testid="intake-default-assignee-deactivated-warning"]').text().length).toBeGreaterThan(
      0,
    )
    expect(wrapper.find('[data-testid="intake-unassigned-warning"]').exists()).toBe(false)
  })

  it('the mode picker lists exactly the three modes', async () => {
    stub({})
    const wrapper = await mountView()

    const modeSelect = selects(wrapper)[0]
    const options = modeSelect.props('options') as Array<{ value: string; label: string }>
    expect(options.map((o) => o.value)).toEqual(['default_assignee', 'round_robin', 'unassigned'])
  })

  it('the assignee dropdown (default_assignee mode) lists active members only — no "Unassigned" entry', async () => {
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'default_assignee', intake_default_assignee_user_id: BOB_ID }),
    })
    const wrapper = await mountView()

    const assigneeSelect = wrapper.get('[data-testid="intake-default-assignee"]')
    const options = assigneeSelect.findComponent(Select).props('options') as Array<{ display_name: string }>
    expect(options.map((o) => o.display_name)).toEqual(['Bob'])
  })

  it('choosing a mode PUTs both fields, echoing the current assignee', async () => {
    let stored: IntakeSettingsResponse = {
      intake_routing_mode: 'unassigned',
      intake_default_assignee_user_id: null,
    }
    stub({
      settings: () => Promise.resolve(stored),
      update: (body) => {
        stored = body
        return Promise.resolve(stored)
      },
    })
    const wrapper = await mountView()

    const modeSelect = selects(wrapper)[0]
    await modeSelect.vm.$emit('update:model-value', 'round_robin')
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledWith(
      '/organization/intake-settings',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ intake_routing_mode: 'round_robin', intake_default_assignee_user_id: null }),
      }),
    )
    expect(wrapper.get('[data-testid="intake-round-robin-description"]').text().length).toBeGreaterThan(0)
  })

  it('choosing an assignee (default_assignee mode) PUTs both fields, echoing the current mode', async () => {
    let stored: IntakeSettingsResponse = {
      intake_routing_mode: 'default_assignee',
      intake_default_assignee_user_id: null,
    }
    stub({
      settings: () => Promise.resolve(stored),
      update: (body) => {
        stored = body
        return Promise.resolve(stored)
      },
    })
    const wrapper = await mountView()

    const assigneeSelect = wrapper.get('[data-testid="intake-default-assignee"]').findComponent(Select)
    await assigneeSelect.vm.$emit('update:model-value', BOB_ID)
    await flushPromises()

    expect(apiFetchMock).toHaveBeenCalledWith(
      '/organization/intake-settings',
      expect.objectContaining({
        method: 'PUT',
        body: JSON.stringify({ intake_routing_mode: 'default_assignee', intake_default_assignee_user_id: BOB_ID }),
      }),
    )
    expect(wrapper.find('[data-testid="intake-default-assignee-deactivated-warning"]').exists()).toBe(false)
  })

  // Reviewer F1 (SLICE_008 review): picking default_assignee with no
  // valid stored assignee must NOT PUT (the server would 422 and the
  // admin could never reach the dropdown — a lockout). The choice is
  // held locally, the dropdown renders, and the single both-fields PUT
  // fires only once a member is chosen.
  it('picking default_assignee with a null stored assignee defers the PUT and renders the dropdown', async () => {
    const update = vi.fn()
    stub({
      settings: () =>
        Promise.resolve({ intake_routing_mode: 'unassigned', intake_default_assignee_user_id: null }),
      update,
    })
    const wrapper = await mountView()

    const modeSelect = selects(wrapper)[0]
    await modeSelect.vm.$emit('update:model-value', 'default_assignee')
    await flushPromises()

    expect(update).not.toHaveBeenCalled()
    expect(wrapper.find('[data-testid="intake-default-assignee"]').exists()).toBe(true)

    const assigneeSelect = wrapper
      .findAllComponents({ name: 'Select' })
      .find((c) => c.attributes('data-testid') === 'intake-default-assignee')!
    await assigneeSelect.vm.$emit('update:model-value', BOB_ID)
    await flushPromises()

    expect(update).toHaveBeenCalledWith({
      intake_routing_mode: 'default_assignee',
      intake_default_assignee_user_id: BOB_ID,
    })
  })

  it('a rejected PUT (e.g. 422 on an immediate-save path) surfaces the mutation error and reverts', async () => {
    stub({
      settings: () =>
        Promise.resolve({
          intake_routing_mode: 'default_assignee',
          intake_default_assignee_user_id: BOB_ID,
        }),
      update: () => Promise.reject(new ApiError(422, 'invalid_assignee', {})),
    })
    const wrapper = await mountView()

    // An immediate-save transition (default_assignee -> unassigned) that
    // the server rejects: error surfaces, server-truth model reverts.
    const modeSelect = selects(wrapper)[0]
    await modeSelect.vm.$emit('update:model-value', 'unassigned')
    await flushPromises()

    expect(wrapper.text()).toContain('Could not update the setting.')
    expect(wrapper.find('[data-testid="intake-default-assignee"]').exists()).toBe(true)
  })

  // SLICE_007g §8: break-glass rotation behind a confirm stating the
  // immediate-invalidation consequence.
  it('rotates the address behind a confirm and shows the new one', async () => {
    let current = ADDRESS
    stub({
      address: () => Promise.resolve({ address: current, scheme: 'local_part' as const }),
      rotate: () => {
        current = 'acme-realty-newtok99@leads.elysianfeld.com'
        return Promise.resolve({ address: current, scheme: 'local_part' as const })
      },
    })
    const wrapper = await mountView()

    await wrapper.find('[data-testid="rotate-address"]').trigger('click')
    await flushPromises()
    // The confirm dialog (teleported) states the consequence; nothing
    // rotated yet.
    expect(document.body.textContent).toContain('stops working immediately')
    expect(
      apiFetchMock.mock.calls.filter(([p]) => p === '/organization/intake-address/rotate'),
    ).toHaveLength(0)

    const confirm = Array.from(document.body.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Rotate' && b.closest('[role="dialog"]'),
    )
    confirm?.click()
    await flushPromises()

    expect(
      apiFetchMock.mock.calls.filter(([p]) => p === '/organization/intake-address/rotate'),
    ).toHaveLength(1)
    expect(wrapper.find('[data-testid="intake-address"]').text()).toContain('newtok99')
    wrapper.unmount()
  })
})
