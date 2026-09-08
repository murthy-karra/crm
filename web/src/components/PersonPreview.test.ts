import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import { createMemoryHistory, createRouter } from 'vue-router'
import { computed, ref } from 'vue'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from '../api/client'
import type { PersonDetailResponse } from '../api/types'
import { CALL_HOST_KEY, type CallHost } from '../telephony/callHost'
import { OPERATOR_LAUNCHER } from '../lib/operatorLauncher'
import PersonPreview from './PersonPreview.vue'

vi.mock('../api/client', async (importOriginal) => ({ ...await importOriginal<typeof import('../api/client')>(), apiFetch: vi.fn() }))
const fixture: PersonDetailResponse = {
  person: {
    id: 'person-1', first_name: 'Grace', last_name: 'Hopper', display_name: 'Grace Hopper',
    stage: { id: 'stage-1', name: 'Lead' }, assigned_user: null, primary_email: 'grace@example.com',
    primary_phone: '+12025550101', inquiry_count: 0, last_inquiry_at: null, created_at: '2026-09-01T12:00:00Z',
  },
  contact_methods: [
    { id: 'phone-1', kind: 'phone', value: '+12025550101' },
    { id: 'phone-2', kind: 'phone', value: '+12025550102' },
    { id: 'email-1', kind: 'email', value: 'grace@example.com' },
  ],
  inquiries: [], history: [],
  tags: [{ id: 'tag-sphere', name: 'Sphere' }, { id: 'tag-investor', name: 'Investor' }],
}
const cleanups: Array<() => void> = []
afterEach(() => { cleanups.splice(0).forEach((cleanup) => cleanup()); vi.clearAllMocks() })

async function mountPreview() {
  vi.mocked(apiFetch).mockResolvedValue(fixture)
  const active = ref(false)
  const outcome = ref(false)
  const start = vi.fn()
  const launch = vi.fn()
  const host = { call: { active }, outcomePromptOpen: computed(() => outcome.value), startFromPerson: start } as unknown as CallHost
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/:pathMatch(.*)*', component: { template: '<div />' } }] })
  await router.push('/people')
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } })
  const wrapper = mount(PersonPreview, {
    props: { orgId: 'org-1', summary: fixture.person },
    global: {
      plugins: [router, [VueQueryPlugin, { queryClient: client }]],
      provide: { [CALL_HOST_KEY as symbol]: host, [OPERATOR_LAUNCHER as symbol]: launch },
    },
  })
  cleanups.push(() => { wrapper.unmount(); client.clear() })
  await flushPromises()
  return { wrapper, active, outcome, start, launch }
}

describe('Person preview actions', () => {
  it('requires an explicit number choice and uses the existing call host', async () => {
    const { wrapper, start, active, outcome } = await mountPreview()
    expect(start).not.toHaveBeenCalled()
    const button = wrapper.get('[aria-label="Call person"]')
    await button.trigger('click')
    expect(start).not.toHaveBeenCalled()
    const options = wrapper.findAll('[aria-label="Choose a phone number"] button')
    expect(options).toHaveLength(2)
    await options[1]!.trigger('click')
    expect(start).toHaveBeenCalledExactlyOnceWith('person-1', 'Grace Hopper', 'phone-2')
    active.value = true
    await flushPromises()
    expect(button.attributes('disabled')).toBeDefined()
    active.value = false
    outcome.value = true
    await flushPromises()
    expect(button.attributes('disabled')).toBeDefined()
  })

  // SLICE_011e §5, §9.9: read-only chips under the name — no remove control,
  // no separate fetch (the preview already reads the detail query).
  it('shows read-only tag chips with no controls and no extra fetch', async () => {
    const { wrapper } = await mountPreview()
    const chips = wrapper.get('[data-testid="person-preview-tags"]')
    expect(chips.text()).toContain('Sphere')
    expect(chips.text()).toContain('Investor')
    expect(chips.findAll('button')).toHaveLength(0)
    expect(vi.mocked(apiFetch).mock.calls.filter(([path]) => String(path).includes('/tags'))).toHaveLength(0)
  })

  it('only launches Operator on activation and exposes email as a client link', async () => {
    const { wrapper, launch } = await mountPreview()
    expect(launch).not.toHaveBeenCalled()
    expect(wrapper.get('[aria-label="Open email app"]').attributes('href')).toBe('mailto:grace%40example.com')
    const ask = wrapper.findAll('button').find((button) => button.text().includes('Ask about this person'))!
    await ask.trigger('click')
    expect(launch).toHaveBeenCalledExactlyOnceWith('person-1')
    expect(vi.mocked(apiFetch).mock.calls.every(([, options]) => !options?.method || options.method === 'GET')).toBe(true)
  })
})
