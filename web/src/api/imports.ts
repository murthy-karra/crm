import { computed, onScopeDispose, ref, watch } from 'vue'
import { useQueryClient } from '@tanstack/vue-query'
import { ApiError, apiFetch } from './client'
import { useAuthSessionLifetime, useMe } from './queries'
import { useSessionVerificationPending } from '../sessionLifecycle'
import { useWorkspacePending } from '../workspaceLifecycle'
import type { MembersResponse, StagesResponse } from './types'

export type ImportState = 'proposed' | 'queued' | 'running' | 'paused' | 'completed' | 'cancelled' | 'expired'
export type ImportPlanState = 'building' | 'ready' | 'paused' | 'failed' | 'superseded'
export type StageChoice = { kind: 'existing'; stage_id: string } | { kind: 'create' } | { kind: 'hold' }
export type AssigneeChoice = { kind: 'member'; user_id: string } | { kind: 'unassigned' } | { kind: 'hold' }
export interface StagePatch { source_key: string; choice: StageChoice }
export interface AssigneePatch { source_key: string; choice: AssigneeChoice }
export interface ImportPatches { stage_mappings: StagePatch[]; assignee_mappings: AssigneePatch[] }
export interface ImportCounts {
  source_people: string; eligible_people: string; held_people: string; invalid_ids: string; contacts: string
  overlap_people: string; stages_to_create: string; assigned_people: string; unassigned_people: string
  imported_people: string; imported_contacts: string; pending_people: string; reasons: Record<string, string>
}
export interface ImportPlan {
  id: string; revision: string; state: ImportPlanState; phase: string; confirmation_digest: string | null
  expires_at: string | null; expired: boolean; counts: ImportCounts; required_reservation_bytes: string
  created_at: string; completed_at: string | null
}
export interface PeopleImport {
  id: string; snapshot_id: string; preview_id: string; source_account_id: string; capture_sequence: string
  state: ImportState; phase: string; pause_reason: string | null; latest_plan_id: string; confirmed_plan_id: string | null
  executor_user_id: string; created_at: string; updated_at: string; counts: ImportCounts
  retained_bytes: string; reserved_bytes: string; cancellation_reserved_bytes: string
  actions: { confirm: boolean; replan: boolean; retry: boolean; cancel: boolean }; plan: ImportPlan
  workspace: { mode: 'operational' | 'migration_review'; revision: string; activation_available: false }
  coverage: { imported_families: string[]; remaining_families: string[]; source_remains_retained: boolean; cutover_complete: false }
  source_window: { first_observed_at: string | null; last_observed_at: string | null }
  policy: { run_byte_limit: string; org_byte_limit: string; run_ceiling_bytes: string; org_ceiling_bytes: string; unit_ceiling_bytes: string; policy_revision: string }
  engine_version: string
}
export interface PeopleImportRead extends PeopleImport { release_ready: boolean }
export interface ImportList { imports: PeopleImportRead[]; next_cursor: string | null }
export interface ImportProposal { import_id: string; plan_id: string; state: 'building' }
export interface ImportEnvelope { import: PeopleImport }
export interface ImportConfirmRequest {
  request_id: string; plan_id: string; plan_revision: string; confirmation_digest: string
  acknowledgments: { held_count: string; review_only: true; remaining_data: true }
}
export interface ImportConfirmReceipt extends ImportEnvelope {
  receipt: { request_id: string; plan_id: string; plan_revision: string; confirmed_by_user_id: string; held_count: string }
  workspace_mode: 'migration_review'; workspace_revision: string
}
export interface ImportDisplayField { value: string; abbreviated: boolean; full_utf8_bytes: string; field_key: string }
export interface ImportProvenanceField extends ImportDisplayField { label: string; label_abbreviated: boolean; label_full_utf8_bytes: string }
export interface ImportSourceDisplay {
  family: 'people' | 'stage' | 'user' | 'invalid'; first_name?: ImportDisplayField | null; last_name?: ImportDisplayField | null
  stage_label?: ImportDisplayField | null; assignee_key?: string | null; label?: ImportDisplayField | null
  name?: ImportDisplayField | null; email?: ImportDisplayField | null; can_create?: boolean; is_pond?: boolean
  contacts?: { entries: Array<{ kind: string; value: string; normalized_value: string; import_order: number }>; total_count: string; abbreviated: boolean; field_key: string }
  provenance: { fields: ImportProvenanceField[]; total_count: string; abbreviated: boolean; field_key: string }
}
export interface ImportMapping {
  id: string; source_key: string; qualified: boolean; disposition: string; source: ImportSourceDisplay | null
  choice: StageChoice | AssigneeChoice; suggestions: Array<{ id: string; name: string; email?: string; position?: number }>
  target: { id: string; name: string; email?: string; position?: number } | null; reasons: string[]; dependent_count: string
}
export interface ImportRecord {
  id: string; source_id: string; disposition: 'eligible' | 'held'; held_reasons: string[]; transformations: string[]
  proposed: ImportSourceDisplay; overlap_count: string; added_byte_bound: string
}
export interface ImportResult {
  id: string; source_id: string; disposition: 'imported' | 'already_imported' | 'held' | 'pending'
  planned_disposition: 'eligible' | 'held'; person_id: string | null; contact_count: string | null; committed_at: string | null; held_reasons: string[]
}
export interface ImportRecordPage { records: ImportRecord[]; next_cursor: string | null }
export interface ImportMappingPage { mappings: ImportMapping[]; next_cursor: string | null }
export interface ImportResultPage { results: ImportResult[]; next_cursor: string | null }
export interface ImportFieldSegment { text: string; offset: string; full_utf8_bytes: string; next_cursor: string | null }
export interface PersonProvenance {
  person_id: string; import_id: string; plan_id: string; snapshot_id: string; source_record_id: string; capture_id: string
  source_id: string; committed_at: string; source: ImportSourceDisplay; operational_use_available: false
}
export type ImportFieldRequest =
  | { kind: 'record'; importId: string; planId: string; recordId: string; fieldKey: string }
  | { kind: 'mapping'; importId: string; planId: string; mappingId: string; fieldKey: string }
  | { kind: 'provenance'; personId: string; fieldKey: string }

const root = '/migrations/fub/imports'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
const planPath = (id: string, plan: string) => `${path(id)}/plans/${encodeURIComponent(plan)}`
function query(values: Record<string, string | number | undefined>) {
  const q = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') q.set(key, String(value))
  return `?${q}`
}
function post<T>(url: string, body: unknown, signal?: AbortSignal) { return apiFetch<T>(url, { method: 'POST', body: JSON.stringify(body), signal }) }
export const fetchImports = (cursor?: string, signal?: AbortSignal) => apiFetch<ImportList>(root + query({ cursor, limit: 20 }), { signal })
export const fetchImport = (id: string, signal?: AbortSignal) => apiFetch<PeopleImportRead>(path(id), { signal })
export const proposeImport = (body: { request_id: string; snapshot_id: string; preview_id: string }, signal?: AbortSignal) => post<ImportProposal>(root, body, signal)
export const replanImport = (id: string, body: ImportPatches & { request_id: string; expected_plan_revision: string }, signal?: AbortSignal) => post<ImportProposal>(`${path(id)}/plans`, body, signal)
export const confirmImport = (id: string, body: ImportConfirmRequest, signal?: AbortSignal) => post<ImportConfirmReceipt>(`${path(id)}/confirm`, body, signal)
export const retryImport = (id: string, requestId: string, signal?: AbortSignal) => post<ImportEnvelope>(`${path(id)}/retry`, { request_id: requestId }, signal)
export const cancelImport = (id: string, requestId: string, signal?: AbortSignal) => post<ImportEnvelope>(`${path(id)}/cancel`, { request_id: requestId }, signal)
export const fetchImportRecords = (id: string, plan: string, disposition?: 'eligible' | 'held', cursor?: string, signal?: AbortSignal) => apiFetch<ImportRecordPage>(`${planPath(id, plan)}/records${query({ disposition, cursor, limit: 50 })}`, { signal })
export const fetchImportMappings = (id: string, plan: string, kind: 'stage' | 'assignee', cursor?: string, signal?: AbortSignal) => apiFetch<ImportMappingPage>(`${planPath(id, plan)}/mappings${query({ kind, cursor, limit: 50 })}`, { signal })
export const fetchImportResults = (id: string, disposition?: ImportResult['disposition'], cursor?: string, signal?: AbortSignal) => apiFetch<ImportResultPage>(`${path(id)}/results${query({ disposition, cursor, limit: 50 })}`, { signal })
export const fetchPersonProvenance = (id: string, signal?: AbortSignal) => apiFetch<PersonProvenance>(`/people/${encodeURIComponent(id)}/import-provenance`, { signal })
export function fetchImportField(request: ImportFieldRequest, cursor?: string, signal?: AbortSignal) {
  const base = request.kind === 'provenance' ? `/people/${encodeURIComponent(request.personId)}/import-provenance`
    : request.kind === 'mapping' ? `${planPath(request.importId, request.planId)}/mappings/${encodeURIComponent(request.mappingId)}`
      : `${planPath(request.importId, request.planId)}/records/${encodeURIComponent(request.recordId)}`
  return apiFetch<ImportFieldSegment>(`${base}/fields/${encodeURIComponent(request.fieldKey)}${query({ cursor, limit: 65536 })}`, { signal })
}
export const fetchImportStages = (signal?: AbortSignal) => apiFetch<StagesResponse>('/stages', { signal })
export const fetchImportMembers = (signal?: AbortSignal) => apiFetch<MembersResponse>('/organization/members', { signal })
export const importQueryKeys = (org: string, actor: string, lifetime: number, revision: string) => ['org', org, 'people-imports', actor, lifetime, revision] as const
export function importActive(value?: PeopleImport) { return !!value && (['queued', 'running'].includes(value.state) || value.state === 'proposed' && value.plan.state === 'building') }
export function sourceName(source: ImportSourceDisplay | null) {
  if (!source) return 'Source value not available'
  return [source.first_name?.value, source.last_name?.value].filter(Boolean).join(' ') || source.label?.value || source.name?.value || source.email?.value || 'Unnamed source record'
}
export function uncertainImportError(error: unknown) { return !(error instanceof ApiError) || error.status === 0 || error.status >= 500 }
export function importAccessError(error: unknown) { return error instanceof ApiError && ([401, 403].includes(error.status) || error.code === 'workspace_in_migration_review') }

// Local lifecycle shared only by these new import screens. The application-wide
// authority transition remains owned by the existing client/session coordinator.
export function useImportAccess(namespace: 'people-imports' | 'metadata-imports' | 'activity-imports' | 'activity-review' | 'history-captures' | 'history-imports' | 'history-review' | 'core-change-reports' = 'people-imports') {
  const { data: me } = useMe()
  const lifetime = useAuthSessionLifetime()
  const verifying = useSessionVerificationPending()
  const workspacePending = useWorkspacePending()
  const client = useQueryClient()
  const denied = ref(false)
  const org = computed(() => me.value?.organization)
  const identity = computed(() => JSON.stringify([org.value?.id, me.value?.user.id, lifetime.value, org.value?.role]))
  const prefix = computed(() => ['org', org.value?.id ?? '', namespace, me.value?.user.id ?? '', lifetime.value, org.value?.workspace_revision ?? ''] as const)
  const scope = computed(() => JSON.stringify([identity.value, org.value?.workspace_mode, org.value?.workspace_revision, verifying.value, workspacePending.value]))
  const enabled = computed(() => !denied.value && !verifying.value && !workspacePending.value && !!org.value?.id && org.value.role === 'admin' && !!org.value.workspace_revision)
  let disposed = false
  let generation = 0
  function remove(key: readonly unknown[]) { void client.cancelQueries({ queryKey: key }); client.removeQueries({ queryKey: key }) }
  watch(scope, (_, previous) => { generation++; if (previous) denied.value = false }, { flush: 'sync' })
  watch(prefix, (next, old) => { if (JSON.stringify(next) !== JSON.stringify(old)) remove(old) }, { flush: 'sync' })
  watch(enabled, (value) => { if (!value) remove(prefix.value) }, { flush: 'sync' })
  onScopeDispose(() => { disposed = true })
  async function read<T>(key: readonly unknown[], currentKey: () => readonly unknown[], request: () => Promise<T>): Promise<T> {
    const expected = scope.value; const epoch = generation; const tag = JSON.stringify(key)
    const current = () => !disposed && epoch === generation && enabled.value && expected === scope.value && tag === JSON.stringify(currentKey())
    try {
      const result = await request()
      if (!current()) throw new Error('Discarded stale import response')
      return result
    } catch (error) {
      if (current() && importAccessError(error)) { denied.value = true; remove(prefix.value) }
      throw error
    }
  }
  return { me, org, lifetime, verifying, client, identity, prefix, scope, enabled, denied, read, remove }
}
