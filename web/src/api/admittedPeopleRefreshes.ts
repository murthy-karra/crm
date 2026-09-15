import type { RepairSummary, RepairAck } from './peopleMappingRepairs'
import { apiFetch } from './client'
import { useImportAccess } from './imports'

export type RefreshState = 'preparing' | 'ready' | 'queued' | 'running' | 'paused' | 'completed' | 'cancelled'
export const refreshDispositions = ['eligible', 'already_current', 'held_local_change', 'held_evidence_gap', 'held_mapping_gap', 'held_target_missing', 'held_original_hold', 'excluded_source_only', 'not_seen_again', 'settled', 'settled_noop', 'held_stale', 'cancelled'] as const
export type RefreshDisposition = typeof refreshDispositions[number]
export interface RefreshCounts {
  eligible: string; already_current: string; held: string; excluded: string
  name_clears: string; assignment_clears: string; contact_removals: string; no_instruction: string
}
export interface RefreshPlan { mapping_repair?: RepairSummary | null; id: string; revision: string; digest: string; expires_at: string | null; counts: RefreshCounts }
export interface AdmittedPeopleRefresh {
  mode?: 'refresh' | 'mapping_repair'; repair_source_refresh_id?: string | null; repair_draft_revision?: string; repair_candidates_complete?: boolean; repair_candidate_count?: string
  confirmed_refresh_plan_id?: string | null; mapping_repair_available?: boolean
  id: string; admission_id: string; parent_import_id: string; report_id: string; state: RefreshState; lifecycle_revision: string
  created_at: string; updated_at: string; pause_reason: string | null; progress: { settled_items: string }
  plan?: RefreshPlan; actions: { confirm: boolean; repreview: boolean; retry: boolean; cancel: boolean }
}
export interface RefreshItem {
  id: string; source_id: string | null; person_id: string | null; disposition: RefreshDisposition; settled_at: string | null
  clear_counts: { names: string; assignments: string; contacts: string }; no_instruction: string[]
}
export interface RefreshProjection {
  first_name?: string | null; last_name?: string | null; stage_id?: string | null; assigned_user_id?: string | null
  contact_counts?: { email: string; phone: string }; truncated_fields?: string[]
  current_stage_label?: string | null; current_assignee_label?: string | null
}
export interface RefreshItemDetail extends RefreshItem {
  plan_id: string; plan_revision: string; baseline: RefreshProjection; current: RefreshProjection; proposed: RefreshProjection
}
export interface RefreshContact {
  id: string; side: 'baseline' | 'current' | 'proposed'; change: 'remove' | 'add' | 'retain' | 'current'; contact_id: string | null; kind: 'email' | 'phone'; import_order: number
  value: { value: string; normalized_value: string }
}
export interface RefreshResult { id: string; item_id: string; source_id: string | null; person_id: string | null; disposition: RefreshDisposition; committed_at: string }
export interface RefreshReceipt { refresh_id: string; state: RefreshState }
export interface RefreshAvailability {
  admission_id: string; report_id: string; parent_import_id: string; parent_plan_id: string
  source_account_id: string; workspace_revision: string; available: boolean; closed_reason_code: string | null
}
export interface RefreshConfirm {
  mapping_repair?: RepairAck
  request_id: string; plan_id: string; plan_revision: number; plan_digest: string
  acknowledged_eligible_count: number; acknowledged_coverage: boolean; acknowledged_exclusions: boolean
  acknowledged_name_clears: number; acknowledged_assignment_clears: number; acknowledged_contact_removals: number
}
export interface RefreshPage { next_cursor: string | null; plan_id: string; plan_revision: string }
const root = '/migrations/fub/admitted-people-refreshes'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
const itemPath = (id: string, item: string) => `${path(id)}/items/${encodeURIComponent(item)}`
function query(values: Record<string, string | number | undefined>) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') params.set(key, String(value))
  return `?${params}`
}
const post = <T>(url: string, body: unknown, signal?: AbortSignal) => apiFetch<T>(url, { method: 'POST', body: JSON.stringify(body), signal })
export const prepareAdmittedPeopleRefresh = (admission_id: string, report_id: string, request_id: string, signal?: AbortSignal) => post<RefreshReceipt>(root, { admission_id, report_id, request_id }, signal)
export const fetchAdmittedPeopleRefresh = (id: string, signal?: AbortSignal) => apiFetch<AdmittedPeopleRefresh>(path(id), { signal })
export const fetchAdmittedPeopleRefreshAvailability = (admission_id: string, report_id: string, signal?: AbortSignal) => apiFetch<RefreshAvailability>(`${root}/availability${query({ admission_id, report_id })}`, { signal })
export const fetchAdmittedPeopleRefreshes = (admission_id: string, cursor?: string, signal?: AbortSignal) => apiFetch<{ refreshes: AdmittedPeopleRefresh[]; next_cursor: string | null }>(root + query({ admission_id, cursor, limit: 20 }), { signal })
export const fetchAdmittedPeopleRefreshItems = (id: string, disposition?: RefreshDisposition, cursor?: string, signal?: AbortSignal) => apiFetch<RefreshPage & { items: RefreshItem[] }>(`${path(id)}/items${query({ disposition, cursor, limit: 50 })}`, { signal })
export const fetchAdmittedPeopleRefreshItem = (id: string, item: string, signal?: AbortSignal) => apiFetch<RefreshItemDetail>(itemPath(id, item), { signal })
export const fetchAdmittedPeopleRefreshContacts = (id: string, item: string, cursor?: string, signal?: AbortSignal) => apiFetch<RefreshPage & { contacts: RefreshContact[] }>(`${itemPath(id, item)}/contacts${query({ cursor, limit: 50 })}`, { signal })
export const fetchAdmittedPeopleRefreshResults = (id: string, cursor?: string, signal?: AbortSignal) => apiFetch<{ results: RefreshResult[]; next_cursor: string | null }>(`${path(id)}/results${query({ cursor, limit: 50 })}`, { signal })
export const confirmAdmittedPeopleRefresh = (id: string, body: RefreshConfirm, signal?: AbortSignal) => post<RefreshReceipt>(`${path(id)}/confirm`, body, signal)
export const repreviewAdmittedPeopleRefresh = (id: string, request_id: string, expected_plan_revision: number, signal?: AbortSignal) => post<RefreshReceipt>(`${path(id)}/plans`, { request_id, expected_plan_revision }, signal)
export const retryAdmittedPeopleRefresh = (id: string, request_id: string, expected_lifecycle_revision: number, signal?: AbortSignal) => post<RefreshReceipt>(`${path(id)}/retry`, { request_id, expected_lifecycle_revision }, signal)
export const cancelAdmittedPeopleRefresh = (id: string, request_id: string, expected_lifecycle_revision: number, signal?: AbortSignal) => post<RefreshReceipt>(`${path(id)}/cancel`, { request_id, expected_lifecycle_revision }, signal)
export const useAdmittedPeopleRefreshAccess = () => useImportAccess('admitted-people-refreshes')
export const refreshActive = (value?: AdmittedPeopleRefresh) => !!value && ['preparing', 'queued', 'running'].includes(value.state)
export function refreshInteger(value: string): number {
  if (!/^\d+$/.test(value) || !Number.isSafeInteger(Number(value))) throw new Error('Refresh count or revision cannot be represented safely')
  return Number(value)
}
const labels: Record<string, string> = {
  awaiting_mapping_choices: 'Save mapping choices, then prepare the full People preview', source_boundary_stale: 'Source evidence changed; prepare a new preview',
  preparing: 'Preparing preview', ready: 'Ready for review', queued: 'Queued', running: 'Applying reviewed changes', paused: 'Paused', completed: 'Completed', cancelled: 'Cancelled',
  eligible: 'Eligible update', already_current: 'Already current', held_local_change: 'Held: local changes', held_evidence_gap: 'Held: evidence gap', held_mapping_gap: 'Held: mapping gap', held_target_missing: 'Held: original target missing', held_original_hold: 'Held by original import', excluded_source_only: 'Excluded source change', not_seen_again: 'Not seen again; retained locally', settled: 'Updated', settled_noop: 'Verified without a change', held_stale: 'Held: changed after preview',
  first_name: 'First name', last_name: 'Last name', firstName: 'First name', lastName: 'Last name', emails: 'Emails', phones: 'Phones', stage: 'Stage', assignment: 'Assignment', assigned_user_id: 'Assignment', stage_id: 'Stage',
  retained_integrity_failed: 'Retained evidence could not be verified', retained_evidence_invalid: 'Retained evidence is incomplete or invalid', initiator_not_authorized: 'Initiating administrator access changed', source_binding_changed: 'Retained source binding changed', work_unit_timed_out: 'Work unit interrupted; retry available',
  storage_limit: 'Storage allowance reached', storage_budget_exhausted: 'Storage allowance reached', release_not_ready: 'Compatible release required', lease_expired: 'Worker interrupted', actor_inactive: 'Initiating administrator is inactive', actor_not_admin: 'Initiating administrator access changed', evidence_unavailable: 'Retained evidence unavailable', corrupt_evidence: 'Retained evidence could not be verified',
  report_not_completed: 'The selected report is not sealed', admission_not_terminal: 'The selected admission is not terminal', admission_not_confirmed: 'The selected admission has no confirmed plan', parent_not_completed: 'The original People import is not completed', review_workspace_required: 'This workspace is no longer in migration review', binding_mismatch: 'The retained admission and report bindings no longer match', no_successful_results: 'The selected admission has no settled People', later_capture_required: 'The selected report does not begin after the admission boundary', source_boundary_not_newer: 'A newer admitted-People refresh boundary is already confirmed', active_refresh_exists: 'Another admitted-People refresh is still active for this admission',
}
export const refreshLabel = (value: string) => Object.hasOwn(labels, value) ? labels[value]! : 'Unrecognized refresh detail'

export interface RefreshFieldFragment extends RefreshPage { side: string; field: string; offset: string; total_bytes: string; fragment: string }
export const fetchAdmittedPeopleRefreshField = (id: string, item: string, side: 'baseline' | 'current' | 'proposed', field: 'first_name' | 'last_name', cursor?: string, signal?: AbortSignal) => apiFetch<RefreshFieldFragment>(`${itemPath(id, item)}/fields/${side}/${field}${query({ cursor, limit: 16384 })}`, { signal })
