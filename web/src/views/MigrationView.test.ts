import { flushPromises, mount } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import { createMemoryHistory, createRouter } from 'vue-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../api/client'
import { queryKeys } from '../api/queries'
import type { MeResponse } from '../api/types'
import type { FubAssessment, FubMigrationSummary } from '../api/migrations'
import MigrationView from './MigrationView.vue'

vi.mock('../api/client', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../api/client')>()
  return { ...actual, apiFetch: vi.fn() }
})

const apiFetchMock = vi.mocked(apiFetch)
const ORG_ID = '11111111-1111-1111-1111-111111111111'
const CONNECTION_ID = '22222222-2222-2222-2222-222222222222'
const ASSESSMENT_ID = '33333333-3333-3333-3333-333333333333'
const SECRET = 'fub-secret-never-cache'
const cleanup: Array<() => void> = []

function me(role: 'admin' | 'member' = 'admin'): MeResponse {
  return {
    user: { id: 'u-alice', email: 'alice@example.test', display_name: 'Alice' },
    organization: { id: ORG_ID, name: 'Example Realty', workspace_mode: 'operational', workspace_revision: '1', role },
    platform_admin: false,
  }
}

function report(state: FubAssessment['state'] = 'completed', probesCompleted = true): FubAssessment {
  const checks = [
    'identity', 'people_excluding_trash', 'people_including_trash', 'users', 'stages', 'custom_fields',
    'notes', 'tasks', 'tags', 'inquiries_history', 'calls', 'texts', 'emails', 'recordings', 'addresses',
    'relationships', 'appointments', 'deals', 'automation_settings',
  ].map((check_key, index) => ({
    check_key,
    state: index < 6 && !probesCompleted ? 'pending' : 'completed',
    reported_total: index < 2 ? String(index + 3) : null,
    retrieved_count: index < 6 ? '1' : '0',
    destination_readiness: index < 6 ? 'model_available' : 'destination_missing',
    reason_codes: index < 6 ? ['bounded_first_page_only'] : ['not_checked_by_profile'],
    next_action: index < 6 ? 'Review mappings.' : 'Assess this family later.',
    coverage: index < 6 ? 'partial' : 'not_checked',
    error_code: null,
    attempts: index < 6 ? 1 : 0,
    observed_at: index < 6 ? '2026-09-10T12:00:00.000Z' : null,
  }))
  return {
    destination_organization_id: ORG_ID,
    source_account_id: 42,
    source_display_name: 'acme.fub.example',
    source_access_scope: 'unknown',
    id: ASSESSMENT_ID,
    connection_id: CONNECTION_ID,
    connection_revision: 1,
    profile_version: 'fub_assessment_v1',
    state,
    pause_reason: state === 'paused' ? 'source_unavailable' : null,
    created_at: '2026-09-10T12:00:00.000Z',
    started_at: '2026-09-10T12:00:01.000Z',
    completed_at: state === 'completed' ? '2026-09-10T12:00:10.000Z' : null,
    checks,
  }
}

function summary(overrides: Partial<FubMigrationSummary> = {}): FubMigrationSummary {
  return {
    connection: {
      id: CONNECTION_ID, source_account_id: 42, source_display_name: 'acme.fub.example', source_access_scope: 'unknown',
      status: 'connected', revision: 1, created_at: '2026-09-10T12:00:00.000Z', updated_at: '2026-09-10T12:00:00.000Z',
    },
    active_assessment: null,
    latest_assessment: report(),
    latest_report: report(),
    ...overrides,
  }
}

function stub(getSummary: () => FubMigrationSummary, role: 'admin' | 'member' = 'admin') {
  apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
    if (path === '/me') return me(role)
    if (path.startsWith('/migrations/fub/imports?')) return { imports: [], next_cursor: null }
    if (path.startsWith('/migrations/fub/snapshots?')) return { snapshots: [], next_cursor: null, active_snapshot_id: null, latest_completed_snapshot_id: null }
    if (path === '/migrations/fub/' && (init?.method ?? 'GET') === 'GET') return getSummary()
    if (path === '/migrations/fub/connections' && init?.method === 'POST') return { connection: summary().connection, request_id: 'request-id' }
    if (path === `/migrations/fub/connections/${CONNECTION_ID}/credential` && init?.method === 'PUT') return { connection: summary().connection, request_id: 'request-id' }
    if (path === '/migrations/fub/assessments' && init?.method === 'POST') return { assessment: report('queued'), request_id: 'request-id' }
    if (path === `/migrations/fub/assessments/${ASSESSMENT_ID}/retry` && init?.method === 'POST') return { assessment: report('queued') }
    if (path === `/migrations/fub/assessments/${ASSESSMENT_ID}/cancel` && init?.method === 'POST') return { assessment: report('cancelled') }
    if (path === `/migrations/fub/connections/${CONNECTION_ID}` && init?.method === 'DELETE') return undefined
    throw new Error(`unexpected ${init?.method ?? 'GET'} ${path}`)
  })
}

async function mountView(getSummary: () => FubMigrationSummary, role: 'admin' | 'member' = 'admin') {
  stub(getSummary, role)
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const router = createRouter({ history: createMemoryHistory(), routes: [{ path: '/', component: MigrationView }] })
  await router.push('/')
  await router.isReady()
  const wrapper = mount(MigrationView, {
    global: { plugins: [router, [VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] },
    attachTo: document.body,
  })
  cleanup.push(() => { wrapper.unmount(); queryClient.clear() })
  await flushPromises()
  return { wrapper, queryClient, router }
}

beforeEach(() => {
  apiFetchMock.mockReset()
  vi.stubGlobal('crypto', { randomUUID: () => '44444444-4444-4444-4444-444444444444' })
})
afterEach(() => {
  cleanup.splice(0).forEach((fn) => fn())
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
  vi.useRealTimers()
})

describe('MigrationView', () => {
  it('submits a replacement key directly, clears it, and never retains it in TanStack state', async () => {
    const { wrapper, queryClient } = await mountView(() => summary())
    await wrapper.get('[data-testid="fub-api-key"]').setValue(SECRET)
    await wrapper.get('form').trigger('submit')
    await flushPromises()

    const call = apiFetchMock.mock.calls.find(([path, init]) => path.endsWith('/credential') && init?.method === 'PUT')
    expect(JSON.parse(String(call?.[1]?.body))).toEqual({ request_id: '44444444-4444-4444-4444-444444444444', expected_revision: 1, api_key: SECRET })
    expect((wrapper.get('[data-testid="fub-api-key"]').element as HTMLInputElement).value).toBe('')
    expect(queryClient.getMutationCache().getAll()).toHaveLength(0)
    expect(JSON.stringify(queryClient.getQueryCache().getAll().map((query) => query.state.data))).not.toContain(SECRET)
    expect(window.localStorage.getItem('fub-api-key')).toBeNull()
  })

  it('keeps a completed report visibly prior while a new assessment is active', async () => {
    const previous = {
      ...report(),
      id: '55555555-5555-5555-5555-555555555555',
      source_display_name: 'previous.fub.example',
      checks: report().checks.map((check, index) => ({ ...check, retrieved_count: index < 6 ? '7' : '0' })),
    }
    const active = report('running')
    const { wrapper } = await mountView(() => summary({ active_assessment: active, latest_assessment: active, latest_report: previous }))
    expect(wrapper.get('[data-testid="active-assessment"]').text()).toContain('Current assessment')
    expect(wrapper.get('[data-testid="previous-report"]').text()).toContain('Previous completed report')
    expect(wrapper.get('[data-testid="previous-report"]').text()).toContain('This remains the prior result')
    expect(wrapper.get('[data-testid="assessment-progress-live"]').text()).toContain('6 of 6 source checks completed')
    expect(wrapper.findAll('tbody tr')).toHaveLength(19)
    expect(wrapper.text()).toContain('Not checked by this profile')
    expect(wrapper.get('[data-testid="assessment-report"]').text()).toContain('Destination: Example Realty')
    expect(wrapper.get('[data-testid="assessment-report"]').text()).not.toContain(`Destination: ${ORG_ID}`)
    expect(wrapper.get('[data-testid="assessment-report"]').text()).toContain('Current report')
    const addressesRow = wrapper.findAll('tbody tr').find((row) => row.text().includes('Addresses'))
    expect(addressesRow?.findAll('td')[1]?.text()).toBe('Not checked')
    await wrapper.get('[data-testid="toggle-previous-report"]').trigger('click')
    expect(wrapper.get('[data-testid="assessment-report"]').text()).toContain('Previous completed report')
    expect(wrapper.get('[data-testid="assessment-report"]').text()).toContain('previous.fub.example')
    expect(wrapper.get('[data-testid="assessment-report"]').text()).toContain('7')
  })

  it('recovers an uncertain credential submit by refetching status instead of retaining the key', async () => {
    let reads = 0
    const { wrapper } = await mountView(() => { reads += 1; return summary() })
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/migrations/fub/') return summary()
      if (path.endsWith('/credential') && init?.method === 'PUT') throw new ApiError(0, 'network_error')
      throw new Error(`unexpected ${path}`)
    })
    await wrapper.get('[data-testid="fub-api-key"]').setValue(SECRET)
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    expect(wrapper.get('[data-testid="credential-error"]').text()).toContain('Recover the connection status')
    expect((wrapper.get('[data-testid="fub-api-key"]').element as HTMLInputElement).value).toBe('')
    expect(wrapper.findAll('[data-testid="recover-fub-status"]')).toHaveLength(1)
    expect(reads).toBeGreaterThanOrEqual(1)
  })

  it('does not read the source summary for a member session', async () => {
    const { wrapper } = await mountView(() => summary(), 'member')
    expect(wrapper.text()).toContain('Follow Up Boss connection')
    expect(apiFetchMock.mock.calls.filter(([path]) => path === '/migrations/fub/')).toHaveLength(0)
  })

  it('stops polling a paused assessment and exposes Retry', async () => {
    const { wrapper } = await mountView(() => summary({ active_assessment: report('paused'), latest_assessment: report('paused') }))
    expect(wrapper.get('[data-testid="retry-fub"]').text()).toContain('Retry')
    expect(wrapper.findAll('[data-testid="cancel-fub"]')).toHaveLength(1)
  })

  it('polls a durable active assessment every two seconds', async () => {
    vi.useFakeTimers()
    await mountView(() => summary({ active_assessment: report('running'), latest_assessment: report('running') }))
    const initialReads = apiFetchMock.mock.calls.filter(([path]) => path === '/migrations/fub/').length
    await vi.advanceTimersByTimeAsync(2_050)
    expect(apiFetchMock.mock.calls.filter(([path]) => path === '/migrations/fub/').length).toBeGreaterThan(initialReads)
  })

  it('reports queued work as zero of the six source checks while unprobed rows are completed', async () => {
    const queued = report('queued', false)
    const { wrapper } = await mountView(() => summary({ active_assessment: queued, latest_assessment: queued }))
    expect(wrapper.get('[data-testid="assessment-progress-live"]').text()).toContain('0 of 6 source checks completed')
  })

  it('does not refresh a detached view when a credential request resolves late', async () => {
    let resolveCredential!: () => void
    const credential = new Promise<void>((resolve) => { resolveCredential = resolve })
    const { wrapper } = await mountView(() => summary())
    apiFetchMock.mockImplementation(async (path: string, init?: RequestInit) => {
      if (path === '/me') return me()
      if (path === '/migrations/fub/') return summary()
      if (path.endsWith('/credential') && init?.method === 'PUT') return credential as never
      throw new Error(`unexpected ${path}`)
    })
    const readsBefore = apiFetchMock.mock.calls.filter(([path]) => path === '/migrations/fub/').length
    await wrapper.get('[data-testid="fub-api-key"]').setValue(SECRET)
    await wrapper.get('form').trigger('submit')
    await flushPromises()
    wrapper.unmount()
    resolveCredential()
    await flushPromises()
    expect(apiFetchMock.mock.calls.filter(([path]) => path === '/migrations/fub/')).toHaveLength(readsBefore)
  })

  it('scopes migration cache by actor and Organization identity', async () => {
    const { queryClient } = await mountView(() => summary())
    expect(queryClient.getQueryData(queryKeys.migration(ORG_ID, 'u-alice', 0))).toEqual(summary())
    expect(queryClient.getQueryData(queryKeys.migration('other-org', 'u-alice', 0))).toBeUndefined()
    expect(queryClient.getQueryData(queryKeys.migration(ORG_ID, 'another-user', 0))).toBeUndefined()
  })
})
