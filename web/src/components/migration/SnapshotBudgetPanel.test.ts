import { flushPromises, mount } from '@vue/test-utils'
import { afterEach, beforeEach, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from '../../api/client'
import type { CoreSnapshot } from '../../api/snapshots'
import SnapshotBudgetPanel from './SnapshotBudgetPanel.vue'
import { bytesToGiB, gibToBytes } from './format'
vi.mock('../../api/client', async (original) => ({ ...await original<typeof import('../../api/client')>(), apiFetch: vi.fn() }))
const fetchMock = vi.mocked(apiFetch)
const s: CoreSnapshot = {
  id: 'run-1', connection_id: 'connection', connection_revision: 1, source_account_id: '42', profile_version: 'fub-core-v1', state: 'paused', pause_reason: 'storage_budget_exhausted',
  created_at: '2026-09-11T12:00:00Z', started_at: null, completed_at: null, proposal_expires_at: '2026-09-11T12:10:00Z',
  raw_bytes: '100', retained_bytes: '1000', reserved_bytes: '0', accepted_captures: '1', capture_sequence: '1', original_run_byte_limit: '67108864', run_byte_limit: '67108864', org_byte_limit: '268435456', org_retained_bytes: '1000', org_reserved_bytes: '0',
  run_budget_revision: '2', org_budget_revision: '3', policy_revision: 'policy-a', run_budget_policy_revision: 'old', org_budget_policy_revision: 'old', run_ceiling_bytes: '134217728', org_ceiling_bytes: '536870912', required_reservation_bytes: '67108864', actions: ['increase_budget'], preview_ids: [],
}
function mountPanel() {
  return mount(SnapshotBudgetPanel, { props: { snapshot: { ...s } }, global: { stubs: {
    ConfirmDialog: { props: ['visible', 'message', 'isPending'], emits: ['confirm', 'update:visible'], template: '<div v-if="visible" data-testid="budget-dialog"><p>{{ message }}</p><button data-testid="confirm-budget" :disabled="isPending" @click="$emit(\'confirm\')">Confirm increase</button></div>' },
  } } })
}
beforeEach(() => { fetchMock.mockReset(); vi.stubGlobal('crypto', { randomUUID: () => 'request-1' }) })
afterEach(() => vi.unstubAllGlobals())
it('converts GiB and large byte values exactly, rejecting fractional bytes and overflow', () => {
  for (const bytes of ['1', '67108864', '9007199254740993', '9223372036854775807']) expect(gibToBytes(bytesToGiB(bytes))).toBe(bytes)
  expect(gibToBytes('0.0625')).toBe('67108864')
  for (const invalid of ['0.1', '0', '-2', '8589934592', '2e3']) expect(gibToBytes(invalid)).toBeNull()
})
it('requires confirmation, freezes reviewed revisions, and starts no source or preview job', async () => {
  const wrapper = mountPanel()
  await wrapper.get('[data-testid="snapshot-budget-open"]').trigger('click')
  await wrapper.get('[data-testid="snapshot-run-budget"]').setValue('0.125')
  await wrapper.get('form').trigger('submit')
  expect(fetchMock).not.toHaveBeenCalled()
  expect(wrapper.get('[data-testid="budget-dialog"]').text()).toContain('67108864 → 134217728 bytes')
  expect(wrapper.get('[data-testid="budget-dialog"]').text()).toContain('does not resume capture or preview')
  await wrapper.setProps({ snapshot: { ...s, run_budget_revision: '99', policy_revision: 'new-policy' } })
  fetchMock.mockResolvedValue({ snapshot: s, budget: { run_byte_limit: '134217728', org_byte_limit: '268435456' } })
  await wrapper.get('[data-testid="confirm-budget"]').trigger('click'); await flushPromises()
  expect(fetchMock).toHaveBeenCalledTimes(1)
  expect(fetchMock.mock.calls[0][0]).toBe('/migrations/fub/snapshots/run-1/budget')
  expect(JSON.parse(String(fetchMock.mock.calls[0][1]?.body))).toEqual({ request_id: 'request-1', expected_run_budget_revision: '2', expected_org_budget_revision: '3', expected_policy_revision: 'policy-a', run_byte_limit: '134217728', org_byte_limit: '268435456' })
  expect(wrapper.emitted('refresh')).toHaveLength(1)
  expect(wrapper.text()).toContain('This change started no work')
  wrapper.unmount()
})
it('rejects over-ceiling amounts and requires a fresh review after conflict', async () => {
  const wrapper = mountPanel()
  await wrapper.get('[data-testid="snapshot-budget-open"]').trigger('click')
  await wrapper.get('[data-testid="snapshot-run-budget"]').setValue('1')
  await wrapper.get('form').trigger('submit')
  expect(wrapper.text()).toContain('An operator must raise')
  expect(fetchMock).not.toHaveBeenCalled()
  await wrapper.get('[data-testid="snapshot-run-budget"]').setValue('0.125')
  await wrapper.get('form').trigger('submit')
  fetchMock.mockRejectedValue(new ApiError(409, 'snapshot_conflict'))
  await wrapper.get('[data-testid="confirm-budget"]').trigger('click'); await flushPromises()
  expect(wrapper.find('[data-testid="budget-dialog"]').exists()).toBe(false)
  expect(wrapper.text()).toContain('review a new increase')
  expect(wrapper.emitted('refresh')).toHaveLength(1)
  expect(fetchMock).toHaveBeenCalledTimes(1)
  wrapper.unmount()
})
it('retries uncertainty with the same request and ignores completion after teardown', async () => {
  const wrapper = mountPanel()
  await wrapper.get('[data-testid="snapshot-budget-open"]').trigger('click')
  await wrapper.get('[data-testid="snapshot-run-budget"]').setValue('0.125')
  await wrapper.get('form').trigger('submit')
  fetchMock.mockRejectedValueOnce(new ApiError(0, 'network_error'))
  await wrapper.get('[data-testid="confirm-budget"]').trigger('click'); await flushPromises()
  let settle!: (value: unknown) => void
  fetchMock.mockImplementationOnce(() => new Promise(resolve => { settle = resolve }))
  await wrapper.get('[data-testid="confirm-budget"]').trigger('click')
  expect(fetchMock.mock.calls[1][1]?.body).toBe(fetchMock.mock.calls[0][1]?.body)
  wrapper.unmount()
  settle({ snapshot: s, budget: { run_byte_limit: '134217728', org_byte_limit: '268435456' } }); await flushPromises()
  expect(wrapper.emitted('refresh')).toBeUndefined()
})
