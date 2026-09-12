<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import HistoryCaptureBudget from './HistoryCaptureBudget.vue'
import HistoryCaptureRecords from './HistoryCaptureRecords.vue'
import { ApiError } from '../../api/client'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { cancelHistoryCapture, confirmHistoryCapture, fetchHistoryCapture, fetchHistoryCaptures, historyActive, historyLabel, increaseHistoryBudget, proposeHistoryCapture, retryHistoryCapture, useHistoryAccess, type HistoryAction, type HistoryBudget, type HistoryConfirmation, type HistoryProposal } from '../../api/historyCaptures'
import type { FubConnection } from '../../api/migrations'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'
const props = defineProps<{ connection: FubConnection | null; refreshWorkspace: () => Promise<void>; sourceBusy?: boolean }>()
const emit = defineEmits<{ sourceBusy: [value: boolean] }>()
const access = useHistoryAccess()
const parentId = ref('')
const selectedId = ref('')
const parentCursors = ref<string[]>([''])
const runCursors = ref<string[]>([''])
const scopeAck = ref(false)
const gapsAck = ref(false)
const retainedAck = ref(false)
const differenceAck = ref(false)
const pending = ref(false)
const uncertain = ref(false)
const actionError = ref('')
const notice = ref('')
const failures = ref(0)
type Intent = { kind: 'propose'; identity: string; body: HistoryProposal }
  | { kind: 'confirm'; identity: string; id: string; body: HistoryConfirmation }
  | { kind: 'retry'; identity: string; id: string; body: HistoryAction }
  | { kind: 'cancel'; identity: string; id: string; body: HistoryAction }
  | { kind: 'budget'; identity: string; id: string; body: HistoryBudget }
const intent = ref<Intent | null>(null)
const review = ref<{ intent: Exclude<Intent, { kind: 'propose' }>; message: string } | null>(null)
let disposed = false
let identityGeneration = 0
let selectionGeneration = 0
let scopeGeneration = 0
const parentsKey = computed(() => [...access.prefix.value, 'parents', parentCursors.value.at(-1) ?? ''])
const parentKey = computed(() => [...access.prefix.value, 'parent', parentId.value])
const listKey = computed(() => [...access.prefix.value, 'list', parentId.value, runCursors.value.at(-1) ?? ''])
const detailKey = computed(() => [...access.prefix.value, 'detail', selectedId.value])
const parents = useQuery({ queryKey: parentsKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentsKey.value, () => parentsKey.value, () => fetchImports(parentCursors.value.at(-1) || undefined, signal)) })
const parent = useQuery({ queryKey: parentKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentKey.value, () => parentKey.value, () => fetchImport(parentId.value, signal)) })
const listing = useQuery({ queryKey: listKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0,
  queryFn: ({ signal }) => access.read(listKey.value, () => listKey.value, () => fetchHistoryCaptures(parentId.value, runCursors.value.at(-1) || undefined, signal)),
  // A selected terminal run must not leave another loaded run's busy status
  // stale. Refresh only this bounded list while it contains active source work.
  refetchInterval: query => query.state.data?.captures.some(historyActive) ? Math.min(30_000, 2_000 * 2 ** Math.min(query.state.fetchFailureCount, 4)) : false,
})
const detail = useQuery({ queryKey: detailKey, enabled: computed(() => access.enabled.value && !!selectedId.value), retry: false, gcTime: 0,
  queryFn: async ({ signal }) => {
    const scope = access.scope.value
    try { const result = await access.read(detailKey.value, () => detailKey.value, () => fetchHistoryCapture(selectedId.value, signal)); failures.value = 0; return result }
    catch (error) { if (scope === access.scope.value && !signal.aborted) failures.value++; throw error }
  },
  refetchInterval: query => historyActive(query.state.data) ? Math.min(30_000, 2_000 * 2 ** Math.min(failures.value, 4)) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const current = computed(() => access.enabled.value ? detail.data.value : undefined)
const busy = computed(() => pending.value || !!intent.value)
const eligibleParent = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review' && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id)
const connected = computed(() => props.connection?.status === 'connected')
const sourceBound = computed(() => connected.value && !!current.value && current.value.initiated_by_user_id === access.me.value?.user.id && current.value.connection_id === props.connection?.id && current.value.connection_revision === String(props.connection?.revision))
const canPropose = computed(() => eligibleParent.value && connected.value && !busy.value)
const canConfirm = computed(() => sourceBound.value && current.value?.actions.confirm && current.value.release_ready && !props.sourceBusy && !busy.value && scopeAck.value && gapsAck.value && retainedAck.value && (!current.value.source_user_difference || differenceAck.value))
const canResume = computed(() => sourceBound.value && current.value?.actions.retry && current.value.release_ready && !props.sourceBusy && !busy.value)
const dialogTitle = computed(() => review.value?.intent.kind === 'confirm' ? 'Start historical capture' : review.value?.intent.kind === 'budget' ? 'Increase historical capture allowances' : 'Cancel historical capture')
const dialogLabel = computed(() => review.value?.intent.kind === 'confirm' ? 'Start historical capture' : review.value?.intent.kind === 'budget' ? 'Increase allowances' : 'Permanently cancel historical capture')
const counters = ['occurrences', 'valid_occurrences', 'invalid_occurrences', 'unique_ids', 'equal_repeats', 'conflicting_variants', 'linked', 'parent_excluded', 'no_parent_identity', 'invalid_person_reference', 'conflicting_reference'] as const
const counterLabels: Record<typeof counters[number], string> = { occurrences: 'Advancing occurrences', valid_occurrences: 'Valid ID occurrences', invalid_occurrences: 'Invalid ID occurrences', unique_ids: 'Distinct source IDs', equal_repeats: 'Equal repeats', conflicting_variants: 'Conflicting variants', linked: 'Linked to parent', parent_excluded: 'Parent excluded / held', no_parent_identity: 'No parent identity', invalid_person_reference: 'Invalid / missing Person reference', conflicting_reference: 'Conflicting Person reference' }
function clearReview() { review.value = null; scopeAck.value = false; gapsAck.value = false; retainedAck.value = false; differenceAck.value = false }
watch(parents.data, value => { if (access.enabled.value && !parentId.value) parentId.value = value?.imports.find(value => value.state === 'completed')?.id ?? '' })
watch(listing.data, value => { if (access.enabled.value && !selectedId.value) selectedId.value = value?.captures[0]?.id ?? '' })
watch(parentId, () => { selectionGeneration++; selectedId.value = ''; runCursors.value = ['']; clearReview() }, { flush: 'sync' })
watch(selectedId, () => { selectionGeneration++; failures.value = 0; clearReview(); notice.value = '' }, { flush: 'sync' })
watch(() => current.value?.revision, clearReview)
watch(() => JSON.stringify([props.connection?.id, props.connection?.revision, props.connection?.status]), clearReview, { flush: 'sync' })
watch([scopeAck, gapsAck, retainedAck, differenceAck], () => { review.value = null }, { flush: 'sync' })
watch(access.scope, () => { scopeGeneration++; clearReview() }, { flush: 'sync' })
watch(access.identity, () => { identityGeneration++; intent.value = null; pending.value = false; uncertain.value = false; actionError.value = ''; notice.value = ''; parentId.value = ''; selectedId.value = ''; parentCursors.value = ['']; runCursors.value = [''] }, { flush: 'sync' })
watch([parents.error, parent.error, listing.error, detail.error], errors => { if (errors.some(importAccessError)) void props.refreshWorkspace().catch(() => {}) })
watch(() => access.enabled.value && (historyActive(current.value) || (listing.data.value?.captures ?? []).some(value => value.id !== selectedId.value && historyActive(value))), value => emit('sourceBusy', value), { immediate: true })
onBeforeUnmount(() => { disposed = true; intent.value = null; access.remove(access.prefix.value); emit('sourceBusy', false) })
function refreshStatus() {
  if (!access.enabled.value) return
  // Do not invalidate observation queries: their immutable page series is owned
  // by an explicit Refresh captured observations action.
  for (const key of [parentsKey.value, parentKey.value, listKey.value, detailKey.value]) void access.client.invalidateQueries({ queryKey: key, exact: true })
}
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const actorGeneration = identityGeneration; const selection = selectionGeneration; const scope = access.scope.value; const authorityGeneration = scopeGeneration
  const sameIdentity = () => !disposed && actorGeneration === identityGeneration && value.identity === access.identity.value
  const valid = () => sameIdentity() && access.enabled.value && authorityGeneration === scopeGeneration && scope === access.scope.value && selection === selectionGeneration
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''; notice.value = ''
  try {
    const receipt = value.kind === 'propose' ? await proposeHistoryCapture(value.body) : value.kind === 'confirm' ? await confirmHistoryCapture(value.id, value.body) : value.kind === 'retry' ? await retryHistoryCapture(value.id, value.body) : value.kind === 'cancel' ? await cancelHistoryCapture(value.id, value.body) : await increaseHistoryBudget(value.id, value.body)
    if (!valid()) { if (sameIdentity()) { uncertain.value = true; review.value = null; actionError.value = 'Access changed while this request was pending. Retry the same reviewed request to recover its outcome.' }; return }
    // The compact receipt never supplies current state. Select its local run and
    // obtain the current detail via GET even when replay returned an old receipt.
    selectedId.value = receipt.capture_id; intent.value = null; uncertain.value = false; clearReview()
    if (value.kind === 'propose') runCursors.value = ['']
    if (value.kind === 'budget') notice.value = 'Storage allowance request confirmed. Capture has not resumed; refresh the current status and use Resume separately.'
    refreshStatus()
  } catch (error) {
    if (!sameIdentity()) return
    review.value = null
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; access.denied.value = true; await props.refreshWorkspace().catch(() => {}); return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The result could not be confirmed. Retry the same reviewed historical capture request to recover its outcome.' : error instanceof ApiError && error.status === 409 ? 'The source binding, capture state, readiness or allowance changed. Refresh and review the current status.' : describeApiError(error, 'Could not complete the historical capture action.')
    refreshStatus()
  } finally { if (sameIdentity()) pending.value = false }
}
function prepare() { const c = props.connection; if (canPropose.value && c) void submit({ kind: 'propose', identity: access.identity.value, body: { request_id: crypto.randomUUID(), parent_import_id: parentId.value, connection_id: c.id, expected_revision: String(c.revision) } }) }
function reviewConfirm() {
  const c = current.value; if (!c || !canConfirm.value) return
  review.value = { intent: { kind: 'confirm', identity: access.identity.value, id: c.id, body: { request_id: crypto.randomUUID(), expected_run_revision: c.revision, acknowledgements: { api_visible_account_scope: true, coverage_gaps: true, retained_not_imported: true, source_user_evidence_revision: c.source_user_evidence_revision, source_user_difference: c.source_user_difference } } }, message: `Capture ${c.id} for People import ${c.parent_import_id}. Source account ${c.source_account_id}, user ${c.source_user_id}; parent user ${c.parent_source_user_id ?? 'unknown'}. Source user ${c.source_user_difference ? 'differs from the parent' : 'matches the parent'}; evidence revision ${c.source_user_evidence_revision}. This starts account-wide API-visible events, calls and texts. Restricted or deleted records and full content remain unknown; email, details and media are not fetched. Exact returned evidence stays retained for administrator review. Nothing is imported into native history or Today, and the workspace remains in review. Capture revision ${c.revision}.` }
}
function resume() { const c = current.value; if (c && canResume.value) void submit({ kind: 'retry', identity: access.identity.value, id: c.id, body: { request_id: crypto.randomUUID(), expected_run_revision: c.revision } }) }
function reviewCancel() { const c = current.value; if (c?.actions.cancel && !busy.value) review.value = { intent: { kind: 'cancel', identity: access.identity.value, id: c.id, body: { request_id: crypto.randomUUID(), expected_run_revision: c.revision } }, message: `Permanently cancel capture ${c.id} at revision ${c.revision}. Committed evidence remains available for administrator review. This run cannot resume. Another independently confirmed capture may be prepared for the same People import.` } }
function reviewBudget(value: { body: HistoryBudget; message: string }) { const c = current.value; if (c?.actions.increase_budget && !busy.value) review.value = { intent: { kind: 'budget', identity: access.identity.value, id: c.id, body: value.body }, message: value.message } }
function confirmReview() { const value = review.value; if (!value || busy.value || !access.enabled.value) return; if (value.intent.kind === 'confirm' && !canConfirm.value) return; void submit(value.intent) }
</script>
<template>
  <div
    class="mb-6 min-w-0 space-y-5"
    data-testid="history-capture-panel"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 class="text-section font-medium">
            Capture historical source data
          </h2><p class="mt-1 text-small text-text-muted">
            Retain API-visible events, calls and texts for a completed People import.
          </p>
        </div><button
          v-if="access.enabled.value"
          type="button"
          :class="buttonClasses()"
          @click="refreshStatus"
        >
          Refresh historical capture status
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
          Retry the same historical capture request
        </button>
        <p
          v-if="notice"
          role="status"
          class="mt-3 text-small"
        >
          {{ notice }}
        </p>
        <p
          v-if="parents.error.value || parent.error.value || listing.error.value || detail.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(parents.error.value ?? parent.error.value ?? listing.error.value ?? detail.error.value, 'Could not load historical capture review. Refresh to recover.') }}
        </p>
        <FormField
          v-slot="{ id }"
          label="People import for historical capture"
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
              v-for="value in parents.data.value?.imports ?? []"
              :key="value.id"
              :value="value.id"
              :disabled="value.state !== 'completed'"
            >
              {{ snapshotTime(value.created_at) }} · {{ snapshotLabel(value.state) }} · {{ value.id }}
            </option><option
              v-if="parent.data.value && !parents.data.value?.imports.some(value => value.id === parentId)"
              :value="parentId"
            >
              {{ snapshotTime(parent.data.value.created_at) }} · {{ parentId }}
            </option>
          </select>
        </FormField>
        <div class="mt-2 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || parentCursors.length < 2 || parents.isFetching.value"
            @click="parentCursors.pop()"
          >
            Previous historical parents
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || !parents.data.value?.next_cursor || parents.isFetching.value"
            @click="parents.data.value?.next_cursor && parentCursors.push(parents.data.value.next_cursor)"
          >
            More historical parents
          </button>
        </div>
        <p
          v-if="parentId && !eligibleParent && !parent.isFetching.value"
          class="mt-2 text-small text-text-muted"
        >
          Preparation requires a completed People import in the administrator review workspace. The server checks the retained parent and source account.
        </p>
        <p
          v-if="!connected"
          class="mt-2 text-small text-text-muted"
        >
          Connect a validated source credential to prepare or resume capture. Retained evidence remains readable while disconnected.
        </p>
        <button
          type="button"
          class="mt-3"
          :class="buttonClasses('primary')"
          :disabled="!canPropose"
          @click="prepare"
        >
          Prepare historical capture
        </button>
        <p class="mt-2 text-small text-text-muted">
          Preparation makes no source requests. Each new capture needs its own confirmation. Previous attempts remain retained; they are not a reconciled delta or a cutover.
        </p>
        <FormField
          v-if="parentId"
          v-slot="{ id }"
          label="Historical capture run"
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
              Choose a historical capture
            </option><option
              v-for="value in listing.data.value?.captures ?? []"
              :key="value.id"
              :value="value.id"
            >
              {{ snapshotTime(value.created_at) }} · {{ snapshotLabel(value.id === current?.id ? current.state : value.state) }} · {{ value.id }}
            </option><option
              v-if="current && !listing.data.value?.captures.some(value => value.id === current?.id)"
              :value="current.id"
            >
              {{ snapshotTime(current.created_at) }} · {{ snapshotLabel(current.state) }} · {{ current.id }}
            </option>
          </select>
        </FormField>
        <div
          v-if="parentId"
          class="mt-2 flex flex-wrap gap-2"
        >
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || runCursors.length < 2 || listing.isFetching.value"
            @click="runCursors.pop()"
          >
            Previous historical captures
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || !listing.data.value?.next_cursor || listing.isFetching.value"
            @click="listing.data.value?.next_cursor && runCursors.push(listing.data.value.next_cursor)"
          >
            More historical captures
          </button>
        </div>
        <p
          v-if="detail.isFetching.value && !current"
          role="status"
          class="mt-3 text-small"
        >
          Loading historical capture…
        </p>
      </template>
    </Card>
    <template v-if="current">
      <Card class="min-w-0">
        <h2 class="text-section font-medium">
          Historical capture · {{ snapshotLabel(current.state) }}
        </h2>
        <p class="mt-1 break-all text-small text-text-muted">
          {{ current.id }} · prepared {{ snapshotTime(current.created_at) }}
        </p>
        <p
          v-if="current.pause_reason"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ historyLabel(current.pause_reason) }}. Review source authority, storage and coverage before an explicit Resume. Changed bindings require a new capture. Resuming an uncertain enumeration does not refetch or repair missing identities.
        </p>
        <p
          v-if="current.state === 'completed_with_gaps'"
          class="mt-3 text-small"
        >
          The three selected queries were enumerated with gaps. This does not establish complete account history, accessible full content or a completed migration.
        </p>
        <p
          v-if="current.state === 'cancelled' || current.state === 'paused'"
          class="mt-3 text-small"
        >
          This is a partial capture. Committed source evidence remains retained. Cancellation is permanent for this run.
        </p>
        <p
          v-if="current.state === 'proposed'"
          class="mt-3 text-small"
        >
          No source work has started. Confirmation expires {{ snapshotTime(current.proposal_expires_at) }}; an expired proposal remains readable and can be cancelled.
        </p>
        <dl class="mt-4 grid min-w-0 gap-3 text-small sm:grid-cols-2">
          <div>
            <dt class="text-text-muted">
              Source account / user
            </dt><dd class="break-all">
              {{ current.source_account_id }} / {{ current.source_user_id }}
            </dd><dd>Parent source user {{ current.parent_source_user_id ?? 'unknown' }} · {{ current.source_user_difference ? 'different source user' : 'same source user' }}</dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Capture / parent boundary
            </dt><dd>{{ current.capture_sequence }} / {{ current.parent_capture_sequence }}</dd><dd class="break-all">
              People import {{ current.parent_import_id }}
            </dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Parent source window
            </dt><dd>{{ snapshotTime(current.parent_source_started_at) }} – {{ snapshotTime(current.parent_source_completed_at) }}</dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Historical capture window
            </dt><dd>{{ snapshotTime(current.started_at) }} – {{ current.completed_at ? snapshotTime(current.completed_at) : 'Not finished' }}</dd><dd>Source evidence may be newer than the frozen People parent.</dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Raw bytes retained
            </dt><dd>{{ formatBytes(current.raw_bytes) }}</dd>
          </div>
          <div>
            <dt class="text-text-muted">
              Source profile
            </dt><dd class="break-all">
              {{ current.profile_version }} · parser {{ current.parser_version }}
            </dd>
          </div>
        </dl>
        <p class="mt-3 text-small text-text-muted">
          Account-wide API-visible scope; restricted/deleted records, privacy placeholders, missing detail content, email, recordings and media remain unknown or excluded. Current-source changes do not update the completed People parent.
        </p>
        <p class="mt-1 text-small">
          Captured data is retained, not imported. Native history, Today and communications remain unchanged. The Organization stays in administrator review.
        </p>
        <ul class="mt-2 list-inside list-disc text-small text-text-muted">
          <li
            v-for="reason in current.coverage_reasons"
            :key="reason"
          >
            {{ historyLabel(reason) }}
          </li>
        </ul>
        <div class="mt-4 grid gap-3 lg:grid-cols-3">
          <section
            v-for="stream in current.streams"
            :key="stream.family"
            class="min-w-0 rounded-lg border border-border p-3"
            :aria-label="`${historyLabel(stream.family)} capture coverage`"
          >
            <h3 class="text-body font-medium">
              {{ historyLabel(stream.family) }} · {{ historyLabel(stream.state) }}
            </h3><p class="mt-1 text-small text-text-muted">
              Frozen reported total {{ stream.reported_total ?? 'unknown' }} · checkpoint {{ stream.checkpoint }}
            </p><dl class="mt-2 grid grid-cols-[1fr_auto] gap-x-3 gap-y-1 text-small">
              <template
                v-for="name in counters"
                :key="name"
              >
                <dt>{{ counterLabels[name] }}</dt><dd class="break-all text-right">
                  {{ stream[name] }}
                </dd>
              </template><dt>API-inaccessible count</dt><dd>Unknown</dd>
            </dl><p class="mt-2 text-small text-text-muted">
              Current run counters use advancing pages. Link counts describe retained references, not importable facts. Enumeration is not complete account history.
            </p>
          </section>
        </div>
        <div
          v-if="current.state === 'proposed'"
          class="mt-4 space-y-3 border-t border-border pt-4"
        >
          <h3 class="text-body font-medium">
            Review source capture scope
          </h3>
          <label class="flex min-h-10 items-start gap-2 text-small"><input
            v-model="scopeAck"
            type="checkbox"
            class="mt-1 h-4 w-4 shrink-0 accent-accent"
            :disabled="busy || !sourceBound"
          >I acknowledge account-wide API-visible events, calls and texts for source account {{ current.source_account_id }}, user {{ current.source_user_id }}.</label>
          <label class="flex min-h-10 items-start gap-2 text-small"><input
            v-model="gapsAck"
            type="checkbox"
            class="mt-1 h-4 w-4 shrink-0 accent-accent"
            :disabled="busy || !sourceBound"
          >I acknowledge unknown restricted/deleted records and missing full content, details, email, recordings and media.</label>
          <label class="flex min-h-10 items-start gap-2 text-small"><input
            v-model="retainedAck"
            type="checkbox"
            class="mt-1 h-4 w-4 shrink-0 accent-accent"
            :disabled="busy || !sourceBound"
          >I understand this retains source evidence without importing native history or activating the workspace.</label>
          <label
            v-if="current.source_user_difference"
            class="flex min-h-10 items-start gap-2 text-small"
          ><input
            v-model="differenceAck"
            type="checkbox"
            class="mt-1 h-4 w-4 shrink-0 accent-accent"
            :disabled="busy || !sourceBound"
          >I acknowledge that source user {{ current.source_user_id }} differs from parent user {{ current.parent_source_user_id ?? 'unknown' }}; evidence revision {{ current.source_user_evidence_revision }}.</label>
          <button
            type="button"
            :class="buttonClasses('primary')"
            :disabled="!canConfirm"
            @click="reviewConfirm"
          >
            Review historical capture confirmation
          </button>
        </div>
        <p
          v-if="!current.release_ready && (current.state === 'proposed' || current.state === 'paused')"
          class="mt-3 text-small text-text-muted"
        >
          Current release readiness is required before source work can start or resume.
        </p>
        <p
          v-if="!sourceBound && (current.state === 'proposed' || current.state === 'paused')"
          class="mt-3 text-small text-text-muted"
        >
          Only the original initiating administrator with the acknowledged connection revision can start or resume this capture. Retained review and cancellation remain separate.
        </p>
        <p
          v-if="sourceBusy"
          class="mt-3 text-small text-text-muted"
        >
          Another source job is active. Wait for it to finish or cancel it before starting source work here.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            v-if="current.actions.retry"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="!canResume"
            @click="resume"
          >
            Resume historical capture
          </button><button
            v-if="current.actions.cancel"
            type="button"
            :class="buttonClasses('danger')"
            :disabled="busy"
            @click="reviewCancel"
          >
            Cancel historical capture
          </button>
        </div>
        <HistoryCaptureBudget
          :capture="current"
          :disabled="busy"
          @review="reviewBudget"
          @change="review = null"
        />
      </Card>
      <HistoryCaptureRecords
        :key="`${access.scope.value}:${current.id}`"
        :capture-id="current.id"
        :current-sequence="current.capture_sequence"
      />
    </template>
    <ConfirmDialog
      :visible="review !== null && access.enabled.value"
      :title="dialogTitle"
      :message="review?.message ?? ''"
      :confirm-label="dialogLabel"
      :confirm-variant="review?.intent.kind === 'cancel' ? 'danger' : 'primary'"
      :is-pending="pending"
      @update:visible="value => { if (!value) review = null }"
      @confirm="confirmReview"
    />
  </div>
</template>
