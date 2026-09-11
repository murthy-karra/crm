import { flushPromises, mount, type VueWrapper } from '@vue/test-utils'
import { QueryClient, VueQueryPlugin, focusManager } from '@tanstack/vue-query'
import PrimeVue from 'primevue/config'
import Select from 'primevue/select'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError } from '../../api/client'
import {
  fetchSnapshotPreview, fetchSnapshotRecords, fetchSnapshotGroups, fetchSnapshotGroupMembers,
  retrySnapshotPreview, retrySnapshot, type SnapshotPreviewDetail, type SnapshotPreviewRecord,
  type SnapshotRecordPage, type SnapshotGroupPage, type SnapshotStream,
} from '../../api/snapshots'
import SnapshotPreviewPanel from './SnapshotPreviewPanel.vue'

vi.mock('../../api/snapshots', async importOriginal => {
  const actual = await importOriginal<typeof import('../../api/snapshots')>()
  return {
    ...actual,
    fetchSnapshotPreview: vi.fn(), fetchSnapshotRecords: vi.fn(), fetchSnapshotGroups: vi.fn(),
    fetchSnapshotGroupMembers: vi.fn(), retrySnapshotPreview: vi.fn(), retrySnapshot: vi.fn(),
  }
})

const previewMock = vi.mocked(fetchSnapshotPreview)
const recordsMock = vi.mocked(fetchSnapshotRecords)
const groupsMock = vi.mocked(fetchSnapshotGroups)
const membersMock = vi.mocked(fetchSnapshotGroupMembers)
const retryMock = vi.mocked(retrySnapshotPreview)
const cleanup: Array<() => void> = []
const baseProps = { snapshotId: 'snapshot-a', previewId: 'preview-a', orgId: 'org-a', actorId: 'actor-a', sessionLifetime: 1 }

function stream(family: string, total: string | null = '0'): SnapshotStream {
  return { stream: family, family, state: 'completed', reported_total: total, returned_items: '0', distinct_ids: '0', accepted_captures: '1', content_gaps: '0', attempts: '1', error_code: null, observed_at: '2026-09-11T12:00:00Z', first_observed_at: '2026-09-11T12:00:00Z' }
}
function report(state: SnapshotPreviewDetail['preview']['state'] = 'completed', previewId = 'preview-a'): SnapshotPreviewDetail {
  return {
    preview: { id: previewId, snapshot_id: 'snapshot-a', state, pause_reason: state === 'paused' ? 'storage_budget_exhausted' : null, engine_version: '1', capture_sequence: '9007199254740993', created_at: '2026-09-11T12:00:00Z', completed_at: state === 'completed' ? '2026-09-11T12:01:00Z' : null, input_observed_at: '2026-09-11T12:00:00Z', actions: state === 'paused' ? ['retry'] : [] },
    coverage: [
      { family: 'people', state: 'complete_for_selected_query', reason: 'Credential-visible People captured.', next_action: 'Review source coverage.' },
      { family: 'emails', state: 'not_captured', reason: 'Email history is not captured.', next_action: 'Capture in a later snapshot step.' },
    ],
    destination_stale: false, first_import_requires_new_empty_organization: true, destination_has_people: false,
    counts: { records: [], issues: [], invalid_ids: [], streams: [stream('people', '0'), stream('notes', null)] },
  }
}
function record(id = 'record-a', sourceId = '1', family = 'people'): SnapshotPreviewRecord {
  return { id, source_id: sourceId, family, disposition: 'reviewable', issues: [], projection: { id: sourceId, name: `Person ${sourceId}`, stage: 'Lead' }, overlap_group_count: '0', candidates: { member_id: null, stage_id: null, custom_field_id: null } }
}
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}
async function mountPanel(props: Partial<typeof baseProps> = {}) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } })
  const wrapper = mount(SnapshotPreviewPanel, { props: { ...baseProps, ...props }, global: { plugins: [[VueQueryPlugin, { queryClient }], [PrimeVue, { unstyled: true }]] }, attachTo: document.body })
  cleanup.push(() => { wrapper.unmount(); queryClient.clear() })
  await flushPromises()
  return { wrapper, queryClient }
}
function button(wrapper: VueWrapper, label: string) {
  const result = wrapper.findAll('button').find(b => b.text() === label)
  expect(result, `button ${label}`).toBeDefined()
  return result!
}

beforeEach(() => {
  vi.clearAllMocks()
  previewMock.mockReset().mockResolvedValue(report())
  recordsMock.mockReset().mockResolvedValue({ records: [record()], next_cursor: null })
  groupsMock.mockReset().mockResolvedValue({ groups: [], next_cursor: null })
  membersMock.mockReset().mockResolvedValue({ members: [], next_cursor: null })
  retryMock.mockReset().mockResolvedValue({ preview_id: 'preview-a', state: 'queued' })
  vi.stubGlobal('crypto', { randomUUID: vi.fn().mockReturnValue('request-1') })
  focusManager.setFocused(true)
})
afterEach(() => {
  cleanup.splice(0).forEach(fn => fn())
  document.body.innerHTML = ''
  vi.unstubAllGlobals()
  vi.useRealTimers()
  focusManager.setFocused(undefined)
})

describe('SnapshotPreviewPanel', () => {
  it('shows exact zero and unknown evidence separately, nonexclusive issues, and remaining coverage', async () => {
    const value = report()
    value.destination_stale = true
    value.destination_has_people = true
    value.counts.issues = [{ family: 'people', issue: 'normalized_contact_overlap', count: '2' }, { family: 'people', issue: 'unsupported_text_value', count: '1' }]
    value.counts.invalid_ids = [{ family: 'notes', count: '3' }]
    previewMock.mockResolvedValue(value)
    recordsMock.mockResolvedValue({ records: [{ ...record(), projection: { firstName: 'Maya', lastName: 'Chen' } }], next_cursor: null })
    const { wrapper } = await mountPanel()
    expect(wrapper.text()).toContain('9007199254740993')
    expect(wrapper.text()).toContain('Destination configuration changed')
    expect(wrapper.text()).toContain('The first import requires a new, empty Organization')
    expect(wrapper.text()).toContain('One record may have several issues')
    expect(wrapper.text()).toContain('Email history is not captured')
    const tables = wrapper.findAll('table')
    expect(tables[1]!.findAll('tbody tr')[0]!.findAll('td')[0]!.text()).toBe('0')
    expect(tables[1]!.findAll('tbody tr')[1]!.findAll('td')[0]!.text()).toBe('Unknown')
    expect(wrapper.find('[data-testid="resume-preview"]').exists()).toBe(false)
    expect(wrapper.get('[data-testid="preview-disposition"]').text()).toContain('All dispositions')
    expect(wrapper.get('[data-testid="preview-records"] summary').text()).toContain('Maya Chen')
  })

  it('renders HTML, URLs, and exact-number wrappers as escaped source text', async () => {
    const value = record('unsafe-record', '9007199254740993', 'notes')
    value.projection = { name: '<img src="https://untrusted.invalid/pixel">', body: '<script>window.leaked = true</script><b>Source note</b>', link: 'https://untrusted.invalid', amount: { _lossless_number: '9007199254740993e0' }, _snapshot: { flags: [] } }
    recordsMock.mockResolvedValue({ records: [value], next_cursor: null })
    const { wrapper } = await mountPanel()
    expect(wrapper.text()).toContain('<script>window.leaked = true</script>')
    expect(wrapper.text()).toContain('9007199254740993e0')
    expect(wrapper.find('script').exists()).toBe(false)
    expect(wrapper.find('img').exists()).toBe(false)
    expect(wrapper.find('a[href]').exists()).toBe(false)
    expect(wrapper.findAll('pre').some(p => p.text().includes('_snapshot'))).toBe(false)
  })

  it('discloses omitted source fields even when the record has no truncation issue', async () => {
    const value = record()
    value.projection = { name: 'Selected fields', _snapshot: { omitted_fields: 3, flags: [], raw_capture_preserved_separately: true } }
    recordsMock.mockResolvedValue({ records: [value], next_cursor: null })
    const { wrapper } = await mountPanel()
    const notice = wrapper.get('[data-testid="projection-review-notice"]').text()
    expect(notice).toContain('3 source fields are omitted from this preview.')
    expect(notice).toContain('Complete raw source bytes are preserved separately.')
    expect(notice).not.toContain('clipped')
    expect(wrapper.findAll('pre').some(p => p.text().includes('_snapshot'))).toBe(false)
  })

  it('labels clipped values and limited variants, including each variant’s omitted fields', async () => {
    const clipped = { body: '<b>Clipped source text', _snapshot: { omitted_fields: 0, flags: ['projection_truncated'], raw_capture_preserved_separately: true } }
    const omitted = { body: 'Selected variant fields', _snapshot: { omitted_fields: 1, flags: [], raw_capture_preserved_separately: true } }
    recordsMock.mockResolvedValue({ records: [
      { ...record('clipped', '1', 'notes'), projection: clipped },
      { ...record('variants', '2', 'notes'), projection: { variants: [omitted, clipped], variant_summary: [{ representation: 'notes', count: '3' }], review_requires_decision: true, display_limited: true } },
    ], next_cursor: null })
    const { wrapper } = await mountPanel()
    const notices = wrapper.findAll('[data-testid="projection-review-notice"]')
    expect(notices[0]!.text()).toContain('Some field values are clipped or truncated for display.')
    expect(notices[1]!.text()).toContain('Only a limited set of source variants is shown.')
    expect(notices[1]!.text()).toContain('Displayed variant 1: 1 source field is omitted from this preview.')
    expect(notices[1]!.text()).toContain('Displayed variant 2: Some field values are clipped or truncated for display.')
    expect(notices.every(notice => notice.text().includes('Complete raw source bytes are preserved separately.'))).toBe(true)
    expect(wrapper.text()).toContain('<b>Clipped source text')
    expect(wrapper.find('b').exists()).toBe(false)
    expect(wrapper.findAll('pre').some(p => p.text().includes('_snapshot'))).toBe(false)
  })

  it('clears the old page while filters change, aborts its request, and rejects a late response', async () => {
    const pending = deferred<SnapshotRecordPage>()
    recordsMock.mockImplementation(async (_run, _preview, family, _disposition, cursor) => {
      if (cursor) return pending.promise
      return { records: [record(`${family}-record`, family === 'people' ? '11' : '21', family)], next_cursor: family === 'people' ? 'record-next' : null }
    })
    const { wrapper } = await mountPanel()
    await button(wrapper, 'Next records').trigger('click')
    await flushPromises()
    expect(wrapper.get('[data-testid="preview-records"]').text()).not.toContain('Person 11')
    const oldSignal = recordsMock.mock.calls.at(-1)![5]!
    wrapper.findAllComponents(Select)[0]!.vm.$emit('update:modelValue', 'notes')
    await flushPromises()
    expect(recordsMock.mock.calls.at(-1)![2]).toBe('notes')
    expect(recordsMock.mock.calls.at(-1)![4]).toBeUndefined()
    expect(oldSignal.aborted).toBe(true)
    pending.resolve({ records: [{ ...record('old-record'), projection: { name: 'STALE PRIVATE PAGE' } }], next_cursor: 'stale-next' })
    await flushPromises()
    expect(wrapper.text()).toContain('Person 21')
    expect(wrapper.text()).not.toContain('STALE PRIVATE PAGE')
    expect(wrapper.text()).not.toContain('Person 11')
  })

  it('paginates records, each record’s groups, and members independently without fetching all candidates', async () => {
    const first = { ...record(), overlap_group_count: '2' }
    recordsMock.mockImplementation(async (_run, _preview, _family, _disposition, cursor) => ({ records: [cursor ? record('record-b', '2') : first], next_cursor: cursor ? null : 'records-2' }))
    groupsMock.mockImplementation(async (_run, _preview, _record, cursor) => ({ groups: [{ id: cursor ? 'group-2' : 'group-1', kind: cursor ? 'email' : 'phone', member_count: '100' }], next_cursor: cursor ? null : 'groups-2' }))
    membersMock.mockImplementation(async (_run, _preview, _group, cursor) => ({ members: [{ source_id: cursor ? '51' : '1', record_id: cursor ? 'member-51' : 'member-1' }], next_cursor: cursor ? null : 'members-2' }))
    const { wrapper } = await mountPanel()
    expect(groupsMock).not.toHaveBeenCalled()
    await wrapper.get('[data-record-groups="record-a"]').trigger('click')
    await flushPromises()
    expect(groupsMock.mock.calls[0]![2]).toBe('record-a')
    expect(membersMock).not.toHaveBeenCalled()
    await wrapper.get('[data-group-members="group-1"]').trigger('click')
    await flushPromises()
    await button(wrapper, 'Next members').trigger('click')
    await flushPromises()
    expect(membersMock.mock.calls.at(-1)![3]).toBe('members-2')
    expect(wrapper.text()).toContain('Source Person 51')
    await button(wrapper, 'Next groups').trigger('click')
    await flushPromises()
    expect(groupsMock.mock.calls.at(-1)![3]).toBe('groups-2')
    expect(wrapper.text()).not.toContain('Source Person 51')
    await wrapper.get('[data-group-members="group-2"]').trigger('click')
    await flushPromises()
    expect(membersMock.mock.calls.at(-1)![2]).toBe('group-2')
    expect(membersMock.mock.calls.at(-1)![3]).toBeUndefined()
    await button(wrapper, 'Next records').trigger('click')
    await flushPromises()
    expect(recordsMock.mock.calls.at(-1)![4]).toBe('records-2')
    expect(wrapper.find('[data-group-members]').exists()).toBe(false)
    await button(wrapper, 'Previous records').trigger('click')
    await flushPromises()
    expect(recordsMock.mock.calls.at(-1)![4]).toBeUndefined()
    expect(wrapper.find('[data-group-members]').exists()).toBe(false)
  })

  it('removes cached plaintext and aborts old work across Organization, actor, and session changes', async () => {
    const lateGroups = deferred<SnapshotGroupPage>()
    groupsMock.mockReturnValue(lateGroups.promise)
    recordsMock.mockResolvedValue({ records: [{ ...record(), projection: { name: 'OLD TENANT CONTENT' }, overlap_group_count: '1' }], next_cursor: null })
    const { wrapper, queryClient } = await mountPanel()
    await wrapper.get('[data-record-groups="record-a"]').trigger('click')
    await flushPromises()
    const signal = groupsMock.mock.calls[0]![4]!
    recordsMock.mockResolvedValue({ records: [{ ...record('new-record'), projection: { name: 'NEW TENANT CONTENT' } }], next_cursor: null })
    previewMock.mockResolvedValue(report('completed', 'preview-b'))
    await wrapper.setProps({ orgId: 'org-b', actorId: 'actor-b', sessionLifetime: 2, previewId: 'preview-b' })
    expect(wrapper.text()).not.toContain('OLD TENANT CONTENT')
    await flushPromises()
    expect(signal.aborted).toBe(true)
    lateGroups.resolve({ groups: [{ id: 'stale-group', kind: 'STALE TENANT GROUP', member_count: '2' }], next_cursor: null })
    await flushPromises()
    expect(wrapper.text()).toContain('NEW TENANT CONTENT')
    expect(wrapper.text()).not.toContain('STALE TENANT GROUP')
    expect(JSON.stringify(queryClient.getQueryCache().getAll().map(q => q.state.data))).not.toContain('OLD TENANT CONTENT')
    expect(queryClient.getQueryCache().getAll().some(q => q.queryKey.includes('org-a'))).toBe(false)
  })

  it('clears visible source content and emits accessDenied after a revoked read', async () => {
    const { wrapper, queryClient } = await mountPanel()
    expect(wrapper.text()).toContain('Person 1')
    previewMock.mockRejectedValue(new ApiError(403, 'forbidden'))
    await button(wrapper, 'Refresh preview').trigger('click')
    await flushPromises()
    expect(wrapper.emitted('accessDenied')).toHaveLength(1)
    expect(wrapper.text()).not.toContain('Person 1')
    expect(wrapper.text()).toContain('Preview access is no longer available')
    expect(JSON.stringify(queryClient.getQueryCache().getAll().map(q => q.state.data))).not.toContain('Person 1')
  })

  it('aborts outstanding reads and cannot repopulate plaintext after teardown', async () => {
    const pending = deferred<SnapshotRecordPage>()
    recordsMock.mockReturnValue(pending.promise)
    const { queryClient } = await mountPanel()
    const signal = recordsMock.mock.calls[0]![5]!
    cleanup.pop()!()
    expect(signal.aborted).toBe(true)
    pending.resolve({ records: [{ ...record(), projection: { name: 'LATE AFTER TEARDOWN' } }], next_cursor: null })
    await flushPromises()
    expect(queryClient.getQueryCache().getAll()).toHaveLength(0)
    expect(document.body.textContent).not.toContain('LATE AFTER TEARDOWN')
  })

  it('polls active reports every two seconds, backs off failures, and stops immediately when paused', async () => {
    vi.useFakeTimers()
    previewMock.mockResolvedValue(report('running'))
    const { wrapper } = await mountPanel()
    const initial = previewMock.mock.calls.length
    await vi.advanceTimersByTimeAsync(2_001)
    await flushPromises()
    expect(previewMock.mock.calls.length).toBe(initial + 1)
    previewMock.mockRejectedValue(new ApiError(503, 'unavailable'))
    await vi.advanceTimersByTimeAsync(2_001)
    await flushPromises()
    const afterFailure = previewMock.mock.calls.length
    await vi.advanceTimersByTimeAsync(2_001)
    expect(previewMock.mock.calls.length).toBe(afterFailure)
    previewMock.mockResolvedValue(report('paused'))
    await vi.advanceTimersByTimeAsync(2_001)
    await flushPromises()
    expect(wrapper.find('[data-testid="resume-preview"]').exists()).toBe(true)
    const paused = previewMock.mock.calls.length
    await vi.advanceTimersByTimeAsync(60_000)
    expect(previewMock.mock.calls.length).toBe(paused)
    expect(recordsMock).not.toHaveBeenCalled()
  })

  it('resumes only the DB-only preview and reuses the request ID after an uncertain result', async () => {
    previewMock.mockResolvedValue(report('paused'))
    retryMock.mockRejectedValueOnce(new ApiError(0, 'network_error')).mockResolvedValueOnce({ preview_id: 'preview-a', state: 'queued' })
    const { wrapper } = await mountPanel()
    expect(wrapper.text()).toContain('No Follow Up Boss requests are made')
    await wrapper.get('[data-testid="resume-preview"]').trigger('click')
    await flushPromises()
    expect(wrapper.text()).toContain('result could not be confirmed')
    previewMock.mockResolvedValue(report('queued'))
    await wrapper.get('[data-testid="resume-preview"]').trigger('click')
    await flushPromises()
    expect(retryMock.mock.calls).toEqual([['snapshot-a', 'preview-a', 'request-1'], ['snapshot-a', 'preview-a', 'request-1']])
    expect(retrySnapshot).not.toHaveBeenCalled()
    expect(wrapper.emitted('refresh')).toHaveLength(1)
  })

  it('does not emit refresh when an old resume resolves after the scope changed', async () => {
    const pending = deferred<{ preview_id: string; state: 'queued' }>()
    retryMock.mockReturnValue(pending.promise)
    previewMock.mockResolvedValue(report('paused'))
    const { wrapper } = await mountPanel()
    await wrapper.get('[data-testid="resume-preview"]').trigger('click')
    previewMock.mockResolvedValue(report('completed', 'preview-b'))
    await wrapper.setProps({ previewId: 'preview-b', sessionLifetime: 2 })
    await flushPromises()
    pending.resolve({ preview_id: 'preview-a', state: 'queued' })
    await flushPromises()
    expect(wrapper.emitted('refresh')).toBeUndefined()
    expect(wrapper.find('[data-testid="resume-preview"]').exists()).toBe(false)
  })
})
