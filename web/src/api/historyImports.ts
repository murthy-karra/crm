import { apiFetch } from './client'
import { useImportAccess } from './imports'
import type { HistoryFamily, HistoryStream } from './historyCaptures'

export type HistoryImportState = 'preparing' | 'ready' | 'queued' | 'running' | 'paused' | 'completed' | 'cancelled'
export interface HistoryImportCounts { occurrences: string; eligible: string; equal_repeats: string; held: string; processed: string; inserted: string; already_imported: string; application_held: string }
export interface HistoryImport {
  id: string; plan_id: string; parent_import_id: string; capture_id: string; state: HistoryImportState
  phase: 'capture' | 'classify' | 'apply' | 'finished'; revision: string; plan_revision: string; workspace_revision: string
  interpretation_version: string; reader_version: string; capture_revision: string; capture_sequence: string
  parent_capture_sequence: string; executor_user_id: string; created_at: string; updated_at: string
  confirmed_at: string | null; completed_at: string | null; plan_expires_at: string | null; pause_reason: string | null
  preview_complete: boolean; counts: HistoryImportCounts
  coverage: { streams: HistoryStream[]; warnings: string[]; api_inaccessible_count: null; enumeration_is_complete_account_history: false }
  added_byte_bound: string
  retained_bytes: string; reserved_bytes: string; run_byte_limit: string; run_budget_revision: string
  org_byte_limit: string; org_budget_revision: string; policy_revision: string; run_byte_ceiling: string
  org_byte_ceiling: string; release_ready: boolean
  actions: { confirm: boolean; resume: boolean; cancel: boolean; increase_budget: boolean; prepare_same_plan: boolean }
}
export interface HistoryImportReceipt { import_id: string; plan_id: string; revision: string; state: HistoryImportState }
export interface HistoryImportPrepare { request_id: string; parent_import_id: string; capture_id: string; expected_capture_revision: string; expected_workspace_revision: string; expected_policy_revision: string }
export interface HistoryImportConfirm {
  request_id: string; plan_id: string; expected_revision: string; expected_plan_revision: string
  expected_workspace_revision: string; expected_policy_revision: string
  acknowledgements: { external_facts: true; date_uncertainty: true; coverage_and_holds: true; review_only: true }
}
export interface HistoryImportAction { request_id: string; expected_revision: string }
export interface HistoryImportResume extends HistoryImportAction { expected_policy_revision: string }
export interface HistoryImportBudget extends HistoryImportAction {
  expected_run_budget_revision: string; expected_org_budget_revision: string; expected_policy_revision: string
  run_byte_limit: string; org_byte_limit: string
}
export interface ImportedMetadata {
  source_id: string | null; source_person_id: string | null; source_access_user_id: string | null
  source_attributed_user_id: string | null; source_creator_user_id: string | null; source_editor_user_id: string | null
  source_created: string | null; source_updated: string | null; source_sent: string | null
  source_event_type: string | null; source_note_id: string | null; source_is_incoming: boolean | null
  source_duration: string | null; source_duration_unit: string | null; source_outcome: string | null
  source_status: string | null; content_availability: string | null; preview_truncated: boolean
}
export interface HistoryManifestItem {
  id: string; position: string; family: HistoryFamily; disposition: 'eligible' | 'equal_repeat' | 'held'
  reason: string | null; person_id: string | null; metadata: ImportedMetadata | null
  capture_id: string; ordinal: number; observation_id: string
}
export interface HistoryResultItem extends Omit<HistoryManifestItem, 'disposition'> {
  disposition: 'imported' | 'already_imported' | 'equal_repeat' | 'held'; result_id: string; fact_id: string | null
}
export interface HistoryImportRecordPage { records: HistoryManifestItem[]; next_cursor: string | null; plan_id: string; revision: string }
export interface HistoryImportResultPage { results: HistoryResultItem[]; next_cursor: string | null; plan_id: string; revision: string }
export interface HistoryImportFilters { family?: HistoryFamily; disposition?: string }
const root = '/migrations/fub/history-imports'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
function query(values: Record<string, string | number | undefined>) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') params.set(key, String(value))
  return `?${params}`
}
const post = (url: string, body: unknown, signal?: AbortSignal) => apiFetch<HistoryImportReceipt>(url, { method: 'POST', body: JSON.stringify(body), signal, cache: 'no-store' })
export const fetchHistoryImports = (parent?: string, cursor?: string, signal?: AbortSignal) => apiFetch<{ imports: HistoryImport[]; next_cursor: string | null }>(root + query({ parent_import_id: parent, limit: 25, cursor }), { signal, cache: 'no-store' })
export const fetchHistoryImport = (id: string, signal?: AbortSignal) => apiFetch<HistoryImport>(path(id), { signal, cache: 'no-store' })
export const prepareHistoryImport = (body: HistoryImportPrepare, signal?: AbortSignal) => post(root, body, signal)
export const confirmHistoryImport = (id: string, body: HistoryImportConfirm, signal?: AbortSignal) => post(`${path(id)}/confirm`, body, signal)
export const resumeHistoryImport = (id: string, body: HistoryImportResume, signal?: AbortSignal) => post(`${path(id)}/resume`, body, signal)
export const cancelHistoryImport = (id: string, body: HistoryImportAction, signal?: AbortSignal) => post(`${path(id)}/cancel`, body, signal)
export const increaseHistoryImportBudget = (id: string, body: HistoryImportBudget, signal?: AbortSignal) => post(`${path(id)}/budget`, body, signal)
export const fetchHistoryImportRecords = (id: string, filters: HistoryImportFilters = {}, cursor?: string, signal?: AbortSignal) => apiFetch<HistoryImportRecordPage>(`${path(id)}/records${query({ ...filters, limit: 25, cursor })}`, { signal, cache: 'no-store' })
export const fetchHistoryImportResults = (id: string, filters: HistoryImportFilters = {}, cursor?: string, signal?: AbortSignal) => apiFetch<HistoryImportResultPage>(`${path(id)}/results${query({ ...filters, limit: 25, cursor })}`, { signal, cache: 'no-store' })
export const useHistoryImportAccess = () => useImportAccess('history-imports')
export const historyImportActive = (run?: HistoryImport) => !!run && ['preparing', 'queued', 'running'].includes(run.state)
