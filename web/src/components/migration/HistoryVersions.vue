<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { ApiError } from '../../api/client'
import { fetchHistoryVersion, fetchHistoryVersions, useHistoryReviewAccess, type HistoryReviewPage, type TimelineDetail, type TimelineEntry, type TimelineKind } from '../../api/historyReview'
import { buttonClasses } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import HistoryTimelineEntry from './HistoryTimelineEntry.vue'
const props = defineProps<{ personId: string; kind: TimelineKind; identity: string; revision: string }>()
const emit = defineEmits<{ stale: [] }>()
const access = useHistoryReviewAccess()
const page = ref<HistoryReviewPage<TimelineEntry> | null>(null)
const detail = ref<TimelineDetail | null>(null)
const cursors = ref([''])
const loading = ref(false)
const detailLoading = ref(false)
const error = ref<unknown>(null)
const detailError = ref<unknown>(null)
let generation = 0
let detailGeneration = 0
let disposed = false
const requests = new Set<AbortController>()
const scope = computed(() => JSON.stringify([access.scope.value, access.enabled.value, props.personId, props.kind, props.identity, props.revision]))
function reset() {
  generation++; detailGeneration++
  requests.forEach(request => request.abort()); requests.clear()
  page.value = null; detail.value = null; cursors.value = ['']; loading.value = false; detailLoading.value = false; error.value = null; detailError.value = null
}
watch(scope, reset, { flush: 'sync' })
onBeforeUnmount(() => { disposed = true; reset() })
const current = (epoch: number) => !disposed && generation === epoch && access.enabled.value
function stale() { reset(); emit('stale') }
async function load() {
  if (!access.enabled.value || loading.value) return
  const epoch = generation; const authority = scope.value; const cursor = cursors.value.at(-1) ?? ''
  detailGeneration++; detail.value = null; detailError.value = null; detailLoading.value = false
  page.value = null; loading.value = true; error.value = null
  const controller = new AbortController(); requests.add(controller)
  try {
    const result = await access.read([authority, cursor, epoch], () => [scope.value, cursors.value.at(-1) ?? '', generation], () => fetchHistoryVersions(props.personId, props.kind, props.identity, cursor || undefined, controller.signal))
    if (!current(epoch)) return
    if (result.read_revision !== props.revision) { stale(); return }
    page.value = result
  } catch (failure) { if (current(epoch)) { if (failure instanceof ApiError && failure.status === 409) stale(); else error.value = failure } }
  finally { requests.delete(controller); if (current(epoch)) loading.value = false }
}
function move(forward: boolean) {
  if (loading.value) return
  if (forward) { if (!page.value?.next_cursor) return; cursors.value.push(page.value.next_cursor) }
  else { if (cursors.value.length < 2) return; cursors.value.pop() }
  void load()
}
async function inspect(version: string) {
  if (!access.enabled.value || detailLoading.value) return
  const epoch = generation; const detailEpoch = ++detailGeneration; const authority = scope.value
  detailLoading.value = true; detail.value = null; detailError.value = null
  const controller = new AbortController(); requests.add(controller)
  try {
    const result = await access.read([authority, version, epoch, detailEpoch], () => [scope.value, version, generation, detailGeneration], () => fetchHistoryVersion(props.personId, props.kind, props.identity, version, controller.signal))
    if (!current(epoch) || detailEpoch !== detailGeneration) return
    if (result.read_revision !== props.revision) { stale(); return }
    detail.value = result
  } catch (failure) { if (current(epoch) && detailEpoch === detailGeneration) { if (failure instanceof ApiError && failure.status === 409) stale(); else detailError.value = failure } }
  finally { requests.delete(controller); if (current(epoch) && detailEpoch === detailGeneration) detailLoading.value = false }
}
</script>
<template>
  <section
    v-if="access.enabled.value"
    class="mt-4 min-w-0 space-y-3 border-t border-border pt-4"
    aria-label="Retained history versions"
  >
    <h3 class="text-body font-medium">
      Retained versions
    </h3>
    <p class="text-small text-text-muted">
      Each version preserves the metadata from its retained capture. Message contents are not displayed.
    </p>
    <button
      v-if="!page"
      type="button"
      :class="buttonClasses()"
      :disabled="loading"
      @click="load"
    >
      {{ loading ? 'Loading versions…' : error ? 'Retry versions' : 'Browse versions' }}
    </button>
    <p
      v-if="error"
      role="alert"
      class="text-small text-danger"
    >
      {{ describeApiError(error, 'Could not load retained versions.') }}
    </p>
    <ol
      v-if="page"
      class="space-y-3"
    >
      <li
        v-for="entry in page.items"
        :key="entry.version"
        class="min-w-0 space-y-2 rounded-lg border border-border p-3"
      >
        <p class="text-small font-medium">
          Version {{ entry.version }}
        </p>
        <HistoryTimelineEntry :entry="entry" />
        <button
          v-if="entry.version"
          type="button"
          :class="buttonClasses()"
          :disabled="detailLoading"
          @click="inspect(entry.version)"
        >
          Inspect version {{ entry.version }}
        </button>
      </li>
    </ol>
    <div
      v-if="page || cursors.length > 1"
      class="flex flex-wrap gap-2"
    >
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="loading || cursors.length < 2"
        @click="move(false)"
      >
        Previous versions
      </button>
      <button
        type="button"
        :class="buttonClasses()"
        :disabled="loading || !page?.next_cursor"
        @click="move(true)"
      >
        More versions
      </button>
    </div>
    <p
      v-if="detailLoading"
      role="status"
      class="text-small text-text-muted"
    >
      Loading version metadata…
    </p>
    <p
      v-if="detailError"
      role="alert"
      class="text-small text-danger"
    >
      {{ describeApiError(detailError, 'Could not load version metadata. Select the version to retry.') }}
    </p>
    <div
      v-if="detail"
      class="min-w-0 space-y-2 rounded-lg border border-border p-3"
    >
      <h4 class="text-body font-medium">
        Version {{ detail.version }} metadata
      </h4>
      <HistoryTimelineEntry
        :entry="detail"
        expanded
      />
      <p
        v-if="detail.provenance?.capture_id"
        class="break-all text-small text-text-muted"
      >
        Retained capture {{ detail.provenance.capture_id }}
      </p>
    </div>
  </section>
</template>
