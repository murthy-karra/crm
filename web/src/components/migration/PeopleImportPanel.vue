<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import ImportMappingPanel from './ImportMappingPanel.vue'
import ImportRecordPanel from './ImportRecordPanel.vue'
import SnapshotBudgetPanel from './SnapshotBudgetPanel.vue'
import { ApiError } from '../../api/client'
import { cancelImport, confirmImport, fetchImport, fetchImports, importAccessError, importActive, proposeImport, replanImport, retryImport, uncertainImportError, useImportAccess, type ImportConfirmRequest, type ImportPatches, type PeopleImport } from '../../api/imports'
import { fetchSnapshot, fetchSnapshotPreview, fetchSnapshots } from '../../api/snapshots'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'

const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
const access = useImportAccess()
const selectedId = ref('')
const sourceId = ref('')
const previewId = ref('')
const importCursors = ref<string[]>([''])
const sourceCursors = ref<string[]>([''])
const readMode = ref<'plan' | 'results'>('plan')
const heldAck = ref(false)
const reviewAck = ref(false)
const remainingAck = ref(false)
const mappingDirty = ref(false)
const confirmation = ref<ImportConfirmRequest | null>(null)
const cancelReview = ref(false)
const actionError = ref('')
const pending = ref(false)
const uncertain = ref(false)
const listFailures = ref(0)
const detailFailures = ref(0)
let disposed = false
let selectionGeneration = 0
let identityGeneration = 0

type Intent =
  | { kind: 'plan'; identity: string; body: { request_id: string; snapshot_id: string; preview_id: string } }
  | { kind: 'replan'; identity: string; id: string; body: ImportPatches & { request_id: string; expected_plan_revision: string } }
  | { kind: 'confirm'; identity: string; id: string; body: ImportConfirmRequest }
  | { kind: 'retry' | 'cancel'; identity: string; id: string; body: { request_id: string } }
const intent = ref<Intent | null>(null)
const listKey = computed(() => [...access.prefix.value, 'list', importCursors.value.at(-1) ?? ''])
const detailKey = computed(() => [...access.prefix.value, 'detail', selectedId.value])
const sourcesKey = computed(() => [...access.prefix.value, 'sources', sourceCursors.value.at(-1) ?? ''])
const sourceKey = computed(() => [...access.prefix.value, 'source', sourceId.value])
const previewKey = computed(() => [...access.prefix.value, 'source-preview', sourceId.value, previewId.value])
function backoff(failures: number) { return Math.min(30_000, 2_000 * 2 ** Math.min(failures, 4)) }
async function verifyObserved(value: PeopleImport, expected: string) {
  if (expected !== access.scope.value || !access.enabled.value) return
  if (value.workspace.revision !== access.org.value?.workspace_revision || value.workspace.mode !== access.org.value?.workspace_mode) {
    await props.refreshWorkspace()
    throw new Error('Workspace status was refreshed')
  }
}
const listing = useQuery({
  queryKey: listKey, enabled: access.enabled, retry: false, gcTime: 0,
  queryFn: async ({ signal }) => {
    const expected = access.scope.value
    try {
      const result = await access.read(listKey.value, () => listKey.value, async () => { const v = await fetchImports(importCursors.value.at(-1) || undefined, signal); if (v.imports[0]) await verifyObserved(v.imports[0], expected); return v })
      listFailures.value = 0; return result
    } catch (error) { if (expected === access.scope.value && !signal.aborted) listFailures.value++; throw error }
  },
  refetchInterval: q => q.state.data?.imports.some(importActive) ? backoff(listFailures.value) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const detail = useQuery({
  queryKey: detailKey, enabled: computed(() => access.enabled.value && !!selectedId.value), retry: false, gcTime: 0,
  queryFn: async ({ signal }) => {
    const expected = access.scope.value; const id = selectedId.value
    try {
      const result = await access.read(detailKey.value, () => detailKey.value, async () => { const v = await fetchImport(id, signal); if (id === selectedId.value) await verifyObserved(v, expected); return v })
      detailFailures.value = 0; return result
    } catch (error) { if (expected === access.scope.value && id === selectedId.value && !signal.aborted) detailFailures.value++; throw error }
  },
  refetchInterval: q => importActive(q.state.data) ? backoff(detailFailures.value) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const sources = useQuery({ queryKey: sourcesKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(sourcesKey.value, () => sourcesKey.value, () => fetchSnapshots(sourceCursors.value.at(-1) || undefined, signal)) })
const source = useQuery({ queryKey: sourceKey, enabled: computed(() => access.enabled.value && !!sourceId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(sourceKey.value, () => sourceKey.value, () => fetchSnapshot(sourceId.value, signal)) })
const preview = useQuery({ queryKey: previewKey, enabled: computed(() => access.enabled.value && !!sourceId.value && !!previewId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(previewKey.value, () => previewKey.value, () => fetchSnapshotPreview(sourceId.value, previewId.value, signal)) })
const current = computed(() => access.enabled.value ? detail.data.value : undefined)
const budgetKey = computed(() => [...access.prefix.value, 'import-budget', current.value?.snapshot_id ?? ''])
const budget = useQuery({ queryKey: budgetKey, enabled: computed(() => access.enabled.value && !!current.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(budgetKey.value, () => budgetKey.value, () => fetchSnapshot(current.value!.snapshot_id, signal)) })
const eligibleSource = computed(() => {
  const s = source.data.value; const p = preview.data.value?.preview
  return access.enabled.value && access.org.value?.workspace_mode === 'operational' && s?.snapshot.profile_version === 'fub-core-v1'
    && ['completed', 'completed_with_gaps'].includes(s.snapshot.state) && ['people', 'users', 'stages'].every(name => s.streams.some(stream => stream.stream === name && stream.state === 'completed'))
    && p?.state === 'completed' && p.capture_sequence === s.snapshot.capture_sequence
})
const busy = computed(() => pending.value || !!intent.value)
const canConfirm = computed(() => current.value?.release_ready === true && current.value.actions.confirm && heldAck.value && reviewAck.value && remainingAck.value && !busy.value && !mappingDirty.value)
const resultVersion = computed(() => current.value ? JSON.stringify([current.value.state, current.value.phase, current.value.updated_at, current.value.counts.imported_people, current.value.counts.imported_contacts, current.value.counts.pending_people]) : '')
const awaitingLabel = computed(() => intent.value?.kind === 'confirm' ? 'Retry confirmation request' : intent.value?.kind === 'replan' ? 'Retry mapping request' : intent.value?.kind === 'plan' ? 'Retry planning request' : intent.value?.kind === 'cancel' ? 'Retry cancellation request' : 'Retry resume request')
watch(listing.data, value => { if (!selectedId.value && access.enabled.value && value?.imports.length) selectedId.value = value.imports[0]!.id })
watch(sources.data, value => { if (!sourceId.value && access.enabled.value && value?.snapshots.length) sourceId.value = value.latest_completed_snapshot_id ?? value.snapshots[0]!.id })
watch(source.data, value => { if (!previewId.value && access.enabled.value && value?.snapshot.preview_ids.length) previewId.value = value.snapshot.preview_ids[0]! })
watch(sourceId, () => { previewId.value = '' }, { flush: 'sync' })
watch(selectedId, () => { selectionGeneration++; confirmation.value = null; cancelReview.value = false; heldAck.value = false; reviewAck.value = false; remainingAck.value = false; readMode.value = 'plan' }, { flush: 'sync' })
watch(() => current.value?.confirmed_plan_id, value => { if (value) readMode.value = 'results' })
watch(() => current.value?.plan.id, () => { confirmation.value = null; heldAck.value = false; reviewAck.value = false; remainingAck.value = false })
watch(mappingDirty, value => { if (value) confirmation.value = null }, { flush: 'sync' })
watch(access.identity, () => {
  identityGeneration++; intent.value = null; uncertain.value = false; pending.value = false; actionError.value = ''; selectedId.value = ''; sourceId.value = ''; previewId.value = ''; importCursors.value = ['']; sourceCursors.value = ['']
}, { flush: 'sync' })
watch(access.scope, () => {
  confirmation.value = null; cancelReview.value = false; heldAck.value = false; reviewAck.value = false; remainingAck.value = false
  // Frozen request inputs may be replayed by this same verified identity.
  // Ordinary choices and read data are discarded by their workspace scope.
}, { flush: 'sync' })
watch([listing.error, detail.error, sources.error, source.error, preview.error], errors => {
  if (errors.some(importAccessError)) void props.refreshWorkspace().catch(() => {})
})
onBeforeUnmount(() => { disposed = true; intent.value = null; access.remove(access.prefix.value) })
function refresh() { if (access.enabled.value) void access.client.invalidateQueries({ queryKey: access.prefix.value }) }
function refreshBudget() { if (access.enabled.value) { void budget.refetch(); void detail.refetch() } }
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const expected = access.scope.value; const generation = selectionGeneration; const actorGeneration = identityGeneration
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''
  const sameIdentity = () => !disposed && actorGeneration === identityGeneration && value.identity === access.identity.value
  const valid = () => sameIdentity() && access.enabled.value && expected === access.scope.value && generation === selectionGeneration
  try {
    if (value.kind === 'confirm') {
      await confirmImport(value.id, value.body)
      if (!sameIdentity()) return
      intent.value = null; uncertain.value = false; confirmation.value = null; pending.value = false
      await props.refreshWorkspace()
      if (sameIdentity()) refresh()
      return
    }
    const result = value.kind === 'plan' ? await proposeImport(value.body) : value.kind === 'replan' ? await replanImport(value.id, value.body) : value.kind === 'retry' ? await retryImport(value.id, value.body.request_id) : await cancelImport(value.id, value.body.request_id)
    if (!valid()) { if (sameIdentity()) { uncertain.value = true; actionError.value = 'Workspace access changed. Retry the same reviewed request to recover its outcome.' }; return }
    intent.value = null; uncertain.value = false; cancelReview.value = false
    if ('import_id' in result) selectedId.value = result.import_id
    refresh()
  } catch (error) {
    if (!sameIdentity()) return
    confirmation.value = null; cancelReview.value = false
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; await props.refreshWorkspace().catch(() => {}); return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The result could not be confirmed. Retry the same reviewed request to recover its outcome.'
      : error instanceof ApiError && error.code === 'workspace_not_empty' ? 'This Organization already has business records. Use a new, empty Organization.'
        : error instanceof ApiError && error.status === 409 ? 'The import changed. Refresh its status and review the available actions.'
          : describeApiError(error, 'Could not complete this import action.')
    refresh()
  } finally { if (sameIdentity()) pending.value = false }
}
function plan() { if (eligibleSource.value && !busy.value) void submit({ kind: 'plan', identity: access.identity.value, body: { request_id: crypto.randomUUID(), snapshot_id: sourceId.value, preview_id: previewId.value } }) }
function apply(patches: ImportPatches) {
  const c = current.value
  if (!c?.actions.replan || busy.value || patches.stage_mappings.length + patches.assignee_mappings.length > 50) return
  void submit({ kind: 'replan', identity: access.identity.value, id: c.id, body: { request_id: crypto.randomUUID(), expected_plan_revision: c.plan.revision, ...patches } })
}
function freshPlan() { if (!mappingDirty.value) apply({ stage_mappings: [], assignee_mappings: [] }) }
function reviewConfirm() {
  const c = current.value
  if (!canConfirm.value || !c?.plan.confirmation_digest) return
  confirmation.value = { request_id: crypto.randomUUID(), plan_id: c.plan.id, plan_revision: c.plan.revision, confirmation_digest: c.plan.confirmation_digest, acknowledgments: { held_count: c.counts.held_people, review_only: true, remaining_data: true } }
}
function confirm() { if (confirmation.value && current.value && canConfirm.value) void submit({ kind: 'confirm', identity: access.identity.value, id: current.value.id, body: confirmation.value }) }
function resume() { if (current.value?.actions.retry && !busy.value) void submit({ kind: 'retry', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID() } }) }
function cancel() { if (current.value?.actions.cancel && !busy.value) void submit({ kind: 'cancel', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID() } }) }
const confirmationText = computed(() => current.value ? `Create ${current.value.counts.eligible_people} separate People and ${current.value.counts.contacts} contact methods using plan revision ${current.value.plan.revision}. ${current.value.counts.held_people} People remain held. The Organization will stay in administrator review; normal agent use and outbound actions remain unavailable. Other source data is not imported.` : '')
</script>
<template>
  <div
    class="min-w-0 space-y-5"
    data-testid="people-import-panel"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 class="text-section font-semibold text-text">
            People import
          </h2><p class="mt-1 text-small text-text-muted">
            Review retained source records before creating separate People in a new, empty Organization.
          </p>
        </div><button
          v-if="access.enabled.value"
          type="button"
          :class="buttonClasses('secondary')"
          @click="refresh"
        >
          Refresh import status
        </button>
      </div>
      <p
        v-if="!access.enabled.value"
        role="status"
        class="mt-3 text-small text-text-muted"
      >
        Verifying administrator access…
      </p>
      <template v-else>
        <p
          v-if="actionError"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ actionError }}
        </p>
        <button
          v-if="uncertain && intent"
          type="button"
          :class="buttonClasses('primary')"
          :disabled="pending"
          class="mt-3"
          @click="submit(intent)"
        >
          {{ awaitingLabel }}
        </button>
        <p
          v-if="listing.error.value || detail.error.value || sources.error.value || source.error.value || preview.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(listing.error.value ?? detail.error.value ?? sources.error.value ?? source.error.value ?? preview.error.value, 'Could not load import review. Refresh to recover.') }}
        </p>
        <div
          v-if="access.org.value?.workspace_mode === 'operational'"
          class="mt-4 grid gap-3 sm:grid-cols-2"
        >
          <FormField
            v-slot="{ id }"
            label="Retained snapshot"
            bare
          >
            <select
              :id="id"
              v-model="sourceId"
              :class="INPUT_CLASSES"
              :disabled="busy"
            >
              <option value="">
                Choose a retained snapshot
              </option><option
                v-for="s in sources.data.value?.snapshots ?? []"
                :key="s.id"
                :value="s.id"
              >
                {{ snapshotTime(s.created_at) }} · {{ snapshotLabel(s.state) }} · {{ s.id }}
              </option>
            </select>
          </FormField>
          <FormField
            v-slot="{ id }"
            label="Completed preview"
            bare
          >
            <select
              :id="id"
              v-model="previewId"
              :class="INPUT_CLASSES"
              :disabled="busy"
            >
              <option value="">
                Choose a retained preview
              </option><option
                v-for="previewOption in source.data.value?.snapshot.preview_ids ?? []"
                :key="previewOption"
                :value="previewOption"
              >
                {{ previewOption }}
              </option>
            </select>
          </FormField>
          <div class="flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="sourceCursors.length < 2 || busy"
              @click="sourceCursors.pop()"
            >
              Newer snapshots
            </button><button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="!sources.data.value?.next_cursor || busy"
              @click="sources.data.value?.next_cursor && sourceCursors.push(sources.data.value.next_cursor)"
            >
              Older snapshots
            </button>
          </div>
          <div>
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="!eligibleSource || busy"
              @click="plan"
            >
              Plan People import
            </button><p
              v-if="!eligibleSource"
              class="mt-2 text-small text-text-muted"
            >
              Choose a final snapshot with completed People, users and stages, and a completed preview at the same captured boundary.
            </p>
          </div>
        </div>
        <div class="mt-4">
          <FormField
            v-slot="{ id }"
            label="Existing People import"
            bare
          >
            <select
              :id="id"
              v-model="selectedId"
              :class="INPUT_CLASSES"
              :disabled="busy"
            >
              <option value="">
                Choose an import
              </option><option
                v-for="run in listing.data.value?.imports ?? []"
                :key="run.id"
                :value="run.id"
              >
                {{ snapshotTime(run.created_at) }} · {{ snapshotLabel(run.state) }} · {{ run.id }}
              </option>
            </select>
          </FormField><div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="importCursors.length < 2 || busy"
              @click="importCursors.pop()"
            >
              Newer imports
            </button><button
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="!listing.data.value?.next_cursor || busy"
              @click="listing.data.value?.next_cursor && importCursors.push(listing.data.value.next_cursor)"
            >
              Older imports
            </button>
          </div>
        </div>
        <p
          v-if="detail.isFetching.value"
          role="status"
          class="mt-3 text-small text-text-muted"
        >
          Loading import status…
        </p>
        <template v-if="current">
          <div class="mt-4 border-t border-border pt-4">
            <h3 class="text-body font-medium">
              {{ snapshotLabel(current.state) }} · {{ snapshotLabel(current.plan.state === 'building' ? current.plan.phase : current.phase) }}
            </h3><p class="mt-1 break-all text-small text-text-muted">
              Plan revision {{ current.plan.revision }} · FUB account {{ current.source_account_id }} · Captured boundary {{ current.capture_sequence }}
            </p><p
              v-if="current.pause_reason"
              role="status"
              class="mt-2 text-small text-danger"
            >
              Paused: {{ snapshotLabel(current.pause_reason) }}. Resolve the cause, then resume explicitly.
            </p><p
              v-if="current.plan.expired"
              class="mt-2 text-small text-danger"
            >
              This confirmation has expired. Prepare a fresh plan revision before confirming.
            </p>
          </div>
          <dl class="mt-3 grid grid-cols-2 gap-3 text-small sm:grid-cols-3">
            <div>
              <dt class="text-text-muted">
                Source People
              </dt><dd>{{ current.counts.source_people }}</dd>
            </div><div>
              <dt class="text-text-muted">
                People to create
              </dt><dd>{{ current.counts.eligible_people }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Held People
              </dt><dd>{{ current.counts.held_people }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Contacts to create
              </dt><dd>{{ current.counts.contacts }}</dd>
            </div><div>
              <dt class="text-text-muted">
                People sharing contacts
              </dt><dd>{{ current.counts.overlap_people }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Stages to create
              </dt><dd>{{ current.counts.stages_to_create }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Assigned / unassigned
              </dt><dd>{{ current.counts.assigned_people }} / {{ current.counts.unassigned_people }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Imported People / contacts
              </dt><dd>{{ current.counts.imported_people }} / {{ current.counts.imported_contacts }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Pending People
              </dt><dd>{{ current.counts.pending_people }}</dd>
            </div><div>
              <dt class="text-text-muted">
                Invalid-ID observations
              </dt><dd>{{ current.counts.invalid_ids }}</dd>
            </div>
          </dl>
          <p class="mt-3 text-small text-text-muted">
            Source observations: {{ snapshotTime(current.source_window.first_observed_at) }} to {{ snapshotTime(current.source_window.last_observed_at) }}.
          </p>
          <p class="mt-2 text-small text-text-muted">
            Not imported: {{ current.coverage.remaining_families.map(snapshotLabel).join(', ') }}. Source evidence remains retained. This is not a completed cutover.
          </p>
          <ul
            v-if="Object.keys(current.counts.reasons).length"
            class="mt-3 text-small"
          >
            <li
              v-for="(count, reason) in current.counts.reasons"
              :key="reason"
            >
              {{ snapshotLabel(reason) }}: {{ count }}
            </li>
          </ul>
          <div class="mt-3 rounded-lg bg-surface-1 p-3 text-small">
            <p>Required reservation: {{ formatBytes(current.plan.required_reservation_bytes) }} · Work-unit limit: {{ formatBytes(current.policy.unit_ceiling_bytes) }}</p><p>Import retained / reserved: {{ formatBytes(current.retained_bytes) }} / {{ formatBytes(current.reserved_bytes) }}. Cancellation reserve: {{ formatBytes(current.cancellation_reserved_bytes) }} (included).</p><p>Run allowance / ceiling: {{ formatBytes(current.policy.run_byte_limit) }} / {{ formatBytes(current.policy.run_ceiling_bytes) }}. Organization allowance / ceiling: {{ formatBytes(current.policy.org_byte_limit) }} / {{ formatBytes(current.policy.org_ceiling_bytes) }}.</p><p class="mt-1 text-text-muted">
              These are retained-data allowances, not physical disk quotas. Increasing an allowance does not resume the import or fix an item that exceeds the work-unit limit.
            </p>
          </div>
          <SnapshotBudgetPanel
            v-if="budget.data.value"
            :key="JSON.stringify(access.prefix.value) + current.snapshot_id"
            :snapshot="budget.data.value.snapshot"
            @refresh="refreshBudget"
            @access-denied="refreshWorkspace"
          />
          <div
            v-if="current.actions.confirm && current.release_ready && !busy"
            class="mt-4 space-y-3"
          >
            <label class="flex items-start gap-2 text-small"><input
              v-model="heldAck"
              type="checkbox"
              class="mt-1 size-4 accent-accent"
            >I acknowledge {{ current.counts.held_people }} held People will not be imported.</label>
            <label class="flex items-start gap-2 text-small"><input
              v-model="reviewAck"
              type="checkbox"
              class="mt-1 size-4 accent-accent"
            >Keep this Organization for administrator review only.</label>
            <label class="flex items-start gap-2 text-small"><input
              v-model="remainingAck"
              type="checkbox"
              class="mt-1 size-4 accent-accent"
            >I acknowledge the remaining data is not imported.</label>
            <button
              type="button"
              :class="buttonClasses('primary')"
              :disabled="!canConfirm"
              @click="reviewConfirm"
            >
              Review People import
            </button><p
              v-if="mappingDirty"
              role="status"
              class="text-small text-text-muted"
            >
              Apply or discard your mapping changes before confirming this import.
            </p><p class="text-small text-text-muted">
              Confirmation expires {{ snapshotTime(current.plan.expires_at) }}.
            </p>
          </div>
          <p
            v-if="!current.release_ready && !current.confirmed_plan_id"
            class="mt-3 text-small text-text-muted"
          >
            Import confirmation is waiting for the operator's release-readiness check.
          </p>
          <div class="mt-4 flex flex-wrap gap-2">
            <button
              v-if="current.actions.replan"
              type="button"
              :class="buttonClasses('secondary')"
              :disabled="busy || mappingDirty"
              @click="freshPlan"
            >
              Prepare fresh plan revision
            </button><button
              v-if="current.actions.retry"
              type="button"
              :class="buttonClasses('primary')"
              :disabled="busy"
              @click="resume"
            >
              Resume {{ current.confirmed_plan_id ? 'import' : 'preparation' }}
            </button><button
              v-if="current.actions.cancel"
              type="button"
              :class="buttonClasses('danger')"
              :disabled="busy"
              @click="cancelReview = true"
            >
              Cancel remaining import
            </button>
          </div>
          <p
            v-if="current.workspace.mode === 'migration_review'"
            class="mt-3 text-small text-text-muted"
          >
            Administrator review remains in place after completion or cancellation. Already committed People and stages are retained.
          </p>
          <div
            v-if="current.confirmed_plan_id"
            class="mt-4 flex flex-wrap gap-2"
          >
            <button
              type="button"
              :class="buttonClasses(readMode === 'results' ? 'primary' : 'secondary')"
              @click="readMode = 'results'"
            >
              View import results
            </button><button
              type="button"
              :class="buttonClasses(readMode === 'plan' ? 'primary' : 'secondary')"
              @click="readMode = 'plan'"
            >
              Review planned People
            </button>
          </div>
        </template>
      </template>
    </Card>
    <ImportMappingPanel
      v-if="current && current.plan.state !== 'building'"
      :import-id="current.id"
      :plan-id="current.plan.id"
      :revision="current.plan.revision"
      :can-replan="current.actions.replan && !busy"
      @apply="apply"
      @dirty="mappingDirty = $event"
    />
    <ImportRecordPanel
      v-if="current && current.plan.state !== 'building'"
      :key="`${current.id}:${current.plan.id}:${readMode}`"
      :import-id="current.id"
      :plan-id="current.plan.id"
      :mode="readMode"
      :result-version="resultVersion"
    />
    <ConfirmDialog
      :visible="!!confirmation && access.enabled.value && !mappingDirty"
      title="Confirm People import"
      :message="confirmationText"
      confirm-label="Confirm People import"
      :is-pending="pending"
      @update:visible="!$event && (confirmation = null)"
      @confirm="confirm"
    />
    <ConfirmDialog
      :visible="cancelReview && access.enabled.value"
      title="Cancel remaining import"
      message="Stop future import work. Already committed People, stages and retained evidence remain. Administrator review stays in place; this does not activate the Organization."
      confirm-label="Cancel remaining import"
      confirm-variant="danger"
      :is-pending="pending"
      @update:visible="cancelReview = $event"
      @confirm="cancel"
    />
  </div>
</template>
