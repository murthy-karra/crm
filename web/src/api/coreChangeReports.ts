import { apiFetch } from './client'
import { useImportAccess } from './imports'

export const coreChangeFamilies = ['people', 'users', 'stages', 'custom_fields', 'notes', 'tasks'] as const
export const coreChangeDispositions = ['unchanged', 'changed', 'newly_observed', 'not_seen_again', 'unresolved'] as const
export type CoreChangeFamily = typeof coreChangeFamilies[number]
export type CoreChangeDisposition = typeof coreChangeDispositions[number]
export type CoreChangeState = 'queued' | 'running' | 'paused' | 'completed' | 'cancelled'
export interface CoreChangeBoundary {
  snapshot_id: string; capture_sequence: string; profile_version: string; schema_version: string
  started_at: string; completed_at: string
  streams: { stream: string; state: string; reported_total: string | null; returned_items: string; content_gaps: string; accepted_captures: string }[]
}
export interface CoreChangeInputs {
  baseline: CoreChangeBoundary; newer: CoreChangeBoundary; source_account_id: string; workspace_revision: string
  source_scope: 'consistent_identity' | 'changed_identity' | 'unknown_identity'; warnings: string[]
}
export interface CoreChangeReport {
  id: string; parent_import_id: string; engine_version: 'fub-core-change-v1'; state: CoreChangeState
  phase: 'capture' | 'compare' | 'sealed'; created_at: string; updated_at: string; completed_at: string | null
  pause_reason: string | null; output_revision: string | null; inputs: CoreChangeInputs
  counts: null | { families: Record<CoreChangeFamily, Record<CoreChangeDisposition, string>>; source_ids: string; invalid_observations: string; observations: string; equal_repeats: string; conflicting_groups: string }
  progress: { captures_processed: string; observations_processed: string; groups_compared: string }
  retained_bytes: string; reserved_bytes: string; actions: { resume: boolean; cancel: boolean }
}
export interface CoreChangeRow {
  id: string; family: CoreChangeFamily; source_id: string | null; disposition: CoreChangeDisposition
  categories: string[]; reasons: string[]; baseline_observations: string; newer_observations: string
  components: { representation: string; disposition: CoreChangeDisposition; baseline_observations: string; newer_observations: string }[]
  evidence: { snapshot_id: string; capture_id: string; ordinal: number; side: 'baseline' | 'newer'; stream: string }[]
  evidence_is_exhaustive: boolean
}
export interface CoreChangeReceipt { report_id: string; state: CoreChangeState; inputs: CoreChangeInputs }
export interface CoreChangeRequest { request_id: string; parent_import_id: string; newer_snapshot_id: string }
export interface CoreChangeList { reports: CoreChangeReport[]; next_cursor: string | null }
export interface CoreChangeRows { rows: CoreChangeRow[]; next_cursor: string | null; output_revision: string }
export interface CoreChangeRowDetail { row: CoreChangeRow; output_revision: string }

const root = '/migrations/fub/core-change-reports'
const path = (id: string) => `${root}/${encodeURIComponent(id)}`
function query(values: Record<string, string | number | undefined>) {
  const params = new URLSearchParams()
  for (const [key, value] of Object.entries(values)) if (value !== undefined && value !== '') params.set(key, String(value))
  return `?${params}`
}
function post<T>(url: string, body: unknown, signal?: AbortSignal) { return apiFetch<T>(url, { method: 'POST', body: JSON.stringify(body), signal }) }
export const createCoreChangeReport = (body: CoreChangeRequest, signal?: AbortSignal) => post<CoreChangeReceipt>(root, body, signal)
export const fetchCoreChangeReports = (parentId: string, cursor?: string, signal?: AbortSignal) => apiFetch<CoreChangeList>(root + query({ parent_import_id: parentId, cursor, limit: 20 }), { signal })
export const fetchCoreChangeReport = (id: string, signal?: AbortSignal) => apiFetch<CoreChangeReport>(path(id), { signal })
export const resumeCoreChangeReport = (id: string, requestId: string, signal?: AbortSignal) => post<CoreChangeReceipt>(`${path(id)}/resume`, { request_id: requestId }, signal)
export const cancelCoreChangeReport = (id: string, requestId: string, signal?: AbortSignal) => post<CoreChangeReceipt>(`${path(id)}/cancel`, { request_id: requestId }, signal)
export const fetchCoreChangeRows = (id: string, filter: { family?: CoreChangeFamily; disposition?: CoreChangeDisposition }, cursor?: string, signal?: AbortSignal) => apiFetch<CoreChangeRows>(`${path(id)}/rows${query({ ...filter, cursor, limit: 50 })}`, { signal })
export const fetchCoreChangeRow = (id: string, rowId: string, signal?: AbortSignal) => apiFetch<CoreChangeRowDetail>(`${path(id)}/rows/${encodeURIComponent(rowId)}`, { signal })
export const useCoreChangeAccess = () => useImportAccess('core-change-reports')
export const coreChangeActive = (value?: Pick<CoreChangeReport, 'state'>) => value?.state === 'queued' || value?.state === 'running'

// Values from retained evidence never become arbitrary labels or property paths.
// Unknown server codes stay visible as uncertainty without echoing their value.
const labels: Record<string, string> = {
  people: 'People', users: 'Users', stages: 'Stages', custom_fields: 'Custom fields', notes: 'Notes', tasks: 'Tasks',
  tasks_open: 'Open tasks', tasks_completed: 'Completed tasks', note_detail: 'Note detail', identity: 'Identity',
  unchanged: 'Unchanged', changed: 'Changed', newly_observed: 'Newly observed', not_seen_again: 'Not seen again', unresolved: 'Unresolved',
  queued: 'Queued', running: 'Running', paused: 'Paused', completed: 'Completed', cancelled: 'Cancelled', pending: 'Pending',
  completed_with_gaps: 'Completed with gaps', inaccessible: 'Inaccessible', failed: 'Failed', waiting_retry: 'Waiting to retry',
  capture: 'Reading retained captures', compare: 'Comparing observations', sealed: 'Results sealed',
  contact_information: 'Contact information', assignment_stage: 'Assignment or stage', embedded_tags: 'Embedded tags',
  note_content: 'Note content', task_status: 'Task status', task_due_date: 'Task due date', other_properties: 'Other preserved properties',
  invalid_source_id: 'Invalid or missing source ID', unsupported_record_shape: 'Unsupported record shape',
  note_content_inaccessible: 'Note content inaccessible', conflicting_observations: 'Conflicting observations',
  incomplete_enumeration: 'Incomplete source enumeration', effective_access_not_proven: 'Effective source access is not proven',
  source_scope_changed_or_unknown: 'Source access changed or is unknown', missing_note_detail: 'Note detail was not available',
  source_identity_changed: 'Source user differs between captures', source_identity_unknown: 'Source identity evidence is incomplete',
  source_access_unknown: 'Effective source access is unknown', storage_limit: 'Storage allowance reached',
  budget_exhausted: 'Storage allowance reached', release_not_ready: 'Compatible release required',
  actor_inactive: 'Initiating administrator is no longer active', actor_not_admin: 'Initiating administrator access changed',
  evidence_unavailable: 'Retained evidence is unavailable', key_unavailable: 'Evidence key is unavailable', corrupt_evidence: 'Retained evidence could not be verified',
  unsupported_capture: 'Unsupported retained capture', unsupported_representation: 'Unsupported source representation',
  source_identity: 'Source identity', person: 'Person', user: 'User', stage: 'Stage', custom_field: 'Custom field',
  retained_integrity_failed: 'Retained evidence could not be verified', storage_budget_exhausted: 'Storage allowance reached',
  executor_not_authorized: 'Initiating administrator access changed', source_binding_changed: 'The retained input binding changed',
  interpretation_bound_exceeded: 'Evidence exceeds the supported interpretation bound',
  not_atomic_snapshots: 'Captures cover intervals, not atomic source snapshots', absence_is_not_deletion: 'Absence does not establish deletion',
  report_is_not_cutover_readiness: 'Completing this report does not establish cutover readiness',
  missing_comparable_component: 'A comparable source representation is missing', missing_note_component: 'A note list or detail component is missing',
  first_observation_is_not_creation: 'First observation does not establish creation', rejected_capture: 'Retained capture could not be qualified',
  representation_mismatch: 'Source representations are incompatible',
  'fub-core-v1/users/allFields,calling': 'User observations', 'fub-core-v1/stages/default': 'Stage observations',
  'fub-core-v1/customFields/default': 'Custom field observations', 'fub-core-v1/people/allFields': 'Person observations',
  'fub-core-v1/notes/list': 'Note list observations', 'fub-core-v1/notes/detail/replies,reactions': 'Enriched note detail observations',
  'fub-core-v1/tasks/default': 'Task observations across open and completed partitions',
}
export function coreChangeLabel(value: string) { return Object.hasOwn(labels, value) ? labels[value]! : 'Unrecognized report detail' }
export function coreChangeMeaning(value: CoreChangeDisposition) {
  return {
    unchanged: 'Qualified comparable observations match.', changed: 'Qualified comparable observations differ.',
    newly_observed: 'Observed only in the newer capture; creation time is not inferred.',
    not_seen_again: 'Absent from the newer enumeration; this does not establish deletion.',
    unresolved: 'Incomplete, inaccessible, invalid or conflicting evidence prevents a conclusion.',
  }[value]
}
