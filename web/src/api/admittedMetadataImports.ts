import { apiFetch } from './client'

export type AdmittedMetadataImport = {
  id: string
  parent_import_id: string
  admission_id: string
  source_report_id: string
  snapshot_id: string
  source_account_id: string
  capture_sequence: string
  workspace_revision: string
  engine_version: 'fub-admitted-metadata-v1'
  state: 'proposed' | 'queued' | 'running' | 'paused' | 'completed' | 'cancelled' | 'expired'
  phase: 'preparation' | 'catalog' | 'people' | 'complete'
  shared_claims_ready: boolean
  cohort_counts: { settled_people: string }
  remainder: { available: boolean }
  actions: { replan: boolean; confirm: boolean; retry: boolean; cancel: boolean; remainder: boolean }
}
export type AdmittedMetadataPage = { imports: AdmittedMetadataImport[]; next_cursor: string | null }
const root = '/migrations/fub/admitted-metadata-imports'
const query = (value: Record<string, string | number | undefined>) => {
  const pairs = Object.entries(value).filter(([, item]) => item !== undefined)
  return pairs.length ? `?${new URLSearchParams(pairs.map(([key, item]) => [key, String(item)]))}` : ''
}
export const fetchAdmittedMetadataImports = (admissionId?: string, cursor?: string, signal?: AbortSignal) => apiFetch<AdmittedMetadataPage>(`${root}${query({ admission_id: admissionId, cursor, limit: 50 })}`, { signal })
export const fetchAdmittedMetadataImport = (id: string, signal?: AbortSignal) => apiFetch<AdmittedMetadataImport>(`${root}/${encodeURIComponent(id)}`, { signal })
export const prepareAdmittedMetadataImport = (body: { request_id: string; admission_id: string; source_report_id: string }) => apiFetch<{ import: AdmittedMetadataImport; request_id: string }>(root, { method: 'POST', body: JSON.stringify(body) })
