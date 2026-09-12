import { computed, onBeforeUnmount, onMounted, toValue, type MaybeRefOrGetter } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import { apiFetch } from './client'
import { useImportAccess } from './imports'
import type { ActivityReviewCore } from './activityReview'
import type { UserRef } from './types'

export const historyKinds = ['person_imported', 'inquiry_received', 'routing_decision', 'assignment_changed', 'stage_changed', 'contact_attempted', 'call_completed', 'correspondence', 'fub_event_record_imported', 'fub_call_record_imported', 'fub_text_record_imported'] as const
export type TimelineKind = typeof historyKinds[number]
export type TimelineFamily = 'all' | 'native' | 'events' | 'calls' | 'text_messages'
export type TimelineDates = 'known' | 'unknown'
export interface HistoryReviewCore extends Omit<ActivityReviewCore, 'person' | 'inquiries' | 'core_history'> {
  person: Omit<ActivityReviewCore['person'], 'inquiry_count'> & { inquiry_count: string }
  inquiries: { count: string; url: string }
  history: { read_revision: string; known_count: string; unknown_count: string; counts: Record<TimelineKind, string>; timeline_url: string }
}
export interface HistoryReviewPage<T> { items: T[]; next_cursor: string | null; read_revision: string }
export interface HistoryInquiry { id: string; source: string; source_external_id: string | null; received_at: string }
export interface TimelineEntry {
  kind: TimelineKind; id: string; display_at: string | null; occurred_at: string; recorded_at: string
  actor: UserRef | null; origin: string; detail_url: string; metadata: Record<string, unknown>
}
export interface TimelineDetail extends TimelineEntry {
  read_revision: string
  correlation_id: string
  provenance: { plan_id: string; attempt_id: string; manifest_id: string; identity_id: string; source_time_basis: 'fub_record_created' | 'unknown'; stable_position: string } | null
}
export interface TimelineFilters { family?: TimelineFamily; dated?: TimelineDates; limit?: number }
const root = (person: string) => `/people/${encodeURIComponent(person)}/migration-review`
function pageQuery(cursor: string | undefined, limit: number, filters: Record<string, string> = {}) {
  const query = new URLSearchParams({ ...filters, limit: String(limit) })
  if (cursor) query.set('cursor', cursor)
  return `?${query}`
}
export const fetchHistoryReviewCore = (person: string, signal?: AbortSignal) => apiFetch<HistoryReviewCore>(`${root(person)}/v2`, { signal, cache: 'no-store' })
export const fetchHistoryInquiries = (person: string, cursor?: string, signal?: AbortSignal, limit = 25) => apiFetch<HistoryReviewPage<HistoryInquiry>>(`${root(person)}/inquiries${pageQuery(cursor, limit)}`, { signal, cache: 'no-store' })
export const fetchHistoryTimeline = (person: string, filters: TimelineFilters = {}, cursor?: string, signal?: AbortSignal) => apiFetch<HistoryReviewPage<TimelineEntry>>(`${root(person)}/timeline${pageQuery(cursor, filters.limit ?? 25, { family: filters.family ?? 'all', dated: filters.dated ?? 'known' })}`, { signal, cache: 'no-store' })
export const fetchHistoryTimelineDetail = (person: string, kind: TimelineKind, id: string, signal?: AbortSignal) => apiFetch<TimelineDetail>(`${root(person)}/timeline/${encodeURIComponent(kind)}/${encodeURIComponent(id)}`, { signal, cache: 'no-store' })
export function historyKindLabel(kind: TimelineKind): string {
  const labels: Record<TimelineKind, string> = {
    person_imported: 'Person imported', inquiry_received: 'Inquiry received', routing_decision: 'Inquiry routed', assignment_changed: 'Assignment changed', stage_changed: 'Stage changed', contact_attempted: 'Contact attempt recorded', call_completed: 'Call completed', correspondence: 'Correspondence captured', fub_event_record_imported: 'FUB event record', fub_call_record_imported: 'FUB call record', fub_text_record_imported: 'FUB text record',
  }
  return labels[kind]
}
export const externalHistoryEntry = (entry: TimelineEntry) => entry.kind.startsWith('fub_')
export function useHistoryReviewAccess() {
  const access = useImportAccess('history-review')
  const enabled = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review')
  const prefix = computed(() => [...access.prefix.value, access.org.value?.role ?? ''])
  return { ...access, enabled, prefix }
}
export const historyReviewCoreKey = (prefix: readonly unknown[], person: string) => [...prefix, 'person', person, 'core-v2'] as const
export function useHistoryReviewCore(person: MaybeRefOrGetter<string>, active: MaybeRefOrGetter<boolean> = true) {
  const access = useHistoryReviewAccess()
  const key = computed(() => historyReviewCoreKey(access.prefix.value, toValue(person)))
  const enabled = computed(() => access.enabled.value && toValue(active) && !!toValue(person))
  const reading = useQuery({
    queryKey: key, enabled, retry: false, gcTime: 0,
    refetchInterval: false, refetchOnWindowFocus: false, refetchOnReconnect: false,
    queryFn: ({ signal }) => access.read(key.value, () => key.value, () => fetchHistoryReviewCore(toValue(person), signal)),
  })
  function refreshCurrent() { if (enabled.value) void reading.refetch() }
  onMounted(() => { window.addEventListener('focus', refreshCurrent); window.addEventListener('online', refreshCurrent) })
  onBeforeUnmount(() => { window.removeEventListener('focus', refreshCurrent); window.removeEventListener('online', refreshCurrent) })
  const data = computed(() => enabled.value ? reading.data.value : undefined)
  return { access, key, enabled, reading, data }
}
