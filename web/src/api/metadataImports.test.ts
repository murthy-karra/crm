import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ApiError, apiFetch } from './client'
import { useImportAccess } from './imports'
import {
  cancelMetadataImport,
  confirmMetadataImport,
  fetchMetadataAliases,
  fetchMetadataField,
  fetchMetadataImport,
  fetchMetadataImports,
  fetchMetadataIssues,
  fetchMetadataMappings,
  fetchMetadataProvenance,
  fetchMetadataRecords,
  fetchMetadataResults,
  fetchMetadataTargets,
  metadataName,
  proposeMetadataImport,
  replanMetadataImport,
  retryMetadataImport,
  useMetadataAccess,
  type MetadataAlias,
  type MetadataConfirm,
  type MetadataFieldRequest,
  type MetadataPage,
  type MetadataReplan,
  type MetadataSegment,
  type MetadataSummary,
} from './metadataImports'

vi.mock('./client', async original => ({
  ...await original<typeof import('./client')>(),
  apiFetch: vi.fn(),
}))
vi.mock('./imports', () => ({ useImportAccess: vi.fn() }))

const fetch = vi.mocked(apiFetch)
const access = vi.mocked(useImportAccess)
const root = '/migrations/fub/metadata-imports'
const revision = '900719925474099312345678901234567890'
const requestId = '018f1889-8821-7800-8000-000000000001'

beforeEach(() => {
  fetch.mockReset()
  access.mockReset()
})

describe('metadata import transport', () => {
  it('preserves explicit mutation choices, request IDs and confirmation inputs', async () => {
    const signal = new AbortController().signal
    const proposal = { request_id: requestId, parent_import_id: 'parent-import' }
    const patch: MetadataReplan = {
      request_id: '018f1889-8821-7800-8000-000000000002',
      expected_plan_revision: revision,
      mappings: [
        { mapping_id: 'mapping-tag', choice: { kind: 'create_matching' } },
        { mapping_id: 'mapping-field', choice: { kind: 'map_existing', target_id: 'existing-field' } },
        { mapping_id: 'mapping-option', choice: { kind: 'hold' } },
      ],
    }
    const confirmation: MetadataConfirm = {
      request_id: '018f1889-8821-7800-8000-000000000003',
      plan_id: 'plan',
      plan_revision: revision,
      confirmation_digest: 'opaque+/digest=',
      workspace_revision: '900719925474099312345678901234567891',
      acknowledgments: {
        held_count: '900719925474099312345678901234567892',
        review_only: true,
        remaining_data: true,
      },
    }
    const retryId = '018f1889-8821-7800-8000-000000000004'
    const cancelId = '018f1889-8821-7800-8000-000000000005'

    await proposeMetadataImport(proposal, signal)
    await replanMetadataImport('child/one', patch, signal)
    await confirmMetadataImport('child/one', confirmation, signal)
    await confirmMetadataImport('child/one', confirmation, signal)
    await retryMetadataImport('child/one', retryId, signal)
    await cancelMetadataImport('child/one', cancelId, signal)

    expect(fetch.mock.calls.map(([url, options]) => [url, JSON.parse(String(options?.body))])).toEqual([
      [root, proposal],
      [`${root}/child%2Fone/plans`, patch],
      [`${root}/child%2Fone/confirm`, confirmation],
      [`${root}/child%2Fone/confirm`, confirmation],
      [`${root}/child%2Fone/retry`, { request_id: retryId }],
      [`${root}/child%2Fone/cancel`, { request_id: cancelId }],
    ])
    expect(fetch.mock.calls[2]![1]?.body).toBe(fetch.mock.calls[3]![1]?.body)
    expect(fetch.mock.calls.every(([, options]) => options?.method === 'POST' && options.signal === signal)).toBe(true)
  })

  it('lists only the requested parent and encodes child identity as one path segment', async () => {
    const signal = new AbortController().signal
    await fetchMetadataImports('parent/one', 'opaque +/=?&', signal)
    await fetchMetadataImports(undefined, undefined, signal)
    await fetchMetadataImport('child/one?#', signal)

    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      `${root}?parent_import_id=parent%2Fone&cursor=opaque+%2B%2F%3D%3F%26&limit=20`,
      `${root}?limit=20`,
      `${root}/child%2Fone%3F%23`,
    ])
    expect(fetch.mock.calls.every(([, options]) => options?.signal === signal && options.body === undefined)).toBe(true)
  })

  it('uses separate bounded mapping, target, alias, record, result, issue and provenance routes', async () => {
    const signal = new AbortController().signal
    const cursor = 'opaque +/=?&'
    await fetchMetadataMappings('i/1', 'p/1', 'field', cursor, signal)
    await fetchMetadataTargets('i/1', 'p/1', 'option', 'field/1', cursor, signal)
    await fetchMetadataAliases('i/1', 'p/1', 'm/1', cursor, signal)
    await fetchMetadataRecords('i/1', 'p/1', 'held', cursor, signal)
    await fetchMetadataResults('i/1', 'people', 'applied', cursor, signal)
    await fetchMetadataIssues('i/1', 'p/1', cursor, signal)
    await fetchMetadataProvenance('person/1', cursor, signal)

    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      `${root}/i%2F1/plans/p%2F1/mappings?kind=field&cursor=opaque+%2B%2F%3D%3F%26&limit=50`,
      `${root}/i%2F1/plans/p%2F1/targets?kind=option&field_id=field%2F1&cursor=opaque+%2B%2F%3D%3F%26&limit=50`,
      `${root}/i%2F1/plans/p%2F1/mappings/m%2F1/aliases?cursor=opaque+%2B%2F%3D%3F%26&limit=50`,
      `${root}/i%2F1/plans/p%2F1/records?disposition=held&cursor=opaque+%2B%2F%3D%3F%26&limit=50`,
      `${root}/i%2F1/results?kind=people&disposition=applied&cursor=opaque+%2B%2F%3D%3F%26&limit=50`,
      `${root}/i%2F1/plans/p%2F1/issues?cursor=opaque+%2B%2F%3D%3F%26&limit=50`,
      '/people/person%2F1/metadata-import-provenance?cursor=opaque+%2B%2F%3D%3F%26&limit=50',
    ])
    expect(fetch.mock.calls.every(([, options]) => options?.signal === signal && options.body === undefined)).toBe(true)
  })

  it('does not invent filters or field dependencies when optional inputs are absent', async () => {
    await fetchMetadataMappings('i', 'p')
    await fetchMetadataTargets('i', 'p', 'field')
    await fetchMetadataAliases('i', 'p', 'm')
    await fetchMetadataRecords('i', 'p')
    await fetchMetadataResults('i')
    await fetchMetadataIssues('i', 'p')
    await fetchMetadataProvenance('person')

    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      `${root}/i/plans/p/mappings?limit=50`,
      `${root}/i/plans/p/targets?kind=field&limit=50`,
      `${root}/i/plans/p/mappings/m/aliases?limit=50`,
      `${root}/i/plans/p/records?limit=50`,
      `${root}/i/results?limit=50`,
      `${root}/i/plans/p/issues?limit=50`,
      '/people/person/metadata-import-provenance?limit=50',
    ])
  })

  it('keeps all four segmented evidence owners distinct, including committed results', async () => {
    const signal = new AbortController().signal
    const requests: MetadataFieldRequest[] = [
      { kind: 'mapping', importId: 'i/1', planId: 'p/1', mappingId: 'm/1', fieldKey: 'source.a/b +#' },
      { kind: 'record', importId: 'i/1', planId: 'p/1', recordId: 'r/1', fieldKey: 'operations.all' },
      { kind: 'result', importId: 'i/1', resultId: 'result/1', fieldKey: 'source.all' },
      { kind: 'provenance', personId: 'person/1', resultId: 'result/2', fieldKey: 'all' },
    ]
    for (const request of requests) await fetchMetadataField(request, 'segment +/=', signal)

    expect(fetch.mock.calls.map(([url]) => url)).toEqual([
      `${root}/i%2F1/plans/p%2F1/mappings/m%2F1/fields/source.a%2Fb%20%2B%23?cursor=segment+%2B%2F%3D&limit=65536`,
      `${root}/i%2F1/plans/p%2F1/records/r%2F1/fields/operations.all?cursor=segment+%2B%2F%3D&limit=65536`,
      `${root}/i%2F1/results/result%2F1/fields/source.all?cursor=segment+%2B%2F%3D&limit=65536`,
      '/people/person%2F1/metadata-import-provenance/result%2F2/fields/all?cursor=segment+%2B%2F%3D&limit=65536',
    ])
    expect(fetch.mock.calls.every(([, options]) => options?.signal === signal)).toBe(true)
  })

  it('keeps exact 128-digit source IDs and opaque continuation cursors from alias evidence', async () => {
    const sourceId = '9'.repeat(128)
    const response: MetadataPage<MetadataAlias> = {
      items: [{
        source_id: sourceId,
        ordinal: '17',
        record_id: 'source-record',
        source: {
          fields: [{
            key: 'source.name', label: 'raw', label_abbreviated: false, label_full_utf8_bytes: '3',
            text: '"  Buyer  "', full_utf8_bytes: '11', abbreviated: false,
          }],
          total_fields: '1', abbreviated: false, field_key: 'source.all',
        },
      }],
      next_cursor: 'opaque +/=?&900719925474099312345678901234567890',
    }
    fetch.mockResolvedValueOnce(response)

    const actual = await fetchMetadataAliases('i', 'p', 'm')
    expect(actual).toBe(response)
    expect(actual.items[0]!.source_id).toBe(sourceId)
    await fetchMetadataAliases('i', 'p', 'm', actual.next_cursor!)
    expect(fetch.mock.calls[1]![0]).toBe(
      `${root}/i/plans/p/mappings/m/aliases?cursor=opaque+%2B%2F%3D%3F%26900719925474099312345678901234567890&limit=50`,
    )
  })

  it('preserves exact segment text and byte counters without parsing source JSON', async () => {
    const response: MetadataSegment = {
      text: '{"id":900719925474099312345678901234567890,"choice":"None","note":"José 🏡"}',
      full_utf8_bytes: '900719925474099312345678901234567890',
      offset_bytes: '900719925474099312345678901234500000',
      next_cursor: 'signed+cursor/=',
      complete: false,
    }
    fetch.mockResolvedValueOnce(response)
    const actual = await fetchMetadataField({
      kind: 'result', importId: 'i', resultId: 'result', fieldKey: 'all',
    })

    expect(actual).toBe(response)
    expect(actual.text).toContain('900719925474099312345678901234567890')
    expect(actual.full_utf8_bytes).toBe(response.full_utf8_bytes)
    expect(actual.offset_bytes).toBe(response.offset_bytes)
    expect(fetch.mock.calls[0]![0]).toBe(`${root}/i/results/result/fields/all?limit=65536`)
  })

  it('passes uncertain mutation failures to the caller without an automatic retry or new request ID', async () => {
    const failure = new ApiError(503, 'temporarily_unavailable')
    fetch.mockRejectedValueOnce(failure)

    await expect(retryMetadataImport('i', requestId)).rejects.toBe(failure)
    expect(fetch).toHaveBeenCalledTimes(1)
    expect(fetch.mock.calls[0]![1]?.body).toBe(JSON.stringify({ request_id: requestId }))
  })

  it('selects the metadata cache namespace when sharing the import access lifecycle', () => {
    useMetadataAccess()
    expect(access).toHaveBeenCalledExactlyOnceWith('metadata-imports')
  })

  it('names actual tag, choice and field summaries and prefers a field label over its machine name', () => {
    function source(entries: Array<[string, string]>): MetadataSummary {
      return {
        fields: entries.map(([label, text], index) => ({
          key: `source.${index.toString(16).padStart(64, '0')}`,
          label, label_abbreviated: false, label_full_utf8_bytes: String(label.length),
          text, full_utf8_bytes: String(new TextEncoder().encode(text).length), abbreviated: false,
        })),
        total_fields: String(entries.length), abbreviated: false, field_key: 'source.all',
      }
    }

    expect(metadataName(source([['tag', '"  Buyer  "']]))).toBe('  Buyer  ')
    expect(metadataName(source([['choice', '"None"'], ['ordinal', '17']]))).toBe('None')
    expect(metadataName(source([
      ['name', '"customPreferredArea"'], ['label', '"Preferred Area"'], ['type', '"dropdown"'],
    ]))).toBe('Preferred Area')
    expect(metadataName(source([['name', '"customPreferredArea"'], ['type', '"text"']]))).toBe('customPreferredArea')
  })
})
