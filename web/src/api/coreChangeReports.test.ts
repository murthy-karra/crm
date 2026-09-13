import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from './client'
import { useImportAccess } from './imports'
import { cancelCoreChangeReport, coreChangeActive, coreChangeLabel, createCoreChangeReport, fetchCoreChangeReport, fetchCoreChangeReports, fetchCoreChangeRow, fetchCoreChangeRows, resumeCoreChangeReport, useCoreChangeAccess } from './coreChangeReports'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
vi.mock('./imports', () => ({ useImportAccess: vi.fn() }))
const api = vi.mocked(apiFetch)
const root = '/migrations/fub/core-change-reports'
beforeEach(() => { api.mockReset() })
describe('core change report transport', () => {
  it('preserves request identity and exact bodies for creation and controls', async () => {
    const signal = new AbortController().signal
    const request = { request_id: 'same-request', parent_import_id: 'parent', newer_snapshot_id: '900719925474099312345' }
    await createCoreChangeReport(request, signal); await createCoreChangeReport(request, signal)
    await resumeCoreChangeReport('report/1', 'resume-id', signal); await cancelCoreChangeReport('report/1', 'cancel-id', signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([root, root, `${root}/report%2F1/resume`, `${root}/report%2F1/cancel`])
    expect(api.mock.calls[0]![1]?.body).toBe(api.mock.calls[1]![1]?.body)
    expect(JSON.parse(String(api.mock.calls[0]![1]?.body))).toEqual(request)
    expect(JSON.parse(String(api.mock.calls[2]![1]?.body))).toEqual({ request_id: 'resume-id' })
    expect(api.mock.calls.every(([, init]) => init?.method === 'POST' && init.signal === signal)).toBe(true)
  })
  it('bounds pages and encodes IDs, closed filters and opaque cursors', async () => {
    const signal = new AbortController().signal
    await fetchCoreChangeReports('p/1', 'next +/=', signal); await fetchCoreChangeReport('r/1', signal)
    await fetchCoreChangeRows('r/1', { family: 'notes', disposition: 'unresolved' }, 'cursor+/=', signal)
    await fetchCoreChangeRow('r/1', 'row/1', signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([`${root}?parent_import_id=p%2F1&cursor=next+%2B%2F%3D&limit=20`, `${root}/r%2F1`, `${root}/r%2F1/rows?family=notes&disposition=unresolved&cursor=cursor%2B%2F%3D&limit=50`, `${root}/r%2F1/rows/row%2F1`])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && !init.body)).toBe(true)
  })
  it('does not retry a write automatically and uses its own guarded cache namespace', async () => {
    const error = new ApiError(503, 'unavailable'); api.mockRejectedValue(error)
    await expect(resumeCoreChangeReport('r', 'same')).rejects.toBe(error)
    expect(api).toHaveBeenCalledTimes(1)
    useCoreChangeAccess(); expect(useImportAccess).toHaveBeenCalledWith('core-change-reports')
    expect(coreChangeActive({ state: 'running' })).toBe(true)
    expect(coreChangeActive({ state: 'paused' })).toBe(false)
  })
  it('never echoes unrecognized reason, representation or property-path strings', () => {
    expect(coreChangeLabel('note_content')).toBe('Note content')
    expect(coreChangeLabel('fub-core-v1/notes/detail/replies,reactions')).toBe('Enriched note detail observations')
    expect(coreChangeLabel('secret.body.customer-text')).toBe('Unrecognized report detail')
    expect(coreChangeLabel('__proto__')).toBe('Unrecognized report detail')
    expect(coreChangeLabel('constructor')).toBe('Unrecognized report detail')
  })
})
