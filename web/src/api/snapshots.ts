// 010b's additive contract. Counts, source IDs and byte values remain exact strings.
import { apiFetch } from './client'

export type SnapshotState = 'proposed' | 'queued' | 'running' | 'waiting_retry' | 'paused' | 'completed' | 'completed_with_gaps' | 'cancelled' | 'expired'
export type SnapshotFamily = 'people' | 'users' | 'stages' | 'custom_fields' | 'notes' | 'tasks'
export const snapshotFamilies: SnapshotFamily[] = ['people', 'users', 'stages', 'custom_fields', 'notes', 'tasks']
export interface CoreSnapshot {
  id: string; connection_id: string; connection_revision: number; source_account_id: string
  profile_version: string; state: SnapshotState; pause_reason: string | null
  created_at: string; started_at: string | null; completed_at: string | null; proposal_expires_at: string
  raw_bytes: string; retained_bytes: string; reserved_bytes: string; accepted_captures: string; capture_sequence: string
  original_run_byte_limit: string; run_byte_limit: string; org_byte_limit: string
  org_retained_bytes: string; org_reserved_bytes: string
  run_budget_revision: string; org_budget_revision: string; policy_revision: string
  run_budget_policy_revision: string; org_budget_policy_revision: string
  run_ceiling_bytes: string; org_ceiling_bytes: string; required_reservation_bytes: string
  actions: string[]; preview_ids: string[]
}
export interface SnapshotStream {
  stream: string; family: string; state: string; reported_total: string | null
  returned_items: string; distinct_ids: string; accepted_captures: string; content_gaps: string
  attempts: string; error_code: string | null; observed_at: string | null; first_observed_at: string | null
}
export interface SnapshotCoverage { family: string; state: string; reason: string; next_action: string }
export interface SnapshotList {
  snapshots: CoreSnapshot[]; next_cursor: string | null
  active_snapshot_id: string | null; latest_completed_snapshot_id: string | null
}
export interface SnapshotDetail { snapshot: CoreSnapshot; streams: SnapshotStream[]; coverage: SnapshotCoverage[] }
export interface SnapshotEnvelope { snapshot: CoreSnapshot }
export interface SnapshotProposal extends SnapshotEnvelope {
  proposal: { families: string[]; profile_version: string; expires_at: string; run_byte_limit: string; org_byte_limit: string; source_scope: string }
}
export interface SnapshotBudgetRequest {
  request_id: string; expected_run_budget_revision: string; expected_org_budget_revision: string
  expected_policy_revision: string; run_byte_limit: string; org_byte_limit: string
}
export interface SnapshotBudgetReceipt {
  approved_by_user_id: string; old_run_byte_limit: string; old_org_byte_limit: string
  run_byte_limit: string; org_byte_limit: string; old_run_budget_revision: string; run_budget_revision: string
  old_org_budget_revision: string; org_budget_revision: string
  old_run_policy_revision: string; old_org_policy_revision: string; policy_revision: string
}
export interface SnapshotPreview {
  id: string; snapshot_id: string; state: 'queued' | 'running' | 'paused' | 'completed' | 'failed'
  pause_reason: string | null; engine_version: string; capture_sequence: string
  created_at: string; completed_at: string | null; input_observed_at: string; actions: string[]
}
export interface SnapshotPreviewDetail {
  preview: SnapshotPreview; coverage: SnapshotCoverage[]; destination_stale: boolean
  first_import_requires_new_empty_organization: boolean; destination_has_people: boolean
  counts: {
    records: Array<{ family: string; disposition: string; count: string }>
    issues: Array<{ family: string; issue: string; count: string }>
    invalid_ids: Array<{ family: string; count: string }>; streams: SnapshotStream[]
  }
}
export interface SnapshotPreviewRecord {
  id: string; source_id: string; family: string; disposition: string; issues: string[]
  projection: unknown; overlap_group_count: string
  candidates: { member_id: string | null; stage_id: string | null; custom_field_id: string | null }
}
export interface SnapshotOverlapGroup { id: string; kind: string; member_count: string }
export interface SnapshotGroupMember { source_id: string; record_id: string }
export interface SnapshotRecordPage { records: SnapshotPreviewRecord[]; next_cursor: string | null }
export interface SnapshotGroupPage { groups: SnapshotOverlapGroup[]; next_cursor: string | null }
export interface SnapshotMemberPage { members: SnapshotGroupMember[]; next_cursor: string | null }
export interface SnapshotPreviewEnvelope { preview_id: string; state: SnapshotPreview['state'] }
const root = '/migrations/fub/snapshots'
const runPath = (id: string) => `${root}/${encodeURIComponent(id)}`
const previewPath = (run: string, id: string) => `${runPath(run)}/previews/${encodeURIComponent(id)}`
function post<T>(path: string, body: unknown) { return apiFetch<T>(path, { method: 'POST', body: JSON.stringify(body) }) }
function pageQuery(values: Record<string, string | number | undefined>) {
  const q = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') q.set(key, String(value))
  return `?${q}`
}
export function fetchSnapshots(cursor?: string, signal?: AbortSignal) {
  return apiFetch<SnapshotList>(root + pageQuery({ cursor, limit: 20 }), { signal })
}
export function fetchSnapshot(id: string, signal?: AbortSignal) { return apiFetch<SnapshotDetail>(runPath(id), { signal }) }
export function proposeSnapshot(connectionId: string, revision: number, requestId: string) {
  return post<SnapshotProposal>(root, { request_id: requestId, connection_id: connectionId, expected_revision: revision })
}
export function confirmSnapshot(id: string, requestId: string) { return post<SnapshotEnvelope>(`${runPath(id)}/confirm`, { request_id: requestId }) }
export function retrySnapshot(id: string, requestId: string) { return post<SnapshotEnvelope>(`${runPath(id)}/retry`, { request_id: requestId }) }
export function cancelSnapshot(id: string) { return post<SnapshotEnvelope>(`${runPath(id)}/cancel`, {}) }
export function increaseSnapshotBudget(id: string, body: SnapshotBudgetRequest) {
  return post<SnapshotEnvelope & { budget: SnapshotBudgetReceipt }>(`${runPath(id)}/budget`, body)
}
export function generateSnapshotPreview(id: string, requestId: string) {
  return post<SnapshotPreviewEnvelope>(`${runPath(id)}/previews`, { request_id: requestId })
}
export function retrySnapshotPreview(run: string, id: string, requestId: string) {
  return post<SnapshotPreviewEnvelope>(`${previewPath(run, id)}/retry`, { request_id: requestId })
}
export function fetchSnapshotPreview(run: string, id: string, signal?: AbortSignal) {
  return apiFetch<SnapshotPreviewDetail>(previewPath(run, id), { signal })
}
export function fetchSnapshotRecords(run: string, id: string, family: string, disposition?: string, cursor?: string, signal?: AbortSignal) {
  return apiFetch<SnapshotRecordPage>(`${previewPath(run, id)}/records${pageQuery({ family, disposition, cursor, limit: 50 })}`, { signal })
}
export function fetchSnapshotGroups(run: string, id: string, recordId?: string, cursor?: string, signal?: AbortSignal) {
  return apiFetch<SnapshotGroupPage>(`${previewPath(run, id)}/overlap-groups${pageQuery({ record_id: recordId, cursor, limit: 50 })}`, { signal })
}
export function fetchSnapshotGroupMembers(run: string, id: string, group: string, cursor?: string, signal?: AbortSignal) {
  return apiFetch<SnapshotMemberPage>(`${previewPath(run, id)}/overlap-groups/${encodeURIComponent(group)}/members${pageQuery({ cursor, limit: 50 })}`, { signal })
}
