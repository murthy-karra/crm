import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch } from './client'
import { cancelImport, confirmImport, fetchImport, fetchImportField, fetchImportMappings, fetchImportRecords, fetchImportResults, fetchImports, fetchPersonProvenance, importQueryKeys, proposeImport, replanImport, retryImport } from './imports'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
const fetch = vi.mocked(apiFetch)
beforeEach(() => { fetch.mockReset() })
describe('People import transport', () => {
  it('keeps exact request inputs and source identifiers without client authority', async () => {
    const body = { request_id: 'request', plan_id: 'plan', plan_revision: '9007199254740993', confirmation_digest: 'digest', acknowledgments: { held_count: '9007199254740995', review_only: true as const, remaining_data: true as const } }
    await confirmImport('import', body); await confirmImport('import', body)
    expect(fetch.mock.calls.map(([, options]) => JSON.parse(String(options?.body)))).toEqual([body, body])
    expect(fetch.mock.calls[0]![0]).toBe('/migrations/fub/imports/import/confirm')
  })
  it('sends only explicit partial mapping patches', async () => {
    const body = { request_id: 'r', expected_plan_revision: '2', stage_mappings: [{ source_key: '900719925474099312345', choice: { kind: 'create' as const } }], assignee_mappings: [{ source_key: 'user:8', choice: { kind: 'unassigned' as const } }] }
    await replanImport('i', body)
    expect(JSON.parse(String(fetch.mock.calls[0]![1]?.body))).toEqual(body)
  })
  it('uses bounded distinct page routes and forwards cancellation', async () => {
    const signal = new AbortController().signal
    await fetchImports('cursor +', signal); await fetchImport('i/unsafe', signal)
    await fetchImportMappings('i', 'p', 'assignee', 'c', signal)
    await fetchImportRecords('i', 'p', 'held', 'c', signal)
    await fetchImportResults('i', 'pending', 'c', signal)
    expect(fetch.mock.calls.map(([url]) => url)).toEqual(['/migrations/fub/imports?cursor=cursor+%2B&limit=20', '/migrations/fub/imports/i%2Funsafe', '/migrations/fub/imports/i/plans/p/mappings?kind=assignee&cursor=c&limit=50', '/migrations/fub/imports/i/plans/p/records?disposition=held&cursor=c&limit=50', '/migrations/fub/imports/i/results?disposition=pending&cursor=c&limit=50'])
    expect(fetch.mock.calls.every(([, init]) => init?.signal === signal)).toBe(true)
  })
  it('keeps mapping, record and Person field routes separate and segmented', async () => {
    const signal = new AbortController().signal
    await fetchImportField({ kind: 'mapping', importId: 'i', planId: 'p', mappingId: 'm', fieldKey: 'source.a/b' }, 'next +', signal)
    await fetchImportField({ kind: 'record', importId: 'i', planId: 'p', recordId: 'r', fieldKey: 'contacts' }, undefined, signal)
    await fetchImportField({ kind: 'provenance', personId: 'person', fieldKey: 'provenance' }, undefined, signal)
    await fetchPersonProvenance('person', signal)
    expect(fetch.mock.calls.map(([url]) => url)).toEqual(['/migrations/fub/imports/i/plans/p/mappings/m/fields/source.a%2Fb?cursor=next+%2B&limit=65536', '/migrations/fub/imports/i/plans/p/records/r/fields/contacts?limit=65536', '/people/person/import-provenance/fields/provenance?limit=65536', '/people/person/import-provenance'])
    expect(fetch.mock.calls.every(([, init]) => init?.signal === signal)).toBe(true)
  })
  it('separates planning, retry and cancellation with supplied request IDs', async () => {
    await proposeImport({ request_id: 'r1', snapshot_id: 's', preview_id: 'p' }); await retryImport('i', 'r2'); await cancelImport('i', 'r3')
    expect(fetch.mock.calls.map(([url, init]) => [url, JSON.parse(String(init?.body))])).toEqual([['/migrations/fub/imports', { request_id: 'r1', snapshot_id: 's', preview_id: 'p' }], ['/migrations/fub/imports/i/retry', { request_id: 'r2' }], ['/migrations/fub/imports/i/cancel', { request_id: 'r3' }]])
  })
  it('keys actor, session lifetime and workspace revision independently', () => {
    expect(importQueryKeys('org', 'actor', 4, '9007199254740993')).toEqual(['org', 'org', 'people-imports', 'actor', 4, '9007199254740993'])
    expect(importQueryKeys('org', 'other', 4, '9007199254740993')).not.toEqual(importQueryKeys('org', 'actor', 4, '9007199254740993'))
  })
})
