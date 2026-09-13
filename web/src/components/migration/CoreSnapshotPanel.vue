<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery, useQueryClient } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import SnapshotBudgetPanel from './SnapshotBudgetPanel.vue'
import SnapshotPreviewPanel from './SnapshotPreviewPanel.vue'
import { queryKeys, useAuthSessionLifetime, useMe } from '../../api/queries'
import { ApiError } from '../../api/client'
import type { FubConnection } from '../../api/migrations'
import { cancelSnapshot, confirmSnapshot, fetchSnapshot, fetchSnapshots, generateSnapshotPreview, proposeSnapshot, retrySnapshot, type CoreSnapshot } from '../../api/snapshots'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime, sourceActive } from './format'

const props = defineProps<{ connection: FubConnection | null; assessmentBusy: boolean }>()
const emit = defineEmits<{ sourceBusy: [busy: boolean] }>()
const { data: me } = useMe()
const session = useAuthSessionLifetime()
const qc = useQueryClient()
const orgId = computed(() => me.value?.organization?.id ?? '')
const actorId = computed(() => me.value?.user.id ?? '')
const scope = computed(() => `${orgId.value}:${actorId.value}:${session.value}`)
const prefix = computed(() => queryKeys.snapshots(orgId.value, actorId.value, session.value))
const denied = ref(false)
const enabled = computed(() => !denied.value && me.value?.organization?.role === 'admin' && !!orgId.value)
const selectedId = ref('')
const selectedPreview = ref('')
// Other retained-evidence panels may navigate here; all reads retain this panel's
// current session/Organization guards and the server's snapshot authorization.
function openSnapshot(id: string) {
  if (disposed || !enabled.value) return
  selectedId.value = id
}
defineExpose({ openSnapshot })
const cursor = ref('')
const previousCursors = ref<string[]>([])
const actionError = ref<string | null>(null)
const pending = ref<string | null>(null)
const confirmation = ref<CoreSnapshot | null>(null)
const requestIds = new Map<string, string>()
const listFailures = ref(0)
const detailFailures = ref(0)
let generation = 0
let disposed = false
const listing = useQuery({
  queryKey: computed(() => [...prefix.value, 'list', cursor.value]),
  queryFn: async ({ signal }) => {
    const expected = scope.value; const page = cursor.value
    try {
      const result = await fetchSnapshots(page || undefined, signal)
      if (disposed || !enabled.value || expected !== scope.value || page !== cursor.value) throw new Error('Discarded stale snapshot list')
      listFailures.value = 0; return result
    } catch (error) {
      if (!signal.aborted && expected === scope.value && page === cursor.value) listFailures.value++
      throw error
    }
  }, enabled, retry: false, gcTime: 0,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
  refetchInterval: (q) => q.state.data?.snapshots.some(s => sourceActive(s.state))
    ? Math.min(30_000, 2_000 * 2 ** Math.min(listFailures.value, 4)) : false,
})
const detail = useQuery({
  queryKey: computed(() => [...prefix.value, 'run', selectedId.value]),
  queryFn: async ({ signal }) => {
    const expected = scope.value; const id = selectedId.value
    try {
      const result = await fetchSnapshot(id, signal)
      if (disposed || !enabled.value || expected !== scope.value || id !== selectedId.value) throw new Error('Discarded stale snapshot detail')
      detailFailures.value = 0; return result
    } catch (error) {
      if (!signal.aborted && expected === scope.value && id === selectedId.value) detailFailures.value++
      throw error
    }
  },
  enabled: computed(() => enabled.value && !!selectedId.value), retry: false, gcTime: 0,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
  refetchInterval: (q) => sourceActive(q.state.data?.snapshot.state)
    ? Math.min(30_000, 2_000 * 2 ** Math.min(detailFailures.value, 4)) : false,
})
const current = computed(() => enabled.value ? detail.data.value?.snapshot : undefined)
const sourceBusy = computed(() => !!listing.data.value?.snapshots.some(s => sourceActive(s.state)) || sourceActive(current.value?.state))
watch(sourceBusy, (busy) => emit('sourceBusy', busy), { immediate: true })
watch(listing.data, (list) => {
  if (enabled.value && !selectedId.value && list?.snapshots.length) selectedId.value = list.active_snapshot_id ?? list.snapshots[0].id
})
watch(selectedId, () => {
  generation++; detailFailures.value = 0; selectedPreview.value = ''; pending.value = null; confirmation.value = null; actionError.value = null
}, { flush: 'sync' })
watch(current, (snapshot) => {
  if (!selectedPreview.value && snapshot?.preview_ids.length) selectedPreview.value = snapshot.preview_ids[0]
})
watch(prefix, (next, previous) => {
  if (next.join(':') === previous.join(':')) return
  generation++; selectedId.value = ''; selectedPreview.value = ''; cursor.value = ''; previousCursors.value = []
  denied.value = false; listFailures.value = 0; detailFailures.value = 0; pending.value = null; confirmation.value = null; actionError.value = null; requestIds.clear()
  void qc.cancelQueries({ queryKey: previous }); qc.removeQueries({ queryKey: previous })
}, { flush: 'sync' })
function accessDenied() {
  denied.value = true; generation++; selectedId.value = ''; selectedPreview.value = ''; confirmation.value = null; pending.value = null; requestIds.clear()
  void qc.cancelQueries({ queryKey: prefix.value }); qc.removeQueries({ queryKey: prefix.value })
  void qc.invalidateQueries({ queryKey: queryKeys.me, exact: true })
}
watch([listing.error, detail.error], (errors) => {
  if (errors.some(e => e instanceof ApiError && [401, 403].includes(e.status))) accessDenied()
})
onBeforeUnmount(() => {
  disposed = true; generation++; requestIds.clear(); emit('sourceBusy', false)
  void qc.cancelQueries({ queryKey: prefix.value }); qc.removeQueries({ queryKey: prefix.value })
})
function requestId(key: string) {
  const id = requestIds.get(key) ?? crypto.randomUUID(); requestIds.set(key, id); return id
}
function refresh() {
  if (!enabled.value) return
  void qc.invalidateQueries({ queryKey: prefix.value })
  void qc.invalidateQueries({ queryKey: queryKeys.migration(orgId.value, actorId.value, session.value), exact: true })
}
async function act<T>(name: string, operation: () => Promise<T>, success: (result: T) => void) {
  if (pending.value || !enabled.value) return
  const expected = scope.value; const ownGeneration = generation
  const valid = () => !disposed && enabled.value && expected === scope.value && ownGeneration === generation
  pending.value = name; actionError.value = null
  try {
    const result = await operation()
    if (!valid()) return
    requestIds.delete(name); success(result); refresh()
  } catch (error) {
    if (!valid()) return
    if (error instanceof ApiError && [401, 403].includes(error.status)) { accessDenied(); return }
    if (error instanceof ApiError && [400, 404, 409].includes(error.status)) requestIds.delete(name)
    actionError.value = error instanceof ApiError && error.status === 409
      ? 'The run or source connection changed, or another source job is active. Refresh the status before retrying.'
      : describeApiError(error, 'Could not confirm this action. Refresh status or retry the same request.')
    if (error instanceof ApiError && error.status === 409) { confirmation.value = null; refresh() }
  } finally { if (valid()) pending.value = null }
}
function prepare() {
  const c = props.connection
  if (!c || c.status !== 'connected') return
  const name = `propose:${c.id}:${c.revision}`
  void act(name, () => proposeSnapshot(c.id, c.revision, requestId(name)), (result) => {
    cursor.value = ''; previousCursors.value = []; selectedId.value = result.snapshot.id; confirmation.value = result.snapshot
  })
}
function confirm() {
  const s = confirmation.value; if (!s) return
  const name = `confirm:${s.id}`
  void act(name, () => confirmSnapshot(s.id, requestId(name)), () => { confirmation.value = null })
}
function retry() {
  const s = current.value; if (!s) return
  const name = `retry:${s.id}`
  void act(name, () => retrySnapshot(s.id, requestId(name)), () => {})
}
function cancel() {
  const s = current.value; if (!s) return
  void act(`cancel:${s.id}`, () => cancelSnapshot(s.id), () => { confirmation.value = null })
}
function generatePreview() {
  const s = current.value; if (!s) return
  const name = `preview:${s.id}`
  void act(name, () => generateSnapshotPreview(s.id, requestId(name)), (result) => { selectedPreview.value = result.preview_id })
}
function older() {
  if (!listing.data.value?.next_cursor) return
  previousCursors.value.push(cursor.value); cursor.value = listing.data.value.next_cursor
}
function newer() { cursor.value = previousCursors.value.pop() ?? '' }
const confirmationText = computed(() => {
  const s = confirmation.value
  return s ? `Read People, users (including deleted users), stages, custom fields, notes with details, and open and completed tasks visible to this FUB credential for account ${s.source_account_id}. Preserve encrypted source responses and review projections with a ${formatBytes(s.run_byte_limit)} run allowance shared within the ${formatBytes(s.org_byte_limit)} Organization allowance. People includes Trash and unclaimed records offered to this source user. This captures evidence over time; it does not import or change CRM records. Other history, email and files remain for later snapshot steps. This proposal expires ${snapshotTime(s.proposal_expires_at)}.` : ''
})
</script>

<template>
  <div
    class="min-w-0"
    data-testid="core-snapshot-panel"
  >
    <Card class="mb-6 min-w-0">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div class="min-w-0">
          <h2 class="text-section font-semibold text-text">
            Core records snapshot
          </h2><p class="mt-1 max-w-2xl text-body text-text-muted">
            Capture source evidence, then review a preview before any import. People, users, stages, custom fields, notes and tasks are included.
          </p>
        </div>
        <div
          v-if="enabled"
          class="flex flex-wrap gap-2"
        >
          <button
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="listing.isFetching.value || detail.isFetching.value"
            data-testid="snapshot-refresh"
            @click="refresh"
          >
            Refresh snapshots
          </button>
          <button
            type="button"
            :class="buttonClasses('primary')"
            :disabled="!!pending || !connection || connection.status !== 'connected' || assessmentBusy || sourceBusy"
            data-testid="snapshot-prepare"
            @click="prepare"
          >
            Prepare snapshot
          </button>
        </div>
      </div>
      <p
        v-if="!enabled"
        role="alert"
        class="mt-4 text-small text-danger"
      >
        A current Organization administrator session is required to read snapshots. Refresh your session to continue.
      </p>
      <template v-else>
        <p
          v-if="assessmentBusy || sourceBusy"
          class="mt-3 text-small text-text-muted"
        >
          A source job is active. Retained reports remain available while it runs.
        </p>
        <p
          v-if="!connection || connection.status !== 'connected'"
          class="mt-3 text-small text-text-muted"
        >
          Connect a FUB account to prepare a new capture. Retained snapshots and previews remain available.
        </p>
        <p
          v-if="actionError"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ actionError }}
        </p>
        <p
          v-if="listing.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(listing.error.value, 'Could not load snapshots. Refresh to recover.') }}
        </p>
        <p
          v-if="listing.isPending.value"
          class="mt-4 text-small text-text-muted"
        >
          Loading snapshots…
        </p>
        <div
          v-else
          class="mt-4"
        >
          <label
            for="snapshot-selection"
            class="mb-1 block text-small font-medium text-text"
          >Retained snapshots</label>
          <select
            id="snapshot-selection"
            v-model="selectedId"
            :class="INPUT_CLASSES"
            data-testid="snapshot-selection"
          >
            <option
              value=""
              disabled
            >
              {{ listing.data.value?.snapshots.length ? 'Choose a snapshot' : 'No snapshots captured yet' }}
            </option>
            <option
              v-if="current && !listing.data.value?.snapshots.some(s => s.id === current?.id)"
              :value="current.id"
            >
              Selected · {{ snapshotTime(current.created_at) }} · {{ snapshotLabel(current.state) }}
            </option>
            <option
              v-for="s in listing.data.value?.snapshots ?? []"
              :key="s.id"
              :value="s.id"
            >
              {{ snapshotTime(s.created_at) }} · {{ snapshotLabel(s.state) }} · {{ s.id.slice(0, 8) }}
            </option>
          </select>
          <div
            v-if="previousCursors.length || listing.data.value?.next_cursor"
            class="mt-2 flex flex-wrap gap-2"
          >
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!previousCursors.length || listing.isFetching.value"
              @click="newer"
            >
              Newer snapshots
            </button>
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!listing.data.value?.next_cursor || listing.isFetching.value"
              @click="older"
            >
              Older snapshots
            </button>
          </div>
        </div>
        <p
          v-if="detail.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(detail.error.value, 'Could not load this snapshot. Refresh to recover.') }}
        </p>
        <p
          v-if="selectedId && detail.isPending.value"
          class="mt-3 text-small text-text-muted"
        >
          Loading snapshot…
        </p>
        <section
          v-if="current"
          class="mt-5 min-w-0"
          aria-label="Selected snapshot"
        >
          <div class="flex flex-wrap items-center justify-between gap-3">
            <p
              aria-live="polite"
              class="text-body font-medium text-text"
              data-testid="snapshot-state"
            >
              {{ snapshotLabel(current.state) }}<span v-if="current.pause_reason"> · {{ snapshotLabel(current.pause_reason) }}</span>
            </p>
            <div class="flex flex-wrap gap-2">
              <button
                v-if="current.actions.includes('confirm')"
                type="button"
                :class="buttonClasses('primary')"
                :disabled="!!pending"
                data-testid="snapshot-review-confirm"
                @click="confirmation = current"
              >
                Review and confirm capture
              </button>
              <button
                v-if="current.actions.includes('retry')"
                type="button"
                :class="buttonClasses('secondary')"
                :disabled="!!pending || assessmentBusy"
                data-testid="snapshot-retry"
                @click="retry"
              >
                Resume source capture
              </button>
              <button
                v-if="current.actions.includes('cancel')"
                type="button"
                :class="buttonClasses('secondary')"
                :disabled="!!pending"
                data-testid="snapshot-cancel"
                @click="cancel"
              >
                Cancel capture
              </button>
              <button
                v-if="current.actions.includes('generate_preview')"
                type="button"
                :class="buttonClasses('secondary')"
                :disabled="!!pending"
                data-testid="snapshot-generate-preview"
                @click="generatePreview"
              >
                Generate new preview
              </button>
            </div>
          </div>
          <p class="mt-2 break-words text-small text-text-muted">
            FUB account {{ current.source_account_id }} · {{ current.profile_version }} · started {{ snapshotTime(current.started_at) }} · completed {{ snapshotTime(current.completed_at) }}
          </p>
          <p class="mt-1 text-small text-text-muted">
            {{ formatBytes(current.raw_bytes) }} source bytes preserved · {{ current.accepted_captures }} successful captures. Observations are made over time, not at one instant. A completed snapshot can still contain review issues.
          </p>
          <p
            v-if="['paused', 'cancelled'].includes(current.state)"
            class="mt-2 text-small text-text"
          >
            Retained evidence is partial. Generating a preview reads retained data only; resuming source capture is a separate action.
          </p>
          <div
            class="mt-4 max-w-full overflow-x-auto"
            tabindex="0"
            role="region"
            aria-label="Snapshot progress by source stream"
          >
            <table class="w-full min-w-[720px] text-left text-small">
              <thead class="border-b border-border text-text-muted">
                <tr>
                  <th class="p-2 font-medium">
                    Source stream
                  </th><th class="p-2 font-medium">
                    Status
                  </th><th class="p-2 font-medium">
                    Reported total
                  </th><th class="p-2 font-medium">
                    Returned
                  </th><th class="p-2 font-medium">
                    Distinct IDs*
                  </th><th class="p-2 font-medium">
                    Content gaps
                  </th><th class="p-2 font-medium">
                    Last observed
                  </th>
                </tr>
              </thead><tbody>
                <tr
                  v-for="stream in detail.data.value?.streams ?? []"
                  :key="stream.stream"
                  class="border-b border-border/70 align-top"
                >
                  <td class="p-2 font-medium">
                    {{ snapshotLabel(stream.stream) }}
                  </td><td class="p-2">
                    {{ snapshotLabel(stream.state) }}<span
                      v-if="stream.error_code"
                      class="block text-text-muted"
                    >{{ snapshotLabel(stream.error_code) }}</span>
                  </td><td class="p-2">
                    {{ stream.reported_total ?? 'Unknown' }}
                  </td><td class="p-2">
                    {{ stream.returned_items }}
                  </td><td class="p-2">
                    {{ stream.distinct_ids }}
                  </td><td class="p-2">
                    {{ stream.content_gaps }}
                  </td><td class="p-2">
                    {{ snapshotTime(stream.observed_at) }}
                  </td>
                </tr>
              </tbody>
            </table>
          </div>
          <p class="mt-2 text-small text-text-muted">
            *Distinct IDs span each family, including both task partitions and note representations. Counts across streams must not be added into a migration total.
          </p>
          <details class="mt-4 text-small">
            <summary class="cursor-pointer py-2 font-medium focus-visible:outline focus-visible:outline-focus">
              Coverage and remaining data
            </summary><ul class="mt-2 space-y-3">
              <li
                v-for="coverage in detail.data.value?.coverage ?? []"
                :key="coverage.family"
              >
                <span class="font-medium">{{ snapshotLabel(coverage.family) }} · {{ snapshotLabel(coverage.state) }}</span><p class="text-text-muted">
                  {{ coverage.reason }} {{ coverage.next_action }}
                </p>
              </li>
            </ul>
          </details>
          <SnapshotBudgetPanel
            :key="current.id"
            :snapshot="current"
            @refresh="refresh"
            @access-denied="accessDenied"
          />
          <div
            v-if="current.preview_ids.length || selectedPreview"
            class="mt-5 border-t border-border pt-4"
          >
            <label
              for="snapshot-preview-selection"
              class="mb-1 block text-small font-medium"
            >Retained previews (latest 20)</label>
            <select
              id="snapshot-preview-selection"
              v-model="selectedPreview"
              :class="INPUT_CLASSES"
              data-testid="snapshot-preview-selection"
            >
              <option value="">
                Choose a preview
              </option><option
                v-if="selectedPreview && !current.preview_ids.includes(selectedPreview)"
                :value="selectedPreview"
              >
                New preview · {{ selectedPreview }}
              </option><option
                v-for="(id, index) in current.preview_ids"
                :key="id"
                :value="id"
              >
                {{ index === 0 ? 'Latest' : `Previous ${index}` }} · {{ id }}
              </option>
            </select>
          </div>
        </section>
      </template>
      <ConfirmDialog
        :visible="confirmation !== null && enabled"
        title="Confirm core snapshot capture"
        :message="confirmationText"
        confirm-label="Start source capture"
        :is-pending="!!pending"
        :error="actionError"
        error-fallback="Could not confirm capture. Refresh status to recover."
        @update:visible="(visible) => { if (!visible) confirmation = null }"
        @confirm="confirm"
      />
    </Card>
    <SnapshotPreviewPanel
      v-if="enabled && selectedId && selectedPreview"
      :key="`${scope}:${selectedId}:${selectedPreview}`"
      :snapshot-id="selectedId"
      :preview-id="selectedPreview"
      :org-id="orgId"
      :actor-id="actorId"
      :session-lifetime="session"
      @refresh="refresh"
      @access-denied="accessDenied"
    />
  </div>
</template>
