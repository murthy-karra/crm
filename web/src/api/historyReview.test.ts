import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { fetchHistoryInquiries, fetchHistoryReviewCore, fetchHistoryTimeline, fetchHistoryTimelineDetail, historyReviewCoreKey } from './historyReview'
import { cancelHistoryImport, confirmHistoryImport, fetchHistoryImportRecords, fetchHistoryImportResults, fetchHistoryImports, increaseHistoryImportBudget, prepareHistoryImport, resumeHistoryImport } from './historyImports'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
const api = vi.mocked(apiFetch)
beforeEach(() => api.mockReset())
describe('010d2 transport contracts', () => {
  it('uses bounded kind-qualified no-store routes and keeps v2 separate from legacy caches', async () => {
    const signal = new AbortController().signal
    await fetchHistoryReviewCore('person/id', signal)
    await fetchHistoryInquiries('person', 'cursor +', signal)
    await fetchHistoryTimeline('person', { family: 'calls', dated: 'unknown', limit: 3 }, 'next+', signal)
    await fetchHistoryTimelineDetail('person', 'fub_call_record_imported', 'fact/id', signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual(['/people/person%2Fid/migration-review/v2', '/people/person/migration-review/inquiries?limit=25&cursor=cursor+%2B', '/people/person/migration-review/timeline?family=calls&dated=unknown&limit=3&cursor=next%2B', '/people/person/migration-review/timeline/fub_call_record_imported/fact%2Fid'])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && init.cache === 'no-store')).toBe(true)
    const prefix = ['org', 'org', 'history-review', 'actor', 3, '9007199254740993', 'admin']
    expect(historyReviewCoreKey(prefix, 'person')).toEqual([...prefix, 'person', 'person', 'core-v2'])
  })
  it('keeps exact decimal mutation revisions and distinct plan/result cursors', async () => {
    const expected_revision = '9007199254740993123'
    await prepareHistoryImport({ request_id: 'request', parent_import_id: 'parent', capture_id: 'capture', expected_capture_revision: expected_revision, expected_workspace_revision: '2', expected_policy_revision: 'policy' })
    await confirmHistoryImport('run/id', { request_id: 'confirm', plan_id: 'plan', expected_revision, expected_plan_revision: '3', expected_workspace_revision: '2', expected_policy_revision: 'policy', acknowledgements: { external_facts: true, date_uncertainty: true, coverage_and_holds: true, review_only: true } })
    await resumeHistoryImport('run', { request_id: 'resume', expected_revision, expected_policy_revision: 'policy' })
    await cancelHistoryImport('run', { request_id: 'cancel', expected_revision })
    await increaseHistoryImportBudget('run', { request_id: 'budget', expected_revision, expected_run_budget_revision: '4', expected_org_budget_revision: '5', expected_policy_revision: 'policy', run_byte_limit: expected_revision, org_byte_limit: expected_revision })
    await fetchHistoryImports('parent', 'cursor +')
    await fetchHistoryImportRecords('run', { family: 'events', disposition: 'held' }, 'record+')
    await fetchHistoryImportResults('run', { disposition: 'already_imported' }, 'result+')
    expect(api.mock.calls.slice(0, 5).every(([, init]) => init?.method === 'POST' && init.cache === 'no-store')).toBe(true)
    expect(JSON.parse(String(api.mock.calls[1]![1]?.body)).expected_revision).toBe(expected_revision)
    expect(api.mock.calls[1]![0]).toBe('/migrations/fub/history-imports/run%2Fid/confirm')
    expect(api.mock.calls.slice(5).map(([url]) => url)).toEqual(['/migrations/fub/history-imports?parent_import_id=parent&limit=25&cursor=cursor+%2B', '/migrations/fub/history-imports/run/records?family=events&disposition=held&limit=25&cursor=record%2B', '/migrations/fub/history-imports/run/results?disposition=already_imported&limit=25&cursor=result%2B'])
  })
})
