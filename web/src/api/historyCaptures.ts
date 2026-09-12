import { apiFetch } from './client'
import { useImportAccess } from './imports'

export type HistoryFamily = 'events' | 'calls' | 'text_messages'
export type HistoryDisposition = 'linked' | 'parent_excluded' | 'no_parent_identity' | 'invalid_person_reference' | 'conflicting_reference'
export type HistoryState = 'proposed' | 'queued' | 'running' | 'waiting_retry' | 'paused' | 'completed_with_gaps' | 'cancelled'
export interface HistoryStream {
  family: HistoryFamily; state: string; checkpoint: string; reported_total: string | null
  occurrences: string; valid_occurrences: string; invalid_occurrences: string; unique_ids: string
  equal_repeats: string; conflicting_variants: string; linked: string; parent_excluded: string
  no_parent_identity: string; invalid_person_reference: string; conflicting_reference: string; attempts: string
  api_inaccessible_count: null; count_basis: 'advancing_pages'; content_scope: 'exact_returned_json_only'
  enumeration_is_complete_account_history: false
}
export interface HistoryCapture {
  id: string; parent_import_id: string; parent_plan_id: string; snapshot_id: string; parent_capture_sequence: string
  connection_id: string; connection_revision: string; source_account_id: string; source_user_id: string
  parent_source_user_id: string | null; source_user_difference: boolean; source_user_evidence_revision: string
  profile_version: string; schema_version: string; parser_version: string; initiated_by_user_id: string
  state: HistoryState; revision: string; pause_reason: string | null; created_at: string; proposal_expires_at: string
  started_at: string | null; completed_at: string | null; confirmed_at: string | null
  parent_source_started_at: string | null; parent_source_completed_at: string | null
  capture_sequence: string; raw_bytes: string; retained_bytes: string; reserved_bytes: string; run_byte_limit: string
  org_byte_limit: string; org_retained_bytes: string; org_reserved_bytes: string; run_budget_revision: string
  org_budget_revision: string; policy_revision: string; run_ceiling_bytes: string; org_ceiling_bytes: string
  required_reservation_bytes: string; release_ready: boolean; coverage_reasons: string[]
  actions: { confirm: boolean; retry: boolean; cancel: boolean; increase_budget: boolean }; streams: HistoryStream[]
}
export interface HistoryReceipt { capture_id: string; revision: string; state: HistoryState }
export interface HistoryProposal { request_id: string; parent_import_id: string; connection_id: string; expected_revision: string }
export interface HistoryAction { request_id: string; expected_run_revision: string }
export interface HistoryConfirmation extends HistoryAction {
  acknowledgements: { api_visible_account_scope: true; coverage_gaps: true; retained_not_imported: true; source_user_evidence_revision: string; source_user_difference: boolean }
}
export interface HistoryBudget extends HistoryAction {
  expected_run_budget_revision: string; expected_org_budget_revision: string; expected_policy_revision: string
  run_byte_limit: string; org_byte_limit: string
}
export interface HistoryRecord {
  id: string; capture_id: string; capture_sequence: string; series_capture_sequence: string; ordinal: string
  family: HistoryFamily; source_id: string | null; source_person_id: string | null; source_user_ids: string[]
  source_created: string | null; source_updated: string | null; source_kind: string
  source_event_type?: string | null; source_outcome?: string | null; source_duration?: string | null; source_is_incoming?: boolean | null
  source_timestamp_uncertain: boolean; preview_truncated: boolean; relationship_uncertain: boolean
  content_availability: 'returned_in_raw' | 'not_returned'; disposition: HistoryDisposition; person_id: string | null
  reference_available: boolean; variant_count: string; observation_count: string; representation: string
  profile_version: string; parser_version: string; schema_version: string; captured_at: string; integrity_status: 'verified_projection'
}
export interface HistoryRecordPage { records: HistoryRecord[]; next_cursor: string | null; capture_sequence: string; counter_basis: 'current_run'; counts: HistoryStream[] }
export interface HistoryFilters { family?: HistoryFamily; disposition?: HistoryDisposition; record_id?: string }
const root = '/migrations/fub/history-captures'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
function query(values: Record<string, string | number | undefined>) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') params.set(key, String(value))
  return `?${params}`
}
const post = (url: string, body: unknown, signal?: AbortSignal) => apiFetch<HistoryReceipt>(url, { method: 'POST', body: JSON.stringify(body), signal })
export const fetchHistoryCaptures = (parent?: string, cursor?: string, signal?: AbortSignal) => apiFetch<{ captures: HistoryCapture[]; next_cursor: string | null }>(root + query({ parent_import_id: parent, cursor, limit: 20 }), { signal })
export const fetchHistoryCapture = (id: string, signal?: AbortSignal) => apiFetch<HistoryCapture>(path(id), { signal })
export const proposeHistoryCapture = (body: HistoryProposal, signal?: AbortSignal) => post(root, body, signal)
export const confirmHistoryCapture = (id: string, body: HistoryConfirmation, signal?: AbortSignal) => post(`${path(id)}/confirm`, body, signal)
export const retryHistoryCapture = (id: string, body: HistoryAction, signal?: AbortSignal) => post(`${path(id)}/retry`, body, signal)
export const cancelHistoryCapture = (id: string, body: HistoryAction, signal?: AbortSignal) => post(`${path(id)}/cancel`, body, signal)
export const increaseHistoryBudget = (id: string, body: HistoryBudget, signal?: AbortSignal) => post(`${path(id)}/budget`, body, signal)
export const fetchHistoryRecords = (id: string, filters: HistoryFilters = {}, cursor?: string, signal?: AbortSignal) => apiFetch<HistoryRecordPage>(`${path(id)}/records${query({ ...filters, cursor, limit: 50 })}`, { signal })
export const fetchHistoryRecord = (id: string, record: string, signal?: AbortSignal) => apiFetch<HistoryRecord>(`${path(id)}/records/${encodeURIComponent(record)}`, { signal })
export const useHistoryAccess = () => useImportAccess('history-captures')
export const historyActive = (run?: HistoryCapture) => !!run && ['queued', 'running', 'waiting_retry'].includes(run.state)
export const historyFamilies: HistoryFamily[] = ['events', 'calls', 'text_messages']
export const historyDispositions: HistoryDisposition[] = ['linked', 'parent_excluded', 'no_parent_identity', 'invalid_person_reference', 'conflicting_reference']
export function historyLabel(value: string) {
  const labels: Record<string, string> = { events: 'Events', calls: 'Calls', text_messages: 'Texts', linked: 'Linked to parent Person', parent_excluded: 'Held or excluded by parent', no_parent_identity: 'No parent identity', invalid_person_reference: 'Invalid or missing Person reference', conflicting_reference: 'Conflicting Person reference', enumerated: 'Selected query enumerated', enumeration_identity_uncertain: 'Record identity coverage is uncertain', source_identity_mismatch: 'Source account or user changed', connection_changed: 'Connection changed', parent_changed: 'People parent changed', storage_limit: 'Storage allowance reached', source_unavailable: 'Source unavailable', access_denied: 'Source access denied', retained_not_imported: 'Retained, not imported', api_restricted_records_unknown: 'Restricted records: unknown', detail_content_not_fetched: 'Details and full content not fetched', not_atomic_snapshot: 'Source changes during capture may not be represented' }
  return labels[value] ?? (value.charAt(0).toUpperCase() + value.slice(1).replaceAll('_', ' '))
}
