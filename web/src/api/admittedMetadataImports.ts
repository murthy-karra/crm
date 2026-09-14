// D-082 / SLICE_010f3_CONTRACT: a separate retained admission-metadata family.
import { apiFetch } from './client'
import { useImportAccess } from './imports'
import type { CoreChangeBoundary } from './coreChangeReports'
import type { MetadataImport, MetadataEnvelope, MetadataReplan, MetadataConfirm, MetadataKind, MetadataResultKind, MetadataMapping, MetadataTarget, MetadataAlias, MetadataRecord, MetadataResult, MetadataIssue, MetadataPage, MetadataSegment, MetadataFieldRequest, MetadataReaders } from './metadataImports'

export interface AdmittedMetadataImport extends MetadataImport {
  admission_id: string
  admission_plan_id: string
  source_report_id: string
  source_output_revision: string
  source_capture_interval: CoreChangeBoundary
  engine_version: 'fub-admitted-metadata-v1'
  shared_claims_ready: boolean
  cohort_counts: { settled_people: string; eligible_people: string; excluded_people: string; settled_metadata_people: string; remaining_people: string }
  progress: { phase: 'fields' | 'cohort' | 'baselines' | 'seal' | 'catalog' | 'people' | 'complete'; fields_processed: string; people_processed: string }
  remainder: { available: boolean; predecessor_import_id: string | null; successor_import_id: string | null; remaining_catalog: string; remaining_people: string; excluded_settled_catalog: string; excluded_settled_people: string; excluded_held_people: string }
  actions: MetadataImport['actions'] & { remainder: boolean }
}
export interface AdmittedMetadataEnvelope extends MetadataEnvelope { import: AdmittedMetadataImport }
export interface AdmittedMetadataPage { imports: AdmittedMetadataImport[]; next_cursor: string | null }
export interface AdmittedMetadataPrepare { request_id: string; admission_id: string; source_report_id: string }
export interface AdmittedMetadataReplan extends MetadataReplan { source_report_id?: string }
const root = '/migrations/fub/admitted-metadata-imports'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
const planPath = (id: string, plan: string) => `${path(id)}/plans/${encodeURIComponent(plan)}`
function query(values: Record<string, string | number | undefined>) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') params.set(key, String(value))
  return `?${params}`
}
const post = <T>(url: string, body: unknown, signal?: AbortSignal) => apiFetch<T>(url, { method: 'POST', body: JSON.stringify(body), signal })
export const fetchAdmittedMetadataImports = (admissionId?: string, cursor?: string, signal?: AbortSignal) => apiFetch<AdmittedMetadataPage>(root + query({ admission_id: admissionId, cursor, limit: 20 }), { signal })
export const fetchAdmittedMetadataImport = (id: string, signal?: AbortSignal) => apiFetch<AdmittedMetadataImport>(path(id), { signal })
export const prepareAdmittedMetadataImport = (body: AdmittedMetadataPrepare, signal?: AbortSignal) => post<AdmittedMetadataEnvelope>(root, body, signal)
export const replanAdmittedMetadataImport = (id: string, body: AdmittedMetadataReplan, signal?: AbortSignal) => post<AdmittedMetadataEnvelope>(`${path(id)}/plans`, body, signal)
export const confirmAdmittedMetadataImport = (id: string, body: MetadataConfirm, signal?: AbortSignal) => post<AdmittedMetadataEnvelope>(`${path(id)}/confirm`, body, signal)
export const retryAdmittedMetadataImport = (id: string, request_id: string, signal?: AbortSignal) => post<AdmittedMetadataEnvelope>(`${path(id)}/retry`, { request_id }, signal)
export const cancelAdmittedMetadataImport = (id: string, request_id: string, signal?: AbortSignal) => post<AdmittedMetadataEnvelope>(`${path(id)}/cancel`, { request_id }, signal)
export const startAdmittedMetadataRemainder = (id: string, request_id: string, signal?: AbortSignal) => post<AdmittedMetadataEnvelope>(`${path(id)}/remainder`, { request_id }, signal)
export const fetchAdmittedMetadataRemainder = (id: string, signal?: AbortSignal) => apiFetch<AdmittedMetadataImport['remainder']>(`${path(id)}/remainder`, { signal })
export const fetchAdmittedMetadataMappings = (id: string, plan: string, kind?: MetadataKind, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataMapping>>(`${planPath(id, plan)}/mappings${query({ kind, cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataTargets = (id: string, plan: string, kind: MetadataKind, fieldId?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataTarget>>(`${planPath(id, plan)}/targets${query({ kind, field_id: fieldId, cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataAliases = (id: string, plan: string, mapping: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataAlias>>(`${planPath(id, plan)}/mappings/${encodeURIComponent(mapping)}/aliases${query({ cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataRecords = (id: string, plan: string, disposition?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataRecord>>(`${planPath(id, plan)}/records${query({ disposition, cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataResults = (id: string, kind?: MetadataResultKind, disposition?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataResult>>(`${path(id)}/results${query({ kind, disposition, cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataIssues = (id: string, plan: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataIssue>>(`${planPath(id, plan)}/issues${query({ cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataProvenance = (person: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataResult>>(`/people/${encodeURIComponent(person)}/admitted-metadata-import-provenance${query({ cursor, limit: 50 })}`, { signal })
export function fetchAdmittedMetadataField(request: MetadataFieldRequest, cursor?: string, signal?: AbortSignal) {
  const base = request.kind === 'provenance' ? `/people/${encodeURIComponent(request.personId)}/admitted-metadata-import-provenance/${encodeURIComponent(request.resultId)}`
    : request.kind === 'result' ? `${path(request.importId)}/results/${encodeURIComponent(request.resultId)}`
      : request.kind === 'mapping' ? `${planPath(request.importId, request.planId)}/mappings/${encodeURIComponent(request.mappingId)}`
        : `${planPath(request.importId, request.planId)}/records/${encodeURIComponent(request.recordId)}`
  return apiFetch<MetadataSegment>(`${base}/fields/${encodeURIComponent(request.fieldKey)}${query({ cursor, limit: 65536 })}`, { signal })
}
// Pass one fixed adapter through the evidence subtree. Both URLs and cache scope
// stay in this family, including nested aliases, target picks and full fields.
export const admittedMetadataReaders: MetadataReaders = {
  namespace: 'admitted-metadata-imports', mappings: fetchAdmittedMetadataMappings,
  targets: fetchAdmittedMetadataTargets, aliases: fetchAdmittedMetadataAliases,
  records: fetchAdmittedMetadataRecords, results: fetchAdmittedMetadataResults,
  field: fetchAdmittedMetadataField,
}
export const useAdmittedMetadataAccess = () => useImportAccess(admittedMetadataReaders.namespace)
export const admittedMetadataActive = (value?: AdmittedMetadataImport) => !!value && (['queued', 'running'].includes(value.state) || value.state === 'proposed' && value.latest_plan?.state === 'building')
