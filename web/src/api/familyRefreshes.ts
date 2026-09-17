import type { TaskKind } from './types'
import { apiFetch } from './client'
import { useImportAccess } from './imports'
export type RefreshFamily = 'metadata' | 'activity' | 'history'
export interface RefreshCounts {
  units: string; inserts: string; updates: string; already_current: string; held: string; excluded: string
  tag_removals: string; field_clears: string; task_completions: string; task_reopens: string; history_corrections: string; source_only: string
}
export interface RefreshBundle {
  id: string; parent_import_id: string; revision: string; state: string; core_report_id: string | null; history_capture_id: string | null
  predecessor_id: string | null; digest: string | null; created_at: string; confirmed_at: string | null
}
export interface RefreshPlan { plan_id: string; family: RefreshFamily; revision: string; state: string; phase: string; pause_reason: string | null; digest: string | null; expires_at: string | null; counts: RefreshCounts; results: RefreshCounts; completed_units: string; remainder_eligible: boolean; retained_bytes: string; reserved_bytes: string; run_byte_limit: string }
export interface RefreshDetail { bundle: RefreshBundle; families: RefreshPlan[] }
export interface RefreshPrepared { bundle_id: string; revision: string; state: string; families: { family: RefreshFamily; plan_id: string; revision: string }[] }
export interface RefreshPrepare { request_id: string; parent_import_id: string; core_report_id: string | null; history_capture_id: string | null; families: RefreshFamily[] }
export type RefreshSelection = { action: 'hold' | 'create_matching' | 'unassigned' } | { action: 'existing'; target_id: string } | { action: 'kind'; kind: TaskKind }
export interface RefreshPatch { mapping_id: string; choice: RefreshSelection }
export interface RefreshReplan { request_id: string; expected_revision: string; family: RefreshFamily; patches: RefreshPatch[]; source_timezone?: string | null }
export interface RefreshConfirm { request_id: string; expected_revision: string; bundle_digest: string; families: { family: RefreshFamily; plan_id: string; plan_revision: string; plan_digest: string; expected_counts: RefreshCounts }[]; acknowledged_exclusions: boolean }
export interface RefreshControl { request_id: string; expected_revision: string; families: RefreshFamily[] }
export interface RefreshMapping {
  id: string; parent_id: string | null; kind: 'tag' | 'field' | 'option' | 'note_author' | 'task_creator' | 'task_assignee' | 'task_kind' | 'timezone'
  label: string | null; value_qualified: boolean; creation_allowed: boolean
  choice: RefreshSelection | { action: 'create_matching'; target_id: string } | { action: 'timezone'; zone: string }
}
export interface RefreshItem { id: string; position: string; kind: string; cohort_id: string | null; person_id: string | null; target_id: string | null; outcome: string; reason: string | null; counts: RefreshCounts }
export interface RefreshResult extends Omit<RefreshItem, 'counts'> { item_id: string; committed_at: string }
export interface RefreshPage<T> { bundle_id: string; bundle_revision: string; plan_id: string; plan_revision: string; family: RefreshFamily; items: T[]; next_cursor: string | null }
export interface RefreshField { id: string; section: string; label: string; total_bytes: string }
export interface RefreshFields { item_id: string; items: RefreshField[]; next_cursor: string | null }
export interface RefreshFragment { item_id: string; field_id: string; total_bytes: string; offset: string; text: string; next_cursor: string | null }
const root = '/migrations/fub/family-refreshes'
const at = (id: string) => `${root}/${encodeURIComponent(id)}`
function query(values: Record<string, string | undefined>) { const q = new URLSearchParams(); for (const [key, value] of Object.entries(values)) if (value !== undefined) q.set(key, value); return `?${q}` }
const read = <T>(url: string, signal?: AbortSignal) => apiFetch<T>(url, { signal, cache: 'no-store' })
const post = <T>(url: string, body: unknown) => apiFetch<T>(url, { method: 'POST', body: JSON.stringify(body), cache: 'no-store' })
export const fetchFamilyRefreshes = (parent: string, cursor?: string, signal?: AbortSignal) => read<{ items: RefreshBundle[]; next_cursor: string | null }>(`${root}${query({ parent_import_id: parent, limit: '25', cursor })}`, signal)
export const fetchFamilyRefresh = (id: string, signal?: AbortSignal) => read<RefreshDetail>(at(id), signal)
export const prepareFamilyRefresh = (body: RefreshPrepare) => post<RefreshPrepared>(root, body)
export const replanFamilyRefresh = (id: string, body: RefreshReplan) => post<RefreshPrepared>(`${at(id)}/plans`, body)
export const confirmFamilyRefresh = (id: string, body: RefreshConfirm) => post<RefreshPrepared>(`${at(id)}/confirm`, body)
export const cancelFamilyRefresh = (id: string, body: RefreshControl) => post<RefreshPrepared>(`${at(id)}/cancel`, body)
export const remainderFamilyRefresh = (id: string, body: RefreshControl) => post<RefreshPrepared>(`${at(id)}/remainder`, body)
export const resumeFamilyRefresh = (id: string, body: RefreshControl) => post<RefreshPrepared>(`${at(id)}/resume`, body)
export const fetchRefreshMappings = (id: string, family: RefreshFamily, plan: string, cursor?: string, signal?: AbortSignal) => read<RefreshPage<RefreshMapping> & { inventory_complete: boolean }>(`${at(id)}/mappings${query({ family, plan_id: plan, limit: '25', cursor })}`, signal)
export const fetchRefreshItems = (id: string, family: RefreshFamily, plan: string, cursor?: string, signal?: AbortSignal, filters: { outcome?: string; cohort_id?: string } = {}) => read<RefreshPage<RefreshItem>>(`${at(id)}/items${query({ family, plan_id: plan, limit: '25', cursor, ...filters })}`, signal)
export const fetchRefreshResults = (id: string, family: RefreshFamily, plan: string, cursor?: string, signal?: AbortSignal, filters: { outcome?: string; cohort_id?: string } = {}) => read<RefreshPage<RefreshResult>>(`${at(id)}/results${query({ family, plan_id: plan, limit: '25', cursor, ...filters })}`, signal)
const fields = (id: string, item: string) => `${at(id)}/items/${encodeURIComponent(item)}/fields`
export const fetchRefreshFields = (id: string, item: string, cursor?: string, signal?: AbortSignal) => read<RefreshFields>(`${fields(id, item)}${query({ limit: '25', cursor })}`, signal)
export const fetchRefreshFragment = (id: string, item: string, field: string, cursor?: string, signal?: AbortSignal) => read<RefreshFragment>(`${fields(id, item)}/${encodeURIComponent(field)}${query({ cursor })}`, signal)
export const useFamilyRefreshAccess = () => useImportAccess('family-refresh')
export interface RefreshTarget { id: string; label: string; field_type: string | null; status: string | null }
export const fetchRefreshTargets = (id: string, mapping: string, cursor?: string, signal?: AbortSignal) => read<{ mapping_id: string; items: RefreshTarget[]; next_cursor: string | null }>(`${at(id)}/mappings/${encodeURIComponent(mapping)}/targets${query({ limit: '25', cursor })}`, signal)
