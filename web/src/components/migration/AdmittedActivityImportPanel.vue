<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from 'vue'
import { provideAdmittedActivityReaders } from './activityFamily'
import { useQuery } from '@tanstack/vue-query'
import Card from '../Card.vue'
import ConfirmDialog from '../ConfirmDialog.vue'
import FormField from '../FormField.vue'
import ActivityMappingPanel from './ActivityMappingPanel.vue'
import ActivityRecordPanel from './ActivityRecordPanel.vue'
import SnapshotBudgetPanel from './SnapshotBudgetPanel.vue'
import { ApiError } from '../../api/client'
import { fetchImport, fetchImports, importAccessError, uncertainImportError } from '../../api/imports'
import { fetchPeopleAdmission, fetchPeopleAdmissions } from '../../api/peopleAdmissions'
import { fetchCoreChangeReport, fetchCoreChangeReports } from '../../api/coreChangeReports'
import { fetchSnapshot } from '../../api/snapshots'
import { cancelAdmittedActivityImport, confirmAdmittedActivityImport, createAdmittedActivityRemainder, fetchAdmittedActivityImport, fetchAdmittedActivityImports, activityActive, proposeAdmittedActivityImport, replanAdmittedActivityImport, retryAdmittedActivityImport, useActivityAccess, type ActivityConfirm, type ActivityPatch, type ActivityReplan } from '../../api/admittedActivityImports'
import { buttonClasses, INPUT_CLASSES } from '../../lib/controls'
import { describeApiError } from '../../lib/errors'
import { formatBytes, snapshotLabel, snapshotTime } from './format'
const props = defineProps<{ refreshWorkspace: () => Promise<void> }>()
provideAdmittedActivityReaders()
const access = useActivityAccess()
const parentId = ref('')
const admissionId = ref('')
const reportId = ref('')
const selectedId = ref('')
const parentCursors = ref<string[]>([''])
const admissionCursors = ref<string[]>([''])
const reportCursors = ref<string[]>([''])
const attemptCursors = ref<string[]>([''])
const readMode = ref<'plan' | 'results'>('plan')
const heldAck = ref(false)
const reviewAck = ref(false)
const remainingAck = ref(false)
const patches = ref<ActivityPatch[]>([])
const resetVersion = ref(0)
const zone = ref('')
const zoneAck = ref(false)
const zoneDirty = computed(() => zone.value.trim() !== (current.value?.latest_plan.source_timezone ?? ''))
const dirty = computed(() => patches.value.length > 0 || zoneDirty.value)
const confirmation = ref<ActivityConfirm | null>(null)
const cancelReview = ref(false)
const pending = ref(false)
const uncertain = ref(false)
const actionError = ref('')
const failures = ref(0)
let disposed = false
let identityGeneration = 0
let selectionGeneration = 0
type Intent =
  | { kind: 'plan'; identity: string; body: { request_id: string; admission_id: string; report_id: string } }
  | { kind: 'replan'; identity: string; id: string; body: ActivityReplan }
  | { kind: 'confirm'; identity: string; id: string; body: ActivityConfirm }
  | { kind: 'retry' | 'cancel' | 'remainder'; identity: string; id: string; body: { request_id: string; expected_revision: string } }
const intent = ref<Intent | null>(null)
const parentsKey = computed(() => [...access.prefix.value, 'parents', parentCursors.value.at(-1) ?? ''])
const parentKey = computed(() => [...access.prefix.value, 'parent', parentId.value])
const admissionsKey = computed(() => [...access.prefix.value, 'admissions', parentId.value, admissionCursors.value.at(-1) ?? ''])
const admissionKey = computed(() => [...access.prefix.value, 'admission', admissionId.value])
const reportsKey = computed(() => [...access.prefix.value, 'reports', parentId.value, reportCursors.value.at(-1) ?? ''])
const reportKey = computed(() => [...access.prefix.value, 'report', reportId.value])
const listKey = computed(() => [...access.prefix.value, 'list', admissionId.value, attemptCursors.value.at(-1) ?? ''])
const detailKey = computed(() => [...access.prefix.value, 'detail', selectedId.value])
const parents = useQuery({ queryKey: parentsKey, enabled: access.enabled, retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentsKey.value, () => parentsKey.value, () => fetchImports(parentCursors.value.at(-1) || undefined, signal)) })
const parent = useQuery({ queryKey: parentKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(parentKey.value, () => parentKey.value, () => fetchImport(parentId.value, signal)) })
const admissions = useQuery({ queryKey: admissionsKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(admissionsKey.value, () => admissionsKey.value, () => fetchPeopleAdmissions(parentId.value, admissionCursors.value.at(-1) || undefined, signal)) })
const admission = useQuery({ queryKey: admissionKey, enabled: computed(() => access.enabled.value && !!admissionId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(admissionKey.value, () => admissionKey.value, () => fetchPeopleAdmission(admissionId.value, signal)) })
const reports = useQuery({ queryKey: reportsKey, enabled: computed(() => access.enabled.value && !!parentId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(reportsKey.value, () => reportsKey.value, () => fetchCoreChangeReports(parentId.value, reportCursors.value.at(-1) || undefined, signal)) })
const report = useQuery({ queryKey: reportKey, enabled: computed(() => access.enabled.value && !!reportId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(reportKey.value, () => reportKey.value, () => fetchCoreChangeReport(reportId.value, signal)) })
const listing = useQuery({ queryKey: listKey, enabled: computed(() => access.enabled.value && !!admissionId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(listKey.value, () => listKey.value, () => fetchAdmittedActivityImports(admissionId.value, attemptCursors.value.at(-1) || undefined, signal)) })
const detail = useQuery({
  queryKey: detailKey, enabled: computed(() => access.enabled.value && !!selectedId.value), retry: false, gcTime: 0,
  queryFn: async ({ signal }) => {
    const expected = access.scope.value
    try {
      const result = await access.read(detailKey.value, () => detailKey.value, async () => {
        const value = await fetchAdmittedActivityImport(selectedId.value, signal)
        if (expected === access.scope.value && value.workspace_revision !== access.org.value?.workspace_revision) { await props.refreshWorkspace(); throw new Error('Workspace status was refreshed') }
        return value
      })
      failures.value = 0; return result
    } catch (error) { if (expected === access.scope.value && !signal.aborted) failures.value++; throw error }
  },
  refetchInterval: query => activityActive(query.state.data) ? Math.min(30_000, 2_000 * 2 ** Math.min(failures.value, 4)) : false,
  refetchOnWindowFocus: 'always', refetchOnReconnect: 'always',
})
const current = computed(() => access.enabled.value ? detail.data.value : undefined)
const sourceId = computed(() => current.value?.snapshot_id ?? report.data.value?.inputs.newer.snapshot_id ?? '')
const sourceKey = computed(() => [...access.prefix.value, 'snapshot', sourceId.value])
const source = useQuery({ queryKey: sourceKey, enabled: computed(() => access.enabled.value && !!sourceId.value), retry: false, gcTime: 0, queryFn: ({ signal }) => access.read(sourceKey.value, () => sourceKey.value, () => fetchSnapshot(sourceId.value, signal)) })
const busy = computed(() => pending.value || !!intent.value)
const requiredStreams = ['people', 'users', 'notes', 'note_detail', 'tasks_open', 'tasks_completed']
const eligibleParent = computed(() => access.enabled.value && access.org.value?.workspace_mode === 'migration_review' && parent.data.value?.state === 'completed' && !!parent.data.value.confirmed_plan_id)
const eligibleAdmission = computed(() => !!admission.data.value && ['completed', 'cancelled'].includes(admission.data.value.state) && admission.data.value.parent_import_id === parentId.value && BigInt(admission.data.value.progress.settled_items) > 0n)
const eligibleSource = computed(() => report.data.value?.state === 'completed' && report.data.value.parent_import_id === parentId.value && !!report.data.value.output_revision && requiredStreams.every(name => report.data.value?.inputs.newer.streams.some(stream => stream.stream === name && stream.state === 'completed')))
const sourceDirty = computed(() => !!current.value && reportId.value !== current.value.source_report_id)
const canCreate = computed(() => eligibleParent.value && eligibleAdmission.value && eligibleSource.value && listing.isSuccess.value && listing.data.value?.imports.every(row => row.state === 'cancelled' && !row.confirmed_plan_id) && !busy.value)
const canConfirm = computed(() => current.value?.release_ready === true && current.value.actions.confirm && heldAck.value && reviewAck.value && remainingAck.value && !busy.value && !dirty.value && !sourceDirty.value)
const canApply = computed(() => access.enabled.value && current.value?.actions.replan && !busy.value && dirty.value && patches.value.length <= 50 && (!zoneDirty.value || !zone.value.trim() || zoneAck.value))
const resultVersion = computed(() => current.value ? JSON.stringify([current.value.state, current.value.phase, current.value.updated_at, current.value.counts, current.value.latest_plan]) : '')
const confirmationText = computed(() => {
  const c = current.value; const p = c?.latest_plan
  return c && p ? `Apply notes and tasks plan ${p.revision}: ${p.counts.notes.eligible} notes and ${p.counts.tasks.eligible} tasks are eligible. ${p.counts.held_count} records stay held; ${p.counts.source_only_count} source-only components remain retained across distinct observations. Source timezone: ${p.source_timezone ?? 'not confirmed'}. The Organization stays in administrator review. Replies, reactions, attachments, reminders, recurrence and other remaining data are not made operational. Cancellation keeps already settled work and permanently ends this child import.` : ''
})
function discard() { patches.value = []; zone.value = current.value?.latest_plan.source_timezone ?? ''; zoneAck.value = false; resetVersion.value++ }
function clearReview() { confirmation.value = null; cancelReview.value = false; heldAck.value = false; reviewAck.value = false; remainingAck.value = false }
watch(parents.data, value => { if (access.enabled.value && !parentId.value) parentId.value = value?.imports.find(row => row.state === 'completed')?.id ?? '' })
watch(listing.data, value => { if (access.enabled.value && !selectedId.value && value?.imports.length) selectedId.value = (value.imports.find(row => !['cancelled', 'completed'].includes(row.state)) ?? value.imports[0])!.id })
watch(parentId, () => { selectionGeneration++; admissionId.value = ''; reportId.value = ''; selectedId.value = ''; admissionCursors.value = ['']; reportCursors.value = ['']; clearReview() }, { flush: 'sync' })
watch(admissions.data, value => { if (access.enabled.value && !admissionId.value) admissionId.value = value?.items.find(row => ['completed', 'cancelled'].includes(row.state) && BigInt(row.progress.settled_items) > 0n)?.id ?? '' })
watch(reports.data, value => { if (access.enabled.value && !reportId.value) reportId.value = value?.reports.find(row => row.state === 'completed')?.id ?? '' })
watch(admissionId, () => { selectionGeneration++; selectedId.value = ''; attemptCursors.value = ['']; clearReview() }, { flush: 'sync' })
watch(reportId, () => { selectionGeneration++; clearReview() }, { flush: 'sync' })
watch(() => current.value?.id, () => { if (current.value) reportId.value = current.value.source_report_id }, { flush: 'sync' })
watch(() => attemptCursors.value.at(-1), () => { selectedId.value = ''; clearReview() }, { flush: 'sync' })
watch(selectedId, () => { selectionGeneration++; clearReview(); readMode.value = 'plan' }, { flush: 'sync' })
watch(() => current.value?.confirmed_plan_id, value => { if (value) readMode.value = 'results' })
watch(() => current.value?.latest_plan?.id, () => { clearReview(); discard() })
watch(dirty, value => { if (value) confirmation.value = null }, { flush: 'sync' })
watch(zone, () => { zoneAck.value = false; confirmation.value = null })
watch(() => current.value?.revision, clearReview)
watch(access.identity, () => { identityGeneration++; intent.value = null; pending.value = false; uncertain.value = false; actionError.value = ''; selectedId.value = ''; parentId.value = ''; admissionId.value = ''; reportId.value = ''; parentCursors.value = ['']; admissionCursors.value = ['']; reportCursors.value = ['']; attemptCursors.value = [''] }, { flush: 'sync' })
watch(access.scope, clearReview, { flush: 'sync' })
watch([parents.error, parent.error, admissions.error, admission.error, reports.error, report.error, listing.error, detail.error, source.error], errors => { if (errors.some(importAccessError)) void props.refreshWorkspace().catch(() => {}) })
watch(resultVersion, () => { if (access.enabled.value && current.value) { void source.refetch() } })
onBeforeUnmount(() => { disposed = true; intent.value = null; access.remove(access.prefix.value) })
function refresh() { if (access.enabled.value) void access.client.invalidateQueries({ queryKey: access.prefix.value }) }
async function submit(value: Intent) {
  if (pending.value || !access.enabled.value || value.identity !== access.identity.value) return
  const actorGeneration = identityGeneration; const generation = selectionGeneration; const scope = access.scope.value
  const sameIdentity = () => !disposed && actorGeneration === identityGeneration && value.identity === access.identity.value
  const valid = () => sameIdentity() && access.enabled.value && scope === access.scope.value && generation === selectionGeneration
  intent.value = value; pending.value = true; uncertain.value = false; actionError.value = ''
  try {
    const result = value.kind === 'plan' ? await proposeAdmittedActivityImport(value.body) : value.kind === 'replan' ? await replanAdmittedActivityImport(value.id, value.body) : value.kind === 'confirm' ? await confirmAdmittedActivityImport(value.id, value.body) : value.kind === 'retry' ? await retryAdmittedActivityImport(value.id, value.body) : value.kind === 'remainder' ? await createAdmittedActivityRemainder(value.id, value.body) : await cancelAdmittedActivityImport(value.id, value.body)
    if (!valid()) { if (sameIdentity()) { uncertain.value = true; actionError.value = 'Access changed while the request was pending. Retry this same reviewed request to recover its outcome.' }; return }
    intent.value = null; uncertain.value = false; clearReview()
    // Receipts prove the request outcome; only fresh reads supply current state.
    selectedId.value = result.import.id
    if (result.import.workspace_revision !== access.org.value?.workspace_revision) await props.refreshWorkspace()
    if (sameIdentity()) refresh()
  } catch (error) {
    if (!sameIdentity()) return
    clearReview()
    if (importAccessError(error)) { intent.value = null; uncertain.value = false; await props.refreshWorkspace().catch(() => {}); return }
    uncertain.value = uncertainImportError(error)
    if (!uncertain.value) intent.value = null
    actionError.value = uncertain.value ? 'The result could not be confirmed. Retry this same reviewed request to recover its outcome.' : error instanceof ApiError && error.status === 409 ? 'The plan, source binding or available allowance changed. Refresh and review the current status.' : describeApiError(error, 'Could not complete the activity import action.')
    refresh()
  } finally { if (sameIdentity()) pending.value = false }
}
function plan() { if (canCreate.value) void submit({ kind: 'plan', identity: access.identity.value, body: { request_id: crypto.randomUUID(), admission_id: admissionId.value, report_id: reportId.value } }) }
function apply() {
  const c = current.value
  if (!c || !canApply.value) return
  const body: ActivityReplan = { request_id: crypto.randomUUID(), expected_plan_id: c.latest_plan.id, choices: patches.value.map(p => ({ mapping_id: p.mapping_id, choice: { ...p.choice } })) }
  if (zoneDirty.value) body.source_timezone = zone.value.trim() || null
  void submit({ kind: 'replan', identity: access.identity.value, id: c.id, body })
}
function fresh() { const c = current.value; if (sourceDirty.value && c && !c.confirmed_plan_id && c.actions.cancel && !busy.value) { cancelReview.value = true; return }; if (c?.actions.replan && !busy.value && !dirty.value) void submit({ kind: 'replan', identity: access.identity.value, id: c.id, body: { request_id: crypto.randomUUID(), expected_plan_id: c.latest_plan.id, choices: [] } }) }
function reviewConfirm() { const c = current.value; const p = c?.latest_plan; if (!canConfirm.value || !p || !c) return; confirmation.value = { request_id: crypto.randomUUID(), plan_id: p.id, expected_revision: c.revision, acknowledge_held: p.counts.held_count, acknowledge_source_only: p.counts.source_only_count } }
function confirm() { if (current.value && confirmation.value && canConfirm.value) void submit({ kind: 'confirm', identity: access.identity.value, id: current.value.id, body: confirmation.value }) }
function resume() { if (current.value?.actions.retry && !busy.value) void submit({ kind: 'retry', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID(), expected_revision: current.value.revision } }) }
function cancel() { if (current.value?.actions.cancel && !busy.value) void submit({ kind: 'cancel', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID(), expected_revision: current.value.revision } }) }
function remainder() { if (current.value?.actions.remainder && !busy.value) void submit({ kind: 'remainder', identity: access.identity.value, id: current.value.id, body: { request_id: crypto.randomUUID(), expected_revision: current.value.revision } }) }
</script>

<template>
  <div
    class="min-w-0 space-y-5"
    data-testid="activity-import-panel"
  >
    <Card class="min-w-0">
      <div class="flex flex-wrap items-center justify-between gap-3">
        <div>
          <h2 class="text-section font-medium">
            Notes and tasks for later-admitted People
          </h2><p class="mt-1 text-small text-text-muted">
            Add retained notes and tasks for one terminal admission cohort. Review the selected sealed capture and counted plan before applying it.
          </p>
        </div><button
          v-if="access.enabled.value"
          type="button"
          :class="buttonClasses()"
          @click="refresh"
        >
          Refresh activity import status
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
          Retry the same activity request
        </button>
        <p
          v-if="parents.error.value || parent.error.value || admissions.error.value || admission.error.value || reports.error.value || report.error.value || listing.error.value || detail.error.value || source.error.value"
          role="alert"
          class="mt-3 text-small text-danger"
        >
          {{ describeApiError(parents.error.value ?? parent.error.value ?? admissions.error.value ?? admission.error.value ?? reports.error.value ?? report.error.value ?? listing.error.value ?? detail.error.value ?? source.error.value, 'Could not load admitted activity review. Refresh to recover.') }}
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
            :disabled="busy"
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
                :disabled="busy || dirty"
              >
                <option value="">
                  Choose an admission cohort
                </option>
                <option
                  v-for="row in admissions.data.value?.items ?? []"
                  :key="row.id"
                  :value="row.id"
                  :disabled="!['completed', 'cancelled'].includes(row.state) || BigInt(row.progress.settled_items) === 0n"
                >
                  {{ snapshotTime(row.created_at) }} · {{ snapshotLabel(row.state) }} · {{ row.progress.settled_items }} settled People · {{ row.id }}
                </option>
              </select>
            </FormField>
            <div class="mt-2 flex flex-wrap gap-2">
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || dirty || admissionCursors.length < 2 || admissions.isFetching.value"
                @click="admissionCursors.pop()"
              >
                Previous admission cohorts
              </button>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || dirty || !admissions.data.value?.next_cursor || admissions.isFetching.value"
                @click="admissions.data.value?.next_cursor && admissionCursors.push(admissions.data.value.next_cursor)"
              >
                Older admission cohorts
              </button>
            </div>
          </div>
          <div class="min-w-0">
            <FormField
              v-slot="{ id }"
              label="Retained activity source report"
              bare
            >
              <select
                :id="id"
                v-model="reportId"
                :class="INPUT_CLASSES"
                :disabled="busy || dirty || !!current?.confirmed_plan_id"
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
                :disabled="busy || dirty || reportCursors.length < 2 || reports.isFetching.value"
                @click="reportCursors.pop()"
              >
                Previous source reports
              </button>
              <button
                type="button"
                :class="buttonClasses('ghost')"
                :disabled="busy || dirty || !reports.data.value?.next_cursor || reports.isFetching.value"
                @click="reports.data.value?.next_cursor && reportCursors.push(reports.data.value.next_cursor)"
              >
                Older source reports
              </button>
            </div>
          </div>
        </div>
        <p
          v-if="parentId && (!eligibleParent || !eligibleAdmission || !eligibleSource)"
          role="status"
          class="mt-3 text-small text-text-muted"
        >
          This step requires a completed People import, a completed or cancelled admission with settled People, and a sealed core-change report whose People, users, notes, note detail, open-task and completed-task streams completed.
        </p>
        <div
          v-if="listing.data.value?.imports.length || current || attemptCursors.length > 1"
          class="mt-4 min-w-0"
        >
          <FormField
            v-slot="{ id }"
            label="Activity import attempt"
            bare
          >
            <select
              :id="id"
              v-model="selectedId"
              :class="INPUT_CLASSES"
              :disabled="busy || dirty || listing.isFetching.value"
            >
              <option value="">
                Choose an activity import attempt
              </option>
              <option
                v-if="current && !listing.data.value?.imports.some(row => row.id === current?.id)"
                :value="current.id"
              >
                {{ snapshotTime(current.created_at) }} · {{ snapshotLabel(current.state) }} · {{ current.id }}
              </option>
              <option
                v-for="row in listing.data.value?.imports ?? []"
                :key="row.id"
                :value="row.id"
              >
                {{ snapshotTime(row.created_at) }} · {{ snapshotLabel(row.state) }} · {{ row.id }}
              </option>
            </select>
          </FormField>
          <div class="mt-2 flex flex-wrap gap-2">
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || dirty || attemptCursors.length < 2 || listing.isFetching.value"
              @click="attemptCursors.pop()"
            >
              Previous activity attempts
            </button>
            <button
              type="button"
              :class="buttonClasses('ghost')"
              :disabled="busy || dirty || !listing.data.value?.next_cursor || listing.isFetching.value"
              @click="listing.data.value?.next_cursor && attemptCursors.push(listing.data.value.next_cursor)"
            >
              More activity attempts
            </button>
          </div>
        </div>
        <button
          v-if="!selectedId || (current?.state === 'cancelled' && !current.confirmed_plan_id)"
          type="button"
          :class="buttonClasses('primary')"
          :disabled="!canCreate"
          class="mt-3 max-w-full max-sm:h-auto max-sm:min-h-10 max-sm:whitespace-normal max-sm:py-2"
          @click="plan"
        >
          Prepare admitted notes and tasks plan
        </button>
        <p class="mt-3 text-small text-text-muted">
          One active admitted activity import is available for each admission cohort. Cancellation keeps settled work. Preparation uses retained data and makes no FUB calls.
        </p>
      </template>
    </Card>
    <template v-if="current">
      <Card class="min-w-0">
        <h2 class="text-section font-medium">
          Activity import · {{ snapshotLabel(current.state) }}
        </h2>
        <p
          role="status"
          class="mt-1 text-small text-text-muted"
        >
          {{ snapshotLabel(current.phase) }} · Plan {{ current.latest_plan.revision }}: {{ snapshotLabel(current.latest_plan.state) }} / {{ snapshotLabel(current.latest_plan.phase) }}
        </p>
        <p
          v-if="current.pause_reason"
          role="alert"
          class="mt-2 text-small text-danger"
        >
          {{ snapshotLabel(current.pause_reason) }}. Resolve the condition, refresh and explicitly resume.
        </p>
        <p
          v-if="current.state === 'cancelled'"
          class="mt-2 text-small"
        >
          This activity import is permanently cancelled. Settled notes and tasks remain visible.
        </p>
        <p
          v-if="current.state === 'completed'"
          class="mt-2 text-small"
        >
          Activity processing finished. Review held records and retained source-only data before any future activation.
        </p>
        <dl class="mt-3 grid min-w-0 gap-3 text-small sm:grid-cols-2">
          <div>
            <dt class="text-text-muted">
              Source account
            </dt><dd class="break-all">
              {{ current.source_account_id }} · capture {{ current.capture_sequence }}
            </dd>
          </div><div>
            <dt class="text-text-muted">
              Retained import storage
            </dt><dd>{{ formatBytes(current.retained_bytes) }} · {{ formatBytes(current.reserved_bytes) }} reserved</dd>
          </div><div>
            <dt class="text-text-muted">
              Native note/task rows
            </dt><dd>{{ formatBytes(current.native_row_bytes) }}</dd>
          </div><div>
            <dt class="text-text-muted">
              Plan expiry
            </dt><dd>{{ current.latest_plan.expires_at ?? 'Preparation has not finished' }}</dd>
          </div>
        </dl>
        <div class="mt-4 grid gap-3 sm:grid-cols-2">
          <section
            v-for="family in (['notes', 'tasks'] as const)"
            :key="family"
            class="rounded-lg border border-border p-3"
          >
            <h3 class="text-body font-medium">
              {{ snapshotLabel(family) }}
            </h3><dl class="mt-2 grid grid-cols-2 gap-x-4 gap-y-1 text-small">
              <template
                v-for="name in (['planned', 'eligible', 'applied', 'already_present', 'held', 'pending'] as const)"
                :key="name"
              >
                <dt>{{ snapshotLabel(name) }}</dt><dd class="text-right">
                  {{ current.counts[family][name] }}
                </dd>
              </template>
            </dl>
          </section>
        </div>
        <p class="mt-3 text-small">
          {{ current.counts.held_count }} held records · {{ current.counts.source_only_count }} source-only components · {{ current.counts.excluded_count ?? '0' }} excluded identities · {{ current.counts.invalid_occurrences }} invalid occurrences · {{ current.counts.unavailable_bodies }} unavailable bodies.
        </p>
        <p class="mt-1 text-small text-text-muted">
          Source-only components are counted across distinct retained observations. Identical observations in one representation collapse; list and detail may be complementary. These are not unique attachment or reply totals.
        </p>
        <p class="mt-2 text-small text-text-muted">
          Remaining data: {{ current.coverage.remaining_data.map(snapshotLabel).join(', ') }}. This workspace stays in administrator review.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            v-if="current.actions.replan"
            type="button"
            :class="buttonClasses()"
            :disabled="busy || dirty"
            @click="fresh"
          >
            {{ sourceDirty ? 'Cancel unconfirmed plan to replace source' : 'Prepare fresh activity plan' }}
          </button><button
            v-if="current.actions.retry"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="busy"
            @click="resume"
          >
            Resume activity import
          </button><button
            v-if="current.actions.cancel"
            type="button"
            :class="buttonClasses('danger')"
            :disabled="busy"
            @click="cancelReview = true"
          >
            Cancel activity import
          </button><button
            v-if="current.actions.remainder"
            type="button"
            :class="buttonClasses('primary')"
            :disabled="busy"
            @click="remainder"
          >
            Continue never-settled remainder
          </button>
        </div>
      </Card>
      <fieldset
        v-if="source.data.value"
        :disabled="busy"
      >
        <SnapshotBudgetPanel
          :key="`${access.scope.value}:${source.data.value.snapshot.id}`"
          :snapshot="source.data.value.snapshot"
          @refresh="refresh"
          @access-denied="refreshWorkspace"
        />
      </fieldset>
      <ActivityMappingPanel
        :import-id="current.id"
        :plan-id="current.latest_plan.id"
        :revision="current.latest_plan.revision"
        :disabled="!current.actions.replan || busy"
        :progress-version="resultVersion"
        :reset-version="resetVersion"
        @change="patches = $event"
      />
      <Card
        v-if="current.actions.replan"
        class="min-w-0"
      >
        <h2 class="text-section font-medium">
          Source timezone and draft choices
        </h2>
        <p class="mt-1 text-small text-text-muted">
          A confirmed IANA timezone converts date-only deadlines to the last second of that local date and checks paired due-date fields. No browser or server timezone is assumed. Clearing the zone holds those tasks; independent eligible records can proceed.
        </p>
        <FormField
          v-slot="{ id }"
          label="IANA source timezone"
          bare
          class="mt-3"
        >
          <input
            :id="id"
            v-model="zone"
            :class="INPUT_CLASSES"
            placeholder="America/Los_Angeles"
            :disabled="busy"
            autocomplete="off"
          >
        </FormField>
        <label
          v-if="zoneDirty && zone.trim()"
          class="mt-3 flex items-start gap-2 text-small"
        ><input
          v-model="zoneAck"
          type="checkbox"
          class="mt-1 h-4 w-4"
          :disabled="busy"
        >I confirm this source timezone applies to the selected date-only records.</label>
        <p class="mt-2 break-all text-small text-text-muted">
          Confirmed zone: {{ current.latest_plan.source_timezone ?? 'None' }} · Timezone database {{ current.latest_plan.tzdb_version }}.
        </p>
        <p
          v-if="dirty"
          role="status"
          class="mt-3 text-small"
        >
          {{ patches.length }} mapping changes{{ zoneDirty ? ' and a timezone change' : '' }} are drafts. Apply or discard them before confirmation.
        </p>
        <div class="mt-3 flex flex-wrap gap-2">
          <button
            type="button"
            :class="buttonClasses('primary')"
            :disabled="!canApply"
            @click="apply"
          >
            Apply activity choices
          </button><button
            type="button"
            :class="buttonClasses()"
            :disabled="busy || !dirty"
            @click="discard"
          >
            Discard activity choices
          </button>
        </div>
      </Card>
      <div
        class="flex flex-wrap gap-2"
        aria-label="Activity record view"
      >
        <button
          type="button"
          :class="buttonClasses(readMode === 'plan' ? 'primary' : 'secondary')"
          :aria-pressed="readMode === 'plan'"
          @click="readMode = 'plan'"
        >
          Planned records
        </button><button
          type="button"
          :class="buttonClasses(readMode === 'results' ? 'primary' : 'secondary')"
          :aria-pressed="readMode === 'results'"
          @click="readMode = 'results'"
        >
          Committed results
        </button>
      </div>
      <ActivityRecordPanel
        :import-id="current.id"
        :plan-id="readMode === 'results' ? (current.confirmed_plan_id ?? current.latest_plan.id) : current.latest_plan.id"
        :mode="readMode"
        :progress-version="resultVersion"
        :source-timezone="current.latest_plan.source_timezone"
      />
      <Card
        v-if="current.actions.confirm"
        class="min-w-0"
      >
        <h2 class="text-section font-medium">
          Review activity confirmation
        </h2>
        <p
          v-if="!current.release_ready"
          role="alert"
          class="mt-2 text-small text-danger"
        >
          The API and worker release readiness check is not satisfied. Refresh after the deployment is ready.
        </p>
        <div class="mt-3 space-y-3 text-small">
          <label class="flex items-start gap-2"><input
            v-model="heldAck"
            type="checkbox"
            class="mt-1 h-4 w-4"
            :disabled="busy"
          >I reviewed {{ current.latest_plan.counts.held_count }} held records and approve the eligible subset.</label><label class="flex items-start gap-2"><input
            v-model="remainingAck"
            type="checkbox"
            class="mt-1 h-4 w-4"
            :disabled="busy"
          >I reviewed {{ current.latest_plan.counts.source_only_count }} source-only components and the remaining data coverage.</label><label class="flex items-start gap-2"><input
            v-model="reviewAck"
            type="checkbox"
            class="mt-1 h-4 w-4"
            :disabled="busy"
          >This keeps the Organization in administrator review. Cancellation is final and retains settled work.</label>
        </div>
        <button
          type="button"
          :class="buttonClasses('primary')"
          class="mt-4"
          :disabled="!canConfirm"
          @click="reviewConfirm"
        >
          Review notes and tasks confirmation
        </button>
      </Card>
    </template>
    <ConfirmDialog
      :visible="!!confirmation"
      title="Confirm notes and tasks import"
      :message="confirmationText"
      confirm-label="Confirm notes and tasks import"
      :is-pending="pending"
      @update:visible="value => !value && (confirmation = null)"
      @confirm="confirm"
    />
    <ConfirmDialog
      :visible="cancelReview"
      title="Permanently cancel activity import"
      :message="current?.confirmed_plan_id ? 'Cancellation ends this activity import permanently. Already settled notes and tasks, results and retained source evidence remain. You can continue its never-settled remainder. The workspace stays in administrator review.' : 'Cancellation ends this unconfirmed activity plan. Its retained source evidence remains. You can prepare a new plan from the selected qualified source. The workspace stays in administrator review.'"
      confirm-label="Permanently cancel activity import"
      confirm-variant="danger"
      :is-pending="pending"
      @update:visible="value => !value && (cancelReview = false)"
      @confirm="cancel"
    />
  </div>
</template>
