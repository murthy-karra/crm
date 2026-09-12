<script setup lang="ts">
import { computed, onBeforeUnmount, reactive, ref, useId, watch } from 'vue'
import { RouterLink } from 'vue-router'
import Dialog from 'primevue/dialog'
import Card from './Card.vue'
import StageLabel from './StageLabel.vue'
import PersonImportProvenance from './migration/PersonImportProvenance.vue'
import PersonMetadataProvenance from './migration/PersonMetadataProvenance.vue'
import ActivityFieldViewer from './migration/ActivityFieldViewer.vue'
import { ApiError } from '../api/client'
import {
  fetchReviewNote, fetchReviewNotes, fetchReviewTasks, reviewHistoryTitle, reviewHistoryDetail, useActivityReviewCore,
  type ReviewFullNote, type ReviewNote, type ReviewPage, type ReviewProvenance, type ReviewTask,
} from '../api/activityReview'
import type { PersonCustomFieldValue } from '../api/types'
import { buttonClasses, dialogPt } from '../lib/controls'
import { describeApiError } from '../lib/errors'
import { formatAbsoluteTime } from '../lib/format'

const props = defineProps<{ personId: string }>()
const { access, reading, data: core } = useActivityReviewCore(() => props.personId)
type Section = 'notes' | 'open' | 'completed'
interface PageState<T> { page: ReviewPage<T> | null; cursor: string; loading: boolean; error: unknown }
const notes = reactive<PageState<ReviewNote>>({ page: null, cursor: '', loading: false, error: null })
const open = reactive<PageState<ReviewTask>>({ page: null, cursor: '', loading: false, error: null })
const completed = reactive<PageState<ReviewTask>>({ page: null, cursor: '', loading: false, error: null })
const refreshing = ref(false)
const refreshNotice = ref('')
const selectedNote = ref('')
const fullNote = ref<ReviewFullNote | null>(null)
const noteLoading = ref(false)
const noteError = ref<unknown>(null)
const selectedSource = ref<ReviewProvenance | null>(null)
const noteTitle = useId()
const sourceTitle = useId()
let generation = 0
let noteGeneration = 0
let disposed = false
const requests = new Set<AbortController>()
const revision = computed(() => core.value?.activity.activity_revision ?? '')
const scope = computed(() => JSON.stringify([access.scope.value, access.enabled.value, props.personId, revision.value]))
function reset() {
  generation++
  for (const request of requests) request.abort()
  requests.clear()
  for (const state of [notes, open, completed]) Object.assign(state, { page: null, cursor: '', loading: false, error: null })
  closeNote()
  selectedSource.value = null
}
function current(epoch: number) { return !disposed && epoch === generation && access.enabled.value && !!core.value && !refreshing.value }
async function refresh() {
  if (refreshing.value || !access.enabled.value) return
  refreshing.value = true
  reset()
  refreshNotice.value = 'Activity changed. The previous pages and open note were cleared.'
  try { await reading.refetch() }
  finally {
    refreshing.value = false
    if (!disposed && access.enabled.value && !reading.isError.value) loadFirstPages()
  }
}
async function load(section: Section, cursor = '') {
  if (!core.value || !access.enabled.value || refreshing.value) return
  const state = section === 'notes' ? notes : section === 'open' ? open : completed
  if (state.loading) return
  state.loading = true; state.error = null; state.page = null; state.cursor = cursor
  const epoch = generation
  const controller = new AbortController(); requests.add(controller)
  const expected = [...access.prefix.value, props.personId, revision.value, section, cursor, epoch]
  const currentKey = () => [...access.prefix.value, props.personId, revision.value, section, state.cursor, generation]
  try {
    const page = await access.read<ReviewPage<ReviewNote> | ReviewPage<ReviewTask>>(expected, currentKey, () => section === 'notes'
      ? fetchReviewNotes(props.personId, cursor || undefined, controller.signal)
      : fetchReviewTasks(props.personId, section, cursor || undefined, controller.signal))
    if (!current(epoch)) return
    if (page.activity_revision !== revision.value) throw new ApiError(409, 'activity_refresh_required')
    if (section === 'notes') notes.page = page as ReviewPage<ReviewNote>
    else (section === 'open' ? open : completed).page = page as ReviewPage<ReviewTask>
  } catch (error) {
    if (!current(epoch)) return
    if (error instanceof ApiError && error.status === 409) void refresh()
    else state.error = error
  } finally { requests.delete(controller); if (current(epoch)) state.loading = false }
}
function loadFirstPages() { for (const section of ['notes', 'open', 'completed'] as const) void load(section) }
watch(scope, () => {
  reset()
  refreshNotice.value = ''
  if (!refreshing.value) loadFirstPages()
}, { immediate: true, flush: 'sync' })
watch(() => reading.isError.value, value => { if (value) reset(); else if (!refreshing.value) loadFirstPages() }, { flush: 'sync' })
onBeforeUnmount(() => { disposed = true; reset() })
function closeNote() { noteGeneration++; selectedNote.value = ''; fullNote.value = null; noteError.value = null; noteLoading.value = false }
async function inspectNote(id: string) {
  if (!core.value || !access.enabled.value) return
  selectedNote.value = id; fullNote.value = null; noteError.value = null; noteLoading.value = true
  selectedSource.value = null
  const epoch = generation
  const noteEpoch = ++noteGeneration
  const controller = new AbortController(); requests.add(controller)
  const key = [...access.prefix.value, props.personId, revision.value, 'note', id, epoch, noteEpoch]
  try {
    const note = await access.read(key, () => [...access.prefix.value, props.personId, revision.value, 'note', selectedNote.value, generation, noteGeneration], () => fetchReviewNote(props.personId, id, controller.signal))
    if (current(epoch) && noteEpoch === noteGeneration && selectedNote.value === id) fullNote.value = note
  } catch (error) {
    if (!current(epoch) || noteEpoch !== noteGeneration || selectedNote.value !== id) return
    if (error instanceof ApiError && error.status === 409) void refresh()
    else noteError.value = error
  } finally { requests.delete(controller); if (current(epoch) && noteEpoch === noteGeneration && selectedNote.value === id) noteLoading.value = false }
}
function fieldValue(field: PersonCustomFieldValue) { return field.option_label ?? Object.values(field.value)[0] }
const taskSections = computed(() => [
  { key: 'open' as const, title: 'Open tasks', count: core.value?.activity.open_tasks_count, state: open },
  { key: 'completed' as const, title: 'Completed tasks', count: core.value?.activity.completed_tasks_count, state: completed },
])
const sourceRequest = computed(() => selectedSource.value ? { kind: 'results' as const, importId: selectedSource.value.activity_import_id, rowId: selectedSource.value.result_id, fieldKey: 'all' } : null)
</script>

<template>
  <div
    class="min-w-0 space-y-4"
    data-testid="person-activity-review"
  >
    <RouterLink
      to="/people"
      class="inline-flex min-h-10 items-center text-small text-text-muted hover:text-text"
    >
      People
    </RouterLink>
    <p
      v-if="!access.enabled.value"
      role="alert"
      class="text-body text-text-muted"
    >
      Person review requires a current Organization administrator session.
    </p>
    <p
      v-else-if="reading.isError.value"
      role="alert"
      class="text-body text-danger"
    >
      {{ describeApiError(reading.error.value, 'Could not load this Person for review.') }}
    </p>
    <p
      v-else-if="!core"
      role="status"
      class="text-body text-text-muted"
    >
      Loading Person for review…
    </p>
    <button
      v-if="access.enabled.value && reading.isError.value"
      type="button"
      :class="buttonClasses()"
      @click="reading.refetch()"
    >
      Retry Person review
    </button>
    <template v-if="core && !reading.isError.value">
      <Card>
        <h1 class="break-words text-title font-medium text-text">
          {{ core.person.display_name }}
        </h1>
        <p class="mt-2 text-small text-text-muted">
          Administrator review · Operational use remains unavailable.
        </p>
        <div class="mt-3 flex flex-wrap items-center gap-3">
          <StageLabel
            :stage="core.person.stage"
            badge
          /><span class="text-small text-text-muted">Owner: {{ core.person.assigned_user?.display_name ?? 'Unassigned' }}</span>
        </div>
        <div
          class="mt-3 flex flex-wrap gap-2"
          aria-label="Tags"
        >
          <span
            v-for="tag in core.tags"
            :key="tag.id"
            class="rounded-lg border border-border px-2 py-1 text-small"
          >{{ tag.name }}</span>
        </div>
        <dl class="mt-3 space-y-2 text-small">
          <div
            v-for="field in core.custom_fields"
            :key="field.field_id"
          >
            <dt class="text-text-muted">
              {{ field.label }}
            </dt><dd class="whitespace-pre-wrap break-words">
              {{ fieldValue(field) }}
            </dd>
          </div>
        </dl>
      </Card>
      <Card>
        <h2 class="text-section font-medium">
          Contact methods
        </h2><ul class="mt-3 space-y-2 text-body">
          <li
            v-for="method in core.contact_methods"
            :key="method.id"
            class="break-all"
          >
            {{ method.kind }} · {{ method.value }}
          </li>
        </ul><p
          v-if="!core.contact_methods.length"
          class="mt-2 text-small text-text-muted"
        >
          No contact methods.
        </p>
      </Card>
      <Card>
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h2 class="text-section font-medium">
            Imported activity
          </h2><button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="refreshing || reading.isFetching.value"
            @click="refresh"
          >
            Refresh activity
          </button>
        </div>
        <p class="mt-2 text-small text-text-muted">
          Counts and pages reflect activity revision {{ revision }}. Imports in progress may add more items.
        </p>
        <p
          v-if="refreshNotice"
          role="status"
          class="mt-2 text-small text-text-muted"
        >
          {{ refreshNotice }}
        </p>
        <p
          v-if="refreshing"
          role="status"
          class="mt-2 text-small text-text-muted"
        >
          Refreshing activity…
        </p>
      </Card>
      <Card>
        <h2 class="text-section font-medium">
          Notes <span class="text-text-muted">({{ core.activity.notes_count }})</span>
        </h2>
        <p
          v-if="notes.loading"
          role="status"
          class="mt-3 text-small text-text-muted"
        >
          Loading notes…
        </p>
        <p
          v-if="notes.error"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(notes.error, 'Could not load notes.') }}
        </p>
        <ul class="mt-3 divide-y divide-border">
          <li
            v-for="note in notes.page?.items ?? []"
            :key="note.id"
            class="min-w-0 py-3"
          >
            <p class="text-small text-text-muted">
              CRM author: {{ note.author?.display_name ?? 'Unmapped or absent' }} · {{ formatAbsoluteTime(note.created_at) }}
            </p>
            <p class="mt-2 whitespace-pre-wrap break-words text-body">
              {{ note.excerpt }}
            </p>
            <p
              v-if="note.has_more"
              class="mt-1 text-small text-text-muted"
            >
              Excerpt · Full note available
            </p>
            <button
              type="button"
              :class="buttonClasses('secondary')"
              class="mt-2"
              :aria-label="`Read full note from ${formatAbsoluteTime(note.created_at)}`"
              @click="inspectNote(note.id)"
            >
              Read full note
            </button>
          </li>
        </ul>
        <p
          v-if="notes.page && !notes.page.items.length"
          class="mt-2 text-small text-text-muted"
        >
          No notes on this page.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="notes.loading || !notes.cursor"
            @click="load('notes')"
          >
            First notes page
          </button><button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="notes.loading || !notes.page?.next_cursor"
            @click="load('notes', notes.page?.next_cursor ?? '')"
          >
            Next notes page
          </button><button
            v-if="notes.error"
            type="button"
            :class="buttonClasses('secondary')"
            @click="load('notes', notes.cursor)"
          >
            Retry notes
          </button>
        </div>
      </Card>
      <Card
        v-for="section in taskSections"
        :key="section.key"
      >
        <h2 class="text-section font-medium">
          {{ section.title }} <span class="text-text-muted">({{ section.count }})</span>
        </h2>
        <p
          v-if="section.state.loading"
          role="status"
          class="mt-3 text-small text-text-muted"
        >
          Loading {{ section.title.toLowerCase() }}…
        </p>
        <p
          v-if="section.state.error"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(section.state.error, 'Could not load tasks.') }}
        </p>
        <ul class="mt-3 divide-y divide-border">
          <li
            v-for="task in section.state.page?.items ?? []"
            :key="task.id"
            class="min-w-0 space-y-2 py-3"
          >
            <h3 class="whitespace-pre-wrap break-words text-body font-medium">
              {{ task.title }}
            </h3>
            <p class="text-small text-text-muted">
              {{ task.kind.replace('_', ' ') }} · {{ task.due_at ? `Due ${formatAbsoluteTime(task.due_at)}` : 'No deadline' }}
            </p>
            <p class="text-small text-text-muted">
              CRM creator: {{ task.created_by?.display_name ?? 'Unmapped or absent' }} · Assignee: {{ task.assignee?.display_name ?? 'Unassigned' }}
            </p>
            <p class="text-small text-text-muted">
              Created {{ formatAbsoluteTime(task.created_at) }} · Updated {{ formatAbsoluteTime(task.updated_at) }}
            </p>
            <p
              v-if="task.completed_at"
              class="text-small text-text-muted"
            >
              Completed {{ formatAbsoluteTime(task.completed_at) }} · CRM completer: {{ task.completed_by?.display_name ?? 'Not recorded' }}
            </p>
            <button
              v-if="task.provenance"
              type="button"
              :class="buttonClasses('secondary')"
              :aria-label="`Inspect source for ${task.title}`"
              @click="selectedSource = task.provenance"
            >
              Inspect task source
            </button>
          </li>
        </ul>
        <p
          v-if="section.state.page && !section.state.page.items.length"
          class="mt-2 text-small text-text-muted"
        >
          No {{ section.title.toLowerCase() }} on this page.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="section.state.loading || !section.state.cursor"
            @click="load(section.key)"
          >
            First {{ section.title.toLowerCase() }} page
          </button><button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="section.state.loading || !section.state.page?.next_cursor"
            @click="load(section.key, section.state.page?.next_cursor ?? '')"
          >
            Next {{ section.title.toLowerCase() }} page
          </button><button
            v-if="section.state.error"
            type="button"
            :class="buttonClasses('secondary')"
            @click="load(section.key, section.state.cursor)"
          >
            Retry {{ section.title.toLowerCase() }}
          </button>
        </div>
      </Card>
      <Card>
        <h2 class="text-section font-medium">
          Inquiries
        </h2><ul class="mt-3 space-y-3">
          <li
            v-for="inquiry in core.inquiries"
            :key="inquiry.id"
            class="text-body"
          >
            <p class="break-words">
              {{ inquiry.source }} · {{ formatAbsoluteTime(inquiry.received_at) }}
            </p><p class="whitespace-pre-wrap break-words text-text-muted">
              {{ inquiry.message }}
            </p>
          </li>
        </ul><p
          v-if="!core.inquiries.length"
          class="mt-2 text-small text-text-muted"
        >
          No inquiries.
        </p>
      </Card>
      <Card>
        <h2 class="text-section font-medium">
          Core history
        </h2><ul class="mt-3 space-y-3">
          <li
            v-for="entry in core.core_history"
            :key="entry.id"
            class="text-small"
          >
            <p>{{ reviewHistoryTitle(entry.kind) }} · {{ formatAbsoluteTime(entry.occurred_at) }}</p><p class="text-text-muted">
              {{ entry.actor?.display_name ?? 'System' }}
            </p><p class="break-words">
              {{ reviewHistoryDetail(entry) }}
            </p>
          </li>
        </ul>
      </Card>
      <Card><PersonImportProvenance :person-id="personId" /><PersonMetadataProvenance :person-id="personId" /></Card>
    </template>
    <Dialog
      :visible="!!selectedNote && access.enabled.value"
      :aria-labelledby="noteTitle"
      modal
      :closable="false"
      :pt="dialogPt()"
      @update:visible="(value: boolean) => !value && closeNote()"
    >
      <template #header>
        <h2
          :id="noteTitle"
          class="text-section font-medium"
        >
          Full imported note
        </h2>
      </template>
      <p
        v-if="noteLoading"
        role="status"
        class="text-small text-text-muted"
      >
        Loading full note…
      </p>
      <p
        v-if="noteError"
        role="alert"
        class="text-small text-danger"
      >
        {{ describeApiError(noteError, 'Could not read this note.') }}
      </p>
      <template v-if="fullNote">
        <p class="text-small text-text-muted">
          CRM author: {{ fullNote.author?.display_name ?? 'Unmapped or absent' }} · Created {{ formatAbsoluteTime(fullNote.created_at) }} · Updated {{ formatAbsoluteTime(fullNote.updated_at) }}
        </p><pre
          class="mt-3 max-h-96 max-w-full overflow-auto whitespace-pre-wrap break-words text-body"
          tabindex="0"
          aria-label="Full native note body"
        >{{ fullNote.body }}</pre><button
          v-if="fullNote.provenance"
          type="button"
          :class="buttonClasses('secondary')"
          class="mt-3"
          @click="selectedSource = fullNote.provenance; closeNote()"
        >
          Inspect original note source
        </button>
      </template>
      <template #footer>
        <button
          v-if="noteError"
          type="button"
          :class="buttonClasses('secondary')"
          @click="inspectNote(selectedNote)"
        >
          Retry full note
        </button><button
          type="button"
          :class="buttonClasses('secondary')"
          @click="closeNote"
        >
          Close note
        </button>
      </template>
    </Dialog>
    <Dialog
      :visible="!!selectedSource && access.enabled.value"
      :aria-labelledby="sourceTitle"
      modal
      :closable="false"
      :pt="dialogPt()"
      @update:visible="(value: boolean) => !value && (selectedSource = null)"
    >
      <template #header>
        <h2
          :id="sourceTitle"
          class="text-section font-medium"
        >
          Imported activity source
        </h2>
      </template>
      <template v-if="selectedSource">
        <p class="break-all text-small text-text-muted">
          FUB account {{ selectedSource.source_account_id }} · Source record {{ selectedSource.source_id }}. Source actor evidence is separate from linked CRM users.
        </p><ActivityFieldViewer
          v-if="sourceRequest"
          :key="JSON.stringify([scope, generation, sourceRequest])"
          :request="sourceRequest"
          title="Original source and committed result"
        />
      </template>
      <template #footer>
        <button
          type="button"
          :class="buttonClasses('secondary')"
          @click="selectedSource = null"
        >
          Close source
        </button>
      </template>
    </Dialog>
  </div>
</template>
