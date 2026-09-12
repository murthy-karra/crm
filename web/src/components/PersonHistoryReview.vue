<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, useId, watch } from 'vue'
import Dialog from 'primevue/dialog'
import Card from './Card.vue'
import FormField from './FormField.vue'
import HistoryTimelineEntry from './migration/HistoryTimelineEntry.vue'
import { ApiError } from '../api/client'
import { fetchHistoryInquiries, fetchHistoryTimeline, fetchHistoryTimelineDetail, useHistoryReviewAccess, type HistoryInquiry, type HistoryReviewCore, type HistoryReviewPage, type TimelineDates, type TimelineDetail, type TimelineEntry, type TimelineFamily } from '../api/historyReview'
import { buttonClasses, dialogPt, INPUT_CLASSES } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { formatAbsoluteTime } from '../lib/format'
const props = defineProps<{ personId: string; core: HistoryReviewCore; refreshCore: () => Promise<unknown>; compact?: boolean }>()
const access = useHistoryReviewAccess()
const family = ref<TimelineFamily>('all')
const dated = ref<TimelineDates>('known')
interface PageState<T> { page: HistoryReviewPage<T> | null; cursors: string[]; loading: boolean; error: unknown; request: number }
const timeline = reactive<PageState<TimelineEntry>>({ page: null, cursors: [''], loading: false, error: null, request: 0 })
const inquiries = reactive<PageState<HistoryInquiry>>({ page: null, cursors: [''], loading: false, error: null, request: 0 })
const needsRefresh = ref(false)
const refreshing = ref(false)
const refreshError = ref<unknown>(null)
const selected = ref<TimelineEntry | null>(null)
const detail = ref<TimelineDetail | null>(null)
const detailLoading = ref(false)
const detailError = ref<unknown>(null)
const titleId = useId()
let generation = 0
let detailGeneration = 0
let refreshGeneration = 0
let disposed = false
const requests = new Set<AbortController>()
const scope = computed(() => JSON.stringify([access.scope.value, access.enabled.value, props.personId]))
const revision = computed(() => props.core.history.read_revision)
function closeDetail() { detailGeneration++; selected.value = null; detail.value = null; detailError.value = null; detailLoading.value = false }
function reset() {
  generation++
  for (const request of requests) request.abort()
  requests.clear()
  for (const state of [timeline, inquiries]) { state.page = null; state.cursors = ['']; state.loading = false; state.error = null; state.request++ }
  closeDetail()
}
function stale() { reset(); needsRefresh.value = true }
const current = (epoch: number) => !disposed && generation === epoch && access.enabled.value && !needsRefresh.value
async function load(section: 'timeline' | 'inquiries') {
  if (!access.enabled.value || refreshing.value || needsRefresh.value) return
  const state = section === 'timeline' ? timeline : inquiries
  if (state.loading) return
  const epoch = generation; const requestId = ++state.request; const expectedRevision = revision.value
  const cursor = state.cursors.at(-1) ?? ''
  state.loading = true; state.error = null; state.page = null
  const controller = new AbortController(); requests.add(controller)
  const key = [...access.prefix.value, props.personId, expectedRevision, section, family.value, dated.value, cursor, epoch, requestId]
  const currentKey = () => [...access.prefix.value, props.personId, revision.value, section, family.value, dated.value, state.cursors.at(-1) ?? '', generation, state.request]
  try {
    const page = await access.read<HistoryReviewPage<TimelineEntry> | HistoryReviewPage<HistoryInquiry>>(key, currentKey, () => section === 'timeline' ? fetchHistoryTimeline(props.personId, { family: family.value, dated: dated.value, limit: props.compact ? 3 : 25 }, cursor || undefined, controller.signal) : fetchHistoryInquiries(props.personId, cursor || undefined, controller.signal, props.compact ? 1 : 25))
    if (!current(epoch) || state.request !== requestId) return
    if (page.read_revision !== expectedRevision) { stale(); return }
    if (section === 'timeline') timeline.page = page as HistoryReviewPage<TimelineEntry>
    else inquiries.page = page as HistoryReviewPage<HistoryInquiry>
  } catch (error) {
    if (!current(epoch) || state.request !== requestId) return
    if (error instanceof ApiError && error.status === 409) stale()
    else state.error = error
  } finally { requests.delete(controller); if (current(epoch) && state.request === requestId) state.loading = false }
}
function firstPages() { void load('timeline'); void load('inquiries') }
async function refresh() {
  if (!access.enabled.value || refreshing.value) return
  refreshing.value = true; reset(); needsRefresh.value = true; refreshError.value = null
  const authority = scope.value
  const refreshEpoch = ++refreshGeneration
  const sameRefresh = () => !disposed && refreshEpoch === refreshGeneration && scope.value === authority && access.enabled.value
  try {
    await props.refreshCore()
    if (!sameRefresh()) return
    refreshing.value = false; needsRefresh.value = false; firstPages()
  } catch (error) { if (sameRefresh()) refreshError.value = error }
  finally { if (sameRefresh()) refreshing.value = false }
}
function move(section: 'timeline' | 'inquiries', forward: boolean) {
  const state = section === 'timeline' ? timeline : inquiries
  if (state.loading || needsRefresh.value) return
  if (forward) { if (!state.page?.next_cursor) return; state.cursors.push(state.page.next_cursor) }
  else { if (state.cursors.length < 2) return; state.cursors.pop() }
  closeDetail(); void load(section)
}
watch(scope, () => { refreshGeneration++; refreshing.value = false; refreshError.value = null; reset(); needsRefresh.value = false; firstPages() }, { immediate: true, flush: 'sync' })
watch(revision, () => { if (refreshing.value) reset(); else stale() }, { flush: 'sync' })
watch([family, dated], () => { reset(); firstPages() }, { flush: 'sync' })
onBeforeUnmount(() => { disposed = true; reset() })
async function inspect(entry: TimelineEntry) {
  if (!access.enabled.value || needsRefresh.value) return
  closeDetail(); selected.value = entry; detailLoading.value = true
  const epoch = generation; const detailEpoch = detailGeneration
  const expectedRevision = revision.value
  const controller = new AbortController(); requests.add(controller)
  const key = [...access.prefix.value, props.personId, revision.value, entry.kind, entry.id, epoch, detailEpoch]
  try {
    const result = await access.read(key, () => [...access.prefix.value, props.personId, revision.value, selected.value?.kind, selected.value?.id, generation, detailGeneration], () => fetchHistoryTimelineDetail(props.personId, entry.kind, entry.id, controller.signal))
    if (current(epoch) && detailEpoch === detailGeneration) {
      if (result.read_revision !== expectedRevision) { stale(); return }
      detail.value = result
    }
  } catch (error) { if (current(epoch) && detailEpoch === detailGeneration) { if (error instanceof ApiError && error.status === 409) stale(); else detailError.value = error } }
  finally { requests.delete(controller); if (current(epoch) && detailEpoch === detailGeneration) detailLoading.value = false }
}
const detailPt = () => ({ ...dialogPt(), root: { class: 'glass-panel w-full max-w-2xl max-h-[calc(100dvh-2rem)] flex flex-col' }, content: { class: 'min-h-0 overflow-y-auto px-5 py-4' }, footer: { class: 'flex shrink-0 flex-wrap items-center justify-end gap-3 px-5 pb-5 pt-2' } })
</script>
<template>
  <div
    v-if="access.enabled.value"
    class="min-w-0 space-y-4"
    data-testid="person-history-review"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <h2 class="text-section font-medium">
          History
        </h2><button
          type="button"
          :class="buttonClasses()"
          :disabled="refreshing || timeline.loading || inquiries.loading"
          @click="refresh"
        >
          Refresh history
        </button>
      </div>
      <p class="mt-2 text-small text-text-muted">
        {{ core.history.known_count }} dated facts · {{ core.history.unknown_count }} with date unknown. Notes and tasks are reviewed separately.
      </p>
      <p
        v-if="needsRefresh"
        role="status"
        class="mt-3 text-small"
      >
        History changed. Refresh to start a current page series.
      </p>
      <p
        v-if="refreshError"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ describeApiError(refreshError, 'Could not refresh history. Retry Refresh history before viewing more facts.') }}
      </p>
      <p
        v-if="refreshing"
        role="status"
        class="mt-3 text-small"
      >
        Refreshing history…
      </p>
      <div
        v-if="!compact"
        class="mt-3 grid gap-3 sm:grid-cols-2"
      >
        <FormField
          v-slot="{ id }"
          label="History family"
          bare
        >
          <select
            :id="id"
            v-model="family"
            :class="INPUT_CLASSES"
          >
            <option value="all">
              All facts
            </option><option value="native">
              Native facts
            </option><option value="events">
              FUB events
            </option><option value="calls">
              FUB calls
            </option><option value="text_messages">
              FUB texts
            </option>
          </select>
        </FormField><FormField
          v-slot="{ id }"
          label="History date group"
          bare
        >
          <select
            :id="id"
            v-model="dated"
            :class="INPUT_CLASSES"
          >
            <option value="known">
              Known dates · newest first
            </option><option value="unknown">
              Date unknown · import order
            </option>
          </select>
        </FormField>
      </div>
      <p class="mt-3 text-small text-text-muted">
        {{ dated === 'unknown' ? 'Undated FUB records use stable import order, without a guessed historical date.' : 'Native facts use their display time. Imported records use explicitly labeled FUB record-created time.' }}
      </p>
      <p
        v-if="timeline.loading"
        role="status"
        class="mt-3 text-small text-text-muted"
      >
        Loading history…
      </p>
      <p
        v-if="timeline.error"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ describeApiError(timeline.error, 'Could not load history.') }}
      </p>
      <button
        v-if="timeline.error"
        type="button"
        class="mt-2"
        :class="buttonClasses()"
        @click="load('timeline')"
      >
        Retry history page
      </button>
      <ol
        v-if="timeline.page"
        class="mt-3 divide-y divide-border"
      >
        <li
          v-for="entry in timeline.page.items"
          :key="`${entry.kind}:${entry.id}`"
          class="min-w-0 space-y-2 py-4"
        >
          <HistoryTimelineEntry :entry="entry" /><button
            v-if="!compact"
            type="button"
            :class="buttonClasses('ghost')"
            :aria-label="`Inspect ${entry.kind} ${entry.id}`"
            @click="inspect(entry)"
          >
            Inspect fact metadata
          </button>
        </li>
      </ol>
      <p
        v-if="timeline.page && !timeline.page.items.length"
        class="mt-3 text-small text-text-muted"
      >
        No facts in this date group and family.
      </p>
      <div
        v-if="!compact"
        class="mt-3 flex flex-wrap gap-2"
      >
        <button
          type="button"
          :class="buttonClasses()"
          :disabled="needsRefresh || timeline.loading || timeline.cursors.length < 2"
          @click="move('timeline', false)"
        >
          Previous history
        </button><button
          type="button"
          :class="buttonClasses()"
          :disabled="needsRefresh || timeline.loading || !timeline.page?.next_cursor"
          @click="move('timeline', true)"
        >
          More history
        </button>
      </div>
    </Card>
    <Card
      v-if="!compact"
      class="min-w-0"
    >
      <h2 class="text-section font-medium">
        Inquiries <span class="text-text-muted">({{ core.inquiries.count }})</span>
      </h2><p class="mt-2 text-small text-text-muted">
        Native inquiry metadata is separate from imported FUB event records.
      </p><p
        v-if="inquiries.loading"
        role="status"
        class="mt-3 text-small text-text-muted"
      >
        Loading inquiries…
      </p><p
        v-if="inquiries.error"
        role="alert"
        class="mt-3 text-small text-danger"
      >
        {{ describeApiError(inquiries.error, 'Could not load inquiries.') }}
      </p><button
        v-if="inquiries.error"
        type="button"
        class="mt-2"
        :class="buttonClasses()"
        @click="load('inquiries')"
      >
        Retry inquiry page
      </button><ul
        v-if="inquiries.page"
        class="mt-3 space-y-3"
      >
        <li
          v-for="inquiry in inquiries.page.items"
          :key="inquiry.id"
          class="break-words text-small"
        >
          {{ inquiry.source }} · {{ formatAbsoluteTime(inquiry.received_at) }}<p
            v-if="inquiry.source_external_id"
            class="break-all text-text-muted"
          >
            Source record {{ inquiry.source_external_id }}
          </p>
        </li>
      </ul><p
        v-if="inquiries.page && !inquiries.page.items.length"
        class="mt-3 text-small text-text-muted"
      >
        No inquiries on this page.
      </p><div class="mt-3 flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses()"
          :disabled="needsRefresh || inquiries.loading || inquiries.cursors.length < 2"
          @click="move('inquiries', false)"
        >
          Previous inquiries
        </button><button
          type="button"
          :class="buttonClasses()"
          :disabled="needsRefresh || inquiries.loading || !inquiries.page?.next_cursor"
          @click="move('inquiries', true)"
        >
          More inquiries
        </button>
      </div>
    </Card>
    <p
      v-else
      class="text-small text-text-muted"
    >
      Latest inquiry source: {{ inquiries.page?.items[0]?.source ?? 'Not available' }}
    </p>
    <Dialog
      :visible="!!selected && access.enabled.value"
      :aria-labelledby="titleId"
      modal
      :closable="false"
      :pt="detailPt()"
      @update:visible="value => !value && closeDetail()"
    >
      <template #header>
        <h2
          :id="titleId"
          class="text-section font-medium"
        >
          Historical fact metadata
        </h2>
      </template>
      <p
        v-if="detailLoading"
        role="status"
        class="text-small text-text-muted"
      >
        Loading fact metadata…
      </p><p
        v-if="detailError"
        role="alert"
        class="text-small text-danger"
      >
        {{ describeApiError(detailError, 'Could not load fact metadata.') }}
      </p>
      <template v-if="detail">
        <HistoryTimelineEntry
          :entry="detail"
          expanded
        /><dl
          v-if="detail.provenance"
          class="mt-3 space-y-2 border-t border-border pt-3 text-small"
        >
          <div>
            <dt class="text-text-muted">
              Import plan / attempt
            </dt><dd class="break-all">
              {{ detail.provenance.plan_id }} / {{ detail.provenance.attempt_id }}
            </dd>
          </div><div>
            <dt class="text-text-muted">
              Manifest / stable position
            </dt><dd class="break-all">
              {{ detail.provenance.manifest_id }} / {{ detail.provenance.stable_position }}
            </dd>
          </div><div>
            <dt class="text-text-muted">
              Time basis
            </dt><dd>{{ detail.provenance.source_time_basis === 'fub_record_created' ? 'FUB record created' : 'Unknown source date' }}</dd>
          </div>
        </dl><p class="mt-3 break-all text-small text-text-muted">
          Correlation {{ detail.correlation_id }}
        </p>
      </template>
      <template #footer>
        <button
          v-if="detailError && selected"
          type="button"
          :class="buttonClasses()"
          @click="inspect(selected)"
        >
          Retry metadata
        </button><button
          type="button"
          :class="buttonClasses()"
          @click="closeDetail"
        >
          Close metadata
        </button>
      </template>
    </Dialog>
  </div>
</template>
