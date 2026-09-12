import { beforeEach, describe, expect, it, vi } from 'vitest'
import { apiFetch, ApiError } from './client'
import { useImportAccess } from './imports'
import { activityActive, cancelActivityImport, confirmActivityImport, fetchActivityField, fetchActivityImport, fetchActivityImports, fetchActivityMappings, fetchActivityObservations, fetchActivityRecords, fetchActivityResults, fetchActivityTargets, proposeActivityImport, replanActivityImport, retryActivityImport, useActivityAccess, type ActivityImport, type ActivityReplan } from './activityImports'
vi.mock('./client', async original => ({ ...await original<typeof import('./client')>(), apiFetch: vi.fn() }))
vi.mock('./imports', () => ({ useImportAccess: vi.fn() }))
const api = vi.mocked(apiFetch)
const root = '/migrations/fub/activity-imports'
const revision = '90071992547409931234567890'
beforeEach(() => { api.mockReset() })
describe('activity import transport', () => {
  it('preserves decimal acknowledgments, explicit mapping variants and omitted versus cleared timezone', async () => {
    const signal = new AbortController().signal
    const body: ActivityReplan = { request_id: 'same-request', expected_plan_id: 'plan', choices: [{ mapping_id: 'a', choice: { kind: 'hold' } }, { mapping_id: 'b', choice: { kind: 'leave_unmapped' } }, { mapping_id: 'c', choice: { kind: 'map_existing', target_id: 'user' } }, { mapping_id: 'd', choice: { kind: 'map_kind', native_kind: 'other' } }] }
    const confirm = { request_id: 'confirm', plan_id: 'plan', expected_revision: revision, acknowledge_held: revision, acknowledge_source_only: '7' }
    await proposeActivityImport({ request_id: 'proposal', parent_import_id: 'parent' }, signal)
    await replanActivityImport('child/1', body, signal)
    await replanActivityImport('child/1', { ...body, source_timezone: null }, signal)
    await replanActivityImport('child/1', { ...body, source_timezone: 'America/Los_Angeles' }, signal)
    await confirmActivityImport('child/1', confirm, signal)
    await confirmActivityImport('child/1', confirm, signal)
    await retryActivityImport('child/1', { request_id: 'retry', expected_revision: revision }, signal)
    await cancelActivityImport('child/1', { request_id: 'cancel', expected_revision: revision }, signal)
    const bodies = api.mock.calls.map(([, init]) => JSON.parse(String(init?.body)))
    expect(bodies[1]).toEqual(body); expect(bodies[1]).not.toHaveProperty('source_timezone')
    expect(bodies[2].source_timezone).toBeNull(); expect(bodies[3].source_timezone).toBe('America/Los_Angeles')
    expect(bodies[4]).toEqual(confirm); expect(api.mock.calls[4]![1]?.body).toBe(api.mock.calls[5]![1]?.body)
    expect(bodies[6]).toEqual({ request_id: 'retry', expected_revision: revision })
    expect(api.mock.calls.slice(1).every(([url]) => url.startsWith(`${root}/child%2F1/`))).toBe(true)
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && init.method === 'POST')).toBe(true)
  })
  it('constructs all scoped bounded routes without following source URLs', async () => {
    const signal = new AbortController().signal
    await fetchActivityImports('parent/1', 'c +/=', signal); await fetchActivityImport('i/1', signal)
    await fetchActivityMappings('i/1', 'p/1', 'task_assignee', 'c +/=', signal)
    await fetchActivityTargets('i/1', 'p/1', undefined, signal)
    await fetchActivityRecords('i/1', 'p/1', { kind: 'note', disposition: 'held', issue: 'missing_body' }, undefined, signal)
    await fetchActivityResults('i/1', 'p/1', { kind: 'task', disposition: 'held', issue: 'target_missing' }, undefined, signal)
    await fetchActivityObservations('i/1', 'p/1', 'r/1', undefined, signal)
    expect(api.mock.calls.map(([url]) => url)).toEqual([
      `${root}?parent_import_id=parent%2F1&cursor=c+%2B%2F%3D&limit=20`, `${root}/i%2F1`, `${root}/i%2F1/mappings?plan_id=p%2F1&kind=task_assignee&cursor=c+%2B%2F%3D&limit=50`, `${root}/i%2F1/targets?plan_id=p%2F1&limit=50`, `${root}/i%2F1/records?plan_id=p%2F1&kind=note&disposition=held&issue=missing_body&limit=25`, `${root}/i%2F1/results?plan_id=p%2F1&kind=task&disposition=held&issue=target_missing&limit=25`, `${root}/i%2F1/records/r%2F1/observations?plan_id=p%2F1&limit=25`,
    ])
    expect(api.mock.calls.every(([, init]) => init?.signal === signal && !init.body)).toBe(true)
  })
  it('keeps source owner, plan and opaque UTF-8 segment cursor separate', async () => {
    const response = { text: '<img src="https://source.invalid/x">José 🏡', offset: revision, next_offset: revision, total_utf8_bytes: revision, next_cursor: 'opaque+/' }
    api.mockResolvedValue(response)
    for (const kind of ['records', 'mappings', 'results', 'observations'] as const) {
      expect(await fetchActivityField({ kind, importId: 'i/1', rowId: 'r/1', planId: 'p/1', fieldKey: 'source.a/b' }, 'c +/=')).toBe(response)
    }
    expect(api.mock.calls.map(([url]) => url)).toEqual(['records', 'mappings', 'results', 'observations'].map(kind => `${root}/i%2F1/${kind}/r%2F1/fields/source.a%2Fb?plan_id=p%2F1&cursor=c+%2B%2F%3D&limit=65536`))
    await fetchActivityField({ kind: 'results', importId: 'i', rowId: 'r', fieldKey: 'all' })
    expect(api.mock.lastCall![0]).toBe(`${root}/i/results/r/fields/all?limit=65536`)
  })
  it('propagates uncertain errors without silently retrying', async () => {
    const error = new ApiError(503, 'unavailable'); api.mockRejectedValue(error)
    await expect(retryActivityImport('i', { request_id: 'exact', expected_revision: revision })).rejects.toBe(error)
    expect(api).toHaveBeenCalledTimes(1)
  })
  it('uses its own cache namespace and polls only active work', () => {
    useActivityAccess(); expect(useImportAccess).toHaveBeenCalledWith('activity-imports')
    for (const state of ['preparing', 'queued', 'running', 'ready', 'paused', 'completed', 'cancelled'] as const) expect(activityActive({ state } as ActivityImport)).toBe(['preparing', 'queued', 'running'].includes(state))
  })
})
