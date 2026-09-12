import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from './client'
import { useImportAccess } from './imports'
import { cancelHistoryCapture, confirmHistoryCapture, fetchHistoryCapture, fetchHistoryCaptures, fetchHistoryRecord, fetchHistoryRecords, historyActive, increaseHistoryBudget, proposeHistoryCapture, retryHistoryCapture, useHistoryAccess, type HistoryCapture, type HistoryConfirmation } from './historyCaptures'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
vi.mock('./imports', () => ({ useImportAccess: vi.fn() }))
const api = vi.mocked(apiFetch)
const root = '/migrations/fub/history-captures'
const decimal = '900719925474099312345'
beforeEach(() => { api.mockReset() })
describe('historical capture contract', () => {
  it('preserves exact decimal strings and frozen bodies for every receipt action', async () => {
    const signal = new AbortController().signal
    const action = { request_id: 'request', expected_run_revision: decimal }
    const confirm: HistoryConfirmation = { ...action, acknowledgements: { api_visible_account_scope: true, coverage_gaps: true, retained_not_imported: true, source_user_evidence_revision: decimal, source_user_difference: true } }
    const budget = { ...action, expected_run_budget_revision: decimal, expected_org_budget_revision: decimal, expected_policy_revision: 'opaque-policy', run_byte_limit: decimal, org_byte_limit: decimal }
    await proposeHistoryCapture({ request_id: 'proposal', parent_import_id: 'parent', connection_id: 'connection', expected_revision: decimal }, signal)
    await confirmHistoryCapture('run/1', confirm, signal); await confirmHistoryCapture('run/1', confirm, signal)
    await retryHistoryCapture('run/1', action, signal); await cancelHistoryCapture('run/1', action, signal); await increaseHistoryBudget('run/1', budget, signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([root, `${root}/run%2F1/confirm`, `${root}/run%2F1/confirm`, `${root}/run%2F1/retry`, `${root}/run%2F1/cancel`, `${root}/run%2F1/budget`])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && init.method === 'POST')).toBe(true)
    expect(api.mock.calls[1]![1]?.body).toBe(api.mock.calls[2]![1]?.body)
    expect(JSON.parse(String(api.mock.calls[1]![1]?.body))).toEqual(confirm)
    expect(JSON.parse(String(api.mock.calls[5]![1]?.body))).toEqual(budget)
  })
  it('uses bounded local routes and opaque cursors without source requests', async () => {
    const signal = new AbortController().signal
    await fetchHistoryCaptures('p/1', 'opaque +/=', signal); await fetchHistoryCapture('r/1', signal)
    await fetchHistoryRecords('r/1', { family: 'text_messages', disposition: 'conflicting_reference', record_id: 'observation/1' }, 'next+/=', signal)
    await fetchHistoryRecord('r/1', 'observation/1', signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([`${root}?parent_import_id=p%2F1&cursor=opaque+%2B%2F%3D&limit=20`, `${root}/r%2F1`, `${root}/r%2F1/records?family=text_messages&disposition=conflicting_reference&record_id=observation%2F1&cursor=next%2B%2F%3D&limit=50`, `${root}/r%2F1/records/observation%2F1`])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && !init.body)).toBe(true)
  })
  it('never silently retries a mutation and scopes its cache separately', async () => {
    const error = new ApiError(503, 'unavailable'); api.mockRejectedValue(error)
    await expect(retryHistoryCapture('r', { request_id: 'same', expected_run_revision: decimal })).rejects.toBe(error)
    expect(api).toHaveBeenCalledTimes(1)
    useHistoryAccess(); expect(useImportAccess).toHaveBeenCalledWith('history-captures')
    for (const state of ['proposed', 'queued', 'running', 'waiting_retry', 'paused', 'completed_with_gaps', 'cancelled'] as const) expect(historyActive({ state } as HistoryCapture)).toBe(['queued', 'running', 'waiting_retry'].includes(state))
  })
})
