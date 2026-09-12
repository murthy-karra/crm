<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import HistoryImportBudget from './HistoryImportBudget.vue'
import HistoryImportRecords from './HistoryImportRecords.vue'
import { ApiError } from '../../api/client'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { fetchHistoryCapture, fetchHistoryCaptures, historyLabel } from '../../api/historyCaptures'
import { cancelHistoryImport, confirmHistoryImport, fetchHistoryImport, fetchHistoryImports, historyImportActive, increaseHistoryImportBudget, prepareHistoryImport, resumeHistoryImport, useHistoryImportAccess, type HistoryImportAction, type HistoryImportBudget as Budget, type HistoryImportConfirm, type HistoryImportPrepare, type HistoryImportResume } from '../../api/historyImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'
const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
const access = useHistoryImportAccess()
const parentId = ref('')
const captureId = ref('')
const selectedId = ref('')
const parentCursors = ref<string[]>([''])
const captureCursors = ref<string[]>([''])
const importCursors = ref<string[]>([''])
const mode = ref<'records' | 'results'>('records')
const meaningAck = ref(false)
const datesAck = ref(false)
const coverageAck = ref(false)
const reviewAck = ref(false)
const pending = ref(false)
const uncertain = ref(false)
const actionError = ref('')
const notice = ref('')
type Intent = { kind: 'prepare'; identity: string; body: HistoryImportPrepare }
  | { kind: 'confirm'; identity: string; id: string; body: HistoryImportConfirm }
  | { kind: 'resume'; identity: string; id: string; body: HistoryImportResume }
  | { kind: 'cancel'; identity: string; id: string; body: HistoryImportAction }
  | { kind: 'budget'; identity: string; id: string; body: Budget }
const intent = ref<Intent | null>(null)
const review = ref<{ intent: Exclude<Intent, { kind: 'prepare' }>; message: string } | null>(null)
let disposed = false
let actorGeneration = 0
let selectionGeneration = 0
let authorityGeneration = 0
const parentsKey = computed(() => [...access.prefix.value, 'parents', parentCursors.value.at(-1) ?? ''])
const parentKey = computed(() => [...access.prefix.value, 'parent', parentId.value])
const capturesKey = computed(() => [...access.prefix.value, 'captures', parentId.value, captureCursors.value.at(-1) ?? ''])
const captureKey = computed(() => [...access.prefix.value, 'capture', captureId.value])
const listKey = computed(() => [...access.prefix.value, 'list', parentId.value, importCursors.value.at(-1) ?? ''])
const detailKey = computed(() => [...access.prefix.value, 'detail', selectedId.value])
const parents = useQuery({ queryKey: parentsKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentsKey.value, () => parentsKey.value, () => fetchImports(parentCursors.value.at(-1) || undefined, signal)) })
const parent = useQuery({ queryKey: parentKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentKey.value, () => parentKey.value, () => fetchImport(parentId.value, signal)) })
const captures = useQuery({ queryKey: capturesKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(capturesKey.value, () => capturesKey.value, () => fetchHistoryCaptures(parentId.value, captureCursors.value.at(-1) || undefined, signal)) })
const capture = useQuery({ queryKey: captureKey, enabled: computed(() => access.enabled.value && !!captureId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(captureKey.value, () => captureKey.value, () => fetchHistoryCapture(captureId.value, signal)) })
const listing = useQuery({ queryKey: listKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0,
  queryFn: ({ signal }) => access.read(listKey.value, () => listKey.value, () => fetchHistoryImports(parentId.value, importCursors.value.at(-1) || undefined, signal)),
  refetchInterval: query => query.state.data?.imports.some(historyImportActive) ? Math.min(30_000, 2_000 * 2 ** Math.min(query.state.fetchFailureCount, 4)) : false,
})
const detail = useQuery({ queryKey: detailKey, enabled: computed(() => access.enabled.value && !!selectedId.value), retry: false, gcTime: 0,
  queryFn: ({ signal }) => access.read(detailKey.value, () => detailKey.value, async () => {
    const scope = access.scope.value
    const value = await fetchHistoryImport(selectedId.value, signal)
    if (scope === access.scope.value && value.workspace_revision !== access.org.value?.workspace_revision) { await props.refreshWorkspace(); throw new Error('Workspace status was refreshed') }
    return value
  }),
  refetchInterval: query => historyImportActive(query.state.data) ? Math.min(30_000, 2_000 * 2 ** Math.min(query.state.fetchFailureCount, 4)) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const current = computed(() => access.enabled.value ? detail.data.value : undefined)
const busy = computed(() => pending.value || !!intent.value)
const anchored = computed(() => listing.data.value?.imports.find(run => !!run.confirmed_at))
const parentReady = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review' && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id)
const canPrepare = computed(() => parentReady.value && listing.isSuccess.value && capture.data.value?.state === 'completed_with_gaps' && capture.data.value.parent_import_id === parentId.value && !busy.value && !listing.data.value?.imports.some(historyImportActive) && (!anchored.value || anchored.value.capture_id === captureId.value) && (!current.value || current.value.actions.prepare_same_plan || !current.value.confirmed_at && current.value.state === 'cancelled'))
const canConfirm = computed(() => current.value?.actions.confirm && current.value.release_ready && !busy.value && meaningAck.value && datesAck.value && coverageAck.value && reviewAck.value)
const dialogTitle = computed(() => review.value?.intent.kind === 'confirm' ? 'Import historical records' : review.value?.intent.kind === 'budget' ? 'Increase import allowances' : review.value?.intent.kind === 'resume' ? 'Resume history import' : 'Cancel history import')
const dialogLabel = computed(() => review.value?.intent.kind === 'confirm' ? 'Import records' : review.value?.intent.kind === 'budget' ? 'Increase allowances' : review.value?.intent.kind === 'resume' ? 'Resume import' : 'Cancel import')
const pauseGuidance = computed(() => ({ storage_limit: 'Review the storage allowances. Increase them within the available ceilings, then Resume separately.', executor_not_authorized: 'A current administrator can review and Resume to take responsibility for the remaining work.', release_not_ready: 'An operator must restore compatible deployment readiness before this import can resume.', retained_integrity_failed: 'Retained evidence could not be verified. An operator must resolve the evidence or key problem before resuming.', source_binding_changed: 'The retained input or completed parent no longer matches. Resolve that binding before resuming; reconnecting the source does not repair retained evidence.', interpretation_bound_exceeded: 'Some retained evidence exceeds the supported interpretation size. The evidence remains retained; this needs a supported interpretation before continuing.' } as Record<string, string>)[current.value?.pause_reason ?? ''])
function clearReview() { review.value = null; meaningAck.value = false; datesAck.value = false; coverageAck.value = false; reviewAck.value = false }
watch(parents.data, value => { if (access.enabled.value && !parentId.value) parentId.value = value?.imports.find(run => run.state === 'completed')?.id ?? '' })
watch(captures.data, value => { if (access.enabled.value && !captureId.value) captureId.value = value?.captures.find(run => run.state === 'completed_with_gaps')?.id ?? '' })
watch(listing.data, value => { if (access.enabled.value && !selectedId.value) selectedId.value = value?.imports[0]?.id ?? '' })
watch(parentId, () => { selectionGeneration++; captureId.value = ''; selectedId.value = ''; captureCursors.value = ['']; importCursors.value = ['']; clearReview() }, { flush: 'sync' })
watch(captureId, () => { selectionGeneration++; clearReview() }, { flush: 'sync' })
watch(selectedId, () => { selectionGeneration++; mode.value = 'records'; clearReview(); notice.value = '' }, { flush: 'sync' })
watch(() => current.value?.confirmed_at, value => { if (value) mode.value = 'results' })
watch(() => current.value?.revision, clearReview)
watch([meaningAck, datesAck, coverageAck, reviewAck], () => { review.value = null }, { flush: 'sync' })
watch(access.scope, () => { authorityGeneration++; clearReview() }, { flush: 'sync' })
watch(access.identity, () => { actorGeneration++; intent.value = null; pending.value = false; uncertain.value = false; actionError.value = ''; notice.value = ''; parentId.value = ''; captureId.value = ''; selectedId.value = ''; parentCursors.value = [''] }, { flush: 'sync' })
watch(access.denied, value => { if (value) void props.refreshWorkspace().catch(() => {}) })
onBeforeUnmount(() => { disposed = true; intent.value = null; access.remove(access.prefix.value) })
function refreshStatus() { if (access.enabled.value) for (const key of [parentsKey.value, parentKey.value, capturesKey.value, captureKey.value, listKey.value, detailKey.value]) void access.client.invalidateQueries({ queryKey: key, exact: true }) }
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const actor = actorGeneration; const selection = selectionGeneration; const authority = authorityGeneration; const scope = access.scope.value
  const sameActor = () => !disposed && actor === actorGeneration && value.identity === access.identity.value
  const valid = () => sameActor() && access.enabled.value && selection === selectionGeneration && authority === authorityGeneration && scope === access.scope.value
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''; notice.value = ''
  try {
    const receipt = value.kind === 'prepare' ? await prepareHistoryImport(value.body) : value.kind === 'confirm' ? await confirmHistoryImport(value.id, value.body) : value.kind === 'resume' ? await resumeHistoryImport(value.id, value.body) : value.kind === 'cancel' ? await cancelHistoryImport(value.id, value.body) : await increaseHistoryImportBudget(value.id, value.body)
    if (!valid()) { if (sameActor()) { uncertain.value = true; review.value = null; actionError.value = 'Access changed while this request was pending. Retry the same reviewed request to recover its outcome.' }; return }
    selectedId.value = receipt.import_id; intent.value = null; uncertain.value = false; clearReview()
    if (value.kind === 'prepare') importCursors.value = ['']
    if (value.kind === 'budget') notice.value = 'Allowance increase confirmed. Use Resume separately to continue paused work.'
    refreshStatus()
  } catch (error) {
    if (!sameActor()) return
    review.value = null
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; access.denied.value = true; return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The result could not be confirmed. Retry the same reviewed history import request to recover its outcome.' : error instanceof ApiError && error.status === 409 ? 'The plan, retained input, readiness or allowance changed. Refresh and review the current status.' : describeApiError(error, 'Could not complete the history import action.')
    refreshStatus()
  } finally { if (sameActor()) pending.value = false }
}
function prepare() {
  const source = capture.data.value
  if (!canPrepare.value || !source || !access.org.value) return
  void submit({ kind: 'prepare', identity: access.identity.value, body: { request_id: crypto.randomUUID(), parent_import_id: parentId.value, capture_id: source.id, expected_capture_revision: source.revision, expected_workspace_revision: access.org.value.workspace_revision, expected_policy_revision: source.policy_revision } })
}
function reviewConfirm() {
  const run = current.value; if (!run || !canConfirm.value) return
  review.value = { intent: { kind: 'confirm', identity: access.identity.value, id: run.id, body: { request_id: crypto.randomUUID(), plan_id: run.plan_id, expected_revision: run.revision, expected_plan_revision: run.plan_revision, expected_workspace_revision: run.workspace_revision, expected_policy_revision: run.policy_revision, acknowledgements: { external_facts: true, date_uncertainty: true, coverage_and_holds: true, review_only: true } } }, message: `Import ${run.counts.eligible} eligible historical records from capture ${run.capture_id}. ${run.counts.held} occurrences stay held and ${run.counts.equal_repeats} are equal repeats. Plan ${run.plan_id}, revision ${run.plan_revision}; added-byte bound ${run.added_byte_bound}. These are FUB record facts, not native Inquiries, calls, contact attempts, messages or consent. Dates describe FUB record creation and may be unknown. Coverage remains incomplete; bodies are not exposed. The parent is permanently anchored to this capture and interpretation. Cancellation keeps committed facts; later attempts can finish only this same plan. The workspace remains in administrator review.` }
}
function reviewResume() { const run = current.value; if (run?.actions.resume && run.release_ready && !busy.value) review.value = { intent: { kind: 'resume', identity: access.identity.value, id: run.id, body: { request_id: crypto.randomUUID(), expected_revision: run.revision, expected_policy_revision: run.policy_revision } }, message: `Resume import ${run.id} at revision ${run.revision}. You become the responsible import executor for subsequent work. The frozen plan and historical source actors remain unchanged; committed facts are kept.` } }
function reviewCancel() { const run = current.value; if (run?.actions.cancel && !busy.value) review.value = { intent: { kind: 'cancel', identity: access.identity.value, id: run.id, body: { request_id: crypto.randomUUID(), expected_revision: run.revision } }, message: `Cancel attempt ${run.id} at revision ${run.revision}. Committed historical facts remain. This attempt cannot resume. If a plan was confirmed, another attempt may finish only the same retained capture and frozen plan. People and source captures are unchanged.` } }
function reviewBudget(value: { body: Budget; message: string }) { const run = current.value; if (run?.actions.increase_budget && !busy.value) review.value = { intent: { kind: 'budget', identity: access.identity.value, id: run.id, body: value.body }, message: value.message } }
function confirmReview() { const value = review.value; if (!value || busy.value || !access.enabled.value || value.intent.kind === 'confirm' && !canConfirm.value) return; void submit(value.intent) }
</script>
<template>
  <section
    class="mb-6 min-w-0 space-y-4"
    data-testid="history-import-panel"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h2 class="text-section font-medium">
            Import historical records
          </h2><p class="mt-1 text-small text-text-muted">
            Preview retained events, calls and texts for a completed People import. No source connection is needed.
          </p>
        </div><button
          v-if="access.enabled.value"
          type="button"
          :class="buttonClasses()"
          @click="refreshStatus"
        >
          Refresh import status
        </button>
      </div>
      <p
        v-if="!access.enabled.value"
        role="status"
        class="mt-3 text-small text-text-muted"
      >
        A verified administrator session is required.
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
          class="mt-3"
          :class="buttonClasses('primary')"
          :disabled="pending"
          @click="submit(intent)"
        >
          Retry same import request
        </button>
        <p
          v-if="notice"
          role="status"
          class="mt-3 text-small"
        >
          {{ notice }}
        </p>
        <p
          v-if="parents.error.value || parent.error.value || captures.error.value || capture.error.value || listing.error.value || detail.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(parents.error.value ?? parent.error.value ?? captures.error.value ?? capture.error.value ?? listing.error.value ?? detail.error.value, 'Could not load history import review. Refresh to retry.') }}
        </p>
        <FormField
          v-slot="{ id }"
          label="People import for historical records"
          bare
          class="mt-4"
        >
          <select
            :id="id"
            v-model="parentId"
            :class="INPUT_CLASSES"
            :disabled="busy"
          >
            <option value="">
              Choose a completed People import
            </option><option
              v-for="run in parents.data.value?.imports ?? []"
              :key="run.id"
              :value="run.id"
              :disabled="run.state !== 'completed'"
            >
              {{ snapshotTime(run.created_at) }} · {{ snapshotLabel(run.state) }} · {{ run.id }}
            </option><option
              v-if="parent.data.value && !parents.data.value?.imports.some(run => run.id === parentId)"
              :value="parentId"
            >
              {{ parentId }}
            </option>
          </select>
        </FormField>
        <div class="mt-2 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || parents.isFetching.value || parentCursors.length < 2"
            @click="parentCursors.pop()"
          >
            Previous parents
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || parents.isFetching.value || !parents.data.value?.next_cursor"
            @click="parents.data.value?.next_cursor && parentCursors.push(parents.data.value.next_cursor)"
          >
            More parents
          </button>
        </div>
        <template v-if="parentId">
          <FormField
            v-slot="{ id }"
            label="Retained historical capture to import"
            bare
            class="mt-3"
          >
            <select
              :id="id"
              v-model="captureId"
              :class="INPUT_CLASSES"
              :disabled="busy"
            >
              <option value="">
                Choose a completed capture
              </option><option
                v-for="run in captures.data.value?.captures ?? []"
                :key="run.id"
                :value="run.id"
                :disabled="run.state !== 'completed_with_gaps'"
              >
                {{ snapshotTime(run.created_at) }} · {{ snapshotLabel(run.state) }} · {{ run.id }}
              </option><option
                v-if="capture.data.value && !captures.data.value?.captures.some(run => run.id === captureId)"
                :value="captureId"
              >
                {{ captureId }}
              </option>
            </select>
          </FormField>
          <div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || captures.isFetching.value || captureCursors.length < 2"
              @click="captureCursors.pop()"
            >
              Previous captures
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || captures.isFetching.value || !captures.data.value?.next_cursor"
              @click="captures.data.value?.next_cursor && captureCursors.push(captures.data.value.next_cursor)"
            >
              More captures
            </button>
          </div>
          <p
            v-if="anchored"
            class="mt-2 break-all text-small text-text-muted"
          >
            This parent is anchored to capture {{ anchored.capture_id }}. A new attempt can continue only that frozen plan.
          </p>
          <button
            type="button"
            class="mt-3"
            :class="buttonClasses('primary')"
            :disabled="!canPrepare"
            @click="prepare"
          >
            Prepare history import
          </button>
          <p class="mt-2 text-small text-text-muted">
            Preparation reads retained evidence and builds a complete preview. Importing requires separate confirmation. To continue a cancelled attempt, select its same capture and prepare again.
          </p>
          <FormField
            v-slot="{ id }"
            label="Historical import attempt"
            bare
            class="mt-4"
          >
            <select
              :id="id"
              v-model="selectedId"
              :class="INPUT_CLASSES"
              :disabled="busy"
            >
              <option value="">
                Choose an attempt
              </option><option
                v-for="run in listing.data.value?.imports ?? []"
                :key="run.id"
                :value="run.id"
              >
                {{ snapshotTime(run.created_at) }} · {{ snapshotLabel(run.state) }} · {{ run.id }}
              </option><option
                v-if="current && !listing.data.value?.imports.some(run => run.id === selectedId)"
                :value="selectedId"
              >
                {{ selectedId }}
              </option>
            </select>
          </FormField>
          <div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || listing.isFetching.value || importCursors.length < 2"
              @click="importCursors.pop()"
            >
              Previous attempts
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || listing.isFetching.value || !listing.data.value?.next_cursor"
              @click="listing.data.value?.next_cursor && importCursors.push(listing.data.value.next_cursor)"
            >
              More attempts
            </button>
          </div>
        </template>
      </template>
    </Card>
    <Card
      v-if="current"
      class="min-w-0"
    >
      <h3 class="text-section font-medium">
        History import · {{ snapshotLabel(current.state) }}
      </h3>
      <p class="mt-2 break-all text-small text-text-muted">
        Attempt {{ current.id }} · plan {{ current.plan_id }} · revision {{ current.revision }}
      </p>
      <p class="mt-1 text-small text-text-muted">
        {{ snapshotLabel(current.phase) }} · {{ current.preview_complete ? 'Preview complete' : 'Preview building' }}<span v-if="current.plan_expires_at && !current.confirmed_at"> · Confirm before {{ snapshotTime(current.plan_expires_at) }}</span>
      </p>
      <p
        v-if="current.pause_reason"
        role="status"
        class="mt-3 text-small"
      >
        {{ snapshotLabel(current.pause_reason) }}
      </p>
      <p
        v-if="pauseGuidance"
        class="mt-2 text-small text-text-muted"
      >
        {{ pauseGuidance }}
      </p>
      <p
        v-if="current.preview_complete && current.counts.eligible === '0'"
        class="mt-2 text-small text-text-muted"
      >
        No records are eligible for confirmation. Review the held outcomes and retained coverage before choosing another input.
      </p>
      <dl class="mt-4 grid gap-3 text-small sm:grid-cols-2 lg:grid-cols-4">
        <div>
          <dt class="text-text-muted">
            Retained occurrences
          </dt><dd>{{ current.counts.occurrences }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Eligible distinct records
          </dt><dd>{{ current.counts.eligible }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Equal repeats / held
          </dt><dd>{{ current.counts.equal_repeats }} / {{ current.counts.held }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Added-byte bound
          </dt><dd>{{ formatBytes(current.added_byte_bound) }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Processed this attempt
          </dt><dd>{{ current.counts.processed }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Inserted this attempt
          </dt><dd>{{ current.counts.inserted }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Already imported
          </dt><dd>{{ current.counts.already_imported }}</dd>
        </div><div>
          <dt class="text-text-muted">
            Held during application
          </dt><dd>{{ current.counts.application_held }}</dd>
        </div>
      </dl>
      <p class="mt-3 text-small text-text-muted">
        Completion reconciles this plan. Restricted or inaccessible account history remains unknown; native Inquiries, calls, messages, consent and Today are unchanged.
      </p>
      <ul class="mt-2 space-y-1 text-small text-text-muted">
        <li
          v-for="warning in current.coverage.warnings"
          :key="warning"
        >
          {{ warning === 'external_facts_only' ? 'Imported as external record facts only' : historyLabel(warning) }}
        </li><li
          v-for="stream in current.coverage.streams"
          :key="stream.family"
        >
          {{ historyLabel(stream.family) }} · {{ stream.occurrences }} occurrences · {{ stream.unique_ids }} distinct source IDs · {{ historyLabel(stream.state) }}
        </li>
      </ul>
      <div
        v-if="current.actions.confirm"
        class="mt-4 space-y-2 text-small"
      >
        <label class="flex min-h-10 items-center gap-2"><input
          v-model="meaningAck"
          type="checkbox"
          :disabled="busy"
        >These are external FUB record facts, with no native or Today effects.</label><label class="flex min-h-10 items-center gap-2"><input
          v-model="datesAck"
          type="checkbox"
          :disabled="busy"
        >FUB record-created dates may be unknown and do not prove event occurrence.</label><label class="flex min-h-10 items-center gap-2"><input
          v-model="coverageAck"
          type="checkbox"
          :disabled="busy"
        >I reviewed the coverage gaps, held records and frozen counts.</label><label class="flex min-h-10 items-center gap-2"><input
          v-model="reviewAck"
          type="checkbox"
          :disabled="busy"
        >Metadata stays in administrator review; this does not activate the workspace.</label>
      </div>
      <p
        v-if="!current.release_ready && ['ready', 'paused'].includes(current.state)"
        class="mt-3 text-small text-text-muted"
      >
        Compatible deployment readiness is required before confirming or resuming.
      </p>
      <div class="mt-3 flex flex-wrap gap-2">
        <button
          v-if="current.actions.confirm"
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!canConfirm"
          @click="reviewConfirm"
        >
          Review confirmation
        </button><button
          v-if="current.actions.resume"
          type="button"
          :class="buttonClasses('primary')"
          :disabled="busy || !current.release_ready"
          @click="reviewResume"
        >
          Resume history import
        </button><button
          v-if="current.actions.cancel"
          type="button"
          :class="buttonClasses()"
          :disabled="busy"
          @click="reviewCancel"
        >
          Cancel history import
        </button>
      </div>
      <HistoryImportBudget
        :run="current"
        :disabled="busy"
        @review="reviewBudget"
        @change="review = null"
      />
    </Card>
    <template v-if="current?.preview_complete">
      <div
        class="flex flex-wrap gap-2"
        aria-label="Historical record view"
      >
        <button
          type="button"
          :class="buttonClasses(mode === 'records' ? 'primary' : 'secondary')"
          :aria-pressed="mode === 'records'"
          @click="mode = 'records'"
        >
          Planned records
        </button><button
          v-if="current.confirmed_at"
          type="button"
          :class="buttonClasses(mode === 'results' ? 'primary' : 'secondary')"
          :aria-pressed="mode === 'results'"
          @click="mode = 'results'"
        >
          Attempt results
        </button>
      </div>
      <HistoryImportRecords
        :key="`${current.id}:${current.plan_id}:${mode}`"
        :import-id="current.id"
        :plan-id="current.plan_id"
        :current-revision="current.revision"
        :mode="mode"
      />
    </template>
    <ConfirmDialog
      scrollable
      :visible="!!review && access.enabled.value"
      :title="dialogTitle"
      :message="review?.message ?? ''"
      :confirm-label="dialogLabel"
      :is-pending="pending"
      @update:visible="value => !value && (review = null)"
      @confirm="confirmReview"
    />
  </section>
</template>
