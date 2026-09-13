<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import CoreChangeCaptureBoundary from './CoreChangeCaptureBoundary.vue'
import CoreChangeReportRows from './CoreChangeReportRows.vue'
import { ApiError } from '../../api/client'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { fetchSnapshot, fetchSnapshots, type CoreSnapshot } from '../../api/snapshots'
import { cancelCoreChangeReport, coreChangeActive, coreChangeDispositions, coreChangeFamilies, coreChangeLabel, coreChangeMeaning, createCoreChangeReport, fetchCoreChangeReport, fetchCoreChangeReports, resumeCoreChangeReport, useCoreChangeAccess, type CoreChangeList, type CoreChangeRequest } from '../../api/coreChangeReports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotTime } from './format'

const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
const emit = defineEmits<{ reviewSnapshot: [snapshotId: string]; recapture: [] }>()
const access = useCoreChangeAccess()
const parentId = ref('')
const newerId = ref('')
const reportId = ref('')
const parentCursors = ref<string[]>([''])
const snapshotCursors = ref<string[]>([''])
const reportCursors = ref<string[]>([''])
const firstReportPage = ref<CoreChangeList | null>(null)
const listEpoch = ref(0)
const pending = ref(false)
const uncertain = ref(false)
const actionError = ref('')
const cancelReview = ref<string | null>(null)
type Intent = { kind: 'create'; identity: string; parentId: string; body: CoreChangeRequest }
  | { kind: 'resume' | 'cancel'; identity: string; parentId: string; id: string; requestId: string }
const intent = ref<Intent | null>(null)
let disposed = false
let identityEpoch = 0
let scopeEpoch = 0
let selectionEpoch = 0
const busy = computed(() => pending.value || intent.value !== null)
const parentsKey = computed(() => [...access.prefix.value, 'parents', parentCursors.value.at(-1) ?? ''])
const parentKey = computed(() => [...access.prefix.value, 'parent', parentId.value])
const snapshotListKey = computed(() => [...access.prefix.value, 'snapshots', parentId.value, snapshotCursors.value.at(-1) ?? ''])
const baselineKey = computed(() => [...access.prefix.value, 'baseline', parent.data.value?.snapshot_id ?? ''])
const newerKey = computed(() => [...access.prefix.value, 'newer', newerId.value])
const listKey = computed(() => [...access.prefix.value, 'list', parentId.value, listEpoch.value, reportCursors.value.at(-1) ?? ''])
const detailKey = computed(() => [...access.prefix.value, 'detail', reportId.value])
const parents = useQuery({ queryKey: parentsKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentsKey.value, () => parentsKey.value, () => fetchImports(parentCursors.value.at(-1) || undefined, signal)) })
const parent = useQuery({ queryKey: parentKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentKey.value, () => parentKey.value, () => fetchImport(parentId.value, signal)) })
const baseline = useQuery({ queryKey: baselineKey, enabled: computed(() => access.enabled.value && !!parent.data.value?.snapshot_id), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(baselineKey.value, () => baselineKey.value, () => fetchSnapshot(parent.data.value!.snapshot_id, signal)) })
const snapshots = useQuery({ queryKey: snapshotListKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(snapshotListKey.value, () => snapshotListKey.value, () => fetchSnapshots(snapshotCursors.value.at(-1) || undefined, signal)) })
const newer = useQuery({ queryKey: newerKey, enabled: computed(() => access.enabled.value && !!newerId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(newerKey.value, () => newerKey.value, () => fetchSnapshot(newerId.value, signal)) })
const listing = useQuery({ queryKey: listKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, staleTime: Infinity, refetchOnWindowFocus: false, refetchOnReconnect: false,
  queryFn: ({ signal }) => access.read(listKey.value, () => listKey.value, async () => {
    const first = reportCursors.value.length === 1; const scope = access.scope.value; const tag = JSON.stringify(listKey.value)
    if (first && firstReportPage.value) return firstReportPage.value
    const value = await fetchCoreChangeReports(parentId.value, reportCursors.value.at(-1) || undefined, signal)
    if (first && !signal.aborted && scope === access.scope.value && tag === JSON.stringify(listKey.value)) firstReportPage.value = value
    return value
  }),
})
const detail = useQuery({ queryKey: detailKey, enabled: computed(() => access.enabled.value && !!reportId.value), retry: false, gcTime: 0,
  queryFn: ({ signal }) => access.read(detailKey.value, () => detailKey.value, () => fetchCoreChangeReport(reportId.value, signal)),
  refetchInterval: query => coreChangeActive(query.state.data) ? Math.min(30_000, 2_000 * 2 ** Math.min(query.state.fetchFailureCount, 4)) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const current = computed(() => access.enabled.value && detail.data.value?.parent_import_id === parentId.value ? detail.data.value : undefined)
const eligibleParent = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review' && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id)
function eligibleSnapshot(value: CoreSnapshot) {
  const original = baseline.data.value?.snapshot
  return !!original && value.id !== original.id && ['completed', 'completed_with_gaps'].includes(value.state)
    && value.profile_version === 'fub-core-v1' && value.profile_version === original.profile_version && value.source_account_id === original.source_account_id
    && !!value.completed_at && !!original.completed_at && Date.parse(value.completed_at) > Date.parse(original.completed_at)
}
const choices = computed(() => access.enabled.value ? (snapshots.data.value?.snapshots ?? []).filter(eligibleSnapshot) : [])
const firstNewerObservation = computed(() => {
  const times = (newer.data.value?.streams ?? []).map(value => value.first_observed_at).filter((value): value is string => !!value && Number.isFinite(Date.parse(value)))
  return times.length ? Math.min(...times.map(value => Date.parse(value))) : NaN
})
const eligibleNewer = computed(() => !!newer.data.value && eligibleSnapshot(newer.data.value.snapshot) && Number.isFinite(firstNewerObservation.value)
  && firstNewerObservation.value > Date.parse(baseline.data.value?.snapshot.completed_at ?? ''))
const canCreate = computed(() => eligibleParent.value && eligibleNewer.value && !busy.value)
const errors = computed(() => [parents.error.value, parent.error.value, baseline.error.value, snapshots.error.value, newer.error.value, listing.error.value, detail.error.value].filter(Boolean))

function resetReportList() { firstReportPage.value = null; reportCursors.value = ['']; listEpoch.value++; access.remove([...access.prefix.value, 'list', parentId.value]) }
function deny() { access.denied.value = true; cancelReview.value = null; void props.refreshWorkspace().catch(() => {}) }
watch(parents.data, value => { if (access.enabled.value && !parentId.value) parentId.value = value?.imports.find(value => value.state === 'completed' && value.confirmed_plan_id)?.id ?? '' })
watch(listing.data, value => { if (access.enabled.value && !reportId.value) reportId.value = value?.reports[0]?.id ?? '' })
watch(parentId, () => { selectionEpoch++; newerId.value = ''; reportId.value = ''; snapshotCursors.value = ['']; resetReportList(); cancelReview.value = null }, { flush: 'sync' })
watch([newerId, reportId], () => { selectionEpoch++; cancelReview.value = null }, { flush: 'sync' })
watch(() => current.value?.state, () => { cancelReview.value = null }, { flush: 'sync' })
watch(access.scope, () => { scopeEpoch++; cancelReview.value = null; firstReportPage.value = null; reportCursors.value = ['']; listEpoch.value++ }, { flush: 'sync' })
watch(access.identity, () => { identityEpoch++; intent.value = null; pending.value = false; uncertain.value = false; actionError.value = ''; parentId.value = ''; newerId.value = ''; reportId.value = ''; parentCursors.value = ['']; snapshotCursors.value = ['']; resetReportList() }, { flush: 'sync' })
watch(errors, values => { if (values.some(importAccessError)) deny() })
onBeforeUnmount(() => { disposed = true; intent.value = null; firstReportPage.value = null; access.remove(access.prefix.value) })
function refresh() {
  if (!access.enabled.value) return
  resetReportList()
  for (const key of [parentsKey.value, parentKey.value, baselineKey.value, snapshotListKey.value, newerKey.value, detailKey.value]) void access.client.invalidateQueries({ queryKey: key, exact: true })
}
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const actorEpoch = identityEpoch; const authorityEpoch = scopeEpoch; const selection = selectionEpoch; const scope = access.scope.value
  const sameIdentity = () => !disposed && actorEpoch === identityEpoch && value.identity === access.identity.value
  const currentRequest = () => sameIdentity() && access.enabled.value && authorityEpoch === scopeEpoch && selection === selectionEpoch && scope === access.scope.value
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''; cancelReview.value = null
  try {
    const result = value.kind === 'create' ? await createCoreChangeReport(value.body) : value.kind === 'resume' ? await resumeCoreChangeReport(value.id, value.requestId) : await cancelCoreChangeReport(value.id, value.requestId)
    if (!currentRequest()) { if (sameIdentity()) { uncertain.value = true; actionError.value = 'Access changed while the request was pending. Verify access, then retry the same request to recover its outcome.' }; return }
    // Replayed receipts describe original acceptance, never current report state.
    parentId.value = value.parentId; reportId.value = result.report_id; intent.value = null; uncertain.value = false
    refresh()
  } catch (error) {
    if (!sameIdentity()) return
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; deny(); return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The request outcome is uncertain. Retry the same request to recover its saved receipt.'
      : error instanceof ApiError && error.status === 409 ? 'The inputs, report state, storage allowance or compatible release changed. Refresh and review the current status.'
        : describeApiError(error, 'Could not complete the change-report request.')
    if (currentRequest()) refresh()
  } finally { if (sameIdentity()) pending.value = false }
}
function create() { if (canCreate.value) void submit({ kind: 'create', identity: access.identity.value, parentId: parentId.value, body: { request_id: crypto.randomUUID(), parent_import_id: parentId.value, newer_snapshot_id: newerId.value } }) }
function resume() { const value = current.value; if (value?.actions.resume && !busy.value) void submit({ kind: 'resume', identity: access.identity.value, parentId: parentId.value, id: value.id, requestId: crypto.randomUUID() }) }
function cancel() { const value = current.value; if (value?.actions.cancel && !busy.value && cancelReview.value === value.id) void submit({ kind: 'cancel', identity: access.identity.value, parentId: parentId.value, id: value.id, requestId: crypto.randomUUID() }) }
</script>

<template>
  <div
    class="mb-6 min-w-0 space-y-5"
    data-testid="core-change-reports"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-start justify-between gap-3">
        <div>
          <h2 class="text-section font-medium">
            Assess changes since the original import
          </h2><p class="mt-1 max-w-3xl text-small text-text-muted">
            Compare two retained core captures for a completed People import. The report preserves uncertainty and leaves CRM records and the review hold unchanged.
          </p>
        </div><button
          type="button"
          :class="buttonClasses()"
          :disabled="!access.enabled.value"
          @click="refresh"
        >
          Refresh change reports
        </button>
      </div>
      <template v-if="access.enabled.value">
        <p
          v-if="!eligibleParent"
          class="mt-3 text-small text-text-muted"
        >
          Choose a completed People import in the current migration-review workspace to assess retained captures.
        </p>
        <div class="mt-4 grid gap-4 lg:grid-cols-2">
          <section class="min-w-0">
            <FormField
              v-slot="{ id }"
              label="Completed People import"
              bare
            >
              <select
                :id="id"
                v-model="parentId"
                :class="INPUT_CLASSES"
                :disabled="busy"
              >
                <option value="">
                  Choose a completed import
                </option><option
                  v-if="parentId && !parents.data.value?.imports.some(value => value.id === parentId)"
                  :value="parentId"
                >
                  Selected import · {{ parentId }}
                </option><option
                  v-for="value in parents.data.value?.imports.filter(value => value.state === 'completed' && value.confirmed_plan_id) ?? []"
                  :key="value.id"
                  :value="value.id"
                >
                  {{ snapshotTime(value.created_at) }} · {{ value.id }}
                </option>
              </select>
            </FormField>
            <p
              v-if="parents.isFetching.value"
              role="status"
              class="mt-2 text-small"
            >
              Loading People imports…
            </p>
            <p
              v-if="parents.data.value && !parents.data.value.imports.some(value => value.state === 'completed' && value.confirmed_plan_id)"
              class="mt-2 text-small text-text-muted"
            >
              No completed import on this page. Check other pages if available.
            </p>
            <div class="mt-2 flex flex-wrap gap-2">
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || parentCursors.length < 2 || parents.isFetching.value"
                @click="parentCursors.pop()"
              >
                Previous import page
              </button><button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || !parents.data.value?.next_cursor || parents.isFetching.value"
                @click="parents.data.value?.next_cursor && parentCursors.push(parents.data.value.next_cursor)"
              >
                More import pages
              </button>
            </div>
          </section>
          <section class="min-w-0">
            <FormField
              v-slot="{ id }"
              label="Newer retained core capture"
              bare
            >
              <select
                :id="id"
                v-model="newerId"
                :class="INPUT_CLASSES"
                :disabled="busy || !eligibleParent || baseline.isFetching.value"
              >
                <option value="">
                  Choose a newer capture
                </option><option
                  v-if="newerId && !choices.some(value => value.id === newerId)"
                  :value="newerId"
                >
                  Selected capture · {{ newerId }}
                </option><option
                  v-for="value in choices"
                  :key="value.id"
                  :value="value.id"
                >
                  {{ snapshotTime(value.completed_at) }} · {{ coreChangeLabel(value.state) }} · {{ value.id }}
                </option>
              </select>
            </FormField>
            <p
              v-if="snapshots.isFetching.value || newer.isFetching.value || baseline.isFetching.value"
              role="status"
              class="mt-2 text-small"
            >
              Checking retained capture boundaries…
            </p>
            <p
              v-else-if="parentId && !choices.length"
              class="mt-2 text-small text-text-muted"
            >
              No newer completed capture for this source account on this page. Review more pages or use the existing core-capture workflow.
            </p>
            <p
              v-if="newerId && newer.data.value && !eligibleNewer"
              class="mt-2 text-small text-danger"
            >
              This capture does not prove a non-overlapping source interval after the original capture. Choose another completed capture.
            </p>
            <div class="mt-2 flex flex-wrap gap-2">
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || snapshotCursors.length < 2 || snapshots.isFetching.value"
                @click="snapshotCursors.pop()"
              >
                Previous capture page
              </button><button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || !snapshots.data.value?.next_cursor || snapshots.isFetching.value"
                @click="snapshots.data.value?.next_cursor && snapshotCursors.push(snapshots.data.value.next_cursor)"
              >
                More capture pages
              </button>
            </div>
          </section>
        </div>
        <p
          v-if="baseline.data.value"
          class="mt-2 break-all text-small text-text-muted"
        >
          Original snapshot {{ baseline.data.value.snapshot.id }} completed {{ snapshotTime(baseline.data.value.snapshot.completed_at) }}. The server also verifies retained identity, schema and source account before accepting the report.
        </p>
        <div class="mt-4 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('primary')"
            :disabled="!canCreate"
            @click="create"
          >
            Assess changes
          </button><button
            type="button"
            :class="buttonClasses()"
            :disabled="busy"
            @click="emit('recapture')"
          >
            Open core capture workflow
          </button>
        </div>
        <p class="mt-2 text-small text-text-muted">
          Assessment reads retained evidence only. A new source capture uses its existing separate confirmation.
        </p>
        <p
          v-if="pending"
          role="status"
          class="mt-3 text-small"
        >
          Saving the report request…
        </p>
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
          @click="submit(intent)"
        >
          Retry the same report request
        </button>
        <p
          v-for="(error, index) in errors"
          :key="index"
          role="alert"
          class="mt-2 text-small text-danger"
        >
          {{ describeApiError(error, 'Could not load retained report data. Refresh to try again.') }}
        </p>
      </template>
      <p
        v-else
        role="status"
        class="mt-3 text-small"
      >
        A currently verified Organization administrator session is required.
      </p>
    </Card>
    <Card
      v-if="access.enabled.value && parentId"
      class="min-w-0"
    >
      <h3 class="text-section font-medium">
        Saved change reports
      </h3><p class="mt-1 text-small text-text-muted">
        Completed and cancelled reports remain available after reload. Refresh to include newly created reports; each page traversal keeps its original boundary.
      </p>
      <FormField
        v-slot="{ id }"
        label="Saved change report"
        bare
        class="mt-3"
      >
        <select
          :id="id"
          v-model="reportId"
          :class="INPUT_CLASSES"
          :disabled="busy"
        >
          <option value="">
            Choose a saved report
          </option><option
            v-if="reportId && !listing.data.value?.reports.some(value => value.id === reportId)"
            :value="reportId"
          >
            Selected report · {{ reportId }}
          </option><option
            v-for="value in listing.data.value?.reports ?? []"
            :key="value.id"
            :value="value.id"
          >
            {{ snapshotTime(value.created_at) }} · {{ coreChangeLabel(value.state) }} · {{ value.id }}
          </option>
        </select>
      </FormField>
      <p
        v-if="listing.isFetching.value"
        role="status"
        class="mt-2 text-small"
      >
        Loading saved reports…
      </p><p
        v-else-if="listing.data.value && !listing.data.value.reports.length"
        class="mt-2 text-small text-text-muted"
      >
        No saved reports on this page for this People import.
      </p>
      <div class="mt-2 flex flex-wrap gap-2">
        <button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="busy || reportCursors.length < 2 || listing.isFetching.value"
          @click="reportCursors.pop()"
        >
          Previous report page
        </button><button
          type="button"
          :class="buttonClasses('ghost')"
          :disabled="busy || !listing.data.value?.next_cursor || listing.isFetching.value"
          @click="listing.data.value?.next_cursor && reportCursors.push(listing.data.value.next_cursor)"
        >
          More report pages
        </button>
      </div>
    </Card>
    <p
      v-if="access.enabled.value && detail.isFetching.value"
      role="status"
      class="text-small"
    >
      Refreshing report status…
    </p>
    <template v-if="current">
      <Card class="min-w-0">
        <div class="flex flex-wrap items-center justify-between gap-2">
          <h3 class="text-section font-medium">
            {{ coreChangeLabel(current.state) }} change report
          </h3><span class="text-small text-text-muted">{{ coreChangeLabel(current.phase) }}</span>
        </div>
        <p class="mt-1 break-all text-small text-text-muted">
          Report {{ current.id }} · People import {{ current.parent_import_id }}
        </p>
        <p class="mt-2 text-small">
          {{ current.state === 'completed' ? 'Comparison finished. This does not establish full account coverage, completed repair or cutover readiness.' : current.state === 'cancelled' ? 'Cancelled. Committed evidence remains retained; partial comparison rows are unpublished. This report cannot resume.' : 'Committed progress is shown while work runs. Comparison rows and final counts remain unpublished until completion.' }}
        </p>
        <dl class="mt-4 grid gap-3 text-small sm:grid-cols-3">
          <div>
            <dt class="text-text-muted">
              Captures processed
            </dt><dd>{{ current.progress.captures_processed }}</dd>
          </div><div>
            <dt class="text-text-muted">
              Observations processed
            </dt><dd>{{ current.progress.observations_processed }}</dd>
          </div><div>
            <dt class="text-text-muted">
              Groups compared
            </dt><dd>{{ current.progress.groups_compared }}</dd>
          </div>
        </dl>
        <p class="mt-3 text-small text-text-muted">
          Retained {{ formatBytes(current.retained_bytes) }} · reserved {{ formatBytes(current.reserved_bytes) }} · status updated {{ snapshotTime(current.updated_at) }}
        </p>
        <p
          v-if="current.pause_reason"
          role="status"
          class="mt-3 text-small"
        >
          {{ coreChangeLabel(current.pause_reason) }}. Resolve the reported condition, refresh status and use Resume when available. Evidence is preserved.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            v-if="current.actions.resume"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="busy"
            @click="resume"
          >
            Resume change report
          </button><button
            v-if="current.actions.cancel"
            type="button"
            :class="buttonClasses('danger')"
            :disabled="busy"
            @click="cancelReview = current.id"
          >
            Cancel change report
          </button>
        </div>
      </Card>
      <Card class="min-w-0">
        <h3 class="text-section font-medium">
          Frozen capture boundaries and coverage
        </h3>
        <p class="mt-2 text-small">
          {{ current.inputs.source_scope === 'consistent_identity' ? 'Retained source identities match. This does not prove unchanged effective permissions or complete account access.' : current.inputs.source_scope === 'changed_identity' ? 'Source user changed between captures. One-sided observations may reflect different access.' : 'Retained source identity or access evidence is incomplete. One-sided observations remain uncertain.' }}
        </p>
        <p class="mt-1 text-small text-text-muted">
          Absence never establishes deletion, even when streams finished. Captures span time and are not atomic account snapshots. Standalone tags, historical events/calls/texts, email and media are outside this report.
        </p>
        <ul
          v-if="current.inputs.warnings.length"
          class="mt-2 list-inside list-disc text-small text-text-muted"
        >
          <li
            v-for="(warning, index) in current.inputs.warnings"
            :key="index"
          >
            {{ coreChangeLabel(warning) }}
          </li>
        </ul>
        <div class="mt-4 grid gap-3 xl:grid-cols-2">
          <CoreChangeCaptureBoundary
            :boundary="current.inputs.baseline"
            title="Original capture"
            @review-snapshot="emit('reviewSnapshot', $event)"
          /><CoreChangeCaptureBoundary
            :boundary="current.inputs.newer"
            title="Newer capture"
            @review-snapshot="emit('reviewSnapshot', $event)"
          />
        </div>
      </Card>
      <Card
        v-if="current.state === 'completed' && current.counts && current.output_revision"
        class="min-w-0"
      >
        <h3 class="text-section font-medium">
          Reconciled comparison counts
        </h3><p class="mt-1 text-small text-text-muted">
          Source-ID dispositions are separate from invalid observations and repeat counts. Not seen again is not deletion; newly observed is not proof of creation.
        </p>
        <dl class="mt-3 grid gap-3 text-small sm:grid-cols-3">
          <div>
            <dt class="text-text-muted">
              Source identities
            </dt><dd>{{ current.counts.source_ids }}</dd>
          </div><div>
            <dt class="text-text-muted">
              Invalid observations
            </dt><dd>{{ current.counts.invalid_observations }}</dd>
          </div><div>
            <dt class="text-text-muted">
              All observations
            </dt><dd>{{ current.counts.observations }}</dd>
          </div><div>
            <dt class="text-text-muted">
              Equal repeated observations
            </dt><dd>{{ current.counts.equal_repeats }}</dd>
          </div><div>
            <dt class="text-text-muted">
              Conflicting groups
            </dt><dd>{{ current.counts.conflicting_groups }}</dd>
          </div>
        </dl>
        <div class="mt-3 max-w-full overflow-x-auto">
          <table class="w-full text-left text-small">
            <caption class="sr-only">
              Reconciled counts by source family and result
            </caption><thead class="border-b border-border text-text-muted">
              <tr>
                <th class="p-2 font-medium">
                  Family
                </th><th
                  v-for="value in coreChangeDispositions"
                  :key="value"
                  class="p-2 font-medium"
                  :title="coreChangeMeaning(value)"
                >
                  {{ coreChangeLabel(value) }}
                </th>
              </tr>
            </thead><tbody class="divide-y divide-border">
              <tr
                v-for="family in coreChangeFamilies"
                :key="family"
              >
                <th class="p-2 font-medium">
                  {{ coreChangeLabel(family) }}
                </th><td
                  v-for="value in coreChangeDispositions"
                  :key="value"
                  class="p-2"
                >
                  {{ current.counts.families[family]?.[value] ?? 'Not reported' }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
      </Card>
      <CoreChangeReportRows
        v-if="current.state === 'completed' && current.output_revision"
        :key="`${access.scope.value}:${current.id}:${current.output_revision}`"
        :report-id="current.id"
        :output-revision="current.output_revision"
        :source-scope="current.inputs.source_scope"
        @review-snapshot="emit('reviewSnapshot', $event)"
        @access-denied="deny"
      />
    </template>
    <ConfirmDialog
      :visible="!!cancelReview && access.enabled.value"
      title="Cancel change assessment"
      message="Cancel this report permanently. Retained evidence and committed partial work remain stored, but partial comparisons stay unpublished. A later explicit assessment can create another report."
      confirm-label="Permanently cancel report"
      confirm-variant="danger"
      :is-pending="pending"
      @update:visible="value => { if (!value) cancelReview = null }"
      @confirm="cancel"
    />
  </div>
</template>
