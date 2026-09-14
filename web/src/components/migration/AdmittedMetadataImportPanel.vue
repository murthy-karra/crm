<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import MetadataMappingPanel from './MetadataMappingPanel.vue'
import MetadataRecordPanel from './MetadataRecordPanel.vue'
import CoreChangeCaptureBoundary from './CoreChangeCaptureBoundary.vue'
import { useMe } from '../../api/queries'
import { fetchPeopleAdmission, fetchPeopleAdmissions } from '../../api/peopleAdmissions'
import { fetchCoreChangeReport, fetchCoreChangeReports } from '../../api/coreChangeReports'
import SnapshotBudgetPanel from './SnapshotBudgetPanel.vue'
import { ApiError } from '../../api/client'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { fetchSnapshot } from '../../api/snapshots'
import { admittedMetadataReaders, cancelAdmittedMetadataImport, confirmAdmittedMetadataImport, fetchAdmittedMetadataImport, fetchAdmittedMetadataImports, fetchAdmittedMetadataIssues, admittedMetadataActive, prepareAdmittedMetadataImport, replanAdmittedMetadataImport, retryAdmittedMetadataImport, startAdmittedMetadataRemainder, useAdmittedMetadataAccess, type AdmittedMetadataReplan } from '../../api/admittedMetadataImports'
import type { MetadataConfirm, MetadataPatch } from '../../api/metadataImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'
const emit = defineEmits<{ reviewSnapshot: [snapshotId: string] }>()
const me = useMe()
async function refreshWorkspace() { await me.refetch() }
const access = useAdmittedMetadataAccess()
const parentId = ref('')
const selectedId = ref('')
const admissionId = ref(''); const reportId = ref('')
const admissionCursors = ref<string[]>(['']); const reportCursors = ref<string[]>(['']); const listCursors = ref<string[]>([''])
const remainderReview = ref(false)
const now = ref(Date.now()); const timer = setInterval(() => { now.value = Date.now() }, 1000)
const parentCursors = ref<string[]>([''])
const issueCursors = ref<string[]>([''])
const readMode = ref<'plan' | 'results'>('plan')
const heldAck = ref(false)
const reviewAck = ref(false)
const remainingAck = ref(false)
const mappingDirty = ref(false)
const confirmation = ref<MetadataConfirm | null>(null)
const cancelReview = ref(false)
const pending = ref(false)
const uncertain = ref(false)
const actionError = ref('')
const failures = ref(0)
let disposed = false
let identityGeneration = 0
let selectionGeneration = 0
type Intent =
  | { kind: 'plan'; identity: string; body: { request_id: string; admission_id: string; source_report_id: string } }
  | { kind: 'replan'; identity: string; id: string; body: AdmittedMetadataReplan }
  | { kind: 'confirm'; identity: string; id: string; body: MetadataConfirm }
  | { kind: 'retry' | 'cancel' | 'remainder'; identity: string; id: string; body: { request_id: string } }
const intent = ref<Intent | null>(null)
const parentsKey = computed(() => [...access.prefix.value, 'parents', parentCursors.value.at(-1) ?? ''])
const parentKey = computed(() => [...access.prefix.value, 'parent', parentId.value])
const listKey = computed(() => [...access.prefix.value, 'list', admissionId.value, listCursors.value.at(-1)])
const admissionsKey = computed(() => [...access.prefix.value, 'admissions', parentId.value, admissionCursors.value.at(-1)])
const admissionKey = computed(() => [...access.prefix.value, 'admission', admissionId.value])
const reportsKey = computed(() => [...access.prefix.value, 'reports', parentId.value, reportCursors.value.at(-1)])
const reportKey = computed(() => [...access.prefix.value, 'report', reportId.value])
const detailKey = computed(() => [...access.prefix.value, 'detail', selectedId.value])
const parents = useQuery({ queryKey: parentsKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentsKey.value, () => parentsKey.value, () => fetchImports(parentCursors.value.at(-1) || undefined, signal)) })
const parent = useQuery({ queryKey: parentKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentKey.value, () => parentKey.value, () => fetchImport(parentId.value, signal)) })
const admissions = useQuery({ queryKey: admissionsKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(admissionsKey.value, () => admissionsKey.value, () => fetchPeopleAdmissions(parentId.value, admissionCursors.value.at(-1) || undefined, signal)) })
const admission = useQuery({ queryKey: admissionKey, enabled: computed(() => access.enabled.value && !!admissionId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(admissionKey.value, () => admissionKey.value, () => fetchPeopleAdmission(admissionId.value, signal)) })
const reports = useQuery({ queryKey: reportsKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(reportsKey.value, () => reportsKey.value, () => fetchCoreChangeReports(parentId.value, reportCursors.value.at(-1) || undefined, signal)) })
const report = useQuery({ queryKey: reportKey, enabled: computed(() => access.enabled.value && !!reportId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(reportKey.value, () => reportKey.value, () => fetchCoreChangeReport(reportId.value, signal)) })
const listing = useQuery({ queryKey: listKey, enabled: computed(() => access.enabled.value && !!admissionId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(listKey.value, () => listKey.value, () => fetchAdmittedMetadataImports(admissionId.value, listCursors.value.at(-1) || undefined, signal)) })
const detail = useQuery({
  queryKey: detailKey, enabled: computed(() => access.enabled.value && !!selectedId.value), retry: false, gcTime: 0,
  queryFn: async ({ signal }) => {
    const expected = access.scope.value
    try {
      const result = await access.read(detailKey.value, () => detailKey.value, async () => {
        const value = await fetchAdmittedMetadataImport(selectedId.value, signal)
        if (expected === access.scope.value && value.workspace_revision !== access.org.value?.workspace_revision) { await refreshWorkspace(); throw new Error('Workspace status was refreshed') }
        return value
      })
      failures.value = 0; return result
    } catch (error) { if (expected === access.scope.value && !signal.aborted) failures.value++; throw error }
  },
  refetchInterval: query => admittedMetadataActive(query.state.data) ? Math.min(30_000, 2_000 * 2 ** Math.min(failures.value, 4)) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const current = computed(() => access.enabled.value ? detail.data.value : undefined)
const sourceId = computed(() => current.value?.snapshot_id ?? report.data.value?.inputs.newer.snapshot_id ?? '')
const sourceKey = computed(() => [...access.prefix.value, 'snapshot', sourceId.value])
const source = useQuery({ queryKey: sourceKey, enabled: computed(() => access.enabled.value && !!sourceId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(sourceKey.value, () => sourceKey.value, () => fetchSnapshot(sourceId.value, signal)) })
const issuesKey = computed(() => [...access.prefix.value, 'issues', current.value?.id ?? '', current.value?.latest_plan?.id ?? '', issueCursors.value.at(-1) ?? ''])
const issues = useQuery({ queryKey: issuesKey, enabled: computed(() => access.enabled.value && !!current.value?.latest_plan?.id), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(issuesKey.value, () => issuesKey.value, () => fetchAdmittedMetadataIssues(current.value!.id, current.value!.latest_plan!.id, issueCursors.value.at(-1) || undefined, signal)) })
const busy = computed(() => pending.value || !!intent.value)
const eligibleParent = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review' && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id)
const eligibleSource = computed(() => report.data.value?.state === 'completed' && report.data.value.parent_import_id === parentId.value && ['people', 'custom_fields'].every(kind => report.data.value?.inputs.newer.streams.some(stream => stream.stream === kind && stream.state === 'completed')))
const eligibleAdmission = computed(() => !!admission.data.value && ['completed', 'cancelled'].includes(admission.data.value.state) && admission.data.value.parent_import_id === parentId.value && BigInt(admission.data.value.progress.settled_items) > 0n)
const canCreate = computed(() => eligibleParent.value && eligibleAdmission.value && eligibleSource.value && listing.isSuccess.value && listing.data.value?.imports.every(row => row.state === 'cancelled' && !row.confirmed_plan_id) && !busy.value)
const sourceDirty = computed(() => !!current.value && reportId.value !== current.value.source_report_id)
const planExpired = computed(() => !!current.value?.latest_plan?.expires_at && Date.parse(current.value.latest_plan.expires_at) <= now.value)
const canConfirm = computed(() => current.value?.release_ready === true && current.value.actions.confirm && heldAck.value && reviewAck.value && remainingAck.value && !busy.value && !mappingDirty.value && !sourceDirty.value && !planExpired.value)
const resultVersion = computed(() => current.value ? JSON.stringify([current.value.state, current.value.phase, current.value.updated_at, current.value.counts, current.value.latest_plan]) : '')
const confirmationText = computed(() => {
  const c = current.value; const p = c?.latest_plan
  return c && p ? `Apply plan revision ${p.revision}: ${p.counts.tags.eligible} tags, ${p.counts.fields.eligible} fields, ${p.counts.options.eligible} options, ${p.counts.tag_links.eligible} Person tag links and ${p.counts.values.eligible} values are eligible. ${p.counts.held_count} tag, field, option, link or value items remain held. Other source issues remain listed for review. Existing differing values will remain unchanged. The Organization stays in administrator review. Notes, tasks and other remaining data are not applied by this import.` : ''
})
function clearReview() { remainderReview.value = false; confirmation.value = null; cancelReview.value = false; heldAck.value = false; reviewAck.value = false; remainingAck.value = false }
watch(parents.data, value => { if (access.enabled.value && !parentId.value) parentId.value = value?.imports.find(row => row.state === 'completed')?.id ?? '' })
watch(listing.data, value => { if (access.enabled.value && !selectedId.value && value?.imports.length) selectedId.value = value.imports[0]!.id })
watch(parentId, () => { selectionGeneration++; admissionId.value = ''; reportId.value = ''; selectedId.value = ''; admissionCursors.value = ['']; reportCursors.value = ['']; listCursors.value = ['']; clearReview(); issueCursors.value = [''] }, { flush: 'sync' })
watch(admissions.data, value => { if (access.enabled.value && !admissionId.value) admissionId.value = value?.items.find(row => ['completed','cancelled'].includes(row.state) && BigInt(row.progress.settled_items) > 0n)?.id ?? '' })
watch(reports.data, value => { if (access.enabled.value && !reportId.value) reportId.value = value?.reports.find(row => row.state === 'completed')?.id ?? '' })
watch(admissionId, () => { selectionGeneration++; selectedId.value = ''; listCursors.value = ['']; clearReview() }, { flush: 'sync' })
watch(reportId, () => { selectionGeneration++; clearReview() }, { flush: 'sync' })
watch(() => current.value?.id, () => { if (current.value) reportId.value = current.value.source_report_id }, { flush: 'sync' })
watch(selectedId, () => { selectionGeneration++; clearReview(); issueCursors.value = ['']; readMode.value = 'plan' }, { flush: 'sync' })
watch(() => current.value?.confirmed_plan_id, value => { if (value) readMode.value = 'results' })
watch(() => current.value?.latest_plan?.id, () => { clearReview(); issueCursors.value = [''] })
watch(mappingDirty, value => { if (value) confirmation.value = null }, { flush: 'sync' })
watch(access.identity, () => { identityGeneration++; intent.value = null; pending.value = false; uncertain.value = false; actionError.value = ''; selectedId.value = ''; parentId.value = ''; parentCursors.value = [''] }, { flush: 'sync' })
watch(access.scope, clearReview, { flush: 'sync' })
watch([parents.error, parent.error, listing.error, detail.error, source.error, issues.error, admissions.error, admission.error, reports.error, report.error], errors => { if (errors.some(importAccessError)) void refreshWorkspace().catch(() => {}) })
watch(resultVersion, () => { if (access.enabled.value && current.value) { void source.refetch(); void issues.refetch() } })
onBeforeUnmount(() => { disposed = true; clearInterval(timer); intent.value = null; access.remove(access.prefix.value) })
function refresh() { if (access.enabled.value) void access.client.invalidateQueries({ queryKey: access.prefix.value }) }
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const actorGeneration = identityGeneration; const generation = selectionGeneration; const scope = access.scope.value
  const sameIdentity = () => !disposed && actorGeneration === identityGeneration && value.identity === access.identity.value
  const valid = () => sameIdentity() && access.enabled.value && scope === access.scope.value && generation === selectionGeneration
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''
  try {
    const result = value.kind === 'plan' ? await prepareAdmittedMetadataImport(value.body) : value.kind === 'replan' ? await replanAdmittedMetadataImport(value.id, value.body) : value.kind === 'confirm' ? await confirmAdmittedMetadataImport(value.id, value.body) : value.kind === 'retry' ? await retryAdmittedMetadataImport(value.id, value.body.request_id) : value.kind === 'remainder' ? await startAdmittedMetadataRemainder(value.id, value.body.request_id) : await cancelAdmittedMetadataImport(value.id, value.body.request_id)
    if (!valid()) { if (sameIdentity()) { uncertain.value = true; actionError.value = 'Access changed while the request was pending. Retry this same reviewed request to recover its outcome.' }; return }
    intent.value = null; uncertain.value = false; clearReview()
    // Receipts prove the request outcome; only fresh reads supply current state.
    selectedId.value = result.import.id
    if (result.import.workspace_revision !== access.org.value?.workspace_revision) await refreshWorkspace()
    if (sameIdentity()) refresh()
  } catch (error) {
    if (!sameIdentity()) return
    clearReview()
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; await refreshWorkspace().catch(() => {}); return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The result could not be confirmed. Retry this same reviewed request to recover its outcome.' : error instanceof ApiError && error.status === 409 ? 'The plan, source binding or available allowance changed. Refresh and review the current status.' : describeApiError(error, 'Could not complete the metadata import action.')
    refresh()
  } finally { if (sameIdentity()) pending.value = false }
}
function plan() { if (canCreate.value) void submit({ kind: 'plan', identity: access.identity.value, body: { request_id: crypto.randomUUID(), admission_id: admissionId.value, source_report_id: reportId.value } }) }
function apply(mappings: MetadataPatch[]) { const c = current.value; if (!access.enabled.value || !c?.actions.replan || !c.latest_plan || busy.value || mappings.length > 50 || !eligibleSource.value || sourceDirty.value && mappings.length > 0) return; void submit({ kind: 'replan', identity: access.identity.value, id: c.id, body: { request_id: crypto.randomUUID(), expected_plan_revision: c.latest_plan.revision, mappings, ...(sourceDirty.value ? { source_report_id: reportId.value } : {}) } }) }
function reviewConfirm() { const c = current.value; const p = c?.latest_plan; if (!canConfirm.value || !p?.confirmation_digest) return; confirmation.value = { request_id: crypto.randomUUID(), plan_id: p.id, plan_revision: p.revision, confirmation_digest: p.confirmation_digest, workspace_revision: c!.workspace_revision, acknowledgments: { held_count: p.counts.held_count, review_only: true, remaining_data: true } } }
function confirm() { if (current.value && confirmation.value && canConfirm.value) void submit({ kind: 'confirm', identity: access.identity.value, id: current.value.id, body: confirmation.value }) }
function resume() { if (current.value?.actions.retry && !busy.value) void submit({ kind: 'retry', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID() } }) }
function cancel() { if (current.value?.actions.cancel && !busy.value) void submit({ kind: 'cancel', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID() } }) }
function remainder() { if (current.value?.actions.remainder && remainderReview.value && !busy.value) void submit({ kind: 'remainder', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID() } }) }
</script>
<template>
  <div
    id="admitted-metadata-import"
    class="min-w-0 space-y-5"
    data-testid="admitted-metadata-import-panel"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 class="text-section font-semibold">
            Metadata for later-admitted People
          </h2><p class="mt-1 text-small text-text-muted">
            Add first tag and custom-field coverage for one completed or cancelled admission cohort. Review the retained capture, mappings and counted subset before applying it.
          </p>
        </div><button
          v-if="access.enabled.value"
          type="button"
          :class="buttonClasses('secondary')"
          @click="refresh"
        >
          Refresh admitted metadata status
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
          :class="buttonClasses('primary')"
          :disabled="pending"
          class="mt-3"
          @click="submit(intent)"
        >
          Retry the same metadata request
        </button>
        <p
          v-if="parents.error.value || parent.error.value || listing.error.value || detail.error.value || source.error.value || admissions.error.value || reports.error.value || admission.error.value || report.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(parents.error.value ?? parent.error.value ?? listing.error.value ?? detail.error.value ?? source.error.value ?? admissions.error.value ?? reports.error.value ?? admission.error.value ?? report.error.value, 'Could not load metadata import review. Refresh to recover.') }}
        </p>
        <FormField
          v-slot="{ id }"
          label="Completed People import"
          bare
          class="mt-4"
        >
          <select
            :id="id"
            v-model="parentId"
            :class="INPUT_CLASSES"
            :disabled="busy || mappingDirty"
          >
            <option value="">
              Choose a completed People import
            </option><option
              v-for="row in parents.data.value?.imports ?? []"
              :key="row.id"
              :value="row.id"
              :disabled="row.state !== 'completed'"
            >
              {{ snapshotTime(row.created_at) }} · {{ snapshotLabel(row.state) }} · {{ row.id }}
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
            Previous People imports
          </button><button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || !parents.data.value?.next_cursor || parents.isFetching.value"
            @click="parents.data.value?.next_cursor && parentCursors.push(parents.data.value.next_cursor)"
          >
            Older People imports
          </button>
        </div>
        <div class="mt-4 grid min-w-0 gap-4 lg:grid-cols-2">
          <div class="min-w-0">
            <FormField
              v-slot="{ id }"
              label="Terminal admission cohort"
              bare
            >
              <select
                :id="id"
                v-model="admissionId"
                :class="INPUT_CLASSES"
                :disabled="busy || mappingDirty"
              >
                <option value="">
                  Choose an admission cohort
                </option>
                <option
                  v-for="row in admissions.data.value?.items ?? []"
                  :key="row.id"
                  :value="row.id"
                  :disabled="!['completed','cancelled'].includes(row.state) || BigInt(row.progress.settled_items) === 0n"
                >
                  {{ snapshotTime(row.created_at) }} · {{ snapshotLabel(row.state) }} · {{ row.progress.settled_items }} settled People · {{ row.id }}
                </option>
              </select>
            </FormField>
            <div class="mt-2 flex flex-wrap gap-2">
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || mappingDirty || admissionCursors.length < 2 || admissions.isFetching.value"
                @click="admissionCursors.pop()"
              >
                Previous admission cohorts
              </button>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || mappingDirty || !admissions.data.value?.next_cursor || admissions.isFetching.value"
                @click="admissions.data.value?.next_cursor && admissionCursors.push(admissions.data.value.next_cursor)"
              >
                Older admission cohorts
              </button>
            </div>
          </div>
          <div class="min-w-0">
            <FormField
              v-slot="{ id }"
              label="Retained metadata source report"
              bare
            >
              <select
                :id="id"
                v-model="reportId"
                :class="INPUT_CLASSES"
                :disabled="busy || mappingDirty || !!current?.confirmed_plan_id"
              >
                <option value="">
                  Choose a completed report
                </option>
                <option
                  v-for="row in reports.data.value?.reports ?? []"
                  :key="row.id"
                  :value="row.id"
                  :disabled="row.state !== 'completed'"
                >
                  {{ snapshotTime(row.inputs.newer.started_at) }} · {{ snapshotLabel(row.state) }} · {{ row.id }}
                </option>
              </select>
            </FormField>
            <div class="mt-2 flex flex-wrap gap-2">
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || mappingDirty || reportCursors.length < 2 || reports.isFetching.value"
                @click="reportCursors.pop()"
              >
                Previous source reports
              </button>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || mappingDirty || !reports.data.value?.next_cursor || reports.isFetching.value"
                @click="reports.data.value?.next_cursor && reportCursors.push(reports.data.value.next_cursor)"
              >
                Older source reports
              </button>
            </div>
          </div>
        </div>
        <CoreChangeCaptureBoundary
          v-if="report.data.value"
          class="mt-3"
          :boundary="report.data.value.inputs.newer"
          title="Selected metadata capture"
          @review-snapshot="emit('reviewSnapshot', $event)"
        />
        <p
          v-if="parentId && (!eligibleParent || !eligibleAdmission || !eligibleSource)"
          role="status"
          class="mt-3 text-small text-text-muted"
        >
          Choose a completed original import, a terminal admission with settled People, and a completed retained report with exhausted People and custom-field streams. The report must be from this admission or a strictly later capture.
        </p>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!canCreate"
          class="mt-3"
          @click="plan"
        >
          Prepare admitted metadata plan
        </button>
        <p class="mt-3 text-small text-text-muted">
          Only settled People from this admission are included. Tags cover embedded People arrays; notes, tasks and history still need later family steps. Preparing the first plan enables the compatible shared catalog registry, which remains active after cancellation.
        </p>
        <FormField
          v-if="listing.data.value?.imports.length"
          v-slot="{ id }"
          label="Admitted metadata import"
          bare
          class="mt-4"
        >
          <select
            :id="id"
            v-model="selectedId"
            :class="INPUT_CLASSES"
            :disabled="busy || mappingDirty"
          >
            <option
              v-for="row in listing.data.value.imports"
              :key="row.id"
              :value="row.id"
            >
              {{ snapshotTime(row.created_at) }} · {{ snapshotLabel(row.state) }} · {{ row.id }}
            </option>
          </select>
        </FormField>
        <div
          v-if="admissionId"
          class="mt-2 flex flex-wrap gap-2"
        >
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || mappingDirty || listCursors.length < 2 || listing.isFetching.value"
            @click="listCursors.pop()"
          >
            Previous metadata imports
          </button>
          <button
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || mappingDirty || !listing.data.value?.next_cursor || listing.isFetching.value"
            @click="listing.data.value?.next_cursor && listCursors.push(listing.data.value.next_cursor)"
          >
            Older metadata imports
          </button>
        </div>
      </template>
    </Card>
    <template v-if="current">
      <CoreChangeCaptureBoundary
        :boundary="current.source_capture_interval"
        title="Frozen metadata capture"
        @review-snapshot="emit('reviewSnapshot', $event)"
      />
      <Card class="min-w-0">
        <h2 class="text-section font-semibold">
          Metadata import · {{ snapshotLabel(current.state) }}
        </h2>
        <p
          role="status"
          class="mt-1 text-small text-text-muted"
        >
          {{ snapshotLabel(current.phase) }}<span v-if="current.latest_plan"> · Plan {{ current.latest_plan.revision }}: {{ snapshotLabel(current.latest_plan.state) }} / {{ snapshotLabel(current.latest_plan.phase) }}</span>
        </p>
        <p
          v-if="current.pause_reason || current.latest_plan?.pause_reason"
          role="alert"
          class="mt-2 text-small text-danger"
        >
          {{ snapshotLabel(current.pause_reason ?? current.latest_plan?.pause_reason ?? '') }}. Resolve the condition, refresh and explicitly resume when available.
        </p>
        <p
          v-if="current.state === 'cancelled'"
          class="mt-2 text-small"
        >
          This attempt is cancelled. Committed metadata, shared catalog claims and held terminal results remain retained. A confirmed attempt may continue only its exact never-settled remainder.
        </p>
        <p
          v-if="current.state === 'completed'"
          class="mt-2 text-small"
        >
          Metadata processing finished. Review held items and remaining source families before considering migration complete.
        </p>
        <p class="mt-2 text-small text-text-muted">
          {{ current.counts.people.settled }} / {{ current.counts.people.eligible }} eligible People settled. {{ current.counts.people.excluded }} People are excluded from this metadata import. {{ current.counts.invalid_source_ids }} invalid source identities.
        </p>
        <p class="mt-2 text-small text-text-muted">
          Cohort: {{ current.cohort_counts.settled_people }} admitted People; {{ current.cohort_counts.eligible_people }} eligible, {{ current.cohort_counts.excluded_people }} excluded. {{ current.cohort_counts.remaining_people }} remain. Preparation: {{ current.progress.fields_processed }} fields and {{ current.progress.people_processed }} People checked.
        </p>
        <p
          v-if="sourceDirty"
          role="status"
          class="mt-2 text-small text-warning"
        >
          A different retained source is selected. Prepare a fresh plan to discard source-dependent choices and review its new counts.
        </p>
        <p
          v-if="planExpired"
          role="status"
          class="mt-2 text-small text-warning"
        >
          This ready plan has expired. Prepare a fresh plan before confirmation.
        </p>
        <div class="mt-3 min-w-0 overflow-x-auto">
          <table class="w-full min-w-[760px] text-left text-small">
            <thead class="text-text-muted">
              <tr>
                <th class="p-2 font-medium">
                  Data
                </th><th
                  v-for="label in ['Planned', 'Eligible', 'Created', 'Applied', 'Already present', 'Held', 'Not supplied', 'Source null', 'Pending']"
                  :key="label"
                  class="p-2 font-medium"
                >
                  {{ label }}
                </th>
              </tr>
            </thead><tbody>
              <tr
                v-for="family in (['tags', 'fields', 'options', 'tag_links', 'values'] as const)"
                :key="family"
                class="border-t border-border"
              >
                <th class="p-2 font-medium">
                  {{ snapshotLabel(family) }}
                </th><td
                  v-for="field in (['planned', 'eligible', 'created', 'applied', 'already_present', 'held', 'not_supplied', 'source_null', 'pending'] as const)"
                  :key="field"
                  class="p-2"
                >
                  {{ current.counts[family][field] }}
                </td>
              </tr>
            </tbody>
          </table>
        </div>
        <p class="mt-3 text-small text-text-muted">
          Native limits: 200 tags per Organization, 20 per Person, 50 live custom fields and 50 options per field. A Person tag set that exceeds its limit is held together. Existing differing values remain held.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            v-if="current.actions.replan"
            type="button"
            :class="buttonClasses('secondary')"
            :disabled="busy || mappingDirty"
            @click="apply([])"
          >
            {{ sourceDirty ? 'Prepare plan from selected source' : 'Prepare fresh metadata plan' }}
          </button><button
            v-if="current.actions.retry"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="busy"
            @click="resume"
          >
            Resume metadata import
          </button><button
            v-if="current.actions.cancel"
            type="button"
            :class="buttonClasses('danger')"
            :disabled="busy"
            @click="cancelReview = true"
          >
            Cancel metadata import
          </button><button
            v-if="current.actions.remainder"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="busy"
            @click="remainderReview = true"
          >
            Review exact remainder
          </button>
        </div>
        <p
          v-if="current.remainder.available || current.remainder.successor_import_id"
          class="mt-2 text-small"
        >
          Exact remainder: {{ current.remainder.remaining_catalog }} catalog units and {{ current.remainder.remaining_people }} People. Excludes {{ current.remainder.excluded_settled_catalog }} settled catalog units, {{ current.remainder.excluded_settled_people }} settled People and {{ current.remainder.excluded_held_people }} held People.
          <button
            v-if="current.remainder.successor_import_id"
            type="button"
            :class="buttonClasses('ghost')"
            :disabled="busy || mappingDirty"
            @click="selectedId = current.remainder.successor_import_id"
          >
            Open remainder attempt
          </button>
        </p>
        <p
          v-if="current.latest_plan?.expires_at"
          class="mt-2 text-small text-text-muted"
        >
          Ready plan expires {{ snapshotTime(current.latest_plan.expires_at) }}. An expired plan requires fresh preparation and review.
        </p>
        <details class="mt-4 text-small">
          <summary class="cursor-pointer font-medium">
            Source coverage and import references
          </summary><dl class="mt-2 break-all">
            <dt>Admission cohort</dt><dd>{{ current.admission_id }}</dd><dt>Retained source report</dt><dd>{{ current.source_report_id }}</dd><dt>Parent People import</dt><dd>{{ current.parent_import_id }}</dd><dt>Metadata import</dt><dd>{{ current.id }}</dd><dt>Selected source snapshot</dt><dd>{{ current.snapshot_id }}</dd><dt>FUB account / capture boundary</dt><dd>{{ current.source_account_id }} / {{ current.capture_sequence }}</dd>
          </dl><p class="mt-2">
            Remaining: {{ current.coverage.remaining_data.map(snapshotLabel).join(', ') || 'See source-family review' }}.
          </p><p
            v-for="gap in current.coverage.source_gaps"
            :key="gap"
          >
            {{ snapshotLabel(gap) }}
          </p><p class="mt-2">
            Communication restrictions are not enforced by this import. The review hold and outbound restrictions stay in place.
          </p>
        </details>
        <section
          class="mt-4 text-small"
          aria-label="Metadata hold reasons"
        >
          <h3 class="font-medium">
            Reasons and holds
          </h3><p
            v-if="issues.error.value"
            role="alert"
            class="mt-2 text-danger"
          >
            {{ describeApiError(issues.error.value, 'Could not load issue counts.') }}
          </p><ul class="mt-2 space-y-1">
            <li
              v-for="issue in issues.data.value?.items ?? []"
              :key="issue.code"
            >
              {{ snapshotLabel(issue.code) }}: {{ issue.count }}
            </li>
          </ul><div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="issueCursors.length < 2 || issues.isFetching.value"
              @click="issueCursors.pop()"
            >
              Previous reasons
            </button><button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="!issues.data.value?.next_cursor || issues.isFetching.value"
              @click="issues.data.value?.next_cursor && issueCursors.push(issues.data.value.next_cursor)"
            >
              More reasons
            </button><button
              v-if="issues.error.value"
              type="button"
              :class="buttonClasses('secondary')"
              @click="issues.refetch()"
            >
              Retry reasons
            </button>
          </div>
        </section>
        <section
          class="mt-4 border-t border-border pt-4 text-small"
          aria-label="Metadata storage accounting"
        >
          <h3 class="font-medium">
            Metadata storage accounting
          </h3><p class="mt-2">
            Retained {{ formatBytes(current.retained_bytes) }} · reserved {{ formatBytes(current.reserved_bytes) }} · cancellation reserve {{ formatBytes(current.cancellation_reserved_bytes) }}.
          </p><p class="mt-1">
            The largest atomic unit adds at most {{ formatBytes(current.latest_plan?.max_added_byte_bound ?? '0') }}. Each atomic unit is limited to {{ formatBytes(current.policy.unit_byte_limit) }}.
          </p><p class="mt-1 text-text-muted">
            These logical retained-data amounts share the snapshot and Organization allowances. Native CRM rows, database indexes, WAL and replicas also consume physical disk.
          </p>
        </section>
        <SnapshotBudgetPanel
          v-if="source.data.value"
          :key="`${access.scope.value}:${source.data.value.snapshot.id}`"
          :snapshot="source.data.value.snapshot"
          @refresh="refresh"
          @access-denied="refreshWorkspace"
        />
      </Card>
      <MetadataMappingPanel
        v-if="current.latest_plan"
        :key="`${access.scope.value}:${current.id}:${current.latest_plan.id}`"
        :readers="admittedMetadataReaders"
        :import-id="current.id"
        :plan-id="current.latest_plan.id"
        :revision="current.latest_plan.revision"
        :progress-version="resultVersion"
        :can-replan="current.actions.replan && !busy && !sourceDirty"
        @apply="apply"
        @dirty="mappingDirty = $event"
      />
      <Card
        v-if="!current.confirmed_plan_id && current.state !== 'cancelled'"
        class="min-w-0"
      >
        <h2 class="text-section font-semibold">
          Review before applying metadata
        </h2>
        <p
          v-if="!current.release_ready"
          class="mt-2 text-small text-danger"
        >
          Compatible migration services are required before this plan can be confirmed.
        </p>
        <p
          v-if="mappingDirty"
          class="mt-2 text-small text-danger"
        >
          Apply or discard draft mapping choices before confirmation.
        </p>
        <div class="mt-3 space-y-3 text-small">
          <label class="flex items-start gap-2"><input
            v-model="heldAck"
            type="checkbox"
            :disabled="busy"
          >I reviewed the {{ current.latest_plan?.counts.held_count ?? current.counts.held_count }} held tag, field, option, link and value items, and all other reported source issues.</label><label class="flex items-start gap-2"><input
            v-model="reviewAck"
            type="checkbox"
            :disabled="busy"
          >Keep this Organization in administrator review, without normal agent use or outbound actions.</label><label class="flex items-start gap-2"><input
            v-model="remainingAck"
            type="checkbox"
            :disabled="busy"
          >I understand that notes, tasks and other remaining data still need later import steps.</label>
        </div>
        <button
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!canConfirm"
          class="mt-4"
          @click="reviewConfirm"
        >
          Review metadata confirmation
        </button>
      </Card>
      <div
        v-if="current.latest_plan"
        class="min-w-0 space-y-3"
      >
        <div class="flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses(readMode === 'plan' ? 'primary' : 'secondary')"
            :aria-pressed="readMode === 'plan'"
            @click="readMode = 'plan'"
          >
            Planned metadata
          </button><button
            type="button"
            :class="buttonClasses(readMode === 'results' ? 'primary' : 'secondary')"
            :aria-pressed="readMode === 'results'"
            @click="readMode = 'results'"
          >
            Metadata reconciliation
          </button>
        </div><MetadataRecordPanel
          :readers="admittedMetadataReaders"
          :import-id="current.id"
          :plan-id="current.confirmed_plan_id ?? current.latest_plan.id"
          :mode="readMode"
          :progress-version="resultVersion"
        />
      </div>
    </template>
    <ConfirmDialog
      :visible="!!confirmation && access.enabled.value"
      title="Apply reviewed tags and custom fields"
      :message="confirmationText"
      confirm-label="Confirm metadata import"
      :is-pending="pending"
      @update:visible="value => { if (!value) confirmation = null }"
      @confirm="confirm"
    />
    <ConfirmDialog
      :visible="cancelReview && access.enabled.value"
      title="Cancel remaining admitted metadata work"
      message="Cancel all uncommitted work. Settled metadata, held results and shared catalogs remain retained. A confirmed attempt may later continue only its exact never-settled remainder using the same source and mappings. The workspace stays in administrator review."
      confirm-label="Cancel remaining work"
      confirm-variant="danger"
      :is-pending="pending"
      @update:visible="value => { if (!value) cancelReview = false }"
      @confirm="cancel"
    />
    <ConfirmDialog
      :visible="remainderReview && access.enabled.value"
      title="Continue the exact unprocessed remainder"
      :message="current ? `Queue ${current.remainder.remaining_catalog} remaining catalog units and ${current.remainder.remaining_people} People from the same confirmed source and mappings. Committed and held terminal units are excluded. This creates or recovers the one allowed successor.` : ''"
      confirm-label="Continue exact remainder"
      :is-pending="pending"
      @update:visible="value => { if (!value) remainderReview = false }"
      @confirm="remainder"
    />
  </div>
</template>
