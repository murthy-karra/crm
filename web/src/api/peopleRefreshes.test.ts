import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from './client'
import { useImportAccess } from './imports'
import { cancelPeopleRefresh, confirmPeopleRefresh, fetchPeopleRefreshContacts, fetchPeopleRefreshItem, fetchPeopleRefreshItems, fetchPeopleRefreshResults, fetchPeopleRefreshes, preparePeopleRefresh, refreshInteger, refreshLabel, repreviewPeopleRefresh, retryPeopleRefresh, usePeopleRefreshAccess } from './peopleRefreshes'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
vi.mock('./imports', () => ({ useImportAccess: vi.fn() }))
const api = vi.mocked(apiFetch); const root = '/migrations/fub/people-refreshes'
beforeEach(() => { api.mockReset() })
describe('People refresh transport', () => {
  it('binds confirmation to the exact plan and separate clear acknowledgments without rewriting a replay', async () => {
    const signal = new AbortController().signal
    const body = { request_id: 'same', plan_id: 'plan', plan_revision: 7, plan_digest: 'digest', acknowledged_coverage: true, acknowledged_exclusions: true, acknowledged_name_clears: 1, acknowledged_assignment_clears: 2, acknowledged_contact_removals: 3 }
    await confirmPeopleRefresh('r/1', body, signal); await confirmPeopleRefresh('r/1', body, signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([`${root}/r%2F1/confirm`, `${root}/r%2F1/confirm`])
    expect(api.mock.calls[0]![1]?.body).toBe(api.mock.calls[1]![1]?.body)
    expect(JSON.parse(String(api.mock.calls[0]![1]?.body))).toEqual(body)
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && init.method === 'POST')).toBe(true)
  })
  it('keeps lifecycle request IDs and expected revisions separate from new previews', async () => {
    await preparePeopleRefresh('report', 'prepare'); await repreviewPeopleRefresh('run', 'repreview', 4)
    await retryPeopleRefresh('run', 'retry', 8); await cancelPeopleRefresh('run', 'cancel', 9)
    expect(api.mock.calls.map(([, init]) => JSON.parse(String(init?.body)))).toEqual([
      { report_id: 'report', request_id: 'prepare' }, { request_id: 'repreview', expected_plan_revision: 4 },
      { request_id: 'retry', expected_lifecycle_revision: 8 }, { request_id: 'cancel', expected_lifecycle_revision: 9 },
    ])
  })
  it('uses bounded separate scalar/contact/result endpoints and opaque encoded cursors', async () => {
    const signal = new AbortController().signal
    await fetchPeopleRefreshes('parent/1', 'next +/=', signal)
    await fetchPeopleRefreshItems('r/1', 'held_local_change', 'next +/=', signal)
    await fetchPeopleRefreshItem('r/1', 'i/1', signal)
    await fetchPeopleRefreshContacts('r/1', 'i/1', 'next +/=', signal)
    await fetchPeopleRefreshResults('r/1', 'next +/=', signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([
      `${root}?parent_import_id=parent%2F1&cursor=next+%2B%2F%3D&limit=20`,
      `${root}/r%2F1/items?disposition=held_local_change&cursor=next+%2B%2F%3D&limit=50`, `${root}/r%2F1/items/i%2F1`,
      `${root}/r%2F1/items/i%2F1/contacts?cursor=next+%2B%2F%3D&limit=50`, `${root}/r%2F1/results?cursor=next+%2B%2F%3D&limit=50`,
    ])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && !init.body)).toBe(true)
  })
  it('never retries a write implicitly or rounds an unsafe revision', async () => {
    api.mockRejectedValue(new ApiError(503, 'unavailable'))
    await expect(retryPeopleRefresh('run', 'same', 1)).rejects.toBeInstanceOf(ApiError); expect(api).toHaveBeenCalledTimes(1)
    expect(refreshInteger('25')).toBe(25)
    for (const value of ['9007199254740993', '-1', '1.2', 'NaN']) expect(() => refreshInteger(value)).toThrow()
    usePeopleRefreshAccess(); expect(useImportAccess).toHaveBeenCalledWith('people-refreshes')
    expect(refreshLabel('secret.customer.content')).toBe('Unrecognized refresh detail'); expect(refreshLabel('__proto__')).toBe('Unrecognized refresh detail')
  })
})
