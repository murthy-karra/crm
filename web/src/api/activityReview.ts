import { computed, onBeforeUnmount, onMounted, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { apiFetch } from './client'
import { useImportAccess } from './imports'
import type { HistoryEntry, PersonDetailResponse, TaskKind, UserRef } from './types'

export interface ReviewProvenance {
  activity_import_id: string; result_id: string; source_account_id: string; source_id: string; source_url: string
}
export type ReviewCoreHistory = Exclude<HistoryEntry, { kind: 'note' | 'task_completed' }>
export function reviewHistoryTitle(kind: ReviewCoreHistory['kind']) {
  if (kind === 'person_imported') return 'Person imported from Follow Up Boss'
  if (kind === 'inquiry_received') return 'Inquiry received'
  if (kind === 'assignment_changed') return 'Assignment changed'
  if (kind === 'stage_changed') return 'Stage changed'
  if (kind === 'routing_decision') return 'Inquiry routed'
  return 'Activity'
}
export function reviewHistoryDetail(entry: ReviewCoreHistory) {
  switch (entry.kind) {
    case 'person_imported': return 'Original source and import result are available below.'
    case 'stage_changed': return `${entry.detail.from_stage?.name ?? 'New Person'} → ${entry.detail.to_stage.name} · ${entry.detail.reason === 'manual' ? 'Manual change' : 'Intake'}`
    case 'assignment_changed': return `${entry.detail.from?.display_name ?? 'Unassigned'} → ${entry.detail.to?.display_name ?? 'Unassigned'} · ${entry.detail.reason === 'manual' ? 'Manual change' : 'Intake'}`
    case 'inquiry_received': return `${entry.detail.source} · ${entry.detail.person_created ? 'Person created' : 'Existing Person'}`
    case 'routing_decision': return `${entry.detail.assignee?.display_name ?? 'Unassigned'} · ${entry.detail.strategy.replaceAll('_', ' ')}`
    default: return ''
  }
}
export interface ActivityReviewCore extends Omit<PersonDetailResponse, 'history' | 'tasks'> {
  core_history: ReviewCoreHistory[]
  activity: {
    notes_count: string; open_tasks_count: string; completed_tasks_count: string; activity_revision: string
    notes_url: string; tasks_url: string
  }
}
export interface ReviewNote {
  id: string; created_at: string; updated_at: string; author: UserRef | null
  can_manage: false; provenance: ReviewProvenance | null; excerpt: string; has_more: boolean
}
export interface ReviewFullNote extends Omit<ReviewNote, 'excerpt' | 'has_more'> { body: string }
export interface ReviewTask {
  id: string; title: string; kind: TaskKind; due_at: string | null; completed_at: string | null
  created_at: string; updated_at: string; assignee: UserRef | null; created_by: UserRef | null
  completed_by: UserRef | null; can_manage: false; provenance: ReviewProvenance | null
}
export interface ReviewPage<T> { items: T[]; next_cursor: string | null; activity_revision: string }
export type ReviewTaskState = 'open' | 'completed'
const path = (person: string) => `/people/${encodeURIComponent(person)}/migration-review`
function pageQuery(cursor?: string, state?: ReviewTaskState) {
  const params = new URLSearchParams({ limit: '50' })
  if (state) params.set('state', state)
  if (cursor) params.set('cursor', cursor)
  return `?${params}`
}
export const fetchActivityReviewCore = (person: string, signal?: AbortSignal) => apiFetch<ActivityReviewCore>(path(person), { signal, cache: 'no-store' })
export const fetchReviewNotes = (person: string, cursor?: string, signal?: AbortSignal) => apiFetch<ReviewPage<ReviewNote>>(`${path(person)}/notes${pageQuery(cursor)}`, { signal, cache: 'no-store' })
export const fetchReviewTasks = (person: string, state: ReviewTaskState, cursor?: string, signal?: AbortSignal) => apiFetch<ReviewPage<ReviewTask>>(`${path(person)}/tasks${pageQuery(cursor, state)}`, { signal, cache: 'no-store' })
export const fetchReviewNote = (person: string, note: string, signal?: AbortSignal) => apiFetch<ReviewFullNote>(`${path(person)}/notes/${encodeURIComponent(note)}`, { signal, cache: 'no-store' })

export function useActivityReviewAccess() {
  const access = useImportAccess('activity-review')
  const enabled = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review')
  // Include role as well as actor, session lifetime and workspace revision.
  const prefix = computed(() => [...access.prefix.value, access.org.value?.role ?? ''])
  return { ...access, enabled, prefix }
}
export function activityReviewCoreKey(prefix: readonly unknown[], person: string) { return [...prefix, 'person', person, 'core'] as const }
export function useActivityReviewCore(person: MaybeRefOrGetter<string>, active: MaybeRefOrGetter<boolean> = true) {
  const access = useActivityReviewAccess()
  const key = computed(() => activityReviewCoreKey(access.prefix.value, toValue(person)))
  const enabled = computed(() => access.enabled.value && toValue(active) && !!toValue(person))
  const reading = useQuery({
    queryKey: key, enabled, retry: false, gcTime: 30_000,
    refetchInterval: false, refetchOnWindowFocus: false, refetchOnReconnect: false,
    queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchActivityReviewCore(toValue(person), signal)),
  })
  // Core has no child state: refresh on explicit focus/reconnect, without
  // continuously polling paused, terminal or pre-child workspaces.
  function refreshCurrent() { if (enabled.value) void reading.refetch() }
  onMounted(() => { window.addEventListener('focus', refreshCurrent); window.addEventListener('online', refreshCurrent) })
  onBeforeUnmount(() => { window.removeEventListener('focus', refreshCurrent); window.removeEventListener('online', refreshCurrent) })
  const data = computed(() => enabled.value ? reading.data.value : undefined)
  return { access, key, enabled, reading, data }
}
