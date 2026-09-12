// D-068: source IDs, revisions, counters and byte amounts stay exact decimal strings.
import { apiFetch } from './client'
import { useImportAccess } from './imports'
import type { MetadataSummary, MetadataImport } from './metadataImports'
export type ActivityKind = 'note' | 'task'
export type ActivityRole = 'note_author' | 'task_creator' | 'task_assignee' | 'task_kind'
export type ActivityTaskKind = 'call' | 'email' | 'text' | 'follow_up' | 'other'
export type ActivityChoice = { kind: 'hold' } | { kind: 'leave_unmapped' } | { kind: 'map_existing'; target_id: string } | { kind: 'map_kind'; native_kind: ActivityTaskKind }
export interface ActivityPatch { mapping_id: string; choice: ActivityChoice }
export interface ActivityFamilyCounts { planned: string; eligible: string; applied: string; already_present: string; held: string; pending: string }
export interface ActivityCounts { notes: ActivityFamilyCounts; tasks: ActivityFamilyCounts; held_count: string; source_only_count: string; invalid_occurrences: string; unavailable_bodies: string }
export interface ActivityPlan { id: string; revision: string; state: 'building' | 'ready' | 'paused' | 'superseded'; phase: string; expires_at: string | null; counts: ActivityCounts; source_timezone: string | null; source_engine: string; html_profile: string; time_profile: string; tzdb_version: string; confirmation_digest: string | null; max_added_byte_bound: string }
export interface ActivityImport {
  id: string; parent_import_id: string; parent_plan_id: string; snapshot_id: string; source_account_id: string; capture_sequence: string; workspace_revision: string; revision: string; activity_revision: string; engine_version: string
  state: 'preparing' | 'ready' | 'queued' | 'running' | 'paused' | 'completed' | 'cancelled'; phase: 'preparation' | 'records' | 'complete'; pause_reason: string | null
  created_at: string; updated_at: string; confirmed_at: string | null; confirmed_plan_id: string | null; completed_at: string | null
  retained_bytes: string; reserved_bytes: string; native_row_bytes: string; cancellation_reserved_bytes: string; release_ready: boolean; counts: ActivityCounts; policy: MetadataImport['policy']; coverage: { remaining_data: string[]; native_review_only: true }; latest_plan: ActivityPlan; actions: { replan: boolean; confirm: boolean; retry: boolean; cancel: boolean }
}
export interface ActivityReplan { request_id: string; expected_plan_id: string; choices: ActivityPatch[]; source_timezone?: string | null }
export interface ActivityConfirm { request_id: string; plan_id: string; expected_revision: string; acknowledge_held: string; acknowledge_source_only: string }
export interface ActivityAction { request_id: string; expected_revision: string }
export interface ActivityEnvelope { import: ActivityImport }
export interface ActivityPage<T> { items: T[]; next_cursor: string | null }
export interface ActivityList { imports: ActivityImport[]; next_cursor: string | null }
export interface ActivityMapping { id: string; role: ActivityRole; source_value: string; source_value_abbreviated: boolean; source_value_full_utf8_bytes: string; choice: ActivityChoice; suggestions: string[]; suggested_kind: ActivityTaskKind | null; dependent_count: string; source_summary: MetadataSummary; field_url: string }
export interface ActivityTarget { id: string; display_name: string; status: 'active' | 'inactive'; role: string }
export type ActivityNative = { kind: 'note'; body: string; author_source_id: string | null; created_at: string; updated_at: string } | { kind: 'task'; title: string; source_type: string; source_type_abbreviated: boolean; source_type_full_utf8_bytes: string; creator_source_id: string | null; assignee_source_id: string | null; created_at: string; updated_at: string; due_at: string | null; completed_at: string | null }
export interface ActivityPreview { native: ActivityNative | null; native_kind: ActivityTaskKind | null; reasons: string[]; transformations: string[]; source_only: Record<string, string>; source_observations: string }
export interface ActivityRecord { id: string; kind: ActivityKind; source_id: string | null; person_id: string | null; target_id: string | null; disposition: 'eligible' | 'already_present' | 'held'; source_summary: MetadataSummary; preview: ActivityPreview; field_url: string; observations_url: string }
export interface ActivityResult { id: string; kind: ActivityKind; source_id: string | null; person_id: string | null; target_id: string | null; disposition: 'applied' | 'already_present' | 'held'; committed_at: string; reasons: string[]; source_summary: MetadataSummary; field_url: string }
export interface ActivityObservation { id: string; capture_id: string; record_id: string | null; stream: string; representation: string; negative: boolean; source_id: string | null; field_url: string; raw_url: string }
export interface ActivityFieldRequest { kind: 'records' | 'mappings' | 'results' | 'observations'; importId: string; rowId: string; planId?: string; fieldKey: string }
export interface ActivitySegment { text: string; offset: string; next_offset: string; total_utf8_bytes: string; next_cursor: string | null }
export interface ActivityFilters { kind?: string; disposition?: string; issue?: string }
const root = '/migrations/fub/activity-imports'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
function query(values: Record<string, string | number | undefined>) { const q = new URLSearchParams(); for (const [k, v] of Object.entries(values)) if (v !== undefined && v !== '') q.set(k, String(v)); return `?${q}` }
function post(url: string, body: unknown, signal?: AbortSignal) { return apiFetch<ActivityEnvelope>(url, { method: 'POST', body: JSON.stringify(body), signal }) }
export const fetchActivityImports = (parent?: string, cursor?: string, signal?: AbortSignal) => apiFetch<ActivityList>(root + query({ parent_import_id: parent, cursor, limit: 20 }), { signal })
export const fetchActivityImport = (id: string, signal?: AbortSignal) => apiFetch<ActivityImport>(path(id), { signal })
export const proposeActivityImport = (body: { request_id: string; parent_import_id: string }, signal?: AbortSignal) => post(root, body, signal)
export const replanActivityImport = (id: string, body: ActivityReplan, signal?: AbortSignal) => post(`${path(id)}/plans`, body, signal)
export const confirmActivityImport = (id: string, body: ActivityConfirm, signal?: AbortSignal) => post(`${path(id)}/confirm`, body, signal)
export const retryActivityImport = (id: string, body: ActivityAction, signal?: AbortSignal) => post(`${path(id)}/retry`, body, signal)
export const cancelActivityImport = (id: string, body: ActivityAction, signal?: AbortSignal) => post(`${path(id)}/cancel`, body, signal)
export const fetchActivityMappings = (id: string, plan: string, role?: ActivityRole, cursor?: string, signal?: AbortSignal) => apiFetch<ActivityPage<ActivityMapping>>(`${path(id)}/mappings${query({ plan_id: plan, kind: role, cursor, limit: 50 })}`, { signal })
export const fetchActivityTargets = (id: string, plan: string, cursor?: string, signal?: AbortSignal) => apiFetch<ActivityPage<ActivityTarget>>(`${path(id)}/targets${query({ plan_id: plan, cursor, limit: 50 })}`, { signal })
export const fetchActivityRecords = (id: string, plan: string, filters: ActivityFilters = {}, cursor?: string, signal?: AbortSignal) => apiFetch<ActivityPage<ActivityRecord>>(`${path(id)}/records${query({ plan_id: plan, ...filters, cursor, limit: 25 })}`, { signal })
export const fetchActivityResults = (id: string, plan: string, filters: ActivityFilters = {}, cursor?: string, signal?: AbortSignal) => apiFetch<ActivityPage<ActivityResult>>(`${path(id)}/results${query({ plan_id: plan, ...filters, cursor, limit: 25 })}`, { signal })
export const fetchActivityObservations = (id: string, plan: string, row: string, cursor?: string, signal?: AbortSignal) => apiFetch<ActivityPage<ActivityObservation>>(`${path(id)}/records/${encodeURIComponent(row)}/observations${query({ plan_id: plan, cursor, limit: 25 })}`, { signal })
export const fetchActivityField = (r: ActivityFieldRequest, cursor?: string, signal?: AbortSignal) => apiFetch<ActivitySegment>(`${path(r.importId)}/${r.kind}/${encodeURIComponent(r.rowId)}/fields/${encodeURIComponent(r.fieldKey)}${query({ plan_id: r.planId, cursor, limit: 65536 })}`, { signal })
export const useActivityAccess = () => useImportAccess('activity-imports')
export const activityActive = (r?: ActivityImport) => !!r && ['preparing', 'queued', 'running'].includes(r.state)
