// Approved 010f1 additive contract; source IDs, revisions and counters remain exact.
import { apiFetch } from './client'
import { useImportAccess, type ImportState, type ImportPlanState } from './imports'

export type MetadataKind = 'tag' | 'field' | 'option'
export type MetadataResultKind = MetadataKind | 'people'
export type MetadataChoice = { kind: 'hold' } | { kind: 'create_matching' } | { kind: 'map_existing'; target_id: string }
export interface MetadataPatch { mapping_id: string; choice: MetadataChoice }
export interface MetadataDispositions {
  planned: string; eligible: string; created: string; applied: string; already_present: string
  held: string; not_supplied: string; source_null: string; pending: string
}
export interface MetadataIssue { code: string; count: string }
export interface MetadataCounts {
  people: { source: string; eligible: string; excluded: string; settled: string }
  tags: MetadataDispositions; fields: MetadataDispositions; options: MetadataDispositions
  tag_links: MetadataDispositions; values: MetadataDispositions
  held_count: string; invalid_source_ids: string; issues: MetadataIssue[]
}
export interface MetadataPlan {
  id: string; revision: string; state: ImportPlanState; phase: string; pause_reason: string | null
  expires_at: string | null; confirmation_digest: string | null; counts: MetadataCounts; max_added_byte_bound: string
}
export interface MetadataImport {
  id: string; parent_import_id: string; parent_plan_id: string; snapshot_id: string; source_account_id: string
  capture_sequence: string; workspace_revision: string; engine_version: string; state: ImportState; phase: string
  pause_reason: string | null; created_at: string; updated_at: string; confirmed_at: string | null; completed_at: string | null
  confirmed_plan_id: string | null; retained_bytes: string; reserved_bytes: string; cancellation_reserved_bytes: string
  release_ready: boolean; counts: MetadataCounts; latest_plan: MetadataPlan | null
  actions: { replan: boolean; confirm: boolean; retry: boolean; cancel: boolean }
  policy: {
    run_byte_limit: string; org_byte_limit: string; run_ceiling_bytes: string; org_ceiling_bytes: string
    run_retained_bytes: string; run_reserved_bytes: string; org_retained_bytes: string; org_reserved_bytes: string
    unit_byte_limit: string; policy_revision: string
  }
  coverage: { embedded_tags_only: true; custom_fields_complete: true; metadata_excluded_people: string; remaining_data: string[]; source_gaps: string[] }
}
export interface MetadataField {
  key: string; label: string; label_abbreviated: boolean; label_full_utf8_bytes: string
  text: string; full_utf8_bytes: string; abbreviated: boolean
}
export interface MetadataSummary { fields: MetadataField[]; total_fields: string; abbreviated: boolean; field_key: string }
export interface MetadataTarget { id: string; kind: MetadataKind; field_id: string | null; label: string; field_type: string | null; source_bound: boolean; archived: false }
export interface MetadataMapping {
  id: string; kind: MetadataKind; parent_mapping_id: string | null; source_id: string | null
  disposition: string; qualified: boolean; create_matching_available: boolean; choice: MetadataChoice; target_id: string | null; field_id: string | null; reasons: string[]
  suggestions: MetadataTarget[]; dependent_count: string; source: MetadataSummary; added_byte_bound: string; alias_count: string
  target: MetadataTarget | null
}
export interface MetadataRecord {
  id: string; source_id: string; person_id: string | null; disposition: string; reasons: string[]
  counts: MetadataCounts; added_byte_bound: string; source: MetadataSummary; operations: MetadataSummary
}
export interface MetadataResult {
  id: string; kind: MetadataResultKind; source_id: string | null; person_id: string | null
  mapping_id: string | null; record_id: string | null; disposition: string; committed_at: string | null
  counts: MetadataCounts; reasons: string[]; source: MetadataSummary; operations: MetadataSummary
}
export interface MetadataAlias { source_id: string; ordinal: string; record_id: string | null; source: MetadataSummary }
export interface MetadataPage<T> { items: T[]; next_cursor: string | null }
export interface MetadataList { imports: MetadataImport[]; next_cursor: string | null }
export interface MetadataEnvelope { import: MetadataImport }
export interface MetadataConfirm {
  request_id: string; plan_id: string; plan_revision: string; confirmation_digest: string; workspace_revision: string
  acknowledgments: { held_count: string; review_only: true; remaining_data: true }
}
export interface MetadataReplan { request_id: string; expected_plan_revision: string; mappings: MetadataPatch[] }
export type MetadataFieldRequest =
  | { kind: 'result'; importId: string; resultId: string; fieldKey: string }
  | { kind: 'record'; importId: string; planId: string; recordId: string; fieldKey: string }
  | { kind: 'mapping'; importId: string; planId: string; mappingId: string; fieldKey: string }
  | { kind: 'provenance'; personId: string; resultId: string; fieldKey: string }
export interface MetadataSegment { text: string; full_utf8_bytes: string; offset_bytes: string; next_cursor: string | null; complete: boolean }

const root = '/migrations/fub/metadata-imports'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
const planPath = (id: string, plan: string) => `${path(id)}/plans/${encodeURIComponent(plan)}`
function query(values: Record<string, string | number | undefined>) {
  const q = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') q.set(key, String(value))
  return `?${q}`
}
function post<T>(url: string, body: unknown, signal?: AbortSignal) { return apiFetch<T>(url, { method: 'POST', body: JSON.stringify(body), signal }) }
export const fetchMetadataImports = (parentId?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataList>(root + query({ parent_import_id: parentId, cursor, limit: 20 }), { signal })
export const fetchMetadataImport = (id: string, signal?: AbortSignal) => apiFetch<MetadataImport>(path(id), { signal })
export const proposeMetadataImport = (body: { request_id: string; parent_import_id: string }, signal?: AbortSignal) => post<MetadataEnvelope>(root, body, signal)
export const replanMetadataImport = (id: string, body: MetadataReplan, signal?: AbortSignal) => post<MetadataEnvelope>(`${path(id)}/plans`, body, signal)
export const confirmMetadataImport = (id: string, body: MetadataConfirm, signal?: AbortSignal) => post<MetadataEnvelope>(`${path(id)}/confirm`, body, signal)
export const retryMetadataImport = (id: string, requestId: string, signal?: AbortSignal) => post<MetadataEnvelope>(`${path(id)}/retry`, { request_id: requestId }, signal)
export const cancelMetadataImport = (id: string, requestId: string, signal?: AbortSignal) => post<MetadataEnvelope>(`${path(id)}/cancel`, { request_id: requestId }, signal)
export const fetchMetadataMappings = (id: string, plan: string, kind?: MetadataKind, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataMapping>>(`${planPath(id, plan)}/mappings${query({ kind, cursor, limit: 50 })}`, { signal })
export const fetchMetadataTargets = (id: string, plan: string, kind: MetadataKind, fieldId?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataTarget>>(`${planPath(id, plan)}/targets${query({ kind, field_id: fieldId, cursor, limit: 50 })}`, { signal })
export const fetchMetadataAliases = (id: string, plan: string, mapping: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataAlias>>(`${planPath(id, plan)}/mappings/${encodeURIComponent(mapping)}/aliases${query({ cursor, limit: 50 })}`, { signal })
export const fetchMetadataRecords = (id: string, plan: string, disposition?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataRecord>>(`${planPath(id, plan)}/records${query({ disposition, cursor, limit: 50 })}`, { signal })
export const fetchMetadataResults = (id: string, kind?: MetadataResultKind, disposition?: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataResult>>(`${path(id)}/results${query({ kind, disposition, cursor, limit: 50 })}`, { signal })
export const fetchMetadataIssues = (id: string, plan: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataIssue>>(`${planPath(id, plan)}/issues${query({ cursor, limit: 50 })}`, { signal })
export const fetchMetadataProvenance = (person: string, cursor?: string, signal?: AbortSignal) => apiFetch<MetadataPage<MetadataResult>>(`/people/${encodeURIComponent(person)}/metadata-import-provenance${query({ cursor, limit: 50 })}`, { signal })
export function fetchMetadataField(request: MetadataFieldRequest, cursor?: string, signal?: AbortSignal) {
  const base = request.kind === 'provenance' ? `/people/${encodeURIComponent(request.personId)}/metadata-import-provenance/${encodeURIComponent(request.resultId)}`
    : request.kind === 'result' ? `${path(request.importId)}/results/${encodeURIComponent(request.resultId)}`
    : request.kind === 'mapping' ? `${planPath(request.importId, request.planId)}/mappings/${encodeURIComponent(request.mappingId)}`
      : `${planPath(request.importId, request.planId)}/records/${encodeURIComponent(request.recordId)}`
  return apiFetch<MetadataSegment>(`${base}/fields/${encodeURIComponent(request.fieldKey)}${query({ cursor, limit: 65536 })}`, { signal })
}
export function useMetadataAccess() { return useImportAccess('metadata-imports') }
export function metadataActive(value?: MetadataImport) { return !!value && (['queued', 'running'].includes(value.state) || value.state === 'proposed' && value.latest_plan?.state === 'building') }
export function metadataName(source: MetadataSummary, fallback = 'Source record') {
  const field = ['label', 'tag', 'choice', 'name', 'firstName', 'raw'].map(label => source.fields.find(f => f.label === label)).find(Boolean)
  if (!field) return fallback
  try { const parsed: unknown = JSON.parse(field.text); return typeof parsed === 'string' ? parsed : field.text }
  catch { return field.text }
}
